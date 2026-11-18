// SPDX-License-Identifier: MIT
// Port patch 0008: BRepCheck_Analyzer finds the seam edges of a face once
// for the 2d closure check of all its wires (BRepCheck_Wire::SeamEdges,
// InContextOfFace) instead of going through all edges of the face for each
// wire, and no longer searches the face for each of its wires: both made the
// check quadratic in the wires of a face. The statuses stay as they were: a
// plate of 2 025 round holes and a cylinder's side with 96 holes are valid,
// a wire of a holed cylindrical face that is closed in space but not in the
// face's parameters (its ends at the seam a period apart) is still found,
// and a plate of 3 600 holes of 24 edges each is checked in time.

#include <chrono>
#include <cmath>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <Geom2d_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2d.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "port_test.hpp"

namespace {

// A hole of the plane z = 0 at (x, y), radius 0.3: a circle, or a regular
// polygon of `sides` edges; clockwise seen from +z, a hole of the face.
TopoDS_Wire hole(double x, double y, int sides) {
  if (sides == 0) {
    const gp_Circ circle(gp_Ax2(gp_Pnt(x, y, 0), gp_Dir(0, 0, -1)), 0.3);
    return BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle).Edge()).Wire();
  }
  BRepBuilderAPI_MakePolygon polygon;
  for (int k = 0; k < sides; ++k) {
    const double angle = -2.0 * test::kPi * k / sides;
    polygon.Add(gp_Pnt(x + 0.3 * std::cos(angle), y + 0.3 * std::sin(angle), 0));
  }
  polygon.Close();
  return polygon.Wire();
}

// The face of an n x n plate with a hole at a pitch of 1 in each cell.
TopoDS_Face holed_plate(int n, int sides) {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(n, 0, 0), gp_Pnt(n, n, 0), gp_Pnt(0, n, 0), true);
  BRepBuilderAPI_MakeFace    face(gp_Pln(), outline.Wire());
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      face.Add(hole(i + 0.5, j + 0.5, sides));
    }
  }
  return face.Face();
}

// The side of a cylinder of radius 1 round the z axis, 0 <= z <= 6 (its
// seam at u = 0), with 12 x 8 holes: circles of radius 0.2 in its
// parameters (u the angle, v = z), clockwise.
TopoDS_Face holed_cylinder_side() {
  const occ::handle<Geom_Surface> cylinder(new Geom_CylindricalSurface(gp_Ax3(), 1.0));
  const TopoDS_Face               side = BRepBuilderAPI_MakeFace(cylinder, 0.0, 2.0 * test::kPi, 0.0, 6.0, 1e-7).Face();
  TopLoc_Location                 location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(side, location);
  CHECK(location.IsIdentity());
  BRepBuilderAPI_MakeFace face(side);
  for (int i = 0; i < 12; ++i) {
    for (int j = 0; j < 8; ++j) {
      const gp_Pnt2d                  centre((i + 0.5) * 2.0 * test::kPi / 12.0, 0.6 + 0.7 * j);
      const occ::handle<Geom2d_Curve> circle(new Geom2d_Circle(gp_Ax2d(centre, gp_Dir2d(1, 0)), 0.2, false));
      const TopoDS_Wire wire = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle, surface).Edge()).Wire();
      BRepLib::BuildCurves3d(wire);
      face.Add(wire);
    }
  }
  TopoDS_Face holed = face.Face();
  BRepLib::UpdateTolerances(holed);
  return holed;
}

bool has_status(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& shape, const TopoDS_Shape& context,
                BRepCheck_Status status) {
  const occ::handle<BRepCheck_Result>& result = analyzer.Result(shape);
  if (result.IsNull() || !result->IsStatusOnShape(context)) {
    return false;
  }
  for (const BRepCheck_Status found : result->StatusOnShape(context)) {
    if (found == status) {
      return true;
    }
  }
  return false;
}

// 2 025 round holes: every wire's 2d closure checked with the seam edges
// found once (none on a plane); the face is valid.
void test_plate_of_two_thousand_holes_is_valid() {
  const TopoDS_Face plate = holed_plate(45, 0);
  CHECK(port::count_of(plate, TopAbs_WIRE) == 2026);
  CHECK(port::valid(plate));
}

// A cylindrical face, its seam found once and given to the closure check of
// each of its 97 wires: valid.
void test_cylinder_side_with_many_holes_is_valid() {
  const TopoDS_Face side = holed_cylinder_side();
  CHECK(port::count_of(side, TopAbs_WIRE) == 97);
  CHECK(port::valid(side));
}

// The holed cylindrical face with one more wire: its bottom circle alone,
// closed in space at the seam's vertex, its ends a period apart in the
// face's parameters. Its check on the face, with the face's seam edges given,
// still finds it not closed (topological checks: the 2d closure is one).
void test_wire_not_closed_in_the_face_is_found() {
  const TopoDS_Face              side   = holed_cylinder_side();
  const std::vector<TopoDS_Edge> bottom = port::edges_where(side, [](const gp_Pnt& p) { return std::abs(p.Z()) < 1e-9; });
  CHECK(bottom.size() == 1);
  if (bottom.size() != 1) {
    return;
  }
  BRep_Builder builder;
  TopoDS_Wire  open;
  builder.MakeWire(open);
  builder.Add(open, bottom.front());
  BRepBuilderAPI_MakeFace face(side);
  face.Add(open);
  const TopoDS_Face        checked = face.Face();
  const BRepCheck_Analyzer analyzer(checked, false);
  CHECK(!analyzer.IsValid());
  CHECK(has_status(analyzer, open, checked, BRepCheck_NotClosed));
  // The holes' wires are closed.
  int closed = 0;
  for (TopExp_Explorer wires(checked, TopAbs_WIRE); wires.More(); wires.Next()) {
    if (!wires.Current().IsSame(open) && has_status(analyzer, wires.Current(), checked, BRepCheck_NoError)) {
      ++closed;
    }
  }
  CHECK(closed == 97);
}

// The bound: 3 600 holes of 24 edges (86 404 edges on the face), its
// topological checks (no geometric controls: the edges' curves are not
// sampled, the wires' 2d closure and the face's wires are checked). Going
// through all edges of the face for each wire's closure (3 600 x 86 404) and
// searching the face for each wire take tens of seconds; with the seam edges
// found once the check takes a few seconds (its classification of the wires
// is quadratic in the wires too, a fraction of a second here), so 10 s
// leaves room for a loaded machine.
void test_plate_of_three_thousand_six_hundred_holes_within_ten_seconds() {
  const TopoDS_Face plate = holed_plate(60, 24);
  CHECK(port::count_of(plate, TopAbs_EDGE) == 4 + 3600 * 24);
  const auto               start = std::chrono::steady_clock::now();
  const BRepCheck_Analyzer analyzer(plate, false);
  const bool               valid   = analyzer.IsValid();
  const double             seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("topological check of a face of 3 600 holes: %.2f s\n", seconds);
  CHECK(valid);
  CHECK(seconds < 10.0);
}

} // namespace

MITCAD_PORT_TEST("0008", test_plate_of_two_thousand_holes_is_valid);
MITCAD_PORT_TEST("0008", test_cylinder_side_with_many_holes_is_valid);
MITCAD_PORT_TEST("0008", test_wire_not_closed_in_the_face_is_found);
MITCAD_PORT_TEST("0008", test_plate_of_three_thousand_six_hundred_holes_within_ten_seconds);
