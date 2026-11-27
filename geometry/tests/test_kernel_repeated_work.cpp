// SPDX-License-Identifier: MIT
// Repeated work taken out of OCCT with the same results (mitcad#1823):
//
// - port patch 1690: a blend's line (BRepBlend_Line) reads its points by
//   index in constant time; it keeps the order, the contents and the
//   addresses of its points as the sequence it held them in did, through
//   appends, prepends, inserts and removals;
// - port patch 1691: IntCurveSurface_HInter::Perform with a surface's kept
//   polyhedra gives the intersections that Perform without them gives, for
//   curves of several intervals, on a B-spline surface and on a surface of
//   revolution (each a polyhedron of its own kind), the polyhedra made by
//   one curve used by the next.
//
// The ctests geometry.fillet_consumed_strip and
// geometry.offset_loft_circle_seam_lean_in are the cases these patches
// were made for; their results are those of OCCT without the patches.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepBlend_Line.hxx>
#include <Blend_Point.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <IntCurveSurface_HInter.hxx>
#include <IntCurveSurface_IntersectionPoint.hxx>
#include <IntCurveSurface_IntersectionSegment.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

Blend_Point point_at(double parameter) {
  return Blend_Point(gp_Pnt(parameter, 0, 0), gp_Pnt(parameter, 1, 0), parameter, parameter, 0, parameter, 1);
}

// The parameters of the line's points in their order.
std::vector<double> parameters(const BRepBlend_Line& line) {
  std::vector<double> found;
  for (int i = 1; i <= line.NbPoints(); ++i) {
    found.push_back(line.Point(i).Parameter());
  }
  return found;
}

void test_blend_line_keeps_order_and_addresses() {
  const occ::handle<BRepBlend_Line> line = new BRepBlend_Line();
  CHECK(line->NbPoints() == 0);
  for (int i = 1; i <= 5; ++i) {
    line->Append(point_at(i));
  }
  line->Prepend(point_at(0));
  CHECK((parameters(*line) == std::vector<double>{0, 1, 2, 3, 4, 5}));
  // References stay valid while points are added and removed elsewhere, as
  // the sequence's nodes did.
  const Blend_Point* three = &line->Point(4);
  line->InsertBefore(2, point_at(0.5)); // before the second point
  line->InsertBefore(line->NbPoints() + 1, point_at(6)); // after the last
  line->InsertBefore(1, point_at(-1)); // before the first
  CHECK((parameters(*line) == std::vector<double>{-1, 0, 0.5, 1, 2, 3, 4, 5, 6}));
  CHECK(&line->Point(6) == three);
  CHECK(three->Parameter() == 3);
  line->Remove(2, 4); // 0, 0.5 and 1
  CHECK((parameters(*line) == std::vector<double>{-1, 2, 3, 4, 5, 6}));
  CHECK(&line->Point(3) == three);
  line->Remove(1, 1);
  line->Remove(line->NbPoints(), line->NbPoints());
  CHECK((parameters(*line) == std::vector<double>{2, 3, 4, 5}));
  CHECK(line->Point(1).PointOnS1().X() == 2 && line->Point(4).PointOnS2().Y() == 1);
  // A long line read at every index, in any order.
  line->Clear();
  CHECK(line->NbPoints() == 0);
  constexpr int kPoints = 20000;
  for (int i = 0; i < kPoints; ++i) {
    if (i % 2 == 0) {
      line->Append(point_at(i));
    } else {
      line->Prepend(point_at(-i));
    }
  }
  CHECK(line->NbPoints() == kPoints);
  bool ordered = true;
  for (int k = 0; k < kPoints; ++k) {
    const int i = 1 + (k * 7919) % kPoints;
    const int j = i + 1;
    if (j <= kPoints && !(line->Point(i).Parameter() < line->Point(j).Parameter())) {
      ordered = false;
    }
  }
  CHECK(ordered);
  CHECK(line->Point(1).Parameter() == -(kPoints - 1));
  CHECK(line->Point(kPoints).Parameter() == kPoints - 2);
}

// A cubic B-spline curve through the points, a knot between each two, so
// that it has an interval for each span.
occ::handle<Geom_BSplineCurve> wavy_curve(const std::vector<gp_Pnt>& poles) {
  const int n = static_cast<int>(poles.size());
  NCollection_Array1<gp_Pnt> p(1, n);
  for (int i = 0; i < n; ++i) {
    p(i + 1) = poles[static_cast<std::size_t>(i)];
  }
  const int knots = n - 2; // degree 3, clamped
  NCollection_Array1<double> k(1, knots);
  NCollection_Array1<int> m(1, knots);
  for (int i = 1; i <= knots; ++i) {
    k(i) = i - 1;
    m(i) = (i == 1 || i == knots) ? 4 : 1;
  }
  return new Geom_BSplineCurve(p, k, m, 3);
}

// A bumpy B-spline patch over [0, 40] x [0, 40], knots in both directions.
occ::handle<Geom_BSplineSurface> bumpy_surface() {
  constexpr int n = 8;
  NCollection_Array2<gp_Pnt> p(1, n, 1, n);
  for (int i = 1; i <= n; ++i) {
    for (int j = 1; j <= n; ++j) {
      p(i, j) = gp_Pnt(40.0 * (i - 1) / (n - 1), 40.0 * (j - 1) / (n - 1), 3.0 * std::sin(i * 1.3) * std::cos(j * 0.9));
    }
  }
  NCollection_Array1<double> k(1, n - 2);
  NCollection_Array1<int> m(1, n - 2);
  for (int i = 1; i <= n - 2; ++i) {
    k(i) = i - 1;
    m(i) = (i == 1 || i == n - 2) ? 4 : 1;
  }
  return new Geom_BSplineSurface(p, k, k, m, m, 3, 3);
}

// The intersections of each curve with the surface, with and without kept
// polyhedra, the same in number and in their parameters.
void compare(const char* what, const occ::handle<Adaptor3d_Surface>& surface,
             const std::vector<occ::handle<Geom_BSplineCurve>>& curves) {
  IntCurveSurface_HInter::Polyhedra kept;
  int total = 0;
  for (std::size_t c = 0; c < curves.size(); ++c) {
    const occ::handle<Adaptor3d_Curve> curve = new GeomAdaptor_Curve(curves[c]);
    IntCurveSurface_HInter fresh;
    fresh.Perform(curve, surface);
    IntCurveSurface_HInter reused;
    reused.Perform(curve, surface, kept);
    CHECK(fresh.IsDone() && reused.IsDone());
    if (!fresh.IsDone() || !reused.IsDone()) {
      continue;
    }
    bool same = fresh.NbPoints() == reused.NbPoints() && fresh.NbSegments() == reused.NbSegments();
    for (int i = 1; same && i <= fresh.NbPoints(); ++i) {
      const IntCurveSurface_IntersectionPoint& a = fresh.Point(i);
      const IntCurveSurface_IntersectionPoint& b = reused.Point(i);
      same = a.W() == b.W() && a.U() == b.U() && a.V() == b.V() && a.Pnt().IsEqual(b.Pnt(), 0.0);
    }
    if (!same) {
      std::fprintf(stderr, "%s, curve %zu: %d points with the kept polyhedra, %d without\n", what, c,
                   reused.NbPoints(), fresh.NbPoints());
      CHECK(false);
    }
    total += fresh.NbPoints();
  }
  std::printf("%s: %d intersection points, the same with and without the kept polyhedra\n", what, total);
  CHECK(total > 0);
}

std::vector<occ::handle<Geom_BSplineCurve>> crossing_curves() {
  std::vector<occ::handle<Geom_BSplineCurve>> curves;
  for (int k = 0; k < 6; ++k) {
    std::vector<gp_Pnt> poles;
    for (int i = 0; i < 9; ++i) {
      const double x = 2.0 + 4.5 * i;
      poles.emplace_back(x, 5.0 + 5.0 * k + 3.0 * std::sin(i + k), ((i + k) % 2 == 0 ? 6.0 : -6.0));
    }
    curves.push_back(wavy_curve(poles));
  }
  return curves;
}

void test_curve_surface_with_kept_polyhedra() {
  // The B-spline surface's polyhedra come from its trimmed samples.
  compare("B-spline surface", new GeomAdaptor_Surface(bumpy_surface()), crossing_curves());
  // A surface of revolution (neither a quadric nor a B-spline): a polyhedron
  // of up to 40 x 40 samples for each of its pieces.
  std::vector<gp_Pnt> profile;
  for (int i = 0; i < 7; ++i) {
    profile.emplace_back(20.0 + 2.0 * std::sin(i * 1.1), 0, 6.0 * i);
  }
  const occ::handle<Geom_SurfaceOfRevolution> revolved =
    new Geom_SurfaceOfRevolution(wavy_curve(profile), gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  std::vector<occ::handle<Geom_BSplineCurve>> through;
  for (int k = 0; k < 6; ++k) {
    std::vector<gp_Pnt> poles;
    for (int i = 0; i < 9; ++i) {
      const double t = -30.0 + 7.5 * i;
      poles.emplace_back(t, 3.0 * k - 8.0 + 2.0 * std::cos(i + k), 4.0 + 5.0 * k + std::sin(i));
    }
    through.push_back(wavy_curve(poles));
  }
  compare("surface of revolution", new GeomAdaptor_Surface(revolved, 0, 2 * kPi, 0, 4), through);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(kernel_repeated_work) {
  test::guarded("test_blend_line_keeps_order_and_addresses", test::test_blend_line_keeps_order_and_addresses);
  test::guarded("test_curve_surface_with_kept_polyhedra", test::test_curve_surface_with_kept_polyhedra);
  if (test::failures == 0) {
    std::puts("test_kernel_repeated_work: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
