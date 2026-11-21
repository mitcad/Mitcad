// SPDX-License-Identifier: MIT
// Port patch 0173: a stripe whose walk failed after a walk rolling on an
// edge (whose end, where the surface's contact reaches an arc, 0170 solves
// with the rolling ball's equations) is computed again with OCCT's
// equations in that inverse function, and kept where that succeeds. Where
// the walk that starts from such an end took no step, the rounding failed.
//
// A sheet of four faces: a plate 10 x 20 (z = 0) with a round hole of
// radius 3 whose rim comes within `gap` of the plate's edge x = 10; a
// conical wall of polynomial B-splines (ruled between the rim and a circle
// of radius 2 at z = 2, in two halves, open at the top) standing on the
// rim; and a side wall 7.5 deep hanging from the plate's edge x = 10. The
// rim's half on the side of x = 10 rounded with 0.5: the contact on the
// plate runs off the plate across its edge with the side wall where the
// rim comes near it, and the ball rolls on that edge and back. At the
// sizes and gaps where OCCT's walk from the rolling ball's end took no step
// (the gap 3e-5 and 2.5e-5 at size 1, 1.5e-5 at size 0.5): built, valid,
// the sheet's area as at the gaps where the walk goes through (355.762,
// within 1e-6). The boundary: gaps of 5e-5 and 1e-4, which OCCT builds
// with the rolling ball's equations alone, the same.

#include <cmath>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFill.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Trsf.hxx>

#include "port_test.hpp"

namespace {

// Half a circle (centre c, radius r, at height z) from the angle a0 on, a
// polynomial B-spline through 41 of its points.
occ::handle<Geom_BSplineCurve> half_circle(const gp_Pnt& c, double r, double z, double a0) {
  NCollection_Array1<gp_Pnt> points(1, 41);
  for (int i = 0; i < 41; ++i) {
    const double a = a0 + test::kPi * i / 40.0;
    points.SetValue(i + 1, gp_Pnt(c.X() + r * std::cos(a), c.Y() + r * std::sin(a), z));
  }
  return GeomAPI_PointsToBSpline(points, 3, 8, GeomAbs_C2, 1e-7).Curve();
}

// The sheet at size 1, the rim `gap` from the plate's edge x = 10.
TopoDS_Shape sheet(double gap) {
  const gp_Pnt      c(7.0 - gap, 10, 0);
  const TopoDS_Edge rim_near = BRepBuilderAPI_MakeEdge(half_circle(c, 3, 0, -0.5 * test::kPi)).Edge();
  const TopoDS_Edge rim_far  = BRepBuilderAPI_MakeEdge(half_circle(c, 3, 0, 0.5 * test::kPi)).Edge();
  const TopoDS_Edge top_near = BRepBuilderAPI_MakeEdge(half_circle(c, 2, 2, -0.5 * test::kPi)).Edge();
  const TopoDS_Edge top_far  = BRepBuilderAPI_MakeEdge(half_circle(c, 2, 2, 0.5 * test::kPi)).Edge();
  BRepBuilderAPI_Sewing      sewing(1e-6);
  BRepBuilderAPI_MakePolygon outline;
  outline.Add(gp_Pnt(0, 0, 0));
  outline.Add(gp_Pnt(10, 0, 0));
  outline.Add(gp_Pnt(10, 20, 0));
  outline.Add(gp_Pnt(0, 20, 0));
  outline.Close();
  BRepBuilderAPI_MakeFace plate(outline.Wire(), true);
  BRepBuilderAPI_MakeWire hole;
  hole.Add(rim_near);
  hole.Add(rim_far);
  plate.Add(TopoDS::Wire(hole.Wire().Reversed()));
  sewing.Add(plate.Face());
  BRepBuilderAPI_MakePolygon side;
  side.Add(gp_Pnt(10, 0, 0));
  side.Add(gp_Pnt(10, 0, -7.5));
  side.Add(gp_Pnt(10, 20, -7.5));
  side.Add(gp_Pnt(10, 20, 0));
  side.Close();
  sewing.Add(BRepBuilderAPI_MakeFace(side.Wire(), true).Face());
  sewing.Add(BRepFill::Face(rim_near, top_near));
  sewing.Add(BRepFill::Face(rim_far, top_far));
  sewing.Perform();
  return sewing.SewedShape();
}

// The sheet scaled by `size`, the rim's near half rounded with 0.5 times
// the size: its area divided by the size's square; -1 where the rounding
// fails or is invalid.
double rounded_area(double gap, double size) {
  gp_Trsf scaling;
  scaling.SetScale(gp_Pnt(0, 0, 0), size);
  const TopoDS_Shape       body = BRepBuilderAPI_Transform(sheet(gap), scaling, true).Shape();
  std::vector<TopoDS_Edge> near_half;
  for (const TopoDS_Edge& edge : port::edges_where(
         body, [size](const gp_Pnt& m) { return std::abs(m.Z()) < 1e-7 * size && m.X() > 9.0 * size; })) {
    if (std::abs(BRep_Tool::Pnt(TopExp::FirstVertex(edge)).Y() - 10.0 * size) < 5.0 * size) {
      near_half.push_back(edge);
    }
  }
  CHECK(near_half.size() == 1);
  if (near_half.size() != 1) {
    return -1.0;
  }
  const TopoDS_Shape result = port::rounded(body, near_half, 0.5 * size);
  if (!port::valid(result)) {
    std::fprintf(stderr, "rim %g from the edge at size %g: no valid rounding\n", gap, size);
    return -1.0;
  }
  return port::area_of(result) / (size * size);
}

// Where the walk from the rolling ball's end took no step.
void test_rounding_rolled_on_the_plates_edge_and_walked_on() {
  const double reference = rounded_area(1e-4, 1.0);
  CHECK(reference > 355.0 && reference < 356.5);
  const double cases[3][2] = {{3e-5, 1.0}, {2.5e-5, 1.0}, {1.5e-5, 0.5}};
  for (const auto& c : cases) {
    const double area = rounded_area(c[0], c[1]);
    CHECK(area > 0.0);
    if (area > 0.0) {
      CHECK_NEAR_TOL(area, reference, 1e-6);
    }
  }
}

// The boundary: gaps the walk goes through with the rolling ball's
// equations alone.
void test_rounding_with_a_wider_gap_is_as_before() {
  const double reference = rounded_area(1e-4, 1.0);
  for (const double gap : {5e-5, 1e-4}) {
    for (const double size : {0.5, 2.0}) {
      const double area = rounded_area(gap, size);
      CHECK(area > 0.0);
      if (area > 0.0) {
        CHECK_NEAR_TOL(area, reference, 1e-6);
      }
    }
  }
}

} // namespace

MITCAD_PORT_TEST("0173", test_rounding_rolled_on_the_plates_edge_and_walked_on);
MITCAD_PORT_TEST("0173", test_rounding_with_a_wider_gap_is_as_before);
