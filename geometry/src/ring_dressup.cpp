// SPDX-License-Identifier: MIT
#include "ring_dressup.hpp"

#include <algorithm>
#include <cmath>
#include <optional>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <BRep_Tool.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Lin.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry::detail {
namespace {

constexpr double kPi = 3.14159265358979323846;
// Directions within this of parallel, axes within this distance (relative
// to the circle's radius) of each other count as the same.
constexpr double kParallel = 1.0e-9;
constexpr double kCoaxial = 1.0e-7;

// A vector in the section: radial and axial components.
struct V2 {
  double r = 0.0;
  double z = 0.0;
};

V2 operator+(V2 a, V2 b) { return {a.r + b.r, a.z + b.z}; }
V2 operator-(V2 a, V2 b) { return {a.r - b.r, a.z - b.z}; }
V2 operator*(double s, V2 a) { return {s * a.r, s * a.z}; }
double dot(V2 a, V2 b) { return a.r * b.r + a.z * b.z; }
double norm(V2 a) { return std::sqrt(dot(a, a)); }
V2 unit(V2 a) { return (1.0 / norm(a)) * a; }
// A quarter turn.
V2 perpendicular(V2 a) { return {-a.z, a.r}; }

bool parallel(const gp_Dir& a, const gp_Dir& b) { return std::abs(std::abs(a.Dot(b)) - 1.0) < kParallel; }

bool coaxial(const gp_Ax1& axis, const gp_Ax1& circle, double radius) {
  return parallel(axis.Direction(), circle.Direction()) &&
         gp_Lin(circle).Distance(axis.Location()) < kCoaxial * std::max(1.0, radius);
}

// True when the face is a plane square to the axis, or a cylinder or cone
// on it: its section through the axis is a straight line.
bool straight_section(const TopoDS_Face& face, const gp_Ax1& axis, double radius) {
  const BRepAdaptor_Surface surface(face, false);
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    return parallel(surface.Plane().Axis().Direction(), axis.Direction());
  case GeomAbs_Cylinder:
    return coaxial(surface.Cylinder().Axis(), axis, radius);
  case GeomAbs_Cone:
    return coaxial(surface.Cone().Axis(), axis, radius);
  default:
    return false;
  }
}

// One edge's section: the closed outline to revolve, from the contact
// point on the first face round to the one on the second, then along the
// rounding or bevel back (its last edge); and whether the edge is convex.
struct Section {
  std::vector<gp_Pnt> outline; // a, beyond a, behind the edge, beyond b, b
  std::optional<gp_Pnt> middle; // a rounding's point nearest the edge
  bool convex = true;
  gp_Ax1 axis;
};

std::optional<Section> section_of(const Shape& body, const RingEdge& ring) {
  const TopoDS_Edge& edge = body.edge(ring.edge);
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() != GeomAbs_Circle || !TopExp::FirstVertex(edge).IsSame(TopExp::LastVertex(edge)) ||
      std::abs(curve.LastParameter() - curve.FirstParameter() - 2.0 * kPi) > 1.0e-9) {
    return std::nullopt;
  }
  const gp_Circ circle = curve.Circle();
  const gp_Ax1 axis = circle.Axis();
  const double radius = circle.Radius();
  const std::vector<int> faces = faces_at_edge(body, ring.edge);
  if (faces.size() != 2 || faces[0] == faces[1]) {
    return std::nullopt;
  }
  for (const int f : faces) {
    if (!straight_section(body.face(f), axis, radius)) {
      return std::nullopt;
    }
  }
  // The section through a point away from the circle's vertex.
  const double t = 1.0 / (2.0 * kPi);
  const gp_Pnt p = curve.Value(curve.FirstParameter() + t * (curve.LastParameter() - curve.FirstParameter()));
  const gp_Vec radial(circle.Location(), p);
  if (radial.Magnitude() < 1.0e-9) {
    return std::nullopt;
  }
  const gp_Dir r_dir(radial);
  const gp_Dir z_dir = axis.Direction();
  const auto in_section = [&](const gp_Dir& d) { return V2{d.Dot(r_dir), d.Dot(z_dir)}; };
  const auto at = [&](V2 v) { return p.Translated(gp_Vec(r_dir) * v.r + gp_Vec(z_dir) * v.z); };
  const gp_Dir na3 = normal_on_edge(body.face(faces[0]), edge, t);
  const gp_Dir nb3 = normal_on_edge(body.face(faces[1]), edge, t);
  const V2 na = in_section(na3);
  const V2 nb = in_section(nb3);
  if (std::abs(norm(na) - 1.0) > 1.0e-6 || std::abs(norm(nb) - 1.0) > 1.0e-6) {
    return std::nullopt;
  }
  // Convex or concave: just below the first face and outside the second
  // is outside convex material, inside concave.
  // (Well past the tolerances of stored bodies, well inside the dressup.)
  const double size =
      ring.fillet ? ring.radius : std::min(ring.distance, ring.distance2 > 0.0 ? ring.distance2 : ring.distance);
  const double probe = 0.05 * std::min(size, radius);
  BRepClass3d_SolidClassifier classifier(body.occt());
  const auto state = [&](V2 v) {
    classifier.Perform(at(v), 1.0e-9);
    return classifier.State();
  };
  const TopAbs_State one = state(probe * (nb - na));
  const TopAbs_State other = state(probe * (na - nb));
  if (one == TopAbs_ON || one == TopAbs_UNKNOWN || one != other) {
    return std::nullopt;
  }
  Section section;
  section.convex = one == TopAbs_OUT;
  section.axis = axis;
  // Along each face away from the edge: away from the other face's outer
  // side at a convex edge, towards it at a concave one.
  const auto along = [&](V2 own, V2 other) -> std::optional<V2> {
    V2 u = perpendicular(own);
    const double side = dot(u, other);
    if (std::abs(side) < 1.0e-6) {
      return std::nullopt; // the faces meet tangentially
    }
    return (side < 0.0) == section.convex ? u : -1.0 * u;
  };
  const std::optional<V2> ua = along(na, nb);
  const std::optional<V2> ub = along(nb, na);
  if (!ua || !ub) {
    return std::nullopt;
  }
  const double wedge = std::acos(std::max(-1.0, std::min(1.0, dot(*ua, *ub))));
  if (!(wedge > 1.0e-3 && wedge < kPi - 1.0e-3)) {
    return std::nullopt;
  }
  V2 a;
  V2 b;
  if (ring.fillet) {
    const double setback = ring.radius / std::tan(wedge / 2.0);
    a = setback * *ua;
    b = setback * *ub;
    const V2 bisector = unit(*ua + *ub);
    const V2 centre = (ring.radius / std::sin(wedge / 2.0)) * bisector;
    section.middle = at(centre - ring.radius * bisector);
  } else {
    // The distances along the first and the second face.
    const bool on_first = ring.face < 0 || ring.face == faces[0];
    if (!on_first && ring.face != faces[1]) {
      return std::nullopt;
    }
    double d1 = ring.distance;
    double d2 = ring.distance2;
    if (ring.angle > 0.0) {
      const double third = kPi - wedge - ring.angle;
      if (!(third > 1.0e-6)) {
        return std::nullopt;
      }
      d2 = d1 * std::sin(ring.angle) / std::sin(third);
    }
    if (!on_first) {
      std::swap(d1, d2);
    }
    a = d1 * *ua;
    b = d2 * *ub;
  }
  // Past the faces (outside a convex edge's material, inside a concave
  // one's), so that no face of the ring lies on one of the body's.
  const double beyond = 0.25 * std::min(norm(a), norm(b));
  const auto away = [](V2 u, V2 other) {
    V2 o = perpendicular(u);
    return dot(o, other) > 0.0 ? -1.0 * o : o;
  };
  const V2 behind = -beyond * unit(*ua + *ub);
  section.outline = {at(a), at(a + beyond * away(*ua, *ub)), at(behind), at(b + beyond * away(*ub, *ua)),
                     at(b)};
  // The section must stay off the axis.
  for (const gp_Pnt& point : section.outline) {
    if (gp_Lin(axis).Distance(point) < 1.0e-6 * std::max(1.0, radius)) {
      return std::nullopt;
    }
  }
  return section;
}

// The ring of a section, its rounding or bevel named `name`, its other
// faces `other`.
ShapePtr ring_of(const Section& section, const std::string& name, const std::string& other) {
  BRepBuilderAPI_MakeWire wire;
  const std::vector<gp_Pnt>& o = section.outline;
  for (std::size_t i = 0; i + 1 < o.size(); ++i) {
    wire.Add(BRepBuilderAPI_MakeEdge(o[i], o[i + 1]).Edge());
  }
  TopoDS_Edge dressup;
  if (section.middle) {
    const occ::handle<Geom_TrimmedCurve> arc = GC_MakeArcOfCircle(o.back(), *section.middle, o.front()).Value();
    dressup = BRepBuilderAPI_MakeEdge(arc).Edge();
  } else {
    dressup = BRepBuilderAPI_MakeEdge(o.back(), o.front()).Edge();
  }
  wire.Add(dressup);
  if (!wire.IsDone()) {
    return nullptr;
  }
  const BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    return nullptr;
  }
  BRepPrimAPI_MakeRevol revolve(face.Face(), section.axis, 2.0 * kPi, true);
  revolve.Build();
  if (!revolve.IsDone()) {
    return nullptr;
  }
  FaceNamer namer(revolve.Shape());
  // The rounding or bevel is the edge between the contact points (the
  // wire's edges are those of the face).
  const double tolerance = 1.0e-7 * std::max(1.0, o.front().Distance(o.back()));
  const auto joins = [&](const gp_Pnt& x, const gp_Pnt& y) {
    return x.Distance(o.front()) < tolerance && y.Distance(o.back()) < tolerance;
  };
  for (TopExp_Explorer it(face.Face(), TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(it.Current());
    const gp_Pnt first = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt last = BRep_Tool::Pnt(TopExp::LastVertex(e));
    namer.generated(revolve, e, joins(first, last) || joins(last, first) ? name : other);
  }
  namer.finish();
  return namer.shape();
}

} // namespace

ShapePtr ring_dressup(const std::string& feature, const char* role, const Shape& body,
                      const std::vector<RingEdge>& edges) {
  if (edges.empty()) {
    return nullptr;
  }
  std::vector<Section> sections;
  for (const RingEdge& ring : edges) {
    std::optional<Section> section = section_of(body, ring);
    if (!section) {
      return nullptr;
    }
    sections.push_back(*section);
  }
  const std::string other = face_name(feature, "ring");
  ShapePtr result;
  for (std::size_t i = 0; i < edges.size(); ++i) {
    const ShapePtr ring = ring_of(sections[i], face_name(feature, role, body.edge_name(edges[i].edge)), other);
    if (!ring) {
      return nullptr;
    }
    const Shape& current = result ? *result : body;
    const BooleanResult made =
        boolean(sections[i].convex ? BooleanOp::Cut : BooleanOp::Join, {&current}, *ring);
    if (made.pieces.size() != 1 || made.touched.empty() || !made.touched.front()) {
      return nullptr;
    }
    result = made.pieces.front().shape;
  }
  // The ring's own faces lie past the body's faces: none is left.
  for (int f = 0; f < result->face_count(); ++f) {
    for (const std::string& name : result->face_names(f)) {
      if (name_matches(name, other)) {
        return nullptr;
      }
    }
  }
  if (!BRepCheck_Analyzer(result->occt()).IsValid()) {
    return nullptr;
  }
  return result;
}

} // namespace mitcad::geometry::detail
