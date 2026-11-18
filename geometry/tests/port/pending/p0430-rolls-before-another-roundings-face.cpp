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
// A second geometry to geometry.fillet_corners' window through a wall 3.54
// thick rounded with 2: a wall T = 5 thick (x from 0 to 5) with a window
// through it (z from 4 to 11, y below 0), the window's end at y = 0 a strip
// w = 0.8 wide at the side x = 0 and a face meeting it at `degrees` up to
// the other side; its two vertical edges at that end rounded with r = 3.
// Overflowing, the rounding beside the strip would touch the face beyond 3
// from the wall x = 0, the other one touches it about 3 from x = 5, so the
// two cross (T < 2 r). At 0.35 degrees (where OCCT also takes the strip and
// the face for tangent) and at -10 degrees: built, valid, the material
// taken off the rolled section beside the strip and the corner's section of
// the other, times the window's height, exactly. The boundary: the window
// of geometry.fillet_corners (T = 3.54, strip 0.5) rounded with 1.5, whose
// contacts on the face beyond (1.5 from x = 0 and 2.04) stay apart: the
// rounding beside the strip overflows onto that face as 0011 makes it.

#include <cmath>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHeight = 7.0; // the window's height

double radians(double degrees) {
  return degrees * test::kPi / 180.0;
}

// A wall `thick` thick (x from 0, y from -20 to 10, z from 0 to 15) with
// the window; its end a strip `strip` wide at x = 0, then a face meeting
// it at `degrees` (negative: turning away from the wall's material).
TopoDS_Shape walled_window(double thick, double strip, double degrees) {
  const double               t     = std::tan(radians(degrees));
  const TopoDS_Shape         block = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, 0), gp_Pnt(thick, 10, 15)).Shape();
  BRepBuilderAPI_MakePolygon outline;
  outline.Add(gp_Pnt(-1, -10, 4));
  outline.Add(gp_Pnt(thick + 1, -10, 4));
  outline.Add(gp_Pnt(thick + 1, (thick + 1 - strip) * t, 4));
  outline.Add(gp_Pnt(strip, 0, 4));
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

// The window's two vertical edges at its end rounded with r; a null shape
// when it fails.
TopoDS_Shape both_rounded(const TopoDS_Shape& body, double thick, double strip, double degrees, double r) {
  const TopoDS_Edge narrow = window_edge(body, 0.0, 0.0);
  const TopoDS_Edge wide   = window_edge(body, thick, (thick - strip) * std::tan(radians(degrees)));
  CHECK(!narrow.IsNull() && !wide.IsNull());
  if (narrow.IsNull() || wide.IsNull()) {
    return TopoDS_Shape();
  }
  return port::rounded(body, {narrow, wide}, r);
}

// The section a rounding of radius r takes off where it rolls on the far
// edge of the strip w wide beside its edge: its ball, r from the wall,
// passes through that edge; the region between the wall's contact, the
// corner, the edge and the arc.
double rolled_section(double r, double w) {
  const double h = std::sqrt(r * r - (r - w) * (r - w));
  return 0.5 * (r + w) * h - 0.5 * r * r * std::acos((r - w) / r);
}

// The section a rounding of radius r takes off a corner whose faces meet
// at alpha inside the material.
double corner_section(double r, double alpha) {
  return r * r * (1.0 / std::tan(0.5 * alpha) - 0.5 * (test::kPi - alpha));
}

// The section a rounding of radius r takes off at the strip's side when it
// overflows the strip (w wide) onto the face beyond, at `degrees` to it
// (positive: turning into the material): the ball touches the wall x = 0
// and that face. The polygon of the corner, the strip's far edge, the
// contact on the face, the ball's centre and the wall's contact less the
// ball's sector between the two contacts.
double overflow_section(double r, double w, double degrees) {
  const double th = radians(degrees);
  const double yc = (r + (r - w) * std::sin(th)) / std::cos(th); // the centre (r, yc)
  const double tx = r + r * std::sin(th), ty = yc - r * std::cos(th);
  const double px[5] = {0.0, w, tx, r, 0.0};
  const double py[5] = {0.0, 0.0, ty, yc, yc};
  double       twice = 0.0;
  for (int i = 0; i < 5; ++i) {
    twice += px[i] * py[(i + 1) % 5] - px[(i + 1) % 5] * py[i];
  }
  return 0.5 * twice - 0.5 * r * r * (0.5 * test::kPi + th);
}

void check_rolls_before_the_other_face(double degrees) {
  const double       thick = 5.0, strip = 0.8, r = 3.0;
  const TopoDS_Shape body    = walled_window(thick, strip, degrees);
  const TopoDS_Shape rounded = both_rounded(body, thick, strip, degrees, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    std::fprintf(stderr, "window at %g degrees: no valid rounding\n", degrees);
    return;
  }
  // The other side's corner: the solid's angle there is 90 degrees plus the
  // turn.
  const double alpha = 0.5 * test::kPi + radians(degrees);
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded),
                 kHeight * (rolled_section(r, strip) + corner_section(r, alpha)), 1e-6);
}

// The face beyond the strip turning 0.35 degrees into the material: OCCT
// took the two for tangent and overflowed.
void test_rounding_rolls_before_a_nearly_tangent_face() {
  check_rolls_before_the_other_face(0.35);
}

// The face beyond the strip turning 10 degrees away from the material.
void test_rounding_rolls_before_a_face_turning_away() {
  check_rolls_before_the_other_face(-10.0);
}

// The boundary: the window 3.54 thick, the strip 0.5 wide, the radius 1.5:
// the overflow's contact on the face beyond (1.51 from x = 0) stays short
// of the other's (2.05), and that side overflows as before.
void test_rounding_overflows_short_of_the_other_contact() {
  const double       thick = 3.54, strip = 0.5, r = 1.5, degrees = 0.35;
  const TopoDS_Shape body    = walled_window(thick, strip, degrees);
  const TopoDS_Shape rounded = both_rounded(body, thick, strip, degrees, r);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  const double alpha = 0.5 * test::kPi + radians(degrees);
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded),
                 kHeight * (overflow_section(r, strip, degrees) + corner_section(r, alpha)), 1e-6);
}

} // namespace

MITCAD_PORT_TEST("0430", test_rounding_rolls_before_a_nearly_tangent_face);
MITCAD_PORT_TEST("0430", test_rounding_rolls_before_a_face_turning_away);
MITCAD_PORT_TEST("0430", test_rounding_overflows_short_of_the_other_contact);
