// SPDX-License-Identifier: MIT
#include "mitcad/geometry/path_sweep.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
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
    const double at = path.locate(plane, center_of(box_of(faces)));
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
    } else {
      PipeMode mode = detail::perpendicular_mode(path);
      if (spec.orientation == SweepOrientation::Parallel) {
        mode.kind = PipeMode::Kind::Fixed;
        mode.fixed = gp_Ax2(path.point(at), path.tangent(at));
      }
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

ShapePtr helix_sweep(const HelixSweepSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to turn along the helix");
  }
  detail::require_positive("helix pitch", spec.pitch);
  detail::require_positive("helix revolutions", spec.revolutions);
  return detail::run("helix", [&] {
    const std::vector<detail::SweepFace> faces =
        detail::merged(detail::region_faces(spec.feature, spec.frame, spec.regions));
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
