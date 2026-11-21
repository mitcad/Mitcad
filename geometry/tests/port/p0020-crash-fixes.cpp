// SPDX-License-Identifier: MIT
// Port patch 0020, two of its three changes.
//
// 1. Where a rounding or chamfer ends on a face, the corner prolongs the edge of the face at the end on the blend's opposite face
// (PerformOneCorner) and splits that prolongation where it crosses the
// opposite face's seam, found with BRepExtrema_ExtCC. When the prolongation
// runs along the seam, the extrema are parallel: one distance and no
// points, and reading the points of the first extremum (its distance 0)
// went past the end of their sequence. OCCT's release build checks no
// range there, so the fillet crashed. The patch skips the seam split for
// parallel curves.
//
// The body: a block with a bore of radius 17 whose seam is at 0 degrees,
// and inside it a lug, a ring sector between radii 12.5 and 17 over 135
// degrees, 5 high, joined to the bore's wall. The concave arc where the
// lug's top meets the wall above it is chamfered or rounded with 2. At
// the lug's ends the radial end faces are the faces at the end, and their
// prolongation on the wall is a vertical line; where an end lies on the
// bore's seam, that line is the seam itself. Without the patch the
// process dies with a segmentation fault for each lug with an end on the
// seam; with it the result is valid and adds the blend's section turned
// through 135 degrees (Pappus). The lug with both ends away from the seam
// (the prolongations parallel to it at a distance, no point read) builds
// the same with and without the patch.
//
// 2. PerformMoreSurfdata, the corner at a stripe's end whose last two
// surface data both meet the face at the end, read the face beside the
// stripe's surface data from the builder's first stripe instead of the
// stripe at the vertex. With several contours that is another stripe: its
// surface data at that index is another blend's (a wrong face, the result
// invalid), or it has fewer of them (a read past the end of its sequence,
// a segmentation fault). The patch reads the stripe at the vertex.
//
// The body: a pipe (radius 25, axis z) with a rod (radius 5) standing
// beside it, 28 from its axis, the pipe's side split at z = 13 and the
// rod's at z = 5.5; the bottom cut by a plane rising by 0.5 per unit of y
// (from z = 5 at y = 5), the top flat at z = 20. The concave junction at
// y > 0 rounded with 3 ends at the inclined bottom over two surface data
// that both meet it (test_fillet_overflow's rod and pipe). Added after the
// junction at y < 0, which is rounded too, the corner read that stripe's
// surface data: the result is invalid without the patch; with it, valid,
// and both junctions take the section between the rod, the ball and the
// pipe between the end planes. With the inclined end at the top (z = 15
// at y = 5, falling by 0.5 per unit of y, the pipe split at 7 and the rod
// at 14.5) and a box beside the body whose edge is rounded first (one
// surface data), the corner read past that stripe's sequence: a
// segmentation fault without the patch; with it, valid, with the junction's
// and the box edge's material.
//
// The patch's third change (StartSol's obstacle without the arc) has no
// test: no synthetic body has reached it (see the patch header).

#include <cmath>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kBore  = 17.0;
constexpr double kInner = 12.5;
constexpr double kSweep = 0.75 * test::kPi; // the lug's 135 degrees
constexpr double kSize  = 2.0;              // the chamfer's distance, the rounding's radius

// The block with the bore (its seam at 0 degrees) and the lug from `start`
// degrees.
TopoDS_Shape lug_in_a_bore(double start) {
  const gp_Dir       up(0, 0, 1);
  const double       a0 = start * test::kPi / 180.0;
  const gp_Dir       from(std::cos(a0), std::sin(a0), 0);
  const TopoDS_Shape block  = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, -16), gp_Pnt(30, 30, 18)).Shape();
  const TopoDS_Shape bore   = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -17), up, gp::DX()), kBore, 36.0).Shape();
  const TopoDS_Shape sector = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -6), up, from), kBore, 5.0, kSweep).Shape();
  const TopoDS_Shape hole   = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -7), up, from), kInner, 7.0).Shape();
  const TopoDS_Shape lug    = BRepAlgoAPI_Cut(sector, hole).Shape();
  return BRepAlgoAPI_Fuse(BRepAlgoAPI_Cut(block, bore).Shape(), lug).Shape();
}

// The arc where the lug's top (z = -1) meets the bore's wall.
std::vector<TopoDS_Edge> top_arc(const TopoDS_Shape& body) {
  return port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() + 1.0) < 1e-6 && std::abs(std::hypot(p.X(), p.Y()) - kBore) < 1e-6;
  });
}

void check_blend(double start, bool chamfer) {
  const TopoDS_Shape body = lug_in_a_bore(start);
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> arc = top_arc(body);
  CHECK(arc.size() == 1u);
  if (arc.size() != 1u) {
    return;
  }
  const TopoDS_Shape result = chamfer ? port::chamfered(body, arc, kSize) : port::rounded(body, arc, kSize);
  CHECK(port::valid(result));
  if (result.IsNull()) {
    return;
  }
  // The material the blend adds: its section turned through the lug's
  // sweep about the axis. The chamfer's triangle has its centroid a third
  // of its leg inside the wall; the rounding's square less the quarter disc
  // has it 0.2234 of the radius inside.
  const double r     = kSize;
  const double area  = chamfer ? 0.5 * r * r : r * r * (1.0 - test::kPi / 4.0);
  const double inset = chamfer ? r / 3.0 : r * (5.0 / 6.0 - test::kPi / 4.0) / (1.0 - test::kPi / 4.0);
  CHECK_NEAR(port::volume_of(result) - port::volume_of(body), kSweep * (kBore - inset) * area);
}

// The lug's end (225 + 135 = 360 degrees) on the bore's seam.
void test_chamfer_of_a_lug_ending_on_the_bores_seam() { check_blend(225.0, true); }
void test_rounding_of_a_lug_ending_on_the_bores_seam() { check_blend(225.0, false); }

// The lug's start on the bore's seam.
void test_chamfer_of_a_lug_starting_on_the_bores_seam() { check_blend(0.0, true); }
void test_rounding_of_a_lug_starting_on_the_bores_seam() { check_blend(0.0, false); }

// Both ends away from the seam (100 and 235 degrees): the prolongations run
// parallel to the seam at a distance.
void test_chamfer_of_a_lug_away_from_the_bores_seam() { check_blend(100.0, true); }
void test_rounding_of_a_lug_away_from_the_bores_seam() { check_blend(100.0, false); }

constexpr double kPipe = 25.0, kRod = 5.0, kApart = 28.0, kBall = 3.0, kFlat = 20.0;

// A cylinder on the z axis through `base`, 40 high, its side split at `split`.
TopoDS_Shape stacked(const gp_Pnt& base, double radius, double split) {
  const gp_Dir up(0, 0, 1);
  return BRepAlgoAPI_Fuse(
           BRepPrimAPI_MakeCylinder(gp_Ax2(base, up), radius, split).Shape(),
           BRepPrimAPI_MakeCylinder(gp_Ax2(base.Translated(gp_Vec(0, 0, split)), up), radius, 40.0 - split).Shape())
    .Shape();
}

// The plane's height at y: z0 + slope (y - 5).
struct Incline {
  bool   bottom;
  double z0, slope;
};

Incline incline(bool bottom) { return bottom ? Incline{true, 5.0, 0.5} : Incline{false, 15.0, -0.5}; }

// The pipe with the rod, one end cut by the inclined plane, the other flat.
TopoDS_Shape pipe_with_a_rod(const Incline& end) {
  const double       pipe_split = end.bottom ? 13.0 : 7.0, rod_split = end.bottom ? 5.5 : 14.5;
  const TopoDS_Shape both       = BRepAlgoAPI_Fuse(stacked(gp_Pnt(0, 0, 0), kPipe, pipe_split),
                                             stacked(gp_Pnt(-kApart, 0, 0), kRod, rod_split))
                              .Shape();
  gp_Trsf turn, lift;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 5, 0), gp::DX()), std::atan(end.slope));
  lift.SetTranslation(gp_Vec(0, 0, end.z0));
  const TopoDS_Shape beyond =
    BRepBuilderAPI_Transform(
      BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, end.bottom ? -100 : 0), 200, 200, 100).Shape(), lift * turn)
      .Shape();
  TopoDS_Shape body = BRepAlgoAPI_Cut(both, beyond).Shape();
  if (end.bottom) {
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, kFlat), 200, 200, 100).Shape()).Shape();
  }
  return body;
}

// The junction's edges on the side y > 0 (`plus`) or y < 0.
std::vector<TopoDS_Edge> junction(const TopoDS_Shape& body, bool plus) {
  return port::edges_where(body, [plus](const gp_Pnt& p) {
    return std::abs(std::hypot(p.X(), p.Y()) - kPipe) < 1e-6 && std::abs(std::hypot(p.X() + kApart, p.Y()) - kRod) < 1e-6
           && (p.Y() > 0.0) == plus;
  });
}

// The rounding's section at y > 0 (between the rod, the ball's arc and the
// pipe): its area and its first moment in y, by Green's theorem round its
// three arcs.
void junction_section(double& area, double& moment) {
  const double R = kPipe, r = kRod, d = kApart, ball = kBall;
  const double jx = -(d * d + R * R - r * r) / (2 * d), jy = std::sqrt(R * R - jx * jx);
  const double cx  = -(d * d + (R + ball) * (R + ball) - (r + ball) * (r + ball)) / (2 * d);
  const double cy  = std::sqrt((R + ball) * (R + ball) - cx * cx);
  const double p1x = -d + r * (cx + d) / (r + ball), p1y = r * cy / (r + ball);
  const double p2x = R * cx / (R + ball), p2y = R * cy / (R + ball);
  area = moment = 0.0;
  const auto arc = [&](double a, double b, double rho, double x0, double y0, double x1, double y1) {
    const double t0 = std::atan2(y0 - b, x0 - a);
    double       t1 = std::atan2(y1 - b, x1 - a);
    if (t1 - t0 > test::kPi) {
      t1 -= 2 * test::kPi;
    } else if (t0 - t1 > test::kPi) {
      t1 += 2 * test::kPi;
    }
    area += 0.5 * (rho * rho * (t1 - t0) + rho * (a * (std::sin(t1) - std::sin(t0)) - b * (std::cos(t1) - std::cos(t0))));
    const auto m = [&](double t) {
      const double c = std::cos(t);
      return b * b * -c + 2 * b * rho * (t / 2 - std::sin(2 * t) / 4) + rho * rho * (-c + c * c * c / 3);
    };
    moment += 0.5 * rho * (m(t1) - m(t0));
  };
  arc(-d, 0, r, jx, jy, p1x, p1y);
  arc(cx, cy, ball, p1x, p1y, p2x, p2y);
  arc(0, 0, R, p2x, p2y, jx, jy);
  if (area < 0) {
    area   = -area;
    moment = -moment;
  }
}

// The material the junction's rounding at y > 0 adds: its section times
// the height between the end planes over it (the inclined plane's height
// z0 + slope (y - 5)).
double junction_fill(const Incline& end) {
  double area = 0.0, moment = 0.0;
  junction_section(area, moment);
  return end.bottom ? (kFlat - end.z0 + 5 * end.slope) * area - end.slope * moment
                    : (end.z0 - 5 * end.slope) * area + end.slope * moment;
}

// The junctions' edges on both sides rounded, in the order given; a null
// shape when the fillet fails.
TopoDS_Shape junctions_rounded(const TopoDS_Shape& body, const std::vector<bool>& sides) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const bool plus : sides) {
    for (const TopoDS_Edge& edge : junction(body, plus)) {
      fillet.Add(kBall, edge);
    }
  }
  return port::built(fillet, "fillet");
}

// Both junctions rounded, the one at y < 0 added first: the corner of the
// one at y > 0 at the inclined bottom is not on the builder's first stripe.
// The two roundings lie apart, so together they add what each adds alone;
// the one at y > 0 adds its section between the end planes.
void test_two_junction_roundings_ending_on_an_inclined_bottom() {
  const Incline      end  = incline(true);
  const TopoDS_Shape body = pipe_with_a_rod(end);
  CHECK(port::valid(body));
  CHECK(!junction(body, false).empty() && !junction(body, true).empty());
  const TopoDS_Shape result = junctions_rounded(body, {false, true});
  CHECK(port::valid(result));
  const TopoDS_Shape minus = junctions_rounded(body, {false});
  const TopoDS_Shape plus  = junctions_rounded(body, {true});
  CHECK(port::valid(minus));
  CHECK(port::valid(plus));
  if (result.IsNull() || minus.IsNull() || plus.IsNull()) {
    return;
  }
  const double v = port::volume_of(body);
  CHECK_NEAR_TOL(port::volume_of(plus), v + junction_fill(end), 1e-9);
  CHECK_NEAR_TOL(port::volume_of(result), port::volume_of(minus) + port::volume_of(plus) - v, 1e-9);
}

// The junction at y > 0 rounded, ending at the inclined top, beside a box
// one of whose edges is rounded first: the builder's first stripe has one
// surface data, the junction's three.
void test_junction_rounding_ending_on_an_inclined_top_beside_a_box() {
  const Incline      end  = incline(false);
  const TopoDS_Shape body = pipe_with_a_rod(end);
  const TopoDS_Shape box  = BRepPrimAPI_MakeBox(gp_Pnt(100, 0, 0), 10, 10, 10).Shape();
  TopoDS_Compound    both;
  BRep_Builder       builder;
  builder.MakeCompound(both);
  builder.Add(both, body);
  builder.Add(both, box);
  // The box's edge along x at y = 0, z = 0.
  const std::vector<TopoDS_Edge> box_edge = port::edges_where(
    box, [](const gp_Pnt& p) { return std::abs(p.X() - 105.0) < 1e-9 && std::abs(p.Y()) < 1e-9 && std::abs(p.Z()) < 1e-9; });
  const std::vector<TopoDS_Edge> plus = junction(body, true);
  CHECK(box_edge.size() == 1u && !plus.empty());
  if (box_edge.size() != 1u) {
    return;
  }
  BRepFilletAPI_MakeFillet fillet(both);
  fillet.Add(1.0, box_edge.front());
  for (const TopoDS_Edge& edge : plus) {
    fillet.Add(kBall, edge);
  }
  const TopoDS_Shape result = port::built(fillet, "fillet");
  CHECK(port::valid(result));
  if (result.IsNull()) {
    return;
  }
  // The box's edge loses (1 - pi / 4) per unit of its length 10.
  const double expected = port::volume_of(body) + junction_fill(end) + 1000.0 - 10.0 * (1.0 - test::kPi / 4.0);
  CHECK_NEAR_TOL(port::volume_of(result), expected, 1e-9);
}

} // namespace

MITCAD_PORT_TEST("0020", test_chamfer_of_a_lug_ending_on_the_bores_seam);
MITCAD_PORT_TEST("0020", test_rounding_of_a_lug_ending_on_the_bores_seam);
MITCAD_PORT_TEST("0020", test_chamfer_of_a_lug_starting_on_the_bores_seam);
MITCAD_PORT_TEST("0020", test_rounding_of_a_lug_starting_on_the_bores_seam);
MITCAD_PORT_TEST("0020", test_chamfer_of_a_lug_away_from_the_bores_seam);
MITCAD_PORT_TEST("0020", test_rounding_of_a_lug_away_from_the_bores_seam);
MITCAD_PORT_TEST("0020", test_two_junction_roundings_ending_on_an_inclined_bottom);
MITCAD_PORT_TEST("0020", test_junction_rounding_ending_on_an_inclined_top_beside_a_box);
