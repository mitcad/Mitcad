// SPDX-License-Identifier: MIT
#include "loft_skin.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdarg>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>

#include <Adaptor3d_Curve.hxx>
#include <Approx_CurvilinearParameter.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill_CompatibleWires.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BSplCLib.hxx>
#include <Bnd_Box.hxx>
#include <Convert_ParameterisationType.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_CompCurveToBSplineCurve.hxx>
#include <GeomFill_SectionGenerator.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_Sequence.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "loft_merge.hpp"
#include "mitcad/geometry/user_error.hpp"
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

// The most a rail may leave a Perpendicular end's section off the face's
// normal (mitcad#2377), radians: half the 1e-6 rad within which the loft
// meets the face, the rest left to the fit of the face's normals.
constexpr double kRailAngle = 5.0e-7;

// A face section's own face as the loft's cap, without its holes: the loft
// takes only its outer boundary (mitcad#1183). A face without holes is
// itself.
TopoDS_Face without_holes(const TopoDS_Face& face) {
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  int wires = 0;
  for (TopExp_Explorer w(forward, TopAbs_WIRE); w.More(); w.Next()) {
    ++wires;
  }
  if (wires <= 1) {
    return face;
  }
  TopoDS_Face outer = TopoDS::Face(forward.EmptyCopied());
  BRep_Builder().Add(outer, BRepTools::OuterWire(forward));
  outer.Orientation(face.Orientation());
  return outer;
}

gp_Pnt centre_of(const TopoDS_Shape& shape) {
  if (shape.ShapeType() == TopAbs_VERTEX) {
    return BRep_Tool::Pnt(TopoDS::Vertex(shape));
  }
  GProp_GProps props;
  BRepGProp::LinearProperties(shape, props);
  return props.CentreOfMass();
}

// (mitcad#2509) An edge's whole circle or ellipse (a section of one closed
// edge) as the kernel's exact rational form that is continuous in its
// first derivative (Convert_RationalC1: degree 4, each knot three times),
// from where the edge's curve starts, its way, over the edge's parameter
// range; null for other curves. The default form (TgtThetaOver2) is exact
// too, but its quadratic arcs meet at knots twice over, continuous only in
// position in the homogeneous coordinates the loft interpolates its rows
// in: a side lofted through such sections had a crease by its knots at
// every join of the arcs (Continuity C0), which the kernel's offset refuses
// (BRepOffset_C0Geometry: Offset Face on the side of a loft through three
// circles said "the neighbouring faces cannot follow the moved faces"), and
// periodic_where_closed could not make it periodic where it closes. The
// rows of this form share its knots and weights, so a side through such
// sections is continuous in its first derivative all round.
Curve whole_conic(const occ::handle<Geom_Curve>& curve, double first, double last) {
  occ::handle<Geom_Curve> basis = curve;
  while (const occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(basis)) {
    basis = trimmed->BasisCurve();
  }
  if ((!basis->IsKind(STANDARD_TYPE(Geom_Circle)) && !basis->IsKind(STANDARD_TYPE(Geom_Ellipse))) ||
      std::abs(last - first - basis->Period()) > Precision::PConfusion()) {
    return Curve();
  }
  try {
    Curve spline = GeomConvert::CurveToBSplineCurve(basis, Convert_RationalC1);
    if (spline.IsNull() || !spline->IsPeriodic()) {
      return Curve();
    }
    // Opened where the edge starts, once round.
    const gp_Pnt start = curve->Value(first);
    GeomAPI_ProjectPointOnCurve on(start, spline);
    if (on.NbPoints() == 0 || on.LowerDistance() > Precision::Confusion()) {
      return Curve();
    }
    const double from = on.LowerDistanceParameter();
    spline->Segment(from, from + spline->Period());
    if (spline->IsPeriodic() || spline->StartPoint().Distance(start) > Precision::Confusion() ||
        spline->EndPoint().Distance(start) > Precision::Confusion()) {
      return Curve();
    }
    gp_Pnt p;
    gp_Vec way;
    gp_Vec own;
    curve->D1(first, p, way);
    spline->D1(spline->FirstParameter(), p, own);
    if (way.Dot(own) < 0.0) {
      spline->Reverse();
    }
    // Over the edge's range, as the other forms are.
    NCollection_Array1<double> knots(spline->Knots());
    BSplCLib::Reparametrize(first, last, knots);
    spline->SetKnots(knots);
    return spline;
  } catch (const Standard_Failure&) {
    return Curve();
  }
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
  Curve spline = whole_conic(curve, first, last);
  if (spline.IsNull()) {
    spline = GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
  }
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

// Whether MITCAD_TRACE_LOFT_BENDS is set (not empty): the fit of the bends
// towards a loft's rails then says on the standard error how near it came
// (a line `loft bends: …`), and when it fails what its breaks are and how
// near each refinement and each piece came (mitcad#2373); nothing else
// changes.
bool tracing_loft_bends() {
#ifdef _WIN32
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_TRACE_LOFT_BENDS") != 0 || value == nullptr) {
    return false;
  }
  const bool on = value[0] != '\0';
  std::free(value);
  return on;
#else
  const char* value = std::getenv("MITCAD_TRACE_LOFT_BENDS");
  return value != nullptr && value[0] != '\0';
#endif
}

#if defined(__GNUC__) || defined(__clang__)
__attribute__((format(printf, 1, 2)))
#endif
std::string bends_line(const char* format, ...) {
  char line[512];
  va_list args;
  va_start(args, format);
  std::vsnprintf(line, sizeof line, format, args);
  va_end(args);
  return std::string("loft bends: ") + line;
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

// The i-th of `count` sections or rails in words: "the first profile",
// "the last rail" (of three or more), "profile 12".
std::string place_words(std::size_t i, std::size_t count, const std::string& noun) {
  static const char* const ordinals[] = {"first", "second",  "third",  "fourth", "fifth",
                                         "sixth", "seventh", "eighth", "ninth",  "tenth"};
  if (count > 2 && i + 1 == count) {
    return "the last " + noun;
  }
  if (i < 10) {
    return std::string("the ") + ordinals[i] + " " + noun;
  }
  return noun + " " + std::to_string(i + 1);
}

// A section in words: "the first profile", "the last profile" (of three or
// more), "profile 12".
std::string profile_words(std::size_t i, std::size_t count) { return place_words(i, count, "profile"); }

// A rail in words (mitcad#2373): "the rail" (the only one), "the second
// rail", "the last rail" (of three or more), "rail 12".
std::string rail_words(std::size_t r, std::size_t count) {
  return count == 1 ? std::string("the rail") : place_words(r, count, "rail");
}

// Where the loft's parameter x is along the loft, in words, by the
// profiles at v on either side of it (mitcad#2373): "near the second
// profile", "about halfway from the first profile to the second profile",
// "about a quarter of the way …", "about three quarters of the way …".
std::string along_words(double x, const std::vector<double>& v) {
  const std::size_t n = v.size();
  std::size_t i = 0;
  while (i + 2 < n && x > v[i + 1]) {
    ++i;
  }
  const double share = std::clamp((x - v[i]) / std::max(v[i + 1] - v[i], 1.0e-12), 0.0, 1.0);
  const std::string from = profile_words(i, n);
  const std::string to = profile_words(i + 1, n);
  if (share < 0.125) {
    return "near " + from;
  }
  if (share > 0.875) {
    return "near " + to;
  }
  const std::string way = share < 0.375    ? "about a quarter of the way"
                          : share <= 0.625 ? "about halfway"
                                           : "about three quarters of the way";
  return way + " from " + from + " to " + to;
}

// A distance in millimetres in words: three significant digits.
std::string distance_words(double value) {
  char text[32];
  std::snprintf(text, sizeof text, "%.3g", value);
  return text;
}

std::string point_words(const gp_Pnt& p) {
  char text[96];
  std::snprintf(text, sizeof text, "(%.6g, %.6g, %.6g)", p.X(), p.Y(), p.Z());
  return text;
}

// The error of a rail the loft could not be bent to follow (mitcad#2373):
// for the person the rail by its place among the rails and where along the
// loft, in words; for agents its number, how far along it (`along`, mm),
// the point there (also painted) and `why`. x is the loft's parameter
// there, v the sections'.
std::string not_followed(const std::vector<Rail>& rails, std::size_t r, double along, double x,
                         const std::vector<double>& v, const std::string& why) {
  const gp_Pnt point = rails[r].spine.point(along);
  const std::string where = along_words(x, v);
  return placed_error("the loft could not be bent to follow " + rail_words(r, rails.size()) + " " + where,
                      "rail " + std::to_string(r + 1) + " at " + distance_words(along) + " mm of its " +
                          distance_words(rails[r].spine.length()) + " mm " + point_words(point) + ", " + where +
                          ": " + why,
                      {point_place(point)});
}

std::vector<Rail> meet_rails(const std::vector<Path>& paths, const std::vector<SkinSection>& sections,
                             double tolerance) {
  std::vector<Rail> rails;
  for (std::size_t r = 0; r < paths.size(); ++r) {
    const std::string which = "rail " + std::to_string(r + 1);
    Rail rail{Spine(path_wire(paths[r])), {}, {}};
    for (std::size_t i = 0; i < sections.size(); ++i) {
      BRepExtrema_DistShapeShape distance(rail.spine.wire(), sections[i].shape);
      if (!distance.IsDone() || distance.NbSolution() == 0) {
        throw std::invalid_argument(placed_error(which + " does not reach " + profile_words(i, sections.size()),
                                                 which + " does not reach section " + std::to_string(i + 1)));
      }
      if (distance.Value() > tolerance) {
        // How far it misses, in words; the nearest points on the rail and
        // the section painted on hover (mitcad#1944).
        const gp_Pnt on_rail = distance.PointOnShape1(1);
        const gp_Pnt on_section = distance.PointOnShape2(1);
        throw std::invalid_argument(placed_error(
            which + " misses " + profile_words(i, sections.size()) + " by " + distance_words(distance.Value()) +
                " mm",
            which + " misses section " + std::to_string(i + 1) + " by " + distance_words(distance.Value()) +
                " mm (the most allowed is " + distance_words(tolerance) + " mm): nearest points " +
                point_words(on_rail) + " on the rail and " + point_words(on_section) + " on the section",
            {point_place(on_rail), point_place(on_section)}));
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
      throw std::invalid_argument(which + " does not run through the profiles in order");
    }
    rails.push_back(std::move(rail));
  }
  return rails;
}

// The wire with its edges split at the points that lie inside them;
// `origin` gets every edge of the result with the edge it came from. Closed,
// but for an `open` chain of curves (mitcad#2345), which OCCT's matching of
// the sections must take as open.
TopoDS_Wire split_at(const TopoDS_Wire& wire, const std::vector<gp_Pnt>& points, double tolerance,
                     std::vector<std::pair<TopoDS_Edge, TopoDS_Edge>>& origin, bool open = false) {
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
  result.Closed(!open);
  return result;
}

// A section of one closed edge (a circle, a ring of tangent edges merged
// into one, mitcad#2162) has no vertex of its own: its seam is only where
// its curve happens to start. Split at the rails (`split`, from
// `original`), the two pieces on either side of the seam are joined into
// one, so that the section's vertices are the rails' hits alone and every
// section runs from rail to rail alike (mitcad#2320: a seam on one side of
// rounded rectangles, matched to its own point on the other sections, made
// that side's faces differ from the other side's). `origin` gets the
// joined edge with the closed edge it came from. The split wire as it is
// where the seam is at a rail, or the section has several edges.
TopoDS_Wire without_seam(const TopoDS_Wire& original, const TopoDS_Wire& split, const std::vector<gp_Pnt>& hits,
                         double tolerance, std::vector<std::pair<TopoDS_Edge, TopoDS_Edge>>& origin) {
  const std::vector<TopoDS_Edge> before = wire_edges(original);
  if (before.size() != 1) {
    return split;
  }
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(before.front(), first, last);
  if (first.IsNull() || !first.IsSame(last)) {
    return split;
  }
  const gp_Pnt seam = BRep_Tool::Pnt(first);
  for (const gp_Pnt& hit : hits) {
    if (seam.Distance(hit) <= tolerance) {
      return split;
    }
  }
  std::vector<TopoDS_Edge> pieces = wire_edges(split);
  if (pieces.size() < 2) {
    return split;
  }
  // From the piece that leaves the seam: the last one then arrives there.
  const auto leaving = std::find_if(pieces.begin(), pieces.end(), [&](const TopoDS_Edge& piece) {
    return TopExp::FirstVertex(piece, true).IsSame(first);
  });
  if (leaving == pieces.end()) {
    return split;
  }
  std::rotate(pieces.begin(), leaving, pieces.end());
  if (!TopExp::LastVertex(pieces.back(), true).IsSame(first)) {
    return split;
  }
  const TopoDS_Edge& arriving = pieces.back();
  const TopoDS_Edge& departing = pieces.front();
  const Curve head = edge_curve(arriving);
  const Curve tail = edge_curve(departing);
  if (head->Degree() != tail->Degree()) {
    return split;
  }
  // Two pieces of one curve: the second goes on where the first ends, its
  // parameter carried on (not rescaled), the knot between them kept once
  // less than the degree where the curve is smooth across its seam (within
  // the kernel's confusion, so that the joined piece stays the curve).
  GeomConvert_CompCurveToBSplineCurve joining(head);
  if (!joining.Add(tail, Precision::Confusion(), true, false, head->Degree() - 1)) {
    return split;
  }
  const Curve joined = joining.BSplineCurve();
  BRepBuilderAPI_MakeEdge edge(joined, TopExp::FirstVertex(arriving, true), TopExp::LastVertex(departing, true),
                               joined->FirstParameter(), joined->LastParameter());
  if (!edge.IsDone()) {
    return split;
  }
  BRep_Builder builder;
  TopoDS_Wire result;
  builder.MakeWire(result);
  for (std::size_t k = 1; k + 1 < pieces.size(); ++k) {
    builder.Add(result, pieces[k]);
  }
  builder.Add(result, edge.Edge());
  result.Closed(true);
  origin.emplace_back(edge.Edge(), before.front());
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

  // The derivative at the first and at the last x (mitcad#2377).
  double start_slope() const { return m_m.front(); }
  double end_slope() const { return m_m.back(); }

private:
  std::vector<double> m_x;
  std::vector<double> m_y;
  std::vector<double> m_m;
};

// How far along the line from rail a's point `a` to rail b's point `b` the
// point p lies, as a share of the way from a to b (mitcad#2374): 0 at a and
// on the plane through a at right angles to the line, 1 at b. A loft's
// section is stretched between neighbouring rails by these shares.
double stretch_share(const gp_Pnt& p, const gp_Pnt& a, const gp_Pnt& b) {
  const gp_Vec chord(a, b);
  const double square = chord.SquareMagnitude();
  if (!(square > 0.0)) {
    throw std::runtime_error("the loft could not be stretched between two rails that meet");
  }
  return gp_Vec(a, p).Dot(chord) / square;
}

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

// A place along the loft's parameter where the bends towards the rails may
// turn less smoothly (mitcad#2373): a section, or where a rail goes on from
// one of its edges to the next; `corner` where a rail turns a corner there.
struct BendBreak {
  double x = 0.0;
  bool section = false;
  bool corner = false;
};

// Edges of a rail meeting at an angle above this (radians) make a corner;
// below it they go on with one tangent (as a sketch's tangency leaves them).
constexpr double kRailCorner = 1.0e-6;

// The arc lengths along the wire where the knots inside its B-spline
// edges are (where their third derivatives may jump).
std::vector<double> knot_lengths(const Spine& spine) {
  std::vector<double> found;
  for (int e = 0; e < spine.edge_count(); ++e) {
    const Spine::EdgeCurve edge = spine.edge_curve(e);
    occ::handle<Geom_Curve> basis = edge.curve;
    while (!Handle(Geom_TrimmedCurve)::DownCast(basis).IsNull()) {
      basis = Handle(Geom_TrimmedCurve)::DownCast(basis)->BasisCurve();
    }
    const Curve spline = Handle(Geom_BSplineCurve)::DownCast(basis);
    if (spline.IsNull()) {
      continue;
    }
    const GeomAdaptor_Curve adaptor(edge.curve, edge.first, edge.last);
    const double margin = Precision::PConfusion() * std::max(1.0, edge.last - edge.first);
    for (int k = 1; k <= spline->NbKnots(); ++k) {
      const double knot = spline->Knot(k);
      if (knot <= edge.first + margin || knot >= edge.last - margin) {
        continue;
      }
      const double local = edge.forward ? GCPnts_AbscissaPoint::Length(adaptor, edge.first, knot)
                                        : GCPnts_AbscissaPoint::Length(adaptor, knot, edge.last);
      found.push_back(spine.edge_start(e) + local);
    }
  }
  return found;
}

// The breaks of the bends in order: the sections at v, and between them
// where each rail's edges meet and where the knots inside its spline
// edges are, at the loft's parameter where the arc length along the rail
// (`lengths`, a monotone cubic per span) reaches them, and `smooth` (the
// knots of the loft's surfaces along it). Between the breaks every bend
// is a smooth function, which the pieces fitted to it reach within the
// fit's tolerance with few samples (one cubic across a knot of a fitted
// spline rail converged slowly, mitcad#2373). Breaks closer than 1e-7 of
// the loft's parameter range are one (a section's place kept): a corner
// that near a section bends the loft by less than the bends are fitted
// to.
std::vector<BendBreak> bend_breaks(const std::vector<Rail>& rails, const std::vector<Monotone>& lengths,
                                   const std::vector<double>& v, const std::vector<double>& smooth) {
  std::vector<BendBreak> breaks;
  for (const double x : v) {
    breaks.push_back({x, true, false});
  }
  for (const double x : smooth) {
    if (x > v.front() && x < v.back()) {
      breaks.push_back({x, false, false});
    }
  }
  for (std::size_t r = 0; r < rails.size(); ++r) {
    const Spine& spine = rails[r].spine;
    const std::vector<double>& s = rails[r].lengths;
    // The loft's parameter where the arc length along the rail is `at`,
    // when that is between two sections.
    const auto parameter_at = [&](double at) -> std::optional<double> {
      for (std::size_t i = 0; i + 1 < v.size(); ++i) {
        const bool up = s[i + 1] > s[i];
        if (at <= std::min(s[i], s[i + 1]) || at >= std::max(s[i], s[i + 1])) {
          continue;
        }
        // The arc length grows (or falls) monotonically over the span.
        double a = v[i];
        double b = v[i + 1];
        for (int k = 0; k < 200; ++k) {
          const double middle = (a + b) / 2;
          if (middle <= a || middle >= b) {
            break;
          }
          if ((lengths[r](middle) < at) == up) {
            a = middle;
          } else {
            b = middle;
          }
        }
        return (a + b) / 2;
      }
      return std::nullopt;
    };
    for (int e = 1; e < spine.edge_count(); ++e) {
      const double joint = spine.edge_start(e);
      const bool corner =
          spine.edge_tangent(e - 1, spine.edge_length(e - 1)).Angle(spine.edge_tangent(e, 0.0)) > kRailCorner;
      if (const std::optional<double> x = parameter_at(joint)) {
        breaks.push_back({*x, false, corner});
      }
      // A joint at a section: its corner is that section's (a joint past
      // the end sections does not bend the loft).
      for (std::size_t i = 0; i < v.size(); ++i) {
        if (std::abs(joint - s[i]) <= 1.0e-9 * std::max(1.0, spine.length())) {
          breaks[i].corner = breaks[i].corner || corner;
        }
      }
    }
    for (const double knot : knot_lengths(spine)) {
      if (const std::optional<double> x = parameter_at(knot)) {
        breaks.push_back({*x, false, false});
      }
    }
  }
  std::stable_sort(breaks.begin(), breaks.end(),
                   [](const BendBreak& a, const BendBreak& b) { return a.x < b.x; });
  const double close = 1.0e-7 * (v.back() - v.front());
  std::vector<BendBreak> merged;
  for (const BendBreak& at : breaks) {
    if (!merged.empty() && at.x - merged.back().x <= close) {
      BendBreak& kept = merged.back();
      if (at.section) {
        kept.x = at.x;
        kept.section = true;
      }
      kept.corner = kept.corner || at.corner;
      continue;
    }
    merged.push_back(at);
  }
  return merged;
}

// Polynomial B-spline curves of one degree over consecutive parameter
// ranges, each starting where the one before ends and at its end point,
// joined into one curve; `joints` gets the index of each joint's knot,
// whose multiplicity is the degree (the curves meet there, nothing more).
Curve joined_pieces(const std::vector<Curve>& pieces, std::vector<int>& joints) {
  const int degree = pieces.front()->Degree();
  std::vector<gp_Pnt> poles;
  std::vector<double> knots;
  std::vector<int> mults;
  joints.clear();
  for (std::size_t p = 0; p < pieces.size(); ++p) {
    const Curve& piece = pieces[p];
    if (piece->Degree() != degree || piece->IsRational()) {
      throw std::runtime_error("the bends towards the rails could not be joined");
    }
    for (int k = p == 0 ? 1 : 2; k <= piece->NbPoles(); ++k) {
      poles.push_back(piece->Pole(k));
    }
    for (int k = 1; k <= piece->NbKnots(); ++k) {
      if (p > 0 && k == 1) {
        // The joint: the knot ending the curve before, once less.
        mults.back() = degree;
        joints.push_back(static_cast<int>(knots.size()));
        continue;
      }
      knots.push_back(piece->Knot(k));
      mults.push_back(piece->Multiplicity(k));
    }
  }
  NCollection_Array1<gp_Pnt> pole_array(1, static_cast<int>(poles.size()));
  for (std::size_t k = 0; k < poles.size(); ++k) {
    pole_array(static_cast<int>(k) + 1) = poles[k];
  }
  NCollection_Array1<double> knot_array(1, static_cast<int>(knots.size()));
  NCollection_Array1<int> mult_array(1, static_cast<int>(mults.size()));
  for (std::size_t k = 0; k < knots.size(); ++k) {
    knot_array(static_cast<int>(k) + 1) = knots[k];
    mult_array(static_cast<int>(k) + 1) = mults[k];
  }
  return new Geom_BSplineCurve(pole_array, knot_array, mult_array, degree);
}

// The bends' pieces (per rail one per span between neighbouring breaks)
// joined into one curve per rail, all in one basis. At a break where no
// rail turns a corner the joint's knot is lowered so that the curves' first
// derivatives are continuous there, and once more, their second ones,
// where every rail's bend allows that within `tolerance` (each piece was
// fitted with its own derivatives at its ends, so they meet with one
// tangent where the bend has one); at a corner the curves keep a corner.
std::vector<Curve> joined_bends(const std::vector<std::vector<Curve>>& fitted, const std::vector<BendBreak>& breaks,
                                double tolerance) {
  std::vector<Curve> bends;
  std::vector<int> joints;
  for (const std::vector<Curve>& pieces : fitted) {
    bends.push_back(joined_pieces(pieces, joints));
  }
  // Every curve's knot lowered to the multiplicity, or none.
  const auto lowered = [&](int index, int multiplicity) {
    std::vector<Curve> copies;
    for (const Curve& bend : bends) {
      Curve copy = Handle(Geom_BSplineCurve)::DownCast(bend->Copy());
      if (!copy->RemoveKnot(index, multiplicity, tolerance)) {
        return false;
      }
      copies.push_back(copy);
    }
    bends = copies;
    return true;
  };
  const int degree = bends.front()->Degree();
  for (std::size_t j = 0; j < joints.size() && j + 1 < breaks.size(); ++j) {
    if (breaks[j + 1].corner || degree < 2) {
      continue;
    }
    if (lowered(joints[j], degree - 1) && degree > 2) {
      lowered(joints[j], degree - 2);
    }
  }
  return bends;
}

// --- End conditions

// A face next to a face section, across one of its edges; across a joined
// edge of merged tangent edges (mitcad#2162) the faces next to the edges it
// joins, each where its edge is nearest.
class Neighbour {
public:
  Neighbour(const TopoDS_Face& face, const TopoDS_Edge& edge) { add(face, edge); }

  void add(const TopoDS_Face& face, const TopoDS_Edge& edge) {
    Piece piece;
    piece.surface = BRep_Tool::Surface(face);
    piece.reversed = face.Orientation() == TopAbs_REVERSED;
    BRepTools::UVBounds(face, piece.u0, piece.u1, piece.v0, piece.v1);
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      if (e.Current().IsSame(edge)) {
        piece.edge = TopoDS::Edge(e.Current());
        break;
      }
    }
    piece.curve = BRep_Tool::Curve(piece.edge, piece.first, piece.last);
    if (piece.curve.IsNull()) {
      throw std::invalid_argument("an edge of a face section has no curve");
    }
    m_pieces.push_back(piece);
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
    // The piece whose edge is nearest.
    const Piece* nearest = &m_pieces.front();
    if (m_pieces.size() > 1) {
      double least = std::numeric_limits<double>::max();
      for (const Piece& piece : m_pieces) {
        GeomAPI_ProjectPointOnCurve on(p, piece.curve, piece.first, piece.last);
        const double d = on.NbPoints() > 0 ? on.LowerDistance()
                                           : std::min(p.Distance(piece.curve->Value(piece.first)),
                                                      p.Distance(piece.curve->Value(piece.last)));
        if (d < least) {
          least = d;
          nearest = &piece;
        }
      }
    }
    const Piece& m = *nearest;
    GeomAPI_ProjectPointOnSurf projection(p, m.surface, m.u0, m.u1, m.v0, m.v1);
    if (projection.NbPoints() == 0) {
      throw std::runtime_error("a section's edge could not be found on the face next to it");
    }
    double u = 0.0;
    double v = 0.0;
    projection.LowerDistanceParameters(u, v);
    Local local;
    gp_Pnt q;
    m.surface->D2(u, v, q, local.su, local.sv, local.suu, local.svv, local.suv);
    gp_Vec n = local.su.Crossed(local.sv);
    if (n.Magnitude() <= 1.0e-12) {
      throw std::runtime_error("the face next to a section has no normal at its edge");
    }
    n.Normalize();
    local.normal = m.reversed ? -n : n;
    GeomAPI_ProjectPointOnCurve on_edge(p, m.curve, m.first, m.last);
    const double t = on_edge.NbPoints() > 0 ? on_edge.LowerDistanceParameter() : m.first;
    gp_Pnt e;
    gp_Vec d1;
    m.curve->D1(t, e, d1);
    if (m.edge.Orientation() == TopAbs_REVERSED) {
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
  struct Piece {
    occ::handle<Geom_Surface> surface;
    bool reversed = false;
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    TopoDS_Edge edge;
    occ::handle<Geom_Curve> curve;
    double first = 0.0, last = 0.0;
  };
  std::vector<Piece> m_pieces;
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
// An open section (a chain of curves, mitcad#2345) has no corner at its
// ends: there each end edge keeps its own takeoff.
std::vector<Field> with_corners(const std::vector<Curve>& curves, const std::vector<Field>& takeoff,
                                const gp_Vec& axis, bool open = false) {
  const std::size_t count = curves.size();
  std::vector<gp_Vec> corners(count + 1);
  corners[count] = takeoff[count - 1](curves[count - 1]->LastParameter());
  for (std::size_t a = 0; a < count; ++a) {
    if (open && a == 0) {
      corners[a] = takeoff[a](curves[a]->FirstParameter());
      continue;
    }
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
    const gp_Vec cb = open && j + 1 == count ? corners[count] : corners[(j + 1) % count];
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
  bool open = false; // a chain of curves that does not close (mitcad#2345)
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
  // A Perpendicular end on a face (mitcad#2345): the first derivatives the
  // rows are fitted to, per face, which skin_surfaces holds them to.
  std::vector<Field> wanted;
};

// The normals of the face a Perpendicular end leaves at right angles
// (mitcad#2345) at points of the section, which must lie on it within
// kOnFace: the unit normal of its surface where the point projects onto
// it, in the face's orientation.
class FaceNormals {
public:
  FaceNormals(const TopoDS_Face& face, const char* which) : m_which(which) {
    m_surface = BRep_Tool::Surface(face);
    if (m_surface.IsNull()) {
      throw std::invalid_argument(std::string("the face of the ") + which + " condition has no surface");
    }
    m_reversed = face.Orientation() == TopAbs_REVERSED;
    BRepTools::UVBounds(face, m_u0, m_u1, m_v0, m_v1);
  }

  gp_Vec at(const gp_Pnt& p) const {
    GeomAPI_ProjectPointOnSurf projection(p, m_surface, m_u0, m_u1, m_v0, m_v1);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > kOnFace) {
      const std::string off =
          projection.NbPoints() == 0 ? std::string() : distance_words(projection.LowerDistance()) + " mm ";
      throw std::invalid_argument("the " + m_which + " section lies " + off +
                                  "off the face it is to leave at right angles; it must lie on the face");
    }
    double u = 0.0;
    double v = 0.0;
    projection.LowerDistanceParameters(u, v);
    gp_Pnt q;
    gp_Vec su;
    gp_Vec sv;
    m_surface->D1(u, v, q, su, sv);
    gp_Vec n = su.Crossed(sv);
    if (n.Magnitude() <= 1.0e-12) {
      throw std::invalid_argument("the face the " + m_which + " section is to leave at right angles has no normal " +
                                  "where the section lies on it");
    }
    n.Normalize();
    return m_reversed ? -n : n;
  }

  // The import's geometric tolerance (mitcad#224).
  static constexpr double kOnFace = 0.01;

private:
  std::string m_which;
  occ::handle<Geom_Surface> m_surface;
  bool m_reversed = false;
  double m_u0 = 0.0, m_u1 = 0.0, m_v0 = 0.0, m_v1 = 0.0;
};

// How fast a Perpendicular end leaves its section along rails
// (mitcad#2377): for face `j` of the section and a point of the section on
// it, the length of the takeoff (the derivative across the section by the
// loft's parameter). Empty: the condition's weight.
using Speeds = std::function<double(std::size_t, const gp_Pnt&)>;

// `length` is the loft's parameter over the span from the section to the
// adjacent one: a takeoff L long over the span is a derivative of
// L / length per millimetre. `speeds` (a Perpendicular end with rails,
// mitcad#2377) replace the weight's constant length.
EndRows end_rows(const LoftEnd& condition, const SkinSection& section, const std::vector<Matched>& matched,
                 const std::vector<Strip>& strips, std::size_t index, std::size_t adjacent, double length,
                 const char* which, const Speeds& speeds = Speeds()) {
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
  case LoftEnd::Kind::Perpendicular:
    break;
  }
  std::vector<Curve> curves;
  for (const Strip& strip : strips) {
    curves.push_back(strip.curves[index]);
  }
  std::vector<Field> takeoff;
  std::vector<Neighbour> neighbours;
  gp_Vec axis;
  // Perpendicular (mitcad#2345) to a face: the picked one, else the face
  // section's own; without either, to the section's plane as Direction at
  // angle 0.
  const TopoDS_Face reference = condition.kind != LoftEnd::Kind::Perpendicular ? TopoDS_Face()
                                : !section.perpendicular.IsNull()               ? section.perpendicular
                                                                                : section.face;
  const bool on_plane = condition.kind == LoftEnd::Kind::Perpendicular && reference.IsNull();
  // A Perpendicular takeoff's length along face j's curve: `constant`, or
  // with rails (mitcad#2377) the speeds at the curve's point.
  const auto length_along = [&speeds](std::size_t j, const Curve& curve,
                                      double constant) -> std::function<double(double)> {
    if (!speeds) {
      return [constant](double) { return constant; };
    }
    const Speeds along = speeds;
    return [along, j, curve](double u) { return along(j, curve->Value(u)); };
  };
  if (condition.kind == LoftEnd::Kind::Perpendicular && !reference.IsNull()) {
    const FaceNormals normals(reference, which);
    // One side for the whole section: the one the next section lies on,
    // seen from corresponding points.
    double side = 0.0;
    gp_Vec mean;
    for (const Strip& strip : strips) {
      const Curve& here_curve = strip.curves[index];
      const Curve& next_curve = strip.curves[adjacent];
      for (int k = 0; k < 8; ++k) {
        const double u =
            here_curve->FirstParameter() + (here_curve->LastParameter() - here_curve->FirstParameter()) * (k + 0.5) / 8;
        const gp_Pnt p = here_curve->Value(u);
        const gp_Pnt q = next_curve.IsNull() ? next.tip : next_curve->Value(u);
        const gp_Vec n = normals.at(p);
        side += n.Dot(gp_Vec(p, q));
        mean += n;
      }
    }
    const double sign = side < 0.0 ? -1.0 : 1.0;
    axis = mean * sign;
    if (axis.Magnitude() > 1.0e-12) {
      axis.Normalize();
    }
    // As long as the sections' corresponding vertices are apart, as a
    // tangent end's.
    weight *= vertex_reach(strips, index, adjacent) / length;
    const std::shared_ptr<const FaceNormals> shared = std::make_shared<const FaceNormals>(normals);
    for (std::size_t j = 0; j < curves.size(); ++j) {
      const Curve curve = curves[j];
      const std::function<double(double)> reach = length_along(j, curve, weight);
      takeoff.push_back([=](double u) { return shared->at(curve->Value(u)) * (sign * reach(u)); });
    }
  } else if (condition.kind == LoftEnd::Kind::Direction || on_plane) {
    std::optional<gp_Dir> normal = section.normal;
    if (!normal && !section.face.IsNull()) {
      if (const std::optional<gp_Pln> plane = face_plane(section.face)) {
        normal = plane->Axis().Direction();
      }
    }
    if (!normal && on_plane) {
      throw std::invalid_argument(std::string("the ") + which +
                                  " section lies in no plane: leaving it at right angles needs the face it lies on");
    }
    if (!normal) {
      throw std::invalid_argument(std::string("the ") + which + " direction condition needs a planar section");
    }
    const gp_Vec n = towards(*normal, here.centre, next.centre);
    axis = n;
    const double out = turning(curves, n, here.centre);
    // Perpendicular to the section's plane is Direction at angle 0.
    const double angle = on_plane ? 0.0 : condition.angle;
    const double c = std::cos(angle);
    // The angle is measured along the loft: positive leaves the first section
    // outward and arrives at the last one moving outward, so the takeoff into
    // the loft from the last section tilts inward.
    const double s = std::sin(angle) * (index == 0 ? 1.0 : -1.0);
    for (std::size_t j = 0; j < curves.size(); ++j) {
      const Curve curve = curves[j];
      // Along rails, a Perpendicular end as fast as they leave (mitcad#2377).
      const std::function<double(double)> reach =
          on_plane ? length_along(j, curve, weight)
                   : std::function<double(double)>([weight](double) { return weight; });
      takeoff.push_back([=](double u) {
        const gp_Vec outward = tangent(curve, u).Crossed(n) * out;
        return (n * c + outward * s) * reach(u);
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
      // A joined edge of merged tangent edges (mitcad#2162): the faces next
      // to the edges it joins.
      std::optional<Neighbour> neighbour;
      for (const TopoDS_Edge& source : source_edges(section.sources, origin)) {
        const int edge = body.edge_index(source);
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
        if (neighbour) {
          neighbour->add(body.face(other), source);
        } else {
          neighbour.emplace(body.face(other), source);
        }
      }
      neighbours.push_back(*neighbour);
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
  const std::vector<Field> first = with_corners(curves, takeoff, axis, here.open);
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
  // A face's normals are no image of the section's curve: skin_surfaces
  // holds the fitted rows to them (mitcad#2345).
  if (condition.kind == LoftEnd::Kind::Perpendicular && !reference.IsNull()) {
    for (const Field& field : first) {
      rows.wanted.push_back([field, into](double u) { return field(u) * into; });
    }
  }
  return rows;
}

// How far the rows of a Perpendicular end on a face (mitcad#2345) are
// from the takeoffs they are fitted to, at four points inside every knot
// span of every face: the largest distance relative to the takeoff's
// length (an angle in radians, near enough). 0 for other ends.
double fit_error(const EndRows& rows, const std::vector<Strip>& strips, std::size_t index) {
  double worst = 0.0;
  for (std::size_t j = 0; j < rows.wanted.size() && j < strips.size(); ++j) {
    const Strip& strip = strips[j];
    NCollection_Array1<gp_Pnt> poles(1, strip.pole_count());
    for (int k = 1; k <= strip.pole_count(); ++k) {
      poles(k) = gp_Pnt(rows.first[j][static_cast<std::size_t>(k) - 1].XYZ());
    }
    const Curve fitted =
        new Geom_BSplineCurve(poles, strip.weights[index], strip.knots, strip.mults, strip.degree);
    for (int s = strip.knots.Lower(); s < strip.knots.Upper(); ++s) {
      const double a = strip.knots(s);
      const double b = strip.knots(s + 1);
      for (int k = 1; k <= 4; ++k) {
        const double u = a + (b - a) * k / 5.0;
        const gp_Vec want = rows.wanted[j](u);
        const double length = want.Magnitude();
        if (length <= 1.0e-12) {
          continue;
        }
        const gp_Vec got(fitted->Value(u).XYZ());
        worst = std::max(worst, (got - want).Magnitude() / length);
      }
    }
  }
  return worst;
}

// `from` copied into `to`, whatever its length was.
template <typename T>
void copy_into(NCollection_Array1<T>& to, const NCollection_Array1<T>& from) {
  to.Resize(from.Lower(), from.Upper(), false);
  for (int k = from.Lower(); k <= from.Upper(); ++k) {
    to(k) = from(k);
  }
}

// Every knot span of the strip's curves halved (the curves themselves
// unchanged, still compatible), a point section's row grown to match.
void split_spans(Strip& strip) {
  std::vector<double> middles;
  for (int s = strip.knots.Lower(); s < strip.knots.Upper(); ++s) {
    middles.push_back((strip.knots(s) + strip.knots(s + 1)) / 2);
  }
  std::size_t reference = strip.curves.size();
  for (std::size_t i = 0; i < strip.curves.size(); ++i) {
    if (strip.curves[i].IsNull()) {
      continue;
    }
    const Curve curve = Handle(Geom_BSplineCurve)::DownCast(strip.curves[i]->Copy());
    for (const double u : middles) {
      curve->InsertKnot(u, 1, 0.0, true);
    }
    strip.curves[i] = curve;
    copy_into(strip.poles[i], curve->Poles());
    if (const NCollection_Array1<double>* weights = curve->Weights()) {
      copy_into(strip.weights[i], *weights);
    } else {
      strip.weights[i].Resize(1, curve->NbPoles(), false);
      strip.weights[i].Init(1.0);
    }
    if (reference == strip.curves.size()) {
      reference = i;
      strip.degree = curve->Degree();
      copy_into(strip.knots, curve->Knots());
      copy_into(strip.mults, curve->Multiplicities());
    }
  }
  // A point is a section of its neighbour's weights, all at the point.
  for (std::size_t i = 0; i < strip.curves.size(); ++i) {
    if (!strip.curves[i].IsNull() || reference == strip.curves.size()) {
      continue;
    }
    const gp_Pnt tip = strip.poles[i](strip.poles[i].Lower());
    const std::size_t neighbour = i == 0 ? 1 : strip.curves.size() - 2;
    strip.poles[i].Resize(1, strip.poles[reference].Length(), false);
    strip.poles[i].Init(tip);
    copy_into(strip.weights[i], strip.weights[neighbour]);
  }
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

// Sections of several edges each, as many as the first has, turned (and
// reversed) so that every vertex lies as near as possible to the one of the
// section before it: the matching of OCCT takes a section turned a quarter
// round the loft's axis (a square ring) a corner off. Sections of one edge
// (circles) and points are left as they are.
void align_to_previous(std::vector<Matched>& matched) {
  const auto sample = [](const Curve& curve) {
    std::array<gp_Pnt, 3> points;
    const double a = curve->FirstParameter();
    const double b = curve->LastParameter();
    for (int k = 0; k < 3; ++k) {
      points[static_cast<std::size_t>(k)] = curve->Value(a + (b - a) * k / 3.0);
    }
    return points;
  };
  for (std::size_t i = 1; i < matched.size(); ++i) {
    Matched& m = matched[i];
    const Matched& before = matched[i - 1];
    const std::size_t n = m.curves.size();
    // An open chain (mitcad#2345) has ends where it is: not turned round.
    if (m.point || before.point || m.open || before.open || n < 2 || before.curves.size() != n) {
      continue;
    }
    std::vector<Curve> forward = m.curves;
    std::vector<Curve> backward;
    for (std::size_t j = n; j-- > 0;) {
      backward.push_back(Handle(Geom_BSplineCurve)::DownCast(m.curves[j]->Copy()));
      backward.back()->Reverse();
    }
    double best = std::numeric_limits<double>::max();
    double current = 0.0;
    std::size_t best_turn = 0;
    bool best_reversed = false;
    for (const bool reversed : {false, true}) {
      const std::vector<Curve>& curves = reversed ? backward : forward;
      for (std::size_t turn = 0; turn < n; ++turn) {
        double cost = 0.0;
        for (std::size_t j = 0; j < n; ++j) {
          const auto here = sample(curves[(j + turn) % n]);
          const auto there = sample(before.curves[j]);
          for (std::size_t k = 0; k < here.size(); ++k) {
            cost += here[k].Distance(there[k]);
          }
        }
        if (!reversed && turn == 0) {
          current = cost;
        }
        if (cost < best) {
          best = cost;
          best_turn = turn;
          best_reversed = reversed;
        }
      }
    }
    // Only a clear improvement moves what the matching chose.
    if (best >= current * 0.999 || (!best_reversed && best_turn == 0)) {
      continue;
    }
    std::vector<Curve> curves;
    std::vector<TopoDS_Edge> edges;
    std::vector<TopoDS_Edge> origins;
    for (std::size_t j = 0; j < n; ++j) {
      const std::size_t from = (j + best_turn) % n;
      curves.push_back((best_reversed ? backward : forward)[from]);
      const std::size_t source = best_reversed ? n - 1 - from : from;
      edges.push_back(best_reversed ? TopoDS::Edge(m.edges[source].Reversed()) : m.edges[source]);
      origins.push_back(m.origins[source]);
    }
    m.curves = std::move(curves);
    m.edges = std::move(edges);
    m.origins = std::move(origins);
  }
}

// --- Smooth seams of lofts through two sections (mitcad#2141, mitcad#2143)
//
// Between two sections OCCT's through-sections rules a face across each
// pair of matched edges, each edge at its own parameter: along the line
// between two matched vertices a face's tangent plane is spanned by that
// line and (1 - v) A' + v B', A' and B' the derivatives of its two edges
// at the vertices. Where both sections run on smoothly through their
// vertices (a circle cut where a slot's arcs meet its straight sides), the
// two faces beside the line share it and their tangent planes at its ends;
// between the ends they are tangent only where their A' and B' stand in
// the same ratio. A circle's long piece matched to a slot's straight side
// and the next, short piece to the slot's half circle stand in ratios
// about twice apart: the faces meet at a crease that grows from nothing at
// the sections to some ten degrees between them. An offset of the faces (a
// shell, a thickened surface loft) has to intersect the two offset faces
// along it, a curve that runs out tangentially at both ends, and the wall
// did not close ("the offset faces do not close up", "the thickened
// surface does not close").
//
// Here each face's two curves are eased (their parameters run faster or
// slower towards an end, the curves themselves unchanged) so that on both
// sides of such a line A' and B' stand in the same ratio: the faces are
// tangent all along it. The faces of a chain joined so are pieces of one
// surface, periodic where the chain runs round the whole section
// (match_seams, skinned_loft).

// Below this angle between the derivatives of two edges at their common
// vertex a section runs on smoothly through it (radians).
constexpr double kSmoothSection = 1.0e-4;

// Below this crease two faces count as tangent along their common line
// (radians).
constexpr double kTangentSeam = 1.0e-6;

// The faces' curves lie within this distance of the sections' edges (mm),
// as OCCT's own polynomial forms of arcs in a loft do (GeomFill_Profiler).
constexpr double kSeamFit = 1.0e-7;

// Below this angle between the derivatives of two edges at their common
// vertex the faces beside the line there are pieces of one surface
// (radians; match_seams).
constexpr double kChainAngle = 1.0e-8;

// The point and the first two derivatives at x of [0, 1] of an edge as it
// runs in its wire: its curve's parameter range scaled to [0, 1].
void edge_d2(const BRepAdaptor_Curve& curve, bool reversed, double x, gp_Pnt& p, gp_Vec& d1, gp_Vec& d2) {
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const double span = last - first;
  curve.D2(reversed ? last - x * span : first + x * span, p, d1, d2);
  d1 *= reversed ? -span : span;
  d2 *= span * span;
}

// The cubic taking [0, 1] onto itself that leaves 0 with the slope `start`
// and arrives at 1 with `end`; increasing for slopes up to 3 (Fritsch and
// Carlson). Slopes of 1 leave the parameter as it is.
struct Easing {
  double start = 1.0;
  double end = 1.0;

  // Its value and first two derivatives at s.
  std::array<double, 3> at(double s) const {
    const double a = start + end - 2.0;
    const double b = 3.0 - 2.0 * start - end;
    return {((a * s + b) * s + start) * s, (3.0 * a * s + 2.0 * b) * s + start, 6.0 * a * s + 2.0 * b};
  }
};

// The edge at the eased parameter, with the first two derivatives.
void eased_d2(const BRepAdaptor_Curve& curve, bool reversed, const Easing& easing, double s, gp_Pnt& p, gp_Vec& d1,
              gp_Vec& d2) {
  const std::array<double, 3> h = easing.at(s);
  gp_Vec c1;
  gp_Vec c2;
  edge_d2(curve, reversed, std::clamp(h[0], 0.0, 1.0), p, c1, c2);
  d1 = c1 * h[1];
  d2 = c2 * (h[1] * h[1]) + c1 * h[2];
}

// A face's two edges, each at its eased parameter, as polynomial B-splines
// over [0, 1] in one basis: interpolated at evenly spaced parameters with
// their exact first and second derivatives at the ends, the spacing halved
// until each lies within kSeamFit of its edge between them. Empty when 512
// spans do not reach that.
std::optional<std::array<Curve, 2>> eased_curves(const std::array<TopoDS_Edge, 2>& edges,
                                                 const std::array<Easing, 2>& easings) {
  const BRepAdaptor_Curve curve0(edges[0]);
  const BRepAdaptor_Curve curve1(edges[1]);
  const std::array<const BRepAdaptor_Curve*, 2> curves{&curve0, &curve1};
  for (int spans = 8; spans <= 512; spans *= 2) {
    std::vector<double> parameters;
    for (int k = 0; k <= spans; ++k) {
      parameters.push_back(static_cast<double>(k) / spans);
    }
    std::array<Curve, 2> fitted;
    bool close = true;
    for (std::size_t i = 0; i < 2 && close; ++i) {
      const bool reversed = edges[i].Orientation() == TopAbs_REVERSED;
      const auto at = [&](double s, gp_Pnt& p, gp_Vec& d1, gp_Vec& d2) {
        eased_d2(*curves[i], reversed, easings[i], s, p, d1, d2);
      };
      gp_Pnt p;
      gp_Vec d1;
      gp_Vec d2;
      std::vector<gp_Pnt> points;
      for (const double s : parameters) {
        at(s, p, d1, d2);
        points.push_back(p);
      }
      EndDerivatives leaving;
      EndDerivatives arriving;
      at(0.0, p, d1, d2);
      leaving.first = d1;
      leaving.second = d2;
      at(1.0, p, d1, d2);
      arriving.first = d1;
      arriving.second = d2;
      fitted[i] = interpolate_with_ends(points, parameters, leaving, arriving);
      for (int k = 0; k < spans && close; ++k) {
        for (const double part : {0.25, 0.5, 0.75}) {
          const double s = (k + part) / spans;
          at(s, p, d1, d2);
          close = close && fitted[i]->Value(s).Distance(p) <= kSeamFit;
        }
      }
    }
    if (close) {
      return fitted;
    }
  }
  return std::nullopt;
}

// The side faces of a loft through two sections as pieces of surfaces
// that each run through a chain of them (match_seams): per chain the two
// sections' curves over one parameter, per face its chain and its range
// of that parameter.
struct SeamChains {
  std::vector<Curve> first;
  std::vector<Curve> second;
  std::vector<std::size_t> chain_of;
  std::vector<std::pair<double, double>> range;
  bool round = false; // one chain round the whole section, closing smoothly
};

// The B-spline's parameter moved and scaled onto [from, from + span].
Curve spanned(const Curve& curve, double from, double span) {
  Curve moved = Handle(Geom_BSplineCurve)::DownCast(curve->Copy());
  NCollection_Array1<double> knots(moved->Knots());
  const double a = knots(knots.Lower());
  const double b = knots(knots.Upper());
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    knots(i) = from + (knots(i) - a) / (b - a) * span;
  }
  moved->SetKnots(knots);
  return moved;
}

// Two sections' faces made tangent along every line between matched
// vertices through which both sections run on smoothly (above), as pieces
// of one surface for each chain of faces joined so. Each face gets a span
// of the chain's parameter, the geometric mean of its two edges' lengths,
// and its edges are eased so that each section's curve runs on with the
// same derivative (by that parameter) across every such line: the chain's
// curves, and the ruled surface between them, are continuous in their
// first derivative there, the faces tangent. (Faces on surfaces of their
// own, only tangent, were not enough: the geometry kernel's offset extends
// each face's surface by the length of its own first ruling, the shared
// edge's curves on the two extended offsets then cover different ranges,
// and the curve it projects again for one of them came out short of the
// edge's range, an edge of an invalid range on an unorientable face, at
// every thickness. On one surface the faces' extensions are the same.)
// The faces of a chain are made on copies of its surface, so that their
// common edges are not taken for seams of one face. A chain runs round
// the whole section where every line is joined so; a corner of a section
// (where the faces meet at an angle anyway), or a line whose easings
// would turn back (slopes beyond 3), ends one. Empty, the loft left to
// OCCT's through-sections as before: no line between faces meeting at a
// crease is joined, or the curves could not be fitted or joined.
std::optional<SeamChains> match_seams(const std::vector<Matched>& matched, double tolerance) {
  if (matched.size() != 2 || matched[0].point || matched[1].point) {
    return std::nullopt;
  }
  const std::size_t n = matched[0].edges.size();
  if (n == 0 || matched[1].edges.size() != n) {
    return std::nullopt;
  }
  constexpr double kTiny = 1.0e-12;
  // Each edge's ends: the points, the derivative leaving its start and the
  // one arriving at its end; its length.
  struct Ends {
    gp_Pnt start;
    gp_Pnt end;
    gp_Vec leaving;
    gp_Vec arriving;
    double length = 0.0;
  };
  std::array<std::vector<Ends>, 2> ends;
  for (std::size_t i = 0; i < 2; ++i) {
    for (const TopoDS_Edge& edge : matched[i].edges) {
      const BRepAdaptor_Curve curve(edge);
      const bool reversed = edge.Orientation() == TopAbs_REVERSED;
      Ends e;
      gp_Vec second;
      edge_d2(curve, reversed, 0.0, e.start, e.leaving, second);
      edge_d2(curve, reversed, 1.0, e.end, e.arriving, second);
      e.length = GCPnts_AbscissaPoint::Length(curve);
      ends[i].push_back(e);
    }
  }
  // Each face's span of its chain's parameter.
  std::vector<double> spans(n);
  for (std::size_t j = 0; j < n; ++j) {
    spans[j] = std::sqrt(ends[0][j].length * ends[1][j].length);
    if (!(spans[j] > kTiny)) {
      return std::nullopt;
    }
  }
  // Per face, the first section's easing and the second's; per line (the
  // one where face k starts), whether the faces beside it are joined.
  std::vector<std::array<Easing, 2>> easings(n);
  std::vector<bool> joined(n, false);
  bool needed = false;
  for (std::size_t k = 0; k < n; ++k) {
    // The line where face k starts and the face before it round the
    // sections ends.
    const std::size_t before = (k + n - 1) % n;
    const Ends& a0 = ends[0][before];
    const Ends& a1 = ends[0][k];
    const Ends& b0 = ends[1][before];
    const Ends& b1 = ends[1][k];
    if (a0.end.Distance(a1.start) > tolerance || b0.end.Distance(b1.start) > tolerance) {
      continue; // open sections: no face before the first
    }
    const gp_Vec& a_in = a0.arriving;
    const gp_Vec& a_out = a1.leaving;
    const gp_Vec& b_in = b0.arriving;
    const gp_Vec& b_out = b1.leaving;
    if (a_in.Magnitude() <= kTiny || a_out.Magnitude() <= kTiny || b_in.Magnitude() <= kTiny ||
        b_out.Magnitude() <= kTiny) {
      continue;
    }
    if (a_in.Angle(a_out) > kSmoothSection || b_in.Angle(b_out) > kSmoothSection) {
      continue; // a corner: the faces meet at an angle there anyway
    }
    const gp_Vec line(a1.start, b1.start);
    if (line.Magnitude() <= tolerance) {
      continue;
    }
    double crease = 0.0;
    for (const double v : {0.25, 0.5, 0.75}) {
      const gp_Vec normal_before = line.Crossed(a_in * (1.0 - v) + b_in * v);
      const gp_Vec normal_after = line.Crossed(a_out * (1.0 - v) + b_out * v);
      if (normal_before.Magnitude() > kTiny && normal_after.Magnitude() > kTiny) {
        crease = std::max(crease, normal_before.Angle(normal_after));
      }
    }
    // Each section's derivative by the chain's parameter, arriving with
    // the slope g and leaving with 1 / g: the same on both sides when g^2
    // is the ratio of the two without easing.
    const double g_first = std::sqrt((a_out.Magnitude() / spans[k]) / (a_in.Magnitude() / spans[before]));
    const double g_second = std::sqrt((b_out.Magnitude() / spans[k]) / (b_in.Magnitude() / spans[before]));
    const auto increasing = [](double g) { return g >= 1.0 / 3.0 && g <= 3.0; };
    if (!increasing(g_first) || !increasing(g_second)) {
      continue;
    }
    easings[before][0].end = g_first;
    easings[k][0].start = 1.0 / g_first;
    easings[before][1].end = g_second;
    easings[k][1].start = 1.0 / g_second;
    // One surface across the line only where both sections run on through
    // it within kChainAngle (the joined curves' knot there is removed
    // within kJoin); elsewhere the faces stay tangent on surfaces of their
    // own.
    joined[k] = a_in.Angle(a_out) <= kChainAngle && b_in.Angle(b_out) <= kChainAngle;
    needed = needed || crease > kTangentSeam;
  }
  if (!needed) {
    return std::nullopt;
  }
  std::vector<std::array<Curve, 2>> pieces;
  for (std::size_t j = 0; j < n; ++j) {
    const std::optional<std::array<Curve, 2>> fitted =
        eased_curves({matched[0].edges[j], matched[1].edges[j]}, easings[j]);
    if (!fitted) {
      return std::nullopt;
    }
    pieces.push_back(*fitted);
  }
  // The chains: from a line that is not joined, or round the whole
  // section from face 0.
  std::size_t first_face = 0;
  bool round = true;
  for (std::size_t k = 0; k < n; ++k) {
    if (!joined[k]) {
      first_face = k;
      round = false;
      break;
    }
  }
  // The pieces' ends meet within this distance (mm).
  constexpr double kJoin = 1.0e-6;
  SeamChains chains;
  chains.round = round;
  chains.chain_of.assign(n, 0);
  chains.range.assign(n, {0.0, 0.0});
  std::optional<GeomConvert_CompCurveToBSplineCurve> joining[2];
  std::size_t chain_faces = 0;
  const auto finish = [&]() {
    if (!joining[0]) {
      return true;
    }
    const Curve a = joining[0]->BSplineCurve();
    const Curve b = joining[1]->BSplineCurve();
    // Joined pieces keep the knot between them only once less than the
    // degree: continuous in the first derivative.
    if (chain_faces > 1 && (a->Continuity() < GeomAbs_C1 || b->Continuity() < GeomAbs_C1)) {
      return false;
    }
    chains.first.push_back(a);
    chains.second.push_back(b);
    joining[0].reset();
    joining[1].reset();
    return true;
  };
  double at = 0.0;
  for (std::size_t step = 0; step < n; ++step) {
    const std::size_t j = (first_face + step) % n;
    if (step > 0 && !joined[j] && !finish()) {
      return std::nullopt;
    }
    if (!joining[0]) {
      at = 0.0;
      chain_faces = 0;
      joining[0].emplace(spanned(pieces[j][0], at, spans[j]));
      joining[1].emplace(spanned(pieces[j][1], at, spans[j]));
    } else if (!joining[0]->Add(spanned(pieces[j][0], at, spans[j]), kJoin, true, false, 4) ||
               !joining[1]->Add(spanned(pieces[j][1], at, spans[j]), kJoin, true, false, 4)) {
      return std::nullopt;
    }
    chains.chain_of[j] = chains.first.size();
    chains.range[j] = {at, at + spans[j]};
    at += spans[j];
    ++chain_faces;
  }
  if (!finish()) {
    return std::nullopt;
  }
  return chains;
}

// --- Matching from the rails (mitcad#1944)

double curve_length(const Curve& curve) { return GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(curve)); }

// The curve running at one speed along it: as it is where it does (a
// line, a ring of merged tangent edges, loft_merge), else a polynomial
// B-spline within kJoinTolerance of it by arc length over [0, its length],
// its ends exactly the curve's. With rails every section's piece of a face
// runs so (mitcad#2320): the face relates the sections' points by the
// share of the way along their pieces, and the rails' bends by arc length
// round the first section, whatever the curves' own parameters (a whole
// circle's rational form runs unevenly round it from wherever it starts,
// so that mirrored sections met each other at points that were not
// mirrored). A curve that cannot be so approximated within the tolerance
// is left as it is.
Curve by_arc_length(const Curve& curve) {
  const double first = curve->FirstParameter();
  const double last = curve->LastParameter();
  double slowest = std::numeric_limits<double>::max();
  double fastest = 0.0;
  for (int k = 0; k <= 32; ++k) {
    const double speed = curve->DN(first + (last - first) * k / 32, 1).Magnitude();
    slowest = std::min(slowest, speed);
    fastest = std::max(fastest, speed);
  }
  if (!(slowest > 0.0) || fastest - slowest <= 1.0e-4 * fastest) {
    return curve;
  }
  const occ::handle<Adaptor3d_Curve> along = new GeomAdaptor_Curve(curve);
  const double length = GCPnts_AbscissaPoint::Length(*along);
  if (!(length > Precision::Confusion())) {
    return curve;
  }
  const Approx_CurvilinearParameter approximation(along, kJoinTolerance, GeomAbs_C2, 9, 1000);
  if (!approximation.HasResult() || approximation.MaxError3d() > 10 * kJoinTolerance) {
    return curve;
  }
  Curve result = approximation.Curve3d();
  NCollection_Array1<double> knots(result->Knots());
  const double from = knots(knots.Lower());
  const double to = knots(knots.Upper());
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    knots(i) = (knots(i) - from) / (to - from) * length;
  }
  result->SetKnots(knots);
  result->SetPole(1, curve->Value(first));
  result->SetPole(result->NbPoles(), curve->Value(last));
  return result;
}

// The parameter `length` along the curve from its start.
double at_length(const Curve& curve, double length) {
  const GeomAdaptor_Curve adaptor(curve);
  GCPnts_AbscissaPoint point(adaptor, length, curve->FirstParameter());
  if (!point.IsDone()) {
    throw std::runtime_error("the sections could not be matched along their rails");
  }
  return std::clamp(point.Parameter(), curve->FirstParameter(), curve->LastParameter());
}

// The sections matched from their rails, where OCCT's matching does not
// keep the rails at corresponding vertices (a circle to a circle cut by a
// chord, a circle to a rectangle with rails meeting its edges' middles):
// every section runs the same way round the loft's axis and starts at rail
// 1's hit point; between each pair of neighbouring rails the sections'
// edges are cut by arc length until they have as many edges, each over the
// same share of the way from one rail to the next. So the rails run
// through corresponding vertices whatever the sections' own edges. `split`
// are the sections' wires split at the rails' hits, `origins` their edges'
// source edges; `wires` gets each section's wire of the matched edges (for
// its cap), and each rail its vertex.
std::vector<Matched> match_along_rails(const std::vector<SkinSection>& sections,
                                       const NCollection_Sequence<TopoDS_Shape>& split,
                                       const std::vector<std::vector<std::pair<TopoDS_Edge, TopoDS_Edge>>>& origins,
                                       std::vector<Rail>& rails, double tolerance, std::vector<TopoDS_Wire>& wires) {
  const std::size_t count = sections.size();
  const std::size_t rail_count = rails.size();
  struct Ring {
    std::vector<Curve> curves;
    std::vector<TopoDS_Edge> sources;
    std::vector<std::size_t> at; // per rail, the curve it starts
  };
  gp_Vec axis(centre_of(sections.front().shape), centre_of(sections.back().shape));
  if (axis.Magnitude() <= tolerance) {
    axis = sections.front().normal ? gp_Vec(*sections.front().normal) : gp_Vec(0, 0, 1);
  }
  std::vector<Ring> rings(count);
  std::vector<std::size_t> order; // the rails in their order round the first section
  double sense = 0.0;
  for (std::size_t i = 0; i < count; ++i) {
    Ring& ring = rings[i];
    for (const TopoDS_Edge& edge : wire_edges(TopoDS::Wire(split.Value(static_cast<int>(i) + 1)))) {
      ring.curves.push_back(edge_curve(edge));
      TopoDS_Edge source = edge;
      for (const auto& [piece, before] : origins[i]) {
        if (piece.IsSame(edge)) {
          source = before;
          break;
        }
      }
      ring.sources.push_back(source);
    }
    // The same way round the axis as the first section.
    const double turn = turning(ring.curves, axis, centre_of(sections[i].shape));
    if (i == 0) {
      sense = turn;
    } else if (turn != sense) {
      std::reverse(ring.curves.begin(), ring.curves.end());
      std::reverse(ring.sources.begin(), ring.sources.end());
      for (Curve& curve : ring.curves) {
        curve->Reverse();
      }
    }
    const auto find = [&](const gp_Pnt& hit) {
      std::size_t found = ring.curves.size();
      double nearest = 2 * tolerance;
      for (std::size_t j = 0; j < ring.curves.size(); ++j) {
        const Curve& curve = ring.curves[j];
        const double d = curve->Value(curve->FirstParameter()).Distance(hit);
        if (d <= nearest) {
          nearest = d;
          found = j;
        }
      }
      if (found == ring.curves.size()) {
        throw std::runtime_error("the sections could not be matched along their rails");
      }
      return found;
    };
    // From rail 1's hit.
    const std::size_t start = find(rails.front().hits[i]);
    std::rotate(ring.curves.begin(), ring.curves.begin() + static_cast<std::ptrdiff_t>(start), ring.curves.end());
    std::rotate(ring.sources.begin(), ring.sources.begin() + static_cast<std::ptrdiff_t>(start),
                ring.sources.end());
    for (const Rail& rail : rails) {
      ring.at.push_back(find(rail.hits[i]));
    }
    for (std::size_t r = 0; r < rail_count; ++r) {
      for (std::size_t s = 0; s < r; ++s) {
        if (ring.at[s] == ring.at[r]) {
          throw std::invalid_argument("rails " + std::to_string(s + 1) + " and " + std::to_string(r + 1) +
                                      " meet the sections at the same points");
        }
      }
    }
    std::vector<std::size_t> here(rail_count);
    for (std::size_t r = 0; r < rail_count; ++r) {
      here[r] = r;
    }
    std::sort(here.begin(), here.end(), [&](std::size_t a, std::size_t b) { return ring.at[a] < ring.at[b]; });
    if (i == 0) {
      order = here;
    } else if (here != order) {
      throw std::invalid_argument(placed_error(
          "the rails meet " + profile_words(i, count) + " in another order round it than the first profile",
          "the rails meet section " + std::to_string(i + 1) + " in another order round it than section 1"));
    }
  }

  std::vector<Matched> matched(count);
  for (std::size_t i = 0; i < count; ++i) {
    matched[i].centre = centre_of(sections[i].shape);
  }
  // Between each pair of neighbouring rails (one rail: all the way round).
  for (std::size_t s = 0; s < rail_count; ++s) {
    rails[order[s]].vertex = static_cast<int>(matched.front().curves.size());
    // Each section's edges there, their lengths, and where they end as
    // shares of the way.
    struct Break {
      double share;
      std::size_t section;
      double length; // from the rail, along the section
    };
    std::vector<std::vector<double>> lengths(count);
    std::vector<double> totals(count, 0.0);
    std::vector<Break> breaks;
    for (std::size_t i = 0; i < count; ++i) {
      const Ring& ring = rings[i];
      const std::size_t begin = ring.at[order[s]];
      const std::size_t end = s + 1 < rail_count ? ring.at[order[s + 1]] : ring.curves.size();
      for (std::size_t j = begin; j < end; ++j) {
        lengths[i].push_back(curve_length(ring.curves[j]));
        totals[i] += lengths[i].back();
        if (j + 1 < end) {
          breaks.push_back({0.0, i, totals[i]});
        }
      }
      if (totals[i] <= 0.0) {
        throw std::runtime_error("the sections could not be matched along their rails");
      }
    }
    for (Break& b : breaks) {
      b.share = b.length / totals[b.section];
    }
    std::sort(breaks.begin(), breaks.end(), [](const Break& a, const Break& b) { return a.share < b.share; });
    // Breaks of different sections at nearly the same share are one vertex
    // of the matched sections (no sliver faces between them).
    std::vector<std::vector<Break>> clusters;
    for (const Break& b : breaks) {
      const bool joins = !clusters.empty() && b.share - clusters.back().front().share <= 1.0e-3 &&
                         std::none_of(clusters.back().begin(), clusters.back().end(),
                                      [&](const Break& c) { return c.section == b.section; });
      if (joins) {
        clusters.back().push_back(b);
      } else {
        clusters.push_back({b});
      }
    }
    for (std::size_t i = 0; i < count; ++i) {
      const Ring& ring = rings[i];
      const std::size_t begin = ring.at[order[s]];
      // The cuts along this section: its own vertices, and the other
      // sections' at their mean share.
      std::vector<double> cuts;
      for (const std::vector<Break>& cluster : clusters) {
        const auto own =
            std::find_if(cluster.begin(), cluster.end(), [&](const Break& b) { return b.section == i; });
        if (own != cluster.end()) {
          cuts.push_back(own->length);
        } else {
          double share = 0.0;
          for (const Break& b : cluster) {
            share += b.share;
          }
          cuts.push_back(share / static_cast<double>(cluster.size()) * totals[i]);
        }
      }
      cuts.push_back(totals[i]);
      const double slack = 1.0e-9 * totals[i];
      double from = 0.0;
      for (const double to : cuts) {
        // The piece lies within one edge: every edge's end is a cut.
        double edge_start = 0.0;
        bool done = false;
        for (std::size_t k = 0; k < lengths[i].size() && !done; ++k) {
          const double edge_end = edge_start + lengths[i][k];
          if (from >= edge_start - slack && to <= edge_end + slack) {
            const Curve& curve = ring.curves[begin + k];
            const double u0 =
                from - edge_start <= slack ? curve->FirstParameter() : at_length(curve, from - edge_start);
            const double u1 = edge_end - to <= slack ? curve->LastParameter() : at_length(curve, to - edge_start);
            Curve piece = Handle(Geom_BSplineCurve)::DownCast(curve->Copy());
            if (u0 > curve->FirstParameter() || u1 < curve->LastParameter()) {
              if (u1 - u0 <= Precision::PConfusion()) {
                throw std::runtime_error("the sections could not be matched along their rails");
              }
              piece->Segment(u0, u1);
            }
            matched[i].curves.push_back(piece);
            matched[i].origins.push_back(ring.sources[begin + k]);
            done = true;
          }
          edge_start = edge_end;
        }
        if (!done) {
          throw std::runtime_error("the sections could not be matched along their rails");
        }
        from = to;
      }
    }
    for (std::size_t i = 1; i < count; ++i) {
      if (matched[i].curves.size() != matched.front().curves.size()) {
        throw std::runtime_error("the sections could not be matched edge for edge");
      }
    }
  }

  // Each section's edges, joined at shared vertices into its wire.
  wires.assign(count, TopoDS_Wire());
  BRep_Builder builder;
  for (std::size_t i = 0; i < count; ++i) {
    Matched& m = matched[i];
    const std::size_t n = m.curves.size();
    std::vector<TopoDS_Vertex> vertices(n);
    for (std::size_t k = 0; k < n; ++k) {
      const gp_Pnt p = m.curves[k]->Value(m.curves[k]->FirstParameter());
      const Curve& before = m.curves[(k + n - 1) % n];
      const double gap = before->Value(before->LastParameter()).Distance(p);
      builder.MakeVertex(vertices[k], p, std::max(Precision::Confusion(), 2 * gap));
    }
    builder.MakeWire(wires[i]);
    for (std::size_t k = 0; k < n; ++k) {
      const Curve& curve = m.curves[k];
      BRepBuilderAPI_MakeEdge edge(curve, vertices[k], vertices[(k + 1) % n], curve->FirstParameter(),
                                   curve->LastParameter());
      if (!edge.IsDone()) {
        throw std::runtime_error("the sections could not be matched along their rails");
      }
      m.edges.push_back(edge.Edge());
      builder.Add(wires[i], edge.Edge());
    }
    wires[i].Closed(true);
  }
  return matched;
}

// A side surface that closes on itself round the sections, continuous in
// its first derivative where it closes (within 1e-6 mm), made periodic
// there, its knot where it closes kept once less than its degree (twice
// less where it is continuous in its second derivative there too); as it
// was where it does not close, or closes with a crease. The kernel's
// offset takes a closed surface so: a clamped one that only closes on
// itself was intersected wrongly where it closes (mitcad#2141: a rim's
// wire crossing itself, Round, the other end open, and the closed surface
// not offset at all; mitcad#2271: a merged circle-to-slot loft not shelled
// Inside). Whatever the sections, as long as the side is one surface all
// round them.
Surface periodic_where_closed(const Surface& surface) {
  constexpr double kClosing = 1.0e-6;
  if (surface->IsUPeriodic() || surface->UDegree() < 2) {
    return surface;
  }
  // Its first and last rows of poles (and weights) the same within
  // kClosing: the pieces of a chain round the section meet within that.
  const int last = surface->NbUPoles();
  for (int l = 1; l <= surface->NbVPoles(); ++l) {
    if (surface->Pole(1, l).Distance(surface->Pole(last, l)) > kClosing ||
        std::abs(surface->Weight(1, l) - surface->Weight(last, l)) > 1.0e-12 * std::abs(surface->Weight(1, l))) {
      return surface;
    }
  }
  const Surface periodic = Handle(Geom_BSplineSurface)::DownCast(surface->Copy());
  periodic->SetUPeriodic();
  const int closing = periodic->FirstUKnotIndex();
  const int smooth = periodic->UDegree() - 1;
  if (periodic->UMultiplicity(closing) > smooth) {
    periodic->RemoveUKnot(closing, smooth, kClosing);
  }
  if (periodic->UMultiplicity(closing) > smooth) {
    return surface;
  }
  // (mitcad#2308) Continuous in its second derivative where it closes too
  // (a ring smooth all round, as a merged circle or spline outline is: by
  // arc length, C2 along each former edge): the knot there once less
  // again, where that moves the surface by no more than the kernel's
  // confusion distance. Kept C1 there, the side's offset (a symmetric
  // Thicken's half wall) was C0 along its seam alone, which the kernel's
  // offset refuses (BRepOffset_C0Geometry) and which can neither be split
  // at nor cut out of the face's own range: the merged circle to spline
  // outline, its knot there 8 times at degree 9, came out once more within
  // 1e-7 mm.
  if (smooth >= 2) {
    periodic->RemoveUKnot(closing, smooth - 1, Precision::Confusion());
  }
  return periodic;
}

// (mitcad#2308) The surface with each knot inside its U range at which it
// is continuous in its first derivative only by its knot (once less than
// its degree) kept once less again where it is continuous in its second
// derivative there too: where that moves the surface by no more than the
// kernel's confusion distance. A merged ring (mitcad#2162) is cut where it
// is to start and joined again C1 there (joined_curve keeps every join
// once less than the degree), though a smooth edge cut in two is as smooth
// there as anywhere: the merged circle to spline outline's side, degree 9,
// had such a knot 8 times, C1, so its offset (a symmetric Thicken's half
// wall) was C0, which the kernel's offset refuses (BRepOffset_C0Geometry);
// that knot came out once more within 1e-7 mm, and the side so smoothed
// was offset by half the thickness and that again with thickening. A join
// of a line and an arc (a slot's), where the curvature jumps, does not
// come out and stays C1.
Surface smooth_where_c2(const Surface& surface) {
  const int degree = surface->UDegree();
  if (degree < 3) {
    return surface;
  }
  Surface smoothed;
  for (int k = surface->FirstUKnotIndex() + 1; k < surface->LastUKnotIndex(); ++k) {
    const Surface& current = smoothed.IsNull() ? surface : smoothed;
    if (current->UMultiplicity(k) != degree - 1) {
      continue;
    }
    const Surface trial = Handle(Geom_BSplineSurface)::DownCast(current->Copy());
    if (trial->RemoveUKnot(k, degree - 2, Precision::Confusion())) {
      smoothed = trial;
    }
  }
  return smoothed.IsNull() ? surface : smoothed;
}

// The side faces' surfaces at the loft's parameters `v`: each edge
// index's curves made compatible (polynomial ones when `plain`), its pole
// rows interpolated in homogeneous coordinates with the derivatives the
// ends ask for. At a conditioned end the weights stay still, so the
// derivatives carry over to the rational surface. `railed`: a loft along
// rails (blended between neighbouring sections, mitcad#2533). `start_speeds`
// and `end_speeds`: how fast Perpendicular ends leave along rails
// (mitcad#2377).
std::vector<Surface> skin_surfaces(const std::vector<Matched>& matched, const std::vector<SkinSection>& sections,
                                   const LoftEnd& start, const LoftEnd& end, const std::vector<double>& v,
                                   bool plain, bool closed, bool railed, const Speeds& start_speeds = Speeds(),
                                   const Speeds& end_speeds = Speeds()) {
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
            return skin_surfaces(matched, sections, start, end, v, true, closed, railed, start_speeds, end_speeds);
          }
        }
      }
    }
  }

  EndRows starts = end_rows(start, sections.front(), matched, strips, 0, 1, v[1] - v[0], "start", start_speeds);
  EndRows ends = end_rows(end, sections.back(), matched, strips, count - 1, count - 2, v[count - 1] - v[count - 2],
                          "end", end_speeds);
  // A Perpendicular end on a face (mitcad#2345): the rows hold the face's
  // normals, which are no image of the section's curves, only at their
  // fitting points; the curves' knot spans halved until they hold them
  // within kPerpendicularFit everywhere, so that the surface meets the face
  // at right angles within 1e-6 rad all along the section.
  constexpr double kPerpendicularFit = 1.0e-7;
  for (int round = 0;
       fit_error(starts, strips, 0) > kPerpendicularFit || fit_error(ends, strips, count - 1) > kPerpendicularFit;
       ++round) {
    if (round == 8) {
      throw std::runtime_error("the loft could not be made to leave the face at right angles all along the section");
    }
    for (Strip& strip : strips) {
      split_spans(strip);
    }
    starts = end_rows(start, sections.front(), matched, strips, 0, 1, v[1] - v[0], "start", start_speeds);
    ends = end_rows(end, sections.back(), matched, strips, count - 1, count - 2, v[count - 1] - v[count - 2], "end",
                end_speeds);
  }
  if (count == 2 && starts.order > 0 && ends.order == 0) {
    ends = far_end(starts, start, strips, 0, 1, v[1] - v[0]);
  } else if (count == 2 && ends.order > 0 && starts.order == 0) {
    starts = far_end(ends, end, strips, 1, 0, v[1] - v[0]);
  }
  // Free ends (no end conditions, no rails) through three or more sections
  // and closed lofts: the stored lofts' piecewise cubic Hermite form. Along
  // rails through three or more sections (mitcad#2533): the blend between
  // neighbouring sections (BlendInterpolation), each place along the loft a
  // blend of the two sections on either side of it with weights that are
  // never negative, the ends' derivatives at the ends. One interpolation
  // through all the sections gave the far sections negative weights
  // between the near ones: through three sections one quadratic, whose
  // weight of the circle of GitHub mitcad/Mitcad#607's design is -0.125
  // halfway between its rectangles, so beside a rail, where the circle
  // curves in and the rectangles run straight, the side stood out beyond
  // the rail by up to 0.108 mm. Blended, each section's outermost point at
  // a rail keeps the rail's vertex line outermost between the sections
  // too. The blend stays still at the inner sections (no blend through
  // three sections that keeps every weight from going negative can move
  // there and stay smooth); the rails' bends carry the loft on through
  // them, so the bent surface runs on smoothly (as each rail does).
  // Otherwise the interpolation with the ends' derivatives.
  const bool hermite = closed || (!railed && count >= 3 && starts.order == 0 && ends.order == 0);
  const bool blended = !hermite && railed && count >= 3;
  std::optional<EndInterpolation> derived;
  std::optional<HermiteInterpolation> piecewise;
  std::optional<BlendInterpolation> blend;
  if (hermite) {
    piecewise.emplace(v, closed);
  } else if (blended) {
    blend.emplace(v, starts.order, ends.order);
  } else {
    derived.emplace(v, starts.order, ends.order);
  }
  const int v_degree = hermite ? piecewise->degree() : blended ? blend->degree() : derived->degree();
  const int columns = hermite ? piecewise->pole_count() : blended ? blend->pole_count() : derived->pole_count();
  const NCollection_Array1<double>& v_knots =
      hermite ? piecewise->knots() : blended ? blend->knots() : derived->knots();
  const NCollection_Array1<int>& v_mults =
      hermite ? piecewise->multiplicities() : blended ? blend->multiplicities() : derived->multiplicities();
  std::vector<Surface> surfaces;
  for (std::size_t j = 0; j < edge_count; ++j) {
    const Strip& strip = strips[j];
    const int rows = strip.pole_count();
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
        if (hermite) {
          solved[static_cast<std::size_t>(c)] = piecewise->solve(values);
        } else if (blended) {
          solved[static_cast<std::size_t>(c)] =
              blend->solve(values, derivatives(starts, 0), derivatives(ends, count - 1));
        } else {
          solved[static_cast<std::size_t>(c)] =
              derived->solve(values, derivatives(starts, 0), derivatives(ends, count - 1));
        }
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
      surfaces.push_back(new Geom_BSplineSurface(net, weights, strip.knots, v_knots, strip.mults, v_mults,
                                                 strip.degree, v_degree, false, closed));
    } else {
      surfaces.push_back(new Geom_BSplineSurface(net, strip.knots, v_knots, strip.mults, v_mults, strip.degree,
                                                 v_degree, false, closed));
    }
  }
  return surfaces;
}

} // namespace

ShapePtr skinned_loft(const std::vector<SkinSection>& sections, const LoftEnd& start, const LoftEnd& end,
                      const std::vector<Path>& rail_paths, const NamedWire& named, const NameList& first,
                      const NameList& last, bool closed, bool solid, bool seams_only, bool matched_already) {
  const std::size_t count = sections.size();
  if (seams_only &&
      (count != 2 || closed || !rail_paths.empty() || sections.front().point || sections.back().point ||
       start.kind != LoftEnd::Kind::Free || end.kind != LoftEnd::Kind::Free)) {
    return nullptr;
  }
  check_end(start, sections.front(), "start");
  check_end(end, sections.back(), "end");
  if (!rail_paths.empty() && (sections.front().point || sections.back().point)) {
    throw std::invalid_argument("unsupported: loft rails with a point section");
  }
  if (closed && (!rail_paths.empty() || count < 3)) {
    throw std::invalid_argument("a closed loft through two sections or along rails is not supported");
  }
  // Open chains of curves (mitcad#2345), a surface's sections all open: no
  // corners at their ends, never turned round, matched as open wires;
  // rails share a closed section's length between them. A solid's
  // sections are closed, as before, whatever their wires say.
  const auto is_open = [](const SkinSection& section) {
    return section.point ||
           (section.shape.ShapeType() == TopAbs_WIRE && !BRep_Tool::IsClosed(section.shape));
  };
  const bool open = !solid && std::all_of(sections.begin(), sections.end(), is_open) &&
                    std::any_of(sections.begin(), sections.end(), [](const SkinSection& s) { return !s.point; });
  if (open && !rail_paths.empty()) {
    throw std::invalid_argument("unsupported: loft rails with open sections");
  }
  if (open && closed) {
    throw std::invalid_argument("a closed loft through open sections is not supported");
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
    const TopoDS_Wire wire = TopoDS::Wire(sections[i].shape);
    wires.Append(
        without_seam(wire, split_at(wire, hits, tolerance, origins[i], open), hits, tolerance, origins[i]));
  }

  // Edge for edge, from corresponding starts. With rails, where OCCT's
  // matching does not keep them at corresponding vertices, the sections are
  // matched from the rails instead (match_along_rails, mitcad#1944); lofts
  // it keeps them for keep its matching and faces.
  std::vector<Matched> matched;
  std::vector<TopoDS_Wire> section_wires(count);
  std::size_t edge_count = 0;
  const auto compatible_matching = [&]() -> bool {
    // Merged sections of one smooth edge each (mitcad#2162) are matched
    // already: OCCT's matching would cut them where it relates their
    // vertices.
    BRepFill_CompatibleWires compatible(wires);
    if (!matched_already) {
      try {
        compatible.Perform();
      } catch (const Standard_Failure&) {
        if (rails.empty()) {
          throw;
        }
        return false;
      }
      if (!compatible.IsDone()) {
        if (rails.empty()) {
          throw std::runtime_error("the sections could not be matched edge for edge");
        }
        return false;
      }
    }
    const NCollection_DataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> unchanged;
    const auto& generated = matched_already ? unchanged : compatible.Generated();
    matched.assign(count, Matched{});
    edge_count = 0;
    for (std::size_t i = 0; i < count; ++i) {
      Matched& m = matched[i];
      m.centre = centre_of(sections[i].shape);
      m.open = open;
      if (sections[i].point) {
        m.point = true;
        m.tip = BRep_Tool::Pnt(TopoDS::Vertex(sections[i].shape));
        continue;
      }
      section_wires[i] = TopoDS::Wire(matched_already ? wires.Value(static_cast<int>(i) + 1)
                                                      : compatible.Shape().Value(static_cast<int>(i) + 1));
      m.edges = wire_edges(section_wires[i]);
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
        if (rails.empty()) {
          throw std::runtime_error("the sections could not be matched edge for edge");
        }
        return false;
      }
    }

    align_to_previous(matched);

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
          return false;
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
    return true;
  };
  if (!compatible_matching()) {
    matched = match_along_rails(sections, wires, origins, rails, tolerance, section_wires);
    edge_count = matched.front().curves.size();
  }
  // With rails, each face's pieces of the sections related by arc length
  // (by_arc_length, mitcad#2320).
  if (!rails.empty()) {
    for (Matched& m : matched) {
      for (Curve& curve : m.curves) {
        curve = by_arc_length(curve);
      }
    }
  }
  // Two sections with free ends: built here only where faces would meet at
  // a crease that fades out at both sections (mitcad#2141), each chain of
  // faces made tangent so one surface.
  std::optional<SeamChains> chains;
  if (seams_only) {
    chains = match_seams(matched, tolerance);
    if (!chains) {
      return nullptr;
    }
  }

  // The loft's parameter: the chord length between the sections' centres.
  std::vector<double> v{0.0};
  for (std::size_t i = 1; i < count; ++i) {
    v.push_back(v.back() + std::max(matched[i].centre.Distance(matched[i - 1].centre), 1.0e-3));
  }
  if (closed) {
    // The span back to the first section, where the surface closes.
    v.push_back(v.back() + std::max(matched.back().centre.Distance(matched.front().centre), 1.0e-3));
  }
  const bool railed = !rails.empty();
  // Along rails through three or more sections the surfaces blend
  // neighbouring sections (skin_surfaces, mitcad#2533).
  const bool blended_along = railed && !closed && count >= 3;
  std::vector<Matched> chained;
  if (chains) {
    // One surface per chain (each face a piece of its chain's).
    chained = matched;
    chained[0].curves = chains->first;
    chained[1].curves = chains->second;
  }
  // Rails (mitcad#2374): the loft is bent by the rails' distances from it
  // along the vertex lines they run through (each rail's bend), at each
  // place along the loft its section stretched between the two rails on
  // either side of it. A point p of the section between rails a and b
  // (their vertex lines' points qa and qb there) moves by
  // d_a + s (d_b - d_a), s the share of the way from qa to qb along the
  // line through them (stretch_share): an affine map of the section that
  // takes each rail's point onto the rail, moves a straight run through a
  // rail at right angles to that line as the rail moves, and keeps a rail
  // the section's outermost point where it was before the bend, so the rail
  // stays the side's outermost line between the sections too. (Before, the
  // bends were shared out by a smooth step along the section, and beside a
  // rail the neighbouring rail's share pulled the other way: on GitHub
  // #607's design the side stood out beyond its rails by up to 2.69 mm;
  // after mitcad#2374 by 0.108 mm between its rectangles, where the loft
  // without the bend did: the three sections were interpolated along it by
  // one quadratic, whose weight of the circle is negative there, and next
  // to the rail the circle curves in where the rectangles run straight.
  // Since mitcad#2533 the loft without the bend blends neighbouring
  // sections with weights that are never negative, skin_surfaces, so its
  // rails' vertex lines stay outermost between the sections too.)
  // With two rails the section is stretched by one map all round (smooth
  // wherever the section is); with three or more each part between two
  // neighbouring rails by its own. A single rail moves the whole section
  // with it (the reference model loft_rail_circle_one: the circles' loft
  // through one bent rail is the cylinder shifted along it).
  // The vertices' arc lengths round the first section.
  std::vector<double> at;
  double length = 0.0;
  if (!rails.empty()) {
    at.push_back(0.0);
    for (const Curve& curve : matched.front().curves) {
      at.push_back(at.back() + GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(curve)));
    }
    length = at.back();
    at.pop_back();
  }
  // The rails before and after each face along the section (around its
  // middle; the faces are split at the rails, so the whole face lies
  // between them); one rail: that rail twice.
  std::vector<std::pair<std::size_t, std::size_t>> pairs;
  if (!rails.empty()) {
    const auto ahead = [&](double from, double to) { return std::fmod(to - from + 2 * length, length); };
    for (std::size_t j = 0; j < edge_count; ++j) {
      const double xa = at[j];
      const double xb = j + 1 < edge_count ? at[j + 1] : length;
      const double middle = xa + (xb - xa) / 2;
      std::size_t previous = 0;
      std::size_t next = 0;
      for (std::size_t r = 0; r < rails.size(); ++r) {
        const double p = at[static_cast<std::size_t>(rails[r].vertex)];
        if (ahead(p, middle) < ahead(at[static_cast<std::size_t>(rails[previous].vertex)], middle)) {
          previous = r;
        }
        if (ahead(middle, p) < ahead(middle, at[static_cast<std::size_t>(rails[next].vertex)])) {
          next = r;
        }
      }
      pairs.emplace_back(previous, next);
    }
  }
  // The rails' distances from the loft along their vertex lines, as
  // curves in a common basis over the loft's parameter.
  std::vector<Monotone> lengths;
  for (const Rail& rail : rails) {
    lengths.emplace_back(v, rail.lengths);
  }
  // A Perpendicular end along rails (mitcad#2377): the takeoff runs along
  // the face's normal at every point of the section, and is as long as
  // each rail's speed at its point (the derivative of the rail's point by
  // the loft's parameter), the speeds in between shared out as the bends
  // are (by the stretch's shares between the rails on either side,
  // mitcad#2374). So the loft leaves each rail's point as the rail does,
  // its bends leave the section with no slope of their own, and the
  // surface stays at right angles to the face all along the section,
  // between the rails too. The condition's weight then sets nothing.
  const auto speeds_of = [&](const LoftEnd& condition, bool at_start) -> Speeds {
    if (rails.empty() || condition.kind != LoftEnd::Kind::Perpendicular) {
      return Speeds();
    }
    std::vector<double> speed;
    for (const Monotone& along : lengths) {
      speed.push_back(std::abs(at_start ? along.start_slope() : along.end_slope()));
    }
    // The rails' points on the section.
    const Matched& section = at_start ? matched.front() : matched.back();
    std::vector<gp_Pnt> points;
    for (const Rail& rail : rails) {
      const Curve& curve = section.curves[static_cast<std::size_t>(rail.vertex)];
      points.push_back(curve->Value(curve->FirstParameter()));
    }
    return [pairs, speed, points](std::size_t j, const gp_Pnt& p) {
      const auto [a, b] = pairs.at(j);
      if (a == b) {
        return speed[a];
      }
      return speed[a] + stretch_share(p, points[a], points[b]) * (speed[b] - speed[a]);
    };
  };
  const Speeds start_speeds = speeds_of(start, true);
  const Speeds end_speeds = speeds_of(end, false);
  std::vector<Surface> surfaces = skin_surfaces(chains ? chained : matched, sections, start, end, v, false, closed,
                                                railed, start_speeds, end_speeds);
  if (!rails.empty()) {
    // A rail's velocity: its point's derivative by the loft's parameter.
    const auto velocity = [&](std::size_t r, bool at_start) {
      const double x = at_start ? v.front() : v.back();
      const Monotone& along = lengths[r];
      return gp_Vec(rails[r].spine.tangent(along(x))) * (at_start ? along.start_slope() : along.end_slope());
    };
    // A Perpendicular end's rails must leave the section at right angles
    // to the face, along the loft's takeoff at their points (mitcad#2377);
    // otherwise the loft could not leave the face at right angles there.
    for (const bool at_start : {true, false}) {
      if ((at_start ? start : end).kind != LoftEnd::Kind::Perpendicular) {
        continue;
      }
      const double x = at_start ? v.front() : v.back();
      for (std::size_t r = 0; r < rails.size(); ++r) {
        const Surface& surface = surfaces[static_cast<std::size_t>(rails[r].vertex)];
        gp_Pnt p;
        gp_Vec su;
        gp_Vec sv;
        surface->D1(surface->UKnot(1), x, p, su, sv);
        const gp_Vec rail = velocity(r, at_start);
        if (sv.Magnitude() <= 1.0e-12 || rail.Magnitude() <= 1.0e-12) {
          continue;
        }
        const double off = sv.Angle(rail);
        if (off > kRailAngle) {
          char degrees[32];
          std::snprintf(degrees, sizeof degrees, "%.3g", off * 90.0 / kHalfPi);
          const std::string which = at_start ? "start" : "end";
          const std::string what = "rail " + std::to_string(r + 1) + " leaves the " + which + " section " +
                                   degrees + " degrees off the face's normal; leaving the section at right " +
                                   "angles to the face needs rails that leave it so";
          throw std::invalid_argument(placed_error(what, what + " (at " + point_words(p) + ")", {point_place(p)}));
        }
      }
    }
    // Rail r's vertex line at x, and its bend there.
    const auto vertex_point = [&](std::size_t r, double x) {
      const Surface& surface = surfaces[static_cast<std::size_t>(rails[r].vertex)];
      return surface->Value(surface->UKnot(1), x);
    };
    const auto distance = [&](std::size_t r, double x) {
      return gp_Vec(vertex_point(r, x), rails[r].spine.point(lengths[r](x)));
    };
    // What the bends are fitted to (mitcad#2374): each rail's bend and, with
    // two rails or more, the stretch's bend of each row of a face's poles
    // across the section (along the loft the curve of the row's poles). The
    // surfaces are polynomial and their basis across the section sums to 1,
    // so the stretch of a face is the stretch of its rows of poles, exactly:
    // the face's bend is a surface in the face's own basis across it whose
    // rows are these bends. A row on a rail's vertex line is that rail's
    // bend; a vertex line two faces share is one target.
    struct Target {
      std::size_t rail = 0; // a rail's bend; for a row, the rail before it (named where its fit fails)
      bool row = false;
      std::size_t face = 0; // a row: its face (the rails on either side of it)
      Curve poles;          // a row: the curve of its poles along the loft
    };
    std::vector<Target> targets;
    // Per face, the targets of its rows of poles in order across it.
    std::vector<std::vector<std::size_t>> rows;
    const auto rail_targets = [&] {
      targets.clear();
      rows.clear();
      for (std::size_t r = 0; r < rails.size(); ++r) {
        targets.push_back({r, false, 0, Curve()});
      }
    };
    const auto row_curve = [&](std::size_t j, int k) -> Curve {
      const Surface& surface = surfaces[j];
      NCollection_Array1<gp_Pnt> poles(1, surface->NbVPoles());
      for (int l = 1; l <= surface->NbVPoles(); ++l) {
        poles(l) = surface->Pole(k, l);
      }
      return new Geom_BSplineCurve(poles, surface->VKnots(), surface->VMultiplicities(), surface->VDegree());
    };
    const auto stretch_targets = [&] {
      rail_targets();
      std::vector<std::size_t> vertex(edge_count);
      for (std::size_t j = 0; j < edge_count; ++j) {
        std::optional<std::size_t> rail;
        for (std::size_t r = 0; r < rails.size(); ++r) {
          if (static_cast<std::size_t>(rails[r].vertex) == j) {
            rail = r;
          }
        }
        if (rail) {
          vertex[j] = *rail;
        } else {
          vertex[j] = targets.size();
          targets.push_back({pairs[j].first, true, j, row_curve(j, 1)});
        }
      }
      for (std::size_t j = 0; j < edge_count; ++j) {
        if (surfaces[j]->IsUPeriodic() || surfaces[j]->IsURational() || surfaces[j]->IsVRational()) {
          throw std::runtime_error("the loft could not be stretched between its rails");
        }
        std::vector<std::size_t> face{vertex[j]};
        for (int k = 2; k < surfaces[j]->NbUPoles(); ++k) {
          face.push_back(targets.size());
          targets.push_back({pairs[j].first, true, j, row_curve(j, k)});
        }
        face.push_back(vertex[(j + 1) % edge_count]);
        rows.push_back(face);
      }
    };
    // Every target's bend at x.
    const auto bends_at = [&](double x, std::vector<gp_Vec>& out) {
      std::vector<gp_Pnt> on(rails.size());
      std::vector<gp_Vec> bend(rails.size());
      for (std::size_t r = 0; r < rails.size(); ++r) {
        on[r] = vertex_point(r, x);
        bend[r] = gp_Vec(on[r], rails[r].spine.point(lengths[r](x)));
      }
      out.resize(targets.size());
      for (std::size_t t = 0; t < targets.size(); ++t) {
        const Target& target = targets[t];
        if (!target.row) {
          out[t] = bend[target.rail];
          continue;
        }
        const auto [a, b] = pairs[target.face];
        out[t] = bend[a] + (bend[b] - bend[a]) * stretch_share(target.poles->Value(x), on[a], on[b]);
      }
    };
    // Where the bends turn less smoothly (mitcad#2373): at the sections,
    // where the arc lengths along the rails (a cubic of the loft's
    // parameter per span) change their second derivative, and where a rail
    // goes on from one of its edges to the next (a spline on to a line),
    // where its curvature jumps or it turns a corner. One cubic through
    // samples of a whole bend missed it near such a break by more than the
    // fit allows however densely it was sampled ("the loft could not be
    // bent"): the bends are fitted piece by piece between the breaks and
    // the pieces joined smoothly where the bend is smooth, at a rail's
    // corner with a corner.
    // (Found again with each fit, the surfaces' knots among them: every
    // face's, whose rows of poles the stretch's bends follow.)
    std::vector<BendBreak> breaks;
    const auto find_breaks = [&] {
      std::vector<double> knots;
      for (const Surface& surface : surfaces) {
        for (int k = 1; k <= surface->NbVKnots(); ++k) {
          knots.push_back(surface->VKnot(k));
        }
      }
      breaks = bend_breaks(rails, lengths, v, knots);
    };
    const double allowed = 1.0e-7 * std::max(1.0, size);
    // The targets' bends at a break: none at the sections, where the rails
    // meet them already (and so the stretch moves nothing).
    const auto bends_at_break = [&](const BendBreak& place, std::vector<gp_Vec>& out) {
      if (place.section) {
        out.assign(targets.size(), gp_Vec(0, 0, 0));
      } else {
        bends_at(place.x, out);
      }
    };
    // The derivatives of the targets' bends at the end of the piece from
    // `from` to `to` (at `to` with `at_end`) as the piece alone has them:
    // five points a thousandth of the piece apart, inside it (fourth
    // order).
    const auto slopes = [&](const BendBreak& from, const BendBreak& to, bool at_end, std::vector<gp_Vec>& out) {
      const BendBreak& end_break = at_end ? to : from;
      const double h = (to.x - from.x) * (at_end ? -1.0e-3 : 1.0e-3);
      static constexpr std::array<double, 5> weights{-25.0, 48.0, -36.0, 16.0, -3.0};
      std::vector<gp_Vec> values;
      bends_at_break(end_break, values);
      out.assign(targets.size(), gp_Vec(0, 0, 0));
      for (std::size_t k = 0; k < weights.size(); ++k) {
        if (k > 0) {
          bends_at(end_break.x + static_cast<double>(k) * h, values);
        }
        for (std::size_t t = 0; t < targets.size(); ++t) {
          out[t] += values[t] * weights[k];
        }
      }
      for (gp_Vec& slope : out) {
        slope /= 12.0 * h;
      }
    };
    // A Perpendicular end (mitcad#2345, mitcad#2377): each bend leaves that
    // section with the distance's own slope there, the rail's velocity less
    // the loft's takeoff at the rail's vertex, which the speeds above make
    // (all but) nothing; a slope off it would leave the bend off the rail
    // near the section. Exact, from the rail's tangent and the rate at
    // which the loft's parameter runs along it, not estimated from the
    // distances. Other ends' bends are free, as before. A row's bend leaves
    // the section as the stretch shares its rails' (the rails' bends being
    // nothing there, the stretch's own change along the loft adds nothing).
    const auto rail_held = [&](std::size_t r, bool at_start) {
      const double x = at_start ? v.front() : v.back();
      const Surface& surface = surfaces[static_cast<std::size_t>(rails[r].vertex)];
      gp_Pnt p;
      gp_Vec su;
      gp_Vec sv;
      surface->D1(surface->UKnot(1), x, p, su, sv);
      return velocity(r, at_start) - sv;
    };
    const auto held = [&](std::size_t t, bool at_start) {
      EndDerivatives derivatives;
      if ((at_start ? start : end).kind != LoftEnd::Kind::Perpendicular) {
        return derivatives;
      }
      const Target& target = targets[t];
      if (!target.row) {
        derivatives.first = rail_held(target.rail, at_start);
        return derivatives;
      }
      const double x = at_start ? v.front() : v.back();
      const auto [a, b] = pairs[target.face];
      const double s = stretch_share(target.poles->Value(x), vertex_point(a, x), vertex_point(b, x));
      derivatives.first = rail_held(a, at_start) * (1.0 - s) + rail_held(b, at_start) * s;
      return derivatives;
    };
    std::vector<Curve> bends;
    // Where the fit missed most: the target, the loft's parameter and by
    // how much.
    std::size_t worst_rail = 0;
    double worst_x = v.front();
    double worst = 0.0;
    const auto fit_bends = [&] {
      // Each piece's derivatives at its ends, as the piece alone has them.
      find_breaks();
      const std::size_t pieces = breaks.size() - 1;
      // The trace's lines (MITCAD_TRACE_LOFT_BENDS), written when the fit
      // fails.
      const bool tracing = tracing_loft_bends();
      std::vector<std::string> notes;
      if (tracing) {
        notes.push_back(bends_line("%zu rails, %zu sections, %zu breaks, allowed %.3g mm", rails.size(), v.size(),
                                   breaks.size(), allowed));
        for (const BendBreak& place : breaks) {
          notes.push_back(bends_line("  break at %.9g%s%s", place.x, place.section ? " section" : "",
                                     place.corner ? " corner" : ""));
        }
      }
      const std::size_t count = targets.size();
      std::vector<std::vector<gp_Vec>> starts(pieces);
      std::vector<std::vector<gp_Vec>> ends(pieces);
      for (std::size_t p = 0; p < pieces; ++p) {
        slopes(breaks[p], breaks[p + 1], false, starts[p]);
        slopes(breaks[p], breaks[p + 1], true, ends[p]);
      }
      // Each piece keeps its own derivatives at its ends: a mean of the two
      // pieces' there was off where they differ (a chain's spline with a
      // corner, mitcad#2373: the piece after the spline's knot missed its
      // first samples by 6e-5 mm at 64 per piece, 3e-5 at 128, the error
      // halving with the step). The joint is made smooth where the fitted
      // pieces agree (joined_bends).
      for (std::size_t p = 1; tracing && p < pieces; ++p) {
        for (std::size_t t = 0; t < count; ++t) {
          const double apart = (ends[p - 1][t] - starts[p][t]).Magnitude();
          if (apart > 1.0e-9) {
            notes.push_back(bends_line("  break at %.9g bend %zu (rail %zu%s): slopes %.3g apart", breaks[p].x,
                                       t + 1, targets[t].rail + 1, targets[t].row ? ", a row" : "", apart));
          }
        }
      }
      // A Perpendicular end holds the first piece's start, or the last
      // one's end, along the loft's takeoff (mitcad#2345).
      for (std::size_t t = 0; t < count; ++t) {
        if (const EndDerivatives first = held(t, true); first.first) {
          starts.front()[t] = *first.first;
        }
        if (const EndDerivatives last = held(t, false); last.first) {
          ends.back()[t] = *last.first;
        }
      }
      // Each target's piece: the cubic through the samples with the end
      // derivatives, in the piece's one basis (EndInterpolation, solved
      // once for all of them).
      const auto piece_of = [](const EndInterpolation& basis, const std::vector<gp_Vec>& values,
                               const gp_Vec& leaving, const gp_Vec& arriving) -> Curve {
        std::array<std::vector<double>, 3> coordinates;
        for (int c = 1; c <= 3; ++c) {
          std::vector<double> at_samples;
          for (const gp_Vec& value : values) {
            at_samples.push_back(value.Coord(c));
          }
          coordinates[static_cast<std::size_t>(c) - 1] =
              basis.solve(at_samples, {leaving.Coord(c)}, {arriving.Coord(c)});
        }
        NCollection_Array1<gp_Pnt> poles(1, basis.pole_count());
        for (int i = 1; i <= basis.pole_count(); ++i) {
          const std::size_t k = static_cast<std::size_t>(i) - 1;
          poles(i) = gp_Pnt(coordinates[0][k], coordinates[1][k], coordinates[2][k]);
        }
        return new Geom_BSplineCurve(poles, basis.knots(), basis.multiplicities(), basis.degree());
      };
      double largest = 0.0;
      for (int per_span = 8;; per_span *= 2) {
        std::vector<double> params;
        std::vector<std::vector<Curve>> fitted(count);
        largest = 0.0;
        for (std::size_t p = 0; p < pieces; ++p) {
          const BendBreak& from = breaks[p];
          const BendBreak& to = breaks[p + 1];
          std::vector<double> samples;
          for (int k = 0; k < per_span; ++k) {
            samples.push_back(from.x + (to.x - from.x) * k / per_span);
          }
          samples.push_back(to.x);
          params.insert(params.end(), samples.begin(), samples.end() - 1);
          // values[t][k]: target t at sample k, exactly through the
          // sections, and through the same point at a break from either
          // side.
          std::vector<std::vector<gp_Vec>> values(count, std::vector<gp_Vec>(samples.size()));
          std::vector<gp_Vec> at_sample;
          for (std::size_t k = 0; k < samples.size(); ++k) {
            if (k == 0) {
              bends_at_break(from, at_sample);
            } else if (k + 1 == samples.size()) {
              bends_at_break(to, at_sample);
            } else {
              bends_at(samples[k], at_sample);
            }
            for (std::size_t t = 0; t < count; ++t) {
              values[t][k] = at_sample[t];
              largest = std::max(largest, at_sample[t].Magnitude());
            }
          }
          const EndInterpolation basis(samples, 1, 1);
          for (std::size_t t = 0; t < count; ++t) {
            fitted[t].push_back(piece_of(basis, values[t], starts[p][t], ends[p][t]));
          }
        }
        params.push_back(breaks.back().x);
        bends = joined_bends(fitted, breaks, 0.1 * allowed);
        worst = 0.0;
        std::size_t worst_target = 0;
        std::vector<gp_Vec> exact;
        for (std::size_t k = 0; k + 1 < params.size(); ++k) {
          const double x = (params[k] + params[k + 1]) / 2;
          bends_at(x, exact);
          for (std::size_t t = 0; t < count; ++t) {
            const double error = gp_Vec(bends[t]->Value(x).XYZ()).Subtracted(exact[t]).Magnitude();
            if (error > worst) {
              worst = error;
              worst_target = t;
              worst_x = x;
            }
          }
        }
        worst_rail = targets[worst_target].rail;
        if (tracing) {
          notes.push_back(bends_line("per piece %d: worst %.3g mm on bend %zu (rail %zu%s) at %.9g (allowed %.3g)",
                                     per_span, worst, worst_target + 1, worst_rail + 1,
                                     targets[worst_target].row ? ", a row" : "", worst_x, allowed));
          // From 64 per piece, per piece and bend where it misses by more
          // than a tenth of the allowed: the fitted piece's own miss and
          // the joined curve's there (the joints' lowered knots move it).
          for (std::size_t p = 0; per_span >= 64 && p < pieces; ++p) {
            std::vector<double> own(count, 0.0);
            std::vector<double> joined(count, 0.0);
            for (int k = 0; k < per_span; ++k) {
              const double x =
                  breaks[p].x + (breaks[p + 1].x - breaks[p].x) * (static_cast<double>(k) + 0.5) / per_span;
              bends_at(x, exact);
              for (std::size_t t = 0; t < count; ++t) {
                own[t] = std::max(own[t], gp_Vec(fitted[t][p]->Value(x).XYZ()).Subtracted(exact[t]).Magnitude());
                joined[t] = std::max(joined[t], gp_Vec(bends[t]->Value(x).XYZ()).Subtracted(exact[t]).Magnitude());
              }
            }
            for (std::size_t t = 0; t < count; ++t) {
              if (std::max(own[t], joined[t]) > 0.1 * allowed) {
                notes.push_back(bends_line("  piece %zu [%.9g, %.9g] bend %zu: own %.3g joined %.3g", p,
                                           breaks[p].x, breaks[p + 1].x, t + 1, own[t], joined[t]));
              }
            }
          }
        }
        if (worst <= allowed) {
          if (tracing) {
            std::fprintf(stderr, "%s\n", notes.back().c_str());
          }
          return largest;
        }
        if (per_span >= 128) {
          if (tracing) {
            // How rough the distance is at the worst place: its fourth
            // differences at steps of 1e-2 to 1e-6 of the loft's length (a
            // smooth distance gives about step^4 times its fourth
            // derivative, noise about 16 times its own size).
            for (const double share : {1.0e-2, 1.0e-3, 1.0e-4, 1.0e-5, 1.0e-6}) {
              const double h = share * (v.back() - v.front());
              const gp_Vec fourth = distance(worst_rail, worst_x - 2 * h) -
                                    distance(worst_rail, worst_x - h) * 4.0 + distance(worst_rail, worst_x) * 6.0 -
                                    distance(worst_rail, worst_x + h) * 4.0 + distance(worst_rail, worst_x + 2 * h);
              notes.push_back(bends_line("  fourth difference at step %.3g: %.3g mm", h, fourth.Magnitude()));
            }
            for (const std::string& note : notes) {
              std::fprintf(stderr, "%s\n", note.c_str());
            }
          }
          throw std::runtime_error(
              not_followed(rails, worst_rail, lengths[worst_rail](worst_x), worst_x, v,
                           "the bend towards it could not be fitted within " + distance_words(allowed) + " mm: " +
                               distance_words(worst) + " mm off"));
        }
      }
    };
    // Rails within half the sewing distance of the loft lie on it already
    // (as straight rails on a ruled loft do).
    const double still = std::max(kSewing / 2, 1.0e-9 * size);
    rail_targets();
    if (fit_bends() > still) {
      // The bends are polynomial: so must the surfaces be.
      bool rebuilt = false;
      if (std::any_of(surfaces.begin(), surfaces.end(),
                      [](const Surface& s) { return s->IsURational() || s->IsVRational(); })) {
        surfaces = skin_surfaces(matched, sections, start, end, v, true, closed, railed, start_speeds,
                                 end_speeds);
        rebuilt = true;
      }
      // With two rails or more, the stretch's bends of the faces' rows of
      // poles too, all in one basis with the rails'.
      if (rails.size() > 1) {
        stretch_targets();
        fit_bends();
      } else if (rebuilt) {
        fit_bends();
      }
      const int vpoles = bends.front()->NbPoles();
      for (std::size_t j = 0; j < edge_count; ++j) {
        Surface bend;
        if (pairs[j].first == pairs[j].second) {
          // One rail: all of its bend all across the face (a cubic across
          // it with every pole the bend).
          NCollection_Array1<double> uknots(1, 2);
          NCollection_Array1<int> umults(1, 2);
          uknots(1) = surfaces[j]->UKnot(1);
          umults(1) = 4;
          uknots(2) = surfaces[j]->UKnot(surfaces[j]->NbUKnots());
          umults(2) = 4;
          const Curve& only = bends[pairs[j].first];
          NCollection_Array2<gp_Pnt> net(1, 4, 1, vpoles);
          for (int k = 1; k <= 4; ++k) {
            for (int l = 1; l <= vpoles; ++l) {
              net(k, l) = only->Pole(l);
            }
          }
          bend = new Geom_BSplineSurface(net, uknots, only->Knots(), umults, only->Multiplicities(), 3,
                                         only->Degree());
        } else {
          // The stretch: across the face in its own basis, each row of
          // poles its row's bend.
          const Surface& face = surfaces[j];
          const std::vector<std::size_t>& across = rows[j];
          if (static_cast<int>(across.size()) != face->NbUPoles()) {
            throw std::runtime_error("the loft could not be stretched between its rails");
          }
          NCollection_Array2<gp_Pnt> net(1, face->NbUPoles(), 1, vpoles);
          for (int k = 1; k <= face->NbUPoles(); ++k) {
            const Curve& row = bends[across[static_cast<std::size_t>(k) - 1]];
            for (int l = 1; l <= vpoles; ++l) {
              net(k, l) = row->Pole(l);
            }
          }
          bend = new Geom_BSplineSurface(net, face->UKnots(), bends.front()->Knots(), face->UMultiplicities(),
                                         bends.front()->Multiplicities(), face->UDegree(), bends.front()->Degree());
        }
        surfaces[j] = added(surfaces[j], bend);
      }
      // The blend between neighbouring sections (mitcad#2533) has a knot of
      // full multiplicity at each inner section, where the bent surface runs
      // on with one tangent (the blend still there, each bend with its
      // rail's tangent): lowered once, so that the surface is C1 by its knots
      // too (the kernel's offset, a Thicken or Shell, refuses C0 surfaces).
      // Where a rail turns a corner the knot cannot be lowered and stays.
      if (blended_along) {
        for (Surface& surface : surfaces) {
          const int degree = surface->VDegree();
          for (int k = surface->FirstVKnotIndex() + 1; k < surface->LastVKnotIndex(); ++k) {
            if (surface->VMultiplicity(k) < degree) {
              continue;
            }
            const Surface trial = Handle(Geom_BSplineSurface)::DownCast(surface->Copy());
            if (trial->RemoveVKnot(k, degree - 1, Precision::Confusion())) {
              surface = trial;
            }
          }
        }
      }
      // The rails lie on the faces (also on either side of each break,
      // where the bends turn).
      std::vector<double> checks;
      for (int k = 0; k <= 64; ++k) {
        checks.push_back(v.front() + (v.back() - v.front()) * k / 64);
      }
      for (std::size_t b = 0; b + 1 < breaks.size(); ++b) {
        const double gap = breaks[b + 1].x - breaks[b].x;
        checks.push_back(breaks[b].x + 0.01 * gap);
        checks.push_back(breaks[b + 1].x - 0.01 * gap);
      }
      const double most = std::max(10 * kSewing, 1.0e-6 * size);
      for (std::size_t r = 0; r < rails.size(); ++r) {
        for (const double x : checks) {
          const double off = distance(r, x).Magnitude();
          if (off > most) {
            throw std::runtime_error(not_followed(rails, r, lengths[r](x), x, v,
                                                  "it lies " + distance_words(off) +
                                                      " mm off the loft (the most allowed is " +
                                                      distance_words(most) + " mm)"));
          }
        }
      }
    }
  }

  // One surface all round the sections (a chain round the whole section,
  // mitcad#2141; one closed edge per section, as merged tangent edges are,
  // mitcad#2271): periodic where it closes smoothly.
  const bool one_round = !open && surfaces.size() == 1 && (chains ? chains->round : edge_count == 1);
  if (one_round) {
    surfaces.front() = smooth_where_c2(periodic_where_closed(surfaces.front()));
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
    TopoDS_Face side;
    if (chains) {
      // Its range of its chain's surface, on a copy of that surface (faces
      // on one surface object would take their common edges for seams).
      const Surface& surface = surfaces[chains->chain_of[j]];
      double u0 = 0.0;
      double u1 = 0.0;
      double v0 = 0.0;
      double v1 = 0.0;
      surface->Bounds(u0, u1, v0, v1);
      const auto [from, to] = chains->range[j];
      BRepBuilderAPI_MakeFace piece(Handle(Geom_BSplineSurface)::DownCast(surface->Copy()), std::max(from, u0),
                                    std::min(to, u1), v0, v1, 1.0e-7);
      if (!piece.IsDone()) {
        throw std::runtime_error("a side face of the loft could not be built");
      }
      side = piece.Face();
    } else {
      BRepBuilderAPI_MakeFace face(surfaces[j], 1.0e-7);
      if (!face.IsDone()) {
        throw std::runtime_error("a side face of the loft could not be built");
      }
      side = face.Face();
    }
    faces.emplace_back(side, names[j].empty() ? NameList{} : NameList{names[j]});
  }
  for (const auto& [index, caps] : {std::pair<std::size_t, const NameList*>{0, &first}, {count - 1, &last}}) {
    // A closed loft and a surface loft have no caps.
    if (sections[index].point || closed || !solid) {
      continue;
    }
    BRepBuilderAPI_MakeFace cap(section_wires[index], true);
    if (cap.IsDone()) {
      faces.emplace_back(cap.Face(), *caps);
    } else if (!sections[index].face.IsNull()) {
      faces.emplace_back(without_holes(sections[index].face), *caps);
    } else {
      throw std::runtime_error("a cap of the loft could not be built");
    }
  }
  for (const auto& [face, face_names] : faces) {
    sewing.Add(face);
  }
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  const TopoDS_Shape sewn_shape = solid ? oriented_solid(sewing.SewedShape(), "the loft") : sewing.SewedShape();
  if (seams_only || one_round) {
    // The faces tangent along their lines (and a side all round the
    // sections along its seam) are marked so, as OCCT's through-sections
    // marks its own (the offsets take such edges as smooth all along).
    BRepLib::EncodeRegularity(sewn_shape, 1.0e-8);
  }
  FaceNamer namer(sewn_shape);
  for (const auto& [face, face_names] : faces) {
    const TopoDS_Shape sewn = sewing.IsModified(face) ? sewing.Modified(face) : face;
    for (const std::string& name : face_names) {
      namer.add(sewn, name);
    }
  }
  return namer.shape();
}

bool periodic_closed_sides(const TopoDS_Shape& loft) {
  bool changed = false;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(loft, TopAbs_FACE, faces);
  BRep_Builder builder;
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Face face = TopoDS::Face(faces(i).Oriented(TopAbs_FORWARD));
    TopLoc_Location location;
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
    const Surface spline = Handle(Geom_BSplineSurface)::DownCast(surface);
    if (spline.IsNull() || spline->IsUPeriodic()) {
      continue;
    }
    const Surface periodic = periodic_where_closed(spline);
    if (periodic == spline) {
      continue;
    }
    // The edges' curves on the face, each with the curve of its other
    // side where it is the seam, carried over to the periodic surface (the
    // same parameters: making it periodic keeps them).
    struct OnFace {
      TopoDS_Edge edge;
      occ::handle<Geom2d_Curve> forward;
      occ::handle<Geom2d_Curve> reversed;
    };
    std::vector<OnFace> curves;
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    TopExp::MapShapes(face, TopAbs_EDGE, edges);
    bool complete = true;
    for (int e = 1; e <= edges.Extent(); ++e) {
      OnFace on;
      on.edge = TopoDS::Edge(edges(e).Oriented(TopAbs_FORWARD));
      double first = 0.0;
      double last = 0.0;
      on.forward = BRep_Tool::CurveOnSurface(on.edge, face, first, last);
      if (BRep_Tool::IsClosed(on.edge, face)) {
        on.reversed =
            BRep_Tool::CurveOnSurface(TopoDS::Edge(on.edge.Oriented(TopAbs_REVERSED)), face, first, last);
      }
      complete = complete && !on.forward.IsNull();
      curves.push_back(on);
    }
    if (!complete) {
      continue;
    }
    // Within the 1e-6 mm periodic_where_closed may move the surface where
    // it closes.
    const double tolerance = std::max(BRep_Tool::Tolerance(face), 1.0e-6);
    builder.UpdateFace(face, periodic, location, tolerance);
    for (const OnFace& on : curves) {
      const double edge_tolerance = std::max(BRep_Tool::Tolerance(on.edge), 1.0e-6);
      // Off the old surface, onto the new one.
      builder.UpdateEdge(on.edge, occ::handle<Geom2d_Curve>(), surface, location, edge_tolerance);
      if (on.reversed.IsNull()) {
        builder.UpdateEdge(on.edge, on.forward, face, edge_tolerance);
      } else {
        builder.UpdateEdge(on.edge, on.forward, on.reversed, face, edge_tolerance);
      }
    }
    changed = true;
  }
  if (changed) {
    // The seam's smoothness, on the new surface (the offsets take it as
    // smooth all along).
    BRepLib::EncodeRegularity(loft, 1.0e-8);
  }
  return changed;
}

} // namespace mitcad::geometry::detail
