// SPDX-License-Identifier: MIT
#include "blend.hpp"

#include <algorithm>
#include <cmath>
#include <map>
#include <optional>
#include <set>
#include <stdexcept>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_IntCS.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <GeomLProp_SLProps.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Edge.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "skin.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// Cross-sections inside each bevel besides its ends: at least this many,
// and one per this much of the bevelled edge's turning (radians).
constexpr int kInnerSections = 8;
constexpr int kMostSections = 48;
constexpr double kTurnPerSection = 0.17453292519943295; // 10 degrees
// How closely the blend surfaces keep to their cross-sections and contact
// lines (mm).
constexpr double kFit = 1.0e-6;
// G2 at tangency weight 1: where the quintic's inner poles sit along each
// face's tangent, as fractions of the way from the contact point to the
// corner the tangents meet in. The weight scales them; the outer pole stays
// short of the corner.
constexpr double kNear = 0.35;
constexpr double kFar = 0.7;
constexpr double kFarthest = 0.95;

// A face's normal out of the material at the point nearest to `point`, and
// the curvature of its normal section along `direction` (positive where the
// face curves towards the normal).
struct Contact {
  gp_Dir normal;
  double curvature = 0.0;
};

Contact contact_on(const TopoDS_Face& face, const gp_Pnt& point, const gp_Dir& direction) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  GeomAPI_ProjectPointOnSurf projection(point, surface);
  if (projection.NbPoints() == 0) {
    throw std::runtime_error("a fillet's contact point is not on its face");
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  GeomLProp_SLProps props(surface, u, v, 2, Precision::Confusion());
  if (!props.IsNormalDefined()) {
    throw std::runtime_error("a face has no normal at a fillet's contact point");
  }
  const bool reversed = face.Orientation() == TopAbs_REVERSED;
  Contact contact;
  contact.normal = reversed ? props.Normal().Reversed() : props.Normal();
  // The normal curvature II(t, t) / I(t, t), t = a Su + b Sv the direction.
  const gp_Vec su = props.D1U();
  const gp_Vec sv = props.D1V();
  const double e = su.Dot(su);
  const double f = su.Dot(sv);
  const double g = sv.Dot(sv);
  const gp_Vec t(direction);
  const double tu = t.Dot(su);
  const double tv = t.Dot(sv);
  const double det = e * g - f * f;
  if (std::abs(det) < 1.0e-18) {
    return contact; // a degenerate point: no curvature term
  }
  const double a = (g * tu - f * tv) / det;
  const double b = (e * tv - f * tu) / det;
  const gp_Vec n(props.Normal());
  const double l = props.D2U().Dot(n);
  const double m = props.DUV().Dot(n);
  const double nn = props.D2V().Dot(n);
  const double first = e * a * a + 2 * f * a * b + g * b * b;
  if (first > 0.0) {
    const double k = (l * a * a + 2 * m * a * b + nn * b * b) / first;
    contact.curvature = reversed ? -k : k;
  }
  return contact;
}

gp_Pnt mid(const gp_Pnt& a, const gp_Pnt& b) { return gp_Pnt((a.XYZ() + b.XYZ()) / 2); }

occ::handle<Geom_BSplineCurve> bezier(const std::vector<gp_Pnt>& points, const std::vector<double>& weights) {
  const int n = static_cast<int>(points.size());
  NCollection_Array1<gp_Pnt> poles(1, n);
  NCollection_Array1<double> w(1, n);
  for (int i = 0; i < n; ++i) {
    poles(i + 1) = points[static_cast<std::size_t>(i)];
    w(i + 1) = weights.empty() ? 1.0 : weights[static_cast<std::size_t>(i)];
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> mults(1, 2);
  mults(1) = n;
  mults(2) = n;
  if (weights.empty()) {
    return new Geom_BSplineCurve(poles, knots, mults, n - 1);
  }
  return new Geom_BSplineCurve(poles, w, knots, mults, n - 1);
}

// The cross-section in a plane from the contact point `a` on face A to `b`
// on face B: tangent there to the faces' traces in the plane, a conic or
// (G2) a quintic that also takes the traces' curvature.
occ::handle<Geom_BSplineCurve> section_curve(const gp_Dir& across, const gp_Pnt& a, const TopoDS_Face& face_a,
                                             const gp_Pnt& b, const TopoDS_Face& face_b,
                                             const BlendSection& kind) {
  struct Trace {
    gp_Dir tangent; // in the plane, towards the corner
    gp_Dir normal;  // the face's normal in the plane
    double curvature = 0.0;
  };
  const auto trace = [&](const gp_Pnt& p, const TopoDS_Face& face, const gp_Pnt& other) {
    if (p.Distance(other) < Precision::Confusion()) {
      throw std::invalid_argument("a fillet's cross-section has no width");
    }
    const Contact contact = contact_on(face, p, gp_Dir(gp_Vec(p, other)));
    gp_Vec n(contact.normal);
    n -= gp_Vec(across) * n.Dot(gp_Vec(across));
    if (n.Magnitude() < 1.0e-6) {
      throw std::invalid_argument("unsupported: a fillet across a face that lies in its cross-section");
    }
    Trace t;
    t.normal = gp_Dir(n);
    gp_Vec tangent = gp_Vec(across).Crossed(gp_Vec(t.normal));
    if (tangent.Dot(gp_Vec(p, other)) < 0.0) {
      tangent.Reverse();
    }
    t.tangent = gp_Dir(tangent);
    if (kind.curvature) {
      t.curvature = contact_on(face, p, t.tangent).curvature;
    }
    return t;
  };
  const Trace ta = trace(a, face_a, b);
  const Trace tb = trace(b, face_b, a);
  // The corner the tangents meet in: a + s ta = b + u tb (least squares).
  const gp_Vec w(a, b);
  const double tt = gp_Vec(ta.tangent).Dot(gp_Vec(tb.tangent));
  const double denom = 1.0 - tt * tt;
  if (denom < 1.0e-8) {
    throw std::invalid_argument("unsupported: a fillet where its faces meet tangentially");
  }
  const double wa = w.Dot(gp_Vec(ta.tangent));
  const double wb = w.Dot(gp_Vec(tb.tangent));
  const double s = (wa - tt * wb) / denom;
  const double u = (tt * wa - wb) / denom;
  if (!(s > 0.0 && u > 0.0)) {
    throw std::invalid_argument("unsupported: a fillet whose faces turn away from each other");
  }
  const gp_Pnt corner = mid(a.Translated(gp_Vec(ta.tangent) * s), b.Translated(gp_Vec(tb.tangent) * u));
  if (!kind.curvature) {
    // The circular arc inscribed in the angle has the middle weight
    // sin(phi / 2); stretched along the legs it is the conic.
    const double phi = gp_Vec(corner, a).Angle(gp_Vec(corner, b));
    return bezier({a, corner, b}, {1.0, std::sin(phi / 2), 1.0});
  }
  const double weight = std::clamp(kind.weight, 0.1, 2.0);
  const double first_pole = kNear * weight;
  const double second_pole = std::min(kFar * weight, kFarthest);
  // A quintic's curvature at its start is 4/5 h / d^2, h the second inner
  // pole's offset from the tangent and d the first's distance.
  const auto inner = [&](const gp_Pnt& p, const Trace& along, double leg, double fraction, bool bent) {
    gp_Pnt q = p.Translated(gp_Vec(along.tangent) * (fraction * leg));
    if (bent) {
      const double d = first_pole * leg;
      q.Translate(gp_Vec(along.normal) * (1.25 * along.curvature * d * d));
    }
    return q;
  };
  return bezier({a, inner(a, ta, s, first_pole, false), inner(a, ta, s, second_pole, true),
                 inner(b, tb, u, second_pole, true), inner(b, tb, u, first_pole, false), b},
                {});
}

// Where an edge crosses a plane strictly inside it, nearest to `close`.
std::optional<gp_Pnt> crossing(const TopoDS_Edge& edge, const gp_Pln& plane, const gp_Pnt& close) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  if (curve.IsNull()) {
    return std::nullopt;
  }
  GeomAPI_IntCS intersection(new Geom_TrimmedCurve(curve, first, last), new Geom_Plane(plane));
  if (!intersection.IsDone()) {
    return std::nullopt;
  }
  const double margin = 1.0e-3 * (last - first);
  std::optional<gp_Pnt> best;
  for (int i = 1; i <= intersection.NbPoints(); ++i) {
    double u = 0.0;
    double v = 0.0;
    double t = 0.0;
    intersection.Parameters(i, u, v, t);
    if (t <= first + margin || t >= last - margin) {
      continue;
    }
    const gp_Pnt p = intersection.Point(i);
    if (!best || p.Distance(close) < best->Distance(close)) {
      best = p;
    }
  }
  return best;
}

// An edge's curve as a B-spline between its vertices, from `start` and
// (for a closed edge) running along `direction` there.
occ::handle<Geom_BSplineCurve> edge_curve(const TopoDS_Edge& edge, const gp_Pnt& start, const gp_Vec& direction) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  occ::handle<Geom_BSplineCurve> spline =
      GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
  const double to_start = spline->StartPoint().Distance(start);
  const double to_end = spline->EndPoint().Distance(start);
  if (std::abs(to_start - to_end) < Precision::Confusion()) {
    gp_Pnt p;
    gp_Vec tangent;
    spline->D1(spline->FirstParameter(), p, tangent);
    if (tangent.Dot(direction) < 0.0) {
      spline->Reverse();
    }
  } else if (to_start > to_end) {
    spline->Reverse();
  }
  return spline;
}

// The point of an edge's curve at one of its vertices.
gp_Pnt end_point(const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  TopoDS_Vertex v1;
  TopoDS_Vertex v2;
  TopExp::Vertices(edge, v1, v2);
  if (v1.IsSame(vertex)) {
    return curve->Value(first);
  }
  if (v2.IsSame(vertex)) {
    return curve->Value(last);
  }
  throw std::runtime_error("a fillet's contact line does not end at its cross-section");
}

// The Gordon surface through the cross-sections and the contact lines. The
// curves go in as they are: Gordon combines rational ones (conics, arcs)
// exactly at a low degree, where their polynomial forms of high degree
// overflow its degree limit. Where it still fails, the polynomial forms of
// degree 5 are tried.
occ::handle<Geom_BSplineSurface> blend_surface(const std::vector<occ::handle<Geom_Curve>>& profiles,
                                               const std::vector<occ::handle<Geom_Curve>>& guides) {
  try {
    return gordon_surface(profiles, guides, kFit);
  } catch (const std::runtime_error&) {
  }
  const auto plain = [](const std::vector<occ::handle<Geom_Curve>>& curves) {
    std::vector<occ::handle<Geom_Curve>> result;
    for (const occ::handle<Geom_Curve>& curve : curves) {
      const occ::handle<Geom_BSplineCurve> spline = occ::down_cast<Geom_BSplineCurve>(curve);
      if (spline.IsNull() || !spline->IsRational()) {
        result.push_back(curve);
        continue;
      }
      GeomConvert_ApproxCurve approximation(spline, 0.1 * kFit, GeomAbs_C2, 200, 5);
      if (!approximation.HasResult() || approximation.MaxError() > 0.1 * kFit) {
        throw std::runtime_error("a fillet's cross-section could not be made polynomial");
      }
      result.push_back(approximation.Curve());
    }
    return result;
  };
  return gordon_surface(plain(profiles), plain(guides), kFit);
}

// The axis of a circle (or an arc of one); none for other curves.
std::optional<gp_Ax1> circle_axis(occ::handle<Geom_Curve> curve) {
  while (const occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(curve)) {
    curve = trimmed->BasisCurve();
  }
  if (const occ::handle<Geom_Circle> circle = occ::down_cast<Geom_Circle>(curve)) {
    return circle->Axis();
  }
  return std::nullopt;
}

// The common axis of circles: the bevelled edge and its contact lines on
// faces of revolution about it. The cross-sections are then the same
// curve turned about it.
std::optional<gp_Ax1> common_axis(const std::vector<TopoDS_Edge>& edges) {
  std::optional<gp_Ax1> axis;
  for (const TopoDS_Edge& edge : edges) {
    double first = 0.0;
    double last = 0.0;
    const std::optional<gp_Ax1> own = circle_axis(BRep_Tool::Curve(edge, first, last));
    if (!own) {
      return std::nullopt;
    }
    if (!axis) {
      axis = own;
    } else if (!axis->IsParallel(*own, 1.0e-9) ||
               gp_Lin(*axis).Distance(own->Location()) > Precision::Confusion()) {
      return std::nullopt;
    }
  }
  return axis;
}

// How much a curve turns between two parameters (radians).
double turning(const occ::handle<Geom_Curve>& curve, double t0, double t1) {
  constexpr int samples = 64;
  double total = 0.0;
  gp_Vec previous;
  for (int i = 0; i <= samples; ++i) {
    gp_Pnt p;
    gp_Vec tangent;
    curve->D1(t0 + (t1 - t0) * i / samples, p, tangent);
    if (i > 0 && tangent.Magnitude() > 1.0e-12 && previous.Magnitude() > 1.0e-12) {
      total += previous.Angle(tangent);
    }
    previous = tangent;
  }
  return total;
}

// A bevel face of the chamfer to replace.
struct Bevel {
  int face = -1;    // of the chamfered body
  int edge = -1;    // the bevelled edge of the body
  BlendSection kind;
  int face_a = -1;  // the faces along the contact lines (chamfered body)
  int face_b = -1;
  int contact_a = -1; // edges of the chamfered body
  int contact_b = -1;
  std::vector<int> ends;
  int seam = -1; // around a closed edge: the bevel's seam, its only end
};

// A cross-section with where it is along the bevelled edge.
struct Placed {
  double along = 0.0;
  occ::handle<Geom_BSplineCurve> curve; // from the contact line on A to B
};

bool shares_name(const NameList& names, const NameList& wanted) {
  for (const std::string& name : wanted) {
    if (std::find(names.begin(), names.end(), name) != names.end()) {
      return true;
    }
  }
  return false;
}

// The bevelled edge's name in a bevel face's name <feature>:fillet(<edge>).
std::optional<std::string> bevelled_edge(const std::string& face, const std::string& prefix) {
  if (face.rfind(prefix, 0) != 0 || face.empty() || face.back() != ')') {
    return std::nullopt;
  }
  return face.substr(prefix.size(), face.size() - prefix.size() - 1);
}

// A face's point in the middle of its parameters and the normal of its
// surface there (the face's orientation not applied).
std::pair<gp_Pnt, gp_Dir> surface_middle(const TopoDS_Face& face) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  GeomLProp_SLProps props(BRep_Tool::Surface(face), (u0 + u1) / 2, (v0 + v1) / 2, 1, Precision::Confusion());
  return {props.Value(), props.Normal()};
}

// The blend face on `surface`: the bevel's boundary with its ends replaced,
// pcurves on the surface, and the bevel's place in the shell (the boundary
// reversed, and the face, where the surface's normal is the bevel's
// reversed).
TopoDS_Face blend_face(const occ::handle<Geom_Surface>& surface, const TopoDS_Face& bevel, const Shape& chamfered,
                       const std::map<int, TopoDS_Edge>& replacements) {
  const auto [point, bevel_normal] = surface_middle(bevel);
  GeomAPI_ProjectPointOnSurf projection(point, surface);
  if (projection.NbPoints() == 0) {
    throw std::runtime_error("a fillet's blend surface does not cover its bevel");
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  GeomLProp_SLProps props(surface, u, v, 1, Precision::Confusion());
  const bool same = props.Normal().Dot(bevel_normal) > 0.0;
  BRep_Builder builder;
  TopoDS_Face face;
  builder.MakeFace(face, surface, Precision::Confusion());
  std::map<int, int> uses;
  for (TopoDS_Iterator wires(bevel.Oriented(TopAbs_FORWARD)); wires.More(); wires.Next()) {
    const TopoDS_Wire& old = TopoDS::Wire(wires.Value());
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    for (TopoDS_Iterator edges(old); edges.More(); edges.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges.Value());
      const int index = chamfered.edge_index(edge);
      ++uses[index];
      const auto found = replacements.find(index);
      builder.Add(wire, found == replacements.end() ? TopoDS_Shape(edge)
                                                    : found->second.Oriented(edge.Orientation()));
    }
    wire.Orientation(old.Orientation());
    builder.Add(face, same ? wire : TopoDS::Wire(wire.Reversed()));
  }
  ShapeFix_Edge fix;
  for (const auto& [index, count] : uses) {
    const auto found = replacements.find(index);
    const TopoDS_Edge& edge = found == replacements.end() ? chamfered.edge(index) : found->second;
    fix.FixAddPCurve(edge, face, count > 1, kFit);
  }
  for (const auto& [index, count] : uses) {
    const auto found = replacements.find(index);
    fix.FixSameParameter(found == replacements.end() ? chamfered.edge(index) : found->second, face);
  }
  // A seam's two pcurves may come the wrong way round for the boundary.
  if (!BRepCheck_Analyzer(face).IsValid()) {
    for (const auto& [index, count] : uses) {
      const auto found = replacements.find(index);
      if (count < 2) {
        continue;
      }
      const TopoDS_Edge& seam = found == replacements.end() ? chamfered.edge(index) : found->second;
      double first = 0.0;
      double last = 0.0;
      const occ::handle<Geom2d_Curve> forward =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(seam.Oriented(TopAbs_FORWARD)), face, first, last);
      const occ::handle<Geom2d_Curve> reversed =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(seam.Oriented(TopAbs_REVERSED)), face, first, last);
      if (!forward.IsNull() && !reversed.IsNull()) {
        builder.UpdateEdge(seam, reversed, forward, face, BRep_Tool::Tolerance(seam));
      }
    }
  }
  return TopoDS::Face(same ? face.Oriented(bevel.Orientation()) : face.Oriented(TopAbs::Reverse(bevel.Orientation())));
}

} // namespace

ShapePtr blend_bevels(const std::string& feature, const Shape& body, const Shape& chamfered,
                      const std::map<std::string, BlendSection>& sections) {
  // The bevels and what bounds them.
  const std::string prefix = feature + ":fillet(";
  std::vector<Bevel> bevels;
  std::map<int, std::size_t> bevel_of_face;
  for (int f = 0; f < chamfered.face_count(); ++f) {
    for (const std::string& name : chamfered.face_names(f)) {
      const std::optional<std::string> edge_name = bevelled_edge(name, prefix);
      const auto kind = edge_name ? sections.find(*edge_name) : sections.end();
      if (kind == sections.end()) {
        continue; // not a bevel: a face of the body, or a rolling ball fillet's
      }
      const std::vector<int> edges = body.find_edges(*edge_name);
      if (edges.size() != 1) {
        throw std::runtime_error("a fillet's bevel has no edge " + *edge_name);
      }
      Bevel bevel;
      bevel.face = f;
      bevel.edge = edges.front();
      bevel.kind = kind->second;
      bevel_of_face[f] = bevels.size();
      bevels.push_back(bevel);
      break;
    }
  }
  for (Bevel& bevel : bevels) {
    const std::array<int, 2> sides = body.edge_faces(bevel.edge);
    if (sides[0] < 0 || sides[1] < 0 || sides[0] == sides[1]) {
      throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of " +
                                  body.edge_name(bevel.edge) + ", which does not lie between two faces");
    }
    std::set<int> seen;
    for (TopExp_Explorer it(chamfered.face(bevel.face), TopAbs_EDGE); it.More(); it.Next()) {
      const int edge = chamfered.edge_index(it.Current());
      if (!seen.insert(edge).second) {
        continue; // a seam, the second time
      }
      const std::array<int, 2> faces = chamfered.edge_faces(edge);
      const int other = faces[0] == bevel.face ? faces[1] : faces[0];
      if (other < 0) {
        throw std::runtime_error("a fillet's bevel has a free edge");
      }
      if (other == bevel.face) {
        bevel.seam = edge;
        bevel.ends.push_back(edge);
        continue;
      }
      const NameList& names = chamfered.face_names(other);
      const bool on_a = shares_name(names, body.face_names(sides[0]));
      const bool on_b = shares_name(names, body.face_names(sides[1]));
      if (on_a && !on_b && bevel.contact_a < 0) {
        bevel.contact_a = edge;
        bevel.face_a = other;
      } else if (on_b && !on_a && bevel.contact_b < 0) {
        bevel.contact_b = edge;
        bevel.face_b = other;
      } else {
        bevel.ends.push_back(edge);
      }
    }
    if (bevel.contact_a < 0 || bevel.contact_b < 0) {
      throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of " +
                                  body.edge_name(bevel.edge) + " whose bevel does not run along both faces");
    }
    if (bevel.seam >= 0 ? bevel.ends.size() != 1 : bevel.ends.size() != 2) {
      throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of " +
                                  body.edge_name(bevel.edge) + " whose bevel does not have two ends");
    }
  }

  // Cross-sections at the bevels' ends, shared where two bevels of a chain
  // meet: the curve from the end edge's first vertex to its last.
  std::map<int, occ::handle<Geom_BSplineCurve>> end_curves;
  std::map<int, gp_Pln> end_planes;
  std::map<int, TopoDS_Edge> replacements;
  ShapeBuild_ReShape reshape;
  for (const Bevel& bevel : bevels) {
    throw_if_cancelled();
    const TopoDS_Edge& contact_a = chamfered.edge(bevel.contact_a);
    const TopoDS_Edge& contact_b = chamfered.edge(bevel.contact_b);
    const TopoDS_Face& face_a = chamfered.face(bevel.face_a);
    const TopoDS_Face& face_b = chamfered.face(bevel.face_b);
    double t0 = 0.0;
    double t1 = 0.0;
    const occ::handle<Geom_Curve> spine = BRep_Tool::Curve(body.edge(bevel.edge), t0, t1);
    if (spine.IsNull()) {
      throw std::invalid_argument("unsupported: a fillet of an edge without a curve");
    }
    const auto along = [&](const gp_Pnt& p) {
      GeomAPI_ProjectPointOnCurve projection(p, spine, t0, t1);
      if (projection.NbPoints() == 0) {
        throw std::runtime_error("a fillet's cross-section is not along its edge");
      }
      return projection.LowerDistanceParameter();
    };
    std::vector<Placed> placed;
    for (int end : bevel.ends) {
      const TopoDS_Edge& edge = chamfered.edge(end);
      TopoDS_Vertex v1;
      TopoDS_Vertex v2;
      TopExp::Vertices(edge, v1, v2);
      const bool first_on_a = TopExp::FirstVertex(contact_a).IsSame(v1) || TopExp::LastVertex(contact_a).IsSame(v1);
      const TopoDS_Vertex& va = first_on_a ? v1 : v2;
      const TopoDS_Vertex& vb = first_on_a ? v2 : v1;
      const gp_Pnt a = end_point(contact_a, va);
      const gp_Pnt b = end_point(contact_b, vb);
      const double at = bevel.seam >= 0 ? t0 : along(mid(a, b));
      auto known = end_curves.find(end);
      if (known == end_curves.end()) {
        const std::array<int, 2> faces = chamfered.edge_faces(end);
        const int other = faces[0] == bevel.face ? faces[1] : faces[0];
        gp_Dir across;
        if (bevel_of_face.count(other) > 0) {
          // A joint of the chain or a closed bevel's seam: the plane through
          // the end edge closest to the one across the bevelled edge there.
          gp_Pnt p;
          gp_Vec tangent;
          spine->D1(bevel.seam >= 0 ? along(mid(a, b)) : at, p, tangent);
          const gp_Vec chord(a, b);
          gp_Vec normal = tangent - chord * (tangent.Dot(chord) / chord.SquareMagnitude());
          if (normal.Magnitude() < 1.0e-9) {
            throw std::runtime_error("a fillet's bevels meet along its edge");
          }
          across = gp_Dir(normal);
        } else {
          const std::optional<gp_Pln> plane = face_plane(chamfered.face(other));
          if (!plane) {
            throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of " +
                                        body.edge_name(bevel.edge) + " that ends on a face that is not planar");
          }
          across = plane->Axis().Direction();
        }
        occ::handle<Geom_BSplineCurve> curve = section_curve(across, a, face_a, b, face_b, bevel.kind);
        if (!first_on_a) {
          curve->Reverse();
        }
        known = end_curves.emplace(end, curve).first;
        end_planes.emplace(end, gp_Pln(a, across));
        const TopoDS_Edge replacement = BRepBuilderAPI_MakeEdge(curve, v1, v2).Edge();
        replacements[end] = replacement;
        reshape.Replace(edge.Oriented(TopAbs_FORWARD), replacement.Oriented(TopAbs_FORWARD));
      }
      occ::handle<Geom_BSplineCurve> curve = occ::down_cast<Geom_BSplineCurve>(known->second->Copy());
      if (curve->StartPoint().Distance(a) > curve->EndPoint().Distance(a)) {
        curve->Reverse();
      }
      placed.push_back({at, curve});
      if (bevel.seam >= 0) {
        // Round the closed edge from the seam back to it.
        const occ::handle<Geom_BSplineCurve> again = occ::down_cast<Geom_BSplineCurve>(curve->Copy());
        placed.push_back({t1, again});
      }
    }
    // Faces of revolution about the edge's axis, the ends in planes through
    // it: the end cross-section turned about the axis.
    std::optional<gp_Ax1> axis = common_axis({body.edge(bevel.edge), contact_a, contact_b});
    for (int end : bevel.ends) {
      const gp_Pln& plane = end_planes.at(end);
      if (axis && (std::abs(plane.Axis().Direction().Dot(axis->Direction())) > 1.0e-9 ||
                   plane.Distance(axis->Location()) > Precision::Confusion())) {
        axis.reset();
      }
    }
    occ::handle<Geom_Surface> surface;
    if (axis) {
      std::sort(placed.begin(), placed.end(), [](const Placed& x, const Placed& y) { return x.along < y.along; });
      const occ::handle<Geom_Surface> turned = new Geom_SurfaceOfRevolution(placed.front().curve, *axis);
      if (bevel.seam >= 0) {
        surface = turned;
      } else {
        // An arc: the patch between its ends (the edge's parameter is its
        // angle about the axis), not periodic, so the pcurves do not wrap.
        const double span = placed.back().along - placed.front().along;
        if (!(span > Precision::Angular())) {
          throw std::runtime_error("a fillet's arc has no length");
        }
        surface = GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(turned, 0.0, span, true));
      }
    } else {
      if (bevel.seam >= 0) {
        const double seam_at = along(placed.front().curve->Value(0.5));
        if (std::min(std::abs(seam_at - t0), std::abs(seam_at - t1)) > 1.0e-6 * std::max(1.0, t1 - t0)) {
          throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet around a "
                                      "closed edge whose bevel's seam is not at the edge's vertex");
        }
      }
      // Cross-sections inside, in planes across the bevelled edge.
      const double turn = turning(spine, t0, t1);
      const int count =
          std::clamp(static_cast<int>(std::ceil(turn / kTurnPerSection)), kInnerSections, kMostSections);
      for (int k = 1; k <= count; ++k) {
        const double t = t0 + (t1 - t0) * k / (count + 1);
        gp_Pnt p;
        gp_Vec tangent;
        spine->D1(t, p, tangent);
        if (tangent.Magnitude() < 1.0e-12) {
          continue;
        }
        const gp_Pln plane(p, gp_Dir(tangent));
        const std::optional<gp_Pnt> a = crossing(contact_a, plane, p);
        const std::optional<gp_Pnt> b = crossing(contact_b, plane, p);
        if (!a || !b) {
          continue; // beyond an end that is not across the edge
        }
        placed.push_back({t, section_curve(gp_Dir(tangent), *a, face_a, *b, face_b, bevel.kind)});
      }
      std::sort(placed.begin(), placed.end(), [](const Placed& x, const Placed& y) { return x.along < y.along; });
      std::vector<occ::handle<Geom_Curve>> profiles;
      double last_along = -1.0e300;
      for (const Placed& section : placed) {
        if (section.along - last_along < 1.0e-9 * std::max(1.0, std::abs(t1 - t0))) {
          continue;
        }
        profiles.push_back(section.curve);
        last_along = section.along;
      }
      if (profiles.size() < 2) {
        throw std::runtime_error("a fillet's bevel is too short for its cross-sections");
      }
      const occ::handle<Geom_BSplineCurve> front = occ::down_cast<Geom_BSplineCurve>(profiles.front());
      gp_Pnt p;
      gp_Vec forward;
      spine->D1(placed.front().along, p, forward);
      const std::vector<occ::handle<Geom_Curve>> guides{edge_curve(contact_a, front->StartPoint(), forward),
                                                        edge_curve(contact_b, front->EndPoint(), forward)};
      surface = blend_surface(profiles, guides);
    }
    reshape.Replace(chamfered.face(bevel.face),
                    blend_face(surface, chamfered.face(bevel.face), chamfered, replacements));
  }
  const TopoDS_Shape replaced = reshape.Apply(chamfered.occt());
  // The faces the ends lie on take the curves: their pcurves.
  ShapeFix_Edge fix;
  for (TopExp_Explorer face(replaced, TopAbs_FACE); face.More(); face.Next()) {
    for (TopExp_Explorer edge(face.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
      fix.FixAddPCurve(TopoDS::Edge(edge.Current()), TopoDS::Face(face.Current()), false, kFit);
    }
  }
  FaceNamer namer(replaced);
  namer.carry(*reshape.History(), chamfered);
  namer.finish();
  if (!BRepCheck_Analyzer(namer.result()).IsValid()) {
    require_valid(namer.result(), "fillet"); // throws with the problem
  }
  return namer.shape();
}

} // namespace mitcad::geometry::detail
