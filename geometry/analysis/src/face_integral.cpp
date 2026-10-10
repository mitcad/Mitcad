// SPDX-License-Identifier: MIT
//
// The integrals of a face over its surface, span by span with fixed Gauss
// rules (mitcad#140).
//
// The face's integral over its domain D in the parameter space is turned
// into one along D's boundary (Green's theorem), as OCCT's BRepGProp does:
//
//   integral over D of f du dv = integral along the boundary of F dv,
//   F(u, v) = integral of f(s, v) ds from the face's first u to u,
//
// or the same with u and v swapped (minus the boundary integral of G du).
// OCCT's fixed rule (BRepGProp_Gauss) takes one set of Gauss points across
// the whole face and along each whole boundary curve, at most 61. Its
// adaptive rule splits the boundary curves only: across the face it keeps
// SIntOrder points (6) per interval of the face's "knots" (a third of a
// turn of a cylinder; one interval for the whole curve of an extrusion or
// beyond a cylinder's first turn), as its error bound there starts at zero
// and is only set in a branch the loop never reaches; and it takes at most
// 32 * 61 + 1 = 1953 intervals per curve, so on a curve of more spans it
// zeroes the 1953rd before it is integrated and never comes back to it.
// Integrated from the face's first u across, that span's part is large on
// a thin face: a thread's crest bounded by helices stored as cubics of 2574
// spans lost 1.4 % of its area, the side of a prism over a closed spline of
// 200 spans moved the solid's centre by 0.009 mm. Here both integrals are
// split where the integrand is not smooth, into pieces short enough for a
// fixed rule:
//
// - across the face, at the surface's knots (B-spline surfaces, the curves
//   of extrusions and revolutions, the basis of offsets), every eighth of a
//   turn of an angle parameter (a cylinder's, a sphere's, a revolution's)
//   and in quarters of rational spans (an area's integrand, a square root,
//   has branch points near them);
// - along a boundary curve, at its knots, where it crosses a knot line of
//   the surface, at those steps of the surface's parameters it passes, and
//   every eighth of a turn along a circle;
// - on each piece Gauss points exact for the polynomial pieces' second
//   moments (8 for cubics), more for rational and offset geometry.
//
// The direction across is the one the surface needs fewer pieces in (along
// a cylinder's axis, not around it). The number of points is fixed by the
// geometry, so a face always measures the same.

#include "face_integral.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <vector>

#include <Adaptor3d_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRep_Tool.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_Array1.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Mat.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>
#include <math.hxx>
#include <math_Vector.hxx>

namespace mitcad::analysis::detail {

namespace {

constexpr double kPi = 3.14159265358979323846;
// The longest piece of an angle parameter: an eighth of a turn, where 8
// points integrate the second moments' terms (up to three times the
// angle) to rounding.
constexpr double kAngleStep = kPi / 4.0;
// Rational spans in quarters: the square root in an area's integrand has
// its branch points near a rational span (a circle scaled into an ellipse
// of 2 to 1, its 120 degree spans: 1e-7 off with 12 points per span).
constexpr int kRationalSplit = 4;
constexpr int kMinPoints = 8;
constexpr int kMaxPoints = 30;
constexpr double kMaxSteps = 1024.0;

// Gauss-Legendre points and weights on [-1, 1].
struct Rule {
  std::vector<double> points;
  std::vector<double> weights;
};

const Rule& gauss_rule(int n) {
  static const std::vector<Rule> rules = [] {
    std::vector<Rule> all(static_cast<std::size_t>(kMaxPoints) + 1);
    for (int k = 1; k <= kMaxPoints; ++k) {
      math_Vector points(1, k);
      math_Vector weights(1, k);
      math::GaussPoints(k, points);
      math::GaussWeights(k, weights);
      Rule& rule = all[static_cast<std::size_t>(k)];
      for (int i = 1; i <= k; ++i) {
        rule.points.push_back(points(i));
        rule.weights.push_back(weights(i));
      }
    }
    return all;
  }();
  return rules[static_cast<std::size_t>(std::min(std::max(n, 1), kMaxPoints))];
}

// Points for a polynomial piece of `degree`: the volume's second moments
// over it are of degree 5 degree - 1.
int polynomial_points(int degree) { return std::min(std::max((5 * degree + 2) / 2, kMinPoints), kMaxPoints); }

// Raw sums of a face's integral about the reference point: the integrand
// (|N| for the surface, r.N for the volume) times 1, r and r r.
struct Sums {
  double m = 0.0;
  double x = 0.0;
  double y = 0.0;
  double z = 0.0;
  double xx = 0.0;
  double yy = 0.0;
  double zz = 0.0;
  double xy = 0.0;
  double xz = 0.0;
  double yz = 0.0;

  void add(const Sums& other, double weight) {
    m += other.m * weight;
    x += other.x * weight;
    y += other.y * weight;
    z += other.z * weight;
    xx += other.xx * weight;
    yy += other.yy * weight;
    zz += other.zz * weight;
    xy += other.xy * weight;
    xz += other.xz * weight;
    yz += other.yz * weight;
  }
};

// GProp_GProps from the sums: mass, centre relative to the reference point
// and the matrix of inertia about it.
class FaceProps : public GProp_GProps {
public:
  FaceProps(const gp_Pnt& at, const Sums& s) : GProp_GProps(at) {
    dim = s.m;
    if (std::abs(dim) >= 1e-30) {
      g.SetCoord(s.x / dim, s.y / dim, s.z / dim);
    } else {
      dim = 0.0;
      g.SetCoord(0.0, 0.0, 0.0);
    }
    inertia = gp_Mat(s.yy + s.zz, -s.xy, -s.xz, -s.xy, s.xx + s.zz, -s.yz, -s.xz, -s.yz, s.xx + s.yy);
  }
};

bool angular_curve(GeomAbs_CurveType type) {
  switch (type) {
  case GeomAbs_Circle:
  case GeomAbs_Ellipse:
  case GeomAbs_Hyperbola:
  case GeomAbs_OffsetCurve:
  case GeomAbs_OtherCurve:
    return true;
  default:
    return false;
  }
}

template <class Curve>
int curve_points(const Curve& curve) {
  switch (curve.GetType()) {
  case GeomAbs_BezierCurve:
  case GeomAbs_BSplineCurve:
    return std::min(polynomial_points(curve.Degree()) + (curve.IsRational() ? 4 : 0), kMaxPoints);
  case GeomAbs_OffsetCurve:
  case GeomAbs_OtherCurve:
    return 12;
  default:
    return kMinPoints;
  }
}

template <class Curve>
int curve_split(const Curve& curve) {
  const GeomAbs_CurveType type = curve.GetType();
  return (type == GeomAbs_BezierCurve || type == GeomAbs_BSplineCurve) && curve.IsRational() ? kRationalSplit : 1;
}

// One parameter direction of the surface.
struct Direction {
  double first = 0.0;
  double last = 0.0;
  bool angular = false;
  int split = 1; // pieces per span
  int points = kMinPoints;
  std::vector<double> breaks; // knots inside (first, last), sorted
  std::vector<double> cuts;   // breaks, every kAngleStep of an angle, spans split

  void finish() {
    cuts = breaks;
    if (angular) {
      // At most kMaxSteps (a parameter taken as an angle may be a length).
      const int steps = static_cast<int>(std::min(std::ceil((last - first) / kAngleStep), kMaxSteps));
      for (int k = 1; k < steps; ++k) {
        cuts.push_back(first + (last - first) * k / steps);
      }
    }
    if (split > 1) {
      std::vector<double> ends{first};
      ends.insert(ends.end(), breaks.begin(), breaks.end());
      ends.push_back(last);
      for (std::size_t k = 0; k + 1 < ends.size(); ++k) {
        for (int j = 1; j < split; ++j) {
          cuts.push_back(ends[k] + (ends[k + 1] - ends[k]) * j / split);
        }
      }
    }
    std::sort(cuts.begin(), cuts.end());
    cuts.erase(std::unique(cuts.begin(), cuts.end()), cuts.end());
  }

  // The pieces a stretch from a to b of this parameter takes: one more
  // than the cuts inside it (a boundary curve is already split where it
  // crosses a knot, so only the angle steps and split spans count).
  double pieces(double a, double b) const {
    if (cuts.size() == breaks.size()) {
      return 1.0;
    }
    const auto low = std::upper_bound(cuts.begin(), cuts.end(), std::min(a, b));
    const auto high = std::lower_bound(cuts.begin(), cuts.end(), std::max(a, b));
    return 1.0 + static_cast<double>(high > low ? high - low : 0);
  }
};

// Whether a patch's weights vary along its U poles (`u`) and along its V
// poles (`v`). OCCT's IsURational and IsVRational name them the other way
// round (a cylinder converted to a B-spline, its circles along U, is
// "V rational"), so the weights are read here.
template <class Patch>
void weights_vary(const Patch& patch, bool& u, bool& v) {
  u = false;
  v = false;
  if (!patch.IsURational() && !patch.IsVRational()) {
    return;
  }
  for (int i = 1; i <= patch.NbUPoles(); ++i) {
    for (int j = 1; j <= patch.NbVPoles(); ++j) {
      const double w = patch.Weight(i, j);
      u = u || (i > 1 && std::abs(w - patch.Weight(i - 1, j)) > 1e-12 * std::abs(w));
      v = v || (j > 1 && std::abs(w - patch.Weight(i, j - 1)) > 1e-12 * std::abs(w));
    }
  }
}

// Which of the surface's parameters are angles, and the points their
// pieces need.
void describe(const Adaptor3d_Surface& surface, Direction& u, Direction& v) {
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    break;
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
    u.angular = true;
    break;
  case GeomAbs_Sphere:
  case GeomAbs_Torus:
    u.angular = true;
    v.angular = true;
    break;
  case GeomAbs_BezierSurface:
  case GeomAbs_BSplineSurface: {
    bool u_rational = false;
    bool v_rational = false;
    if (surface.GetType() == GeomAbs_BSplineSurface) {
      weights_vary(*surface.BSpline(), u_rational, v_rational);
    } else {
      weights_vary(*surface.Bezier(), u_rational, v_rational);
    }
    u.points = std::min(polynomial_points(surface.UDegree()) + (u_rational ? 4 : 0), kMaxPoints);
    v.points = std::min(polynomial_points(surface.VDegree()) + (v_rational ? 4 : 0), kMaxPoints);
    u.split = u_rational ? kRationalSplit : 1;
    v.split = v_rational ? kRationalSplit : 1;
    break;
  }
  case GeomAbs_SurfaceOfRevolution: {
    u.angular = true;
    const occ::handle<Adaptor3d_Curve> curve = surface.BasisCurve();
    v.angular = angular_curve(curve->GetType());
    v.points = curve_points(*curve);
    v.split = curve_split(*curve);
    break;
  }
  case GeomAbs_SurfaceOfExtrusion: {
    const occ::handle<Adaptor3d_Curve> curve = surface.BasisCurve();
    u.angular = angular_curve(curve->GetType());
    u.points = curve_points(*curve);
    u.split = curve_split(*curve);
    break;
  }
  case GeomAbs_OffsetSurface:
    describe(*surface.BasisSurface(), u, v);
    u.points = std::min(u.points + 4, kMaxPoints);
    v.points = std::min(v.points + 4, kMaxPoints);
    break;
  default:
    u.angular = true;
    v.angular = true;
    u.points = 12;
    v.points = 12;
    break;
  }
}

// The surface's knots inside its bounds in one direction.
std::vector<double> surface_breaks(const BRepAdaptor_Surface& surface, bool along_u) {
  std::vector<double> breaks;
  try {
    const int count = along_u ? surface.NbUIntervals(GeomAbs_CN) : surface.NbVIntervals(GeomAbs_CN);
    if (count > 1) {
      NCollection_Array1<double> bounds(1, count + 1);
      if (along_u) {
        surface.UIntervals(bounds, GeomAbs_CN);
      } else {
        surface.VIntervals(bounds, GeomAbs_CN);
      }
      for (int i = 2; i <= count; ++i) {
        breaks.push_back(bounds(i));
      }
    }
  } catch (const Standard_Failure&) {
    breaks.clear();
  }
  std::sort(breaks.begin(), breaks.end());
  return breaks;
}

// The parameters of a boundary curve where its pieces join: its knots.
std::vector<double> curve_breaks(const Geom2dAdaptor_Curve& curve) {
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  std::vector<double> breaks{first};
  try {
    const int count = curve.NbIntervals(GeomAbs_CN);
    if (count > 1) {
      NCollection_Array1<double> bounds(1, count + 1);
      curve.Intervals(bounds, GeomAbs_CN);
      for (int i = 2; i <= count; ++i) {
        if (bounds(i) > first && bounds(i) < last) {
          breaks.push_back(bounds(i));
        }
      }
    }
  } catch (const Standard_Failure&) {
    breaks.resize(1);
  }
  std::sort(breaks.begin(), breaks.end());
  breaks.push_back(last);
  return breaks;
}

class FaceIntegrator {
public:
  FaceIntegrator(const TopoDS_Face& face, const gp_Pnt& at, Integral integral)
      : forward_(TopoDS::Face(face.Oriented(TopAbs_FORWARD))), surface_(forward_, true),
        reversed_(face.Orientation() == TopAbs_REVERSED), at_(at), integral_(integral) {
    u_.first = surface_.FirstUParameter();
    u_.last = surface_.LastUParameter();
    v_.first = surface_.FirstVParameter();
    v_.last = surface_.LastVParameter();
  }

  bool finite() const {
    return !Precision::IsInfinite(u_.first) && !Precision::IsInfinite(u_.last) &&
           !Precision::IsInfinite(v_.first) && !Precision::IsInfinite(v_.last);
  }

  GProp_GProps run() {
    describe(surface_, u_, v_);
    u_.breaks = surface_breaks(surface_, true);
    v_.breaks = surface_breaks(surface_, false);
    u_.finish();
    v_.finish();
    along_u_ = u_.cuts.size() <= v_.cuts.size();

    Sums total;
    if (forward_.NbChildren() == 0) {
      // Natural bounds: the rectangle's side at the far end of the inner
      // parameter is the only one that adds anything.
      if (along_u_) {
        boundary(Geom2dAdaptor_Curve(new Geom2d_Line(gp_Pnt2d(u_.last, v_.first), gp_Dir2d(0.0, 1.0)), 0.0,
                                     v_.last - v_.first),
                 total);
      } else {
        boundary(Geom2dAdaptor_Curve(new Geom2d_Line(gp_Pnt2d(u_.last, v_.last), gp_Dir2d(-1.0, 0.0)), 0.0,
                                     u_.last - u_.first),
                 total);
      }
    } else {
      TopLoc_Location location;
      const occ::handle<Geom_Surface> geometry = BRep_Tool::Surface(forward_, location);
      for (TopExp_Explorer it(forward_, TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
        const TopAbs_Orientation orientation = edge.Orientation();
        if (orientation != TopAbs_FORWARD && orientation != TopAbs_REVERSED) {
          continue;
        }
        double first = 0.0;
        double last = 0.0;
        occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, geometry, location, first, last);
        if (curve.IsNull()) {
          // As OCCT: a face with an edge off its surface measures nothing.
          return FaceProps(at_, Sums());
        }
        if (orientation == TopAbs_REVERSED) {
          const double start = first;
          first = curve->ReversedParameter(last);
          last = curve->ReversedParameter(start);
          curve = curve->Reversed();
        }
        boundary(Geom2dAdaptor_Curve(curve, first, last), total);
      }
    }
    if (integral_ == Integral::Volume) {
      // The divergence theorem: the volume r.N / 3, its moments r r.N / 4
      // and r r r.N / 5.
      Sums volume;
      volume.add(total, 1.0);
      volume.m /= 3.0;
      volume.x /= 4.0;
      volume.y /= 4.0;
      volume.z /= 4.0;
      volume.xx /= 5.0;
      volume.yy /= 5.0;
      volume.zz /= 5.0;
      volume.xy /= 5.0;
      volume.xz /= 5.0;
      volume.yz /= 5.0;
      return FaceProps(at_, volume);
    }
    return FaceProps(at_, total);
  }

private:
  // Adds the integrand at the surface point (u, v), times `weight`.
  void add_point(double u, double v, double weight, Sums& sums) const {
    gp_Pnt point;
    gp_Vec du;
    gp_Vec dv;
    surface_.D1(u, v, point, du, dv);
    gp_Vec normal = du.Crossed(dv);
    if (reversed_) {
      normal.Reverse();
    }
    const double x = point.X() - at_.X();
    const double y = point.Y() - at_.Y();
    const double z = point.Z() - at_.Z();
    const double d = weight * (integral_ == Integral::Surface ? normal.Magnitude()
                                                              : x * normal.X() + y * normal.Y() + z * normal.Z());
    sums.m += d;
    sums.x += x * d;
    sums.y += y * d;
    sums.z += z * d;
    sums.xx += x * x * d;
    sums.yy += y * y * d;
    sums.zz += z * z * d;
    sums.xy += x * y * d;
    sums.xz += x * z * d;
    sums.yz += y * z * d;
  }

  // The integral across the face from its first inner parameter to `to`,
  // at `other` of the outer one.
  Sums across(double to, double other) const {
    const Direction& inner = along_u_ ? u_ : v_;
    Sums sums;
    const Rule& rule = gauss_rule(inner.points);
    double from = inner.first;
    auto cut = inner.cuts.begin();
    while (from < to) {
      const double end = cut != inner.cuts.end() && *cut < to ? *cut++ : to;
      const double middle = 0.5 * (from + end);
      const double half = 0.5 * (end - from);
      for (std::size_t i = 0; i < rule.points.size(); ++i) {
        const double s = middle + half * rule.points[i];
        if (along_u_) {
          add_point(s, other, rule.weights[i] * half, sums);
        } else {
          add_point(other, s, rule.weights[i] * half, sums);
        }
      }
      from = end;
    }
    return sums;
  }

  // Where the curve's coordinate (x or y) crosses `value` between ta and tb,
  // where it is ha and hb: halving and secant steps in turn, a fixed number.
  static double crossing(const Geom2dAdaptor_Curve& curve, bool x, double value, double ta, double tb, double ha,
                         double hb) {
    if (curve.GetType() == GeomAbs_Line) {
      return ta + (value - ha) / (hb - ha) * (tb - ta);
    }
    double a = ta;
    double b = tb;
    double fa = ha - value;
    double fb = hb - value;
    const double resolution = 1e-14 * (tb - ta);
    for (int i = 0; i < 120 && b - a > resolution; ++i) {
      double t = (i % 2 == 0 && fb != fa) ? a - fa * (b - a) / (fb - fa) : 0.5 * (a + b);
      if (!(t > a && t < b)) {
        t = 0.5 * (a + b);
      }
      const gp_Pnt2d p = curve.Value(t);
      const double f = (x ? p.X() : p.Y()) - value;
      if (f == 0.0) {
        return t;
      }
      if ((f < 0.0) == (fa < 0.0)) {
        a = t;
        fa = f;
      } else {
        b = t;
        fb = f;
      }
    }
    return 0.5 * (a + b);
  }

  static void add_crossings(const Geom2dAdaptor_Curve& curve, bool x, const std::vector<double>& breaks,
                            double ta, double tb, double ha, double hb, std::vector<double>& cuts) {
    if (breaks.empty() || ha == hb) {
      return;
    }
    const double low = std::min(ha, hb);
    const double high = std::max(ha, hb);
    for (auto it = std::upper_bound(breaks.begin(), breaks.end(), low); it != breaks.end() && *it < high; ++it) {
      cuts.push_back(crossing(curve, x, *it, ta, tb, ha, hb));
    }
  }

  // The boundary integral along one curve of the face's domain.
  void boundary(const Geom2dAdaptor_Curve& curve, Sums& total) const {
    const GeomAbs_CurveType type = curve.GetType();
    if (type == GeomAbs_Line) {
      // A line along the inner parameter adds nothing.
      const gp_Dir2d direction = curve.Line().Direction();
      if ((along_u_ ? direction.Y() : direction.X()) == 0.0) {
        return;
      }
    }
    const bool curve_angular = angular_curve(type);
    const int points = std::max({curve_points(curve), u_.points, v_.points});
    const Rule& rule = gauss_rule(points);
    const std::vector<double> spans = curve_breaks(curve);
    std::vector<double> cuts;
    for (std::size_t k = 0; k + 1 < spans.size(); ++k) {
      const double ta = spans[k];
      const double tb = spans[k + 1];
      if (!(tb > ta)) {
        continue;
      }
      const gp_Pnt2d pa = curve.Value(ta);
      const gp_Pnt2d pb = curve.Value(tb);
      cuts.assign({ta, tb});
      add_crossings(curve, true, u_.breaks, ta, tb, pa.X(), pb.X(), cuts);
      add_crossings(curve, false, v_.breaks, ta, tb, pa.Y(), pb.Y(), cuts);
      std::sort(cuts.begin(), cuts.end());
      for (std::size_t j = 0; j + 1 < cuts.size(); ++j) {
        const double sa = cuts[j];
        const double sb = cuts[j + 1];
        if (!(sb > sa)) {
          continue;
        }
        const gp_Pnt2d qa = curve.Value(sa);
        const gp_Pnt2d qb = curve.Value(sb);
        double pieces = std::max({u_.pieces(qa.X(), qb.X()), v_.pieces(qa.Y(), qb.Y()),
                                  static_cast<double>(curve_split(curve))});
        if (curve_angular) {
          pieces = std::max(pieces, std::ceil((sb - sa) / kAngleStep));
          if (type == GeomAbs_OffsetCurve || type == GeomAbs_OtherCurve) {
            pieces = std::max(pieces, 8.0);
          }
        }
        const int count = static_cast<int>(std::min(pieces, kMaxSteps));
        const double step = (sb - sa) / count;
        for (int p = 0; p < count; ++p) {
          const double from = sa + step * p;
          const double to = p + 1 == count ? sb : sa + step * (p + 1);
          const double middle = 0.5 * (from + to);
          const double half = 0.5 * (to - from);
          for (std::size_t i = 0; i < rule.points.size(); ++i) {
            gp_Pnt2d point;
            gp_Vec2d tangent;
            curve.D1(middle + half * rule.points[i], point, tangent);
            const double weight = rule.weights[i] * half * (along_u_ ? tangent.Y() : -tangent.X());
            if (weight == 0.0) {
              continue;
            }
            // Within the face's bounds, as OCCT clamps them.
            const double u = std::min(std::max(point.X(), u_.first), u_.last);
            const double v = std::min(std::max(point.Y(), v_.first), v_.last);
            total.add(along_u_ ? across(u, v) : across(v, u), weight);
          }
        }
      }
    }
  }

  TopoDS_Face forward_;
  BRepAdaptor_Surface surface_;
  bool reversed_;
  gp_Pnt at_;
  Integral integral_;
  Direction u_;
  Direction v_;
  bool along_u_ = true;
};

} // namespace

std::optional<GProp_GProps> integrate_face(const TopoDS_Face& face, const gp_Pnt& at, Integral integral) {
  FaceIntegrator integrator(face, at, integral);
  if (!integrator.finite()) {
    return std::nullopt;
  }
  return integrator.run();
}

} // namespace mitcad::analysis::detail
