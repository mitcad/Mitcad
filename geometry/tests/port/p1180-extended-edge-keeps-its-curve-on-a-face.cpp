// SPDX-License-Identifier: MIT
// Port patch 1180: extending an offset edge (BRepOffset_Inter2d::ExtentEdge)
// keeps a curve on a face that lies on the edge all along the edge's own
// range where reprojecting it fails, instead of letting SameParameter
// replace it.
//
// ExtentEdge extends an edge without a 3D curve, made of curves on two
// faces' surfaces, for the intersections of an offset: each curve is cut to
// its surface's bounds, the 3D curve is rebuilt on the narrower one over
// that range (here the whole extended range), and every other curve is
// projected again from it (GeomProjLib::Curve2d). Where the surfaces part
// beyond the edge's own range (the halves of a loft side in mitcad#1201,
// extended by different lengths), the rebuilt curve's first point lies off
// the other surface; the projection onto a B-spline surface
// (ProjLib_ComputeApproxOnPolarSurface::BuildInitialCurve2d) gives up
// where that point lies more than 100 times its tolerance, 100 * 1e-7 mm,
// from the surface, and returns no curve. Without the patch the edge then
// went through BRepLib::SameParameter, which cannot make the exact curve
// follow a 3D curve that leaves its surface: the edge came out with its
// tolerance raised to that distance (or not same-parameter at all), and
// its curve on that face reparametrized. With the patch the curve is kept
// where it lies on the rebuilt 3D curve at 33 points of the edge's own
// range, and the edge keeps its tolerance.
//
// The edge here: the segment from (0, 0, 0) to (10, 0, 0) (its range 0 to
// 10) with two curves, both the line v = 0 with u = t: one on the plane
// z = 0 trimmed to u from -2 to 12 (the narrower: the 3D curve is rebuilt
// on it from -2 to 12, the straight line (t, 0, 0)), the other on a B-spline
// surface of degree 1 from u = -4 to 14 which is the plane z = 0 from
// u = 0 on but rises as a ramp of slope 1/4 before it (z = -u / 4 there).
// The 3D curve's first point, (-2, 0, 0), lies 2 / sqrt(17) = 0.485 mm from
// the ramp, so the projection fails; along the edge's own range (0 to 10)
// the curve on the B-spline lies on the 3D curve exactly. Checked: the
// extended edge keeps its tolerance (1e-7) and stays same-parameter, and
// its curve on the B-spline still lies on the 3D curve within 1e-6 over the
// edge's range. Without the patch SameParameter leaves the edge 0.485 mm
// wide or not same-parameter.
//
// The boundaries: the B-spline surface flat all along (the projection
// succeeds and replaces the curve, with and without the patch), and the
// ramp ending at u = 5, inside the edge's own range (the curve does not
// follow the 3D curve there, so the patch keeps nothing and SameParameter
// runs as before: the edge is not left exact).

#include <algorithm>
#include <cstdio>

#include <BRepOffset_Inter2d.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Line.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <TopLoc_Location.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pnt2d.hxx>

#include "port_test.hpp"

namespace {

constexpr double kEdgeTolerance = 1e-7;

// The B-spline surface of degree 1 over u from -4 to 14 and v from -1 to 1:
// (x, y, z) = (u, v, z(u)) with z rising linearly from 0 at u = `ramp_end`
// to `rise` at u = -4, and 0 from `ramp_end` to 14.
occ::handle<Geom_BSplineSurface> ramp(double ramp_end, double rise) {
  const double               xs[4] = {-4.0, ramp_end, 10.0, 14.0};
  const double               zs[4] = {rise, 0.0, 0.0, 0.0};
  NCollection_Array2<gp_Pnt> poles(1, 4, 1, 2);
  for (int i = 1; i <= 4; ++i) {
    poles.SetValue(i, 1, gp_Pnt(xs[i - 1], -1.0, zs[i - 1]));
    poles.SetValue(i, 2, gp_Pnt(xs[i - 1], 1.0, zs[i - 1]));
  }
  NCollection_Array1<double> uknots(1, 4);
  NCollection_Array1<int>    umults(1, 4);
  for (int i = 1; i <= 4; ++i) {
    uknots.SetValue(i, xs[i - 1]);
    umults.SetValue(i, (i == 1 || i == 4) ? 2 : 1);
  }
  NCollection_Array1<double> vknots(1, 2);
  NCollection_Array1<int>    vmults(1, 2);
  vknots.SetValue(1, -1.0);
  vknots.SetValue(2, 1.0);
  vmults.SetValue(1, 2);
  vmults.SetValue(2, 2);
  return new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, 1, 1);
}

struct Extended {
  bool                             done = false;
  TopoDS_Edge                      edge;
  occ::handle<Geom_BSplineSurface> surface; // the second face's surface
};

// The edge from (0, 0, 0) to (10, 0, 0), as curves on the trimmed plane
// and on `surface`, extended by ExtentEdge.
Extended extended(const occ::handle<Geom_BSplineSurface>& surface) {
  const occ::handle<Geom_Plane>   base  = new Geom_Plane(gp_Ax3(gp::XOY()));
  const occ::handle<Geom_Surface> plane = new Geom_RectangularTrimmedSurface(base, -2.0, 12.0, -5.0, 5.0);
  const occ::handle<Geom2d_Line> along = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(1.0, 0.0));
  BRep_Builder                   builder;
  TopoDS_Edge                    edge;
  builder.MakeEdge(edge);
  builder.UpdateEdge(edge, along, plane, TopLoc_Location(), kEdgeTolerance);
  // A curve of its own on the second surface (ExtentEdge tells the
  // narrower curve from the others by its handle).
  const occ::handle<Geom2d_Line> across = new Geom2d_Line(gp_Pnt2d(0.0, 0.0), gp_Dir2d(1.0, 0.0));
  builder.UpdateEdge(edge, across, surface, TopLoc_Location(), kEdgeTolerance);
  builder.Range(edge, 0.0, 10.0);
  Extended result;
  result.surface = surface;
  try {
    OCC_CATCH_SIGNALS
    result.done = BRepOffset_Inter2d::ExtentEdge(edge, result.edge, 1.0);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "ExtentEdge: %s\n", failure.what());
    result.done = false;
  }
  return result;
}

// Whether the extended edge is exact: same-parameter within 1e-6 mm, its
// curve on the B-spline surface on its 3D curve within 1e-6 mm at every
// millimetre of the edge's own range.
bool exact(const Extended& e) {
  if (!e.done || e.edge.IsNull()) {
    return false;
  }
  double                        first = 0.0, last = 0.0;
  const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(e.edge, first, last);
  double                        pfirst = 0.0, plast = 0.0;
  const occ::handle<Geom2d_Curve> pcurve =
    BRep_Tool::CurveOnSurface(e.edge, e.surface, TopLoc_Location(), pfirst, plast);
  const double tolerance = BRep_Tool::Tolerance(e.edge);
  const bool   same      = BRep_Tool::SameParameter(e.edge);
  double       off       = 0.0;
  if (!curve.IsNull() && !pcurve.IsNull()) {
    for (int i = 0; i <= 10; ++i) {
      const double   t  = static_cast<double>(i);
      const gp_Pnt2d uv = pcurve->Value(t);
      off               = std::max(off, e.surface->Value(uv.X(), uv.Y()).Distance(curve->Value(t)));
    }
  }
  std::printf("  extended over [%g, %g], tolerance %.3g, %s, its curve on the B-spline %.3g mm off\n", first, last,
              tolerance, same ? "same parameter" : "not same parameter", off);
  return !curve.IsNull() && !pcurve.IsNull() && same && tolerance <= 1e-6 && off <= 1e-6;
}

// The surfaces part beyond the edge's own range: the curve is kept.
void test_curve_kept_where_the_surfaces_part_beyond_the_edge() {
  const Extended e = extended(ramp(0.0, 1.0));
  CHECK(e.done);
  CHECK(exact(e));
}

// The boundary: the surface flat all along, the projection succeeds.
void test_curve_projected_where_the_surfaces_agree() {
  const Extended e = extended(ramp(0.0, 0.0));
  CHECK(e.done);
  CHECK(exact(e));
}

// The boundary: the surfaces part within the edge's own range (the ramp up
// to u = 5, z = (5 - u) / 4): the curve is not on the 3D curve there, so it
// is not kept, and the edge is not left exact.
void test_curve_not_kept_where_the_surfaces_part_within_the_edge() {
  const Extended e = extended(ramp(5.0, 2.25));
  CHECK(e.done);
  CHECK(!exact(e));
}

} // namespace

MITCAD_PORT_TEST("1180", test_curve_kept_where_the_surfaces_part_beyond_the_edge);
MITCAD_PORT_TEST("1180", test_curve_projected_where_the_surfaces_agree);
MITCAD_PORT_TEST("1180", test_curve_not_kept_where_the_surfaces_part_within_the_edge);
