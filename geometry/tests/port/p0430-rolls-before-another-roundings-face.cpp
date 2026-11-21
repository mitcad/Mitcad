// SPDX-License-Identifier: MIT
// Port patch 0430: two roundings at the ends of a face too narrow for both,
// one of them beside a strip narrower than its radius. The rounding beside
// the strip overflowed it onto the face beyond (0011), up to its radius
// from its edge, across the other rounding's contact on that face, and
// their ends on the faces at the ends crossed ("fillets have too big
// radiuses"): the fillet failed. With the patch that side may not overflow
// (RollsInsteadOfOverflow): it rolls on the strip's far edge, the ball
// tangent to its other face, and the other rounding keeps its face.
//
// A wall 3.54 thick (x from 0, y from -20 to 10, z from 0 to 15) with a
// window through it (z from 4 to 11, y below 0); the window's end at y = 0
// a strip 0.5 wide at the side x = 0 and a face meeting it at `degrees` up
// to the other side; its two vertical edges at that end rounded with 2.
// Overflowing, the rounding beside the strip would touch the face beyond 2
// from x = 0, the other one touches it about 2 from x = 3.54, so the two
// cross. At 0.35 degrees (where OCCT also takes the strip and the face for
// tangent), -0.35 and -10 degrees: built, valid, the material taken off the
// rolled section beside the strip and the corner's section of the other,
// times the window's height, exactly. With strips on both sides (the face
// between them at -0.35 degrees, the second strip's edge 0.003 off a sliver
// or not) both roll. The boundary: the same window rounded with 1.5, whose
// contacts on the face beyond (1.5 from x = 0 and 2.04) stay apart: the
// fillet is built, valid, as before.

#include <cmath>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHeight = 7.0;  // the window's height
constexpr double kThick  = 3.54; // the wall's thickness

double radians(double degrees) {
  return degrees * test::kPi / 180.0;
}

// The wall with the window: its end a strip 0.5 wide at x = 0, then a face
// at `degrees` (negative: turning away from the wall's material) up to the
// other side, or (both) up to a second strip 0.5 wide at x = kThick whose
// edge lies `sliver` back from the face's end (a sliver of that width).
TopoDS_Shape walled_window(double degrees, bool both, double sliver) {
  const double               t     = std::tan(radians(degrees));
  const TopoDS_Shape         block = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(kThick, 10, 15)).Shape();
  BRepBuilderAPI_MakePolygon outline;
  outline.Add(gp_Pnt(-1, -10, 4));
  outline.Add(gp_Pnt(kThick + 1, -10, 4));
  if (both) {
    const double ym = 2.54 * t;
    outline.Add(gp_Pnt(kThick + 1, ym - sliver, 4));
    outline.Add(gp_Pnt(3.04, ym - sliver, 4));
    outline.Add(gp_Pnt(3.04, ym, 4));
  } else {
    outline.Add(gp_Pnt(kThick + 1, 4.04 * t, 4));
  }
  outline.Add(gp_Pnt(0.5, 0, 4));
  outline.Add(gp_Pnt(-1, 0, 4));
  outline.Close();
  const TopoDS_Shape window =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(0, 0, kHeight)).Shape();
  return BRepAlgoAPI_Cut(block, window).Shape();
}

// The window's vertical edge at (x, y).
TopoDS_Edge window_edge(const TopoDS_Shape& body, double x, double y) {
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (a.Distance(gp_Pnt(x, y, a.Z())) < 1e-9 && b.Distance(gp_Pnt(x, y, b.Z())) < 1e-9 &&
        std::abs(a.Z() - b.Z()) > 1.0) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The volume the rounding of the window's two vertical edges at (0, 0) and
// (kThick, y) with r takes off; -1 where it fails or is invalid.
double removed_by_both(const TopoDS_Shape& body, double y, double r) {
  const TopoDS_Edge a = window_edge(body, 0.0, 0.0);
  const TopoDS_Edge b = window_edge(body, kThick, y);
  CHECK(!a.IsNull() && !b.IsNull());
  if (a.IsNull() || b.IsNull()) {
    return -1.0;
  }
  const TopoDS_Shape rounded = port::rounded(body, {a, b}, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return -1.0;
  }
  return port::volume_of(body) - port::volume_of(rounded);
}

// The section a rounding of radius 2 takes off where it rolls on the far
// edge of the strip 0.5 wide beside its edge: its ball, 2 from the wall,
// passes through that edge; the region between the wall's contact, the
// corner, the edge and the arc.
double rolled_section() {
  const double r = 2.0, w = 0.5;
  const double yc = std::sqrt(r * r - (r - w) * (r - w));
  return 0.5 * (r + w) * yc - 0.5 * r * r * std::acos((r - w) / r);
}

// The section a rounding of radius r takes off a corner whose faces meet
// at alpha inside the material.
double corner_section(double r, double alpha) {
  return r * r * (1.0 / std::tan(0.5 * alpha) - 0.5 * (test::kPi - alpha));
}

// One strip: the rounding beside it rolls on its far edge, the other one
// rounds its corner.
void check_rolls_before_the_other_face(double degrees) {
  const TopoDS_Shape body    = walled_window(degrees, false, 0.0);
  const double       removed = removed_by_both(body, 3.04 * std::tan(radians(degrees)), 2.0);
  if (removed < 0.0) {
    std::fprintf(stderr, "window at %g degrees: no valid rounding\n", degrees);
    return;
  }
  const double alpha = 0.5 * test::kPi + radians(degrees);
  CHECK_NEAR_TOL(removed, kHeight * (rolled_section() + corner_section(2.0, alpha)), 1e-6);
}

// The face beyond the strip turning 0.35 degrees into the material: OCCT
// took the two for tangent and overflowed.
void test_rounding_rolls_before_a_nearly_tangent_face() {
  check_rolls_before_the_other_face(0.35);
}

// The face beyond the strip turning 0.35 and 10 degrees away from the
// material.
void test_rounding_rolls_before_a_face_turning_away() {
  check_rolls_before_the_other_face(-0.35);
  check_rolls_before_the_other_face(-10.0);
}

// Strips on both sides: both roundings roll, also past a sliver of 0.003.
void test_roundings_beside_two_strips_both_roll() {
  for (const double sliver : {0.0, 0.003}) {
    const TopoDS_Shape body    = walled_window(-0.35, true, sliver);
    const double       removed = removed_by_both(body, 2.54 * std::tan(radians(-0.35)) - sliver, 2.0);
    if (removed < 0.0) {
      std::fprintf(stderr, "two strips, sliver %g: no valid rounding\n", sliver);
      continue;
    }
    CHECK_NEAR_TOL(removed, 2.0 * kHeight * rolled_section(), 1e-6);
  }
}

// The boundary: rounded with 1.5, the overflow's contact on the face beyond
// stays short of the other's: built and valid as before, taking off more
// than the corner's section alone on the other side.
void test_roundings_whose_contacts_stay_apart_are_built() {
  const double       degrees = 0.35;
  const TopoDS_Shape body    = walled_window(degrees, false, 0.0);
  const double       removed = removed_by_both(body, 3.04 * std::tan(radians(degrees)), 1.5);
  CHECK(removed > kHeight * corner_section(1.5, 0.5 * test::kPi + radians(degrees)));
}

} // namespace

MITCAD_PORT_TEST("0430", test_rounding_rolls_before_a_nearly_tangent_face);
MITCAD_PORT_TEST("0430", test_rounding_rolls_before_a_face_turning_away);
MITCAD_PORT_TEST("0430", test_roundings_beside_two_strips_both_roll);
MITCAD_PORT_TEST("0430", test_roundings_whose_contacts_stay_apart_are_built);
