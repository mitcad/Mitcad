// SPDX-License-Identifier: MIT
#include "mitcad/geometry/path_sweep.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepFill_Edge3DLaw.hxx>
#include <BRepFill_ShapeLaw.hxx>
#include <BRepFill_Sweep.hxx>
#include <BRepLib_MakeFace.hxx>
#include <BRep_Builder.hxx>
#include <GeomFill_LocationLaw.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_HArray2.hxx>
#include <NCollection_Map.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Mat.hxx>
#include <Message_ProgressRange.hxx>
#include <ShapeFix_Solid.hxx>
#include <BRepOffsetAPI_MakePipe.hxx>
#include <BRepBuilderAPI_GTransform.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepLib.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom2d_Line.hxx>
#include <gp.hxx>
#include <TopLoc_Location.hxx>
#include <NCollection_Array1.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GC_MakeSegment2d.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <NCollection_HArray1.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_GTrsf.hxx>
#include <gp_Trsf.hxx>

#include "history.hpp"
#include "spine.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;
constexpr double kPi = kTwoPi / 2.0;

using detail::NamedWire;
using detail::PipeMode;
using detail::Spine;

// The part of a spine a sweep covers, from the point `at` (arc length)
// `extent1` of the rest of it on and `extent2` back (of the whole length on
// a closed spine), and the names of the caps at its start and end.
struct Range {
  double from = 0.0;
  double to = 0.0;
  bool around = false;   // once round a closed spine: no caps
  bool at_start = true;  // `at` is the start of the part
};

Range swept_range(const Spine& spine, double at, double extent1, double extent2) {
  for (const double extent : {extent1, extent2}) {
    if (!(extent >= 0.0 && extent <= 1.0)) {
      throw std::invalid_argument("a fraction of the path must be between 0 and 1");
    }
  }
  const double length = spine.length();
  const double tolerance = 1e-9 * std::max(1.0, length);
  Range range;
  if (spine.closed()) {
    if (extent1 + extent2 > 1.0 + 1e-9) {
      throw std::invalid_argument("on a closed path the fractions add up to at most 1");
    }
    range.from = at - extent2 * length;
    range.to = at + extent1 * length;
    range.around = extent1 + extent2 >= 1.0 - 1e-9;
    if (range.around && (at <= tolerance || at >= length - tolerance)) {
      range.from = 0.0;
      range.to = length;
    }
    while (range.to > length + tolerance) {
      range.from -= length;
      range.to -= length;
    }
  } else {
    range.from = at - extent2 * at;
    range.to = at + extent1 * (length - at);
  }
  if (range.to - range.from <= tolerance) {
    throw std::invalid_argument("the sweep covers none of the path");
  }
  range.at_start = std::abs(at - range.from) <= tolerance;
  return range;
}

// The names of the caps at the start and the end of the swept part: start
// at the profile when it starts the part, else (two sides, or the profile
// at the end) start at the end of side one.
std::pair<NameList, NameList> cap_order(const Range& range, const NameList& start, const NameList& end) {
  if (range.around) {
    return {{}, {}};
  }
  return range.at_start ? std::pair{start, end} : std::pair{end, start};
}

Bnd_Box box_of(const std::vector<detail::SweepFace>& faces) {
  Bnd_Box box;
  for (const detail::SweepFace& face : faces) {
    box.Add(detail::bounds_of(face.face));
  }
  return box;
}

gp_Pnt center_of(const Bnd_Box& box) {
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  return gp_Pnt((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0);
}

// The farthest point of the profiles' outer loops from `center`.
double reach(const std::vector<detail::SweepFace>& faces, const gp_Pnt& center) {
  double farthest = 0.0;
  for (const detail::SweepFace& face : faces) {
    for (TopExp_Explorer e(face.face, TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      for (int i = 0; i <= 32; ++i) {
        const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 32.0;
        farthest = std::max(farthest, curve.Value(t).Distance(center));
      }
    }
  }
  return farthest;
}

// A copy of the loop moved by the transformation, with its edges' names.
NamedWire moved(const NamedWire& loop, const gp_GTrsf& transformation, bool affine) {
  NamedWire result;
  if (!affine) {
    BRepBuilderAPI_Transform transform(loop.wire, transformation.Trsf(), true);
    result.wire = TopoDS::Wire(transform.Shape());
    for (const auto& [edge, name] : loop.edges) {
      result.edges.emplace_back(TopoDS::Edge(transform.ModifiedShape(edge)), name);
    }
    return result;
  }
  BRepBuilderAPI_GTransform transform(loop.wire, transformation, true);
  result.wire = TopoDS::Wire(transform.Shape());
  for (const auto& [edge, name] : loop.edges) {
    result.edges.emplace_back(TopoDS::Edge(transform.ModifiedShape(edge)), name);
  }
  return result;
}

// A sweep that turns or sizes its profile along the path: copies of the
// profile placed along the part, lofted. The path must be smooth.
struct Variation {
  double twist = 0.0;
  double taper = 0.0;
  std::optional<Spine> rail;
  ProfileScaling scaling = ProfileScaling::Scale;
  const Spine* path = nullptr; // the whole path, for the rail's fractions
  double from = 0.0;           // where the part starts on it
};

ShapePtr varied_sweep(const SweepSpec& spec, const std::vector<detail::SweepFace>& faces,
                      const TopoDS_Wire& part, double at, const Variation& variation,
                      const Range& range) {
  const Spine spine(part);
  const double length = spine.length();
  const double s0 = std::clamp(at - range.from, 0.0, length);
  int segments = 8 * spine.edge_count();
  segments = std::max(segments, static_cast<int>(std::ceil(std::abs(variation.twist) / (kPi / 36.0))));
  if (variation.rail) {
    segments = std::max(segments, 8 * variation.rail->edge_count());
  }
  segments = std::clamp(segments, 8, 120);
  std::vector<double> stations;
  for (int i = 0; i <= segments; ++i) {
    stations.push_back(length * i / segments);
  }
  stations.push_back(s0);
  std::sort(stations.begin(), stations.end());
  stations.erase(std::unique(stations.begin(), stations.end(),
                             [length](double a, double b) { return b - a <= 1e-9 * std::max(1.0, length); }),
                 stations.end());
  std::size_t k0 = 0;
  for (std::size_t i = 0; i < stations.size(); ++i) {
    if (std::abs(stations[i] - s0) < std::abs(stations[k0] - s0)) {
      k0 = i;
    }
  }
  stations[k0] = s0;
  const gp_Dir t0 = spine.tangent(s0);
  const std::optional<gp_Dir> plane = spine.plane_normal();
  const gp_Dir n0 = plane ? plane->Crossed(t0) : detail::any_normal(t0);
  const std::vector<gp_Ax3> frames = detail::follow_frames(spine, s0, n0, stations);

  // The rail's direction (at right angles to the path) and distance at each
  // station.
  std::vector<gp_Vec> towards(stations.size());
  if (variation.rail) {
    const double path_length = variation.path->length();
    for (std::size_t i = 0; i < stations.size(); ++i) {
      double s = variation.from + stations[i];
      if (variation.path->closed()) {
        s = std::fmod(std::fmod(s, path_length) + path_length, path_length);
      }
      const double fraction = std::clamp(s / path_length, 0.0, 1.0);
      const gp_Pnt q = variation.rail->point(fraction * variation.rail->length());
      const gp_Dir t = frames[i].Direction();
      gp_Vec v(frames[i].Location(), q);
      v -= gp_Vec(t) * v.Dot(gp_Vec(t));
      if (v.Magnitude() <= Precision::Confusion()) {
        throw std::invalid_argument("the guide rail meets the path; it must run beside it");
      }
      towards[i] = v;
    }
  }
  const auto angle_of = [&](std::size_t i) {
    const gp_Vec n(frames[i].XDirection());
    const gp_Vec t(frames[i].Direction());
    return std::atan2(n.Crossed(towards[i]).Dot(t), n.Dot(towards[i]));
  };
  double size = 1.0;
  if (variation.taper != 0.0) {
    size = reach(faces, spine.point(s0));
    if (size <= Precision::Confusion()) {
      throw std::invalid_argument("the profile has no size to taper");
    }
  }
  std::vector<gp_GTrsf> placements;
  bool affine = false;
  for (std::size_t i = 0; i < stations.size(); ++i) {
    const gp_Pnt p = frames[i].Location();
    const gp_Dir t = frames[i].Direction();
    gp_Trsf move;
    move.SetDisplacement(frames[k0], frames[i]);
    double angle = variation.twist * (stations[i] - s0) / length;
    double scale = 1.0;
    if (variation.rail) {
      double turn = angle_of(i) - angle_of(k0);
      turn = std::remainder(turn, kTwoPi);
      angle = turn;
      if (variation.scaling != ProfileScaling::None) {
        scale = towards[i].Magnitude() / towards[k0].Magnitude();
      }
    } else if (variation.taper != 0.0) {
      scale = 1.0 + std::abs(stations[i] - s0) * std::tan(variation.taper) / size;
      if (scale <= 1e-6) {
        throw std::invalid_argument("the taper closes the profile before the end of the sweep");
      }
    }
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(p, t), angle);
    gp_GTrsf placement(turn.Multiplied(move));
    if (std::abs(scale - 1.0) > 1e-12) {
      if (variation.rail && variation.scaling == ProfileScaling::Stretch) {
        gp_GTrsf stretch;
        stretch.SetAffinity(gp_Ax2(p, gp_Dir(towards[i])), scale);
        placement = stretch.Multiplied(placement);
        affine = true;
      } else {
        gp_Trsf grow;
        grow.SetScale(p, scale);
        placement = gp_GTrsf(grow.Multiplied(turn.Multiplied(move)));
      }
    }
    placements.push_back(placement);
  }
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const auto caps = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                detail::cap_names(spec.feature, "end", face));
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    std::vector<ShapePtr> solids;
    for (std::size_t l = 0; l < loops.size(); ++l) {
      std::vector<TopoDS_Shape> sections;
      for (std::size_t i = 0; i < stations.size(); ++i) {
        sections.push_back(i == k0 ? TopoDS_Shape(loops[l].wire)
                                   : TopoDS_Shape(moved(loops[l], placements[i], affine).wire));
      }
      solids.push_back(detail::through_sections(sections, loops[l], l == 0 ? caps.first : NameList{},
                                                l == 0 ? caps.second : NameList{}, false, false));
    }
    parts.push_back(detail::with_holes(solids.front(), {solids.begin() + 1, solids.end()}, "the sweep"));
  }
  return detail::fused(parts, "the sweep");
}

// The arc lengths inside the spine where its tangent turns through the
// plane at right angles to `normal`: where the spine's distance along
// `normal` stops growing or shrinking (a turn back, or an inflection).
std::vector<double> turns(const Spine& spine, const gp_Dir& normal) {
  const double length = spine.length();
  const double snap = std::max(1e-9 * std::max(1.0, length), Precision::Confusion());
  const auto along = [&](double s) { return gp_Vec(spine.tangent(s)).Dot(gp_Vec(normal)); };
  const double flat = 1e-9;
  const int count = 64 * spine.edge_count();
  std::vector<double> found;
  double last = 0.0;  // the last sample off the plane
  double value = 0.0; // and its value
  for (int i = 0; i <= count; ++i) {
    const double s = length * i / count;
    const double v = along(s);
    if (std::abs(v) <= flat) {
      continue;
    }
    if (value != 0.0 && (v < 0.0) != (value < 0.0)) {
      double a = last;
      double b = s;
      for (int k = 0; k < 100 && b - a > 1e-13 * std::max(1.0, length); ++k) {
        const double m = (a + b) / 2.0;
        const double vm = along(m);
        if (std::abs(vm) > flat && (vm < 0.0) == (value < 0.0)) {
          a = m;
        } else {
          b = m;
        }
      }
      const double turn = (a + b) / 2.0;
      if (turn > snap && turn < length - snap && (found.empty() || turn - found.back() > snap)) {
        found.push_back(turn);
      }
    }
    last = s;
    value = v;
  }
  return found;
}

// The parallel orientation: the profile moved along the part without
// turning. Where the part turns back through the profile's plane its copies
// would pass through the sweep's own sides, so the part is swept in pieces
// between those places, each moving one way along the profile's normal,
// and the pieces are fused. A piece ending where the part turns back keeps
// its cap there, named turn(<region>).
ShapePtr parallel_sweep(const SweepSpec& spec, const std::vector<detail::SweepFace>& faces,
                        const TopoDS_Wire& part, const PipeMode& mode, double at, const Range& range) {
  const Spine spine(part);
  const double length = spine.length();
  const double s0 = std::clamp(at - range.from, 0.0, length);
  const gp_Dir normal = spec.frame.normal();
  std::vector<double> ends{0.0};
  for (const double turn : turns(spine, normal)) {
    ends.push_back(turn);
  }
  ends.push_back(length);
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const auto [first, last] = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                         detail::cap_names(spec.feature, "end", face));
    const NameList turn = detail::cap_names(spec.feature, "turn", face);
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    for (std::size_t k = 0; k + 1 < ends.size(); ++k) {
      const double a = ends[k];
      const double b = ends[k + 1];
      const bool whole = ends.size() == 2;
      // A piece that keeps within the profile's plane adds nothing.
      if (!whole && std::abs(gp_Vec(spine.point(a), spine.point(b)).Dot(gp_Vec(normal))) <=
                        Precision::Confusion()) {
        continue;
      }
      const TopoDS_Wire piece = whole ? part : spine.part(a, b);
      // The profile moved to the piece: where it is, else the piece's
      // middle, which only one place of the piece lies level with.
      const double place = s0 >= a && s0 <= b ? s0 : (a + b) / 2.0;
      gp_Trsf move;
      move.SetTranslation(spine.point(s0), spine.point(place));
      const gp_GTrsf transformation(move);
      const auto loop = [&](std::size_t l) {
        return place == s0 ? loops[l] : moved(loops[l], transformation, false);
      };
      const ShapePtr outer = detail::pipe_shell(loop(0), piece, mode, k == 0 ? first : turn,
                                                k + 2 == ends.size() ? last : turn);
      std::vector<ShapePtr> holes;
      for (std::size_t l = 1; l < loops.size(); ++l) {
        holes.push_back(detail::pipe_shell(loop(l), piece, mode, {}, {}));
      }
      parts.push_back(detail::with_holes(outer, holes, "the sweep"));
    }
  }
  return detail::fused(parts, "the sweep");
}

} // namespace

ShapePtr sweep(const SweepSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to sweep");
  }
  detail::require_finite("twist angle", spec.twist);
  detail::require_finite("taper angle", spec.taper);
  if (std::abs(spec.taper) >= kPi / 2.0) {
    throw std::invalid_argument("the taper angle must be less than 90 degrees");
  }
  return detail::run("sweep", [&] {
    const std::vector<detail::SweepFace> faces =
        detail::merged(detail::region_faces(spec.feature, spec.frame, spec.regions));
    const Spine path(detail::path_wire(spec.path));
    const gp_Pln plane(spec.frame.origin, spec.frame.normal());
    double at = path.locate(plane, center_of(box_of(faces)));
    // A profile at an end of the path within the modelling tolerance is
    // at that end.
    if (path.point(at).Distance(path.point(0.0)) <= Precision::Confusion() &&
        at <= path.length() / 2.0) {
      at = 0.0;
    } else if (path.point(at).Distance(path.point(path.length())) <= Precision::Confusion() &&
               at >= path.length() / 2.0) {
      at = path.length();
    }
    const Range range = swept_range(path, at, spec.extent1, spec.extent2);
    const TopoDS_Wire part = path.part(range.from, range.to);
    const bool guided = !spec.rail.empty();
    const bool varied = guided || spec.twist != 0.0 || spec.taper != 0.0;
    if (varied && spec.orientation == SweepOrientation::Parallel) {
      throw std::invalid_argument("unsupported: twists, tapers and guide rails with parallel orientation");
    }
    if (varied && range.around) {
      throw std::invalid_argument("unsupported: twists, tapers and guide rails round a whole closed path");
    }
    ShapePtr result;
    if (varied) {
      Variation variation;
      if (guided) {
        variation.rail.emplace(detail::path_wire(spec.rail));
        variation.scaling = spec.scaling;
      } else {
        variation.twist = spec.twist;
        variation.taper = spec.taper;
      }
      variation.path = &path;
      variation.from = range.from;
      result = varied_sweep(spec, faces, part, at, variation, range);
    } else if (spec.orientation == SweepOrientation::Parallel) {
      PipeMode mode;
      mode.kind = PipeMode::Kind::Fixed;
      mode.fixed = gp_Ax2(path.point(at), path.tangent(at));
      result = parallel_sweep(spec, faces, part, mode, at, range);
    } else {
      const PipeMode mode = detail::perpendicular_mode(path);
      std::vector<ShapePtr> parts;
      for (const detail::SweepFace& face : faces) {
        const auto [first, last] = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                             detail::cap_names(spec.feature, "end", face));
        const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
        const ShapePtr outer = detail::pipe_shell(loops.front(), part, mode, first, last);
        std::vector<ShapePtr> holes;
        for (std::size_t l = 1; l < loops.size(); ++l) {
          holes.push_back(detail::pipe_shell(loops[l], part, mode, {}, {}));
        }
        parts.push_back(detail::with_holes(outer, holes, "the sweep"));
      }
      result = detail::fused(parts, "the sweep");
    }
    return detail::finished(*result, "the sweep");
  });
}

namespace {

// A regular section about `center` in the plane at right angles to
// `normal`: a circle of radius `radius`, or a polygon with its corners on
// that circle, the first at `angle` from `x` (counter-clockwise about the
// normal). Edges are named <feature>:<role><i> in order.
NamedWire section(const std::string& feature, const std::string& role, const gp_Pnt& center,
                  const gp_Dir& normal, const gp_Dir& x, double radius, int corners, double angle) {
  NamedWire wire;
  if (corners == 0) {
    const TopoDS_Edge edge =
        BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(center, normal, x), radius)).Edge();
    wire.wire = BRepBuilderAPI_MakeWire(edge).Wire();
    wire.edges.emplace_back(edge, face_name(feature, role + "0"));
    return wire;
  }
  const gp_Dir y = normal.Crossed(x);
  std::vector<gp_Pnt> points;
  for (int i = 0; i < corners; ++i) {
    const double a = angle + kTwoPi * i / corners;
    points.push_back(center.Translated(gp_Vec(x) * (radius * std::cos(a)) + gp_Vec(y) * (radius * std::sin(a))));
  }
  BRepBuilderAPI_MakeWire maker;
  for (int i = 0; i < corners; ++i) {
    const TopoDS_Edge edge =
        BRepBuilderAPI_MakeEdge(points[static_cast<std::size_t>(i)],
                                points[static_cast<std::size_t>((i + 1) % corners)])
            .Edge();
    maker.Add(edge);
    wire.edges.emplace_back(maker.Edge(), face_name(feature, role + std::to_string(i)));
  }
  wire.wire = maker.Wire();
  return wire;
}

} // namespace

ShapePtr pipe(const PipeSpec& spec) {
  detail::require_positive("pipe section size", spec.size);
  detail::require_finite("pipe wall thickness", spec.thickness);
  const double radius = spec.size / 2.0;
  // The circle inside the section, and what the wall takes off the
  // circle round it.
  double inside = radius;
  double shrink = spec.thickness;
  int corners = 0;
  double angle = 0.0;
  switch (spec.section) {
  case PipeSection::Circular:
    break;
  case PipeSection::Square:
    inside = radius / std::sqrt(2.0);
    shrink = spec.thickness * std::sqrt(2.0);
    corners = 4;
    angle = kPi / 4.0;
    break;
  case PipeSection::Triangular:
    inside = radius / 2.0;
    shrink = 2.0 * spec.thickness;
    corners = 3;
    break;
  }
  if (spec.thickness < 0.0 || spec.thickness >= inside) {
    throw std::invalid_argument("the pipe's wall must be thinner than " + std::to_string(inside));
  }
  return detail::run("pipe", [&] {
    const Spine path(detail::path_wire(spec.path));
    const gp_Pnt start = path.point(0.0);
    const gp_Dir tangent = path.tangent(0.0);
    const std::optional<gp_Dir> plane = path.plane_normal();
    const gp_Dir x = plane ? plane->Crossed(tangent) : detail::any_normal(tangent);
    const NamedWire outer = section(spec.feature, "side", start, tangent, x, radius, corners, angle);
    const double length = path.length();
    Range range;
    if (path.closed()) {
      range = swept_range(path, 0.0, spec.extent1, spec.extent2);
    } else {
      if (spec.extent2 != 0.0) {
        throw std::invalid_argument("only a pipe along a closed path runs backwards");
      }
      range = swept_range(path, 0.0, spec.extent1, 0.0);
    }
    (void)length;
    const TopoDS_Wire part = path.part(range.from, range.to);
    const auto [first, last] =
        cap_order(range, {face_name(spec.feature, "start")}, {face_name(spec.feature, "end")});
    const PipeMode mode = detail::perpendicular_mode(path);
    ShapePtr result = detail::pipe_shell(outer, part, mode, first, last);
    if (spec.thickness > 0.0) {
      const NamedWire inner =
          section(spec.feature, "inner", start, tangent, x, radius - shrink, corners, angle);
      result = detail::with_holes(result, {detail::pipe_shell(inner, part, mode, {}, {})}, "the pipe");
    }
    return detail::finished(*result, "the pipe");
  });
}

ShapePtr coil(const CoilSpec& spec) {
  detail::require_positive("coil diameter", spec.diameter);
  detail::require_positive("coil revolutions", spec.revolutions);
  detail::require_positive("coil pitch", spec.pitch);
  detail::require_positive("coil section size", spec.size);
  detail::require_finite("coil angle", spec.angle);
  if (std::abs(spec.angle) >= kPi / 2.0) {
    throw std::invalid_argument("the coil's angle must be less than 90 degrees");
  }
  if (spec.spiral && spec.angle != 0.0) {
    throw std::invalid_argument("a spiral coil has no angle");
  }
  // The section's corners on a circle of radius r, and how far it reaches
  // out of the coil (`out`), into it (`in`) and along it (`along`).
  const double r = spec.size / 2.0;
  int corners = 0;
  double angle = 0.0;
  double out = r;
  double in = r;
  double along = r;
  switch (spec.section) {
  case CoilSection::Circular:
    break;
  case CoilSection::Square:
    corners = 4;
    angle = kPi / 4.0;
    out = in = along = r / std::sqrt(2.0);
    break;
  case CoilSection::TriangularExternal:
    corners = 3;
    in = r / 2.0;
    along = r * std::sqrt(3.0) / 2.0;
    break;
  case CoilSection::TriangularInternal:
    corners = 3;
    angle = kPi;
    out = r / 2.0;
    along = r * std::sqrt(3.0) / 2.0;
    break;
  }
  double radius = spec.diameter / 2.0;
  if (spec.position == CoilPosition::Inside) {
    radius -= out;
  } else if (spec.position == CoilPosition::Outside) {
    radius += in;
  }
  if (radius - in <= Precision::Confusion()) {
    throw std::invalid_argument("the coil's section reaches its axis; make the diameter larger");
  }
  const double height = spec.revolutions * spec.pitch;
  if (spec.spiral) {
    if (spec.pitch <= out + in) {
      throw std::invalid_argument("the spiral's pitch is smaller than its section, so its turns overlap");
    }
  } else {
    // The section is tilted by the helix angle; neighbouring turns must not
    // meet.
    const double slope = spec.pitch / (kTwoPi * radius);
    if (spec.pitch <= 2.0 * along * std::sqrt(1.0 + slope * slope)) {
      throw std::invalid_argument("the coil's pitch is smaller than its section, so its turns overlap");
    }
    if (radius + height * std::tan(spec.angle) - in <= Precision::Confusion()) {
      throw std::invalid_argument("the coil's angle narrows it into its axis");
    }
  }
  return detail::run("coil", [&] {
    const gp_Pnt origin = spec.frame.origin;
    const gp_Dir axis = spec.frame.normal();
    gp_Ax3 axes(origin, axis, spec.frame.x_axis);
    if (spec.clockwise) {
      axes.YReverse();
    }
    TopoDS_Edge edge;
    const int samples = std::max(32, static_cast<int>(std::ceil(spec.revolutions * 32.0)));
    if (spec.spiral) {
      // An Archimedean spiral, interpolated.
      const int count = std::max(64, static_cast<int>(std::ceil(spec.revolutions * 64.0)));
      const occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, count + 1);
      const gp_Vec x(axes.XDirection());
      const gp_Vec y(axes.YDirection());
      for (int i = 0; i <= count; ++i) {
        const double t = kTwoPi * spec.revolutions * i / count;
        const double rho = radius + spec.pitch * t / kTwoPi;
        points->SetValue(i + 1, origin.Translated(x * (rho * std::cos(t)) + y * (rho * std::sin(t))));
      }
      GeomAPI_Interpolate interpolate(points, false, 1.0e-9);
      interpolate.Perform();
      if (!interpolate.IsDone()) {
        throw std::runtime_error("the spiral could not be built");
      }
      edge = BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge();
    } else {
      // On the unrolled cylinder or cone the helix is a line: a turn round
      // the axis rises one pitch.
      occ::handle<Geom_Surface> surface;
      const double c = std::cos(spec.angle);
      if (spec.angle == 0.0) {
        surface = new Geom_CylindricalSurface(axes, radius);
      } else {
        surface = new Geom_ConicalSurface(axes, spec.angle, radius);
      }
      const gp_Vec2d direction(kTwoPi * c, spec.pitch);
      const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(direction));
      BRepBuilderAPI_MakeEdge helix(line, surface, 0.0, spec.revolutions * direction.Magnitude() / c);
      if (!helix.IsDone()) {
        throw std::runtime_error("the coil's helix could not be built");
      }
      edge = helix.Edge();
      BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, std::max(30, samples));
    }
    const TopoDS_Wire spine = BRepBuilderAPI_MakeWire(edge).Wire();
    const BRepAdaptor_Curve curve(edge);
    gp_Pnt start;
    gp_Vec velocity;
    curve.D1(curve.FirstParameter(), start, velocity);
    const gp_Dir tangent(velocity);
    // Outwards from the axis, at right angles to the helix.
    gp_Vec radial(origin, start);
    radial -= gp_Vec(axis) * radial.Dot(gp_Vec(axis));
    radial -= gp_Vec(tangent) * radial.Dot(gp_Vec(tangent));
    const NamedWire profile =
        section(spec.feature, "side", start, tangent, gp_Dir(radial), r, corners, angle);
    PipeMode mode;
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = axis;
    const ShapePtr result = detail::pipe_shell(profile, spine, mode, {face_name(spec.feature, "start")},
                                               {face_name(spec.feature, "end")}, 1.0e-6);
    return detail::finished(*result, "the coil");
  });
}

namespace {

// The path of a widening helix of FreeCAD's construction
// (HelixSweepSpec::growth with `freecad`) as FreeCAD makes it: a spiral on
// a cone's surface of revolution, in one piece, its axis the reference
// axis and its start towards the profiles' centre `center` (seen across
// the profile plane's normal), a hundred times as far, so that following
// its Frenet frame moves the profiles nearly as a screw motion widening by
// the growth.
TopoDS_Wire widening_path(const HelixSweepSpec& spec, const gp_Pnt& center) {
  const double period = 1000.0;
  const gp_Dir travel = spec.axis.Direction();
  const gp_Dir axis = spec.flip ? travel.Reversed() : travel;
  const gp_Pnt base = spec.axis.Location();
  const gp_Dir normal = spec.frame.normal();
  gp_Vec start = gp_Vec(axis).Crossed(gp_Vec(normal));
  if (start.Magnitude() < Precision::Confusion()) {
    start = gp_Vec(normal).Crossed(gp_Vec(1.0, 2.0, 3.0));
    if (start.Magnitude() < Precision::Confusion()) {
      start = gp_Vec(normal).Crossed(gp_Vec(3.0, 2.0, 1.0));
    }
  }
  const gp_Dir start_dir(start);
  const gp_XYZ a = axis.XYZ();
  const gp_XYZ s = start_dir.XYZ();
  const double offset = 100.0 * (center.XYZ().Dot(s) - base.XYZ().Dot(s));
  double radius = std::abs(offset);
  const bool turned = offset < 0.0;
  const double height = spec.pitch * spec.revolutions;
  const double shift =
      10000.0 * std::abs((spec.growth <= 0.0 ? 1.0 : 0.0) * center.XYZ().Dot(a) - base.XYZ().Dot(a));
  if (radius < Precision::Confusion()) {
    radius = 1000.0;
  }
  const double top = radius + spec.revolutions * spec.growth;
  // The spiral on the surface of revolution of the line from (radius, 0, 0)
  // to (top, 0, height): one segment of the parameter plane.
  NCollection_Array1<gp_Pnt> poles(1, 2);
  poles(1) = gp_Pnt(radius, 0.0, 0.0);
  poles(2) = gp_Pnt(top, 0.0, height);
  const occ::handle<Geom_BezierCurve> meridian = new Geom_BezierCurve(poles);
  const occ::handle<Geom_Surface> surface =
      new Geom_SurfaceOfRevolution(meridian, gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp::DZ()));
  const double periods = spec.revolutions / period;
  const double full = std::floor(periods);
  const double part = periods - full;
  gp_Vec2d direction((spec.left_handed ? -1.0 : 1.0) * period * 2.0 * kPi, 1.0 / periods);
  gp_Pnt2d begin(0.0, 0.0);
  BRepBuilderAPI_MakeWire wire;
  for (int i = 0; i < static_cast<int>(full); ++i) {
    const gp_Pnt2d end = begin.Translated(direction);
    const occ::handle<Geom2d_TrimmedCurve> segment = GC_MakeSegment2d(begin, end);
    wire.Add(BRepBuilderAPI_MakeEdge(segment, surface).Edge());
    begin = end;
  }
  if (part > Precision::Confusion()) {
    direction.Scale(part);
    const occ::handle<Geom2d_TrimmedCurve> segment = GC_MakeSegment2d(begin, begin.Translated(direction));
    wire.Add(BRepBuilderAPI_MakeEdge(segment, surface).Edge());
  }
  TopoDS_Wire path = wire.Wire();
  BRepLib::BuildCurves3d(path, Precision::Confusion() * 1.0e-6 * (radius + top), GeomAbs_C1, 14, 10000);
  // Into place: raised by the shift, turned over when reversed, turned
  // half round when the profiles are on the other side, then onto the axis.
  const gp_Pnt origin(0.0, 0.0, 0.0);
  gp_Trsf move;
  if (shift > 0.0) {
    move.SetTranslation(gp_Vec(0.0, 0.0, shift));
    path.Move(TopLoc_Location(move));
  }
  if (spec.flip) {
    move.SetRotation(gp_Ax1(origin, gp::DX()), kPi);
    path.Move(TopLoc_Location(move));
  }
  if (turned) {
    move.SetRotation(gp_Ax1(origin, gp::DZ()), kPi);
    path.Move(TopLoc_Location(move));
  }
  move.SetTransformation(gp_Ax3(origin, gp::DZ(), gp::DX()), gp_Ax3(base, axis, start_dir));
  path.Move(TopLoc_Location(move).Inverted());
  return path;
}

// A region's face (holes and all) swept along the path by its Frenet
// frame: sides named after the face's edges, caps `first` and `last`.
ShapePtr frenet_pipe(const detail::SweepFace& face, const TopoDS_Wire& path, const NameList& first,
                     const NameList& last) {
  BRepOffsetAPI_MakePipe pipe(path, face.face, GeomFill_IsFrenet, false);
  detail::build(pipe);
  if (!pipe.IsDone()) {
    throw std::runtime_error("the profile could not be swept along the helix");
  }
  // As FreeCAD finishes its helices: the solid fixed (which may reorient
  // its shell and rebuild it); the faces stay.
  TopoDS_Shape result = detail::oriented_solid(pipe.Shape(), "the helix");
  ShapeFix_Solid fixer;
  fixer.Init(TopoDS::Solid(result));
  if (fixer.Perform()) {
    result = fixer.Solid();
  }
  detail::FaceNamer namer(result);
  for (const auto& [edge, name] : face.edges) {
    namer.generated(pipe, edge, name);
  }
  detail::name_caps(namer, pipe.FirstShape(), first);
  detail::name_caps(namer, pipe.LastShape(), last);
  return namer.shape();
}

// The motion of Mitcad's widening helix (HelixSweepSpec::growth, mitcad#83):
// at the fraction u of the way a point X goes to
// base + R(u angle) (X - base + u growth out) + u rise axis, R the turn
// about the axis: a screw motion whose profiles also move out along `out`,
// the direction from the axis towards them at the start.
struct GrowingScrew {
  gp_Pnt base;
  gp_Dir axis;
  gp_Dir out;
  double angle = 0.0;  // the whole turn, signed about `axis`
  double rise = 0.0;   // along the axis
  double growth = 0.0; // along `out`
};

// The cross product with `a` as a matrix (K x = a x x).
gp_Mat cross_matrix(const gp_Dir& a) {
  return gp_Mat(0.0, -a.Z(), a.Y(), a.Z(), 0.0, -a.X(), -a.Y(), a.X(), 0.0);
}

// The motion as a location law of a sweep along any curve: the curve's
// parameter is spread evenly over the motion (only its domain counts).
class GrowingScrewLaw : public GeomFill_LocationLaw {
public:
  explicit GrowingScrewLaw(const GrowingScrew& screw) : m_screw(screw) {
    m_k = cross_matrix(screw.axis);
    m_k2 = m_k * m_k;
    m_trans.SetIdentity();
  }

  bool SetCurve(const occ::handle<Adaptor3d_Curve>& curve) override {
    m_curve = curve;
    m_first = curve->FirstParameter();
    m_last = curve->LastParameter();
    m_from = m_first;
    m_to = m_last;
    return m_last > m_first;
  }
  const occ::handle<Adaptor3d_Curve>& GetCurve() const override { return m_curve; }
  void SetTrsf(const gp_Mat& transformation) override { m_trans = transformation; }
  occ::handle<GeomFill_LocationLaw> Copy() const override {
    occ::handle<GrowingScrewLaw> copy = new GrowingScrewLaw(m_screw);
    if (!m_curve.IsNull()) {
      copy->SetCurve(m_curve);
    }
    copy->SetTrsf(m_trans);
    return copy;
  }

  bool D0(const double t, gp_Mat& m, gp_Vec& v) override {
    gp_Mat dm, d2m;
    gp_Vec dv, d2v;
    evaluate(t, 0, m, v, dm, dv, d2m, d2v);
    return true;
  }
  bool D0(const double t, gp_Mat& m, gp_Vec& v, NCollection_Array1<gp_Pnt2d>&) override { return D0(t, m, v); }
  bool D1(const double t, gp_Mat& m, gp_Vec& v, gp_Mat& dm, gp_Vec& dv, NCollection_Array1<gp_Pnt2d>&,
          NCollection_Array1<gp_Vec2d>&) override {
    gp_Mat d2m;
    gp_Vec d2v;
    evaluate(t, 1, m, v, dm, dv, d2m, d2v);
    return true;
  }
  bool D2(const double t, gp_Mat& m, gp_Vec& v, gp_Mat& dm, gp_Vec& dv, gp_Mat& d2m, gp_Vec& d2v,
          NCollection_Array1<gp_Pnt2d>&, NCollection_Array1<gp_Vec2d>&, NCollection_Array1<gp_Vec2d>&) override {
    evaluate(t, 2, m, v, dm, dv, d2m, d2v);
    return true;
  }

  int NbIntervals(const GeomAbs_Shape) const override { return 1; }
  void Intervals(NCollection_Array1<double>& t, const GeomAbs_Shape) const override {
    t(t.Lower()) = m_first;
    t(t.Lower() + 1) = m_last;
  }
  void SetInterval(const double first, const double last) override {
    m_from = first;
    m_to = last;
  }
  void GetInterval(double& first, double& last) const override {
    first = m_from;
    last = m_to;
  }
  void GetDomain(double& first, double& last) const override {
    first = m_first;
    last = m_last;
  }
  double GetMaximalNorm() override { return 1.0; }
  void GetAverageLaw(gp_Mat& m, gp_Vec& v) override {
    m = gp_Mat(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    v = gp_Vec(0.0, 0.0, 0.0);
    for (int i = 0; i <= 10; ++i) {
      gp_Mat mi;
      gp_Vec vi;
      D0(m_from + (m_to - m_from) * i / 10.0, mi, vi);
      m += mi;
      v += vi;
    }
    m.Divide(11.0);
    v /= 11.0;
  }

private:
  // M, V and their derivatives by the curve's parameter up to `order`.
  void evaluate(double t, int order, gp_Mat& m, gp_Vec& v, gp_Mat& dm, gp_Vec& dv, gp_Mat& d2m,
                gp_Vec& d2v) const {
    const double span = m_last - m_first;
    const double u = (t - m_first) / span;
    const double phi = u * m_screw.angle;
    const double c = std::cos(phi);
    const double s = std::sin(phi);
    gp_Mat identity;
    identity.SetIdentity();
    // R = I + sin K + (1 - cos) K^2, R' = cos K + sin K^2, R'' = -sin K + cos K^2.
    const gp_Mat r = identity + m_k.Multiplied(s) + m_k2.Multiplied(1.0 - c);
    // V = base + R w + u rise axis, w = u growth out - base.
    const gp_XYZ out = m_screw.out.XYZ() * m_screw.growth;
    const gp_XYZ w = out * u - m_screw.base.XYZ();
    const gp_XYZ up = m_screw.axis.XYZ() * m_screw.rise;
    m = r * m_trans;
    v = gp_Vec(m_screw.base.XYZ() + w.Multiplied(r) + up * u);
    if (order < 1) {
      return;
    }
    const double dphi = m_screw.angle / span; // dphi / dt
    const gp_Mat r1 = m_k.Multiplied(c) + m_k2.Multiplied(s);
    dm = r1.Multiplied(dphi) * m_trans;
    dv = gp_Vec(w.Multiplied(r1) * dphi + out.Multiplied(r) / span + up / span);
    if (order < 2) {
      return;
    }
    const gp_Mat r2 = m_k.Multiplied(-s) + m_k2.Multiplied(c);
    d2m = r2.Multiplied(dphi * dphi) * m_trans;
    d2v = gp_Vec(w.Multiplied(r2) * (dphi * dphi) + out.Multiplied(r1) * (2.0 * dphi / span));
  }

  GrowingScrew m_screw;
  gp_Mat m_k;
  gp_Mat m_k2;
  gp_Mat m_trans;
  occ::handle<Adaptor3d_Curve> m_curve;
  double m_first = 0.0;
  double m_last = 1.0;
  double m_from = 0.0;
  double m_to = 1.0;
};

// Whether the first edge of the face runs the same way in it as in the
// face of `shell` that also has it.
bool same_way(const TopoDS_Face& face, const TopoDS_Shape& shell) {
  TopExp_Explorer e(face, TopAbs_EDGE);
  if (!e.More()) {
    return false;
  }
  const TopoDS_Shape edge = e.Current();
  for (TopExp_Explorer f(shell, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer other(f.Current(), TopAbs_EDGE); other.More(); other.Next()) {
      if (other.Current().IsSame(edge)) {
        return other.Current().Orientation() == edge.Orientation();
      }
    }
  }
  return false;
}

// A loop of a profile moved by the motion into a solid: the sides named
// after its edges, the caps `first` at the profile and `last` at the end.
ShapePtr screwed_loop(const NamedWire& loop, const GrowingScrew& screw, double revolutions, const NameList& first,
                      const NameList& last) {
  // The law's curve: the axis, as long as the rise.
  const TopoDS_Edge line =
      BRepBuilderAPI_MakeEdge(screw.base, screw.base.Translated(gp_Vec(screw.axis) * screw.rise)).Edge();
  const TopoDS_Wire path = BRepBuilderAPI_MakeWire(line).Wire();
  const occ::handle<BRepFill_ShapeLaw> section = new BRepFill_ShapeLaw(loop.wire, true);
  const occ::handle<BRepFill_LocationLaw> location = new BRepFill_Edge3DLaw(path, new GrowingScrewLaw(screw));
  BRepFill_Sweep sweep(section, location, true);
  sweep.SetTolerance(1.0e-6, 1.0e-6, 1.0e-5, 1.0e-2);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> reversed;
  NCollection_DataMap<TopoDS_Shape, occ::handle<NCollection_HArray2<TopoDS_Shape>>, TopTools_ShapeMapHasher> tapes;
  NCollection_DataMap<TopoDS_Shape, occ::handle<NCollection_HArray2<TopoDS_Shape>>, TopTools_ShapeMapHasher> rails;
  const int segments = std::max(30, static_cast<int>(std::ceil(revolutions * 16.0)));
  sweep.Build(reversed, tapes, rails, BRepFill_Modified, GeomAbs_C2, GeomFill_Location, 11, segments);
  throw_if_cancelled();
  if (!sweep.IsDone()) {
    throw std::runtime_error("the profile could not be swept along the helix");
  }
  // The caps: the section's edges at the start and at the end, filled.
  const occ::handle<NCollection_HArray2<TopoDS_Shape>> sections = sweep.Sections();
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (TopExp_Explorer f(sweep.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    builder.Add(shell, f.Current());
  }
  std::vector<TopoDS_Face> caps;
  for (const int column : {sections->LowerCol(), sections->UpperCol()}) {
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    for (int i = 1; i <= section->NbLaw(); ++i) {
      const TopoDS_Shape& edge = sections->Value(i, column);
      if (!edge.IsNull() && edge.ShapeType() == TopAbs_EDGE) {
        builder.Add(wire, edge);
      }
    }
    wire.Closed(true);
    BRepLib_MakeFace cap(wire, true);
    if (!cap.IsDone()) {
      throw std::runtime_error("the helix's ends could not be closed");
    }
    // Oriented as the sides: each shared edge runs opposite ways in the two.
    TopoDS_Face face = cap.Face();
    if (same_way(face, sweep.Shape())) {
      face.Reverse();
    }
    caps.push_back(face);
    builder.Add(shell, face);
  }
  detail::FaceNamer namer(detail::oriented_solid(shell, "the helix"));
  const occ::handle<NCollection_HArray2<TopoDS_Shape>> faces = sweep.SubShape();
  for (int i = 1; i <= section->NbLaw(); ++i) {
    const TopoDS_Edge& edge = section->Edge(i);
    for (const auto& [named, name] : loop.edges) {
      if (named.IsSame(edge)) {
        for (int j = faces->LowerCol(); j <= faces->UpperCol(); ++j) {
          const TopoDS_Shape& face = faces->Value(i, j);
          if (!face.IsNull() && face.ShapeType() == TopAbs_FACE) {
            namer.add(face, name);
          }
        }
      }
    }
  }
  for (const std::string& name : first) {
    namer.add(caps.front(), name);
  }
  for (const std::string& name : last) {
    namer.add(caps.back(), name);
  }
  return namer.shape();
}

// Mitcad's widening helix (mitcad#83): each region moved by the growing
// screw motion; a profile that the narrowing would take across the axis
// fails.
ShapePtr growing_helix(const HelixSweepSpec& spec, const std::vector<detail::SweepFace>& faces) {
  GrowingScrew screw;
  screw.base = spec.axis.Location();
  screw.axis = spec.axis.Direction();
  screw.angle = (spec.left_handed ? -1.0 : 1.0) * kTwoPi * spec.revolutions;
  screw.rise = spec.pitch * spec.revolutions;
  screw.growth = spec.growth * spec.revolutions;
  // Out: from the axis towards the profiles' centre, else across the
  // profile plane's normal (profiles centred on the axis).
  const gp_Vec offset(screw.base, center_of(box_of(faces)));
  const gp_Vec radial = offset - gp_Vec(screw.axis) * offset.Dot(gp_Vec(screw.axis));
  const gp_Vec across = gp_Vec(screw.axis).Crossed(gp_Vec(spec.frame.normal()));
  if (radial.Magnitude() > Precision::Confusion()) {
    screw.out = gp_Dir(radial);
  } else if (across.Magnitude() > Precision::Confusion()) {
    screw.out = gp_Dir(across);
  } else {
    screw.out = detail::any_normal(screw.axis);
  }
  if (screw.growth < 0.0) {
    // The profiles' nearest point to the axis along `out` at the end.
    gp_Trsf local;
    local.SetTransformation(gp_Ax3(screw.base, screw.axis, screw.out));
    Bnd_Box box;
    for (const detail::SweepFace& face : faces) {
      BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(face.face, local, true).Shape(), box, false, false);
    }
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (x0 + screw.growth <= Precision::Confusion()) {
      throw std::invalid_argument("the helix narrows into its axis; make it narrow less or turn less");
    }
  }
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    const ShapePtr outer = screwed_loop(loops.front(), screw, spec.revolutions,
                                        detail::cap_names(spec.feature, "start", face),
                                        detail::cap_names(spec.feature, "end", face));
    std::vector<ShapePtr> holes;
    for (std::size_t l = 1; l < loops.size(); ++l) {
      holes.push_back(screwed_loop(loops[l], screw, spec.revolutions, {}, {}));
    }
    parts.push_back(detail::with_holes(outer, holes, "the helix"));
  }
  return detail::finished(*detail::fused(parts, "the helix"), "the helix");
}

} // namespace

ShapePtr helix_sweep(const HelixSweepSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to turn along the helix");
  }
  detail::require_positive("helix pitch", spec.pitch);
  detail::require_positive("helix revolutions", spec.revolutions);
  detail::require_finite("helix growth", spec.growth);
  return detail::run("helix", [&] {
    const std::vector<detail::SweepFace> faces =
        detail::merged(detail::region_faces(spec.feature, spec.frame, spec.regions));
    if (spec.growth != 0.0 && !spec.freecad) {
      return growing_helix(spec, faces);
    }
    if (spec.growth != 0.0) {
      const TopoDS_Wire spine = widening_path(spec, center_of(box_of(faces)));
      std::vector<ShapePtr> parts;
      for (const detail::SweepFace& face : faces) {
        parts.push_back(frenet_pipe(face, spine, detail::cap_names(spec.feature, "start", face),
                                    detail::cap_names(spec.feature, "end", face)));
      }
      return detail::finished(*detail::fused(parts, "the helix"), "the helix");
    }
    // A helix through the profiles' centre: with the axis as the fixed
    // binormal every helix of the axis and pitch moves the profiles alike
    // (each place along it is the start turned and raised).
    const gp_Dir axis = spec.axis.Direction();
    const gp_Vec offset(spec.axis.Location(), center_of(box_of(faces)));
    const double along = offset.Dot(gp_Vec(axis));
    const gp_Vec radial = offset - gp_Vec(axis) * along;
    double radius = radial.Magnitude();
    gp_Dir x = detail::any_normal(axis);
    if (radius > Precision::Confusion()) {
      x = gp_Dir(radial);
    } else {
      radius = 1.0;
    }
    gp_Ax3 axes(spec.axis.Location().Translated(gp_Vec(axis) * along), axis, x);
    if (spec.left_handed) {
      axes.YReverse();
    }
    // On the unrolled cylinder the helix is a line: a turn rises a pitch.
    const occ::handle<Geom_Surface> surface = new Geom_CylindricalSurface(axes, radius);
    const gp_Vec2d direction(kTwoPi, spec.pitch);
    const occ::handle<Geom2d_Line> line = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(direction));
    BRepBuilderAPI_MakeEdge helix(line, surface, 0.0, spec.revolutions * direction.Magnitude());
    if (!helix.IsDone()) {
      throw std::runtime_error("the helix could not be built");
    }
    TopoDS_Edge edge = helix.Edge();
    const int samples = std::max(32, static_cast<int>(std::ceil(spec.revolutions * 32.0)));
    BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, samples);
    const TopoDS_Wire spine = BRepBuilderAPI_MakeWire(edge).Wire();
    PipeMode mode;
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = axis;
    std::vector<ShapePtr> parts;
    for (const detail::SweepFace& face : faces) {
      const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
      const ShapePtr outer =
          detail::pipe_shell(loops.front(), spine, mode, detail::cap_names(spec.feature, "start", face),
                             detail::cap_names(spec.feature, "end", face), 1.0e-6);
      std::vector<ShapePtr> holes;
      for (std::size_t l = 1; l < loops.size(); ++l) {
        holes.push_back(detail::pipe_shell(loops[l], spine, mode, {}, {}, 1.0e-6));
      }
      parts.push_back(detail::with_holes(outer, holes, "the helix"));
    }
    return detail::finished(*detail::fused(parts, "the helix"), "the helix");
  });
}

} // namespace mitcad::geometry
