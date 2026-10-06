// SPDX-License-Identifier: MIT
#include "skin.hpp"

#include <algorithm>
#include <cmath>
#include <stdexcept>
#include <string>
#include <utility>

#include <BSplCLib.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <GeomFill_Gordon.hxx>
#include <math_Gauss.hxx>
#include <math_Matrix.hxx>
#include <math_Vector.hxx>

namespace mitcad::geometry::detail {
namespace {

// The distinct knots of a flat knot sequence and their multiplicities.
void split_flat(const std::vector<double>& flat, NCollection_Array1<double>& knots,
                NCollection_Array1<int>& mults) {
  std::vector<double> distinct;
  std::vector<int> counts;
  for (const double knot : flat) {
    if (!distinct.empty() && knot == distinct.back()) {
      ++counts.back();
    } else {
      distinct.push_back(knot);
      counts.push_back(1);
    }
  }
  knots.Resize(1, static_cast<int>(distinct.size()), false);
  mults.Resize(1, static_cast<int>(counts.size()), false);
  for (std::size_t i = 0; i < distinct.size(); ++i) {
    knots(static_cast<int>(i) + 1) = distinct[i];
    mults(static_cast<int>(i) + 1) = counts[i];
  }
}

// The derivatives of order `order` (0: the values) of the basis functions
// of degree `degree` at `parameter`, written into `row` of `matrix` (the
// columns are the 1-based poles).
void put_basis(math_Matrix& matrix, int row, const NCollection_Array1<double>& flat, int degree,
               double parameter, int order) {
  math_Matrix basis(1, order + 1, 1, degree + 1, 0.0);
  int first = 0;
  if (BSplCLib::EvalBsplineBasis(order, degree + 1, flat, parameter, first, basis) != 0) {
    throw std::runtime_error("a B-spline basis could not be evaluated");
  }
  for (int k = 1; k <= degree + 1; ++k) {
    matrix(row, first + k - 1) = basis(order + 1, k);
  }
}

const char* gordon_status(GeomFill_Gordon::ResultStatus status) {
  using Status = GeomFill_Gordon::ResultStatus;
  switch (status) {
  case Status::InvalidInput:
    return "too few curves";
  case Status::IntersectionFailed:
    return "the curves do not all meet";
  case Status::OrderingFailed:
    return "the curves could not be ordered";
  case Status::RationalDegreeOverflow:
  case Status::RationalConstructionFailed:
  case Status::RationalReparametrizationFailed:
    return "the rational curves could not be combined";
  default:
    return "the network could not be made compatible";
  }
}

} // namespace

EndInterpolation::EndInterpolation(std::vector<double> parameters, int start, int end)
    : m_parameters(std::move(parameters)), m_start(start), m_end(end) {
  const std::size_t n = m_parameters.size();
  if (n < 2 || start < 0 || start > 2 || end < 0 || end > 2) {
    throw std::invalid_argument("an interpolation needs two or more points");
  }
  for (std::size_t i = 1; i < n; ++i) {
    if (!(m_parameters[i] > m_parameters[i - 1])) {
      throw std::invalid_argument("the parameters of an interpolation must increase");
    }
  }
  // The data sites: the ends once per value and once per derivative.
  m_sites.assign(static_cast<std::size_t>(start) + 1, m_parameters.front());
  m_sites.insert(m_sites.end(), m_parameters.begin() + 1, m_parameters.end() - 1);
  m_sites.insert(m_sites.end(), static_cast<std::size_t>(end) + 1, m_parameters.back());
  const int m = static_cast<int>(m_sites.size());
  m_degree = std::min(start == 2 || end == 2 ? 5 : 3, m - 1);
  const int p = m_degree;
  std::vector<double> flat(static_cast<std::size_t>(p) + 1, m_sites.front());
  for (int j = 1; j <= m - 1 - p; ++j) {
    double sum = 0.0;
    for (int i = j; i < j + p; ++i) {
      sum += m_sites[static_cast<std::size_t>(i)];
    }
    flat.push_back(sum / p);
  }
  flat.insert(flat.end(), static_cast<std::size_t>(p) + 1, m_sites.back());
  split_flat(flat, m_knots, m_mults);

  NCollection_Array1<double> flat_knots(1, static_cast<int>(flat.size()));
  for (std::size_t i = 0; i < flat.size(); ++i) {
    flat_knots(static_cast<int>(i) + 1) = flat[i];
  }
  // Rows in the order solve() puts the conditions: the first value, the
  // start derivatives, the inner values, the end derivatives, the last
  // value.
  math_Matrix matrix(1, m, 1, m, 0.0);
  int row = 1;
  put_basis(matrix, row++, flat_knots, p, m_parameters.front(), 0);
  for (int d = 1; d <= start; ++d) {
    put_basis(matrix, row++, flat_knots, p, m_parameters.front(), d);
  }
  for (std::size_t i = 1; i + 1 < n; ++i) {
    put_basis(matrix, row++, flat_knots, p, m_parameters[i], 0);
  }
  for (int d = 1; d <= end; ++d) {
    put_basis(matrix, row++, flat_knots, p, m_parameters.back(), d);
  }
  put_basis(matrix, row, flat_knots, p, m_parameters.back(), 0);
  m_solver = std::make_unique<math_Gauss>(matrix);
  if (!m_solver->IsDone()) {
    throw std::runtime_error("the interpolation conditions are singular");
  }
}

EndInterpolation::~EndInterpolation() = default;
EndInterpolation::EndInterpolation(EndInterpolation&&) noexcept = default;
EndInterpolation& EndInterpolation::operator=(EndInterpolation&&) noexcept = default;

std::vector<double> EndInterpolation::solve(const std::vector<double>& values,
                                            const std::vector<double>& start,
                                            const std::vector<double>& end) const {
  if (values.size() != m_parameters.size() || start.size() != static_cast<std::size_t>(m_start) ||
      end.size() != static_cast<std::size_t>(m_end)) {
    throw std::invalid_argument("the interpolation conditions do not match its basis");
  }
  const int m = pole_count();
  math_Vector b(1, m);
  int row = 1;
  b(row++) = values.front();
  for (const double d : start) {
    b(row++) = d;
  }
  for (std::size_t i = 1; i + 1 < values.size(); ++i) {
    b(row++) = values[i];
  }
  for (const double d : end) {
    b(row++) = d;
  }
  b(row) = values.back();
  math_Vector x(1, m);
  m_solver->Solve(b, x);
  std::vector<double> poles(static_cast<std::size_t>(m));
  for (int i = 1; i <= m; ++i) {
    poles[static_cast<std::size_t>(i) - 1] = x(i);
  }
  return poles;
}

occ::handle<Geom_BSplineCurve> interpolate_with_ends(const std::vector<gp_Pnt>& points,
                                                     const std::vector<double>& parameters,
                                                     const EndDerivatives& start,
                                                     const EndDerivatives& end) {
  if (points.size() != parameters.size()) {
    throw std::invalid_argument("an interpolation needs a parameter per point");
  }
  const EndInterpolation basis(parameters, start.count(), end.count());
  const auto derivatives = [](const EndDerivatives& ends, int c) {
    std::vector<double> values;
    if (ends.first) {
      values.push_back(ends.first->Coord(c));
    }
    if (ends.second) {
      values.push_back(ends.second->Coord(c));
    }
    return values;
  };
  std::array<std::vector<double>, 3> coordinates;
  for (int c = 1; c <= 3; ++c) {
    std::vector<double> values;
    for (const gp_Pnt& point : points) {
      values.push_back(point.Coord(c));
    }
    coordinates[static_cast<std::size_t>(c) - 1] = basis.solve(values, derivatives(start, c), derivatives(end, c));
  }
  NCollection_Array1<gp_Pnt> poles(1, basis.pole_count());
  for (int i = 1; i <= basis.pole_count(); ++i) {
    const std::size_t k = static_cast<std::size_t>(i) - 1;
    poles(i) = gp_Pnt(coordinates[0][k], coordinates[1][k], coordinates[2][k]);
  }
  return new Geom_BSplineCurve(poles, basis.knots(), basis.multiplicities(), basis.degree());
}

std::vector<gp_Vec> fit_field(const Geom_BSplineCurve& curve, const std::function<gp_Vec(double)>& f) {
  if (curve.IsPeriodic()) {
    throw std::invalid_argument("a field is fitted on a non-periodic curve");
  }
  const int n = curve.NbPoles();
  const int p = curve.Degree();
  const NCollection_Array1<double>& sequence = curve.KnotSequence();
  NCollection_Array1<double> flat(1, sequence.Length());
  for (int i = 0; i < sequence.Length(); ++i) {
    flat(i + 1) = sequence(sequence.Lower() + i);
  }
  math_Matrix matrix(1, n, 1, n, 0.0);
  std::array<math_Vector, 3> rhs{math_Vector(1, n), math_Vector(1, n), math_Vector(1, n)};
  for (int j = 1; j <= n; ++j) {
    double greville = 0.0;
    for (int i = j + 1; i <= j + p; ++i) {
      greville += flat(i);
    }
    greville /= p;
    put_basis(matrix, j, flat, p, greville, 0);
    // The rational basis: N_k w_k over their sum.
    double sum = 0.0;
    for (int k = 1; k <= n; ++k) {
      matrix(j, k) *= curve.Weight(k);
      sum += matrix(j, k);
    }
    for (int k = 1; k <= n; ++k) {
      matrix(j, k) /= sum;
    }
    const gp_Vec value = f(greville);
    for (int c = 0; c < 3; ++c) {
      rhs[static_cast<std::size_t>(c)](j) = value.Coord(c + 1);
    }
  }
  const math_Gauss solver(matrix);
  if (!solver.IsDone()) {
    throw std::runtime_error("a field could not be fitted along a curve");
  }
  std::array<math_Vector, 3> solved{math_Vector(1, n), math_Vector(1, n), math_Vector(1, n)};
  for (std::size_t c = 0; c < 3; ++c) {
    solver.Solve(rhs[c], solved[c]);
  }
  std::vector<gp_Vec> field;
  for (int k = 1; k <= n; ++k) {
    field.emplace_back(solved[0](k), solved[1](k), solved[2](k));
  }
  return field;
}

occ::handle<Geom_BSplineCurve> polynomial(const occ::handle<Geom_BSplineCurve>& curve, double tolerance) {
  if (!curve->IsRational()) {
    return curve;
  }
  const GeomAbs_Shape continuity = std::min(curve->Continuity(), GeomAbs_C2);
  GeomConvert_ApproxCurve approximation(curve, tolerance, continuity, 200, 9);
  if (!approximation.HasResult() || approximation.MaxError() > tolerance) {
    throw std::runtime_error("a rational curve could not be made polynomial");
  }
  return approximation.Curve();
}

occ::handle<Geom_BSplineSurface> gordon_surface(const std::vector<occ::handle<Geom_Curve>>& profiles,
                                                const std::vector<occ::handle<Geom_Curve>>& guides,
                                                double tolerance) {
  NCollection_Array1<occ::handle<Geom_Curve>> profile_array(1, static_cast<int>(profiles.size()));
  NCollection_Array1<occ::handle<Geom_Curve>> guide_array(1, static_cast<int>(guides.size()));
  for (std::size_t i = 0; i < profiles.size(); ++i) {
    profile_array.ChangeAt(i) = profiles[i];
  }
  for (std::size_t i = 0; i < guides.size(); ++i) {
    guide_array.ChangeAt(i) = guides[i];
  }
  GeomFill_Gordon gordon;
  gordon.Init(profile_array, guide_array, tolerance);
  gordon.Perform();
  if (!gordon.IsDone()) {
    throw std::runtime_error(std::string("no surface could be built through the curve network: ") +
                             gordon_status(gordon.Status()));
  }
  const GeomFill_Gordon::BuildReport& report = gordon.Report();
  if (report.IsApproximate || report.MaxProfileDeviation > tolerance ||
      report.MaxGuideDeviation > tolerance) {
    throw std::runtime_error("the surface through the curve network strays from its curves");
  }
  return gordon.Surface();
}

} // namespace mitcad::geometry::detail
