// SPDX-License-Identifier: MIT
#include "mitred_dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <Adaptor3d_Curve.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry::detail {
namespace {

// Directions within this of square or parallel count as such.
constexpr double kSquare = 1.0e-7;
// Points within this (relative to the radius, at least 1 mm) count as one.
constexpr double kSame = 1.0e-7;
// Two edges at a corner must open at least this far (one less the cosine
// of the angle between them: about 26 degrees), so that the plane halfway
// between them cuts their roundings at a slant the sweeps can reach.
constexpr double kOpen = 0.1;

// A face at an edge: its plane's normal out of the material and the
// direction into the face from the edge, square to it.
struct Side {
  int face = -1;
  gp_Dir normal;
  gp_Dir into;
};

// The side of a face at the point `at` of a line along `line` on its
// boundary; none where the face is not a plane along the line or the point
// `probe` into it from the line is not in it.
std::optional<Side> side_of(const Shape& body, int face, const gp_Pnt& at, const gp_Dir& line, double probe) {
  const std::optional<gp_Pln> plane = face_plane(body.face(face));
  if (!plane) {
    return std::nullopt;
  }
  const gp_Dir normal = plane->Axis().Direction();
  if (std::abs(normal.Dot(line)) > kSquare) {
    return std::nullopt;
  }
  const gp_Dir across = normal.Crossed(line);
  for (const double sign : {1.0, -1.0}) {
    const gp_Pnt inside = at.Translated(gp_Vec(across) * (sign * probe));
    if (BRepClass_FaceClassifier(body.face(face), inside, Precision::Confusion()).State() == TopAbs_IN) {
      return Side{face, normal, sign > 0.0 ? across : across.Reversed()};
    }
  }
  return std::nullopt;
}

// A face's far edge along an edge: the one other edge of the face that is
// a line along it, its point in the section through the edge's middle
// (which the far edge must reach past), and its distance from the edge.
struct FarEdge {
  int edge = -1;
  gp_Pnt point;
  double width = 0.0;
};

std::optional<FarEdge> far_edge(const Shape& body, const Side& side, int edge, const gp_Pnt& middle,
                                const gp_Dir& line) {
  std::optional<FarEdge> found;
  const TopoDS_Edge& own = body.edge(edge);
  for (TopExp_Explorer it(body.face(side.face), TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& other = TopoDS::Edge(it.Current());
    if (other.IsSame(own) || BRep_Tool::Degenerated(other)) {
      continue;
    }
    const BRepAdaptor_Curve curve(other);
    if (curve.GetType() != GeomAbs_Line ||
        std::abs(std::abs(curve.Line().Direction().Dot(line)) - 1.0) > kSquare) {
      continue;
    }
    if (found) {
      return std::nullopt; // more than one
    }
    const gp_Pnt start = curve.Value(curve.FirstParameter());
    const gp_Pnt end = curve.Value(curve.LastParameter());
    const gp_Vec offset(middle, start);
    const gp_Vec across = offset - gp_Vec(line) * offset.Dot(gp_Vec(line));
    const double width = across.Magnitude();
    // Into the face, and reaching past the middle's section.
    const double reach = gp_Vec(start, end).Dot(gp_Vec(line));
    const double at = gp_Vec(start, middle).Dot(gp_Vec(line));
    const double margin = kSame * std::max(1.0, std::abs(reach));
    if (!(width > kSame) || (across - gp_Vec(side.into) * width).Magnitude() > kSame * std::max(1.0, width) ||
        !(at > std::min(0.0, reach) + margin && at < std::max(0.0, reach) - margin)) {
      return std::nullopt;
    }
    found = FarEdge{body.edge_index(other), middle.Translated(across), width};
  }
  return found;
}

// One edge's rounding, as the section through the edge's middle gives it.
struct Rounding {
  int edge = -1;
  double radius = 0.0;
  std::array<TopoDS_Vertex, 2> ends;
  std::array<gp_Pnt, 2> points;
  gp_Dir along; // from points[0] to points[1]
  gp_Pnt middle;
  // The ball's centre, its contact on the face it rolls along and the
  // point of the far edge it rolls on, in the middle's section.
  gp_Pnt centre;
  gp_Pnt contact;
  gp_Pnt rim;
  // The rounding's point halfway between the contact and the far edge.
  gp_Pnt arc;
  // What the rounding takes away, less its arc: from the contact out past
  // the faces round the edge to the far edge (outside the material, so
  // that no face of the sweep lies on one of the body's).
  std::vector<gp_Pnt> outline;
  // How far the section reaches from the middle.
  double reach = 0.0;
  // At each end, the normal of the plane halfway between this rounding
  // and the next one, pointing into this one.
  std::array<gp_Dir, 2> mitre;
};

std::optional<Rounding> rounding_of(const Shape& body, const MitredEdge& item) {
  if (item.edge < 0 || item.edge >= body.edge_count() || !(item.radius > 0.0)) {
    return std::nullopt;
  }
  const TopoDS_Edge& edge = body.edge(item.edge);
  if (BRep_Tool::Degenerated(edge) || BRepAdaptor_Curve(edge).GetType() != GeomAbs_Line) {
    return std::nullopt;
  }
  const double r = item.radius;
  Rounding rounding;
  rounding.edge = item.edge;
  rounding.radius = r;
  rounding.ends = {TopExp::FirstVertex(edge), TopExp::LastVertex(edge)};
  if (rounding.ends[0].IsNull() || rounding.ends[1].IsNull() || rounding.ends[0].IsSame(rounding.ends[1])) {
    return std::nullopt;
  }
  rounding.points = {BRep_Tool::Pnt(rounding.ends[0]), BRep_Tool::Pnt(rounding.ends[1])};
  const double length = rounding.points[0].Distance(rounding.points[1]);
  const double same = kSame * std::max(1.0, r);
  if (!(length > same)) {
    return std::nullopt;
  }
  rounding.along = gp_Dir(gp_Vec(rounding.points[0], rounding.points[1]));
  rounding.middle = gp_Pnt(0.5 * (rounding.points[0].XYZ() + rounding.points[1].XYZ()));
  const std::array<int, 2> faces = body.edge_faces(item.edge);
  if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
    return std::nullopt;
  }
  const double probe = 1.0e-3 * std::min(r, length);
  const std::optional<Side> a = side_of(body, faces[0], rounding.middle, rounding.along, probe);
  const std::optional<Side> b = side_of(body, faces[1], rounding.middle, rounding.along, probe);
  if (!a || !b) {
    return std::nullopt;
  }
  // Convex: along each face away from the edge lies under the other face.
  if (!(a->into.Dot(b->normal) < -kSquare && b->into.Dot(a->normal) < -kSquare)) {
    return std::nullopt;
  }
  // Where a ball touching both planes would touch them, from the edge; the
  // face the rounding takes away is narrower than that, its far edge
  // nearer, and the ball rolls along the other face.
  const double both = r / std::tan(0.5 * a->into.Angle(b->into));
  const std::optional<FarEdge> far_a = far_edge(body, *a, item.edge, rounding.middle, rounding.along);
  const std::optional<FarEdge> far_b = far_edge(body, *b, item.edge, rounding.middle, rounding.along);
  const bool takes_a = far_a && far_a->width < both * (1.0 - 1.0e-9);
  const bool takes_b = far_b && far_b->width < both * (1.0 - 1.0e-9);
  if (takes_a == takes_b) {
    return std::nullopt;
  }
  const Side& rolled = takes_a ? *b : *a;
  const Side& taken = takes_a ? *a : *b;
  const FarEdge& rim = takes_a ? *far_a : *far_b;
  // The face beyond the far edge: a plane, not going on tangentially.
  if (rim.edge < 0) {
    return std::nullopt;
  }
  const std::array<int, 2> next = body.edge_faces(rim.edge);
  if (next[0] < 0 || next[1] < 0 || next[0] == next[1] || (next[0] != taken.face && next[1] != taken.face)) {
    return std::nullopt;
  }
  const std::optional<Side> beyond =
      side_of(body, next[0] == taken.face ? next[1] : next[0], rim.point, rounding.along, probe);
  if (!beyond || beyond->normal.Dot(taken.normal) > 1.0 - 1.0e-6) {
    return std::nullopt;
  }
  // The ball's centre: r inside the face it rolls along, at s from the edge
  // along it, and r from the far edge's point K: |middle + s into - r n -
  // K| = r. Of the two, the ball that stays under the face it takes away
  // and under the face beyond on their sides of K: the centre is behind K
  // seen along each face and under its plane.
  const gp_Vec q = gp_Vec(rolled.normal) * -r - gp_Vec(rounding.middle, rim.point);
  const double half = q.Dot(gp_Vec(rolled.into));
  const double disc = half * half - q.SquareMagnitude() + r * r;
  if (!(disc >= 0.0)) {
    return std::nullopt;
  }
  bool found = false;
  for (const double s : {-half + std::sqrt(disc), -half - std::sqrt(disc)}) {
    if (!(s > same)) {
      continue;
    }
    const gp_Pnt centre = rounding.middle.Translated(gp_Vec(rolled.into) * s - gp_Vec(rolled.normal) * r);
    const gp_Vec g(rim.point, centre);
    if (g.Dot(gp_Vec(taken.into)) > same && g.Dot(gp_Vec(taken.normal)) < same &&
        g.Dot(gp_Vec(beyond->into)) < -same && g.Dot(gp_Vec(beyond->normal)) < same) {
      rounding.centre = centre;
      rounding.contact = rounding.middle.Translated(gp_Vec(rolled.into) * s);
      found = true;
      break;
    }
  }
  // Its contact lies in the face it rolls along.
  if (!found || BRepClass_FaceClassifier(body.face(rolled.face), rounding.contact, Precision::Confusion()).State() !=
                    TopAbs_IN) {
    return std::nullopt;
  }
  rounding.rim = rim.point;
  // The arc on the ball's outer side between the contact and K.
  const gp_Vec out = gp_Vec(rolled.normal) + gp_Vec(rounding.centre, rim.point) / r;
  if (!(out.Magnitude() > 1.0e-6)) {
    return std::nullopt;
  }
  rounding.arc = rounding.centre.Translated(out.Normalized() * r);
  // Out from the contact square to the face it rolls along, round the edge
  // behind it, out over the face taken away and the face beyond at K, back
  // to K.
  const double past = 0.25 * std::min(r, rim.width);
  const gp_Vec behind = gp_Vec(rolled.into) + gp_Vec(taken.into); // the wedge is under half a turn
  rounding.outline = {rounding.contact,
                      rounding.contact.Translated(gp_Vec(rolled.normal) * past),
                      rounding.middle.Translated(behind.Normalized() * -past),
                      rim.point.Translated(gp_Vec(taken.normal) * past),
                      rim.point.Translated(gp_Vec(beyond->normal) * past),
                      rim.point};
  for (std::size_t i = 0; i + 1 < rounding.outline.size(); ++i) {
    if (!(rounding.outline[i].Distance(rounding.outline[i + 1]) > same)) {
      return std::nullopt;
    }
  }
  rounding.reach = rounding.middle.Distance(rounding.centre) + r;
  for (const gp_Pnt& point : rounding.outline) {
    rounding.reach = std::max(rounding.reach, rounding.middle.Distance(point));
  }
  return rounding;
}

// The rounding's section swept along its edge past both ends and cut at
// each end by its mitre: what it takes away, its rounding (the one face not
// a plane) named `name`, its other faces `other`. Null where it cannot be
// built.
ShapePtr sweep_of(const Rounding& rounding, const std::string& name, const std::string& other) {
  // How far past each end the sweep runs before the mitre cuts it: the
  // section's reach over the sine of half the angle at the corner, and
  // that reach again.
  std::array<double, 2> past{};
  for (int e = 0; e < 2; ++e) {
    const gp_Dir inward = e == 0 ? rounding.along : rounding.along.Reversed();
    const double slant = inward.Dot(rounding.mitre[static_cast<std::size_t>(e)]);
    if (!(slant > 0.0)) {
      return nullptr;
    }
    past[static_cast<std::size_t>(e)] = rounding.reach / slant + rounding.reach;
  }
  const double length = rounding.points[0].Distance(rounding.points[1]);
  const gp_Vec start = gp_Vec(rounding.along) * -(0.5 * length + past[0]);
  const std::vector<gp_Pnt>& o = rounding.outline;
  BRepBuilderAPI_MakeWire wire;
  for (std::size_t i = 0; i + 1 < o.size(); ++i) {
    wire.Add(BRepBuilderAPI_MakeEdge(o[i].Translated(start), o[i + 1].Translated(start)).Edge());
  }
  const GC_MakeArcOfCircle arc(o.back().Translated(start), rounding.arc.Translated(start),
                               o.front().Translated(start));
  if (!arc.IsDone()) {
    return nullptr;
  }
  const occ::handle<Geom_TrimmedCurve> curve = arc.Value();
  wire.Add(BRepBuilderAPI_MakeEdge(curve).Edge());
  if (!wire.IsDone()) {
    return nullptr;
  }
  const BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    return nullptr;
  }
  BRepPrimAPI_MakePrism prism(face.Face(), gp_Vec(rounding.along) * (length + past[0] + past[1]));
  prism.Build();
  if (!prism.IsDone()) {
    return nullptr;
  }
  TopoDS_Shape sweep = prism.Shape();
  for (int e = 0; e < 2; ++e) {
    // A box outside the mitre, reaching past the sweep's end there.
    const std::size_t i = static_cast<std::size_t>(e);
    const double size = 2.0 * (past[i] + rounding.reach);
    const gp_Dir outward = rounding.mitre[i].Reversed();
    const gp_Ax2 frame(rounding.points[i], outward);
    const gp_Pnt corner = rounding.points[i].Translated(gp_Vec(frame.XDirection()) * -size +
                                                        gp_Vec(frame.YDirection()) * -size);
    const TopoDS_Shape box =
        BRepPrimAPI_MakeBox(gp_Ax2(corner, outward, frame.XDirection()), 2.0 * size, 2.0 * size, size).Shape();
    BRepAlgoAPI_Cut cut(sweep, box);
    if (!cut.IsDone() || cut.HasErrors()) {
      return nullptr;
    }
    sweep = cut.Shape();
  }
  int solids = 0;
  for (TopExp_Explorer it(sweep, TopAbs_SOLID); it.More(); it.Next()) {
    ++solids;
  }
  if (solids != 1) {
    return nullptr;
  }
  std::vector<Shape::NamedFace> faces;
  int rounded = 0;
  for (TopExp_Explorer it(sweep, TopAbs_FACE); it.More(); it.Next()) {
    // (A segment swept: a plane, or a line's extrusion.)
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()), false);
    const bool plane = surface.GetType() == GeomAbs_Plane ||
                       (surface.GetType() == GeomAbs_SurfaceOfExtrusion &&
                        surface.BasisCurve()->GetType() == GeomAbs_Line);
    rounded += plane ? 0 : 1;
    faces.push_back({it.Current(), {plane ? other : name}});
  }
  if (rounded != 1) {
    return nullptr;
  }
  return std::make_shared<Shape>(sweep, faces);
}

} // namespace

ShapePtr mitred_fillet(const std::string& feature, const Shape& body, const std::vector<MitredEdge>& edges) {
  if (edges.size() < 2) {
    return nullptr;
  }
  std::vector<Rounding> roundings;
  for (const MitredEdge& item : edges) {
    std::optional<Rounding> rounding = rounding_of(body, item);
    if (!rounding || body.edge_name(item.edge).empty()) {
      return nullptr;
    }
    roundings.push_back(std::move(*rounding));
  }
  // Each end is the end of one other rounding, its mirror image in the
  // plane halfway between the two edges.
  ShapeMap vertices;
  std::vector<std::vector<std::pair<std::size_t, std::size_t>>> at;
  for (std::size_t i = 0; i < roundings.size(); ++i) {
    for (std::size_t e = 0; e < 2; ++e) {
      const int v = vertices.Add(roundings[i].ends[e]);
      if (static_cast<std::size_t>(v) > at.size()) {
        at.resize(static_cast<std::size_t>(v));
      }
      at[static_cast<std::size_t>(v - 1)].push_back({i, e});
    }
  }
  for (const auto& ends : at) {
    if (ends.size() != 2 || ends[0].first == ends[1].first) {
      return nullptr;
    }
    Rounding& one = roundings[ends[0].first];
    Rounding& two = roundings[ends[1].first];
    const gp_Pnt& vertex = one.points[ends[0].second];
    const gp_Dir d1 = ends[0].second == 0 ? one.along : one.along.Reversed();
    const gp_Dir d2 = ends[1].second == 0 ? two.along : two.along.Reversed();
    if (1.0 - d1.Dot(d2) < kOpen) {
      return nullptr;
    }
    const gp_Dir normal(gp_Vec(d1) - gp_Vec(d2));
    one.mitre[ends[0].second] = normal;
    two.mitre[ends[1].second] = normal.Reversed();
    const double tolerance = kSame * std::max({1.0, one.radius, two.radius});
    if (std::abs(one.radius - two.radius) > tolerance) {
      return nullptr;
    }
    gp_Trsf mirror;
    mirror.SetMirror(gp_Ax2(vertex, normal));
    const auto mirrored = [&](const gp_Pnt& p, const gp_Pnt& q) {
      return gp_Lin(q, two.along).Distance(p.Transformed(mirror)) <= tolerance;
    };
    if (!mirrored(one.centre, two.centre) || !mirrored(one.arc, two.arc)) {
      return nullptr;
    }
    for (std::size_t k = 0; k < one.outline.size(); ++k) {
      if (!mirrored(one.outline[k], two.outline[k])) {
        return nullptr;
      }
    }
  }
  // The mitres at the two ends of an edge do not meet within its sweep.
  for (const Rounding& rounding : roundings) {
    for (std::size_t e = 0; e < 2; ++e) {
      if (gp_Vec(rounding.points[e], rounding.points[1 - e]).Dot(gp_Vec(rounding.mitre[e])) <= 2.0 * rounding.reach) {
        return nullptr;
      }
    }
  }
  // The sweeps joined first (their faces on the mitres meet and go), then
  // cut from the body at once: an edge of the body on a mitre (where two
  // faces the roundings take away meet) lies inside the joined sweeps, not
  // on a face of one of them.
  const std::string other = face_name(feature, "mitre");
  std::vector<ShapePtr> sweeps;
  NCollection_List<TopoDS_Shape> first;
  NCollection_List<TopoDS_Shape> rest;
  for (const Rounding& rounding : roundings) {
    ShapePtr sweep = sweep_of(rounding, face_name(feature, "fillet", body.edge_name(rounding.edge)), other);
    if (!sweep) {
      return nullptr;
    }
    (sweeps.empty() ? first : rest).Append(sweep->occt());
    sweeps.push_back(std::move(sweep));
  }
  BRepAlgoAPI_Fuse fuse;
  fuse.SetArguments(first);
  fuse.SetTools(rest);
  fuse.SetRunParallel(false);
  fuse.Build();
  if (!fuse.IsDone() || fuse.HasErrors()) {
    return nullptr;
  }
  FaceNamer namer(fuse.Shape());
  for (const ShapePtr& sweep : sweeps) {
    namer.carry(fuse, *sweep);
  }
  namer.finish();
  const ShapePtr cutter = namer.shape();
  const BooleanResult made = boolean(BooleanOp::Cut, {&body}, *cutter);
  if (made.pieces.size() != 1 || made.touched.empty() || !made.touched.front()) {
    return nullptr;
  }
  const ShapePtr result = made.pieces.front().shape;
  // The sweeps' other faces lie past the body's faces and on the mitres:
  // none is left; every rounding is.
  for (int f = 0; f < result->face_count(); ++f) {
    if (face_matches(result->face_names(f), other)) {
      return nullptr;
    }
  }
  for (const Rounding& rounding : roundings) {
    const std::string name = face_name(feature, "fillet", body.edge_name(rounding.edge));
    bool held = false;
    for (int f = 0; f < result->face_count() && !held; ++f) {
      held = face_matches(result->face_names(f), name);
    }
    if (!held) {
      return nullptr;
    }
  }
  if (!BRepCheck_Analyzer(result->occt(), true, true).IsValid()) {
    return nullptr;
  }
  return result;
}

} // namespace mitcad::geometry::detail
