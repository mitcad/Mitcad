// SPDX-License-Identifier: MIT
#include "loft_skin.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <functional>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill_CompatibleWires.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomConvert.hxx>
#include <GeomFill_SectionGenerator.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_Sequence.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "skin.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Curve = occ::handle<Geom_BSplineCurve>;
using Surface = occ::handle<Geom_BSplineSurface>;
using Field = std::function<gp_Vec(double)>;

// The length of a direction takeoff of weight 1 per millimetre of the
// loft's parameter (the chord length between the sections' centres), as
// the bodies stored in the reference models measure it (loft_direction_*).
// Tangent and smooth takeoffs scale with the distance between the sections'
// corresponding vertices instead, point takeoffs with each part's distance
// from the axis (end_rows).
constexpr double kWeightScale = 1.0;

// Faces are sewn within this distance.
constexpr double kSewing = 1.0e-6;

constexpr double kHalfPi = 1.57079632679489661923;

gp_Pnt centre_of(const TopoDS_Shape& shape) {
  if (shape.ShapeType() == TopAbs_VERTEX) {
    return BRep_Tool::Pnt(TopoDS::Vertex(shape));
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props);
  return props.CentreOfMass();
}

// The curve of an edge as a non-periodic B-spline running along the edge
// as it is oriented in its wire.
Curve edge_curve(const TopoDS_Edge& edge) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  if (curve.IsNull()) {
    throw std::invalid_argument("a loft section has an edge without a curve");
  }
  Curve spline = GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
  if (spline->IsPeriodic()) {
    spline->SetNotPeriodic();
  }
  if (edge.Orientation() == TopAbs_REVERSED) {
    spline->Reverse();
  }
  return spline;
}

std::vector<TopoDS_Edge> wire_edges(const TopoDS_Wire& wire) {
  std::vector<TopoDS_Edge> edges;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    edges.push_back(it.Current());
  }
  return edges;
}

// A point section as the matching algorithm takes it: a wire of one
// degenerated edge.
TopoDS_Wire point_wire(const TopoDS_Vertex& vertex) {
  BRep_Builder builder;
  TopoDS_Edge edge;
  builder.MakeEdge(edge);
  builder.Add(edge, vertex.Oriented(TopAbs_FORWARD));
  builder.Add(edge, vertex.Oriented(TopAbs_REVERSED));
  builder.Degenerated(edge, true);
  TopoDS_Wire wire;
  builder.MakeWire(wire);
  builder.Add(wire, edge);
  wire.Closed(true);
  return wire;
}

// The unit tangent of a curve at u (the direction it runs in).
gp_Vec tangent(const Curve& curve, double u) {
  gp_Pnt p;
  gp_Vec d1;
  curve->D1(u, p, d1);
  if (d1.Magnitude() <= 1.0e-12) {
    // A stationary point: the direction towards a point a little further.
    const double first = curve->FirstParameter();
    const double last = curve->LastParameter();
    const double step = 1.0e-6 * (last - first);
    const double a = std::max(first, u - step);
    const double b = std::min(last, u + step);
    d1 = gp_Vec(curve->Value(a), curve->Value(b));
  }
  return d1.Magnitude() > 0.0 ? d1.Normalized() : gp_Vec(0, 0, 0);
}

// The coefficients (a, b) of v in the plane of x and y: v ~ a x + b y.
std::pair<double, double> decompose(const gp_Vec& v, const gp_Vec& x, const gp_Vec& y) {
  const double xx = x.Dot(x);
  const double xy = x.Dot(y);
  const double yy = y.Dot(y);
  const double det = xx * yy - xy * xy;
  if (std::abs(det) <= 1.0e-14 * std::max(1.0, xx * yy)) {
    return {xx > 0.0 ? v.Dot(x) / xx : 0.0, 0.0};
  }
  const double vx = v.Dot(x);
  const double vy = v.Dot(y);
  return {(vx * yy - vy * xy) / det, (vy * xx - vx * xy) / det};
}

// --- Rails

// A rail and where it meets the sections: the points on the sections and
// the arc lengths along the rail.
struct Rail {
  Spine spine;
  std::vector<gp_Pnt> hits;
  std::vector<double> lengths;
  int vertex = -1; // the sections' vertex it runs through
};

std::vector<Rail> meet_rails(const std::vector<Path>& paths, const std::vector<SkinSection>& sections,
                             double tolerance) {
  std::vector<Rail> rails;
  for (std::size_t r = 0; r < paths.size(); ++r) {
    const std::string which = "rail " + std::to_string(r + 1);
    Rail rail{Spine(path_wire(paths[r])), {}, {}};
    for (std::size_t i = 0; i < sections.size(); ++i) {
      BRepExtrema_DistShapeShape distance(rail.spine.wire(), sections[i].shape);
      if (!distance.IsDone() || distance.NbSolution() == 0 || distance.Value() > tolerance) {
        throw std::invalid_argument(which + " does not meet section " + std::to_string(i + 1));
      }
      rail.hits.push_back(distance.PointOnShape2(1));
      rail.lengths.push_back(rail.spine.nearest(distance.PointOnShape1(1)));
    }
    bool up = true;
    bool down = true;
    for (std::size_t i = 1; i < rail.lengths.size(); ++i) {
      up = up && rail.lengths[i] > rail.lengths[i - 1] + tolerance;
      down = down && rail.lengths[i] < rail.lengths[i - 1] - tolerance;
    }
    if (!up && !down) {
      throw std::invalid_argument(which + " does not run through the sections in order");
    }
    rails.push_back(std::move(rail));
  }
  return rails;
}

// The wire with its edges split at the points that lie inside them;
// `origin` gets every edge of the result with the edge it came from.
TopoDS_Wire split_at(const TopoDS_Wire& wire, const std::vector<gp_Pnt>& points, double tolerance,
                     std::vector<std::pair<TopoDS_Edge, TopoDS_Edge>>& origin) {
  BRep_Builder builder;
  TopoDS_Wire result;
  builder.MakeWire(result);
  for (const TopoDS_Edge& edge : wire_edges(wire)) {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    TopoDS_Vertex v1;
    TopoDS_Vertex v2;
    TopExp::Vertices(edge, v1, v2);
    std::vector<double> cuts;
    for (const gp_Pnt& point : points) {
      if (curve.IsNull() || BRep_Tool::Pnt(v1).Distance(point) <= tolerance ||
          BRep_Tool::Pnt(v2).Distance(point) <= tolerance) {
        continue;
      }
      GeomAPI_ProjectPointOnCurve projection(point, curve, first, last);
      if (projection.NbPoints() > 0 && projection.LowerDistance() <= tolerance) {
        cuts.push_back(projection.LowerDistanceParameter());
      }
    }
    if (cuts.empty()) {
      builder.Add(result, edge);
      origin.emplace_back(edge, edge);
      continue;
    }
    std::sort(cuts.begin(), cuts.end());
    std::vector<TopoDS_Vertex> vertices{v1};
    std::vector<double> bounds{first};
    for (const double cut : cuts) {
      vertices.push_back(BRepBuilderAPI_MakeVertex(curve->Value(cut)).Vertex());
      bounds.push_back(cut);
    }
    vertices.push_back(v2);
    bounds.push_back(last);
    std::vector<TopoDS_Edge> pieces;
    for (std::size_t k = 0; k + 1 < bounds.size(); ++k) {
      pieces.push_back(BRepBuilderAPI_MakeEdge(curve, vertices[k], vertices[k + 1], bounds[k], bounds[k + 1]).Edge());
    }
    if (edge.Orientation() == TopAbs_REVERSED) {
      std::reverse(pieces.begin(), pieces.end());
    }
    for (const TopoDS_Edge& piece : pieces) {
      const TopoDS_Edge oriented = TopoDS::Edge(piece.Oriented(edge.Orientation()));
      builder.Add(result, oriented);
      origin.emplace_back(oriented, edge);
    }
  }
  result.Closed(true);
  return result;
}

// The arc length along a rail at the loft's parameter: a monotone cubic
// through the sections' (v_i, s_i) (Fritsch and Carlson), so that the rail
// is followed smoothly through the middle sections.
class Monotone {
public:
  Monotone(std::vector<double> x, std::vector<double> y) : m_x(std::move(x)), m_y(std::move(y)) {
    const std::size_t n = m_x.size();
    std::vector<double> h(n - 1);
    std::vector<double> d(n - 1);
    for (std::size_t i = 0; i + 1 < n; ++i) {
      h[i] = m_x[i + 1] - m_x[i];
      d[i] = (m_y[i + 1] - m_y[i]) / h[i];
    }
    m_m.assign(n, 0.0);
    m_m.front() = d.front();
    m_m.back() = d.back();
    for (std::size_t i = 1; i + 1 < n; ++i) {
      if (d[i - 1] * d[i] > 0.0) {
        m_m[i] = 3.0 * (h[i - 1] + h[i]) / ((2.0 * h[i] + h[i - 1]) / d[i - 1] + (h[i] + 2.0 * h[i - 1]) / d[i]);
      }
    }
  }

  double operator()(double x) const {
    std::size_t i = 0;
    while (i + 2 < m_x.size() && x > m_x[i + 1]) {
      ++i;
    }
    const double h = m_x[i + 1] - m_x[i];
    const double t = (x - m_x[i]) / h;
    const double t2 = t * t;
    const double t3 = t2 * t;
    return (2 * t3 - 3 * t2 + 1) * m_y[i] + (t3 - 2 * t2 + t) * h * m_m[i] + (-2 * t3 + 3 * t2) * m_y[i + 1] +
           (t3 - t2) * h * m_m[i + 1];
  }

private:
  std::vector<double> m_x;
  std::vector<double> m_y;
  std::vector<double> m_m;
};

// The sum of two polynomial B-spline surfaces over the same parameter
// ranges, brought to common degrees and knots.
Surface added(const Surface& a0, const Surface& b0) {
  Surface a = Handle(Geom_BSplineSurface)::DownCast(a0->Copy());
  Surface b = Handle(Geom_BSplineSurface)::DownCast(b0->Copy());
  const int ud = std::max(a->UDegree(), b->UDegree());
  const int vd = std::max(a->VDegree(), b->VDegree());
  a->IncreaseDegree(ud, vd);
  b->IncreaseDegree(ud, vd);
  const double tolerance = Precision::PConfusion();
  a->InsertUKnots(b->UKnots(), b->UMultiplicities(), tolerance, false);
  b->InsertUKnots(a->UKnots(), a->UMultiplicities(), tolerance, false);
  a->InsertVKnots(b->VKnots(), b->VMultiplicities(), tolerance, false);
  b->InsertVKnots(a->VKnots(), a->VMultiplicities(), tolerance, false);
  if (a->NbUPoles() != b->NbUPoles() || a->NbVPoles() != b->NbVPoles()) {
    throw std::runtime_error("the bend towards a rail could not be added to the loft's surface");
  }
  for (int k = 1; k <= a->NbUPoles(); ++k) {
    for (int l = 1; l <= a->NbVPoles(); ++l) {
      a->SetPole(k, l, gp_Pnt(a->Pole(k, l).XYZ() + b->Pole(k, l).XYZ()));
    }
  }
  return a;
}

// --- End conditions

// A face next to a face section, across one of its edges.
class Neighbour {
public:
  Neighbour(const TopoDS_Face& face, const TopoDS_Edge& edge)
      : m_surface(BRep_Tool::Surface(face)), m_reversed(face.Orientation() == TopAbs_REVERSED) {
    BRepTools::UVBounds(face, m_u0, m_u1, m_v0, m_v1);
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      if (e.Current().IsSame(edge)) {
        m_edge = TopoDS::Edge(e.Current());
        break;
      }
    }
    m_curve = BRep_Tool::Curve(m_edge, m_first, m_last);
    if (m_curve.IsNull()) {
      throw std::invalid_argument("an edge of a face section has no curve");
    }
  }

  // The geometry of the face at a point of the edge: its normal out of the
  // material, the direction across the edge out of the face (its tangent
  // plane continued), and its derivatives for curvatures.
  struct Local {
    gp_Vec normal;
    gp_Vec across;
    gp_Vec su, sv, suu, suv, svv;
  };

  Local at(const gp_Pnt& p) const {
    GeomAPI_ProjectPointOnSurf projection(p, m_surface, m_u0, m_u1, m_v0, m_v1);
    if (projection.NbPoints() == 0) {
      throw std::runtime_error("a section's edge could not be found on the face next to it");
    }
    double u = 0.0;
    double v = 0.0;
    projection.LowerDistanceParameters(u, v);
    Local local;
    gp_Pnt q;
    m_surface->D2(u, v, q, local.su, local.sv, local.suu, local.svv, local.suv);
    gp_Vec n = local.su.Crossed(local.sv);
    if (n.Magnitude() <= 1.0e-12) {
      throw std::runtime_error("the face next to a section has no normal at its edge");
    }
    n.Normalize();
    local.normal = m_reversed ? -n : n;
    GeomAPI_ProjectPointOnCurve on_edge(p, m_curve, m_first, m_last);
    const double t = on_edge.NbPoints() > 0 ? on_edge.LowerDistanceParameter() : m_first;
    gp_Pnt e;
    gp_Vec d1;
    m_curve->D1(t, e, d1);
    if (m_edge.Orientation() == TopAbs_REVERSED) {
      d1.Reverse();
    }
    // The face lies to the left of its edges about its normal.
    gp_Vec across = d1.Crossed(local.normal);
    across -= local.normal * across.Dot(local.normal);
    if (across.Magnitude() <= 1.0e-12) {
      throw std::runtime_error("the face next to a section has no direction across its edge");
    }
    local.across = across.Normalized();
    return local;
  }

  // The second fundamental form at the point in direction d: the normal
  // component of the second derivative of a curve on the face with first
  // derivative d.
  static double second(const Local& local, const gp_Vec& d) {
    const auto [a, b] = decompose(d, local.su, local.sv);
    return a * a * local.suu.Dot(local.normal) + 2 * a * b * local.suv.Dot(local.normal) +
           b * b * local.svv.Dot(local.normal);
  }

private:
  occ::handle<Geom_Surface> m_surface;
  bool m_reversed = false;
  double m_u0 = 0.0, m_u1 = 0.0, m_v0 = 0.0, m_v1 = 0.0;
  TopoDS_Edge m_edge;
  occ::handle<Geom_Curve> m_curve;
  double m_first = 0.0, m_last = 0.0;
};

// The vector two edges meeting at a vertex both leave it with: the line
// where the planes of the edges' takeoffs (each with its edge's tangent)
// meet, its component along `axis` the mean of theirs; in one plane the
// point where the takeoffs, slid along their edges, meet.
gp_Vec corner_vector(const gp_Vec& d_prev, const gp_Vec& t_prev, const gp_Vec& d_next, const gp_Vec& t_next,
                     const gp_Vec& axis) {
  const gp_Vec mean = (d_prev + d_next) / 2;
  gp_Vec n1 = t_prev.Crossed(d_prev);
  gp_Vec n2 = t_next.Crossed(d_next);
  if (n1.Magnitude() <= 1.0e-12 || n2.Magnitude() <= 1.0e-12) {
    return mean;
  }
  n1.Normalize();
  n2.Normalize();
  gp_Vec line = n1.Crossed(n2);
  if (line.Magnitude() <= 1.0e-6) {
    // One plane: d_next + l t_next = d_prev + m t_prev.
    if (t_prev.Crossed(t_next).Magnitude() <= 1.0e-6) {
      return mean;
    }
    const auto [l, m] = decompose(d_prev - d_next, t_next, -t_prev);
    return ((d_next + t_next * l) + (d_prev + t_prev * m)) / 2;
  }
  line.Normalize();
  if (line.Dot(mean) < 0.0) {
    line.Reverse();
  }
  const double along = line.Dot(axis);
  const double target = mean.Dot(axis);
  if (std::abs(along) <= 1.0e-6 || target <= 0.0) {
    return line * ((d_prev.Magnitude() + d_next.Magnitude()) / 2);
  }
  return line * (target / along);
}

// The first and second derivatives an end condition asks for along every
// edge of its section (from the section into the loft), as fields over the
// edges' parameters, shared at the vertices.
struct Takeoffs {
  std::vector<Field> first;
  std::vector<Field> second; // Smooth
};

// The takeoff fields along the section's edges, adjusted at the vertices
// so that neighbouring edges leave each vertex with one vector: each
// edge's takeoff scaled and slid along the edge, blended linearly between
// its ends.
std::vector<Field> with_corners(const std::vector<Curve>& curves, const std::vector<Field>& takeoff,
                                const gp_Vec& axis) {
  const std::size_t count = curves.size();
  std::vector<gp_Vec> corners(count);
  for (std::size_t a = 0; a < count; ++a) {
    const std::size_t prev = (a + count - 1) % count;
    const double ue = curves[prev]->LastParameter();
    const double us = curves[a]->FirstParameter();
    corners[a] = corner_vector(takeoff[prev](ue), tangent(curves[prev], ue), takeoff[a](us), tangent(curves[a], us), axis);
  }
  std::vector<Field> fields;
  for (std::size_t j = 0; j < count; ++j) {
    const Curve curve = curves[j];
    const Field d = takeoff[j];
    const double u0 = curve->FirstParameter();
    const double u1 = curve->LastParameter();
    const gp_Vec ca = corners[j];
    const gp_Vec cb = corners[(j + 1) % count];
    // Plain variables, not structured bindings: a lambda may capture those
    // only from C++20 on.
    const std::pair<double, double> start = decompose(ca, d(u0), tangent(curve, u0));
    const std::pair<double, double> end = decompose(cb, d(u1), tangent(curve, u1));
    const double ka = start.first;
    const double la = start.second;
    const double kb = end.first;
    const double lb = end.second;
    fields.push_back([=](double u) {
      if (u <= u0) {
        return ca;
      }
      if (u >= u1) {
        return cb;
      }
      const double s = (u - u0) / (u1 - u0);
      return d(u) * ((1 - s) * ka + s * kb) + tangent(curve, u) * ((1 - s) * la + s * lb);
    });
  }
  return fields;
}

// The second derivatives of a Smooth end: along each edge the normal
// component the neighbouring face's curvature asks for, the rest blended
// between the vertices, where both faces' conditions hold.
std::vector<Field> curvature_fields(const std::vector<Curve>& curves, const std::vector<Field>& first,
                                    const std::vector<Neighbour>& neighbours) {
  const std::size_t count = curves.size();
  const auto local = [&](std::size_t j, double u) { return neighbours[j].at(curves[j]->Value(u)); };
  std::vector<gp_Vec> corners(count);
  for (std::size_t a = 0; a < count; ++a) {
    const std::size_t prev = (a + count - 1) % count;
    const double ue = curves[prev]->LastParameter();
    const double us = curves[a]->FirstParameter();
    const gp_Vec c = first[a](us);
    const Neighbour::Local lp = local(prev, ue);
    const Neighbour::Local ln = local(a, us);
    const double sp = Neighbour::second(lp, c);
    const double sn = Neighbour::second(ln, c);
    const double g = lp.normal.Dot(ln.normal);
    if (std::abs(g) >= 1.0 - 1.0e-9) {
      corners[a] = ln.normal * ((sp * (g > 0 ? 1.0 : -1.0) + sn) / 2);
    } else {
      // c2 = x n_prev + y n_next with c2.n_prev = sp, c2.n_next = sn.
      const double det = 1.0 - g * g;
      const double x = (sp - g * sn) / det;
      const double y = (sn - g * sp) / det;
      corners[a] = lp.normal * x + ln.normal * y;
    }
  }
  std::vector<Field> fields;
  for (std::size_t j = 0; j < count; ++j) {
    const Curve curve = curves[j];
    const Field f = first[j];
    const Neighbour& neighbour = neighbours[j];
    const double u0 = curve->FirstParameter();
    const double u1 = curve->LastParameter();
    const gp_Vec ca = corners[j];
    const gp_Vec cb = corners[(j + 1) % count];
    const Neighbour::Local l0 = neighbour.at(curve->Value(u0));
    const Neighbour::Local l1 = neighbour.at(curve->Value(u1));
    const gp_Vec ra = ca - l0.normal * ca.Dot(l0.normal);
    const gp_Vec rb = cb - l1.normal * cb.Dot(l1.normal);
    fields.push_back([=, &neighbour](double u) {
      if (u <= u0) {
        return ca;
      }
      if (u >= u1) {
        return cb;
      }
      const double s = (u - u0) / (u1 - u0);
      const Neighbour::Local l = neighbour.at(curve->Value(u));
      const gp_Vec rest = ra * (1 - s) + rb * s;
      return l.normal * Neighbour::second(l, f(u)) + (rest - l.normal * rest.Dot(l.normal));
    });
  }
  return fields;
}

// One section after matching: a point, or its edges' curves in order.
struct Matched {
  bool point = false;
  gp_Pnt tip;
  gp_Pnt centre;
  std::vector<TopoDS_Edge> edges;
  std::vector<TopoDS_Edge> origins; // the section's own edge each came from
  std::vector<Curve> curves;
};

// The curves of one edge index across the sections, made compatible.
struct Strip {
  int degree = 0;
  NCollection_Array1<double> knots;
  NCollection_Array1<int> mults;
  std::vector<NCollection_Array1<gp_Pnt>> poles; // per section
  std::vector<NCollection_Array1<double>> weights;
  std::vector<Curve> curves; // per section; null for a point
  int pole_count() const { return poles.front().Length(); }
};

// The mean distance between the corresponding vertices of sections a and b
// (each strip's first point; a point section's tip).
double vertex_reach(const std::vector<Strip>& strips, std::size_t a, std::size_t b) {
  double sum = 0.0;
  for (const Strip& strip : strips) {
    sum += strip.poles[a](1).Distance(strip.poles[b](1));
  }
  return sum / static_cast<double>(strips.size());
}

// The plane normal of a section pointing towards its neighbour section.
gp_Vec towards(const gp_Dir& normal, const gp_Pnt& centre, const gp_Pnt& neighbour) {
  gp_Vec n(normal);
  return n.Dot(gp_Vec(centre, neighbour)) < 0.0 ? -n : n;
}

// +1 when the section's curves run counter-clockwise about the normal.
double turning(const std::vector<Curve>& curves, const gp_Vec& normal, const gp_Pnt& centre) {
  double area = 0.0;
  gp_Vec previous;
  bool started = false;
  gp_Vec first;
  for (const Curve& curve : curves) {
    for (int k = 0; k < 16; ++k) {
      const double u = curve->FirstParameter() + (curve->LastParameter() - curve->FirstParameter()) * k / 16;
      const gp_Vec p(centre, curve->Value(u));
      if (started) {
        area += previous.Crossed(p).Dot(normal);
      } else {
        first = p;
        started = true;
      }
      previous = p;
    }
  }
  area += previous.Crossed(first).Dot(normal);
  return area >= 0.0 ? 1.0 : -1.0;
}

// What an end asks of every pole row of every face: first derivatives (S_v
// along the loft) and second ones, per face and pole.
struct EndRows {
  int order = 0;
  std::vector<std::vector<gp_Vec>> first;
  std::vector<std::vector<gp_Vec>> second;
};

// `length` is the loft's parameter over the span from the section to the
// adjacent one: a takeoff L long over the span is a derivative of
// L / length per millimetre.
EndRows end_rows(const LoftEnd& condition, const SkinSection& section, const std::vector<Matched>& matched,
                 const std::vector<Strip>& strips, std::size_t index, std::size_t adjacent, double length,
                 const char* which) {
  EndRows rows;
  double weight = condition.weight * kWeightScale;
  const double into = index == 0 ? 1.0 : -1.0; // S_v is the takeoff at the start, its opposite at the end
  const Matched& here = matched[index];
  const Matched& next = matched[adjacent];
  switch (condition.kind) {
  case LoftEnd::Kind::Free:
  case LoftEnd::Kind::PointSharp:
    return rows;
  case LoftEnd::Kind::PointTangent: {
    // Radially out from the axis through the tip and the next section's
    // centre, each part of the section by its distance from the axis per
    // the span: a rounded tip whose tangent plane is at right angles to
    // the axis (with the far end of the parabola, a paraboloid from a
    // circle).
    const gp_Vec axis(here.tip, next.centre);
    if (axis.Magnitude() <= Precision::Confusion()) {
      throw std::invalid_argument(std::string("the ") + which +
                                  " point lies at the centre of the section next to it");
    }
    const gp_Dir a(axis);
    const auto radial = [&](const gp_Pnt& p) {
      const gp_Vec v(here.tip, p);
      return v - gp_Vec(a) * v.Dot(gp_Vec(a));
    };
    double sum = 0.0;
    int samples = 0;
    for (const Curve& curve : next.curves) {
      for (int k = 0; k < 16; ++k) {
        const double u = curve->FirstParameter() + (curve->LastParameter() - curve->FirstParameter()) * k / 16;
        sum += radial(curve->Value(u)).Magnitude();
        ++samples;
      }
    }
    const double reach = sum / samples;
    if (reach <= Precision::Confusion()) {
      throw std::invalid_argument(std::string("the ") + which + " point lies on the section next to it");
    }
    rows.order = 1;
    for (const Strip& strip : strips) {
      std::vector<gp_Vec> row;
      const NCollection_Array1<gp_Pnt>& poles = strip.poles[adjacent];
      for (int k = poles.Lower(); k <= poles.Upper(); ++k) {
        row.push_back(radial(poles(k)) * (into * weight / length));
      }
      rows.first.push_back(row);
    }
    return rows;
  }
  case LoftEnd::Kind::Direction:
  case LoftEnd::Kind::Tangent:
  case LoftEnd::Kind::Smooth:
    break;
  }
  std::vector<Curve> curves;
  for (const Strip& strip : strips) {
    curves.push_back(strip.curves[index]);
  }
  std::vector<Field> takeoff;
  std::vector<Neighbour> neighbours;
  gp_Vec axis;
  if (condition.kind == LoftEnd::Kind::Direction) {
    std::optional<gp_Dir> normal = section.normal;
    if (!normal && !section.face.IsNull()) {
      if (const std::optional<gp_Pln> plane = face_plane(section.face)) {
        normal = plane->Axis().Direction();
      }
    }
    if (!normal) {
      throw std::invalid_argument(std::string("the ") + which + " direction condition needs a planar section");
    }
    const gp_Vec n = towards(*normal, here.centre, next.centre);
    axis = n;
    const double out = turning(curves, n, here.centre);
    const double c = std::cos(condition.angle);
    const double s = std::sin(condition.angle);
    for (const Curve& curve : curves) {
      takeoff.push_back([=](double u) {
        const gp_Vec outward = tangent(curve, u).Crossed(n) * out;
        return (n * c + outward * s) * weight;
      });
    }
  } else {
    if (section.face.IsNull() || !section.body) {
      throw std::invalid_argument(std::string("the ") + which +
                                  " tangent and smooth conditions need a face of a body as the section");
    }
    const Shape& body = *section.body;
    const int self = body.face_index(section.face);
    for (const TopoDS_Edge& origin : here.origins) {
      const int edge = body.edge_index(origin);
      int other = -1;
      if (edge >= 0) {
        for (const int f : faces_at_edge(body, edge)) {
          if (f != self) {
            other = f;
          }
        }
      }
      if (other < 0) {
        throw std::invalid_argument(std::string("the ") + which +
                                    " condition needs a face next to every edge of the section");
      }
      neighbours.emplace_back(body.face(other), origin);
    }
    std::optional<gp_Pln> plane = face_plane(section.face);
    axis = plane ? towards(plane->Axis().Direction(), here.centre, next.centre) : gp_Vec(here.centre, next.centre);
    if (axis.Magnitude() > 0.0) {
      axis.Normalize();
    }
    // These takeoffs scale with the distance between the sections'
    // corresponding vertices (a square face's corners to the next square's),
    // not between their centres.
    weight *= vertex_reach(strips, index, adjacent) / length;
    for (std::size_t j = 0; j < curves.size(); ++j) {
      const Curve curve = curves[j];
      const Neighbour* neighbour = &neighbours[j];
      takeoff.push_back([=](double u) { return neighbour->at(curve->Value(u)).across * weight; });
    }
  }
  const std::vector<Field> first = with_corners(curves, takeoff, axis);
  std::vector<Field> second;
  if (condition.kind == LoftEnd::Kind::Smooth) {
    second = curvature_fields(curves, first, neighbours);
  }
  rows.order = second.empty() ? 1 : 2;
  for (std::size_t j = 0; j < curves.size(); ++j) {
    std::vector<gp_Vec> row = fit_field(*curves[j], first[j]);
    for (gp_Vec& d : row) {
      d *= into;
    }
    rows.first.push_back(row);
    if (!second.empty()) {
      rows.second.push_back(fit_field(*curves[j], second[j]));
    }
  }
  return rows;
}

// The far end of a two-section loft whose other end alone has a
// condition: each pole row leaves the conditioned end with its takeoff d
// and reaches the far end as the parabola would that leaves along d by the
// chord's projection on it (the cubic is that parabola for a takeoff of
// that length): the derivative 2 c - (c.d) d over the span, c the chord
// from the row's pole at the conditioned end to its pole at the far end,
// d its unit takeoff. A smooth end's second derivative keeps the cubic's
// part along the takeoff (the rest is the neighbouring faces'), and the
// far end the cubic's, so the curve is quintic. Settled by the reference
// models loft_direction_*, loft_tangent_face, loft_smooth_face and
// loft_point_tangent. Above weight w = 1 the far end's part along d is
// c.d - (w - 1) max(0, c.d - r / w), r the mean distance between the
// sections' corresponding vertices: r / 2 at weight 2, below 0 at 3 (the
// curves rise beyond the far section and come back to it). Straight across
// (a takeoff w c.d long) the curve is then, at every weight, the parabola
// that leaves with the takeoff plus (w - 1) min(c.d, r / w) (t^3 - t^2) d.
// The stored lofts of weights 1.5, 2 and 3 straight across between circles
// and between squares have exactly these poles (loft_direction_w15, _w2,
// _w3, _squares_w2); a square's faces take their corners' r at every pole
// row, hence one r per section. Tilted takeoffs above weight 1 are
// Mitcad's reading.
//
// The rule is applied along the sections' curves and fitted to pole rows
// (as the takeoffs are), so that round sections stay round.
EndRows far_end(EndRows& near, const LoftEnd& condition, const std::vector<Strip>& strips, std::size_t index,
                std::size_t far, double length) {
  // Along the loft's parameter from the conditioned end to the far one.
  const double along = index < far ? 1.0 : -1.0;
  const double weight = condition.weight;
  const double reach = vertex_reach(strips, index, far);
  EndRows rows;
  rows.order = near.order;
  for (std::size_t j = 0; j < strips.size(); ++j) {
    const Strip& strip = strips[j];
    // The strip's curves share their knots and weights: one of them
    // carries the fields, the other end may be a point.
    const Curve& basis = strip.curves[index].IsNull() ? strip.curves[far] : strip.curves[index];
    const auto at = [&](std::size_t i, double u) {
      return strip.curves[i].IsNull() ? strip.poles[i](1) : strip.curves[i]->Value(u);
    };
    // The near end's takeoff over the span, towards the far end: its pole
    // row as a curve in the strip's basis.
    NCollection_Array1<gp_Pnt> takeoff_poles(1, strip.pole_count());
    for (int k = 1; k <= strip.pole_count(); ++k) {
      takeoff_poles(k) = gp_Pnt(near.first[j][static_cast<std::size_t>(k) - 1].XYZ() * (along * length));
    }
    const Curve takeoff = new Geom_BSplineCurve(takeoff_poles, strip.weights[index], strip.knots, strip.mults,
                                                strip.degree);
    // The chord, the takeoff, its unit and the far end's derivative at u,
    // all over the span.
    struct Local {
      gp_Vec c, d0, unit, d1;
    };
    const auto local = [&](double u) {
      Local l;
      l.c = gp_Vec(at(index, u), at(far, u));
      l.d0 = gp_Vec(takeoff->Value(u).XYZ());
      l.d1 = l.c * 2;
      if (l.d0.Magnitude() > Precision::Confusion()) {
        l.unit = l.d0.Normalized();
        const double on = l.c.Dot(l.unit);
        const double part = weight > 1.0 ? on - (weight - 1.0) * std::max(0.0, on - reach / weight) : on;
        l.d1 = (l.c - l.unit * on) * 2 + l.unit * part;
      }
      return l;
    };
    std::vector<gp_Vec> first = fit_field(*basis, [&](double u) { return local(u).d1 * (along / length); });
    rows.first.push_back(first);
    if (near.order >= 2) {
      const double square = length * length;
      const std::vector<gp_Vec> kept = fit_field(*basis, [&](double u) {
        const Local l = local(u);
        return l.unit * ((l.c * 6 - l.d0 * 4 - l.d1 * 2).Dot(l.unit) / square);
      });
      for (std::size_t k = 0; k < kept.size(); ++k) {
        near.second[j][k] += kept[k];
      }
      rows.second.push_back(fit_field(*basis, [&](double u) {
        const Local l = local(u);
        return (l.d0 * 2 + l.d1 * 4 - l.c * 6) / square;
      }));
    }
  }
  return rows;
}

void check_end(const LoftEnd& condition, const SkinSection& section, const char* which) {
  const bool for_points =
      condition.kind == LoftEnd::Kind::PointSharp || condition.kind == LoftEnd::Kind::PointTangent;
  if (condition.kind != LoftEnd::Kind::Free && for_points != section.point) {
    throw std::invalid_argument(std::string("the ") + which + " condition does not fit a " +
                                (section.point ? "point" : "curve") + " section");
  }
  if (condition.kind == LoftEnd::Kind::Free || condition.kind == LoftEnd::Kind::PointSharp) {
    return;
  }
  if (!std::isfinite(condition.weight) || condition.weight <= 0.0) {
    throw std::invalid_argument(std::string("the ") + which + " condition's weight must be positive");
  }
  if (!std::isfinite(condition.angle) || std::abs(condition.angle) >= kHalfPi) {
    throw std::invalid_argument(std::string("the ") + which + " condition's angle must lie between -90 and 90 degrees");
  }
}

// The side faces' surfaces at the loft's parameters `v`: each edge
// index's curves made compatible (polynomial ones when `plain`), its pole
// rows interpolated in homogeneous coordinates with the derivatives the
// ends ask for. At a conditioned end the weights stay still, so the
// derivatives carry over to the rational surface.
std::vector<Surface> skin_surfaces(const std::vector<Matched>& matched, const std::vector<SkinSection>& sections,
                                   const LoftEnd& start, const LoftEnd& end, const std::vector<double>& v,
                                   bool plain) {
  const std::size_t count = matched.size();
  const auto first = std::find_if(matched.begin(), matched.end(), [](const Matched& m) { return !m.point; });
  const std::size_t edge_count = first->curves.size();
  std::vector<Strip> strips(edge_count);
  for (std::size_t j = 0; j < edge_count; ++j) {
    Strip& strip = strips[j];
    GeomFill_SectionGenerator generator;
    for (const Matched& m : matched) {
      if (!m.point) {
        generator.AddCurve(plain ? polynomial(m.curves[j]) : m.curves[j]);
      }
    }
    generator.Perform(Precision::PConfusion());
    const int poles = generator.NbPoles();
    strip.degree = generator.Degree();
    strip.knots.Resize(1, generator.NbKnots(), false);
    strip.mults.Resize(1, generator.NbKnots(), false);
    generator.KnotsAndMults(strip.knots, strip.mults);
    int index = 1;
    for (const Matched& m : matched) {
      strip.poles.emplace_back(1, poles);
      strip.weights.emplace_back(1, poles);
      if (m.point) {
        strip.curves.emplace_back();
        continue;
      }
      generator.Poles(index, strip.poles.back());
      generator.Weights(index, strip.weights.back());
      ++index;
      strip.curves.push_back(new Geom_BSplineCurve(strip.poles.back(), strip.weights.back(), strip.knots,
                                                   strip.mults, strip.degree));
    }
    // A point is a section of its neighbour's weights, all at the point.
    for (std::size_t i = 0; i < count; ++i) {
      if (matched[i].point) {
        const std::size_t neighbour = i == 0 ? 1 : count - 2;
        strip.poles[i].Init(matched[i].tip);
        strip.weights[i] = strip.weights[neighbour];
      }
    }
  }

  // Sections of different kinds of curves (a square and a circle) have
  // weights that differ from section to section, and the matching may cut
  // rational curves where their weights are not those of the neighbouring
  // pieces: the rows at a vertex would then differ between its two faces.
  // Polynomial forms share their weights of 1.
  if (!plain) {
    for (const Strip& strip : strips) {
      const std::size_t reference = static_cast<std::size_t>(first - matched.begin());
      for (std::size_t i = 0; i < count; ++i) {
        for (int k = 1; k <= strip.pole_count(); ++k) {
          const double a = strip.weights[i](k);
          const double b = strip.weights[reference](k);
          if (std::abs(a - b) > 1.0e-12 * std::max(1.0, std::abs(b))) {
            return skin_surfaces(matched, sections, start, end, v, true);
          }
        }
      }
    }
  }

  EndRows starts = end_rows(start, sections.front(), matched, strips, 0, 1, v[1] - v[0], "start");
  EndRows ends =
      end_rows(end, sections.back(), matched, strips, count - 1, count - 2, v[count - 1] - v[count - 2], "end");
  if (count == 2 && starts.order > 0 && ends.order == 0) {
    ends = far_end(starts, start, strips, 0, 1, v[1] - v[0]);
  } else if (count == 2 && ends.order > 0 && starts.order == 0) {
    starts = far_end(ends, end, strips, 1, 0, v[1] - v[0]);
  }
  const EndInterpolation basis(v, starts.order, ends.order);
  std::vector<Surface> surfaces;
  for (std::size_t j = 0; j < edge_count; ++j) {
    const Strip& strip = strips[j];
    const int rows = strip.pole_count();
    const int columns = basis.pole_count();
    NCollection_Array2<gp_Pnt> net(1, rows, 1, columns);
    NCollection_Array2<double> weights(1, rows, 1, columns);
    bool rational = false;
    for (int k = 1; k <= rows; ++k) {
      std::array<std::vector<double>, 4> solved;
      for (int c = 0; c < 4; ++c) {
        std::vector<double> values;
        for (std::size_t i = 0; i < count; ++i) {
          const double w = strip.weights[i](k);
          values.push_back(c < 3 ? strip.poles[i](k).Coord(c + 1) * w : w);
        }
        const auto derivatives = [&](const EndRows& rows_of, std::size_t i) {
          std::vector<double> d;
          const double w = strip.weights[i](k);
          const std::size_t pole = static_cast<std::size_t>(k) - 1;
          if (rows_of.order >= 1) {
            d.push_back(c < 3 ? rows_of.first[j][pole].Coord(c + 1) * w : 0.0);
          }
          if (rows_of.order >= 2) {
            d.push_back(c < 3 ? rows_of.second[j][pole].Coord(c + 1) * w : 0.0);
          }
          return d;
        };
        solved[static_cast<std::size_t>(c)] = basis.solve(values, derivatives(starts, 0), derivatives(ends, count - 1));
      }
      for (int l = 1; l <= columns; ++l) {
        const std::size_t at = static_cast<std::size_t>(l) - 1;
        const double w = solved[3][at];
        if (!(w > 1.0e-9)) {
          throw std::runtime_error("the loft's surface could not be built through its sections");
        }
        net(k, l) = gp_Pnt(solved[0][at] / w, solved[1][at] / w, solved[2][at] / w);
        weights(k, l) = w;
        rational = rational || std::abs(w - 1.0) > 1.0e-12;
      }
    }
    if (rational) {
      surfaces.push_back(new Geom_BSplineSurface(net, weights, strip.knots, basis.knots(), strip.mults,
                                                 basis.multiplicities(), strip.degree, basis.degree()));
    } else {
      surfaces.push_back(new Geom_BSplineSurface(net, strip.knots, basis.knots(), strip.mults,
                                                 basis.multiplicities(), strip.degree, basis.degree()));
    }
  }
  return surfaces;
}

} // namespace

ShapePtr skinned_loft(const std::vector<SkinSection>& sections, const LoftEnd& start, const LoftEnd& end,
                      const std::vector<Path>& rail_paths, const NamedWire& named, const NameList& first,
                      const NameList& last) {
  const std::size_t count = sections.size();
  check_end(start, sections.front(), "start");
  check_end(end, sections.back(), "end");
  if (!rail_paths.empty() && (sections.front().point || sections.back().point)) {
    throw std::invalid_argument("unsupported: loft rails with a point section");
  }
  Bnd_Box box;
  for (const SkinSection& section : sections) {
    BRepBndLib::Add(section.shape, box);
  }
  const double size = box.IsVoid() ? 1.0 : std::sqrt(box.SquareExtent());
  const double tolerance = std::max(1.0e-4, 1.0e-6 * size);

  // Rails, and the sections split where the rails meet them.
  std::vector<Rail> rails = meet_rails(rail_paths, sections, tolerance);
  std::vector<std::vector<std::pair<TopoDS_Edge, TopoDS_Edge>>> origins(count);
  NCollection_Sequence<TopoDS_Shape> wires;
  for (std::size_t i = 0; i < count; ++i) {
    if (sections[i].point) {
      wires.Append(point_wire(TopoDS::Vertex(sections[i].shape)));
      continue;
    }
    std::vector<gp_Pnt> hits;
    for (const Rail& rail : rails) {
      hits.push_back(rail.hits[i]);
    }
    wires.Append(split_at(TopoDS::Wire(sections[i].shape), hits, tolerance, origins[i]));
  }

  // Edge for edge, from corresponding starts.
  BRepFill_CompatibleWires compatible(wires);
  compatible.Perform();
  if (!compatible.IsDone()) {
    throw std::runtime_error("the sections could not be matched edge for edge");
  }
  const auto& generated = compatible.Generated();
  std::vector<Matched> matched(count);
  std::size_t edge_count = 0;
  for (std::size_t i = 0; i < count; ++i) {
    Matched& m = matched[i];
    m.centre = centre_of(sections[i].shape);
    if (sections[i].point) {
      m.point = true;
      m.tip = BRep_Tool::Pnt(TopoDS::Vertex(sections[i].shape));
      continue;
    }
    m.edges = wire_edges(TopoDS::Wire(compatible.Shape().Value(static_cast<int>(i) + 1)));
    for (const TopoDS_Edge& edge : m.edges) {
      TopoDS_Edge source;
      for (const auto& [piece, before] : origins[i]) {
        bool same = piece.IsSame(edge);
        if (!same && generated.IsBound(piece)) {
          for (const TopoDS_Shape& g : generated.Find(piece)) {
            same = same || g.IsSame(edge);
          }
        }
        if (same) {
          source = before;
          break;
        }
      }
      m.origins.push_back(source);
      m.curves.push_back(edge_curve(edge));
    }
    if (edge_count == 0) {
      edge_count = m.edges.size();
    } else if (m.edges.size() != edge_count) {
      throw std::runtime_error("the sections could not be matched edge for edge");
    }
  }

  // The rails run through corresponding vertices.
  for (std::size_t r = 0; r < rails.size(); ++r) {
    for (std::size_t i = 0; i < count; ++i) {
      int vertex = -1;
      for (std::size_t j = 0; j < edge_count; ++j) {
        const Curve& curve = matched[i].curves[j];
        if (curve->Value(curve->FirstParameter()).Distance(rails[r].hits[i]) <= 2 * tolerance) {
          vertex = static_cast<int>(j);
        }
      }
      if (vertex < 0 || (i > 0 && vertex != rails[r].vertex)) {
        throw std::invalid_argument("unsupported: rail " + std::to_string(r + 1) +
                                    " does not meet corresponding points of the sections");
      }
      rails[r].vertex = vertex;
    }
    for (std::size_t s = 0; s < r; ++s) {
      if (rails[s].vertex == rails[r].vertex) {
        throw std::invalid_argument("rails " + std::to_string(s + 1) + " and " + std::to_string(r + 1) +
                                    " meet the sections at the same points");
      }
    }
  }

  // The loft's parameter: the chord length between the sections' centres.
  std::vector<double> v{0.0};
  for (std::size_t i = 1; i < count; ++i) {
    v.push_back(v.back() + std::max(matched[i].centre.Distance(matched[i - 1].centre), 1.0e-3));
  }
  std::vector<Surface> surfaces = skin_surfaces(matched, sections, start, end, v, false);

  // Rails: each face bent by the rails' distances from it along the
  // vertex lines they run through, in shares that fall from 1 at a rail's
  // own vertex to 0 at the neighbouring rails along the section,
  // smoothly with no slope at the vertices. A single rail moves the whole
  // section with it (the reference model loft_rail_circle_one: the
  // circles' loft through one bent rail is the cylinder shifted along it).
  if (!rails.empty()) {
    // The vertices' arc lengths round the first section.
    std::vector<double> at{0.0};
    for (const Curve& curve : matched.front().curves) {
      at.push_back(at.back() + GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(curve)));
    }
    const double length = at.back();
    at.pop_back();
    const auto share = [&](const Rail& rail, double x) {
      if (rails.size() == 1) {
        return 1.0;
      }
      const double p = at[static_cast<std::size_t>(rail.vertex)];
      double ahead = length;
      double behind = length;
      for (const Rail& other : rails) {
        if (&other == &rail) {
          continue;
        }
        const double q = at[static_cast<std::size_t>(other.vertex)];
        ahead = std::min(ahead, std::fmod(q - p + length, length));
        behind = std::min(behind, std::fmod(p - q + length, length));
      }
      const double forward = std::fmod(x - p + length, length);
      const double backward = std::fmod(p - x + length, length);
      return std::max({0.0, 1.0 - forward / ahead, 1.0 - backward / behind});
    };
    // The rails' distances from the loft along their vertex lines, as
    // curves in a common basis over the loft's parameter.
    std::vector<Monotone> lengths;
    for (const Rail& rail : rails) {
      lengths.emplace_back(v, rail.lengths);
    }
    const auto distance = [&](std::size_t r, double x) {
      const Surface& surface = surfaces[static_cast<std::size_t>(rails[r].vertex)];
      return gp_Vec(surface->Value(surface->UKnot(1), x), rails[r].spine.point(lengths[r](x)));
    };
    std::vector<Curve> bends;
    const auto fit_bends = [&] {
      double largest = 0.0;
      for (int per_span = 8;; per_span *= 2) {
        std::vector<double> params;
        for (std::size_t i = 0; i + 1 < count; ++i) {
          for (int k = 0; k < per_span; ++k) {
            params.push_back(v[i] + (v[i + 1] - v[i]) * k / per_span);
          }
        }
        params.push_back(v.back());
        bends.clear();
        double error = 0.0;
        largest = 0.0;
        for (std::size_t r = 0; r < rails.size(); ++r) {
          std::vector<gp_Pnt> points;
          for (std::size_t k = 0; k < params.size(); ++k) {
            // Exactly through the sections.
            const bool section = (k % static_cast<std::size_t>(per_span)) == 0;
            points.emplace_back(section ? gp_XYZ(0, 0, 0) : distance(r, params[k]).XYZ());
            largest = std::max(largest, gp_Vec(points.back().XYZ()).Magnitude());
          }
          bends.push_back(interpolate_with_ends(points, params, {}, {}));
          for (std::size_t k = 0; k + 1 < params.size(); ++k) {
            const double x = (params[k] + params[k + 1]) / 2;
            error = std::max(error, gp_Vec(bends.back()->Value(x).XYZ()).Subtracted(distance(r, x)).Magnitude());
          }
        }
        if (error <= 1.0e-7 * std::max(1.0, size)) {
          return largest;
        }
        if (per_span >= 128) {
          throw std::runtime_error("the loft could not be bent to follow its rails");
        }
      }
    };
    // Rails within half the sewing distance of the loft lie on it already
    // (as straight rails on a ruled loft do).
    const double still = std::max(kSewing / 2, 1.0e-9 * size);
    if (fit_bends() > still) {
      // The bends are polynomial: so must the surfaces be.
      if (std::any_of(surfaces.begin(), surfaces.end(),
                      [](const Surface& s) { return s->IsURational() || s->IsVRational(); })) {
        surfaces = skin_surfaces(matched, sections, start, end, v, true);
        fit_bends();
      }
      for (std::size_t j = 0; j < edge_count; ++j) {
        const double xa = at[j];
        const double xb = j + 1 < edge_count ? at[j + 1] : length;
        const double u0 = surfaces[j]->UKnot(1);
        const double u1 = surfaces[j]->UKnot(surfaces[j]->NbUKnots());
        // Shares across the face: a smooth step between its vertices (and
        // down to the far side and back on a single closed edge).
        const bool middle = edge_count == 1;
        NCollection_Array1<double> uknots(1, middle ? 3 : 2);
        NCollection_Array1<int> umults(1, middle ? 3 : 2);
        uknots(1) = u0;
        umults(1) = 4;
        if (middle) {
          uknots(2) = (u0 + u1) / 2;
          umults(2) = 3;
        }
        uknots(uknots.Upper()) = u1;
        umults(umults.Upper()) = 4;
        const int upoles = middle ? 7 : 4;
        const int vpoles = bends.front()->NbPoles();
        NCollection_Array2<gp_Pnt> net(1, upoles, 1, vpoles);
        net.Init(gp_Pnt(0, 0, 0));
        for (std::size_t r = 0; r < rails.size(); ++r) {
          std::vector<double> shares;
          const double a = share(rails[r], xa);
          const double b = middle ? a : share(rails[r], xb);
          if (middle) {
            const double m = share(rails[r], xa + length / 2);
            shares = {a, a, m, m, m, a, a};
          } else {
            shares = {a, a, b, b};
          }
          for (int k = 1; k <= upoles; ++k) {
            for (int l = 1; l <= vpoles; ++l) {
              net(k, l).ChangeCoord() += bends[r]->Pole(l).XYZ() * shares[static_cast<std::size_t>(k) - 1];
            }
          }
        }
        const Surface bend = new Geom_BSplineSurface(net, uknots, bends.front()->Knots(), umults,
                                                     bends.front()->Multiplicities(), 3, bends.front()->Degree());
        surfaces[j] = added(surfaces[j], bend);
      }
      // The rails lie on the faces.
      for (std::size_t r = 0; r < rails.size(); ++r) {
        for (int k = 0; k <= 64; ++k) {
          const double x = v.front() + (v.back() - v.front()) * k / 64;
          if (distance(r, x).Magnitude() > std::max(10 * kSewing, 1.0e-6 * size)) {
            throw std::runtime_error("the loft could not be bent to follow rail " + std::to_string(r + 1));
          }
        }
      }
    }
  }

  // The faces, sewn with the caps.
  std::vector<std::string> names(edge_count);
  const auto named_section = std::find_if(matched.begin(), matched.end(), [](const Matched& m) { return !m.point; });
  for (std::size_t j = 0; j < edge_count; ++j) {
    for (const auto& [edge, name] : named.edges) {
      if (edge.IsSame(named_section->origins[j])) {
        names[j] = name;
      }
    }
  }
  BRepBuilderAPI_Sewing sewing(kSewing);
  std::vector<std::pair<TopoDS_Face, NameList>> faces;
  for (std::size_t j = 0; j < edge_count; ++j) {
    BRepBuilderAPI_MakeFace face(surfaces[j], 1.0e-7);
    if (!face.IsDone()) {
      throw std::runtime_error("a side face of the loft could not be built");
    }
    faces.emplace_back(face.Face(), names[j].empty() ? NameList{} : NameList{names[j]});
  }
  for (const auto& [index, caps] : {std::pair<std::size_t, const NameList*>{0, &first}, {count - 1, &last}}) {
    if (sections[index].point) {
      continue;
    }
    const TopoDS_Wire wire = TopoDS::Wire(compatible.Shape().Value(static_cast<int>(index) + 1));
    BRepBuilderAPI_MakeFace cap(wire, true);
    if (cap.IsDone()) {
      faces.emplace_back(cap.Face(), *caps);
    } else if (!sections[index].face.IsNull()) {
      faces.emplace_back(sections[index].face, *caps);
    } else {
      throw std::runtime_error("a cap of the loft could not be built");
    }
  }
  for (const auto& [face, face_names] : faces) {
    sewing.Add(face);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  FaceNamer namer(oriented_solid(sewing.SewedShape(), "the loft"));
  for (const auto& [face, face_names] : faces) {
    const TopoDS_Shape sewn = sewing.IsModified(face) ? sewing.Modified(face) : face;
    for (const std::string& name : face_names) {
      namer.add(sewn, name);
    }
  }
  return namer.shape();
}

} // namespace mitcad::geometry::detail
