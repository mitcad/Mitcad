// SPDX-License-Identifier: MIT
#include "g2_corner.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomConvert.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Edge.hxx>
#include <TopAbs.hxx>
#include <TopLoc_Location.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>
#include <math_Gauss.hxx>
#include <math_Matrix.hxx>
#include <math_Vector.hxx>

#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/user_error.hpp"

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {

gp_Pln G2Corner::plane(std::size_t i) const {
  const gp_Dir& toward = legs.at(i).toward;
  return gp_Pln(apex.Translated(gp_Vec(toward) * -distance), toward);
}

namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// A rolling ball corner's setback: this many times the least the corner
// can be built with (as the setback corners' 1.5 times the radius).
constexpr double kSetbackFactor = 1.5;
// How much more than that least a setback corner's setback must be.
constexpr double kSetbackMargin = 1.02;
// How much of an edge the setbacks may take, the rest left to its rounding;
// how much a worked out setback takes at most.
constexpr double kMostOfEdge = 0.98;
constexpr double kWorkedOutOfEdge = 0.9;
// Where the patch passes between the middle of the roundings' end sections
// and the point their extensions along the edges come nearest to, as a
// fraction of the way (MitCad's own shape: the quintic cross-section's
// middle lies 0.547 of the way from its chord to its tangents' corner at
// tangency weight 1).
constexpr double kMiddle = 0.55;
// How fast the seams reach the middle: their derivative there this share of
// the way to it in the middle's plane (a larger share lets the patch bulge
// out past the faces at a large tangency weight).
constexpr double kMiddleSpeed = 0.6;
// The highest degree a rounding's end section is taken at (an arc is
// approximated by a polynomial of this degree).
constexpr int kSectionDegree = 13;
// How closely the patch takes on its boundary (mm) and the tangent planes
// across it (radians), checked after it is built; the sewing's tolerance.
constexpr double kPatchGap = 1.0e-6;
constexpr double kPatchAngle = 1.0e-4;
constexpr double kSewing = 1.0e-4;

// The refusal in words; `cause` (for agents, in the detail only) says
// which check refused where several share the words.
[[noreturn]] void refuse(const std::string& vertex, const std::string& why, const std::string& cause = {}) {
  throw std::invalid_argument(placed_error("unsupported: curvature continuous fillets that meet at a corner " + why,
                                           "unsupported: curvature continuous fillets that meet at vertex " +
                                               vertex + " " + why + (cause.empty() ? "" : " (" + cause + ")"),
                                           {vertex_place(vertex)}));
}

double binomial(int n, int k) {
  double result = 1.0;
  for (int i = 1; i <= k; ++i) {
    result = result * (n - k + i) / i;
  }
  return result;
}

// n! / (n - j)!
double falling(int n, int j) {
  double result = 1.0;
  for (int i = 0; i < j; ++i) {
    result *= n - i;
  }
  return result;
}

// A polynomial curve on [0, 1] by its Bezier points; a scalar one keeps its
// values in X().
class Poly {
public:
  Poly() : m_points(1, gp_XYZ(0.0, 0.0, 0.0)) {}
  explicit Poly(std::vector<gp_XYZ> points) : m_points(std::move(points)) {
    if (m_points.empty()) {
      m_points.assign(1, gp_XYZ(0.0, 0.0, 0.0));
    }
  }
  static Poly number(double value) { return Poly(std::vector<gp_XYZ>{gp_XYZ(value, 0.0, 0.0)}); }

  int degree() const { return static_cast<int>(m_points.size()) - 1; }
  const std::vector<gp_XYZ>& points() const { return m_points; }

  gp_XYZ value(double t) const {
    std::vector<gp_XYZ> work = m_points;
    for (std::size_t level = 1; level < work.size(); ++level) {
      for (std::size_t i = 0; i + level < work.size(); ++i) {
        work[i] = work[i] * (1.0 - t) + work[i + 1] * t;
      }
    }
    return work.front();
  }
  Poly derivative() const {
    const int n = degree();
    if (n == 0) {
      return Poly();
    }
    std::vector<gp_XYZ> derived(static_cast<std::size_t>(n));
    for (std::size_t i = 0; i < derived.size(); ++i) {
      derived[i] = (m_points[i + 1] - m_points[i]) * n;
    }
    return Poly(std::move(derived));
  }
  // The order-th derivative at t.
  gp_XYZ d(double t, int order) const {
    Poly derived = *this;
    for (int k = 0; k < order; ++k) {
      derived = derived.derivative();
    }
    return derived.value(t);
  }
  Poly elevated(int to) const {
    std::vector<gp_XYZ> points = m_points;
    while (static_cast<int>(points.size()) - 1 < to) {
      const int n = static_cast<int>(points.size()) - 1;
      std::vector<gp_XYZ> raised(points.size() + 1);
      raised.front() = points.front();
      raised.back() = points.back();
      for (int i = 1; i <= n; ++i) {
        const double f = static_cast<double>(i) / (n + 1);
        raised[static_cast<std::size_t>(i)] =
            points[static_cast<std::size_t>(i) - 1] * f + points[static_cast<std::size_t>(i)] * (1.0 - f);
      }
      points = std::move(raised);
    }
    return Poly(std::move(points));
  }
  Poly reversed() const { return Poly(std::vector<gp_XYZ>(m_points.rbegin(), m_points.rend())); }
  // The polynomial on [a, b], taken to [0, 1]: its points are the blossom
  // at a (n - i times) and b (i times).
  Poly part(double a, double b) const {
    const int n = degree();
    std::vector<gp_XYZ> out(m_points.size());
    for (int i = 0; i <= n; ++i) {
      std::vector<gp_XYZ> work = m_points;
      for (int level = 1; level <= n; ++level) {
        const double t = level <= n - i ? a : b;
        for (int j = 0; j + level <= n; ++j) {
          work[static_cast<std::size_t>(j)] =
              work[static_cast<std::size_t>(j)] * (1.0 - t) + work[static_cast<std::size_t>(j) + 1] * t;
        }
      }
      out[static_cast<std::size_t>(i)] = work.front();
    }
    return Poly(std::move(out));
  }
  Poly operator+(const Poly& other) const {
    const int n = std::max(degree(), other.degree());
    std::vector<gp_XYZ> sum = elevated(n).m_points;
    const std::vector<gp_XYZ> added = other.elevated(n).m_points;
    for (std::size_t i = 0; i < sum.size(); ++i) {
      sum[i] += added[i];
    }
    return Poly(std::move(sum));
  }
  Poly operator*(double factor) const {
    std::vector<gp_XYZ> scaled = m_points;
    for (gp_XYZ& point : scaled) {
      point *= factor;
    }
    return Poly(std::move(scaled));
  }
  // The curve moved by a constant.
  Poly plus(const gp_XYZ& offset) const {
    std::vector<gp_XYZ> moved = m_points;
    for (gp_XYZ& point : moved) {
      point += offset;
    }
    return Poly(std::move(moved));
  }
  // This curve times a scalar polynomial.
  Poly times(const Poly& scalar) const {
    const int m = degree();
    const int n = scalar.degree();
    std::vector<gp_XYZ> product(static_cast<std::size_t>(m + n) + 1, gp_XYZ(0.0, 0.0, 0.0));
    for (int i = 0; i <= m; ++i) {
      for (int j = 0; j <= n; ++j) {
        const double f = scalar.m_points[static_cast<std::size_t>(j)].X() * binomial(m, i) * binomial(n, j) /
                         binomial(m + n, i + j);
        product[static_cast<std::size_t>(i + j)] += m_points[static_cast<std::size_t>(i)] * f;
      }
    }
    return Poly(std::move(product));
  }
  // A scalar polynomial times a constant vector.
  Poly along(const gp_XYZ& direction) const {
    std::vector<gp_XYZ> out;
    out.reserve(m_points.size());
    for (const gp_XYZ& point : m_points) {
      out.push_back(direction * point.X());
    }
    return Poly(std::move(out));
  }

private:
  std::vector<gp_XYZ> m_points;
};

// The Bezier points from one end, from the derivatives there (with the
// signs of a parameter running the other way for the end at 1).
std::vector<gp_XYZ> end_points(int degree, const std::vector<gp_XYZ>& derivatives, double sign) {
  std::vector<gp_XYZ> points;
  double power = 1.0;
  for (std::size_t j = 0; j < derivatives.size(); ++j) {
    gp_XYZ sum(0.0, 0.0, 0.0);
    for (std::size_t i = 0; i < j; ++i) {
      sum += points[i] * (((j - i) % 2 == 0 ? 1.0 : -1.0) * binomial(static_cast<int>(j), static_cast<int>(i)));
    }
    points.push_back(derivatives[j] * (power / falling(degree, static_cast<int>(j))) - sum);
    power *= sign;
  }
  return points;
}

// The polynomial of the least degree with the derivatives `start` (the
// value, the first derivative, ...) at 0 and `end` at 1.
Poly hermite(const std::vector<gp_XYZ>& start, const std::vector<gp_XYZ>& end) {
  const int degree = static_cast<int>(start.size() + end.size()) - 1;
  std::vector<gp_XYZ> points(static_cast<std::size_t>(degree) + 1, gp_XYZ(0.0, 0.0, 0.0));
  const std::vector<gp_XYZ> first = end_points(degree, start, 1.0);
  const std::vector<gp_XYZ> last = end_points(degree, end, -1.0);
  for (std::size_t i = 0; i < first.size(); ++i) {
    points[i] = first[i];
  }
  for (std::size_t i = 0; i < last.size(); ++i) {
    points[static_cast<std::size_t>(degree) - i] = last[i];
  }
  return Poly(std::move(points));
}

Poly hermite_number(const std::vector<double>& start, const std::vector<double>& end) {
  const auto lift = [](const std::vector<double>& values) {
    std::vector<gp_XYZ> out;
    for (const double value : values) {
      out.emplace_back(value, 0.0, 0.0);
    }
    return out;
  };
  return hermite(lift(start), lift(end));
}

// From `from` to `to`, level at both ends to the second derivative.
Poly ramp(double from, double to) { return hermite_number({from, 0.0, 0.0}, {to, 0.0, 0.0}); }

// The quintic Hermite functions of the patch's blend: the value, the first
// and the second derivative at 0, then at 1.
std::array<double, 6> blend_basis(double t) {
  const double t2 = t * t;
  const double t3 = t2 * t;
  const double t4 = t3 * t;
  const double t5 = t4 * t;
  return {1.0 - 10.0 * t3 + 15.0 * t4 - 6.0 * t5, t - 6.0 * t3 + 8.0 * t4 - 3.0 * t5,
          0.5 * t2 - 1.5 * t3 + 1.5 * t4 - 0.5 * t5, 10.0 * t3 - 15.0 * t4 + 6.0 * t5,
          -4.0 * t3 + 7.0 * t4 - 3.0 * t5, 0.5 * t3 - t4 + 0.5 * t5};
}

// A function of a quad's side parameter on [0, 1], a polynomial on each
// piece between the breaks.
struct Datum {
  std::vector<double> breaks{0.0, 1.0};
  std::vector<Poly> pieces{Poly()};

  static Datum of(Poly poly) {
    Datum datum;
    datum.pieces = {std::move(poly)};
    return datum;
  }
  gp_XYZ at(double x, int order = 0) const {
    std::size_t i = 0;
    while (i + 1 < pieces.size() && x > breaks[i + 1]) {
      ++i;
    }
    const double width = breaks[i + 1] - breaks[i];
    return pieces[i].d((x - breaks[i]) / width, order) * std::pow(1.0 / width, order);
  }
  int degree() const {
    int most = 0;
    for (const Poly& piece : pieces) {
      most = std::max(most, piece.degree());
    }
    return most;
  }
};

// a x + b y nearest to z.
std::pair<double, double> combination(const gp_XYZ& x, const gp_XYZ& y, const gp_XYZ& z) {
  const double xx = x.Dot(x);
  const double xy = x.Dot(y);
  const double yy = y.Dot(y);
  const double det = xx * yy - xy * xy;
  if (!(det > 1.0e-12 * xx * yy)) {
    throw std::runtime_error("a corner patch's side has no cross direction");
  }
  const double xz = x.Dot(z);
  const double yz = y.Dot(z);
  return {(xz * yy - yz * xy) / det, (yz * xx - xz * xy) / det};
}

// The point nearest to the lines (least squares).
gp_XYZ nearest_to_lines(const std::vector<gp_XYZ>& points, const std::vector<gp_XYZ>& directions,
                        const gp_XYZ& fallback) {
  math_Matrix a(1, 3, 1, 3, 0.0);
  math_Vector rhs(1, 3, 0.0);
  for (std::size_t k = 0; k < points.size(); ++k) {
    for (int r = 1; r <= 3; ++r) {
      for (int c = 1; c <= 3; ++c) {
        const double m = (r == c ? 1.0 : 0.0) - directions[k].Coord(r) * directions[k].Coord(c);
        a(r, c) += m;
        rhs(r) += m * points[k].Coord(c);
      }
    }
  }
  math_Gauss gauss(a, 1.0e-9);
  if (!gauss.IsDone()) {
    return fallback;
  }
  math_Vector x(1, 3);
  gauss.Solve(rhs, x);
  return gp_XYZ(x(1), x(2), x(3));
}

// The Bezier points of a polynomial of degree p in u and q in v from its
// values at the (p + 1) x (q + 1) points u = i / p, v = j / q.
std::vector<std::vector<gp_XYZ>> bezier_net(const std::vector<std::vector<gp_XYZ>>& values, int p, int q) {
  const auto basis = [](int n) {
    math_Matrix matrix(1, n + 1, 1, n + 1, 0.0);
    for (int r = 0; r <= n; ++r) {
      const double t = static_cast<double>(r) / n;
      for (int i = 0; i <= n; ++i) {
        matrix(r + 1, i + 1) = binomial(n, i) * std::pow(t, i) * std::pow(1.0 - t, n - i);
      }
    }
    return matrix;
  };
  const math_Gauss along_u(basis(p), 1.0e-14);
  const math_Gauss along_v(basis(q), 1.0e-14);
  if (!along_u.IsDone() || !along_v.IsDone()) {
    throw std::runtime_error("a corner patch could not be built");
  }
  // First along u for each v, then along v for each u.
  std::vector<std::vector<gp_XYZ>> half(static_cast<std::size_t>(p) + 1,
                                        std::vector<gp_XYZ>(static_cast<std::size_t>(q) + 1));
  for (int j = 0; j <= q; ++j) {
    for (int c = 1; c <= 3; ++c) {
      math_Vector rhs(1, p + 1);
      for (int r = 0; r <= p; ++r) {
        rhs(r + 1) = values[static_cast<std::size_t>(r)][static_cast<std::size_t>(j)].Coord(c);
      }
      math_Vector x(1, p + 1);
      along_u.Solve(rhs, x);
      for (int i = 0; i <= p; ++i) {
        half[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)].SetCoord(c, x(i + 1));
      }
    }
  }
  std::vector<std::vector<gp_XYZ>> net = half;
  for (int i = 0; i <= p; ++i) {
    for (int c = 1; c <= 3; ++c) {
      math_Vector rhs(1, q + 1);
      for (int r = 0; r <= q; ++r) {
        rhs(r + 1) = half[static_cast<std::size_t>(i)][static_cast<std::size_t>(r)].Coord(c);
      }
      math_Vector x(1, q + 1);
      along_v.Solve(rhs, x);
      for (int j = 0; j <= q; ++j) {
        net[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)].SetCoord(c, x(j + 1));
      }
    }
  }
  return net;
}

// An edge's curve from the point `from` to its other end as one polynomial
// piece; a rational or many-span curve fitted within 1e-7 mm.
Poly edge_poly(const std::string& vertex, const TopoDS_Edge& edge, const gp_Pnt& from) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
  if (curve.IsNull()) {
    refuse(vertex, "where a rounding's end has no curve");
  }
  const occ::handle<Geom_Curve> trimmed(new Geom_TrimmedCurve(curve, first, last));
  const occ::handle<Geom_BSplineCurve> spline = GeomConvert::CurveToBSplineCurve(trimmed);
  std::vector<gp_XYZ> points;
  if (!spline->IsRational() && spline->NbKnots() == 2 && spline->Degree() <= kSectionDegree) {
    // A polynomial already (a G2 rounding's quintic): its own points.
    for (int i = 1; i <= spline->NbPoles(); ++i) {
      points.push_back(spline->Pole(i).XYZ());
    }
  } else {
    // An arc (a G1 rounding's) or other curve: the polynomial of degree
    // kSectionDegree through it at Chebyshev-Lobatto points of its
    // parameter, its ends exact (a quarter circle within 1e-13 of its
    // radius), checked along it.
    const int n = kSectionDegree;
    math_Matrix basis(1, n + 1, 1, n + 1, 0.0);
    std::vector<gp_XYZ> values;
    for (int r = 0; r <= n; ++r) {
      const double t = (1.0 - std::cos(3.14159265358979323846 * r / n)) / 2.0;
      for (int i = 0; i <= n; ++i) {
        basis(r + 1, i + 1) = binomial(n, i) * std::pow(t, i) * std::pow(1.0 - t, n - i);
      }
      values.push_back(curve->Value(first + (last - first) * t).XYZ());
    }
    const math_Gauss gauss(basis, 1.0e-14);
    if (!gauss.IsDone()) {
      refuse(vertex, "where a rounding's end cannot be followed", "its section's fit has no solution");
    }
    points.assign(static_cast<std::size_t>(n) + 1, gp_XYZ(0.0, 0.0, 0.0));
    for (int c = 1; c <= 3; ++c) {
      math_Vector rhs(1, n + 1);
      for (int r = 0; r <= n; ++r) {
        rhs(r + 1) = values[static_cast<std::size_t>(r)].Coord(c);
      }
      math_Vector x(1, n + 1);
      gauss.Solve(rhs, x);
      for (int i = 0; i <= n; ++i) {
        points[static_cast<std::size_t>(i)].SetCoord(c, x(i + 1));
      }
    }
    const Poly fitted(points);
    double worst = 0.0;
    for (int k = 0; k <= 200; ++k) {
      const double t = k / 200.0;
      worst = std::max(worst, (fitted.value(t) - curve->Value(first + (last - first) * t).XYZ()).Modulus());
    }
    if (worst > 1.0e-7) {
      refuse(vertex, "where a rounding's end cannot be followed",
             "its section's polynomial deviates " + std::to_string(worst) + " mm");
    }
  }
  Poly poly(std::move(points));
  if (gp_Pnt(poly.value(1.0)).Distance(from) < gp_Pnt(poly.value(0.0)).Distance(from)) {
    poly = poly.reversed();
  }
  return poly;
}

// A step along the hole's boundary: an edge from one vertex to the next,
// the face outside the hole along it and the plane it lies in.
struct Step {
  TopoDS_Edge edge;
  TopoDS_Vertex from;
  TopoDS_Vertex to;
  TopoDS_Face outside;
  std::size_t plane = 0;
};

// A side of the hole, in the plane across one edge: a line on the face
// before it, the rounding's end section, a line on the face after it. As a
// curve s of the side (s from 0 at the first corner), the lines run for
// first_length and last_length, the section for 1, curvature continuous at
// the joints where the rounding is (the lines' speed matched to the
// section's), the lines level at the corners to the fourth derivative.
struct Side {
  std::size_t leg = 0;
  gp_XYZ toward;
  Step first_step;
  Step section_step;
  Step last_step;
  Poly first; // from the corner to the section, on [0, 1]
  Poly section;
  Poly last;  // from the section to the next corner
  double first_length = 0.0;
  double last_length = 0.0;
  bool smooth = true; // the section meets the faces curvature continuously
  // The section cut at its middle, where a seam of the patch starts: the
  // vertex there and the halves in the side's direction.
  TopoDS_Vertex middle;
  TopoDS_Edge half_first;
  TopoDS_Edge half_second;
};

Side make_side(const std::string& vertex, const G2Corner& corner, const Step& a, const Step& b, const Step& c) {
  for (const Step* straight : {&a, &c}) {
    if (BRepAdaptor_Curve(straight->edge).GetType() != GeomAbs_Line) {
      refuse(vertex, "where a rounding does not end between straight lines on its faces");
    }
  }
  if (BRepAdaptor_Curve(b.edge).GetType() == GeomAbs_Line) {
    refuse(vertex, "where a rounding does not end between straight lines on its faces");
  }
  Side side;
  side.leg = a.plane;
  side.toward = corner.legs[a.plane].toward.XYZ();
  side.first_step = a;
  side.section_step = b;
  side.last_step = c;
  const gp_Pnt start = BRep_Tool::Pnt(a.from);
  const gp_Pnt joint_a = BRep_Tool::Pnt(a.to);
  const gp_Pnt joint_b = BRep_Tool::Pnt(c.from);
  const gp_Pnt end = BRep_Tool::Pnt(c.to);
  side.section = edge_poly(vertex, b.edge, joint_a);
  if (gp_Pnt(side.section.value(0.0)).Distance(joint_a) > 1.0e-5 ||
      gp_Pnt(side.section.value(1.0)).Distance(joint_b) > 1.0e-5) {
    refuse(vertex, "where a rounding's end cannot be followed", "its section does not end at the joints");
  }
  // The section's length: the lines run at its mean speed, so that the
  // side's parameter goes evenly (a section of a small tangency weight
  // starts slowly: lines at its speed there would take most of the side).
  double mean = 0.0;
  for (int k = 1; k <= 400; ++k) {
    mean += (side.section.value(k / 400.0) - side.section.value((k - 1) / 400.0)).Modulus();
  }
  // A line from p to q matched to the section's speed and acceleration at
  // its joint, the section's parameter t (`at_end`: the joint is the
  // line's end, else its start); at the corner level to the fourth
  // derivative, at twice the mean speed less the joint's.
  const auto line = [&](const gp_Pnt& p, const gp_Pnt& q, double t, bool at_end, double& length) {
    const double span = p.Distance(q);
    if (!(span > Precision::Confusion())) {
      refuse(vertex, "where a rounding reaches the setback");
    }
    const gp_XYZ direction = (q.XYZ() - p.XYZ()) / span;
    const gp_XYZ d1 = side.section.d(t, 1);
    const gp_XYZ d2 = side.section.d(t, 2);
    const double speed = d1.Dot(direction);
    if (!(speed > 0.0) || (d1 - direction * speed).Modulus() > 1.0e-3 * d1.Modulus()) {
      refuse(vertex, "where a rounding does not meet a face tangentially at its end");
    }
    const double along = d2.Dot(direction);
    if ((d2 - direction * along).Modulus() > 1.0e-5 * speed * speed) {
      side.smooth = false; // an arc: tangent continuous only
    }
    for (const bool even : {true, false}) {
      length = span / (even ? mean : speed);
      const double corner_speed = even ? std::max(2.0 * mean - speed, 0.2 * mean) : speed;
      const Poly position =
          at_end ? hermite_number({0.0, length * corner_speed, 0.0, 0.0, 0.0},
                                  {span, length * speed, along * length * length})
                 : hermite_number({0.0, length * speed, along * length * length},
                                  {span, length * corner_speed, 0.0, 0.0, 0.0});
      bool forward = true;
      for (int k = 0; k <= 40 && forward; ++k) {
        forward = position.d(k / 40.0, 1).X() > 0.0;
      }
      if (forward) {
        return position.along(direction).plus(p.XYZ());
      }
    }
    refuse(vertex, "where a rounding's end cannot be followed", "a straight part of its side runs back");
  };
  side.first = line(start, joint_a, 0.0, true, side.first_length);
  side.last = line(joint_b, end, 1.0, false, side.last_length);
  return side;
}

// The quad's data on one side: the position and the first and second
// derivatives across it, functions of the parameter along it.
struct Ribbon {
  Datum position;
  Datum first;
  Datum second;
};

// A side along a face and a rounding as the side's curve swept along its
// leg (the faces and the roundings there): position E, across alpha E' +
// beta t, second alpha^2 E''; alpha from `alpha0` at 0 to nothing at
// `alpha_end`, beta from `beta0` at 0 to `beta1` at 1.
Ribbon swept_ribbon(const Datum& position, const gp_XYZ& toward, double alpha0, double alpha_end, double beta0,
                    double beta1) {
  Ribbon ribbon;
  ribbon.position = position;
  ribbon.first.breaks = position.breaks;
  ribbon.second.breaks = position.breaks;
  ribbon.first.pieces.clear();
  ribbon.second.pieces.clear();
  const Poly beta = ramp(beta0, beta1);
  for (std::size_t i = 0; i < position.pieces.size(); ++i) {
    const double a = position.breaks[i];
    const double b = position.breaks[i + 1];
    const double width = b - a;
    const Poly& piece = position.pieces[i];
    const Poly beta_here = beta.part(a, b);
    // alpha on this piece, in its own parameter: the ramp over [0, alpha_end].
    Poly alpha = Poly::number(0.0);
    if (alpha0 != 0.0 && a < alpha_end) {
      alpha = ramp(alpha0, 0.0).part(a / alpha_end, std::min(b, alpha_end) / alpha_end);
      if (b > alpha_end + 1.0e-12) {
        throw std::runtime_error("a corner patch's ramp crosses a joint");
      }
    }
    // Derivatives along the side, in the side's parameter.
    const Poly d1 = piece.derivative() * (1.0 / width);
    const Poly d2 = piece.derivative().derivative() * (1.0 / (width * width));
    ribbon.first.pieces.push_back(d1.times(alpha) + beta_here.along(toward));
    ribbon.second.pieces.push_back(d2.times(alpha).times(alpha));
  }
  return ribbon;
}

// A seam's curve and its fields across, the patch's faces on both sides
// taking them on with their own scalars.
struct Seam {
  Poly curve;
  Poly across;     // w
  Poly across_two; // Z
};

Ribbon seam_ribbon(const Seam& seam, double a0, double a1, double b0, double b1) {
  const Poly a = ramp(a0, a1);
  const Poly b = ramp(b0, b1);
  const Poly c1 = seam.curve.derivative();
  const Poly c2 = c1.derivative();
  const Poly w1 = seam.across.derivative();
  Ribbon ribbon;
  ribbon.position = Datum::of(seam.curve);
  ribbon.first = Datum::of(c1.times(a) + seam.across.times(b));
  const Poly ab = a.times(b);
  ribbon.second = Datum::of(c2.times(a).times(a) + w1.times(ab) * 2.0 + seam.across_two.times(b).times(b));
  return ribbon;
}

// The B-spline surface on [0, 1]^2 that takes on the quad's four ribbons:
// v = 0 (functions of u), v = 1, u = 0 (functions of v), u = 1. The blend
// of their Hermite interpolations (a Boolean sum), the corners' mixed
// derivatives from the side the lower order needs (where the sides' data
// agree, as they do at a curvature continuous joint, from both).
occ::handle<Geom_BSplineSurface> quad_surface(const std::array<Ribbon, 2>& along_u, const std::array<Ribbon, 2>& along_v) {
  // v-ribbons: functions of u, at v = 0 and v = 1; u-ribbons: of v.
  const auto datum = [](const Ribbon& ribbon, int order) -> const Datum& {
    return order == 0 ? ribbon.position : order == 1 ? ribbon.first : ribbon.second;
  };
  // corner[ue][ve][i][j]: d^i/du^i d^j/dv^j at (ue, ve).
  gp_XYZ corner[2][2][3][3];
  for (int ue = 0; ue < 2; ++ue) {
    for (int ve = 0; ve < 2; ++ve) {
      for (int i = 0; i < 3; ++i) {
        for (int j = 0; j < 3; ++j) {
          const gp_XYZ from_u = datum(along_v[static_cast<std::size_t>(ue)], i).at(ve, j);
          const gp_XYZ from_v = datum(along_u[static_cast<std::size_t>(ve)], j).at(ue, i);
          corner[ue][ve][i][j] = i < j ? from_v : j < i ? from_u : (from_u + from_v) * 0.5;
        }
      }
    }
  }
  const auto evaluate = [&](double u, double v) {
    const std::array<double, 6> pu = blend_basis(u);
    const std::array<double, 6> pv = blend_basis(v);
    gp_XYZ sum(0.0, 0.0, 0.0);
    for (int e = 0; e < 2; ++e) {
      for (int k = 0; k < 3; ++k) {
        sum += datum(along_u[static_cast<std::size_t>(e)], k).at(u) * pv[static_cast<std::size_t>(3 * e + k)];
        sum += datum(along_v[static_cast<std::size_t>(e)], k).at(v) * pu[static_cast<std::size_t>(3 * e + k)];
      }
    }
    for (int ue = 0; ue < 2; ++ue) {
      for (int i = 0; i < 3; ++i) {
        for (int ve = 0; ve < 2; ++ve) {
          for (int j = 0; j < 3; ++j) {
            sum -= corner[ue][ve][i][j] *
                   (pu[static_cast<std::size_t>(3 * ue + i)] * pv[static_cast<std::size_t>(3 * ve + j)]);
          }
        }
      }
    }
    return sum;
  };
  // The degrees: the blend's 5, the data's; the joints of the ribbons'
  // pieces along each direction.
  int p = 5;
  int q = 5;
  std::vector<double> ubreaks{0.0, 1.0};
  std::vector<double> vbreaks{0.0, 1.0};
  const auto add_breaks = [](std::vector<double>& breaks, const Datum& d) {
    for (const double b : d.breaks) {
      if (std::none_of(breaks.begin(), breaks.end(), [b](double known) { return std::abs(known - b) < 1.0e-12; })) {
        breaks.push_back(b);
      }
    }
  };
  for (int e = 0; e < 2; ++e) {
    for (int k = 0; k < 3; ++k) {
      p = std::max(p, datum(along_u[static_cast<std::size_t>(e)], k).degree());
      q = std::max(q, datum(along_v[static_cast<std::size_t>(e)], k).degree());
      add_breaks(ubreaks, datum(along_u[static_cast<std::size_t>(e)], k));
      add_breaks(vbreaks, datum(along_v[static_cast<std::size_t>(e)], k));
    }
  }
  std::sort(ubreaks.begin(), ubreaks.end());
  std::sort(vbreaks.begin(), vbreaks.end());
  const int uspans = static_cast<int>(ubreaks.size()) - 1;
  const int vspans = static_cast<int>(vbreaks.size()) - 1;
  // One Bezier net per cell between the joints, joined with knots of
  // multiplicity p and q (the rows and columns the cells share averaged),
  // then made as smooth as the data are; fitted about the middle, so that a
  // patch far from the origin loses no digits.
  const gp_XYZ origin = evaluate(0.5, 0.5);
  NCollection_Array2<gp_Pnt> poles(1, uspans * p + 1, 1, vspans * q + 1);
  std::vector<std::vector<int>> shares(static_cast<std::size_t>(uspans * p + 1),
                                       std::vector<int>(static_cast<std::size_t>(vspans * q + 1), 0));
  for (int a = 0; a < uspans; ++a) {
    for (int s = 0; s < vspans; ++s) {
      const double u0 = ubreaks[static_cast<std::size_t>(a)];
      const double u1 = ubreaks[static_cast<std::size_t>(a) + 1];
      const double v0 = vbreaks[static_cast<std::size_t>(s)];
      const double v1 = vbreaks[static_cast<std::size_t>(s) + 1];
      std::vector<std::vector<gp_XYZ>> values(static_cast<std::size_t>(p) + 1,
                                              std::vector<gp_XYZ>(static_cast<std::size_t>(q) + 1));
      for (int r = 0; r <= p; ++r) {
        for (int c = 0; c <= q; ++c) {
          values[static_cast<std::size_t>(r)][static_cast<std::size_t>(c)] =
              evaluate(u0 + (u1 - u0) * r / p, v0 + (v1 - v0) * c / q) - origin;
        }
      }
      const std::vector<std::vector<gp_XYZ>> net = bezier_net(values, p, q);
      for (int i = 0; i <= p; ++i) {
        for (int j = 0; j <= q; ++j) {
          const int row = a * p + i;
          const int column = s * q + j;
          int& shared = shares[static_cast<std::size_t>(row)][static_cast<std::size_t>(column)];
          const gp_XYZ point = net[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)] + origin;
          const gp_XYZ before = shared == 0 ? gp_XYZ(0.0, 0.0, 0.0) : poles(row + 1, column + 1).XYZ();
          poles(row + 1, column + 1) = gp_Pnt((before * shared + point) / (shared + 1));
          ++shared;
        }
      }
    }
  }
  const auto knots = [](const std::vector<double>& breaks, int degree, NCollection_Array1<double>& values,
                        NCollection_Array1<int>& mults) {
    const int last = static_cast<int>(breaks.size());
    for (int s = 0; s < last; ++s) {
      values(s + 1) = breaks[static_cast<std::size_t>(s)];
      mults(s + 1) = s == 0 || s == last - 1 ? degree + 1 : degree;
    }
  };
  NCollection_Array1<double> uknots(1, uspans + 1);
  NCollection_Array1<int> umults(1, uspans + 1);
  NCollection_Array1<double> vknots(1, vspans + 1);
  NCollection_Array1<int> vmults(1, vspans + 1);
  knots(ubreaks, p, uknots, umults);
  knots(vbreaks, q, vknots, vmults);
  occ::handle<Geom_BSplineSurface> surface(new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, p, q));
  // C2 where the data are (a curvature continuous joint), else C1.
  for (int s = 2; s <= uspans; ++s) {
    if (!surface->RemoveUKnot(s, p - 2, 1.0e-9)) {
      surface->RemoveUKnot(s, p - 1, 1.0e-9);
    }
  }
  for (int s = 2; s <= vspans; ++s) {
    if (!surface->RemoveVKnot(s, q - 2, 1.0e-9)) {
      surface->RemoveVKnot(s, q - 1, 1.0e-9);
    }
  }
  return surface;
}

// The face on `surface` with its edges in order round [0, 1]^2 (u along
// the first), the seams' p-curves given, the others projected.
TopoDS_Face quad_face(const occ::handle<Geom_BSplineSurface>& surface, const std::vector<TopoDS_Edge>& edges,
                      const std::vector<std::pair<TopoDS_Edge, occ::handle<Geom2d_Line>>>& seams) {
  BRep_Builder builder;
  TopoDS_Face face;
  const occ::handle<Geom_Surface> base(surface);
  builder.MakeFace(face, base, Precision::Confusion());
  TopoDS_Wire wire;
  builder.MakeWire(wire);
  for (const TopoDS_Edge& edge : edges) {
    builder.Add(wire, edge);
  }
  builder.Add(face, wire);
  for (const auto& [seam, line] : seams) {
    const occ::handle<Geom2d_Curve> pcurve(line);
    builder.UpdateEdge(seam, pcurve, face, BRep_Tool::Tolerance(seam));
  }
  ShapeFix_Edge fix;
  for (const TopoDS_Edge& edge : edges) {
    bool known = false;
    for (const auto& seam : seams) {
      known = known || seam.first.IsSame(edge);
    }
    if (!known) {
      fix.FixAddPCurve(edge, face, false, kPatchGap);
      fix.FixSameParameter(edge, face);
    }
  }
  return face;
}

// The side's section edge cut at `middle` (on it, within the section's
// fit): its halves get the edge's p-curve on the rounding's surface and
// replace it in `reshape`.
void split_section(Side& side, const gp_Pnt& middle, ShapeBuild_ReShape& reshape) {
  const TopoDS_Edge forward = TopoDS::Edge(side.section_step.edge.Oriented(TopAbs_FORWARD));
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(forward, first, last);
  GeomAPI_ProjectPointOnCurve projection(middle, curve, first, last);
  if (curve.IsNull() || projection.NbPoints() == 0) {
    throw std::runtime_error("a rounding's end section could not be cut for a corner patch");
  }
  const double at = projection.LowerDistanceParameter();
  TopoDS_Vertex start;
  TopoDS_Vertex end;
  TopExp::Vertices(forward, start, end);
  BRep_Builder builder;
  TopoDS_Vertex cut;
  builder.MakeVertex(cut, middle, std::max(Precision::Confusion(), 2.0 * projection.LowerDistance()));
  BRepBuilderAPI_MakeEdge make_first(curve, start, cut, first, at);
  BRepBuilderAPI_MakeEdge make_second(curve, cut, end, at, last);
  if (!make_first.IsDone() || !make_second.IsDone()) {
    throw std::runtime_error("a rounding's end section could not be cut for a corner patch");
  }
  const TopoDS_Edge piece_first = make_first.Edge();
  const TopoDS_Edge piece_second = make_second.Edge();
  const TopoDS_Face& rounding = side.section_step.outside;
  double from = 0.0;
  double to = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(forward, rounding, from, to);
  if (!pcurve.IsNull()) {
    TopLoc_Location location;
    const occ::handle<Geom_Surface>& surface = BRep_Tool::Surface(rounding, location);
    const double tolerance = BRep_Tool::Tolerance(forward);
    builder.UpdateEdge(piece_first, pcurve, surface, location, tolerance);
    builder.UpdateEdge(piece_second, pcurve, surface, location, tolerance);
  }
  TopoDS_Wire pieces;
  builder.MakeWire(pieces);
  builder.Add(pieces, piece_first);
  builder.Add(pieces, piece_second);
  reshape.Replace(forward, pieces);
  side.middle = cut;
  const bool along = start.IsSame(side.section_step.from);
  side.half_first = along ? piece_first : piece_second;
  side.half_second = along ? piece_second : piece_first;
}

// The edge oriented to run from the vertex.
TopoDS_Edge from_vertex(const TopoDS_Edge& edge, const TopoDS_Vertex& from) {
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), first, last);
  return TopoDS::Edge(edge.Oriented(first.IsSame(from) ? TopAbs_FORWARD : TopAbs_REVERSED));
}

// The patch of one corner: its faces, added to `patches`, and the hole's
// faces, added to `removed`. Returns whether it is curvature continuous all
// round.
bool corner_patch(const std::string& feature, const Shape& rounded, const G2Corner& corner,
                  const Ancestors& faces_of_edge, ShapeMap& removed, std::vector<TopoDS_Face>& patches,
                  ShapeBuild_ReShape& reshape) {
  const std::string& vertex = corner.vertex;
  const std::size_t n = corner.legs.size();
  // The hole: the notch's faces as the roundings left them.
  ShapeMap hole;
  for (const int f : rounded.find_faces(face_name(feature, "corner", vertex))) {
    hole.Add(rounded.face(f));
  }
  if (hole.IsEmpty()) {
    refuse(vertex, "whose notch the roundings took away");
  }
  // Its boundary, one loop of steps.
  ShapeMap boundary;
  std::vector<TopoDS_Face> outside;
  for (int h = 1; h <= hole.Extent(); ++h) {
    for (TopExp_Explorer it(hole(h), TopAbs_EDGE); it.More(); it.Next()) {
      if (boundary.Contains(it.Current())) {
        continue;
      }
      for (const TopoDS_Shape& face : faces_of_edge.FindFromKey(it.Current())) {
        if (!hole.Contains(face)) {
          if (boundary.Add(it.Current()) > static_cast<int>(outside.size())) {
            outside.push_back(TopoDS::Face(face));
          }
          break;
        }
      }
    }
  }
  const int count = boundary.Extent();
  std::vector<Step> loop;
  std::vector<char> used(static_cast<std::size_t>(count), 0);
  const auto ends = [&](int e) {
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(TopoDS::Edge(boundary(e + 1).Oriented(TopAbs_FORWARD)), first, last);
    return std::make_pair(first, last);
  };
  for (int e = 0; e < count && loop.empty(); ++e) {
    const auto [first, last] = ends(e);
    loop.push_back({TopoDS::Edge(boundary(e + 1)), first, last, outside[static_cast<std::size_t>(e)], 0});
    used[static_cast<std::size_t>(e)] = 1;
  }
  while (!loop.empty() && static_cast<int>(loop.size()) < count) {
    const TopoDS_Vertex at = loop.back().to;
    int next = -1;
    for (int e = 0; e < count && next < 0; ++e) {
      if (used[static_cast<std::size_t>(e)] == 0 && (ends(e).first.IsSame(at) || ends(e).second.IsSame(at))) {
        next = e;
      }
    }
    if (next < 0) {
      refuse(vertex, "whose notch's boundary is not one loop");
    }
    used[static_cast<std::size_t>(next)] = 1;
    const auto [first, last] = ends(next);
    loop.push_back({TopoDS::Edge(boundary(next + 1)), at, first.IsSame(at) ? last : first,
                    outside[static_cast<std::size_t>(next)], 0});
  }
  if (loop.empty() || !loop.back().to.IsSame(loop.front().from) || loop.front().from.IsSame(loop.front().to)) {
    refuse(vertex, "whose notch's boundary is not one loop");
  }
  // The plane of each step, the loop started at a side's first step, cut
  // into sides.
  for (Step& step : loop) {
    const BRepAdaptor_Curve curve(step.edge);
    const gp_Pnt middle = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
    std::size_t found = n;
    for (std::size_t i = 0; i < n && found == n; ++i) {
      if (corner.plane(i).Distance(middle) < 1.0e-5) {
        found = i;
      }
    }
    if (found == n) {
      refuse(vertex, "whose notch's boundary leaves the planes across its edges");
    }
    step.plane = found;
  }
  std::size_t start = 0;
  while (start < loop.size() && loop[start].plane == loop[(start + loop.size() - 1) % loop.size()].plane) {
    ++start;
  }
  if (start == loop.size()) {
    refuse(vertex, "whose notch's boundary lies in one plane");
  }
  std::rotate(loop.begin(), loop.begin() + static_cast<std::ptrdiff_t>(start), loop.end());
  std::vector<std::vector<Step>> runs;
  for (const Step& step : loop) {
    if (runs.empty() || runs.back().back().plane != step.plane) {
      runs.emplace_back();
    }
    runs.back().push_back(step);
  }
  if (runs.size() != n) {
    refuse(vertex, "whose notch's boundary does not run once across each edge");
  }
  std::vector<Side> sides;
  for (const std::vector<Step>& run : runs) {
    if (run.size() != 3) {
      refuse(vertex, "where a rounding does not end between straight lines on its faces");
    }
    sides.push_back(make_side(vertex, corner, run[0], run[1], run[2]));
  }
  for (std::size_t k = 0; k < n; ++k) {
    for (std::size_t j = 0; j < k; ++j) {
      if (sides[j].leg == sides[k].leg) {
        refuse(vertex, "whose notch's boundary does not run once across each edge");
      }
    }
  }

  // The patch's middle M: from the middle of the end sections towards the
  // point their extensions along the edges come nearest to; the plane there
  // across the legs' directions summed, its curvature the same all round
  // (a paraboloid through the sections' middles on average).
  std::vector<gp_XYZ> middles;
  std::vector<gp_XYZ> towards;
  gp_XYZ centre(0.0, 0.0, 0.0);
  gp_XYZ summed(0.0, 0.0, 0.0);
  for (const Side& side : sides) {
    middles.push_back(side.section.value(0.5));
    towards.push_back(side.toward);
    centre += middles.back() / static_cast<double>(n);
    summed += side.toward;
  }
  const gp_XYZ nearest = nearest_to_lines(middles, towards, centre);
  const gp_XYZ m = centre + (nearest - centre) * kMiddle;
  if (!(summed.Modulus() > 1.0e-6)) {
    refuse(vertex, "whose edges' directions cancel out");
  }
  const gp_XYZ normal = summed / summed.Modulus();
  // The plane's axes: e1 towards the first side's joint, e2 so that the
  // sides go round anticlockwise.
  const auto flat = [&](const gp_XYZ& point) {
    const gp_XYZ q = point - m;
    return q - normal * q.Dot(normal);
  };
  gp_XYZ e1 = flat(middles.front());
  if (!(e1.Modulus() > 1.0e-9)) {
    refuse(vertex, "whose patch has no middle");
  }
  e1 /= e1.Modulus();
  gp_XYZ e2 = normal.Crossed(e1);
  std::vector<std::array<double, 2>> directions;
  std::vector<double> reach;
  std::vector<gp_XYZ> joints; // where the seams start: the sections' middles
  for (const Side& side : sides) {
    joints.push_back(side.section.value(0.5));
    const gp_XYZ q = flat(joints.back());
    reach.push_back(q.Modulus());
    if (!(reach.back() > 1.0e-6)) {
      refuse(vertex, "whose patch has no middle");
    }
    directions.push_back({q.Dot(e1) / reach.back(), q.Dot(e2) / reach.back()});
  }
  const auto cross = [](const std::array<double, 2>& x, const std::array<double, 2>& y) {
    return x[0] * y[1] - x[1] * y[0];
  };
  double turning = 0.0;
  for (std::size_t k = 0; k < n; ++k) {
    turning += cross(directions[k], directions[(k + 1) % n]);
  }
  if (turning < 0.0) {
    e2.Reverse();
    for (std::array<double, 2>& d : directions) {
      d[1] = -d[1];
    }
  }
  double total = 0.0;
  for (std::size_t k = 0; k < n; ++k) {
    const std::array<double, 2>& x = directions[k];
    const std::array<double, 2>& y = directions[(k + 1) % n];
    if (!(cross(x, y) > 1.0e-3)) {
      refuse(vertex, "whose roundings do not go round the patch's middle");
    }
    total += std::atan2(cross(x, y), x[0] * y[0] + x[1] * y[1]);
  }
  if (std::abs(total - 2.0 * 3.14159265358979323846) > 1.0e-6) {
    refuse(vertex, "whose roundings do not go round the patch's middle");
  }
  double curvature = 0.0;
  for (const gp_XYZ& point : middles) {
    const gp_XYZ q = point - m;
    const double height = q.Dot(normal);
    const double planar = (q - normal * height).SquareModulus();
    curvature += planar > 0.0 ? 2.0 * height / planar / static_cast<double>(n) : 0.0;
  }
  const auto in_space = [&](const std::array<double, 2>& d) { return e1 * d[0] + e2 * d[1]; };
  const auto perpendicular = [](const std::array<double, 2>& d) { return std::array<double, 2>{-d[1], d[0]}; };
  const auto dot = [](const std::array<double, 2>& x, const std::array<double, 2>& y) {
    return x[0] * y[0] + x[1] * y[1];
  };

  // Each side as the quads take it: from its corner to the section's
  // middle (`forth`, quad k's along u) and from its end back to the
  // section's middle (`back`, quad k + 1's along v), the parameter running
  // as the side's (the section's half over half the lines' unit).
  std::vector<Datum> forth(n);
  std::vector<Datum> back(n);
  for (std::size_t k = 0; k < n; ++k) {
    const Side& side = sides[k];
    forth[k].breaks = {0.0, side.first_length / (side.first_length + 0.5), 1.0};
    forth[k].pieces = {side.first, side.section.part(0.0, 0.5)};
    back[k].breaks = {0.0, side.last_length / (side.last_length + 0.5), 1.0};
    back[k].pieces = {side.last.reversed(), side.section.part(0.5, 1.0).reversed()};
  }

  // The seams from each section's middle to M: the curve leaves it along
  // the leg towards the vertex and reaches M along the paraboloid (its
  // derivative there kMiddleSpeed of the way in the plane); across it, the
  // side's derivative there (the swept side) turning into the paraboloid's.
  std::vector<Seam> seams(n);
  std::vector<double> speed(n);
  const gp_XYZ zero(0.0, 0.0, 0.0);
  for (std::size_t k = 0; k < n; ++k) {
    const Side& side = sides[k];
    const double r = kMiddleSpeed * reach[k];
    speed[k] = (m - joints[k]).Modulus();
    const gp_XYZ bend = normal * (r * r * curvature);
    seams[k].curve =
        hermite({joints[k], side.toward * speed[k], zero}, {m, in_space(directions[k]) * -r, bend, zero, zero});
    seams[k].across =
        hermite({forth[k].at(1.0, 1), zero, zero}, {in_space(perpendicular(directions[k])) * r, zero, zero, zero});
    seams[k].across_two = hermite({forth[k].at(1.0, 2), zero, zero}, {bend, zero, zero});
  }

  // The quads, one at each corner of the hole: quad k between the end of
  // side k - 1 and the start of side k.
  for (Side& side : sides) {
    split_section(side, gp_Pnt(side.section.value(0.5)), reshape);
  }
  const TopoDS_Vertex middle_vertex = BRepBuilderAPI_MakeVertex(gp_Pnt(m)).Vertex();
  std::vector<TopoDS_Edge> seam_edges;
  for (std::size_t k = 0; k < n; ++k) {
    const std::vector<gp_XYZ>& points = seams[k].curve.points();
    NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(points.size()));
    for (std::size_t i = 0; i < points.size(); ++i) {
      poles(static_cast<int>(i) + 1) = gp_Pnt(points[i]);
    }
    NCollection_Array1<double> knots(1, 2);
    NCollection_Array1<int> mults(1, 2);
    knots(1) = 0.0;
    knots(2) = 1.0;
    mults(1) = static_cast<int>(points.size());
    mults(2) = static_cast<int>(points.size());
    const occ::handle<Geom_Curve> curve(new Geom_BSplineCurve(poles, knots, mults, static_cast<int>(points.size()) - 1));
    BRepBuilderAPI_MakeEdge make(curve, sides[k].middle, middle_vertex);
    if (!make.IsDone()) {
      throw std::runtime_error("a corner patch's seam could not be made");
    }
    seam_edges.push_back(make.Edge());
  }
  bool smooth = true;
  for (std::size_t k = 0; k < n; ++k) {
    throw_if_cancelled();
    const std::size_t before = (k + n - 1) % n;
    const Side& a = sides[k];      // along u from the corner
    const Side& b = sides[before]; // along v from the corner, backwards
    smooth = smooth && a.smooth && b.smooth && corner.legs[a.leg].curvature && corner.legs[b.leg].curvature;
    // v = 0: side k from the corner to its section's middle; u = 0: side
    // k - 1 back from its end to its section's middle.
    const Datum& position_a = forth[k];
    const Datum& position_b = back[before];
    const gp_XYZ along_a = position_a.at(0.0, 1);
    const gp_XYZ along_b = position_b.at(0.0, 1);
    const auto [alpha_a, beta_a] = combination(along_a, a.toward, along_b);
    const auto [alpha_b, beta_b] = combination(along_b, b.toward, along_a);
    const Ribbon v0 = swept_ribbon(position_a, a.toward, alpha_a, position_a.breaks[1], beta_a, speed[k]);
    const Ribbon u0 = swept_ribbon(position_b, b.toward, alpha_b, position_b.breaks[1], beta_b, speed[before]);
    // u = 1: seam k, across it towards the next quad; at M the paraboloid
    // across seam k - 1's direction.
    const double ratio_a = reach[before] / reach[k];
    const Ribbon u1 =
        seam_ribbon(seams[k], 0.0, ratio_a * dot(directions[before], directions[k]), 1.0,
                    -ratio_a * dot(directions[before], perpendicular(directions[k])));
    // v = 1: seam k - 1, across it towards the quad before; at the joint
    // the derivative of side k - 1 there, as a multiple of the seam's field.
    const gp_XYZ joint_across = seams[before].across.value(0.0);
    const double start_b = u0.position.at(1.0, 1).Dot(joint_across) / joint_across.SquareModulus();
    const double ratio_b = reach[k] / reach[before];
    const Ribbon v1 = seam_ribbon(seams[before], 0.0, ratio_b * dot(directions[k], directions[before]), start_b,
                                  -ratio_b * dot(directions[k], perpendicular(directions[before])));
    const occ::handle<Geom_BSplineSurface> surface = quad_surface({v0, v1}, {u0, u1});

    // Built as asked: the boundary taken on and the tangent planes across
    // it the sides' (a check of the construction, not of the input).
    for (int i = 0; i <= 8; ++i) {
      const double t = i / 8.0;
      const std::array<std::pair<gp_Pnt2d, const Ribbon*>, 4> checks{
          std::make_pair(gp_Pnt2d(t, 0.0), &v0), std::make_pair(gp_Pnt2d(t, 1.0), &v1),
          std::make_pair(gp_Pnt2d(0.0, t), &u0), std::make_pair(gp_Pnt2d(1.0, t), &u1)};
      for (std::size_t c = 0; c < checks.size(); ++c) {
        const gp_Pnt2d& uv = checks[c].first;
        const Ribbon& ribbon = *checks[c].second;
        gp_Pnt point;
        gp_Vec du;
        gp_Vec dv;
        surface->D1(uv.X(), uv.Y(), point, du, dv);
        const bool on_u = c < 2; // a side along u
        const gp_XYZ expected = ribbon.position.at(on_u ? uv.X() : uv.Y());
        const gp_XYZ along = ribbon.position.at(on_u ? uv.X() : uv.Y(), 1);
        const gp_XYZ across = ribbon.first.at(on_u ? uv.X() : uv.Y());
        const gp_Vec wanted(on_u ? along.Crossed(across) : across.Crossed(along));
        const gp_Vec made = du.Crossed(dv);
        if (point.XYZ().Subtracted(expected).Modulus() > kPatchGap ||
            (made.Magnitude() > 1.0e-12 && wanted.Magnitude() > 1.0e-12 && i > 0 && i < 8 &&
             made.Angle(wanted) > kPatchAngle)) {
          throw std::runtime_error("a corner patch does not take on its boundary");
        }
      }
    }

    // Nor folded over: no point where the surface has no normal, and the
    // normals of neighbouring points of a grid within a quarter turn.
    constexpr int kGrid = 12;
    std::vector<gp_Vec> normals;
    for (int i = 0; i <= kGrid; ++i) {
      for (int j = 0; j <= kGrid; ++j) {
        gp_Pnt point;
        gp_Vec du;
        gp_Vec dv;
        surface->D1(static_cast<double>(i) / kGrid, static_cast<double>(j) / kGrid, point, du, dv);
        const gp_Vec made = du.Crossed(dv);
        if (!(made.Magnitude() > 1.0e-9 * du.Magnitude() * dv.Magnitude())) {
          throw std::runtime_error("a corner patch has a point without a normal");
        }
        normals.push_back(made.Normalized());
      }
    }
    for (int i = 0; i <= kGrid; ++i) {
      for (int j = 0; j <= kGrid; ++j) {
        const gp_Vec& here = normals[static_cast<std::size_t>(i * (kGrid + 1) + j)];
        if ((i < kGrid && here.Dot(normals[static_cast<std::size_t>((i + 1) * (kGrid + 1) + j)]) <= 0.0) ||
            (j < kGrid && here.Dot(normals[static_cast<std::size_t>(i * (kGrid + 1) + j + 1)]) <= 0.0)) {
          throw std::runtime_error("a corner patch folds over");
        }
      }
    }

    // The face: side k's first line and the first half of its section, seam
    // k, seam k - 1 back, side k - 1's second half and last line.
    const std::vector<TopoDS_Edge> edges{
        from_vertex(a.first_step.edge, a.first_step.from),
        from_vertex(a.half_first, a.first_step.to),
        TopoDS::Edge(seam_edges[k].Oriented(TopAbs_FORWARD)),
        TopoDS::Edge(seam_edges[before].Oriented(TopAbs_REVERSED)),
        from_vertex(b.half_second, b.middle),
        from_vertex(b.last_step.edge, b.last_step.from),
    };
    const occ::handle<Geom2d_Line> on_u1(new Geom2d_Line(gp_Pnt2d(1.0, 0.0), gp_Dir2d(0.0, 1.0)));
    const occ::handle<Geom2d_Line> on_v1(new Geom2d_Line(gp_Pnt2d(0.0, 1.0), gp_Dir2d(1.0, 0.0)));
    TopoDS_Face face = quad_face(surface, edges, {{seam_edges[k], on_u1}, {seam_edges[before], on_v1}});
    // Out of the material as the face along side k's first line.
    gp_Pnt point;
    gp_Vec du;
    gp_Vec dv;
    surface->D1(0.0, 0.0, point, du, dv);
    const gp_Dir out = normal_on_edge(a.first_step.outside, a.first_step.edge);
    if (du.Crossed(dv).Dot(gp_Vec(out)) < 0.0) {
      face.Reverse();
    }
    patches.push_back(face);
  }
  for (int h = 1; h <= hole.Extent(); ++h) {
    removed.Add(hole(h));
  }
  return smooth;
}

} // namespace

G2Corner g2_corner(const Shape& body, int vertex, const std::vector<CornerLeg>& legs, double setback) {
  G2Corner corner;
  corner.vertex = body.vertex_name(vertex);
  const std::string& name = corner.vertex;
  corner.apex = BRep_Tool::Pnt(body.vertex(vertex));
  if (legs.size() < 3) {
    refuse(name, "where fewer than three edges meet");
  }
  const std::vector<int>& faces = body.vertex_faces(vertex);
  if (faces.size() != legs.size()) {
    refuse(name, "where the faces and the edges at the vertex do not alternate");
  }
  int convex = 0;
  for (const CornerLeg& leg : legs) {
    const TopoDS_Edge& edge = body.edge(leg.edge);
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() != GeomAbs_Line) {
      refuse(name, "where an edge is not straight");
    }
    const gp_Pnt first = curve.Value(curve.FirstParameter());
    const gp_Pnt last = curve.Value(curve.LastParameter());
    const gp_Pnt other_end = first.Distance(corner.apex) < last.Distance(corner.apex) ? last : first;
    G2Corner::Leg made;
    made.edge = body.edge_name(leg.edge);
    made.length = other_end.Distance(corner.apex);
    if (!(made.length > Precision::Confusion())) {
      refuse(name, "where an edge has no length");
    }
    made.toward = gp_Dir(gp_Vec(other_end, corner.apex));
    made.curvature = leg.curvature;
    corner.legs.push_back(made);
    const std::array<int, 2> sides = body.edge_faces(leg.edge);
    if (sides[0] < 0 || sides[1] < 0) {
      refuse(name, "where an edge does not lie between two faces");
    }
    const ChFiDS_TypeOfConcavity type =
        ChFi3d::DefineConnectType(edge, body.face(sides[0]), body.face(sides[1]), 1.0e-4, false);
    if (type == ChFiDS_Convex) {
      ++convex;
    } else if (type != ChFiDS_Concave) {
      refuse(name, "where an edge is neither convex nor concave");
    }
  }
  if (convex != 0 && convex != static_cast<int>(legs.size())) {
    refuse(name, "where convex and concave edges meet");
  }
  corner.convex = convex != 0;
  // Each face's two edges at the vertex: how far from the vertex their
  // roundings' contact lines on it reach.
  double least = 0.0;
  for (const int f : faces) {
    const TopoDS_Face& face = body.face(f);
    if (!face_plane(face)) {
      refuse(name, "where a face is not planar");
    }
    std::vector<std::size_t> on;
    for (std::size_t i = 0; i < legs.size(); ++i) {
      const std::array<int, 2> sides = body.edge_faces(legs[i].edge);
      if (sides[0] == f || sides[1] == f) {
        on.push_back(i);
      }
    }
    if (on.size() != 2) {
      refuse(name, "where the faces and the edges at the vertex do not alternate");
    }
    corner.normals.push_back(normal_on_edge(face, body.edge(legs[on[0]].edge)));
    const gp_Vec x = -gp_Vec(corner.legs[on[0]].toward);
    const gp_Vec y = -gp_Vec(corner.legs[on[1]].toward);
    const double angle = x.Angle(y);
    if (!(angle > 1.0e-3 && angle < 3.0)) {
      refuse(name, "where a face's corner is a half turn or wider");
    }
    // Inside the face between its edges, not outside a reflex corner.
    const double step = 0.01 * std::min(corner.legs[on[0]].length, corner.legs[on[1]].length);
    const gp_Pnt inside = corner.apex.Translated((x + y).Normalized() * step);
    const BRepClass_FaceClassifier classifier(face, inside, Precision::Confusion());
    if (classifier.State() != TopAbs_IN) {
      refuse(name, "where a face's corner is a half turn or wider");
    }
    least = std::max(least, std::max(legs[on[0]].contact, legs[on[1]].contact) / std::tan(angle / 2.0));
  }
  corner.least = kSetbackMargin * least;
  corner.asked = setback > 0.0;
  double shortest = corner.legs.front().length;
  for (const G2Corner::Leg& leg : corner.legs) {
    shortest = std::min(shortest, leg.length);
  }
  if (corner.asked) {
    corner.distance = setback;
    if (corner.distance < corner.least) {
      refuse(name, "where the roundings reach past the setback (Rolling Ball corners set them back far enough)");
    }
    if (corner.distance > kMostOfEdge * shortest) {
      refuse(name, "where an edge is shorter than the setback");
    }
  } else {
    corner.distance = std::min(kSetbackFactor * least, kWorkedOutOfEdge * shortest);
    if (corner.distance < corner.least) {
      refuse(name, "where an edge is shorter than the setback the corner needs");
    }
  }
  return corner;
}

void share_edges(std::vector<G2Corner>& corners) {
  for (std::size_t i = 0; i < corners.size(); ++i) {
    for (std::size_t j = 0; j < i; ++j) {
      G2Corner& x = corners[i];
      G2Corner& y = corners[j];
      for (const G2Corner::Leg& a : x.legs) {
        for (const G2Corner::Leg& b : y.legs) {
          const double room = kWorkedOutOfEdge * a.length;
          if (a.edge != b.edge || x.distance + y.distance <= room) {
            continue;
          }
          if (!x.asked && !y.asked) {
            x.distance = std::min(x.distance, room / 2.0);
            y.distance = std::min(y.distance, room / 2.0);
          } else if (!x.asked) {
            x.distance = room - y.distance;
          } else if (!y.asked) {
            y.distance = room - x.distance;
          }
          if (x.distance < x.least || y.distance < y.least || x.distance + y.distance > kMostOfEdge * a.length) {
            throw std::invalid_argument(placed_error(
                "unsupported: curvature continuous fillets that meet at corners at both ends of an edge shorter "
                "than their setbacks",
                "unsupported: curvature continuous fillets that meet at corners at both ends of edge " + a.edge +
                    ", shorter than their setbacks",
                {edge_place(a.edge), vertex_place(x.vertex), vertex_place(y.vertex)}));
          }
        }
      }
    }
  }
}

ShapePtr notch_corners(const std::string& feature, const Shape& body, const std::vector<G2Corner>& corners) {
  ShapePtr current;
  for (const G2Corner& corner : corners) {
    throw_if_cancelled();
    const Shape& on = current ? *current : body;
    // The notch: between the planes across the edges and the faces' planes
    // moved out of the material by the setback (in for a concave corner),
    // inside a box about the vertex.
    const double reach = 10.0 * corner.distance;
    TopoDS_Shape tool = BRepPrimAPI_MakeBox(corner.apex.Translated(gp_Vec(-reach, -reach, -reach)), 2.0 * reach,
                                            2.0 * reach, 2.0 * reach)
                            .Shape();
    const auto keep = [&](const gp_Pln& plane) {
      const TopoDS_Face face = BRepBuilderAPI_MakeFace(plane).Face();
      BRepPrimAPI_MakeHalfSpace half(face, corner.apex);
      BRepAlgoAPI_Common common(tool, half.Solid());
      detail::build(common);
      if (!common.IsDone()) {
        throw std::runtime_error("the notch of a curvature continuous corner could not be made");
      }
      tool = common.Shape();
    };
    for (std::size_t i = 0; i < corner.legs.size(); ++i) {
      keep(corner.plane(i));
    }
    for (const gp_Dir& normal : corner.normals) {
      keep(gp_Pln(corner.apex.Translated(gp_Vec(normal) * (corner.convex ? corner.distance : -corner.distance)),
                  normal));
    }
    TopExp_Explorer solids(tool, TopAbs_SOLID);
    if (!solids.More()) {
      throw std::runtime_error("the notch of a curvature continuous corner could not be made");
    }
    const std::string notch_name = face_name(feature, "corner", corner.vertex);
    std::vector<Shape::NamedFace> named;
    for (TopExp_Explorer it(solids.Current(), TopAbs_FACE); it.More(); it.Next()) {
      named.push_back({it.Current(), {notch_name}});
    }
    const Shape notch(solids.Current(), named);
    const BooleanResult made = boolean(corner.convex ? BooleanOp::Cut : BooleanOp::Join, {&on}, notch);
    if (made.touched.empty() || !made.touched.front() || made.pieces.size() != 1) {
      refuse(corner.vertex, "where other faces of the body lie within the corner's setback");
    }
    const ShapePtr notched = made.pieces.front().shape;
    // One face in each plane across the edges, none elsewhere.
    std::vector<int> in_plane(corner.legs.size(), 0);
    for (const int f : notched->find_faces(notch_name)) {
      const std::optional<gp_Pln> plane = face_plane(notched->face(f));
      std::size_t found = corner.legs.size();
      for (std::size_t i = 0; plane && i < corner.legs.size(); ++i) {
        const gp_Pln across = corner.plane(i);
        if (std::abs(plane->Axis().Direction().Dot(across.Axis().Direction())) > 1.0 - 1.0e-9 &&
            across.Distance(plane->Location()) < 1.0e-6) {
          found = i;
        }
      }
      if (found == corner.legs.size()) {
        refuse(corner.vertex, "where other faces of the body lie within the corner's setback");
      }
      ++in_plane[found];
    }
    if (std::any_of(in_plane.begin(), in_plane.end(), [](int k) { return k != 1; })) {
      refuse(corner.vertex, "where other faces of the body lie within the corner's setback");
    }
    current = notched;
  }
  return current;
}

ShapePtr g2_corner_patches(const std::string& feature, const Shape& rounded, const std::vector<G2Corner>& corners) {
  Ancestors faces_of_edge;
  TopExp::MapShapesAndAncestors(rounded.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  ShapeMap removed;
  std::vector<std::pair<std::vector<TopoDS_Face>, std::string>> patches;
  // The roundings' end sections cut where the patches' seams start.
  ShapeBuild_ReShape reshape;
  bool smooth = true;
  for (const G2Corner& corner : corners) {
    throw_if_cancelled();
    std::vector<TopoDS_Face> faces;
    smooth = corner_patch(feature, rounded, corner, faces_of_edge, removed, faces, reshape) && smooth;
    patches.emplace_back(std::move(faces), face_name(feature, "corner", corner.vertex));
  }
  // The faces left (the roundings with their sections cut) and the
  // patches sewn into the solid.
  BRepBuilderAPI_Sewing sewing(kSewing);
  std::vector<TopoDS_Shape> kept(static_cast<std::size_t>(rounded.face_count()));
  for (int f = 0; f < rounded.face_count(); ++f) {
    if (!removed.Contains(rounded.face(f))) {
      kept[static_cast<std::size_t>(f)] = reshape.Apply(rounded.face(f));
      sewing.Add(kept[static_cast<std::size_t>(f)]);
    }
  }
  for (const auto& [faces, name] : patches) {
    for (const TopoDS_Face& face : faces) {
      sewing.Add(face);
    }
  }
  detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
  TopoDS_Shell shell;
  int shells = 0;
  for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
    shell = TopoDS::Shell(it.Current());
  }
  if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
    throw std::runtime_error("the curvature continuous corners do not close the body");
  }
  BRepBuilderAPI_MakeSolid make(shell);
  TopoDS_Solid solid = make.Solid();
  BRepLib::OrientClosedSolid(solid);
  FaceNamer namer(solid);
  const auto image = [&](const TopoDS_Shape& face) {
    return sewing.IsModified(face) ? sewing.Modified(face) : face;
  };
  for (int f = 0; f < rounded.face_count(); ++f) {
    if (!kept[static_cast<std::size_t>(f)].IsNull()) {
      for (const std::string& name : rounded.face_names(f)) {
        namer.add(image(kept[static_cast<std::size_t>(f)]), name);
      }
    }
  }
  for (const auto& [faces, name] : patches) {
    for (const TopoDS_Face& face : faces) {
      namer.add(image(face), name);
    }
  }
  namer.finish();
  require_valid(namer.result(), "curvature continuous corner");
  ShapePtr result = namer.shape();
  result->copy_notes(rounded);
  if (!smooth) {
    result->add_info("where a tangent continuous rounding meets a curvature continuous corner, the corner is "
                     "tangent continuous (G1) to it, curvature continuous (G2) elsewhere");
  }
  return result;
}

} // namespace mitcad::geometry::detail
