// SPDX-License-Identifier: MIT
// Normal projections onto a bounded plane whose UV tolerance corresponds
// to less than the spacing of the curve's floating-point parameters. The
// projection's boundary search must finish at the nearest representable
// parameter, with the port's patch 0960 (mitcad#504).

#include <cmath>
#include <cstdio>

#include <GeomAdaptor_Curve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <NCollection_Array1.hxx>
#include <ProjLib_CompProjectedCurve.hxx>
#include <Standard_Failure.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace {

// A straight segment from (-1,0,1) to (1,0,1), parameterized over a short
// knot span. Its normal projection enters and leaves a rectangle of z=0
// at x=-0.33 and x=0.7. With the parameter offset of 1e6, neither crossing
// has a representable parameter: the adjacent parameters are about 2e-4
// apart in x. Check their bracket, and the projection of the represented
// curve points, rather than require a point the curve cannot evaluate.
void projection_crossing_plane_bounds(double first, double span) {
  NCollection_Array1<gp_Pnt> poles(1, 2);
  poles(1) = gp_Pnt(-1, 0, 1);
  poles(2) = gp_Pnt(1, 0, 1);
  NCollection_Array1<double> knots(1, 2);
  knots(1) = first;
  knots(2) = first + span;
  NCollection_Array1<int> mults(1, 2);
  mults.Init(2);
  occ::handle<Geom_BSplineCurve> line = new Geom_BSplineCurve(poles, knots, mults, 1);
  occ::handle<Geom_Plane> plane = new Geom_Plane(gp_Pln());
  occ::handle<Geom_RectangularTrimmedSurface> trimmed =
      new Geom_RectangularTrimmedSurface(plane, -0.33, 0.7, -1.0, 1.0);
  occ::handle<GeomAdaptor_Curve> curve = new GeomAdaptor_Curve(line);
  occ::handle<GeomAdaptor_Surface> surface = new GeomAdaptor_Surface(trimmed);

  ProjLib_CompProjectedCurve projection(surface, curve, 1e-12, 1e-12);
  CHECK(projection.NbCurves() == 1);
  if (projection.NbCurves() != 1) {
    return;
  }
  double begin, end;
  projection.Bounds(1, begin, end);
  CHECK(begin >= first);
  CHECK(end <= first + span);
  CHECK(begin < end);
  const double before = std::nextafter(begin, first);
  const double after = std::nextafter(end, first + span);
  CHECK(curve->Value(begin).X() >= -0.33 - 1e-6);
  CHECK(curve->Value(before).X() <= -0.33 + 1e-6);
  CHECK(curve->Value(end).X() <= 0.7 + 1e-6);
  CHECK(curve->Value(after).X() >= 0.7 - 1e-6);
  for (const double parameter : {begin, begin + (end - begin) / 2, end}) {
    const gp_Pnt2d uv = projection.Value(parameter);
    const gp_Pnt projected = surface->Value(uv.X(), uv.Y());
    const gp_Pnt point = curve->Value(parameter);
    CHECK_NEAR(projected.X(), point.X());
    CHECK_NEAR(projected.Y(), point.Y());
    CHECK_NEAR(projected.Z(), 0.0);
  }
}

} // namespace

MITCAD_BUNDLED_TEST(projection_boundary) {
  try {
    projection_crossing_plane_bounds(1.0, 1e-6);
    projection_crossing_plane_bounds(1e6, 1e-6);
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "projection boundary threw: %s\n", e.what());
    ++test::failures;
  }
  return test::failures == 0 ? 0 : 1;
}
