// SPDX-License-Identifier: MIT
#include "mitcad/geometry/path_sweep.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Ax2.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <TopExp.hxx>
#include <ElCLib.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>
#include <BRepBndLib.hxx>
#include <BRepFill_Edge3DLaw.hxx>
#include <BRepFill_ShapeLaw.hxx>
#include <BRepFill_Sweep.hxx>
#include <BRepGProp.hxx>
#include <GProp_GProps.hxx>
#include <BRepLib_MakeFace.hxx>
#include <BRep_Builder.hxx>
#include <GeomFill_LocationLaw.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_HArray2.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_Map.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Mat.hxx>
#include <Message_ProgressRange.hxx>
#include <ShapeFix_Solid.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
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

#include "canonical.hpp"
#include "helix_brep.hpp"
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

// The face's loops offset in its plane by `distance` (positive: away from the
// material, so that the outline grows and a hole closes in), each with its
// edges' names; a taper leans every side by this much per unit of the path.
// `loops` are the face's loops as detail::face_loops gives them.
std::vector<NamedWire> offset_loops(const TopoDS_Face& face, const std::vector<NamedWire>& loops, double distance) {
  BRepOffsetAPI_MakeOffset offset(face, GeomAbs_Intersection);
  offset.Perform(distance);
  if (!offset.IsDone() || offset.Shape().IsNull()) {
    throw std::invalid_argument("the taper closes the profile before the end of the sweep");
  }
  std::vector<NamedWire> result;
  for (const NamedWire& loop : loops) {
    NamedWire moved_loop;
    for (const auto& [edge, name] : loop.edges) {
      for (const TopoDS_Shape& image : offset.Generated(edge)) {
        if (image.ShapeType() == TopAbs_EDGE) {
          moved_loop.edges.emplace_back(TopoDS::Edge(image), name);
        }
      }
    }
    if (moved_loop.edges.size() < loop.edges.size()) {
      throw std::invalid_argument("the taper closes the profile before the end of the sweep");
    }
    for (TopExp_Explorer w(offset.Shape(), TopAbs_WIRE); w.More() && moved_loop.wire.IsNull(); w.Next()) {
      for (TopExp_Explorer e(w.Current(), TopAbs_EDGE); e.More(); e.Next()) {
        if (e.Current().IsSame(moved_loop.edges.front().first)) {
          moved_loop.wire = TopoDS::Wire(w.Current());
          break;
        }
      }
    }
    if (moved_loop.wire.IsNull()) {
      throw std::invalid_argument("the taper closes the profile before the end of the sweep");
    }
    result.push_back(std::move(moved_loop));
  }
  return result;
}

// A sweep that turns or sizes its profile along the path: copies of the
// profile placed along the part, lofted. The path must be smooth.
struct Variation {
  double twist = 0.0;
  double taper = 0.0;
  double scale = 1.0; // at the ends of the swept part (mitcad#464)
  std::optional<Spine> rail;
  std::optional<Spine> rail2; // a second rail (mitcad#464)
  ProfileScaling scaling = ProfileScaling::Scale;
  RailPoint point = RailPoint::Fraction; // mitcad#469
  const Spine* path = nullptr; // the whole path, for the rail's fractions
  double from = 0.0;           // where the part starts on it
  double swept = 0.0;          // the whole swept length the twist runs over (0: the part's)
};

// The rail's points at the stations by the plane rule (mitcad#469; see
// SweepSpec): where it crosses each station's plane square to the path.
// `frames` are the stations' frames (z along the path), `k0` the
// profile's station, `length` the part's length (for the message).
std::vector<gp_Pnt> plane_points(const Spine& rail, const std::vector<gp_Ax3>& frames,
                                 const std::vector<double>& stations, std::size_t k0, double length,
                                 double path_length) {
  const double tolerance = std::max(1e-3, 1e-6 * path_length);
  const double rail_length = rail.length();
  const double slack = 1e-9 * std::max(1.0, rail_length);
  const auto crossings = [&](std::size_t i) {
    return rail.crossings(gp_Pln(frames[i].Location(), frames[i].Direction()), tolerance);
  };
  const auto missing = [&](std::size_t i) {
    const long percent = length > 0.0 ? std::lround(100.0 * stations[i] / length) : 0;
    return std::invalid_argument("the guide rail does not reach the plane square to the path " +
                                 std::to_string(percent) +
                                 "% of the way along the swept part; it must run beside all of it");
  };
  std::vector<gp_Pnt> points(frames.size());
  // At the profile: the crossing nearest to the path.
  const std::vector<double> first = crossings(k0);
  if (first.empty()) {
    throw missing(k0);
  }
  const gp_Pnt p0 = frames[k0].Location();
  double start = first.front();
  for (const double s : first) {
    if (rail.point(s).Distance(p0) < rail.point(start).Distance(p0)) {
      start = s;
    }
  }
  points[k0] = rail.point(start);
  // The way the rail runs with the path there (onwards from its nearer
  // end where it crosses square to the path).
  const double along = gp_Vec(rail.tangent(start)).Dot(gp_Vec(frames[k0].Direction()));
  const int way = std::abs(along) > 1e-9 ? (along > 0.0 ? 1 : -1) : (start <= rail_length / 2.0 ? 1 : -1);
  // Station by station from the profile, each way: the first crossing on
  // from the last one.
  const auto walk = [&](int step, int sense) {
    double cursor = start;
    for (long i = static_cast<long>(k0) + step; i >= 0 && i < static_cast<long>(frames.size()); i += step) {
      const std::size_t at = static_cast<std::size_t>(i);
      double best = std::numeric_limits<double>::infinity();
      double next = cursor;
      for (const double s : crossings(at)) {
        double ahead = sense * (s - cursor);
        if (rail.closed()) {
          ahead = std::fmod(std::fmod(ahead, rail_length) + rail_length, rail_length);
          if (ahead > rail_length - slack) {
            ahead -= rail_length;
          }
        }
        if (ahead >= -slack && ahead < best) {
          best = ahead;
          next = s;
        }
      }
      if (!std::isfinite(best)) {
        throw missing(at);
      }
      cursor = next;
      points[at] = rail.point(cursor);
    }
  };
  walk(1, way);
  walk(-1, -way);
  return points;
}

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
  if (variation.rail2) {
    segments = std::max(segments, 8 * variation.rail2->edge_count());
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
  // station; with two rails (mitcad#464) the first rail's point (in the
  // station's plane) and the way across to the second, square to the path.
  std::vector<gp_Vec> towards(stations.size());
  std::vector<gp_Pnt> anchors(stations.size());
  if (variation.rail) {
    const double path_length = variation.path->length();
    // The rails' points: by the plane rule (mitcad#469) where they cross
    // the stations' planes, else at the stations' fractions of the path.
    std::vector<gp_Pnt> on_rail;
    std::vector<gp_Pnt> on_rail2;
    if (variation.point == RailPoint::Plane) {
      on_rail = plane_points(*variation.rail, frames, stations, k0, length, path_length);
      if (variation.rail2) {
        on_rail2 = plane_points(*variation.rail2, frames, stations, k0, length, path_length);
      }
    } else {
      for (std::size_t i = 0; i < stations.size(); ++i) {
        double s = variation.from + stations[i];
        if (variation.path->closed()) {
          s = std::fmod(std::fmod(s, path_length) + path_length, path_length);
        }
        const double fraction = std::clamp(s / path_length, 0.0, 1.0);
        on_rail.push_back(variation.rail->point(fraction * variation.rail->length()));
        if (variation.rail2) {
          on_rail2.push_back(variation.rail2->point(fraction * variation.rail2->length()));
        }
      }
    }
    for (std::size_t i = 0; i < stations.size(); ++i) {
      const gp_Pnt q = on_rail[i];
      const gp_Dir t = frames[i].Direction();
      gp_Vec v(frames[i].Location(), q);
      v -= gp_Vec(t) * v.Dot(gp_Vec(t));
      if (variation.rail2) {
        anchors[i] = frames[i].Location().Translated(v);
        v = gp_Vec(q, on_rail2[i]);
        v -= gp_Vec(t) * v.Dot(gp_Vec(t));
        if (v.Magnitude() <= Precision::Confusion()) {
          throw std::invalid_argument("the guide rails meet; they must run apart");
        }
      } else if (v.Magnitude() <= Precision::Confusion()) {
        throw std::invalid_argument("the guide rail meets the path; it must run beside it");
      }
      towards[i] = v;
    }
  }
  // The end scale (mitcad#464): from 1 at the profile to `scale` at each end
  // of the swept part, in proportion to the length run.
  const auto end_scale = [&](std::size_t i) {
    const double run = stations[i] - s0;
    const double side = run >= 0.0 ? length - s0 : s0;
    return side > 0.0 ? 1.0 + (variation.scale - 1.0) * std::abs(run) / side : 1.0;
  };
  const auto angle_of = [&](std::size_t i) {
    const gp_Vec n(frames[i].XDirection());
    const gp_Vec t(frames[i].Direction());
    return std::atan2(n.Crossed(towards[i]).Dot(t), n.Dot(towards[i]));
  };
  std::vector<gp_GTrsf> placements;
  bool affine = false;
  for (std::size_t i = 0; i < stations.size(); ++i) {
    const gp_Pnt p = frames[i].Location();
    const gp_Dir t = frames[i].Direction();
    gp_Trsf move;
    move.SetDisplacement(frames[k0], frames[i]);
    double angle = variation.twist * (stations[i] - s0) / (variation.swept > 0.0 ? variation.swept : length);
    double scale = variation.rail ? 1.0 : end_scale(i);
    if (variation.rail) {
      double turn = angle_of(i) - angle_of(k0);
      turn = std::remainder(turn, kTwoPi);
      angle = turn;
      if (variation.scaling != ProfileScaling::None) {
        scale = towards[i].Magnitude() / towards[k0].Magnitude();
      }
    }
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(p, t), angle);
    gp_Trsf rigid = turn.Multiplied(move);
    // Sizes about the path, or with two rails the profile moves with the
    // first rail and sizes about it.
    gp_Pnt center = p;
    if (variation.rail2) {
      gp_Trsf shift;
      shift.SetTranslation(anchors[k0].Transformed(rigid), anchors[i]);
      rigid = shift.Multiplied(rigid);
      center = anchors[i];
    }
    gp_GTrsf placement(rigid);
    if (std::abs(scale - 1.0) > 1e-12) {
      if (variation.rail && variation.scaling == ProfileScaling::Stretch) {
        gp_GTrsf stretch;
        stretch.SetAffinity(gp_Ax2(center, gp_Dir(towards[i])), scale);
        placement = stretch.Multiplied(placement);
        affine = true;
      } else {
        gp_Trsf grow;
        grow.SetScale(center, scale);
        placement = gp_GTrsf(grow.Multiplied(rigid));
      }
    }
    placements.push_back(placement);
  }
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const auto caps = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                detail::cap_names(spec.feature, "end", face));
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    // The profile at every station: moved, and with a taper offset first (each
    // side leans out by the angle, as an extrusion's does, so a square stays
    // a square and a circle becomes a cone).
    std::vector<std::vector<NamedWire>> at_station(stations.size());
    for (std::size_t i = 0; i < stations.size(); ++i) {
      const double lean = variation.taper != 0.0 ? std::abs(stations[i] - s0) * std::tan(variation.taper) : 0.0;
      const std::vector<NamedWire> shaped =
          std::abs(lean) > Precision::Confusion() ? offset_loops(face.face, loops, lean) : loops;
      for (const NamedWire& loop : shaped) {
        at_station[i].push_back(i == k0 ? loop : moved(loop, placements[i], affine));
      }
    }
    std::vector<ShapePtr> solids;
    for (std::size_t l = 0; l < loops.size(); ++l) {
      std::vector<TopoDS_Shape> sections;
      for (std::size_t i = 0; i < stations.size(); ++i) {
        sections.push_back(at_station[i][l].wire);
      }
      // The first section's own edges carry the names of the faces swept.
      solids.push_back(detail::through_sections(sections, at_station.front()[l], l == 0 ? caps.first : NameList{},
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

// How far a path may be from the guide surface it lies on, mm.
constexpr double kOnGuide = 1.0e-4;

// How far the curve that gives a path beside its guide surfaces their
// normal runs from the path (mm; mitcad#161).
constexpr double kBesideGuide = 1.0;

// The tolerance of the swept surfaces (`detail::pipe_shell`'s default), mm.
constexpr double kPipeTolerance = 1.0e-4;

// The tolerance of a sweep along a Frenet, auxiliary or constant binormal
// frame (mitcad#539), mm: OCCT's confusion, as FreeCAD's pipes in those
// modes are approximated (at 1e-4 a bar turned by an auxiliary spine
// held 800.0015 of its 800 mm³, 1.9e-6 off FreeCAD's).
constexpr double kFramedTolerance = 1.0e-7;

// How far the surface a path sweeps along a fixed binormal reaches from
// the path (mm; mitcad#397: only its normal along the path is used).
constexpr double kBinormalReach = 10.0;

// How far (radians) a path along a fixed binormal may turn where its
// curves meet (mitcad#397).
constexpr double kBinormalKink = 1.0e-6;

// A fixed binormal (mitcad#397): the surfaces the path's curves sweep
// moved along the binormal, each curve's own (a surface of linear
// extrusion, moved the other way along a curve reversed in the wire, so
// that every normal is the path's tangent x the binormal), and a copy of
// the path whose curves carry their curves on them (v = 0), for a pipe
// shell's `Support` mode: its frames are then the tangent, the level
// normal tangent x binormal and the binormal made square to the tangent.
TopoDS_Compound binormal_support(const TopoDS_Wire& wire, const gp_Dir& binormal, TopoDS_Wire& spine) {
  // (At a corner the frames on either side do not meet in a mitre.)
  std::vector<std::pair<gp_Vec, gp_Vec>> ends;
  for (BRepTools_WireExplorer e(wire); e.More(); e.Next()) {
    const BRepAdaptor_Curve curve(e.Current());
    gp_Pnt p;
    gp_Vec a;
    gp_Vec b;
    curve.D1(curve.FirstParameter(), p, a);
    curve.D1(curve.LastParameter(), p, b);
    if (e.Current().Orientation() == TopAbs_REVERSED) {
      ends.emplace_back(-b, -a);
    } else {
      ends.emplace_back(a, b);
    }
  }
  const std::size_t joints =
      ends.size() < 2 ? 0 : (BRep_Tool::IsClosed(wire) ? ends.size() : ends.size() - 1);
  for (std::size_t i = 0; i < joints; ++i) {
    const gp_Vec& out = ends[i].second;
    const gp_Vec& in = ends[(i + 1) % ends.size()].first;
    if (out.Magnitude() > 0.0 && in.Magnitude() > 0.0 && out.Angle(in) > kBinormalKink) {
      throw std::invalid_argument("unsupported: a fixed binormal along a path with corners");
    }
  }
  BRepBuilderAPI_Copy copy(wire, true, false);
  spine = TopoDS::Wire(copy.Shape());
  BRep_Builder builder;
  TopoDS_Compound support;
  builder.MakeCompound(support);
  for (TopExp_Explorer e(spine, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
    if (curve.IsNull()) {
      throw std::invalid_argument("a path curve has no geometry");
    }
    if (!location.IsIdentity()) {
      curve = occ::handle<Geom_Curve>::DownCast(curve->Transformed(location.Transformation()));
    }
    const gp_Dir along = edge.Orientation() == TopAbs_REVERSED ? binormal.Reversed() : binormal;
    const occ::handle<Geom_SurfaceOfLinearExtrusion> surface = new Geom_SurfaceOfLinearExtrusion(curve, along);
    BRepBuilderAPI_MakeFace face(surface, first, last, -kBinormalReach, kBinormalReach, Precision::Confusion());
    if (!face.IsDone()) {
      throw std::runtime_error("the path could not be swept along the binormal");
    }
    // (The edge's own location is taken into account by the builder.)
    builder.UpdateEdge(edge, new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(1.0, 0.0)), face.Face(),
                       BRep_Tool::Tolerance(edge));
    builder.Add(support, face.Face());
  }
  return support;
}

// The faces of a sweep's guide surfaces (every piece of each name).
std::vector<TopoDS_Face> guide_faces(const std::vector<GuideFace>& guide) {
  std::vector<TopoDS_Face> faces;
  for (const GuideFace& g : guide) {
    if (!g.body) {
      throw std::invalid_argument("a guide surface's body is missing");
    }
    const std::vector<int> found = g.body->find_faces(g.face);
    if (found.empty()) {
      throw std::invalid_argument("guide surface " + g.face + " does not exist");
    }
    for (const int i : found) {
      faces.push_back(g.body->face(i));
    }
  }
  return faces;
}

// Guided sweeps that pass over themselves (mitcad#161).

// Where a sweep of sections reaching `reach` from the spine passes over
// itself (a thread's profile taller than its pitch, swept along the
// thread): the arc lengths that cut the spine into pieces none of which
// comes back within two reaches of itself, {0, length} when it does not.
std::vector<double> overlap_cuts(const Spine& spine, double reach) {
  const double length = spine.length();
  if (reach <= 0.0 || length <= 4.0 * reach) {
    return {0.0, length};
  }
  const double step = std::max(std::min(reach / 2.0, length / 16.0), length / 4000.0);
  const int n = static_cast<int>(std::ceil(length / step));
  std::vector<gp_Pnt> points;
  for (int i = 0; i <= n; ++i) {
    points.push_back(spine.point(length * i / n));
  }
  // The shortest stretch of the spine between two points farther apart
  // along it than four reaches but within two reaches of each other.
  double shortest = length;
  for (int i = 0; i <= n; ++i) {
    for (int j = i + 1; j <= n; ++j) {
      const double along = length * (j - i) / n;
      if (along > 4.0 * reach && along < shortest && points[i].Distance(points[j]) < 2.0 * reach) {
        shortest = along;
      }
    }
  }
  if (shortest >= length) {
    return {0.0, length};
  }
  // (Pieces shorter than that stretch: as few as there can be.)
  const int pieces = static_cast<int>(std::ceil(length / (0.9 * shortest)));
  std::vector<double> cuts;
  for (int k = 0; k <= pieces; ++k) {
    cuts.push_back(length * k / pieces);
  }
  return cuts;
}

// The perpendicular sweep along guide surfaces (mitcad#157): the frames
// follow the faces' normal, on a path on them (whose curves carry their
// curves on the faces they lie on) or beside them (their normal at the
// points nearest to it, mitcad#161). A sweep that passes over itself is
// swept in pieces, each section placed by the frames where its piece
// starts, and the pieces are fused. `at`: the profile's arc length along
// `part`.
ShapePtr guided_sweep(const SweepSpec& spec, const std::vector<detail::SweepFace>& faces,
                      const TopoDS_Wire& part, double at, const Range& range) {
  const std::vector<TopoDS_Face> support = guide_faces(spec.guide);
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Face& face : support) {
    builder.Add(compound, face);
  }
  const auto mode_on = [&](const TopoDS_Wire& piece, TopoDS_Wire& spine) {
    PipeMode mode;
    if (detail::lies_on_faces(piece, support, kOnGuide)) {
      spine = detail::on_faces(piece, support, kOnGuide);
      mode.kind = PipeMode::Kind::Support;
      mode.support = compound;
    } else {
      spine = piece;
      mode.kind = PipeMode::Kind::Auxiliary;
      mode.auxiliary = detail::normal_spine(piece, support, kBesideGuide);
    }
    return mode;
  };
  const Spine whole(part);
  const double s0 = std::clamp(at, 0.0, whole.length());
  const std::vector<double> cuts =
      range.around ? std::vector<double>{0.0, whole.length()} : overlap_cuts(whole, reach(faces, whole.point(s0)));
  // The frame of the tangent and the faces' normal at an arc length.
  const auto frame = [&](double s) {
    const gp_Pnt p = whole.point(s);
    const gp_Dir t = whole.tangent(s);
    const std::optional<gp_Dir> n = detail::guide_normal(compound, p);
    if (!n) {
      throw std::invalid_argument("the guide surface's normal is not found beside the path");
    }
    const gp_Vec across = gp_Vec(*n) - gp_Vec(t) * n->Dot(t);
    if (across.Magnitude() <= 1.0e-6) {
      throw std::invalid_argument("the path runs along the guide surface's normal");
    }
    return gp_Ax3(p, t, gp_Dir(across));
  };
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const auto [first, last] = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                         detail::cap_names(spec.feature, "end", face));
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    for (std::size_t k = 0; k + 1 < cuts.size(); ++k) {
      const double a = cuts[k];
      const double b = cuts[k + 1];
      const bool whole_part = cuts.size() == 2;
      TopoDS_Wire spine;
      const PipeMode mode = mode_on(whole_part ? part : whole.part(a, b), spine);
      // The sections where the piece starts, unless the profile is on it.
      const bool here = s0 >= a && s0 <= b;
      gp_Trsf move;
      if (!here) {
        move.SetDisplacement(frame(s0), frame(a));
      }
      const auto loop = [&](std::size_t l) {
        return here ? loops[l] : moved(loops[l], gp_GTrsf(move), false);
      };
      const ShapePtr outer = detail::pipe_shell(loop(0), spine, mode, k == 0 ? first : NameList{},
                                                k + 2 == cuts.size() ? last : NameList{});
      std::vector<ShapePtr> holes;
      for (std::size_t l = 1; l < loops.size(); ++l) {
        holes.push_back(detail::pipe_shell(loop(l), spine, mode, {}, {}));
      }
      parts.push_back(detail::with_holes(outer, holes, "the sweep"));
    }
  }
  if (cuts.size() == 2) {
    return detail::fused(parts, "the sweep");
  }
  // Pieces of a sweep that passes over itself: the sides of turns over each
  // other lie on one surface, swept apart within the pipe's tolerance
  // (exactly, the boolean left the overlaps twice), and keep their faces
  // apart (merging the swept faces of neighbouring pieces made invalid
  // faces).
  return detail::fused(parts, "the sweep", false, kPipeTolerance);
}

} // namespace

namespace {

// A planar face of a body as a sweep's profile (mitcad#543): a copy of the
// face (the sweep shares nothing with the body), its edges named after the
// body's (an edge without a name by its index among the face's edges), its
// caps keyed by the stored reference; `frame` becomes the face's plane, its
// normal the face's outward one.
detail::SweepFace profile_face_of(const SweepSpec& spec, Frame& frame) {
  const Shape& body = *spec.profile_body;
  const std::vector<int> found = body.find_faces(spec.profile_face);
  if (found.empty()) {
    throw std::invalid_argument("the body has no face " + spec.profile_face);
  }
  if (found.size() > 1) {
    throw std::invalid_argument(spec.profile_face + " names " + std::to_string(found.size()) +
                                " faces; a sweep takes one face");
  }
  const TopoDS_Face& face = body.face(found.front());
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull()) {
    throw std::invalid_argument("face " + spec.profile_face +
                                " has no surface (a mesh body's triangles): a sweep takes a planar face");
  }
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    throw std::invalid_argument("face " + spec.profile_face + " is not planar: a sweep takes a planar face");
  }
  const detail::InputCopy copy(face);
  detail::SweepFace profile{TopoDS::Face(copy.shape()), {}, {spec.profile_key}};
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(face, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    const std::optional<std::string> name = body.name_of_edge(edge);
    profile.edges.emplace_back(TopoDS::Edge(copy.of(edge)),
                               face_name(spec.feature, "side", name ? *name : std::to_string(i - 1)));
  }
  const gp_Pln plane = surface.Plane();
  gp_Dir normal = plane.Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  frame.origin = plane.Location();
  frame.x_axis = plane.XAxis().Direction();
  frame.y_axis = normal.Crossed(frame.x_axis);
  return profile;
}

} // namespace

ShapePtr sweep(const SweepSpec& requested) {
  // (A body face as the profile, mitcad#543, sets the frame to its plane.)
  SweepSpec spec = requested;
  if (spec.regions.empty() && !spec.profile_body) {
    throw std::invalid_argument("no profiles to sweep");
  }
  if (spec.profile_body && (spec.twist != 0.0 || spec.taper != 0.0 || spec.scale != 1.0 || !spec.rail.empty() ||
                            !spec.rail2.empty() || !spec.guide.empty())) {
    throw std::invalid_argument(
        "unsupported: a sweep of a body face with twists, tapers, end scales, guide rails or guide surfaces");
  }
  detail::require_finite("twist angle", spec.twist);
  detail::require_finite("taper angle", spec.taper);
  if (std::abs(spec.taper) >= kPi / 2.0) {
    throw std::invalid_argument("the taper angle must be less than 90 degrees");
  }
  if (!(spec.scale >= 0.01 && spec.scale <= 100.0)) {
    throw std::invalid_argument("the sweep's end scale must be between 0.01 and 100");
  }
  return detail::run("sweep", [&] {
    const std::vector<detail::SweepFace> faces =
        spec.profile_body ? std::vector<detail::SweepFace>{profile_face_of(spec, spec.frame)}
                          : detail::merged(detail::region_faces(spec.feature, spec.frame, spec.regions));
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
    const bool on_surface = !spec.guide.empty();
    if (on_surface && guided) {
      throw std::invalid_argument("a sweep takes a guide rail or guide surfaces, not both");
    }
    if (on_surface && spec.orientation == SweepOrientation::Parallel) {
      throw std::invalid_argument("unsupported: guide surfaces with parallel orientation");
    }
    if (!spec.rail2.empty() && !guided) {
      throw std::invalid_argument("a second guide rail needs a first one");
    }
    if (spec.scale != 1.0 && (guided || on_surface)) {
      throw std::invalid_argument("an end scale is not taken with guide rails or guide surfaces");
    }
    const bool varied =
        !on_surface && (guided || spec.twist != 0.0 || spec.taper != 0.0 || spec.scale != 1.0);
    if (varied && spec.orientation == SweepOrientation::Parallel) {
      throw std::invalid_argument(
          "unsupported: twists, tapers, end scales and guide rails with parallel orientation");
    }
    if (varied && range.around) {
      throw std::invalid_argument(
          "unsupported: twists, tapers, end scales and guide rails round a whole closed path");
    }
    // A fixed binormal (mitcad#397) only for a plain perpendicular sweep.
    if (spec.binormal && (varied || on_surface || spec.orientation == SweepOrientation::Parallel)) {
      throw std::invalid_argument(
          "unsupported: a fixed binormal with twists, tapers, guide rails, guide surfaces or parallel orientation");
    }
    // A Frenet or auxiliary frame (mitcad#539), likewise.
    const int frames = int(spec.frenet) + int(!spec.auxiliary.empty()) + int(spec.constant_binormal.has_value());
    const bool framed = frames > 0;
    if (frames > 1) {
      throw std::invalid_argument(
          "a sweep follows the Frenet frame, an auxiliary curve or a constant binormal, not both");
    }
    if (framed && (varied || on_surface || spec.binormal || spec.orientation == SweepOrientation::Parallel)) {
      throw std::invalid_argument(
          "unsupported: a Frenet or auxiliary frame with twists, tapers, guide rails, guide surfaces, a fixed "
          "binormal or parallel orientation");
    }
    // (The frame is the tangent, binormal x tangent and the binormal square
    // to the tangent: it has none where the path runs along the binormal.)
    if (spec.binormal) {
      constexpr int kStations = 64;
      for (int i = 0; i <= kStations; ++i) {
        const double s = range.from + (range.to - range.from) * i / kStations;
        if (gp_Vec(path.tangent(s)).Crossed(gp_Vec(*spec.binormal)).Magnitude() < 1.0e-6) {
          throw std::invalid_argument("the path runs along the sweep's binormal");
        }
      }
    }
    ShapePtr result;
    if (varied) {
      Variation variation;
      if (guided) {
        variation.rail.emplace(detail::path_wire(spec.rail));
        if (!spec.rail2.empty()) {
          variation.rail2.emplace(detail::path_wire(spec.rail2));
        }
        variation.scaling = spec.scaling;
        variation.point = spec.rail_point;
      } else {
        variation.twist = spec.twist;
        variation.taper = spec.taper;
        variation.scale = spec.scale;
      }
      variation.path = &path;
      variation.from = range.from;
      const double inside = 1e-9 * std::max(1.0, path.length());
      if (variation.scale != 1.0 && at - range.from > inside && range.to - at > inside) {
        // An end scale both ways from the profile (mitcad#464) grows the
        // profile away from it on either side: a crease there, which one
        // smooth loft through all the copies would round over. Each side
        // is lofted from the profile and the two are fused; the far ends
        // keep their caps (start beyond the profile, end before it).
        variation.swept = range.to - range.from;
        std::vector<ShapePtr> sides;
        for (const auto& [from, to] : {std::pair{at, range.to}, std::pair{range.from, at}}) {
          Range side;
          side.from = from;
          side.to = to;
          side.at_start = false;
          variation.from = from;
          sides.push_back(varied_sweep(spec, faces, path.part(from, to), at, variation, side));
        }
        result = detail::fused(sides, "the sweep");
      } else {
        result = varied_sweep(spec, faces, part, at, variation, range);
      }
    } else if (spec.orientation == SweepOrientation::Parallel) {
      PipeMode mode;
      mode.kind = PipeMode::Kind::Fixed;
      mode.fixed = gp_Ax2(path.point(at), path.tangent(at));
      result = parallel_sweep(spec, faces, part, mode, at, range);
    } else {
      if (on_surface) {
        result = guided_sweep(spec, faces, part, at - range.from, range);
      } else {
        PipeMode mode = detail::perpendicular_mode(path, spec.path);
        TopoDS_Wire spine = part;
        if (spec.binormal) {
          mode.kind = PipeMode::Kind::Support;
          mode.support = binormal_support(part, *spec.binormal, spine);
        } else if (spec.frenet) {
          mode.kind = PipeMode::Kind::Frenet;
        } else if (!spec.auxiliary.empty()) {
          mode.kind = PipeMode::Kind::Auxiliary;
          mode.auxiliary = detail::path_wire(spec.auxiliary);
          mode.curvilinear = spec.curvilinear;
        } else if (spec.constant_binormal) {
          mode.kind = PipeMode::Kind::Binormal;
          mode.binormal = *spec.constant_binormal;
        }
        std::vector<ShapePtr> parts;
        for (const detail::SweepFace& face : faces) {
          const auto [first, last] = cap_order(range, detail::cap_names(spec.feature, "start", face),
                                               detail::cap_names(spec.feature, "end", face));
          const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
          const ShapePtr outer = detail::pipe_shell(loops.front(), spine, mode, first, last, framed ? kFramedTolerance : kPipeTolerance);
          std::vector<ShapePtr> holes;
          for (std::size_t l = 1; l < loops.size(); ++l) {
            holes.push_back(detail::pipe_shell(loops[l], spine, mode, {}, {}, framed ? kFramedTolerance : kPipeTolerance));
          }
          parts.push_back(detail::with_holes(outer, holes, "the sweep"));
        }
        result = detail::fused(parts, "the sweep");
      }
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
  // The radius of the circle round the section: half the circle's size,
  // the square's side over the root of two, two thirds of the triangle's
  // height (mitcad#25: the volumes of the variant models).
  double radius = spec.size / 2.0;
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
    radius = spec.size / std::sqrt(2.0);
    inside = radius / std::sqrt(2.0);
    shrink = spec.thickness * std::sqrt(2.0);
    corners = 4;
    angle = kPi / 4.0;
    break;
  case PipeSection::Triangular:
    radius = 2.0 * spec.size / 3.0;
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
    gp_Pnt center = start;
    if (spec.section == PipeSection::Triangular) {
      // A corner points along the path's plane normal (along x without a
      // plane) and the path passes through the middle of the height: the
      // circle's centre lies a sixth of the height towards the base.
      const gp_Dir apex = plane ? *plane : x;
      if (plane) {
        angle = kPi / 2.0;
      }
      center = start.Translated(gp_Vec(apex) * (-spec.size / 6.0));
    }
    const NamedWire outer = section(spec.feature, "side", center, tangent, x, radius, corners, angle);
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
    const PipeMode mode = detail::perpendicular_mode(path, spec.path);
    ShapePtr result = detail::pipe_shell(outer, part, mode, first, last, kPipeTolerance, spec.corners);
    if (spec.thickness > 0.0) {
      const NamedWire inner =
          section(spec.feature, "inner", center, tangent, x, radius - shrink, corners, angle);
      result = detail::with_holes(
          result, {detail::pipe_shell(inner, part, mode, {}, {}, kPipeTolerance, spec.corners)}, "the pipe");
    }
    return detail::finished(*result, "the pipe");
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
//
// With ends (HelixSweepSpec, mitcad#235) the rise is not in proportion to
// the turn: `ends` gives the rise at each angle, and the growth follows the
// rise.
struct RiseLaw {
  double per_radian = 0.0; // the rise per radian at the full pitch
  double start_flat = 0.0;
  double start_transition = 0.0;
  double middle = 0.0;
  double end_transition = 0.0;
  double end_flat = 0.0;

  double total() const { return start_flat + start_transition + middle + end_transition + end_flat; }

  // The angles where the pieces meet, from the start (inside the turn).
  std::vector<double> breaks() const {
    std::vector<double> out;
    double at = 0.0;
    for (const double piece : {start_flat, start_transition, middle, end_transition}) {
      at += piece;
      if (piece > 0.0 && at > 0.0 && at < total()) {
        out.push_back(at);
      }
    }
    return out;
  }

  // The rise at `phi` radians from the start and its first and second
  // derivatives by the angle.
  void at(double phi, double& z, double& dz, double& d2z) const {
    const double k = per_radian;
    double t = phi - start_flat;
    z = dz = d2z = 0.0;
    if (t <= 0.0) {
      return;
    }
    if (t <= start_transition) {
      z = k * t * t / (2.0 * start_transition);
      dz = k * t / start_transition;
      d2z = k / start_transition;
      return;
    }
    double base = k * start_transition / 2.0;
    t -= start_transition;
    if (t <= middle) {
      z = base + k * t;
      dz = k;
      return;
    }
    base += k * middle;
    t -= middle;
    if (t <= end_transition) {
      z = base + k * (t - t * t / (2.0 * end_transition));
      dz = k * (1.0 - t / end_transition);
      d2z = -k / end_transition;
      return;
    }
    z = base + k * end_transition / 2.0;
  }
};

struct GrowingScrew {
  gp_Pnt base;
  gp_Dir axis;
  gp_Dir out;
  double angle = 0.0;  // the whole turn, signed about `axis`
  double rise = 0.0;   // along the axis
  double growth = 0.0; // along `out`
  // Ends: the rise by the angle; the growth is then `growth` per `rise`.
  std::optional<RiseLaw> ends;
  // The part of the motion a sweep covers (fractions of the whole): turns
  // that pass through each other are swept in pieces.
  double from = 0.0;
  double to = 1.0;
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

  // With ends the motion is C1 where its pieces meet (the rise's second
  // derivative jumps there).
  int NbIntervals(const GeomAbs_Shape shape) const override {
    return static_cast<int>(inner_breaks(shape).size()) + 1;
  }
  void Intervals(NCollection_Array1<double>& t, const GeomAbs_Shape shape) const override {
    t(t.Lower()) = m_first;
    int i = t.Lower() + 1;
    for (const double at : inner_breaks(shape)) {
      t(i++) = at;
    }
    t(i) = m_last;
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
  // The curve's parameters inside its domain where the motion is only C1
  // (the pieces of the rise law meet), for a continuity `shape` above that.
  std::vector<double> inner_breaks(const GeomAbs_Shape shape) const {
    std::vector<double> out;
    if (!m_screw.ends || shape < GeomAbs_C2) {
      return out;
    }
    const double total = m_screw.ends->total();
    const double share = m_screw.to - m_screw.from;
    for (const double at : m_screw.ends->breaks()) {
      const double f = at / total;
      if (f > m_screw.from && f < m_screw.to) {
        out.push_back(m_first + (f - m_screw.from) / share * (m_last - m_first));
      }
    }
    return out;
  }

  // M, V and their derivatives by the curve's parameter up to `order`.
  void evaluate(double t, int order, gp_Mat& m, gp_Vec& v, gp_Mat& dm, gp_Vec& dv, gp_Mat& d2m,
                gp_Vec& d2v) const {
    // The curve's domain covers the fractions `from` to `to` of the motion:
    // `span` is what the whole would take.
    const double span = (m_last - m_first) / (m_screw.to - m_screw.from);
    const double u = m_screw.from + (t - m_first) / span;
    const double phi = u * m_screw.angle;
    const double c = std::cos(phi);
    const double s = std::sin(phi);
    gp_Mat identity;
    identity.SetIdentity();
    // R = I + sin K + (1 - cos) K^2, R' = cos K + sin K^2, R'' = -sin K + cos K^2.
    const gp_Mat r = identity + m_k.Multiplied(s) + m_k2.Multiplied(1.0 - c);
    // The rise z and the growth g along `out` and their derivatives by t:
    // in proportion to the turn, or with ends by the rise law (the growth
    // following the rise).
    double z = u * m_screw.rise;
    double dz = m_screw.rise / span;
    double d2z = 0.0;
    double g = u * m_screw.growth;
    double dg = m_screw.growth / span;
    double d2g = 0.0;
    if (m_screw.ends) {
      const double total = m_screw.ends->total();
      const double per_t = total / span;
      m_screw.ends->at(u * total, z, dz, d2z);
      dz *= per_t;
      d2z *= per_t * per_t;
      const double ratio = m_screw.rise != 0.0 ? m_screw.growth / m_screw.rise : 0.0;
      g = ratio * z;
      dg = ratio * dz;
      d2g = ratio * d2z;
    }
    // V = base + R w + z axis, w = g out - base.
    const gp_XYZ out = m_screw.out.XYZ();
    const gp_XYZ w = out * g - m_screw.base.XYZ();
    const gp_XYZ up = m_screw.axis.XYZ();
    m = r * m_trans;
    v = gp_Vec(m_screw.base.XYZ() + w.Multiplied(r) + up * z);
    if (order < 1) {
      return;
    }
    const double dphi = m_screw.angle / span; // dphi / dt
    const gp_Mat r1 = m_k.Multiplied(c) + m_k2.Multiplied(s);
    dm = r1.Multiplied(dphi) * m_trans;
    dv = gp_Vec(w.Multiplied(r1) * dphi + (out * dg).Multiplied(r) + up * dz);
    if (order < 2) {
      return;
    }
    const gp_Mat r2 = m_k.Multiplied(-s) + m_k2.Multiplied(c);
    d2m = r2.Multiplied(dphi * dphi) * m_trans;
    d2v = gp_Vec(w.Multiplied(r2) * (dphi * dphi) + (out * dg).Multiplied(r1) * (2.0 * dphi) +
                 (out * d2g).Multiplied(r) + up * d2z);
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
      BRepBuilderAPI_MakeEdge(screw.base, screw.base.Translated(gp_Vec(screw.axis) * (screw.rise != 0.0 ? screw.rise : 1.0))).Edge();
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
  TopoDS_Shape closed = detail::oriented_solid(shell, "the helix");
  // A profile wound the other way gives a solid turned inside out.
  GProp_GProps volume;
  BRepGProp::VolumeProperties(closed, volume);
  if (volume.Mass() < 0.0) {
    closed.Reverse();
  }
  detail::FaceNamer namer(closed);
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
// Whether the helix has ends (HelixSweepSpec, mitcad#235).
bool has_ends(const HelixSweepSpec& spec) {
  return spec.start_flat > 0.0 || spec.start_transition > 0.0 || spec.end_transition > 0.0 || spec.end_flat > 0.0;
}

// The pieces of a helix seamed a third of a turn apart (kShiftedPiece), the
// first seam `shift` turns from the start (HelixSweepSpec::piece_shift,
// mitcad#235): the turns from the start where each piece begins, and the
// whole `turns` at the end. A last piece shorter than kShortestPiece is
// joined to the one before it.
constexpr double kShiftedPiece = 1.0 / 3.0;
constexpr double kShortestPiece = 0.05;

std::vector<double> shifted_cuts(double turns, double shift) {
  std::vector<double> cuts{0.0};
  for (double at = shift; at < turns - kShortestPiece; at += kShiftedPiece) {
    cuts.push_back(at);
  }
  cuts.push_back(turns);
  return cuts;
}

ShapePtr growing_helix(const HelixSweepSpec& spec, const std::vector<detail::SweepFace>& faces) {
  GrowingScrew screw;
  screw.base = spec.axis.Location();
  screw.axis = spec.axis.Direction();
  screw.angle = (spec.left_handed ? -1.0 : 1.0) * kTwoPi * spec.revolutions;
  screw.rise = spec.pitch * spec.revolutions;
  screw.growth = spec.growth * spec.revolutions;
  double turns = spec.revolutions;
  if (has_ends(spec)) {
    RiseLaw law;
    law.per_radian = spec.pitch / kTwoPi;
    law.start_flat = spec.start_flat;
    law.start_transition = spec.start_transition;
    law.middle = kTwoPi * spec.revolutions;
    law.end_transition = spec.end_transition;
    law.end_flat = spec.end_flat;
    double z = 0.0;
    double dz = 0.0;
    double d2z = 0.0;
    law.at(law.total(), z, dz, d2z);
    screw.angle = (spec.left_handed ? -1.0 : 1.0) * law.total();
    screw.rise = z;
    screw.growth = spec.growth * z / spec.pitch;
    screw.ends = law;
    turns = law.total() / kTwoPi;
  }
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
  // Turns that pass through each other (a profile taller along the axis
  // than the rise over a turn, as on a flat end, mitcad#235) make a tool
  // that passes through itself, which the booleans cannot use: it is swept
  // in pieces of at most a third of a turn, and they are fused.
  int pieces = 1;
  if (screw.ends) {
    gp_Trsf local;
    local.SetTransformation(gp_Ax3(screw.base, screw.axis, screw.out));
    Bnd_Box box;
    for (const detail::SweepFace& face : faces) {
      BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(face.face, local, true).Shape(), box, false, false);
    }
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    const double tall = z1 - z0;
    const RiseLaw& law = *screw.ends;
    double least = std::numeric_limits<double>::infinity();
    const int steps = 720;
    for (int i = 0; i <= steps && law.total() > kTwoPi; ++i) {
      const double phi = (law.total() - kTwoPi) * i / steps;
      double a = 0, b = 0, da = 0, db = 0, d2a = 0, d2b = 0;
      law.at(phi, a, da, d2a);
      law.at(phi + kTwoPi, b, db, d2b);
      least = std::min(least, b - a);
    }
    if (tall >= least - Precision::Confusion()) {
      pieces = std::max(1, static_cast<int>(std::ceil(turns * 3.0 - 1.0e-9)));
    }
  }
  // Shifted seams (HelixSweepSpec::piece_shift): the pieces' turns.
  const bool shifted = spec.piece_shift > 0.0;
  const std::vector<double> cuts = shifted ? shifted_cuts(turns, spec.piece_shift) : std::vector<double>{};
  if (shifted) {
    pieces = static_cast<int>(cuts.size()) - 1;
  }
  std::vector<ShapePtr> parts;
  for (const detail::SweepFace& face : faces) {
    const std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
    for (int p = 0; p < pieces; ++p) {
      GrowingScrew piece = screw;
      double piece_turns = turns / pieces;
      if (shifted) {
        const std::size_t at = static_cast<std::size_t>(p);
        piece.from = cuts[at] / turns;
        piece.to = p == pieces - 1 ? 1.0 : cuts[at + 1] / turns;
        piece_turns = cuts[at + 1] - cuts[at];
      } else {
        piece.from = static_cast<double>(p) / pieces;
        piece.to = static_cast<double>(p + 1) / pieces;
      }
      const ShapePtr outer =
          screwed_loop(loops.front(), piece, piece_turns,
                       p == 0 ? detail::cap_names(spec.feature, "start", face) : NameList{},
                       p == pieces - 1 ? detail::cap_names(spec.feature, "end", face) : NameList{});
      std::vector<ShapePtr> holes;
      for (std::size_t l = 1; l < loops.size(); ++l) {
        holes.push_back(screwed_loop(loops[l], piece, piece_turns, {}, {}));
      }
      parts.push_back(detail::with_holes(outer, holes, "the helix"));
    }
  }
  return detail::finished(*detail::fused(parts, "the helix"), "the helix");
}

// Helices whose profile has a straight side on a cylinder of a body about
// the axis (mitcad#60). The side sweeps a band on that cylinder, coincident
// with the body's face but only approximately the same surface, and OCCT's
// booleans gave invalid results or lost faces. Two things make them work:
//
// - The helix is swept along a spine of edges of at most a third of a turn
//   each, so that every face of the tool winds less than half way round the
//   axis: the intersections with the body's coaxial faces are then single
//   arcs, which the booleans find reliably (one face winding several turns
//   is what they failed on, with or without the coincident band). The
//   shape is the same.
// - A side on a cylinder as long as the pitch makes the turns touch along
//   an edge on the cylinder, which no boolean resolves. Such a side is
//   moved off the cylinder, away from the profile, by kOffCylinder and
//   drawn in by as much at each end; its ends are joined by straight lines
//   to the neighbouring sides where those are kCornerDepth deep into the
//   profile. The sliver added lies where the boolean changes nothing (in
//   the void beyond the face a cut's profile lies on, in the material
//   behind the face a join's profile lies on), and the turns keep a strip of
//   the cylinder about twice kOffCylinder wide between them.
constexpr double kOffCylinder = 1.0e-4;
constexpr double kCornerDepth = 1.0e-3;
constexpr double kSpineTurns = 1.0 / 3.0;
// How close to a cylinder a side counts as on it (mm): thread profiles are
// often drawn a fraction of a micrometre off the cylinder they are cut
// into, which the booleans fail on as they do on sides exactly on it.
constexpr double kOnCylinder = 1.0e-3;
// How far the helix's swept sides may lie from the analytic surfaces they
// are taken as (the pipe's approximation tolerance, 1e-6 mm).
constexpr double kSweepExact = 1.0e-6;

// A cylinder of a body about the axis: its radius and its extent along
// the axis (from the axis' location).
struct AxialCylinder {
  double radius = 0.0;
  double from = 0.0;
  double to = 0.0;
};

// The cylindrical faces of the bodies about the axis.
std::vector<AxialCylinder> axial_cylinders(const std::vector<ShapePtr>& bodies, const gp_Ax1& axis) {
  std::vector<AxialCylinder> cylinders;
  const gp_Lin line(axis);
  for (const ShapePtr& body : bodies) {
    if (!body) {
      continue;
    }
    for (TopExp_Explorer f(body->occt(), TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Face& face = TopoDS::Face(f.Current());
      const BRepAdaptor_Surface surface(face, false);
      if (surface.GetType() != GeomAbs_Cylinder) {
        continue;
      }
      const gp_Cylinder cylinder = surface.Cylinder();
      const gp_Ax1 own = cylinder.Axis();
      if (!own.Direction().IsParallel(axis.Direction(), 1.0e-6) || line.Distance(own.Location()) > 1.0e-4) {
        continue;
      }
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      const double base = gp_Vec(axis.Location(), own.Location()).Dot(gp_Vec(axis.Direction()));
      const double way = own.Direction().Dot(axis.Direction());
      cylinders.push_back({cylinder.Radius(), base + std::min(v0, v1) * way, base + std::max(v0, v1) * way});
      if (cylinders.back().from > cylinders.back().to) {
        std::swap(cylinders.back().from, cylinders.back().to);
      }
    }
  }
  return cylinders;
}

// An edge of a loop in the loop's order, with its ends in that order.
struct LoopEdge {
  TopoDS_Edge edge;
  std::string name;
  gp_Pnt first;
  gp_Pnt last;
  bool on_cylinder = false;
  bool touching = false; // on a cylinder and as long as the pitch
  // The bodies' cylinders it lies on (touching sides).
  std::vector<AxialCylinder> cylinders;
};

// A side moved off a cylinder (kOffCylinder): the cylinder's radius, the
// way it was moved (towards the axis or away from it), the extents along
// the axis of the bodies' faces on that cylinder, the side's name, the
// direction from the axis to it and where it starts along the axis.
struct MovedSide {
  double radius = 0.0;
  bool inwards = false;
  std::vector<AxialCylinder> extents;
  std::string name;
  gp_Dir out;
  double low = 0.0;
  // The side in the profile's loop, and the way across it away from the
  // profile.
  TopoDS_Edge edge;
  gp_Dir away;
};

// The edge cut back from its end `at_last` (in the loop's order) to where
// it is kCornerDepth deep beyond the plane through `corner` across `into`
// (the profile's side of a side on a cylinder), and that point; at most
// half of it is cut, and nothing comes back when it does not get at least
// twice kOffCylinder deep in that half.
std::optional<std::pair<TopoDS_Edge, gp_Pnt>> cut_back(const LoopEdge& e, bool at_last, const gp_Pnt& corner,
                                                       const gp_Dir& into) {
  const BRepAdaptor_Curve curve(e.edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const double length = GCPnts_AbscissaPoint::Length(curve);
  // The loop runs the edge's way unless the edge is reversed.
  const bool from_last = at_last != (e.edge.Orientation() == TopAbs_REVERSED);
  const auto depth = [&](double u) { return gp_Vec(corner, curve.Value(u)).Dot(gp_Vec(into)); };
  std::optional<double> found;
  constexpr int kSteps = 40;
  for (int k = 1; k <= kSteps && !found; ++k) {
    const double s = 0.5 * length * k / kSteps;
    GCPnts_AbscissaPoint at(curve, from_last ? -s : s, from_last ? last : first);
    if (!at.IsDone()) {
      return std::nullopt;
    }
    const double d = depth(at.Parameter());
    if (d >= kCornerDepth || (k == kSteps && d >= 2.0 * kOffCylinder)) {
      found = at.Parameter();
    }
  }
  if (!found) {
    return std::nullopt;
  }
  const double u = *found;
  double curve_first = 0, curve_last = 0;
  const occ::handle<Geom_Curve> geometry = BRep_Tool::Curve(e.edge, curve_first, curve_last);
  if (geometry.IsNull()) {
    return std::nullopt;
  }
  BRepBuilderAPI_MakeEdge made(geometry, from_last ? first : u, from_last ? u : last);
  if (!made.IsDone()) {
    return std::nullopt;
  }
  TopoDS_Edge edge = made.Edge();
  edge.Orientation(e.edge.Orientation());
  return std::make_pair(edge, curve.Value(u));
}

// The loop of `face` with its sides on the cylinders that are as long as
// the pitch moved off them (see kOffCylinder); the loop itself when there
// are none. `on_cylinder` is set when any side lies on a cylinder. `rise`
// is the helix's travel along the axis. The sides moved are added to
// `moved_sides`; `touching` is set when any side is as long as the pitch.
NamedWire off_cylinders(const NamedWire& loop, const TopoDS_Face& face, const gp_Ax1& axis, double pitch,
                        double rise, const std::vector<AxialCylinder>& cylinders, bool& on_cylinder,
                        std::vector<MovedSide>& moved_sides, bool& touching) {
  const gp_Lin line(axis);
  const gp_Vec along(axis.Direction());
  std::vector<LoopEdge> edges;
  bool any = false;
  for (BRepTools_WireExplorer w(loop.wire); w.More(); w.Next()) {
    LoopEdge e;
    e.edge = w.Current();
    for (const auto& [named, name] : loop.edges) {
      if (named.IsSame(e.edge)) {
        e.name = name;
      }
    }
    e.first = BRep_Tool::Pnt(TopExp::FirstVertex(e.edge, true));
    e.last = BRep_Tool::Pnt(TopExp::LastVertex(e.edge, true));
    const BRepAdaptor_Curve curve(e.edge);
    const gp_Vec side(e.first, e.last);
    const double length = side.Magnitude();
    if (curve.GetType() == GeomAbs_Line && length > 10.0 * kOffCylinder &&
        std::abs(std::abs(side.Dot(along)) - length) <= 1.0e-9 * length) {
      const double radius = line.Distance(e.first);
      const double t0 = gp_Vec(axis.Location(), e.first).Dot(along);
      const double t1 = gp_Vec(axis.Location(), e.last).Dot(along);
      const double low = std::min(t0, t1);
      const double high = std::max(t0, t1) + rise;
      for (const AxialCylinder& c : cylinders) {
        if (std::abs(c.radius - radius) <= kOnCylinder && c.from < high - 1.0e-6 && c.to > low + 1.0e-6) {
          e.on_cylinder = true;
          e.touching = length >= pitch - 1.0e-6;
          e.cylinders.push_back(c);
        }
      }
    } else if (curve.GetType() == GeomAbs_Circle) {
      // An arc in a plane through the axis that touches a cylinder: its
      // point nearest to or farthest from the axis on it.
      const gp_Circ circle = curve.Circle();
      const gp_Pnt center = circle.Location();
      const gp_Vec off_axis = gp_Vec(axis.Location(), center) - along * gp_Vec(axis.Location(), center).Dot(along);
      const bool in_plane = std::abs(gp_Vec(circle.Axis().Direction()).Dot(along)) < 1.0e-9 &&
                            std::abs(gp_Vec(circle.Axis().Direction()).Dot(gp_Vec(axis.Location(), center))) < 1.0e-7;
      if (in_plane && off_axis.Magnitude() > Precision::Confusion()) {
        const gp_Vec out = off_axis.Normalized();
        const double t = gp_Vec(axis.Location(), center).Dot(along);
        for (const double sign : {-1.0, 1.0}) {
          const gp_Pnt touch = center.Translated(out * (sign * circle.Radius()));
          double u = ElCLib::Parameter(circle, touch);
          const double first = curve.FirstParameter();
          while (u < first) {
            u += kTwoPi;
          }
          const double radius = off_axis.Magnitude() + sign * circle.Radius();
          for (const AxialCylinder& c : cylinders) {
            if (u <= curve.LastParameter() + 1.0e-9 && std::abs(c.radius - radius) <= kOnCylinder &&
                c.from < t + rise && c.to > t) {
              e.on_cylinder = true;
            }
          }
        }
      }
    }
    on_cylinder = on_cylinder || e.on_cylinder;
    any = any || e.touching;
    touching = touching || e.touching;
    edges.push_back(e);
  }
  if (!any || edges.size() < 3) {
    return loop;
  }
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Plane) {
    return loop;
  }
  const gp_Dir plane_normal = surface.Plane().Axis().Direction();
  const std::size_t n = edges.size();
  // Each side's direction away from the profile (across it in the
  // profile's plane, to the side the face is not on), and its neighbours
  // cut back: the edges and their new ends.
  std::vector<gp_Dir> away(n);
  std::vector<TopoDS_Edge> trimmed(n);
  std::vector<std::optional<gp_Pnt>> new_first(n), new_last(n);
  std::vector<MovedSide> sides;
  for (std::size_t i = 0; i < n; ++i) {
    const LoopEdge& e = edges[i];
    if (!e.touching) {
      continue;
    }
    const std::size_t before = (i + n - 1) % n;
    const std::size_t after = (i + 1) % n;
    if (edges[before].touching || edges[after].touching) {
      return loop;
    }
    const gp_Vec side(e.first, e.last);
    const gp_Vec across = gp_Vec(plane_normal).Crossed(side);
    if (across.Magnitude() < 1.0e-9 * side.Magnitude()) {
      return loop;
    }
    away[i] = gp_Dir(across);
    const gp_Pnt probe = e.first.Translated(side / 2.0 + gp_Vec(away[i]) * kOffCylinder);
    if (BRepClass_FaceClassifier(face, probe, Precision::Confusion()).State() == TopAbs_IN) {
      away[i].Reverse();
    }
    const auto a = cut_back(edges[before], true, e.first, away[i].Reversed());
    const auto b = cut_back(edges[after], false, e.last, away[i].Reversed());
    // (A side between two such sides is left as it is.)
    if (!a || !b || !trimmed[before].IsNull() || !trimmed[after].IsNull()) {
      return loop;
    }
    trimmed[before] = a->first;
    new_last[before] = a->second;
    trimmed[after] = b->first;
    new_first[after] = b->second;
    const gp_Vec from_axis = gp_Vec(axis.Location(), e.first) - along * gp_Vec(axis.Location(), e.first).Dot(along);
    if (from_axis.Magnitude() <= Precision::Confusion()) {
      return loop;
    }
    const double low =
        std::min(gp_Vec(axis.Location(), e.first).Dot(along), gp_Vec(axis.Location(), e.last).Dot(along));
    sides.push_back({line.Distance(e.first), gp_Vec(away[i]).Dot(from_axis) < 0.0, e.cylinders, e.name,
                     gp_Dir(from_axis), low, e.edge, away[i]});
  }
  BRepBuilderAPI_MakeWire wire;
  NamedWire moved;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& name) {
    wire.Add(edge);
    if (!wire.IsDone()) {
      return false;
    }
    if (!name.empty()) {
      moved.edges.emplace_back(wire.Edge(), name);
    }
    return true;
  };
  const auto segment = [](const gp_Pnt& a, const gp_Pnt& b) { return BRepBuilderAPI_MakeEdge(a, b).Edge(); };
  for (std::size_t i = 0; i < n; ++i) {
    const LoopEdge& e = edges[i];
    if (!e.touching) {
      if (!add(trimmed[i].IsNull() ? e.edge : trimmed[i], e.name)) {
        return loop;
      }
      continue;
    }
    const gp_Vec u = gp_Vec(e.first, e.last).Normalized();
    const gp_Vec out = gp_Vec(away[i]) * kOffCylinder;
    const gp_Pnt a = e.first.Translated(out + u * kOffCylinder);
    const gp_Pnt b = e.last.Translated(out - u * kOffCylinder);
    const gp_Pnt from = *new_last[(i + n - 1) % n];
    const gp_Pnt to = *new_first[(i + 1) % n];
    if (!add(segment(from, a), e.name) || !add(segment(a, b), e.name) || !add(segment(b, to), e.name)) {
      return loop;
    }
  }
  if (!wire.IsDone() || !BRep_Tool::IsClosed(wire.Wire())) {
    return loop;
  }
  moved.wire = wire.Wire();
  moved_sides.insert(moved_sides.end(), sides.begin(), sides.end());
  return moved;
}

// How far the shapes reach along the axis (from its location): false when
// there is nothing.
bool axial_range(const std::vector<TopoDS_Shape>& shapes, const gp_Ax1& axis, double& from, double& to) {
  gp_Trsf local;
  local.SetTransformation(gp_Ax3(axis.Location(), axis.Direction()));
  Bnd_Box box;
  for (const TopoDS_Shape& shape : shapes) {
    if (!shape.IsNull()) {
      BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(shape, local, true).Shape(), box, false, false);
    }
  }
  if (box.IsVoid()) {
    return false;
  }
  double x0 = 0, y0 = 0, x1 = 0, y1 = 0;
  box.Get(x0, y0, from, x1, y1, to);
  return true;
}

// The helix with the slivers of its sides moved off the cylinders
// (kOffCylinder) cut back to the cylinder beyond the extents of the bodies'
// faces on it, where the bodies reach. A sliver lies in the void beyond the
// face (a cut's) or in the material behind it (a join's) only along the
// face: past a blind bore's bottom it cut a slot 0.1 um thick into the
// material there (a thread running out into a bore's bottom, mitcad#235),
// and past a shoulder of a rod it joined a skin 0.1 um thick to it. Beyond
// the faces the sides lie on the cylinder, where none of the bodies' faces
// coincides with them, and the turns stay apart there by the corners cut
// back. The helix turns `sign` (+1 or -1) about the axis and rises `pitch`
// a turn. `helix` is returned when there is nothing to cut back, and when
// the cut does not leave one valid solid smaller than it. The shells reach
// `far` beyond the cylinder and `near` into the profile's side; with
// `everywhere` they run over the whole helix less the faces, not only
// where the bodies reach (a core beyond the faces, see helix_brep, would be
// joined to the bodies where it lies outside them). `failed` (if given) is
// set when there was something to cut back and the cut did not do it.
ShapePtr slivers_within_faces(const ShapePtr& helix, const std::vector<MovedSide>& sides, const gp_Ax1& axis,
                              double pitch, double sign, const std::vector<ShapePtr>& bodies,
                              double far = 10.0 * kOffCylinder, double near = 0.0, bool everywhere = false,
                              bool* failed = nullptr) {
  if (sides.empty()) {
    return helix;
  }
  double z0 = 0, z1 = 0, b0 = 0, b1 = 0;
  std::vector<TopoDS_Shape> body_shapes;
  for (const ShapePtr& body : bodies) {
    if (body) {
      body_shapes.push_back(body->occt());
    }
  }
  if (!axial_range({helix->occt()}, axis, z0, z1) || !axial_range(body_shapes, axis, b0, b1)) {
    return helix;
  }
  // The shells about the cylinders, `far` beyond them (by default ten
  // times as wide as the slivers' reach) and `near` into the profile's side,
  // over the helix's and the bodies' extent less the faces'.
  const double low = everywhere ? z0 - 1.0 : std::max(z0, b0) - 1.0;
  const double high = everywhere ? z1 + 1.0 : std::min(z1, b1) + 1.0;
  NCollection_List<TopoDS_Shape> tools;
  std::vector<std::pair<TopoDS_Shape, std::string>> shells;
  for (const MovedSide& side : sides) {
    std::vector<std::pair<double, double>> along;
    for (const AxialCylinder& c : side.extents) {
      along.emplace_back(c.from, c.to);
    }
    std::sort(along.begin(), along.end());
    std::vector<std::pair<double, double>> gaps;
    double at = low;
    for (const auto& [from, to] : along) {
      if (from > at + Precision::Confusion()) {
        gaps.emplace_back(at, std::min(from, high));
      }
      at = std::max(at, to);
    }
    if (high > at + Precision::Confusion()) {
      gaps.emplace_back(at, high);
    }
    const double inner = side.inwards ? side.radius - far : side.radius - near;
    const double outer = side.inwards ? side.radius + near : side.radius + far;
    for (const auto& [from, to] : gaps) {
      if (to <= z0 || from >= z1 || (!everywhere && (to <= b0 || from >= b1))) {
        continue;
      }
      // The shell's seam half a turn from where its ends inside the helix
      // cross the strips between the turns: OCCT's cut built no solid
      // where the seam crossed the same piece of the helix's faces as such
      // an end (the faces split there not used).
      std::vector<double> crossings;
      for (const double end : {from, to}) {
        if (end > z0 && end < z1) {
          const double turns = (end - side.low) / pitch;
          crossings.push_back(kTwoPi * (turns - std::floor(turns)));
        }
      }
      double seam = kPi;
      if (crossings.size() == 1) {
        seam = crossings.front() + kPi;
      } else if (crossings.size() == 2) {
        // Halfway round from both, on the side farther from them.
        const double a = std::min(crossings[0], crossings[1]);
        const double b = std::max(crossings[0], crossings[1]);
        seam = b - a <= kPi ? (a + b) / 2.0 + kPi : (a + b) / 2.0;
      }
      gp_Ax2 place(axis.Location().Translated(gp_Vec(axis.Direction()) * from), axis.Direction(), side.out);
      place.Rotate(axis, sign * seam);
      BRepAlgoAPI_Cut ring(BRepPrimAPI_MakeCylinder(place, outer, to - from).Shape(),
                           BRepPrimAPI_MakeCylinder(place, inner, to - from).Shape());
      if (!ring.IsDone() || ring.HasErrors()) {
        return helix;
      }
      tools.Append(ring.Shape());
      shells.emplace_back(ring.Shape(), side.name);
    }
  }
  if (tools.IsEmpty()) {
    return helix;
  }
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(helix->occt());
  cut.SetArguments(arguments);
  cut.SetTools(tools);
  cut.SetNonDestructive(true);
  detail::build(cut);
  if (!cut.IsDone() || cut.HasErrors() || cut.HasWarnings()) {
    if (failed != nullptr) {
      *failed = true;
    }
    return helix;
  }
  int solids = 0;
  for (TopExp_Explorer s(cut.Shape(), TopAbs_SOLID); s.More(); s.Next()) {
    ++solids;
  }
  GProp_GProps before;
  GProp_GProps after;
  BRepGProp::VolumeProperties(helix->occt(), before, 1.0e-9);
  BRepGProp::VolumeProperties(cut.Shape(), after, 1.0e-9);
  // (A cut that removes nothing had nothing to cut back, but for the core
  // beyond the faces, which every gap across the helix meets.)
  const bool removed = after.Mass() < before.Mass() - 1.0e-9 * std::max(1.0, before.Mass());
  if (solids != 1 || !removed || !detail::is_valid(cut.Shape())) {
    if (failed != nullptr && (removed || solids != 1 || everywhere)) {
      *failed = true;
    }
    return helix;
  }
  detail::FaceNamer namer(cut.Shape());
  namer.carry(cut, *helix);
  // The faces the cut leaves on the cylinders are the sides'.
  for (const auto& [shell, name] : shells) {
    for (TopExp_Explorer f(shell, TopAbs_FACE); f.More(); f.Next()) {
      NCollection_List<TopoDS_Shape> pieces = cut.Modified(f.Current());
      if (pieces.IsEmpty()) {
        pieces.Append(f.Current());
      }
      for (const TopoDS_Shape& piece : pieces) {
        if (namer.contains(piece) && !namer.named(piece) && !name.empty()) {
          namer.add(piece, name);
        }
      }
    }
  }
  return namer.shape();
}

// A helix whose profile has a side on a cylinder of the bodies about the
// axis, built as a B-rep whose faces each run all the turns
// (detail::screwed_brep), as the bodies stored in files keep threads
// (mitcad#235): the sweep in pieces left a thread's run-out past a body's
// end face in many small faces, some of them slivers, that a later union
// could not take. A side as long as the pitch (`sides`, at most one) makes
// the turns meet in one edge there, as the stored bodies have them (the
// sweep moved that side 0.1 um off the cylinder instead, which left a strip
// of it 0.2 um wide between the turns that no rounding of the crest could
// take). The solid then holds a core beyond that side, out to `depth` and twice that at its middle (a fifth of the
// room to the next cylinder of the bodies beyond it, at most a tenth of the
// pitch), so that the turns join along the core's faces; along the bodies'
// faces on that cylinder the core lies where the boolean changes nothing.
// Beyond those faces the core is cut away (slivers_within_faces), over the
// whole helix (a join keeps what lies outside the bodies), and the turns
// meet in an edge there too. The faces are split every kSpineTurns, the
// pieces sharing their surface: the booleans meet faces that wind less than
// half a turn, and the merge after them makes each face whole again.
ShapePtr helix_brep(const HelixSweepSpec& spec, const detail::SweepFace& face,
                    const std::vector<AxialCylinder>& cylinders, const std::vector<MovedSide>& sides) {
  const NamedWire loop = detail::face_loops(face.face, face.edges).front();
  std::optional<detail::MeetingSide> meeting;
  double depth = 0.0;
  if (!sides.empty() && spec.revolutions > 1.0 + 1.0e-9) {
    const MovedSide& side = sides.front();
    // The room beyond the side: to the axis, and to the bodies' other
    // cylinders beyond it along its extent.
    double room = side.inwards ? side.radius : std::numeric_limits<double>::infinity();
    const double top = side.low + spec.pitch * (spec.revolutions + 1.0);
    for (const AxialCylinder& c : cylinders) {
      const double gap = side.inwards ? side.radius - c.radius : c.radius - side.radius;
      if (gap > kOnCylinder && c.from < top && c.to > side.low) {
        room = std::min(room, gap);
      }
    }
    depth = std::min(0.1 * spec.pitch, 0.2 * room);
    meeting = detail::MeetingSide{side.edge, side.away, depth};
  }
  ShapePtr helix =
      detail::screwed_brep(loop, spec.axis, spec.pitch, spec.revolutions, spec.left_handed,
                           detail::cap_names(spec.feature, "start", face), detail::cap_names(spec.feature, "end", face),
                           meeting, kSpineTurns);
  if (meeting) {
    bool failed = false;
    helix = slivers_within_faces(helix, sides, spec.axis, spec.pitch, spec.left_handed ? -1.0 : 1.0, spec.bodies,
                                 3.0 * depth, 0.0, true, &failed);
    if (failed) {
      throw std::runtime_error("the helix's core beyond the bodies' faces could not be cut away");
    }
  }
  return detail::finished(*helix, "the helix");
}

// How far the faces reach along the axis.
double axial_extent(const std::vector<detail::SweepFace>& faces, const gp_Ax1& axis) {
  gp_Trsf local;
  local.SetTransformation(gp_Ax3(axis.Location(), axis.Direction()));
  Bnd_Box box;
  for (const detail::SweepFace& face : faces) {
    BRepBndLib::AddOptimal(BRepBuilderAPI_Transform(face.face, local, true).Shape(), box, false, false);
  }
  if (box.IsVoid()) {
    return 0.0;
  }
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  return z1 - z0;
}

} // namespace

// A coil: the section lies in the plane through the axis and the start
// direction and moves by a screw motion (the helix's: it turns about the
// axis and rises, and a cone's or a spiral's section also moves out), so
// that its volume is the section's area times the turns of its centroid
// (mitcad#25: the volumes of the variant models). The size is the circle's
// diameter, the square's side or the triangle's height (its radial extent);
// the section is positioned by its radial extent.
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
  // The section's corners on a circle of radius r about its centre, the
  // first at `angle` from the way out, and how far it reaches out of the
  // coil (`out`), into it (`in`) and along the axis (`along`, half).
  double r = spec.size / 2.0;
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
    r = spec.size / std::sqrt(2.0);
    out = in = along = spec.size / 2.0;
    break;
  case CoilSection::TriangularExternal:
    corners = 3;
    r = 2.0 * spec.size / 3.0;
    out = r;
    in = r / 2.0;
    along = r * std::sqrt(3.0) / 2.0;
    break;
  case CoilSection::TriangularInternal:
    corners = 3;
    angle = kPi;
    r = 2.0 * spec.size / 3.0;
    out = r / 2.0;
    in = r;
    along = r * std::sqrt(3.0) / 2.0;
    break;
  }
  // The radius of the section's centre: its radial extent centred on the
  // diameter, or touching it from inside or outside.
  double radius = spec.diameter / 2.0 + (in - out) / 2.0;
  if (spec.position == CoilPosition::Inside) {
    radius = spec.diameter / 2.0 - out;
  } else if (spec.position == CoilPosition::Outside) {
    radius = spec.diameter / 2.0 + in;
  }
  if (radius - in <= Precision::Confusion()) {
    throw std::invalid_argument("the coil's section reaches its axis; make the diameter larger");
  }
  const double rise = spec.spiral ? 0.0 : spec.revolutions * spec.pitch;
  if (spec.spiral) {
    if (spec.pitch <= out + in) {
      throw std::invalid_argument("the spiral's pitch is smaller than its section, so its turns overlap");
    }
  } else {
    if (spec.pitch <= 2.0 * along) {
      throw std::invalid_argument("the coil's pitch is smaller than its section, so its turns overlap");
    }
    if (radius + rise * std::tan(spec.angle) - in <= Precision::Confusion()) {
      throw std::invalid_argument("the coil's angle narrows it into its axis");
    }
  }
  return detail::run("coil", [&] {
    const gp_Pnt origin = spec.frame.origin;
    const gp_Dir axis = spec.frame.normal();
    const gp_Dir x = spec.frame.x_axis;
    GrowingScrew screw;
    screw.base = origin;
    screw.axis = axis;
    screw.out = x;
    screw.angle = (spec.clockwise ? -1.0 : 1.0) * kTwoPi * spec.revolutions;
    screw.rise = rise;
    screw.growth = spec.spiral ? spec.revolutions * spec.pitch : rise * std::tan(spec.angle);
    // In the plane through the axis: the way out is the section's x and
    // the axis its y.
    const gp_Dir normal = gp_Dir(gp_Vec(x).Crossed(gp_Vec(axis)));
    const gp_Pnt center = origin.Translated(gp_Vec(x) * radius);
    const NamedWire profile = section(spec.feature, "side", center, normal, x, r, corners, angle);
    const ShapePtr result = screwed_loop(profile, screw, spec.revolutions, {face_name(spec.feature, "start")},
                                         {face_name(spec.feature, "end")});
    return detail::finished(*result, "the coil");
  });
}

ShapePtr helix_sweep(const HelixSweepSpec& spec) {
  if (spec.regions.empty()) {
    throw std::invalid_argument("no profiles to turn along the helix");
  }
  detail::require_positive("helix pitch", spec.pitch);
  detail::require_positive("helix revolutions", spec.revolutions);
  detail::require_finite("helix growth", spec.growth);
  for (const double end : {spec.start_flat, spec.start_transition, spec.end_transition, spec.end_flat}) {
    detail::require_finite("helix end angle", end);
    if (end < 0.0) {
      throw std::invalid_argument("the helix's end angles must not be negative");
    }
  }
  if (has_ends(spec) && spec.freecad) {
    throw std::invalid_argument("a helix with flat ends is built by Mitcad's construction, not FreeCAD's");
  }
  detail::require_finite("helix piece shift", spec.piece_shift);
  if (spec.piece_shift < 0.0 || spec.piece_shift >= kShiftedPiece) {
    throw std::invalid_argument("the helix's piece shift must be at least 0 and less than a third of a turn");
  }
  return detail::run("helix", [&] {
    const std::vector<detail::SweepFace> faces =
        detail::merged(detail::region_faces(spec.feature, spec.frame, spec.regions));
    if ((spec.growth != 0.0 || has_ends(spec)) && !spec.freecad) {
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
    // Sides on the bodies' cylinders (see kSpineTurns and kOffCylinder).
    const std::vector<AxialCylinder> cylinders = axial_cylinders(spec.bodies, spec.axis);
    std::vector<std::vector<NamedWire>> face_loops;
    bool on_cylinder = false;
    std::vector<MovedSide> moved_sides;
    bool touching = false;
    for (const detail::SweepFace& face : faces) {
      std::vector<NamedWire> loops = detail::face_loops(face.face, face.edges);
      if (!cylinders.empty()) {
        for (NamedWire& loop : loops) {
          loop = off_cylinders(loop, face.face, spec.axis, spec.pitch, spec.pitch * spec.revolutions, cylinders,
                               on_cylinder, moved_sides, touching);
        }
      }
      face_loops.push_back(std::move(loops));
    }
    // Turns that overlap (profiles longer along the axis than the pitch)
    // make a tool that passes through itself, which the booleans cannot
    // use: it is swept in pieces of at most kSpineTurns, none of which
    // meets itself, and they are fused.
    const bool overlapping = axial_extent(faces, spec.axis) > spec.pitch + 1.0e-6;
    // One edge, else edges of at most kSpineTurns each; with shifted seams
    // (HelixSweepSpec::piece_shift) always edges of kSpineTurns, the first
    // `piece_shift` long.
    const bool shifted = spec.piece_shift > 0.0;
    // A single profile of corners (no closed curve) with a side on a
    // cylinder of the bodies: built as a B-rep whose faces run all the turns
    // (helix_brep); a side as long as the pitch makes its turns meet. The
    // booleans on such faces sample, project and merge only the parts of the
    // faces they work on (OCCT port 0591 to 0593).
    const bool cornered = faces.size() == 1 && face_loops.front().size() == 1 &&
                          face_loops.front().front().edges.size() >= 2;
    if (on_cylinder && !overlapping && !shifted && cornered &&
        (moved_sides.empty() || (touching && moved_sides.size() == 1 && spec.revolutions > 1.0 + 1.0e-9))) {
      return helix_brep(spec, faces.front(), cylinders, moved_sides);
    }
    const std::vector<double> cuts = shifted ? shifted_cuts(spec.revolutions, spec.piece_shift) : std::vector<double>{};
    const int count = shifted ? static_cast<int>(cuts.size()) - 1
                      : on_cylinder || overlapping
                          ? std::max(1, static_cast<int>(std::ceil(spec.revolutions / kSpineTurns - 1.0e-9)))
                          : 1;
    // The turns where piece k starts.
    const auto turns_at = [&](int k) {
      return shifted ? cuts[static_cast<std::size_t>(k)] : spec.revolutions * k / count;
    };
    // A helix swept in pieces for the bodies' booleans has the sides that
    // straight sides along the axis sweep as cylinders: the sweep gives
    // them B-spline surfaces within its tolerance (kSweepExact) of their
    // cylinders, which later booleans there meet as approximations (a cut
    // across a thread beside its root's cylinder removed 0.02 mm^3 instead
    // of the file's 4.6, mitcad#235). Pieces span less than a turn, so each
    // such side is a patch of its cylinder.
    const auto exact_sides = [&](ShapePtr shape) {
      if (cylinders.empty() || count < 2) {
        return shape;
      }
      if (ShapePtr analytic = detail::canonical_dressup(Shape(TopoDS_Shape()), *shape, kSweepExact)) {
        return analytic;
      }
      return shape;
    };
    // The pieces fused, the slivers cut back beyond the bodies' faces and
    // the sides made exact.
    const auto helix_of = [&](const std::vector<ShapePtr>& pieces) {
      const ShapePtr fused = slivers_within_faces(detail::fused(pieces, "the helix"), moved_sides, spec.axis,
                                                  spec.pitch, spec.left_handed ? -1.0 : 1.0, spec.bodies);
      return exact_sides(detail::finished(*fused, "the helix"));
    };
    const double length = spec.revolutions * direction.Magnitude();
    BRepBuilderAPI_MakeWire spine_wire;
    std::vector<TopoDS_Edge> spine_edges;
    for (int k = 0; k < count; ++k) {
      const double from = shifted ? turns_at(k) * direction.Magnitude() : length * k / count;
      const double to =
          shifted ? (k == count - 1 ? length : turns_at(k + 1) * direction.Magnitude()) : length * (k + 1) / count;
      BRepBuilderAPI_MakeEdge helix(line, surface, from, to);
      if (!helix.IsDone()) {
        throw std::runtime_error("the helix could not be built");
      }
      TopoDS_Edge edge = helix.Edge();
      const double piece_turns = shifted ? turns_at(k + 1) - turns_at(k) : spec.revolutions / count;
      const int samples = std::max(32, static_cast<int>(std::ceil(piece_turns * 32.0)));
      BRepLib::BuildCurves3d(edge, 1.0e-7, GeomAbs_C1, 14, samples);
      spine_wire.Add(edge);
      spine_edges.push_back(edge);
    }
    const TopoDS_Wire spine = spine_wire.Wire();
    PipeMode mode;
    mode.kind = PipeMode::Kind::Binormal;
    mode.binormal = axis;
    std::vector<ShapePtr> parts;
    if (overlapping) {
      // Each piece from the profiles moved by the screw motion to its start.
      const double sign = spec.left_handed ? -1.0 : 1.0;
      for (int k = 0; k < count; ++k) {
        const double turns = turns_at(k);
        gp_Trsf turn;
        turn.SetRotation(spec.axis, sign * kTwoPi * turns);
        gp_Trsf rise;
        rise.SetTranslation(gp_Vec(axis) * (spec.pitch * turns));
        const gp_GTrsf screw(rise * turn);
        const TopoDS_Wire piece = BRepBuilderAPI_MakeWire(spine_edges[static_cast<std::size_t>(k)]).Wire();
        for (std::size_t f = 0; f < faces.size(); ++f) {
          const detail::SweepFace& face = faces[f];
          const std::vector<NamedWire>& loops = face_loops[f];
          const ShapePtr outer = detail::pipe_shell(
              moved(loops.front(), screw, false), piece, mode,
              k == 0 ? detail::cap_names(spec.feature, "start", face) : NameList{},
              k == count - 1 ? detail::cap_names(spec.feature, "end", face) : NameList{}, 1.0e-6);
          std::vector<ShapePtr> holes;
          for (std::size_t l = 1; l < loops.size(); ++l) {
            holes.push_back(detail::pipe_shell(moved(loops[l], screw, false), piece, mode, {}, {}, 1.0e-6));
          }
          parts.push_back(detail::with_holes(outer, holes, "the helix"));
        }
      }
      return helix_of(parts);
    }
    for (std::size_t f = 0; f < faces.size(); ++f) {
      const detail::SweepFace& face = faces[f];
      const std::vector<NamedWire>& loops = face_loops[f];
      const ShapePtr outer =
          detail::pipe_shell(loops.front(), spine, mode, detail::cap_names(spec.feature, "start", face),
                             detail::cap_names(spec.feature, "end", face), 1.0e-6);
      std::vector<ShapePtr> holes;
      for (std::size_t l = 1; l < loops.size(); ++l) {
        holes.push_back(detail::pipe_shell(loops[l], spine, mode, {}, {}, 1.0e-6));
      }
      parts.push_back(detail::with_holes(outer, holes, "the helix"));
    }
    return helix_of(parts);
  });
}

} // namespace mitcad::geometry
