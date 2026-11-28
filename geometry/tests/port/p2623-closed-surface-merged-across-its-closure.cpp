// SPDX-License-Identifier: MIT
// Port patch 2623: ShapeUpgrade_UnifySameDomain joining B-spline faces (as
// the booleans' SimplifyResult asks for) across the edge where their
// surface closes on itself without being periodic. The merge makes the
// surface periodic in that direction (a B-spline made of any other surface
// first) and, when the faces' union falls into two intervals of the
// period, moves them to a new origin and trims the surface over one
// period from there.
//
// The ring: the exact rational circle of radius 3 round (10, 0, 0) in the
// xz plane, from x = 13, revolved about z (a surface of revolution closed
// in v, not periodic), its inner half (inside the cylinder of radius 10)
// and its half at y < 0 cut away: two faces of the revolution meet at the
// circle's start. Stock: the merge trims the surface of revolution itself
// past its v bounds and throws ("V parameters out of range"); with the
// patch one face on the trimmed periodic B-spline, valid, the volume kept
// (pi x (10 + 4 / pi) x 9 pi / 2) within the approximation's tolerance.
//
// The tube: a tube of radius 10 and height 10 whose side is a rational
// B-spline closed in u, not periodic, its half at x < 0 cut away: the two
// side faces meet at the circle's start and merge into one face, stock as
// with the patch. Stock, the merge made the faces' surface, which the
// input shape shares, periodic in place; with the patch the input's
// surfaces stay as they were.

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>

#include "port_test.hpp"

namespace {

// The nine poles of the rational circle of radius r from its point at +x,
// its centre at `centre`, in the plane spanned by the x axis and `up`
// (the z axis: the xz plane; the y axis: a plane z = const), and its
// weights.
void circle_poles(const gp_Pnt& centre, const gp_Vec& up, double r, NCollection_Array1<gp_Pnt>& poles,
                  NCollection_Array1<double>& weights) {
  const double w = std::sqrt(2.0) / 2.0;
  const double c[9] = {1, 1, 0, -1, -1, -1, 0, 1, 1};
  const double d[9] = {0, 1, 1, 1, 0, -1, -1, -1, 0};
  for (int i = 0; i < 9; ++i) {
    poles(i + 1) = centre.Translated(gp_Vec(r * c[i], 0, 0) + up * (r * d[i]));
    weights(i + 1) = (i % 2 == 1) ? w : 1.0;
  }
}

// The circle's knots: 0 to 4, a quarter each.
void circle_knots(NCollection_Array1<double>& knots, NCollection_Array1<int>& mults) {
  for (int i = 1; i <= 5; ++i) {
    knots(i) = i - 1;
    mults(i) = (i == 1 || i == 5) ? 3 : 2;
  }
}

// The surface of a face without a trim.
occ::handle<Geom_Surface> surface_of(const TopoDS_Face& face) {
  occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (const auto trimmed = occ::down_cast<Geom_RectangularTrimmedSurface>(surface); !trimmed.IsNull()) {
    surface = trimmed->BasisSurface();
  }
  return surface;
}

// The faces of the shape on a periodic B-spline surface (trimmed or not).
int faces_on_periodic_bsplines(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const auto spline = occ::down_cast<Geom_BSplineSurface>(surface_of(TopoDS::Face(f.Current())));
    if (!spline.IsNull() && (spline->IsUPeriodic() || spline->IsVPeriodic())) {
      ++count;
    }
  }
  return count;
}

// The shape unified as the booleans' SimplifyResult does (edges, faces,
// B-spline faces joined; the safe input mode); a null shape when the
// unification throws.
TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  try {
    OCC_CATCH_SIGNALS
    unify.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "unify: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return unify.Shape();
}

// The ring: the revolved circle's outer half at y >= 0.
TopoDS_Shape ring() {
  NCollection_Array1<gp_Pnt> poles(1, 9);
  NCollection_Array1<double> weights(1, 9);
  circle_poles(gp_Pnt(10, 0, 0), gp_Vec(0, 0, 1), 3.0, poles, weights);
  NCollection_Array1<double> knots(1, 5);
  NCollection_Array1<int>    mults(1, 5);
  circle_knots(knots, mults);
  const occ::handle<Geom_BSplineCurve> profile = new Geom_BSplineCurve(poles, weights, knots, mults, 2);
  const TopoDS_Edge  edge  = BRepBuilderAPI_MakeEdge(profile).Edge();
  const TopoDS_Wire  wire  = BRepBuilderAPI_MakeWire(edge).Wire();
  const TopoDS_Face  disc  = BRepBuilderAPI_MakeFace(wire, true).Face();
  const TopoDS_Shape torus = BRepPrimAPI_MakeRevol(disc, gp_Ax1(gp::Origin(), gp::DZ())).Shape();
  const TopoDS_Shape core  = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -10), gp::DZ()), 10, 20).Shape();
  const TopoDS_Shape outer = BRepAlgoAPI_Cut(torus, core).Shape();
  const TopoDS_Shape box   = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -10), 40, 20, 20).Shape();
  return BRepAlgoAPI_Cut(outer, box).Shape();
}

// A planar disc of radius 10 at height z.
TopoDS_Face cap(double z) {
  const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, z), gp::DZ(), gp::DX()), 10);
  return BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle).Edge()).Wire(), true)
    .Face();
}

// The tube's half at x >= 0.
TopoDS_Shape half_tube() {
  NCollection_Array2<gp_Pnt> poles(1, 9, 1, 2);
  NCollection_Array2<double> weights(1, 9, 1, 2);
  for (int j = 1; j <= 2; ++j) {
    NCollection_Array1<gp_Pnt> ring_poles(1, 9);
    NCollection_Array1<double> ring_weights(1, 9);
    circle_poles(gp_Pnt(0, 0, j == 1 ? 0.0 : 10.0), gp_Vec(0, 1, 0), 10.0, ring_poles, ring_weights);
    for (int i = 1; i <= 9; ++i) {
      poles(i, j)   = ring_poles(i);
      weights(i, j) = ring_weights(i);
    }
  }
  NCollection_Array1<double> uknots(1, 5);
  NCollection_Array1<int>    umults(1, 5);
  circle_knots(uknots, umults);
  NCollection_Array1<double> vknots(1, 2);
  NCollection_Array1<int>    vmults(1, 2);
  vknots(1) = 0.0;
  vknots(2) = 10.0;
  vmults(1) = 2;
  vmults(2) = 2;
  const occ::handle<Geom_BSplineSurface> side_surface =
    new Geom_BSplineSurface(poles, weights, uknots, vknots, umults, vmults, 2, 1);
  BRepBuilderAPI_Sewing sewing(1e-6);
  sewing.Add(BRepBuilderAPI_MakeFace(side_surface, 1e-7).Face());
  sewing.Add(cap(0.0));
  sewing.Add(cap(10.0));
  sewing.Perform();
  TopExp_Explorer shell(sewing.SewedShape(), TopAbs_SHELL);
  if (!shell.More()) {
    return TopoDS_Shape();
  }
  TopoDS_Solid tube = BRepBuilderAPI_MakeSolid(TopoDS::Shell(shell.Current())).Solid();
  BRepLib::OrientClosedSolid(tube);
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -1), 20, 40, 12).Shape();
  return BRepAlgoAPI_Cut(tube, box).Shape();
}

// The ring's two faces of the revolution merged into one on the periodic
// B-spline made of it (stock: the trim of the surface of revolution
// throws).
void test_faces_merged_across_the_closure_of_a_surface_of_revolution() {
  const TopoDS_Shape input = ring();
  CHECK(port::valid(input));
  CHECK(port::count_of(input, TopAbs_FACE) == 5);
  const double exact = test::kPi * (10.0 + 4.0 / test::kPi) * 9.0 * test::kPi / 2.0;
  CHECK_NEAR_TOL(port::volume_of(input), exact, 1e-5);
  const TopoDS_Shape result = unified(input);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_FACE) == 4);
  CHECK(faces_on_periodic_bsplines(result) == 1);
  CHECK_NEAR_TOL(port::volume_of(result), exact, 1e-3);
}

// The tube's two side faces merged into one; the input's B-spline surface
// is not made periodic under it.
void test_input_bspline_not_made_periodic_in_place() {
  const TopoDS_Shape input = half_tube();
  CHECK(port::valid(input));
  CHECK(port::count_of(input, TopAbs_FACE) == 5);
  CHECK(faces_on_periodic_bsplines(input) == 0);
  const TopoDS_Shape result = unified(input);
  CHECK(!result.IsNull());
  CHECK(faces_on_periodic_bsplines(input) == 0);
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_FACE) == 4);
  CHECK_NEAR_TOL(port::volume_of(result), 500.0 * test::kPi, 1e-4);
}

} // namespace

MITCAD_PORT_TEST("2623", test_faces_merged_across_the_closure_of_a_surface_of_revolution);
MITCAD_PORT_TEST("2623", test_input_bspline_not_made_periodic_in_place);
