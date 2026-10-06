// SPDX-License-Identifier: MIT
#pragma once

// Curve and surface fitting shared by the lofts with end conditions and
// rails and by blends that need surfaces of their own: interpolation with
// prescribed end derivatives, vector fields in a curve's own basis,
// polynomial forms of rational curves, and Gordon surfaces through curve
// networks. Internal to the geometry library.

#include <array>
#include <functional>
#include <memory>
#include <optional>
#include <vector>

#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_Array1.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

class math_Gauss;

namespace mitcad::geometry::detail {

// Derivatives prescribed at one end of an interpolation, with respect to its
// parameter: none, the first, or the first and the second.
struct EndDerivatives {
  std::optional<gp_Vec> first;
  std::optional<gp_Vec> second; // only together with `first`

  int count() const { return first ? (second ? 2 : 1) : 0; }
};

// The B-spline basis that interpolates values at increasing parameters
// together with `start` and `end` derivatives (0, 1 or 2 each) at the first
// and the last parameter: degree 3, or 5 when a second derivative is given,
// lower when there are fewer conditions (one less than their number); knots
// by averaging the parameters, each end repeated once per derivative, so
// the Schoenberg-Whitney conditions hold. The basis depends only on the
// parameters and the numbers of derivatives, so the rows of a surface
// interpolated with it share their knots.
class EndInterpolation {
public:
  EndInterpolation(std::vector<double> parameters, int start, int end);
  ~EndInterpolation();
  EndInterpolation(EndInterpolation&&) noexcept;
  EndInterpolation& operator=(EndInterpolation&&) noexcept;

  int degree() const { return m_degree; }
  int pole_count() const { return static_cast<int>(m_sites.size()); }
  const NCollection_Array1<double>& knots() const { return m_knots; }
  const NCollection_Array1<int>& multiplicities() const { return m_mults; }

  // The poles of one coordinate: `values` at the parameters, `start` and
  // `end` the derivatives (first, then second) at the ends.
  std::vector<double> solve(const std::vector<double>& values, const std::vector<double>& start,
                            const std::vector<double>& end) const;

private:
  std::vector<double> m_parameters;
  int m_start = 0;
  int m_end = 0;
  int m_degree = 1;
  std::vector<double> m_sites;
  NCollection_Array1<double> m_knots;
  NCollection_Array1<int> m_mults;
  std::unique_ptr<math_Gauss> m_solver;
};

// The polynomial B-spline curve through the points at the parameters with
// the end derivatives (see EndInterpolation).
occ::handle<Geom_BSplineCurve> interpolate_with_ends(const std::vector<gp_Pnt>& points,
                                                     const std::vector<double>& parameters,
                                                     const EndDerivatives& start,
                                                     const EndDerivatives& end);

// A vector field along a non-periodic B-spline curve in the curve's own
// basis (its knots, degree and weights): the vectors D_k with
// sum_k R_k(u) D_k = f(u) at the Greville abscissae of the curve, R_k its
// rational basis. A field that is an affine image of the curve is
// reproduced exactly.
std::vector<gp_Vec> fit_field(const Geom_BSplineCurve& curve, const std::function<gp_Vec(double)>& f);

// The curve as a polynomial B-spline: rational curves (arcs, ellipses) are
// approximated within `tolerance`, so that curves of different kinds get
// common weights. Polynomial curves come back as they are.
occ::handle<Geom_BSplineCurve> polynomial(const occ::handle<Geom_BSplineCurve>& curve,
                                          double tolerance = 1.0e-7);

// The Gordon surface through a network: profiles along the surface's u at
// increasing v, guides along v; every profile must meet every guide. Exact
// construction only; throws with the reason when it fails or when the
// surface strays from a curve by more than `tolerance`.
occ::handle<Geom_BSplineSurface> gordon_surface(const std::vector<occ::handle<Geom_Curve>>& profiles,
                                                const std::vector<occ::handle<Geom_Curve>>& guides,
                                                double tolerance);

} // namespace mitcad::geometry::detail
