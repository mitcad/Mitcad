// SPDX-License-Identifier: MIT
// Port patch 0121: BRepCheck_Shell's orientation check lists the edges of a
// face of 64 edges or more once instead of searching the face for each of
// its edges. The statuses stay as they were; a plate of many holes is
// checked in time linear in its edges.

#include <chrono>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Shell.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

// A plate of n x n circular holes of radius 0.3 at a pitch of 1, 0.5 thick:
// its top and bottom faces have 4 + n * n edges each.
TopoDS_Shape holed_plate(int n) {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(n, 0, 0), gp_Pnt(n, n, 0), gp_Pnt(0, n, 0), true);
  BRepBuilderAPI_MakeFace    face(gp_Pln(), outline.Wire());
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      // Clockwise seen from +z: a hole of the face.
      const gp_Circ circle(gp_Ax2(gp_Pnt(i + 0.5, j + 0.5, 0), gp_Dir(0, 0, -1)), 0.3);
      face.Add(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle).Edge()).Wire());
    }
  }
  return BRepPrimAPI_MakePrism(face.Face(), gp_Vec(0, 0, 0.5)).Shape();
}

TopoDS_Shell first_shell(const TopoDS_Shape& shape) {
  TopExp_Explorer shells(shape, TopAbs_SHELL);
  return TopoDS::Shell(shells.Current());
}

BRepCheck_Status orientation_of(const TopoDS_Shell& shell) {
  BRepCheck_Shell check(shell);
  check.Closed();
  return check.Orientation();
}

// 81 holes put the top and bottom faces past the 64 edges from which the
// patch lists a face's edges; the shell's orientation is right.
void test_shell_of_a_plate_with_many_holes_is_oriented() {
  const TopoDS_Shape plate = holed_plate(9); // 81 holes, 85 edges per cap
  CHECK(port::valid(plate));
  CHECK(orientation_of(first_shell(plate)) == BRepCheck_NoError);
}

// The same shell with its top face reversed: the listed orientations still
// find the bad orientation (the cap and every hole's wall disagree at their
// common edges).
void test_reversed_face_of_many_edges_is_found() {
  const TopoDS_Shell shell = first_shell(holed_plate(9));
  TopoDS_Shell       broken;
  BRep_Builder       builder;
  builder.MakeShell(broken);
  bool reversed = false;
  for (TopExp_Explorer faces(shell, TopAbs_FACE); faces.More(); faces.Next()) {
    TopoDS_Shape face = faces.Current();
    if (!reversed && port::count_of(face, TopAbs_EDGE) > 64) {
      face.Reverse();
      reversed = true;
    }
    builder.Add(broken, face);
  }
  CHECK(reversed);
  CHECK(orientation_of(broken) == BRepCheck_BadOrientationOfSubshape);
}

// Below the threshold (49 holes, 53 edges per cap) the faces are searched
// as before, with the same status.
void test_shell_below_the_listing_threshold_is_oriented() {
  const TopoDS_Shape plate = holed_plate(7);
  CHECK(orientation_of(first_shell(plate)) == BRepCheck_NoError);
}

// The bound: 141 x 141 holes (19 885 edges per cap). Searching each cap for
// each of its edges grows with the square of its edges (9.6 s for 23 428),
// about 7 s here; the listed orientations take a small part of a second,
// so 2 s leaves room for a loaded machine.
void test_orientation_of_twenty_thousand_holes_within_two_seconds() {
  const TopoDS_Shell shell = first_shell(holed_plate(141));
  const auto         start = std::chrono::steady_clock::now();
  CHECK(orientation_of(shell) == BRepCheck_NoError);
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::printf("orientation of 19 881 holes: %.2f s\n", seconds);
  CHECK(seconds < 2.0);
}

} // namespace

MITCAD_PORT_TEST("0121", test_shell_of_a_plate_with_many_holes_is_oriented);
MITCAD_PORT_TEST("0121", test_reversed_face_of_many_edges_is_found);
MITCAD_PORT_TEST("0121", test_shell_below_the_listing_threshold_is_oriented);
MITCAD_PORT_TEST("0121", test_orientation_of_twenty_thousand_holes_within_two_seconds);
