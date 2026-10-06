// SPDX-License-Identifier: MIT
#include "sweep.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <limits>
#include <map>
#include <optional>
#include <stdexcept>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAlgoAPI_Splitter.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
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
#include <gp_Ax3.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>

#include "history.hpp"
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

std::vector<ShapePtr> split_pieces(const Shape& solid, const TopoDS_Shape& tool,
                                   const NameList& tool_names, const char* what) {
  BRepAlgoAPI_Splitter splitter;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(solid.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool);
  splitter.SetArguments(arguments);
  splitter.SetTools(tools);
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
  for (const Region& region : regions) {
    const ProfileFace profile = make_profile(frame, region);
    SweepFace face{profile.face, {}, {region.name}};
    for (const auto& [edge, segment] : profile.edges) {
      face.edges.emplace_back(edge, face_name(feature, "side", segment));
    }
    faces.push_back(std::move(face));
  }
  return faces;
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

std::vector<SweepFace> wall_faces(const std::string& feature, const Frame& frame,
                                  const std::vector<Region>& regions, const ThinWall& wall) {
  require_positive("thin wall thickness", wall.thickness);
  const double side1 = wall.location == WallLocation::Side1    ? wall.thickness
                       : wall.location == WallLocation::Center ? wall.thickness / 2.0
                                                               : 0.0;
  const double side2 = wall.thickness - side1;
  const gp_Pln plane = plane_of(frame);
  std::vector<SweepFace> faces;
  for (const Region& region : regions) {
    const ProfileFace profile = make_profile(frame, region);
    ShapeMap profile_edges;
    std::vector<std::string> segments;
    for (const auto& [edge, segment] : profile.edges) {
      profile_edges.Add(edge);
      segments.push_back(segment);
    }
    std::vector<TopoDS_Wire> loops{BRepTools::OuterWire(profile.face)};
    for (TopExp_Explorer w(profile.face, TopAbs_WIRE); w.More(); w.Next()) {
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
      outline1 = offset_loops(profile.face, loops, side1, edges1, origin1);
    }
    if (side2 > 0.0) {
      outline2 = offset_loops(profile.face, loops, -side2, edges2, origin2);
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
      SweepFace face{band(plane, *outline, hole), {}, {region.name}};
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
    // The face and the faces next to it.
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

ShapePtr trimmed(const Shape& sweep, const Target& target, const TopoDS_Shape& tool,
                 const gp_Dir& along, const std::string& near, const NameList& far,
                 const std::string& unreached) {
  const std::vector<ShapePtr> pieces = split_pieces(sweep, tool, far, "the sweep to its target");
  int near_faces = 0;
  for (const ShapePtr& piece : pieces) {
    near_faces += count_named(*piece, near);
  }
  if (near_faces > count_named(sweep, near)) {
    throw std::invalid_argument("the target crosses the profile where the sweep starts");
  }
  std::vector<ShapePtr> kept;
  if (target.kind == Target::Kind::Body && target.through) {
    // Up to the last piece inside the body, in the order along the sweep.
    BRepClass3d_SolidClassifier inside(tool);
    double last = -std::numeric_limits<double>::infinity();
    std::vector<double> position;
    for (const ShapePtr& piece : pieces) {
      GProp_GProps props;
      BRepGProp::VolumeProperties(piece->occt(), props);
      position.push_back(along_of(props.CentreOfMass(), along));
      inside.Perform(interior_point(piece->occt()), Precision::Confusion());
      if (inside.State() == TopAbs_IN) {
        last = std::max(last, position.back());
      }
    }
    if (!std::isfinite(last)) {
      throw std::invalid_argument("the sweep does not reach the target body");
    }
    for (std::size_t i = 0; i < pieces.size(); ++i) {
      if (position[i] <= last + Precision::Confusion()) {
        kept.push_back(pieces[i]);
      }
    }
  } else {
    for (const ShapePtr& piece : pieces) {
      if (count_named(*piece, near) > 0) {
        kept.push_back(piece);
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

ShapePtr fused(const std::vector<ShapePtr>& parts, const char* what) {
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
  detail::build(fuse);
  if (!fuse.IsDone() || fuse.HasErrors()) {
    throw std::runtime_error(std::string(what) + ": the parts could not be fused");
  }
  // Merge faces in one plane or surface.
  fuse.SimplifyResult();
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
