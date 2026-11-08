// SPDX-License-Identifier: MIT
#include "sweep.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <limits>
#include <map>
#include <optional>
#include <string>
#include <stdexcept>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax3.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

gp_Pln plane_of(const Frame& frame) {
  return gp_Pln(gp_Ax3(frame.origin, frame.normal(), frame.x_axis));
}

// True when the wire runs clockwise about the plane's normal. The face is
// made with Inside = false: Inside = true would turn the face round to
// hold the wire's inside, and the test would always say counter-clockwise.
bool is_clockwise(const gp_Pln& plane, const TopoDS_Wire& wire) {
  BRepBuilderAPI_MakeFace face(plane, wire, false);
  if (!face.IsDone()) {
    throw std::runtime_error("a thin wall outline does not bound a planar region");
  }
  return BRepTopAdaptor_FClass2d(face.Face(), Precision::Confusion()).PerformInfinitePoint() ==
         TopAbs_IN;
}

TopoDS_Wire oriented(const gp_Pln& plane, const TopoDS_Wire& wire, bool counter_clockwise) {
  return is_clockwise(plane, wire) == counter_clockwise ? TopoDS::Wire(wire.Reversed()) : wire;
}

// The face inside `outline` and outside `hole`.
TopoDS_Face band(const gp_Pln& plane, const TopoDS_Wire& outline,
                 const std::optional<TopoDS_Wire>& hole) {
  BRepBuilderAPI_MakeFace maker(plane, oriented(plane, outline, true), true);
  if (hole) {
    maker.Add(oriented(plane, *hole, false));
  }
  if (!maker.IsDone()) {
    throw std::runtime_error("the thin wall could not be built");
  }
  return maker.Face();
}

// The outlines of the face's loops offset by `distance` (positive: away
// from the material), by the index of the loop's wire in `loops`. A loop
// that the offset closes up has none. `origin` maps each offset edge to the
// loop edge it comes from.
std::map<int, TopoDS_Wire> offset_loops(const TopoDS_Face& face, const std::vector<TopoDS_Wire>& loops,
                                        double distance, ShapeMap& offset_edges,
                                        std::vector<TopoDS_Shape>& origin) {
  BRepOffsetAPI_MakeOffset offset(face, GeomAbs_Intersection);
  offset.Perform(distance);
  if (!offset.IsDone()) {
    throw std::runtime_error("the thin wall could not be offset from the profile; it may be too thick");
  }
  std::vector<int> loop_of;
  for (std::size_t i = 0; i < loops.size(); ++i) {
    for (TopExp_Explorer e(loops[i], TopAbs_EDGE); e.More(); e.Next()) {
      for (const TopoDS_Shape& image : offset.Generated(e.Current())) {
        if (image.ShapeType() == TopAbs_EDGE && offset_edges.FindIndex(image) == 0) {
          offset_edges.Add(image);
          origin.push_back(e.Current());
          loop_of.push_back(static_cast<int>(i));
        }
      }
    }
  }
  std::map<int, TopoDS_Wire> result;
  for (TopExp_Explorer w(offset.Shape(), TopAbs_WIRE); w.More(); w.Next()) {
    for (TopExp_Explorer e(w.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const int index = offset_edges.FindIndex(e.Current());
      if (index > 0) {
        result.emplace(loop_of[static_cast<std::size_t>(index - 1)], TopoDS::Wire(w.Current()));
        break;
      }
    }
  }
  return result;
}

std::array<gp_Pnt, 8> corners(const Bnd_Box& box) {
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  std::array<gp_Pnt, 8> result;
  for (int i = 0; i < 8; ++i) {
    result[static_cast<std::size_t>(i)] =
        gp_Pnt(i & 1 ? x1 : x0, i & 2 ? y1 : y0, i & 4 ? z1 : z0);
  }
  return result;
}

double diagonal(const Bnd_Box& box) { return box.IsVoid() ? 0.0 : std::sqrt(box.SquareExtent()); }

gp_Pnt center_of(const Bnd_Box& box) {
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  return gp_Pnt((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0);
}

double along_of(const gp_Pnt& p, const gp_Dir& along) { return gp_Vec(p.XYZ()).Dot(gp_Vec(along)); }

// The face's surface continued past its edges, `size` far in every
// direction that has no natural end.
TopoDS_Face extended_face(const TopoDS_Face& face, double size) {
  const BRepAdaptor_Surface surface(face);
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const double vm = (v0 + v1) / 2.0;
  switch (surface.GetType()) {
  case GeomAbs_Cylinder:
    return BRepBuilderAPI_MakeFace(surface.Cylinder(), 0.0, kTwoPi, vm - size, vm + size).Face();
  case GeomAbs_Cone: {
    const gp_Cone cone = surface.Cone();
    // Up to the apex, on the side of it the face lies on.
    const double apex = -cone.RefRadius() / std::sin(cone.SemiAngle());
    const double gap = 1.0e-6 * std::max(1.0, size);
    const bool after = vm > apex;
    const double low = after ? std::max(apex + gap, vm - size) : vm - size;
    const double high = after ? vm + size : std::min(apex - gap, vm + size);
    return BRepBuilderAPI_MakeFace(cone, 0.0, kTwoPi, low, high).Face();
  }
  case GeomAbs_Sphere:
    return BRepBuilderAPI_MakeFace(surface.Sphere()).Face();
  case GeomAbs_Torus:
    return BRepBuilderAPI_MakeFace(surface.Torus()).Face();
  default: {
    TopLoc_Location location;
    const occ::handle<Geom_Surface> geometry = BRep_Tool::Surface(face, location);
    double a0 = 0, a1 = 0, b0 = 0, b1 = 0;
    geometry->Bounds(a0, a1, b0, b1);
    const auto clamp = [size](double value, double middle) {
      return std::isfinite(value) ? value : (value < 0 ? middle - size : middle + size);
    };
    const double um = (u0 + u1) / 2.0;
    BRepBuilderAPI_MakeFace maker(geometry, clamp(a0, um), clamp(a1, um), clamp(b0, vm), clamp(b1, vm),
                                  Precision::Confusion());
    if (!maker.IsDone()) {
      throw std::runtime_error("the target face cannot be extended");
    }
    return TopoDS::Face(maker.Face().Moved(location));
  }
  }
}

TopoDS_Shape translated(const TopoDS_Shape& shape, const gp_Vec& vector) {
  if (vector.Magnitude() <= Precision::Confusion()) {
    return shape;
  }
  gp_Trsf trsf;
  trsf.SetTranslation(vector);
  return BRepBuilderAPI_Transform(shape, trsf, true).Shape();
}

TopoDS_Face planar_face(const gp_Pln& plane, const gp_Pnt& center, double size) {
  const gp_Pnt middle = center.Translated(
      gp_Vec(plane.Axis().Direction()) *
      gp_Vec(plane.Location(), center).Dot(gp_Vec(plane.Axis().Direction())) * -1.0);
  return BRepBuilderAPI_MakeFace(gp_Pln(middle, plane.Axis().Direction()), -size, size, -size, size)
      .Face();
}

// Features of the shapes within a micron are one when a sweep is split by its
// target: the faces of a profile with a spline in place of a circle (a
// projected edge) deviate that much from the surface they stand for, and the
// splitting misses the target there.
constexpr double kSplitFuzzy = 1.0e-3;

std::vector<ShapePtr> split_pieces(const Shape& solid, const TopoDS_Shape& tool,
                                   const NameList& tool_names, const char* what) {
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(solid.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool);
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
  splitter.SetFuzzyValue(kSplitFuzzy);
  splitter.SetNonDestructive(true);
  detail::build(splitter);
  if (!splitter.IsDone() || splitter.HasErrors()) {
    throw std::runtime_error(std::string(what) + ": splitting with the target failed");
  }
  FaceNamer namer(splitter.Shape());
  namer.carry(splitter, solid);
  for (TopExp_Explorer f(tool, TopAbs_FACE); f.More(); f.Next()) {
    NCollection_List<TopoDS_Shape> images = splitter.Modified(f.Current());
    if (images.IsEmpty()) {
      images.Append(f.Current());
    }
    for (const TopoDS_Shape& image : images) {
      for (const std::string& name : tool_names) {
        namer.add(image, name);
      }
    }
  }
  std::vector<ShapePtr> pieces;
  for (const FaceNamer::Piece& piece : namer.pieces()) {
    pieces.push_back(piece.shape);
  }
  return pieces;
}

int count_named(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    const NameList& names = shape.face_names(i);
    count += std::find(names.begin(), names.end(), name) != names.end() ? 1 : 0;
  }
  return count;
}

} // namespace

NameList cap_names(const std::string& feature, const std::string& role, const SweepFace& face) {
  NameList names;
  for (const std::string& region : face.regions) {
    names.push_back(face_name(feature, role, region));
  }
  return names;
}

std::vector<SweepFace> region_faces(const std::string& feature, const Frame& frame,
                                    const std::vector<Region>& regions) {
  std::vector<SweepFace> faces;
  bool pinched = false;
  for (const Region& region : regions) {
    const ProfileFace profile = make_profile(frame, region);
    pinched = pinched || profile.pinched;
    SweepFace face{profile.face, {}, {region.name}};
    for (const auto& [edge, segment] : profile.edges) {
      face.edges.emplace_back(edge, face_name(feature, "side", segment));
    }
    faces.push_back(std::move(face));
  }
  // The prism of a face whose hole touches its outer boundary is no
  // manifold solid and does not fuse with the prisms beside it: touching
  // faces are united as faces first.
  return pinched ? merged(faces) : faces;
}

std::vector<SweepFace> merged(const std::vector<SweepFace>& faces) {
  if (faces.size() < 2) {
    return faces;
  }
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(faces.front().face);
  NCollection_List<TopoDS_Shape> tools;
  for (std::size_t i = 1; i < faces.size(); ++i) {
    tools.Append(faces[i].face);
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  detail::build(fuse);
  if (!fuse.IsDone() || fuse.HasErrors()) {
    throw std::runtime_error("the profiles could not be merged");
  }
  fuse.SimplifyResult();
  const auto images = [&fuse](const TopoDS_Shape& shape) {
    NCollection_List<TopoDS_Shape> result = fuse.Modified(shape);
    if (result.IsEmpty() && !fuse.IsDeleted(shape)) {
      result.Append(shape);
    }
    return result;
  };
  std::vector<SweepFace> result;
  for (TopExp_Explorer f(fuse.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    SweepFace face{TopoDS::Face(f.Current()), {}, {}};
    ShapeMap edges;
    TopExp::MapShapes(face.face, TopAbs_EDGE, edges);
    for (const SweepFace& input : faces) {
      for (const TopoDS_Shape& image : images(input.face)) {
        if (image.IsSame(face.face)) {
          face.regions.insert(face.regions.end(), input.regions.begin(), input.regions.end());
        }
      }
      for (const auto& [edge, name] : input.edges) {
        for (const TopoDS_Shape& image : images(edge)) {
          const int index = edges.FindIndex(image);
          if (index > 0) {
            face.edges.emplace_back(TopoDS::Edge(edges(index)), name);
          }
        }
      }
    }
    result.push_back(std::move(face));
  }
  return result;
}

namespace {

using NamedEdges = std::vector<std::pair<TopoDS_Edge, std::string>>;

// How far the wall reaches to side 1 and to side 2 of its curves.
std::pair<double, double> wall_sides(const ThinWall& wall) {
  require_positive("thin wall thickness", wall.thickness);
  if (wall.location == WallLocation::Both) {
    require_positive("thin wall thickness on side 2", wall.thickness2);
    return {wall.thickness, wall.thickness2};
  }
  const double side1 = wall.location == WallLocation::Side1    ? wall.thickness
                       : wall.location == WallLocation::Center ? wall.thickness / 2.0
                                                               : 0.0;
  return {side1, wall.thickness - side1};
}

// The walls along the loops of a planar face whose edges follow the named
// segments: side 1 away from its material.
void loop_walls(const std::string& feature, const gp_Pln& plane, const TopoDS_Face& profile,
                const NamedEdges& named, const std::string& region, double side1, double side2,
                std::vector<SweepFace>& faces) {
  ShapeMap profile_edges;
  std::vector<std::string> segments;
  for (const auto& [edge, segment] : named) {
    profile_edges.Add(edge);
    segments.push_back(segment);
  }
  std::vector<TopoDS_Wire> loops{BRepTools::OuterWire(profile)};
  for (TopExp_Explorer w(profile, TopAbs_WIRE); w.More(); w.Next()) {
    if (!w.Current().IsSame(loops.front())) {
      loops.push_back(TopoDS::Wire(w.Current()));
    }
  }
  // The outlines on each side of the curves, and the curve each of their
  // edges follows.
  ShapeMap edges1;
  ShapeMap edges2;
  std::vector<TopoDS_Shape> origin1;
  std::vector<TopoDS_Shape> origin2;
  std::map<int, TopoDS_Wire> outline1;
  std::map<int, TopoDS_Wire> outline2;
  if (side1 > 0.0) {
    outline1 = offset_loops(profile, loops, side1, edges1, origin1);
  }
  if (side2 > 0.0) {
    outline2 = offset_loops(profile, loops, -side2, edges2, origin2);
  }
  for (std::size_t i = 0; i < loops.size(); ++i) {
    const int loop = static_cast<int>(i);
    if (side1 <= 0.0) {
      outline1[loop] = loops[i];
    }
    if (side2 <= 0.0) {
      outline2[loop] = loops[i];
    }
  }
  const auto segment_of = [&](const TopoDS_Shape& edge, const ShapeMap& offset_edges,
                              const std::vector<TopoDS_Shape>& origin) -> std::string {
    const int offset = offset_edges.FindIndex(edge);
    const TopoDS_Shape& source = offset > 0 ? origin[static_cast<std::size_t>(offset - 1)] : edge;
    const int index = profile_edges.FindIndex(source);
    return index > 0 ? segments[static_cast<std::size_t>(index - 1)] : std::string();
  };
  for (std::size_t i = 0; i < loops.size(); ++i) {
    const auto one = outline1.find(static_cast<int>(i));
    const auto two = outline2.find(static_cast<int>(i));
    const bool has1 = one != outline1.end();
    const bool has2 = two != outline2.end();
    // Side 1 lies away from the material: outside the outer loop, inside
    // a hole.
    const bool outer = i == 0;
    std::optional<TopoDS_Wire> outline;
    std::optional<TopoDS_Wire> hole;
    if (outer) {
      if (has1) outline = one->second;
      if (has2) hole = two->second;
    } else {
      if (has2) outline = two->second;
      if (has1) hole = one->second;
    }
    if (!outline) {
      continue;
    }
    SweepFace face{band(plane, *outline, hole), {}, {region}};
    ShapeMap on1;
    if (has1) {
      TopExp::MapShapes(one->second, TopAbs_EDGE, on1);
    }
    for (TopExp_Explorer e(face.face, TopAbs_EDGE); e.More(); e.Next()) {
      const bool first = on1.FindIndex(e.Current()) > 0;
      const std::string segment = first ? segment_of(e.Current(), edges1, origin1)
                                        : segment_of(e.Current(), edges2, origin2);
      if (!segment.empty()) {
        face.edges.emplace_back(TopoDS::Edge(e.Current()),
                                face_name(feature, first ? "outer" : "inner", segment));
      }
    }
    faces.push_back(std::move(face));
  }
}

// The edges of a wire in order along it, and its first and last point.
struct WireRun {
  std::vector<TopoDS_Edge> edges;
  gp_Pnt first;
  gp_Pnt last;
};

WireRun run_of(const TopoDS_Wire& wire) {
  WireRun run;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    run.edges.push_back(it.Current());
  }
  if (run.edges.empty()) {
    throw std::runtime_error("a thin wall outline has no edges");
  }
  TopoDS_Vertex a;
  TopoDS_Vertex b;
  TopExp::Vertices(run.edges.front(), a, b, true);
  run.first = BRep_Tool::Pnt(a);
  TopExp::Vertices(run.edges.back(), a, b, true);
  run.last = BRep_Tool::Pnt(b);
  return run;
}

// The open wire moved `distance` (> 0) to its left (`left`) or right in
// the plane, its edges named by the curves they follow (`names` holds the
// wire's edges and their curves) with `role`.
std::pair<TopoDS_Wire, NamedEdges> offset_open(const gp_Pln& plane, const TopoDS_Wire& wire,
                                               const NamedEdges& names, double distance, bool left,
                                               const std::string& feature, const char* role) {
  // An open result lies to the left of the wire whatever the sign of the
  // distance, so the right side is the left of the wire reversed. Which
  // side it is, is checked by the point the wire's middle moves to.
  const TopoDS_Edge& probe = names.front().first;
  const BRepAdaptor_Curve curve(probe);
  const double middle = (curve.FirstParameter() + curve.LastParameter()) / 2.0;
  gp_Pnt at;
  gp_Vec tangent;
  curve.D1(middle, at, tangent);
  if (probe.Orientation() == TopAbs_REVERSED) {
    tangent.Reverse();
  }
  if (tangent.Magnitude() <= Precision::Confusion()) {
    throw std::invalid_argument("a thin wall curve has no direction");
  }
  const gp_Vec to_left = gp_Vec(plane.Axis().Direction()).Crossed(tangent).Normalized();
  for (const bool reversed : {!left, left}) {
    BRepOffsetAPI_MakeOffset offset;
    offset.Init(BRepBuilderAPI_MakeFace(plane).Face(), GeomAbs_Intersection, true);
    offset.AddWire(reversed ? TopoDS::Wire(wire.Reversed()) : wire);
    offset.Perform(distance);
    if (!offset.IsDone()) {
      throw std::runtime_error("the thin wall could not be offset from its curves; it may be too thick");
    }
    std::vector<TopoDS_Wire> wires;
    for (TopExp_Explorer w(offset.Shape(), TopAbs_WIRE); w.More(); w.Next()) {
      wires.push_back(TopoDS::Wire(w.Current()));
    }
    if (wires.size() != 1) {
      throw std::invalid_argument("the thin wall is too thick for the curves it follows");
    }
    BRepExtrema_DistShapeShape nearest(BRepBuilderAPI_MakeVertex(at).Vertex(), wires.front());
    if (!nearest.IsDone() || nearest.NbSolution() == 0) {
      throw std::runtime_error("the thin wall could not be offset from its curves");
    }
    const double side = gp_Vec(at, nearest.PointOnShape2(1)).Dot(to_left);
    if ((side > 0.0) != left) {
      continue;
    }
    NamedEdges named;
    ShapeMap edges;
    TopExp::MapShapes(wires.front(), TopAbs_EDGE, edges);
    for (const auto& [edge, curve_name] : names) {
      for (const TopoDS_Shape& image : offset.Generated(edge)) {
        const int index = edges.FindIndex(image);
        if (image.ShapeType() == TopAbs_EDGE && index > 0) {
          named.emplace_back(TopoDS::Edge(edges(index)), face_name(feature, role, curve_name));
        }
      }
    }
    return {wires.front(), named};
  }
  throw std::runtime_error("the thin wall could not be offset to the side asked for");
}

// The wall along an open chain: the band between its two sides, square to
// it at its ends.
SweepFace open_wall(const std::string& feature, const gp_Pln& plane, const TopoDS_Wire& wire,
                    const NamedEdges& names, const Path& chain, const std::string& key, double side1,
                    double side2) {
  // Side 1 to the right, side 2 to the left.
  const auto outline = [&](double distance, bool left, const char* role) {
    if (distance > 0.0) {
      return offset_open(plane, wire, names, distance, left, feature, role);
    }
    NamedEdges named;
    for (const auto& [edge, curve_name] : names) {
      named.emplace_back(edge, face_name(feature, role, curve_name));
    }
    return std::make_pair(wire, named);
  };
  const auto [right, right_names] = outline(side1, false, "outer");
  const auto [left, left_names] = outline(side2, true, "inner");
  const WireRun chain_run = run_of(wire);
  WireRun one = run_of(right);
  WireRun two = run_of(left);
  // Both runs from the chain's start.
  const auto from_start = [&chain_run](WireRun& run) {
    if (run.last.Distance(chain_run.first) < run.first.Distance(chain_run.first)) {
      std::reverse(run.edges.begin(), run.edges.end());
      std::swap(run.first, run.last);
    }
  };
  from_start(one);
  from_start(two);
  std::map<int, std::string> roles;
  ShapeMap named_edges;
  for (const NamedEdges* list : {&right_names, &left_names}) {
    for (const auto& [edge, name] : *list) {
      const int index = named_edges.Add(edge);
      roles[index] = name;
    }
  }
  BRepBuilderAPI_MakeWire maker;
  SweepFace face{TopoDS_Face(), {}, {key}};
  std::vector<std::pair<TopoDS_Edge, std::string>> added;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& name) {
    maker.Add(edge);
    if (!maker.IsDone()) {
      throw std::invalid_argument("the thin wall's outline does not close; it may be too thick");
    }
    if (!name.empty()) {
      added.emplace_back(maker.Edge(), name);
    }
  };
  const auto name_of = [&](const TopoDS_Edge& edge) {
    const int index = named_edges.FindIndex(edge);
    return index > 0 ? roles[index] : std::string();
  };
  // Side 1 from the start to the end, across the end, side 2 back, and
  // across the start.
  for (const TopoDS_Edge& edge : one.edges) {
    add(edge, name_of(edge));
  }
  add(BRepBuilderAPI_MakeEdge(one.last, two.last).Edge(), face_name(feature, "tip1", chain.back().name));
  for (auto it = two.edges.rbegin(); it != two.edges.rend(); ++it) {
    add(*it, name_of(*it));
  }
  add(BRepBuilderAPI_MakeEdge(two.first, one.first).Edge(), face_name(feature, "tip0", chain.front().name));
  face.face = band(plane, maker.Wire(), std::nullopt);
  if (!BRepCheck_Analyzer(face.face).IsValid()) {
    throw std::invalid_argument("the thin wall folds over itself; it may be too thick for its curves");
  }
  ShapeMap on_face;
  TopExp::MapShapes(face.face, TopAbs_EDGE, on_face);
  for (const auto& [edge, name] : added) {
    const int index = on_face.FindIndex(edge);
    if (index > 0) {
      face.edges.emplace_back(TopoDS::Edge(on_face(index)), name);
    }
  }
  return face;
}

} // namespace

std::vector<SweepFace> wall_faces(const std::string& feature, const Frame& frame,
                                  const std::vector<Region>& regions, const ThinWall& wall) {
  const auto [side1, side2] = wall_sides(wall);
  const gp_Pln plane = plane_of(frame);
  std::vector<SweepFace> faces;
  for (const Region& region : regions) {
    const ProfileFace profile = make_profile(frame, region);
    loop_walls(feature, plane, profile.face, profile.edges, region.name, side1, side2, faces);
  }
  return faces;
}

std::vector<SweepFace> chain_wall_faces(const std::string& feature, const Frame& frame,
                                        const std::vector<Path>& chains, const NameList& keys,
                                        const ThinWall& wall) {
  if (chains.empty()) {
    return {};
  }
  if (keys.size() != chains.size()) {
    throw std::invalid_argument("each chain of curves needs its key");
  }
  const auto [side1, side2] = wall_sides(wall);
  const gp_Pln plane = plane_of(frame);
  std::vector<SweepFace> faces;
  for (std::size_t i = 0; i < chains.size(); ++i) {
    const Path& chain = chains[i];
    if (chain.empty()) {
      throw std::invalid_argument("a chain of thin wall curves is empty");
    }
    BRepBuilderAPI_MakeWire maker;
    NamedEdges names;
    for (const PathCurve& piece : chain) {
      maker.Add(model_edge(piece.curve));
      if (!maker.IsDone()) {
        throw std::invalid_argument("thin wall curve " + piece.name + " does not join the curve before it");
      }
      names.emplace_back(maker.Edge(), piece.name);
    }
    const TopoDS_Wire wire = maker.Wire();
    const WireRun run = run_of(wire);
    const double length = std::max(1.0, diagonal(bounds_of(wire)));
    if (run.first.Distance(run.last) <= 1.0e-6 * length) {
      // A closed chain bounds an area: its walls as a region's.
      BRepBuilderAPI_MakeFace area(plane, wire, true);
      if (!area.IsDone() || !BRepCheck_Analyzer(area.Face()).IsValid()) {
        throw std::invalid_argument("the closed chain of " + chain.front().name +
                                    " does not bound an area; it may cross itself");
      }
      ShapeMap on_face;
      TopExp::MapShapes(area.Face(), TopAbs_EDGE, on_face);
      NamedEdges named;
      for (const auto& [edge, name] : names) {
        const int index = on_face.FindIndex(edge);
        if (index > 0) {
          named.emplace_back(TopoDS::Edge(on_face(index)), name);
        }
      }
      loop_walls(feature, plane, area.Face(), named, keys[i], side1, side2, faces);
    } else {
      faces.push_back(open_wall(feature, plane, wire, names, chain, keys[i], side1, side2));
    }
  }
  return faces;
}

SweepFace transformed(const SweepFace& face, const gp_Trsf& trsf) {
  BRepBuilderAPI_Transform transform(face.face, trsf, true);
  SweepFace result{TopoDS::Face(transform.Shape()), {}, face.regions};
  for (const auto& [edge, name] : face.edges) {
    result.edges.emplace_back(TopoDS::Edge(transform.ModifiedShape(edge)), name);
  }
  return result;
}

ShapePtr named_prism(const SweepFace& face, const gp_Vec& vector, const NameList& near,
                     const NameList& far) {
  BRepPrimAPI_MakePrism maker(face.face, vector);
  maker.Build();
  if (!maker.IsDone()) {
    throw std::runtime_error("the extrusion of a profile failed");
  }
  FaceNamer namer(maker.Shape());
  for (const auto& [edge, name] : face.edges) {
    namer.generated(maker, edge, name);
  }
  for (const std::string& name : near) {
    namer.add(maker.FirstShape(), name);
  }
  for (const std::string& name : far) {
    namer.add(maker.LastShape(), name);
  }
  return namer.shape();
}

ShapePtr with_swept_names(const Shape& solid,
                          const std::vector<std::pair<TopoDS_Edge, std::string>>& edges,
                          const gp_Ax1& axis, double angle) {
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < solid.face_count(); ++i) {
    faces.push_back({solid.face(i), solid.face_names(i)});
  }
  bool added = false;
  for (const auto& [edge, name] : edges) {
    if (name.empty() || !solid.find_faces(name).empty() || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
    // On the axis an edge sweeps nothing.
    if (gp_Lin(axis).Distance(middle) <= Precision::Confusion()) {
      continue;
    }
    gp_Trsf turn;
    turn.SetRotation(axis, angle / 2.0);
    middle.Transform(turn);
    const TopoDS_Vertex probe = BRepBuilderAPI_MakeVertex(middle).Vertex();
    for (int i = 0; i < solid.face_count(); ++i) {
      BRepExtrema_DistShapeShape distance(probe, solid.face(i));
      if (distance.IsDone() && distance.Value() <= 1.0e-6) {
        faces[static_cast<std::size_t>(i)].names.push_back(name);
        added = true;
        break;
      }
    }
  }
  return added ? std::make_shared<Shape>(solid.occt(), faces) : std::make_shared<Shape>(solid);
}

namespace {

// Throws when leaning a cylindrical side face by `angle` would take its
// cone through the apex before the face ends: an arc of the profile that
// the taper shrinks to nothing (the profile closes). OCCT's draft does not
// fail there but can loop forever intersecting the cones (T1b).
void check_cone_apexes(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                       const gp_Dir& pull, double angle) {
  const double slope = std::tan(angle);
  for (int i = 0; i < solid.face_count(); ++i) {
    if (!leaned[static_cast<std::size_t>(i)]) {
      continue;
    }
    const TopoDS_Face& face = solid.face(i);
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Cylinder) {
      continue;
    }
    const gp_Cylinder cylinder = surface.Cylinder();
    // How far the face runs from the neutral plane along the pull.
    double height = 0.0;
    for (TopExp_Explorer v(face, TopAbs_VERTEX); v.More(); v.Next()) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
      height = std::max(height, std::abs(gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull))));
    }
    // Convex (the material lies towards the axis): a taper away from the
    // material widens it; concave (a hole's wall): it narrows.
    const double u = (surface.FirstUParameter() + surface.LastUParameter()) / 2.0;
    const double v = (surface.FirstVParameter() + surface.LastVParameter()) / 2.0;
    const gp_Pnt point = surface.Value(u, v);
    const gp_Ax1& axis = cylinder.Axis();
    const gp_Vec along(axis.Direction());
    const gp_Pnt foot = axis.Location().Translated(along * gp_Vec(axis.Location(), point).Dot(along));
    if (foot.Distance(point) <= Precision::Confusion()) {
      continue;
    }
    const gp_Vec inward(point, foot);
    const double step = std::min(1.0e-3, 1.0e-3 * cylinder.Radius());
    BRepClass3d_SolidClassifier inside(solid.occt(), point.Translated(inward.Normalized() * step),
                                       Precision::Confusion());
    const double side = inside.State() == TopAbs_IN ? 1.0 : -1.0;
    const double far = cylinder.Radius() + side * height * slope;
    if (far <= std::max(Precision::Confusion(), 1.0e-6 * cylinder.Radius())) {
      throw std::invalid_argument(
          "the taper closes the profile: an arc shrinks to nothing before the end of the extrusion");
    }
  }
}

// The part of space where `n . x + d <= 0`.
struct HalfSpace {
  gp_XYZ n;
  double d = 0.0;
  double at(const gp_XYZ& p) const { return n.Dot(p) + d; }
};

using Polygon = std::vector<gp_XYZ>;

// The convex polygon cut down to a half-space (Sutherland-Hodgman).
Polygon clipped(const Polygon& polygon, const HalfSpace& half) {
  Polygon out;
  const std::size_t count = polygon.size();
  for (std::size_t i = 0; i < count; ++i) {
    const gp_XYZ& a = polygon[i];
    const gp_XYZ& b = polygon[(i + 1) % count];
    const double fa = half.at(a);
    const double fb = half.at(b);
    if (fa <= 0.0) {
      out.push_back(a);
    }
    if ((fa < 0.0 && fb > 0.0) || (fa > 0.0 && fb < 0.0)) {
      out.push_back(a + (b - a) * (fa / (fa - fb)));
    }
  }
  return out;
}

// A square of side 2 * `size` in the plane through `centre` with the unit
// normal `normal`.
Polygon square_in(const gp_XYZ& centre, const gp_XYZ& normal, double size) {
  const gp_Dir axis(normal);
  const gp_Ax3 frame(gp_Pnt(centre), axis);
  const gp_XYZ u = gp_XYZ(frame.XDirection().XYZ()) * size;
  const gp_XYZ v = gp_XYZ(frame.YDirection().XYZ()) * size;
  return {centre - u - v, centre + u - v, centre + u + v, centre - u + v};
}

double polygon_area(const Polygon& polygon) {
  gp_XYZ sum(0, 0, 0);
  for (std::size_t i = 0; i < polygon.size(); ++i) {
    sum += polygon[i].Crossed(polygon[(i + 1) % polygon.size()]);
  }
  return sum.Modulus() / 2.0;
}

// A taper that closes a circle: the cylinder, leaned in by `angle`, is a
// cone that ends in its apex before the far end of the extrusion. The solid
// then is the cone from the near cap to the apex. Null unless the solid is
// a cylinder along the pull between two caps whose side leans in and the
// apex lies before the far cap.
ShapePtr closing_cone(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                      const gp_Dir& pull, double angle) {
  if (!(angle < 0.0) || solid.face_count() != 3) {
    return nullptr;
  }
  int side = -1;
  int near_cap = -1;
  double near_height = std::numeric_limits<double>::infinity();
  for (int i = 0; i < solid.face_count(); ++i) {
    const BRepAdaptor_Surface surface(solid.face(i));
    if (leaned[static_cast<std::size_t>(i)]) {
      if (side >= 0 || surface.GetType() != GeomAbs_Cylinder) {
        return nullptr;
      }
      side = i;
    } else {
      if (surface.GetType() != GeomAbs_Plane ||
          !surface.Plane().Axis().Direction().IsParallel(pull, 1.0e-9)) {
        return nullptr;
      }
      const double height = std::abs(gp_Vec(neutral.Location(), surface.Plane().Location()).Dot(gp_Vec(pull)));
      if (height < near_height) {
        near_height = height;
        near_cap = i;
      }
    }
  }
  if (side < 0 || near_cap < 0) {
    return nullptr;
  }
  const BRepAdaptor_Surface surface(solid.face(side));
  const gp_Cylinder cylinder = surface.Cylinder();
  if (!cylinder.Axis().Direction().IsParallel(pull, 1.0e-9) || near_height > 1.0e-7) {
    return nullptr;
  }
  // The material lies inside the cylinder.
  const double u = (surface.FirstUParameter() + surface.LastUParameter()) / 2.0;
  const double v = (surface.FirstVParameter() + surface.LastVParameter()) / 2.0;
  const gp_Pnt point = surface.Value(u, v);
  const gp_Ax1& axis = cylinder.Axis();
  const gp_Vec along(axis.Direction());
  const gp_Pnt foot = axis.Location().Translated(along * gp_Vec(axis.Location(), point).Dot(along));
  if (foot.Distance(point) <= Precision::Confusion()) {
    return nullptr;
  }
  const double step = std::min(1.0e-3, 1.0e-3 * cylinder.Radius());
  BRepClass3d_SolidClassifier inside(
      solid.occt(), point.Translated(gp_Vec(point, foot).Normalized() * step), Precision::Confusion());
  if (inside.State() != TopAbs_IN) {
    return nullptr;
  }
  double height = 0.0;
  for (TopExp_Explorer vertex(solid.occt(), TopAbs_VERTEX); vertex.More(); vertex.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
    height = std::max(height, gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull)));
  }
  const double slope = std::tan(-angle);
  const double radius = cylinder.Radius();
  const double apex = radius / slope;
  if (!(apex < height - 1.0e-9 * std::max(1.0, height))) {
    return nullptr;
  }
  // The cone stands on the near cap, its axis the cylinder's.
  const gp_Pnt base = axis.Location().Translated(gp_Vec(pull) *
                                                 gp_Vec(axis.Location(), neutral.Location()).Dot(gp_Vec(pull)));
  BRepPrimAPI_MakeCone cone(gp_Ax2(base, pull), radius, 0.0, apex);
  cone.Build();
  if (!cone.IsDone()) {
    return nullptr;
  }
  FaceNamer namer(cone.Shape());
  for (TopExp_Explorer f(cone.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface made(TopoDS::Face(f.Current()));
    const int from = made.GetType() == GeomAbs_Plane ? near_cap : side;
    for (const std::string& name : solid.face_names(from)) {
      namer.add(f.Current(), name);
    }
  }
  return namer.shape();
}

// A taper that closes a convex profile: the sides of the prism, leaned in
// by `angle`, meet before the far end of the extrusion (a hexagon whose
// sides lean in by 85 degrees closes into a pyramid within a fraction of a
// millimetre). The solid then ends where the profile closes (an apex, or a
// ridge), as the extrusion does in the stored designs: the convex polyhedron
// the leaned sides and the near cap bound. OCCT's draft cannot build it (it
// fails, or loops when the faces run past their meeting). Null when the
// solid is not a convex prism with plane sides, when the taper widens, or
// when the profile does not close within the extrusion: the draft takes
// those.
ShapePtr closing_taper(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                       const gp_Dir& pull, double angle) {
  if (!(angle < 0.0)) {
    return nullptr;
  }
  const double slope = std::tan(-angle);
  const gp_XYZ origin = neutral.Location().XYZ();
  const gp_XYZ along = pull.XYZ();
  const Bnd_Box box = bounds_of(solid.occt());
  const double scale = std::max(1.0, diagonal(box));
  const double tolerance = 1.0e-9 * scale;

  struct Side {
    int face;
    gp_XYZ normal;  // outwards, unit
    gp_XYZ through; // a point of the face's plane
  };
  std::vector<Side> sides;
  int near_cap = -1;
  double near_height = std::numeric_limits<double>::infinity();
  for (int i = 0; i < solid.face_count(); ++i) {
    const TopoDS_Face& face = solid.face(i);
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      return nullptr;
    }
    const gp_Pln plane = surface.Plane();
    gp_XYZ normal = plane.Axis().Direction().XYZ();
    if (face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    const gp_XYZ through = plane.Location().XYZ();
    if (leaned[static_cast<std::size_t>(i)]) {
      if (std::abs(normal.Dot(along)) > 1.0e-6) {
        return nullptr;
      }
      sides.push_back({i, normal, through});
    } else {
      // A cap: across the pull; the one next to the neutral plane stays.
      if (std::abs(std::abs(normal.Dot(along)) - 1.0) > 1.0e-6) {
        return nullptr;
      }
      const double height = std::abs((through - origin).Dot(along));
      if (height < near_height) {
        near_height = height;
        near_cap = i;
      }
    }
  }
  if (sides.size() < 3 || near_cap < 0 || near_height > tolerance) {
    return nullptr;
  }
  // A convex prism: every vertex on the inner side of every side's plane,
  // no two sides in one plane. How far the extrusion runs along the pull.
  double height = 0.0;
  for (TopExp_Explorer v(solid.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_XYZ p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).XYZ();
    height = std::max(height, (p - origin).Dot(along));
    for (const Side& side : sides) {
      if (side.normal.Dot(p - side.through) > 1.0e-7 * scale) {
        return nullptr;
      }
    }
  }
  for (std::size_t i = 0; i < sides.size(); ++i) {
    for (std::size_t j = i + 1; j < sides.size(); ++j) {
      if (sides[i].normal.Dot(sides[j].normal) > 1.0 - 1.0e-9 &&
          std::abs(sides[i].normal.Dot(sides[j].through - sides[i].through)) < 1.0e-7 * scale) {
        return nullptr;
      }
    }
  }
  if (!(height > tolerance)) {
    return nullptr;
  }

  // The sides moved in by slope * (distance along the pull) are the
  // half-spaces the solid lies in.
  std::vector<HalfSpace> leans;
  for (const Side& side : sides) {
    const gp_XYZ n = side.normal + along * slope;
    leans.push_back({n, -side.normal.Dot(side.through) - slope * along.Dot(origin)});
  }
  const HalfSpace from_start{along.Reversed(), along.Dot(origin)};
  const HalfSpace to_end{along, -along.Dot(origin) - height};
  const double reach = 8.0 * scale;

  // Does the profile close before the far end?
  Polygon end = square_in(origin + along * height, along, reach);
  for (const HalfSpace& lean : leans) {
    end = clipped(end, lean);
  }
  if (end.size() >= 3 && polygon_area(end) > tolerance * scale) {
    return nullptr;
  }

  struct Piece {
    Polygon polygon;
    NameList names;
  };
  std::vector<Piece> pieces;
  Polygon start = square_in(origin, along, reach);
  for (const HalfSpace& lean : leans) {
    start = clipped(start, lean);
  }
  if (start.size() < 3 || !(polygon_area(start) > tolerance * scale)) {
    return nullptr;
  }
  pieces.push_back({start, solid.face_names(near_cap)});
  for (std::size_t i = 0; i < sides.size(); ++i) {
    Polygon polygon = square_in(sides[i].through, leans[i].n / leans[i].n.Modulus(), reach);
    polygon = clipped(clipped(polygon, from_start), to_end);
    for (std::size_t j = 0; j < sides.size(); ++j) {
      if (j != i) {
        polygon = clipped(polygon, leans[j]);
      }
    }
    if (polygon.size() >= 3 && polygon_area(polygon) > tolerance * scale) {
      pieces.push_back({polygon, solid.face_names(sides[i].face)});
    }
  }
  if (pieces.size() < 4) {
    return nullptr;
  }

  // Corners shared by several faces come out of different intersections
  // alike to rounding: make them one point, drop repeats.
  std::vector<gp_XYZ> corners;
  for (Piece& piece : pieces) {
    Polygon unique;
    for (const gp_XYZ& p : piece.polygon) {
      gp_XYZ chosen = p;
      for (const gp_XYZ& known : corners) {
        if ((known - p).Modulus() <= 100.0 * tolerance) {
          chosen = known;
          break;
        }
      }
      if (chosen.IsEqual(p, 0.0)) {
        corners.push_back(p);
      }
      if (unique.empty() || !unique.back().IsEqual(chosen, 0.0)) {
        unique.push_back(chosen);
      }
    }
    while (unique.size() > 1 && unique.front().IsEqual(unique.back(), 0.0)) {
      unique.pop_back();
    }
    piece.polygon = unique;
  }

  BRepBuilderAPI_Sewing sewing(Precision::Confusion());
  std::vector<TopoDS_Face> faces;
  for (const Piece& piece : pieces) {
    if (piece.polygon.size() < 3) {
      return nullptr;
    }
    BRepBuilderAPI_MakePolygon outline;
    for (const gp_XYZ& p : piece.polygon) {
      outline.Add(gp_Pnt(p));
    }
    outline.Close();
    BRepBuilderAPI_MakeFace face(outline.Wire(), true);
    if (!face.IsDone()) {
      return nullptr;
    }
    faces.push_back(face.Face());
    sewing.Add(face.Face());
  }
  sewing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
    shell = TopoDS::Shell(it.Current());
  }
  if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
    return nullptr;
  }
  BRepBuilderAPI_MakeSolid make(shell);
  TopoDS_Solid closed = make.Solid();
  BRepLib::OrientClosedSolid(closed);

  FaceNamer namer(closed);
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    const TopoDS_Shape image = sewing.IsModified(faces[i]) ? sewing.Modified(faces[i]) : faces[i];
    for (const std::string& name : pieces[i].names) {
      namer.add(image, name);
    }
  }
  return namer.shape();
}

// The draft of the leaned faces (below; a closing cone or convex prism
// built as it closes).
ShapePtr tapered_draft(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                       const gp_Dir& pull, double angle);

// The near cap of a tapered prism: the face that is no side and lies on the
// neutral plane; -1 when there is none.
int near_cap_of(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                const gp_Dir& pull) {
  int near = -1;
  double near_height = std::numeric_limits<double>::infinity();
  for (int i = 0; i < solid.face_count(); ++i) {
    if (leaned[static_cast<std::size_t>(i)]) {
      continue;
    }
    const BRepAdaptor_Surface surface(solid.face(i));
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const double height = std::abs(gp_Vec(neutral.Location(), surface.Plane().Location()).Dot(gp_Vec(pull)));
    if (height < near_height) {
      near_height = height;
      near = i;
    }
  }
  const double scale = std::max(1.0, diagonal(bounds_of(solid.occt())));
  return near_height <= 1.0e-9 * scale ? near : -1;
}

// A taper of a profile with holes, as its outline's taper less its holes'
// (mitcad#235): every hole leans the other way, so that where the outline
// narrows the holes widen. The draft leans the walls of a ring into each
// other until they cross (a 50 degree taper closes a ring of 1 mm within
// half a millimetre) and gives an invalid solid; as the difference, the ring
// ends where its walls meet, a ridge, as the extrusion does in the stored
// designs. Null when the near cap has no holes.
ShapePtr holed_taper(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                     const gp_Dir& pull, double angle, const NameList& caps) {
  const int near = near_cap_of(solid, leaned, neutral, pull);
  if (near < 0) {
    return nullptr;
  }
  const TopoDS_Face& cap = solid.face(near);
  const TopoDS_Wire outer = BRepTools::OuterWire(cap);
  std::vector<TopoDS_Wire> holes;
  for (TopExp_Explorer w(cap, TopAbs_WIRE); w.More(); w.Next()) {
    if (!w.Current().IsSame(outer)) {
      holes.push_back(TopoDS::Wire(w.Current()));
    }
  }
  if (outer.IsNull() || holes.empty()) {
    return nullptr;
  }
  double height = 0.0;
  for (TopExp_Explorer v(solid.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    height = std::max(height, gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull)));
  }
  if (!(height > Precision::Confusion())) {
    return nullptr;
  }
  // The names of the side face each edge of the cap sweeps, and of the caps.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
      faces_of;
  TopExp::MapShapesAndAncestors(solid.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  const auto names_of = [&solid](const TopoDS_Shape& face) {
    const int index = solid.face_index(face);
    return index >= 0 ? solid.face_names(index) : NameList{};
  };
  NameList near_names = solid.face_names(near);
  NameList far_names;
  for (int i = 0; i < solid.face_count(); ++i) {
    if (i != near && !leaned[static_cast<std::size_t>(i)]) {
      const NameList& names = solid.face_names(i);
      far_names.insert(far_names.end(), names.begin(), names.end());
    }
  }
  const gp_Pln plane = BRepAdaptor_Surface(cap).Plane();
  // The prism of one loop as a region of its own, its sides named as the
  // solid's.
  const auto prism_of = [&](const TopoDS_Wire& loop) {
    TopoDS_Wire wire = loop;
    BRepBuilderAPI_MakeFace made(plane, wire, true);
    if (!made.IsDone()) {
      throw std::runtime_error("a loop of the tapered profile does not bound a region");
    }
    TopoDS_Face face = made.Face();
    if (BRepTopAdaptor_FClass2d(face, Precision::Confusion()).PerformInfinitePoint() == TopAbs_IN) {
      wire = TopoDS::Wire(loop.Reversed());
      face = BRepBuilderAPI_MakeFace(plane, wire, true).Face();
    }
    BRepPrimAPI_MakePrism maker(face, gp_Vec(pull) * height);
    maker.Build();
    if (!maker.IsDone()) {
      throw std::runtime_error("the extrusion of a profile failed");
    }
    FaceNamer namer(maker.Shape());
    for (TopExp_Explorer e(wire, TopAbs_EDGE); e.More(); e.Next()) {
      const int index = faces_of.FindIndex(e.Current());
      if (index == 0) {
        continue;
      }
      for (const TopoDS_Shape& side : faces_of.FindFromIndex(index)) {
        if (side.IsSame(cap)) {
          continue;
        }
        for (const std::string& name : names_of(side)) {
          namer.generated(maker, e.Current(), name);
        }
      }
    }
    for (const std::string& name : near_names) {
      namer.add(maker.FirstShape(), name);
    }
    for (const std::string& name : far_names) {
      namer.add(maker.LastShape(), name);
    }
    return namer.shape();
  };
  const ShapePtr outline = tapered(*prism_of(outer), neutral, pull, angle, caps);
  std::vector<ShapePtr> leaned_holes;
  for (const TopoDS_Wire& hole : holes) {
    leaned_holes.push_back(tapered(*prism_of(hole), neutral, pull, -angle, caps));
  }
  const ShapePtr result = with_holes(outline, leaned_holes, "the taper");
  GProp_GProps volume;
  BRepGProp::VolumeProperties(result->occt(), volume);
  if (!(volume.Mass() > Precision::Confusion())) {
    throw std::invalid_argument("the taper closes the profile at its start");
  }
  return result;
}

// A length or an angle in words: up to three decimals, no trailing zeros.
std::string number_text(const double value) {
  char text[64];
  std::snprintf(text, sizeof text, "%.3f", value);
  std::string out = text;
  while (!out.empty() && out.back() == '0') {
    out.pop_back();
  }
  if (!out.empty() && out.back() == '.') {
    out.pop_back();
  }
  return out == "-0" ? "0" : out;
}

// Where a taper closes or splits the profile it leans (mitcad#1186): the
// profile's walls move into the profile (out of it for a taper that
// widens) by t tan(angle) at height t above the near cap, as the profile
// offset in its plane by that distance (OCCT's offset of the cap face,
// joined as the draft's planes meet). Its first change of shape on the way
// up is where the taper cannot go on: two walls not next to each other
// meeting (the profile splits there, or a notch closes) or a wall shrinking
// to nothing. `height` is how far the extrusion runs above the near cap.
struct TaperClosure {
  double at = 0.0;                 // the height where it happens
  std::vector<std::string> walls;  // the walls that meet (in pairs) or vanish there
  double gap = 0.0;                // how far apart the meeting walls are on the cap
  bool meet = false;               // walls meet (else they vanish)
};

std::optional<TaperClosure> taper_closure(const Shape& solid, const std::vector<bool>& leaned,
                                          const gp_Pln& neutral, const gp_Dir& pull, double angle) {
  const int near = near_cap_of(solid, leaned, neutral, pull);
  if (near < 0 || std::abs(angle) < 1.0e-9) {
    return std::nullopt;
  }
  const TopoDS_Face cap = TopoDS::Face(solid.face(near).Oriented(TopAbs_FORWARD));
  double height = 0.0;
  for (TopExp_Explorer v(solid.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    height = std::max(height, gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull)));
  }
  if (!(height > Precision::Confusion())) {
    return std::nullopt;
  }
  // The cap's edges and the wall each one leans.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(solid.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<TopoDS_Edge> edges;
  std::vector<std::string> walls;
  int wires = 0;
  for (TopExp_Explorer w(cap, TopAbs_WIRE); w.More(); w.Next()) {
    ++wires;
  }
  for (TopExp_Explorer e(cap, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    std::string wall;
    const int index = faces_of.FindIndex(edge);
    if (index > 0) {
      for (const TopoDS_Shape& face : faces_of.FindFromIndex(index)) {
        const int f = solid.face_index(face);
        if (f >= 0 && f != near && leaned[static_cast<std::size_t>(f)] && !solid.face_names(f).empty()) {
          wall = solid.face_names(f).front();
        }
      }
    }
    edges.push_back(edge);
    walls.push_back(wall.empty() ? "an unnamed wall" : wall);
  }
  // The profile offset by d (out of it where positive): whether it is the
  // cap's shape still, and the edges that vanished.
  const auto changed = [&](const double d, std::vector<std::size_t>* vanished) {
    BRepOffsetAPI_MakeOffset offset(cap, GeomAbs_Intersection);
    try {
      OCC_CATCH_SIGNALS
      offset.Perform(d);
    } catch (const Standard_Failure&) {
      return true;
    }
    if (!offset.IsDone() || offset.Shape().IsNull()) {
      return true;
    }
    int count = 0;
    for (TopExp_Explorer w(offset.Shape(), TopAbs_WIRE); w.More(); w.Next()) {
      ++count;
    }
    bool gone = false;
    for (std::size_t i = 0; i < edges.size(); ++i) {
      if (offset.Generated(edges[i]).IsEmpty()) {
        gone = true;
        if (vanished != nullptr) {
          vanished->push_back(i);
        }
      }
    }
    return gone || count != wires;
  };
  // The walls move out of the profile by t tan(angle) at height t.
  const double slope = std::tan(angle);
  const double end = height * slope;
  if (!changed(end, nullptr)) {
    return std::nullopt;
  }
  // The first change on the way up, sampled (the offset itself is unsteady
  // right at a change: it may fail there, or split the profile a moment).
  constexpr int kSamples = 64;
  double below = 0.0;
  double above = end;
  for (int k = 1; k <= kSamples; ++k) {
    const double d = end * k / kSamples;
    if (changed(d, nullptr)) {
      above = d;
      break;
    }
    below = d;
  }
  TaperClosure closure;
  // Walls that meet there: two edges of the cap facing each other across
  // the profile (out of it, for a taper that widens), their nearest points
  // inside both edges, half their distance between the offsets sampled on
  // either side of the change.
  // (A sample's width either way: the offset may split the profile a
  // moment before its walls' true meeting, or fail a moment after.)
  const double window = std::abs(end) / kSamples;
  double first_meet = std::numeric_limits<double>::infinity();
  std::vector<std::size_t> named;
  for (std::size_t i = 0; i < edges.size(); ++i) {
    for (std::size_t j = i + 1; j < edges.size(); ++j) {
      BRepExtrema_DistShapeShape distance(edges[i], edges[j]);
      if (!distance.IsDone() || distance.NbSolution() == 0) {
        continue;
      }
      const double gap = distance.Value();
      const double half = 0.5 * gap;
      if (half < std::abs(below) - window || half > std::abs(above) + window) {
        continue;
      }
      bool facing = false;
      for (int n = 1; n <= distance.NbSolution() && !facing; ++n) {
        // (Square to both edges where their nearest points are: walls that
        // face each other. Edges of a convex corner nearest at its vertices
        // do not.)
        const gp_Pnt p1 = distance.PointOnShape1(n), p2 = distance.PointOnShape2(n);
        const gp_Vec across(p1, p2);
        bool square = across.Magnitude() > Precision::Confusion();
        for (int k = 0; k < 2 && square; ++k) {
          const TopoDS_Edge& e = k == 0 ? edges[i] : edges[j];
          const BRepExtrema_SupportType type = k == 0 ? distance.SupportTypeShape1(n) : distance.SupportTypeShape2(n);
          double t = 0.0;
          if (type == BRepExtrema_IsOnEdge) {
            if (k == 0) {
              distance.ParOnEdgeS1(n, t);
            } else {
              distance.ParOnEdgeS2(n, t);
            }
          } else if (type == BRepExtrema_IsVertex) {
            t = BRep_Tool::Parameter(TopoDS::Vertex(k == 0 ? distance.SupportOnShape1(n) : distance.SupportOnShape2(n)), e);
          } else {
            square = false;
            break;
          }
          gp_Pnt at;
          gp_Vec tangent;
          BRepAdaptor_Curve(e).D1(t, at, tangent);
          square = tangent.Magnitude() > gp::Resolution() &&
                   std::abs(tangent.Normalized().Dot(across.Normalized())) < 1.0e-6;
        }
        if (!square) {
          continue;
        }
        const gp_Pnt middle((p1.XYZ() + p2.XYZ()) / 2.0);
        BRepClass_FaceClassifier inside(cap, middle, Precision::Confusion());
        facing = gap > Precision::Confusion() &&
                 (end < 0.0 ? inside.State() == TopAbs_IN : inside.State() == TopAbs_OUT);
      }
      if (!facing) {
        continue;
      }
      if (gap < first_meet - 1.0e-9 * height) {
        first_meet = gap;
        named.clear();
      }
      if (std::abs(gap - first_meet) <= 1.0e-9 * height) {
        named.push_back(i);
        named.push_back(j);
      }
    }
  }
  if (!named.empty()) {
    closure.meet = true;
    closure.gap = first_meet;
    closure.at = 0.5 * first_meet / std::abs(slope);
  } else {
    // Walls that shrink to nothing: gone on the change's far side; where
    // the first of them goes, halved down to a millionth of the height.
    std::vector<std::size_t> gone;
    for (double d = above; gone.empty() && std::abs(d) <= std::abs(end) * (1.0 + 1.0e-9); d += end / kSamples) {
      changed(d, &gone);
      above = d;
    }
    if (gone.empty()) {
      return std::nullopt;
    }
    named = gone;
    const auto vanished = [&](const double d) {
      std::vector<std::size_t> now;
      changed(d, &now);
      return std::any_of(gone.begin(), gone.end(), [&now](const std::size_t i) {
        return std::find(now.begin(), now.end(), i) != now.end();
      });
    };
    double lo = below;
    double hi = above;
    while (std::abs(hi - lo) > 1.0e-6 * std::abs(end)) {
      const double mid = 0.5 * (lo + hi);
      (vanished(mid) ? hi : lo) = mid;
    }
    closure.at = std::abs(hi / slope);
  }
  if (closure.meet) {
    // Pairs, each once.
    for (std::size_t k = 0; k + 1 < named.size(); k += 2) {
      const std::string& a = walls[named[k]];
      const std::string& b = walls[named[k + 1]];
      bool known = false;
      for (std::size_t m = 0; m + 1 < closure.walls.size() && !known; m += 2) {
        known = (closure.walls[m] == a && closure.walls[m + 1] == b) || (closure.walls[m] == b && closure.walls[m + 1] == a);
      }
      if (!known && a != b) {
        closure.walls.push_back(a);
        closure.walls.push_back(b);
      }
    }
  } else {
    for (const std::size_t i : named) {
      if (std::find(closure.walls.begin(), closure.walls.end(), walls[i]) == closure.walls.end()) {
        closure.walls.push_back(walls[i]);
      }
    }
  }
  return closure;
}

// The error of a taper the draft could not build: where and how it closes
// the profile when it does within the extrusion, else the kernel's own
// failure with what was asked.
std::string taper_failure(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                          const gp_Dir& pull, double angle, const std::string& kernel) {
  double height = 0.0;
  for (TopExp_Explorer v(solid.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    height = std::max(height, gp_Vec(neutral.Location(), p).Dot(gp_Vec(pull)));
  }
  const std::string degrees = number_text(std::abs(angle) * 180.0 / 3.14159265358979323846);
  std::optional<TaperClosure> closure;
  try {
    OCC_CATCH_SIGNALS
    closure = taper_closure(solid, leaned, neutral, pull, angle);
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    closure.reset();
  }
  if (!closure || closure->walls.empty()) {
    return "the taper failed: the geometry kernel could not lean the profile's walls by " + degrees +
           " degrees over " + number_text(height) + " mm (" + kernel + ")";
  }
  std::string walls;
  if (closure->meet) {
    // "A and B", "A and B; C and D", up to three pairs.
    const std::size_t pairs = closure->walls.size() / 2;
    for (std::size_t k = 0; k < pairs && k < 3; ++k) {
      walls += (k == 0 ? "" : "; ") + closure->walls[2 * k] + " and " + closure->walls[2 * k + 1];
    }
    if (pairs > 3) {
      walls += "; and " + std::to_string(pairs - 3) + (pairs == 4 ? " more pair" : " more pairs");
    }
  } else {
    for (std::size_t i = 0; i < closure->walls.size() && i < 4; ++i) {
      walls += (i == 0 ? "" : i + 1 == closure->walls.size() ? " and " : ", ") + closure->walls[i];
    }
    if (closure->walls.size() > 4) {
      walls += " and " + std::to_string(closure->walls.size() - 4) + " more";
    }
  }
  const std::string where = number_text(closure->at) + " mm from the sketch plane, before the extrusion's end " +
                            number_text(height) + " mm from it";
  const std::string outcome =
      angle < 0.0 ? "the taper closes the profile there" : "the widening taper makes the profile overlap itself there";
  if (closure->meet) {
    return "the taper failed: leaning by " + degrees + " degrees, the walls " + walls + ", " +
           number_text(closure->gap) + " mm apart in the profile, meet " + where + ": " + outcome;
  }
  return "the taper failed: leaning by " + degrees + " degrees, " +
         (closure->walls.size() == 1 ? "the wall " : "the walls ") + walls + " shrink" +
         (closure->walls.size() == 1 ? "s" : "") + " to nothing " + where + ": " + outcome;
}

// A point in the plane across the pull.
struct Flat {
  double x = 0.0;
  double y = 0.0;
};

double cross_of(const Flat& o, const Flat& a, const Flat& b) {
  return (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
}

double to_segment(const Flat& p, const Flat& a, const Flat& b) {
  const double dx = b.x - a.x;
  const double dy = b.y - a.y;
  const double square = dx * dx + dy * dy;
  const double t = square > 0.0 ? std::clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / square, 0.0, 1.0) : 0.0;
  return std::hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

// Whether the segments ab and cd neither cross nor come within `gap`.
bool segments_apart(const Flat& a, const Flat& b, const Flat& c, const Flat& d, double gap) {
  const double c1 = cross_of(a, b, c);
  const double c2 = cross_of(a, b, d);
  const double c3 = cross_of(c, d, a);
  const double c4 = cross_of(c, d, b);
  if (((c1 > 0.0 && c2 < 0.0) || (c1 < 0.0 && c2 > 0.0)) && ((c3 > 0.0 && c4 < 0.0) || (c3 < 0.0 && c4 > 0.0))) {
    return false;
  }
  return std::min({to_segment(a, c, d), to_segment(b, c, d), to_segment(c, a, b), to_segment(d, a, b)}) > gap;
}

// The taper of a prism of lines built from the leaned planes themselves
// (mitcad#1483): each corner of the near cap runs up the line where the
// leaned planes of its two sides meet, so the far cap is the near cap's
// loop offset by height * tan(angle) with mitred corners, and each side is
// the quadrilateral between a line of the one and of the other. OCCT's draft
// can build such a prism invalid although nothing closes: a band of 52 short
// lines, nearly in line at its corners, came out invalid from 5 degrees on.
// Null unless the solid is the prism of one loop of lines on the neutral
// plane between two caps across the pull, and the loop stays a simple
// polygon all the way up, no line shrinking to nothing or turning round: a
// taper that closes or folds the profile stays the draft's, and is
// explained by taper_failure.
ShapePtr planar_taper(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                      const gp_Dir& pull, double angle) {
  const int near = near_cap_of(solid, leaned, neutral, pull);
  if (near < 0) {
    return nullptr;
  }
  const TopoDS_Face& cap = solid.face(near);
  int loops = 0;
  for (TopExp_Explorer w(cap, TopAbs_WIRE); w.More(); w.Next()) {
    ++loops;
  }
  const TopoDS_Wire outer = BRepTools::OuterWire(cap);
  if (loops != 1 || outer.IsNull()) {
    return nullptr;
  }
  const gp_XYZ origin = neutral.Location().XYZ();
  const gp_XYZ along = pull.XYZ();
  const double scale = std::max(1.0, diagonal(bounds_of(solid.occt())));
  const double tolerance = 1.0e-7 * scale;
  // The far cap across the pull; every other face a plane along it.
  int far_cap = -1;
  int sides = 0;
  for (int i = 0; i < solid.face_count(); ++i) {
    if (i == near) {
      continue;
    }
    const BRepAdaptor_Surface surface(solid.face(i));
    if (surface.GetType() != GeomAbs_Plane) {
      return nullptr;
    }
    const double across = std::abs(surface.Plane().Axis().Direction().XYZ().Dot(along));
    if (leaned[static_cast<std::size_t>(i)]) {
      if (across > 1.0e-9) {
        return nullptr;
      }
      ++sides;
    } else {
      if (far_cap >= 0 || std::abs(across - 1.0) > 1.0e-9) {
        return nullptr;
      }
      far_cap = i;
    }
  }
  if (far_cap < 0 || sides < 3) {
    return nullptr;
  }
  const double height = (BRepAdaptor_Surface(solid.face(far_cap)).Plane().Location().XYZ() - origin).Dot(along);
  if (!(height > tolerance)) {
    return nullptr;
  }
  // The near cap's corners in the loop's order, and the side each line
  // from a corner to the next one bounds.
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndUniqueAncestors(solid.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<gp_XYZ> corners;
  std::vector<int> walls;
  for (BRepTools_WireExplorer e(outer, cap); e.More(); e.Next()) {
    const TopoDS_Edge& edge = e.Current();
    if (BRep_Tool::Degenerated(edge) || BRepAdaptor_Curve(edge).GetType() != GeomAbs_Line) {
      return nullptr;
    }
    int wall = -1;
    const int index = faces_of.FindIndex(edge);
    if (index > 0) {
      for (const TopoDS_Shape& face : faces_of.FindFromIndex(index)) {
        const int f = solid.face_index(face);
        if (f >= 0 && f != near && leaned[static_cast<std::size_t>(f)]) {
          wall = f;
        }
      }
    }
    if (wall < 0) {
      return nullptr;
    }
    corners.push_back(BRep_Tool::Pnt(e.CurrentVertex()).XYZ());
    walls.push_back(wall);
  }
  const std::size_t count = corners.size();
  std::vector<int> distinct = walls;
  std::sort(distinct.begin(), distinct.end());
  if (count < 3 || static_cast<int>(count) != sides ||
      std::adjacent_find(distinct.begin(), distinct.end()) != distinct.end()) {
    return nullptr;
  }
  const gp_Ax3 frame(neutral.Location(), pull);
  const gp_XYZ ex = frame.XDirection().XYZ();
  const gp_XYZ ey = frame.YDirection().XYZ();
  const auto flat = [&](const gp_XYZ& p) { return Flat{(p - origin).Dot(ex), (p - origin).Dot(ey)}; };
  std::vector<Flat> base(count);
  double twice = 0.0;
  for (std::size_t k = 0; k < count; ++k) {
    base[k] = flat(corners[k]);
  }
  for (std::size_t k = 0; k < count; ++k) {
    twice += cross_of(Flat{}, base[k], base[(k + 1) % count]);
  }
  if (!(std::abs(twice) > tolerance * scale)) {
    return nullptr;
  }
  // The loop counter-clockwise about the pull, or not: the outward normal
  // of each line, and the way each corner runs as its sides lean out by
  // one (along the sum of its sides' normals, (n1 + n2) / (1 + n1.n2)).
  const double sense = twice > 0.0 ? 1.0 : -1.0;
  std::vector<gp_XYZ> normals(count);
  for (std::size_t k = 0; k < count; ++k) {
    const gp_XYZ line = corners[(k + 1) % count] - corners[k];
    if (!(line.Modulus() > tolerance)) {
      return nullptr;
    }
    normals[k] = line.Crossed(along).Normalized() * sense;
  }
  std::vector<gp_XYZ> mitres(count);
  for (std::size_t k = 0; k < count; ++k) {
    const gp_XYZ& before = normals[(k + count - 1) % count];
    const gp_XYZ& after = normals[k];
    const double turn = 1.0 + before.Dot(after);
    if (!(turn > 1.0e-6)) {
      return nullptr;
    }
    mitres[k] = (before + after) / turn;
  }
  const double slope = std::tan(angle);
  const auto at = [&](const std::size_t k, const double s) {
    return corners[k] + along * s + mitres[k] * (s * slope);
  };
  // The loop at heights up the prism: every line keeping its way and some
  // length, lines not next to each other apart, the area on the same side.
  constexpr int kSteps = 16;
  std::vector<Flat> loop(count);
  for (int step = 1; step <= kSteps; ++step) {
    const double s = height * step / kSteps;
    for (std::size_t k = 0; k < count; ++k) {
      loop[k] = flat(at(k, s));
    }
    double area = 0.0;
    for (std::size_t k = 0; k < count; ++k) {
      const std::size_t next = (k + 1) % count;
      area += cross_of(Flat{}, loop[k], loop[next]);
      const double dx = base[next].x - base[k].x;
      const double dy = base[next].y - base[k].y;
      const double along_line = (loop[next].x - loop[k].x) * dx + (loop[next].y - loop[k].y) * dy;
      if (!(along_line > tolerance * std::hypot(dx, dy))) {
        return nullptr;
      }
    }
    if (!(area * sense > tolerance * scale)) {
      return nullptr;
    }
    for (std::size_t i = 0; i < count; ++i) {
      for (std::size_t j = i + 2; j < count; ++j) {
        // (The last line and the first meet at the first corner.)
        if (i == 0 && j + 1 == count) {
          continue;
        }
        if (!segments_apart(loop[i], loop[i + 1], loop[j], loop[(j + 1) % count], tolerance)) {
          return nullptr;
        }
      }
    }
  }
  // The caps and the sides as planar polygons on the shared corners, sewn.
  struct Piece {
    std::vector<gp_Pnt> outline;
    int from;
  };
  std::vector<gp_Pnt> low(count);
  std::vector<gp_Pnt> high(count);
  for (std::size_t k = 0; k < count; ++k) {
    low[k] = gp_Pnt(corners[k]);
    high[k] = gp_Pnt(at(k, height));
  }
  std::vector<Piece> pieces;
  pieces.push_back({low, near});
  pieces.push_back({high, far_cap});
  for (std::size_t k = 0; k < count; ++k) {
    const std::size_t next = (k + 1) % count;
    pieces.push_back({{low[k], low[next], high[next], high[k]}, walls[k]});
  }
  BRepBuilderAPI_Sewing sewing(Precision::Confusion());
  std::vector<TopoDS_Face> faces;
  for (const Piece& piece : pieces) {
    BRepBuilderAPI_MakePolygon outline;
    for (const gp_Pnt& p : piece.outline) {
      outline.Add(p);
    }
    outline.Close();
    if (!outline.IsDone()) {
      return nullptr;
    }
    BRepBuilderAPI_MakeFace face(outline.Wire(), true);
    if (!face.IsDone()) {
      return nullptr;
    }
    faces.push_back(face.Face());
    sewing.Add(face.Face());
  }
  sewing.Perform();
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
    shell = TopoDS::Shell(it.Current());
  }
  if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
    return nullptr;
  }
  BRepBuilderAPI_MakeSolid make(shell);
  if (!make.IsDone()) {
    return nullptr;
  }
  TopoDS_Solid made = make.Solid();
  if (!BRepLib::OrientClosedSolid(made)) {
    return nullptr;
  }
  FaceNamer namer(made);
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    const TopoDS_Shape image = sewing.IsModified(faces[i]) ? sewing.Modified(faces[i]) : faces[i];
    for (const std::string& name : solid.face_names(pieces[i].from)) {
      namer.add(image, name);
    }
  }
  return namer.shape();
}

// planar_taper's solid where it builds one and the checker finds it valid.
ShapePtr valid_planar_taper(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                            const gp_Dir& pull, double angle) {
  ShapePtr built;
  try {
    OCC_CATCH_SIGNALS
    built = planar_taper(solid, leaned, neutral, pull, angle);
  } catch (const Standard_Failure&) {
    return nullptr;
  }
  return built && !built->occt().IsNull() && is_valid(built->occt()) ? built : nullptr;
}

} // namespace

ShapePtr tapered(const Shape& solid, const gp_Pln& neutral, const gp_Dir& pull, double angle,
                 const NameList& caps) {
  std::vector<bool> leaned(static_cast<std::size_t>(solid.face_count()), false);
  for (int i = 0; i < solid.face_count(); ++i) {
    const NameList& names = solid.face_names(i);
    leaned[static_cast<std::size_t>(i)] =
        std::none_of(caps.begin(), caps.end(), [&names](const std::string& name) {
          return std::find(names.begin(), names.end(), name) != names.end();
        });
  }
  // A profile with holes: the draft, else (where it fails, or its walls
  // cross) the outline's taper less the holes'.
  const int near = near_cap_of(solid, leaned, neutral, pull);
  int loops = 0;
  if (near >= 0) {
    for (TopExp_Explorer w(solid.face(near), TopAbs_WIRE); w.More(); w.Next()) {
      ++loops;
    }
  }
  if (loops < 2) {
    // (A draft that fails, or whose walls cross into an invalid solid, is
    // explained by where the taper closes the profile, when it does; a
    // draft valid or not explained so is as before. A prism of lines that
    // the taper neither closes nor folds is built from its leaned planes
    // where the draft fails or comes out invalid, mitcad#1483.)
    ShapePtr drafted;
    try {
      drafted = tapered_draft(solid, leaned, neutral, pull, angle);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::runtime_error& error) {
      if (std::string(error.what()).find("out of memory") != std::string::npos) {
        throw;
      }
      if (ShapePtr planar = valid_planar_taper(solid, leaned, neutral, pull, angle)) {
        return planar;
      }
      throw std::runtime_error(taper_failure(solid, leaned, neutral, pull, angle, error.what()));
    }
    if (!drafted->occt().IsNull() && !is_valid(drafted->occt())) {
      if (ShapePtr planar = valid_planar_taper(solid, leaned, neutral, pull, angle)) {
        return planar;
      }
      std::optional<TaperClosure> closure;
      try {
        OCC_CATCH_SIGNALS
        closure = taper_closure(solid, leaned, neutral, pull, angle);
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
      }
      if (closure && !closure->walls.empty()) {
        throw std::runtime_error(
            taper_failure(solid, leaned, neutral, pull, angle, "the taper produced an invalid shape"));
      }
    }
    return drafted;
  }
  try {
    ShapePtr drafted = tapered_draft(solid, leaned, neutral, pull, angle);
    if (!drafted->occt().IsNull() && is_valid(drafted->occt())) {
      return drafted;
    }
  } catch (const Cancelled&) {
    throw;
  } catch (const std::invalid_argument&) {
  } catch (const std::runtime_error&) {
  }
  if (ShapePtr holed = holed_taper(solid, leaned, neutral, pull, angle, caps)) {
    return holed;
  }
  throw std::runtime_error(
      taper_failure(solid, leaned, neutral, pull, angle, "the taper failed; the angle may close the profile"));
}

namespace {

ShapePtr tapered_draft(const Shape& solid, const std::vector<bool>& leaned, const gp_Pln& neutral,
                       const gp_Dir& pull, double angle) {
  if (ShapePtr closed = closing_cone(solid, leaned, neutral, pull, angle)) {
    return closed;
  }
  if (ShapePtr closed = closing_taper(solid, leaned, neutral, pull, angle)) {
    return closed;
  }
  check_cone_apexes(solid, leaned, neutral, pull, angle);
  BRepOffsetAPI_DraftAngle draft(solid.occt());
  for (int i = 0; i < solid.face_count(); ++i) {
    if (!leaned[static_cast<std::size_t>(i)]) {
      continue;
    }
    const NameList& names = solid.face_names(i);
    // OCCT leans a face into the material for a positive angle.
    draft.Add(solid.face(i), pull, -angle, neutral);
    if (!draft.AddDone()) {
      throw std::invalid_argument("the taper cannot be applied to " +
                                  (names.empty() ? std::string("a side face") : names.front()) +
                                  "; tapers need straight lines and arcs");
    }
  }
  detail::build(draft);
  if (!draft.IsDone()) {
    throw std::runtime_error("the taper failed; the angle may close the profile");
  }
  // The draft's Modified() does not list the leaned faces; ModifiedShape does.
  FaceNamer namer(draft.Shape());
  for (int i = 0; i < solid.face_count(); ++i) {
    const TopoDS_Shape image = draft.ModifiedShape(solid.face(i));
    for (const std::string& name : solid.face_names(i)) {
      namer.add(image, name);
    }
  }
  return namer.shape();
}

} // namespace

std::vector<TopoDS_Face> target_faces(const Target& target) {
  if (!target.body) {
    throw std::invalid_argument("the target has no body");
  }
  std::vector<TopoDS_Face> faces;
  for (const int i : target.body->find_faces(target.face)) {
    faces.push_back(target.body->face(i));
  }
  if (faces.empty()) {
    throw std::invalid_argument("the body has no face " + target.face);
  }
  return faces;
}

gp_Pln target_plane(const Target& target, const gp_Dir& along) {
  gp_Pln plane = target.plane;
  if (target.kind == Target::Kind::Face) {
    const std::vector<TopoDS_Face> faces = target_faces(target);
    const BRepAdaptor_Surface surface(faces.front());
    if (surface.GetType() != GeomAbs_Plane) {
      throw std::invalid_argument("face " + target.face + " is not planar");
    }
    plane = surface.Plane();
  } else if (target.kind == Target::Kind::Body) {
    throw std::invalid_argument("a body is not a plane");
  }
  gp_Dir normal = plane.Axis().Direction();
  if (normal.Dot(along) < 0.0) {
    normal.Reverse();
  }
  return gp_Pln(plane.Location().Translated(gp_Vec(normal) * target.offset), normal);
}

namespace {

// A plane, or a planar face continued past its edges.
bool is_planar_target(const Target& target) {
  if (target.kind == Target::Kind::Plane) {
    return true;
  }
  if (target.kind != Target::Kind::Face || !target.extend) {
    return false;
  }
  return BRepAdaptor_Surface(target_faces(target).front()).GetType() == GeomAbs_Plane;
}

} // namespace

double reach(const Target& target, const gp_Dir& along, const Bnd_Box& start) {
  const std::array<gp_Pnt, 8> starts = corners(start);
  if (is_planar_target(target)) {
    const gp_Pln plane = target_plane(target, along);
    const gp_Dir normal = plane.Axis().Direction();
    const double slope = normal.Dot(along);
    if (slope < 1.0e-9) {
      throw std::invalid_argument("the sweep runs parallel to the target plane");
    }
    double farthest = -std::numeric_limits<double>::infinity();
    for (const gp_Pnt& corner : starts) {
      farthest = std::max(farthest, gp_Vec(corner, plane.Location()).Dot(gp_Vec(normal)) / slope);
    }
    if (farthest <= Precision::Confusion()) {
      throw std::invalid_argument("the target lies behind the start of the sweep");
    }
    return farthest * 1.05 + 1.0;
  }
  Bnd_Box box;
  if (target.kind == Target::Kind::Face) {
    for (const TopoDS_Face& face : target_faces(target)) {
      BRepBndLib::Add(face, box);
    }
  } else {
    BRepBndLib::Add(target.body->occt(), box);
  }
  double target_far = -std::numeric_limits<double>::infinity();
  for (const gp_Pnt& corner : corners(box)) {
    target_far = std::max(target_far, along_of(corner, along));
  }
  target_far += std::max(0.0, target.offset);
  double start_near = std::numeric_limits<double>::infinity();
  double start_far = -std::numeric_limits<double>::infinity();
  for (const gp_Pnt& corner : starts) {
    start_near = std::min(start_near, along_of(corner, along));
    start_far = std::max(start_far, along_of(corner, along));
  }
  if (target_far <= start_near + Precision::Confusion()) {
    throw std::invalid_argument("the target lies behind the start of the sweep");
  }
  return (target_far - start_near) + diagonal(start) + 1.0;
}

TopoDS_Face plane_tool(const gp_Pln& plane, const Bnd_Box& around) {
  return planar_face(plane, center_of(around), 4.0 * diagonal(around) + 100.0);
}

TopoDS_Shape target_tool(const Target& target, const gp_Dir& along, const Bnd_Box& around) {
  const gp_Vec shift = gp_Vec(along) * target.offset;
  switch (target.kind) {
  case Target::Kind::Plane:
    return plane_tool(target_plane(target, along), around);
  case Target::Kind::Face: {
    const std::vector<TopoDS_Face> faces = target_faces(target);
    if (target.extend) {
      if (is_planar_target(target)) {
        return plane_tool(target_plane(target, along), around);
      }
      Bnd_Box all = around;
      for (const TopoDS_Face& face : faces) {
        BRepBndLib::Add(face, all);
      }
      return translated(extended_face(faces.front(), 4.0 * diagonal(all) + 100.0), shift);
    }
    // The face, the faces next to it and the faces joined to them smoothly (the
    // chain of tangent faces).
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>
        ancestors;
    TopExp::MapShapesAndAncestors(target.body->occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
    ShapeMap chosen;
    for (const TopoDS_Face& face : faces) {
      chosen.Add(face);
      for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
        if (ancestors.Contains(e.Current())) {
          for (const TopoDS_Shape& next : ancestors.FindFromKey(e.Current())) {
            chosen.Add(next);
          }
        }
      }
    }
    std::vector<int> seeds;
    for (const TopoDS_Face& face : faces) {
      const int index = target.body->face_index(face);
      if (index >= 0) {
        seeds.push_back(index);
      }
    }
    for (const int index : tangent_closure(*target.body, seeds)) {
      chosen.Add(target.body->face(index));
    }
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    for (int i = 1; i <= chosen.Extent(); ++i) {
      builder.Add(compound, chosen(i));
    }
    return translated(compound, shift);
  }
  case Target::Kind::Body:
    return translated(target.body->occt(), shift);
  }
  throw std::invalid_argument("unknown target kind");
}

ShapePtr split_keep(const Shape& solid, const TopoDS_Shape& tool, const NameList& tool_names,
                    const std::function<bool(const Shape&)>& keep, const char* what) {
  std::vector<ShapePtr> kept;
  for (const ShapePtr& piece : split_pieces(solid, tool, tool_names, what)) {
    if (keep(*piece)) {
      kept.push_back(piece);
    }
  }
  if (kept.empty()) {
    throw std::invalid_argument(std::string(what) + ": nothing is left past the target");
  }
  return fused(kept, what);
}

namespace {

// Walls along a straight sweep through the lines where the tool crosses
// its start face (named `near`) inside it, from behind the start to past
// the end: they part the sweep into columns over the parts of the profile.
TopoDS_Compound start_walls(const Shape& sweep, const std::string& near, const TopoDS_Shape& tool,
                            const gp_Dir& along) {
  BRep_Builder builder;
  TopoDS_Compound walls;
  builder.MakeCompound(walls);
  const double size = diagonal(bounds_of(sweep.occt())) + 1.0;
  gp_Trsf back;
  back.SetTranslation(gp_Vec(along) * -size);
  for (int i = 0; i < sweep.face_count(); ++i) {
    const NameList& names = sweep.face_names(i);
    if (std::find(names.begin(), names.end(), near) == names.end()) {
      continue;
    }
    const TopoDS_Face& start = sweep.face(i);
    BRepAlgoAPI_Section section(start, tool, false);
    section.Approximation(true);
    detail::build(section);
    if (!section.IsDone() || section.HasErrors()) {
      throw std::runtime_error("the target's crossing with the profile could not be found");
    }
    for (TopExp_Explorer e(section.Shape(), TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
      // Not along the profile's own edges.
      const BRepClass_FaceClassifier inside(start, middle, Precision::Confusion());
      if (inside.State() != TopAbs_IN) {
        continue;
      }
      const TopoDS_Shape behind = BRepBuilderAPI_Transform(e.Current(), back, true).Shape();
      BRepPrimAPI_MakePrism wall(behind, gp_Vec(along) * (3.0 * size));
      wall.Build();
      if (!wall.IsDone()) {
        throw std::runtime_error("the sweep could not be parted where the target crosses the profile");
      }
      builder.Add(walls, wall.Shape());
    }
  }
  return walls;
}

// The angle (0 to 2 pi) of a point about `axis` from `reference`, right
// handed about the axis direction (mitcad#539: the order of a
// revolution's pieces).
double angle_about(const gp_Ax1& axis, const gp_Pnt& reference, const gp_Pnt& point) {
  const gp_Vec a(axis.Direction());
  gp_Vec u(axis.Location(), reference);
  u -= a * u.Dot(a);
  gp_Vec v(axis.Location(), point);
  v -= a * v.Dot(a);
  if (u.Magnitude() <= Precision::Confusion() || v.Magnitude() <= Precision::Confusion()) {
    return 0.0;
  }
  const double angle = std::atan2(u.Crossed(v).Dot(a), u.Dot(v));
  return angle < 0.0 ? angle + kTwoPi : angle;
}

// The centre of the faces of the shape named `name`.
gp_Pnt centre_of_named(const Shape& shape, const std::string& name) {
  GProp_GProps all;
  for (int i = 0; i < shape.face_count(); ++i) {
    const NameList& names = shape.face_names(i);
    if (std::find(names.begin(), names.end(), name) != names.end()) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(shape.face(i), props);
      all.Add(props);
    }
  }
  return all.CentreOfMass();
}

} // namespace

ShapePtr trimmed(const Shape& sweep, const Target& target, const TopoDS_Shape& tool,
                 const gp_Dir& along, const std::string& near, const NameList& far,
                 const std::string& unreached, bool straight, const gp_Ax1* turn) {
  std::vector<ShapePtr> pieces = split_pieces(sweep, tool, far, "the sweep to its target");
  // The column of each piece: all in one unless a profile that starts inside
  // the target is parted.
  std::vector<std::size_t> column_of(pieces.size(), 0);
  int near_faces = 0;
  for (const ShapePtr& piece : pieces) {
    near_faces += count_named(*piece, near);
  }
  if (near_faces > count_named(sweep, near)) {
    // A plane that crosses the profile leaves its end undefined. Up to a
    // body (its first face) or a curved face that the profile starts
    // inside of or on, each part of a straight sweep ends at the first
    // face beyond the profile: the sweep is parted into columns over the
    // parts first, so that a column inside the body does not take the
    // body's inside next to another column.
    const bool each_part = straight && !is_planar_target(target);
    if (!each_part) {
      throw std::invalid_argument("the target crosses the profile where the sweep starts");
    }
    const TopoDS_Compound walls = start_walls(sweep, near, tool, along);
    if (TopExp_Explorer(walls, TopAbs_FACE).More()) {
      pieces.clear();
      column_of.clear();
      std::size_t next_column = 0;
      for (const ShapePtr& column : split_pieces(sweep, walls, far, "the sweep along the profile's parts")) {
        for (const ShapePtr& piece : split_pieces(*column, tool, far, "the sweep to its target")) {
          pieces.push_back(piece);
          column_of.push_back(next_column);
        }
        ++next_column;
      }
    }
  }
  std::vector<ShapePtr> kept;
  if (target.kind == Target::Kind::Body && target.through) {
    // Up to the last piece inside the body, in the order along the sweep;
    // where the profile starts inside the body and is parted into columns,
    // each column's own.
    BRepClass3d_SolidClassifier inside(tool);
    std::vector<double> last;
    std::vector<double> position;
    // A revolution's pieces by their angles from its start (mitcad#539).
    const gp_Pnt start = turn ? centre_of_named(sweep, near) : gp_Pnt();
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      GProp_GProps props;
      BRepGProp::VolumeProperties(pieces[i]->occt(), props);
      position.push_back(turn ? angle_about(*turn, start, props.CentreOfMass())
                              : along_of(props.CentreOfMass(), along));
      if (last.size() <= column_of[i]) {
        last.resize(column_of[i] + 1, -std::numeric_limits<double>::infinity());
      }
      inside.Perform(interior_point(pieces[i]->occt()), Precision::Confusion());
      if (inside.State() == TopAbs_IN) {
        last[column_of[i]] = std::max(last[column_of[i]], position.back());
      }
    }
    if (std::none_of(last.begin(), last.end(), [](double v) { return std::isfinite(v); })) {
      throw std::invalid_argument("the sweep does not reach the target body");
    }
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      if (std::isfinite(last[column_of[i]]) && position[i] <= last[column_of[i]] + Precision::Confusion()) {
        kept.push_back(pieces[i]);
      }
    }
  } else {
    // Up to the first crossing of the target along the sweep: a closed
    // surface (a cylinder the sweep enters from outside) crosses it twice,
    // and what lies beyond the first crossing is not the extrusion's. Each
    // column's pieces are taken in order along the sweep up to the first
    // that the target bounds.
    std::vector<double> position;
    for (const ShapePtr& piece : pieces) {
      GProp_GProps props;
      BRepGProp::VolumeProperties(piece->occt(), props);
      position.push_back(along_of(props.CentreOfMass(), along));
    }
    std::vector<double> first(column_of.empty() ? 1 : *std::max_element(column_of.begin(), column_of.end()) + 1,
                              std::numeric_limits<double>::infinity());
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      if (count_named(*pieces[i], near) > 0) {
        first[column_of[i]] = std::min(first[column_of[i]], position[i]);
      }
    }
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      if (count_named(*pieces[i], near) > 0 && position[i] <= first[column_of[i]] + Precision::Confusion()) {
        kept.push_back(pieces[i]);
      }
    }
  }
  if (kept.empty()) {
    throw std::invalid_argument("nothing of the sweep is left before the target");
  }
  for (const ShapePtr& piece : kept) {
    if (count_named(*piece, unreached) > 0) {
      throw std::invalid_argument("the target does not cut across the whole profile");
    }
  }
  return fused(kept, "the sweep to its target");
}

ShapePtr fused(const std::vector<ShapePtr>& parts, const char* what, bool merge, double fuzzy) {
  if (parts.empty()) {
    throw std::invalid_argument(std::string(what) + ": nothing to fuse");
  }
  if (parts.size() == 1) {
    return parts.front();
  }
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(parts.front()->occt());
  NCollection_List<TopoDS_Shape> tools;
  for (std::size_t i = 1; i < parts.size(); ++i) {
    tools.Append(parts[i]->occt());
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.SetNonDestructive(true);
  if (fuzzy > 0.0) {
    fuse.SetFuzzyValue(fuzzy);
  }
  detail::build(fuse);
  if (!fuse.IsDone() || fuse.HasErrors()) {
    throw std::runtime_error(std::string(what) + ": the parts could not be fused");
  }
  // Merge faces in one plane or surface.
  if (merge) {
    fuse.SimplifyResult();
  }
  FaceNamer namer(fuse.Shape());
  for (const ShapePtr& part : parts) {
    namer.carry(fuse, *part);
  }
  return namer.shape();
}

ShapePtr finished(const Shape& shape, const char* what) {
  FaceNamer namer(shape.occt());
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      namer.add(shape.face(i), name);
    }
  }
  namer.finish();
  require_valid(namer.result(), what);
  return namer.shape();
}

gp_Pnt interior_point(const TopoDS_Shape& solid) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(solid, props);
  const gp_Pnt centre = props.CentreOfMass();
  BRepClass3d_SolidClassifier classifier(solid, centre, Precision::Confusion());
  if (classifier.State() == TopAbs_IN) {
    return centre;
  }
  // Just inside the middle of a face.
  const double step = 1.0e-3 * std::max(1.0, diagonal(bounds_of(solid)));
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const gp_Pnt2d middle((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    if (BRepTopAdaptor_FClass2d(face, Precision::Confusion()).Perform(middle) != TopAbs_IN) {
      continue;
    }
    const BRepAdaptor_Surface surface(face);
    gp_Pnt point;
    gp_Vec du;
    gp_Vec dv;
    surface.D1(middle.X(), middle.Y(), point, du, dv);
    gp_Vec normal = du.Crossed(dv);
    if (normal.Magnitude() <= Precision::Confusion()) {
      continue;
    }
    normal.Normalize();
    if (face.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    const gp_Pnt candidate = point.Translated(normal * -step);
    classifier.Perform(candidate, Precision::Confusion());
    if (classifier.State() == TopAbs_IN) {
      return candidate;
    }
  }
  return centre;
}

Bnd_Box bounds_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  return box;
}

bool has_face_named(const Shape& shape, const std::string& name) { return count_named(shape, name) > 0; }

} // namespace mitcad::geometry::detail
