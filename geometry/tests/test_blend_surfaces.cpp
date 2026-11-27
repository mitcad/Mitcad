// SPDX-License-Identifier: MIT
// Rolling-ball blends of imported bodies stored without their approximation
// (mitcad#621): the surface a ball sweeps while its centre runs along a
// spine touching two supports, walked by OCCT and approximated.
//
// Two cases whose exact surfaces are known:
// - between two perpendicular planes, the ball's centre on the line at the
//   radius from both: a quarter of the cylinder about that line;
// - between a plane and a circle (the rim of a hole in a plate above it),
//   the centre on the circle at the radius from both: the torus about the
//   circle's axis, between the plane's contact and the rim.
// Rolled once round a closed spine, the surface is closed (mitcad#622).
// Every point of the surface lies at the radius from the spine, its edges
// u = 0 and u = 1 are the contacts on the first and the second support,
// and the sections are the minor arcs between them.

#include <cmath>
#include <cstdio>

#include <Geom2d_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Line.hxx>
#include <Geom_Plane.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <NCollection_Array1.hxx>
#include <Standard_Failure.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include <cstdint>
#include <string>
#include <vector>

#include "../src/blend_surfaces.hpp"
#include "mitcad/geometry/brep_import.hpp"
#include "check.hpp"
#include "bundled_test.hpp"

namespace {

using mitcad::brep::BlendSupport;

// The largest distance from the spine of the surface's points less the
// radius, over a grid.
double off_the_ball(const Handle(Geom_Surface)& s, const Handle(Geom_Curve)& spine, double radius) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  s->Bounds(u0, u1, v0, v1);
  double worst = 0.0;
  for (int i = 0; i <= 10; ++i) {
    for (int j = 0; j <= 10; ++j) {
      const double v = v0 + (v1 - v0) * j / 10.0;
      const gp_Pnt p = s->Value(u0 + (u1 - u0) * i / 10.0, v);
      // The centre of the section through p: the spine's point at v.
      worst = std::max(worst, std::abs(p.Distance(spine->Value(v)) - radius));
    }
  }
  return worst;
}

void between_two_planes() {
  // The floor z = 0 and the wall x = 0; the ball of radius 2 in the corner
  // between them, its centre on the line x = z = 2 along y.
  BlendSupport floor;
  floor.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  BlendSupport wall;
  wall.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  const Handle(Geom_Curve) spine = new Geom_Line(gp_Pnt(2, 0, 2), gp_Dir(0, 1, 0));
  const Handle(Geom_Surface) s = mitcad::brep::rolling_ball_surface(floor, wall, spine, 2.0, 0.0, 10.0, 0.0, 10.0);
  CHECK(off_the_ball(s, spine, 2.0) < 1e-5);
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  s->Bounds(u0, u1, v0, v1);
  CHECK_NEAR(u0, 0.0);
  CHECK_NEAR(u1, 1.0);
  CHECK_NEAR(v0, 0.0);
  CHECK_NEAR(v1, 10.0);
  // u = 0 on the floor, u = 1 on the wall, v the spine's parameter.
  const gp_Pnt on_floor = s->Value(0.0, 4.0);
  CHECK(on_floor.Distance(gp_Pnt(2, 4, 0)) < 1e-5);
  const gp_Pnt on_wall = s->Value(1.0, 4.0);
  CHECK(on_wall.Distance(gp_Pnt(0, 4, 2)) < 1e-5);
  // The minor arc: its middle towards the corner.
  const gp_Pnt middle = s->Value(0.5, 4.0);
  CHECK(middle.X() < 2.0 && middle.Z() < 2.0);
  CHECK_NEAR_TOL(middle.Distance(gp_Pnt(2, 4, 2)), 2.0, 1e-5);
  // The same with the supports the other way round: u runs from the wall.
  const Handle(Geom_Surface) t = mitcad::brep::rolling_ball_surface(wall, floor, spine, 2.0, 0.0, 10.0, 0.0, 10.0);
  CHECK(t->Value(0.0, 4.0).Distance(gp_Pnt(0, 4, 2)) < 1e-5);
  CHECK(t->Value(1.0, 4.0).Distance(gp_Pnt(2, 4, 0)) < 1e-5);

  // Its contact on the floor as a curve on the surface (u = 0).
  NCollection_Array1<gp_Pnt2d> poles(1, 2);
  poles(1) = gp_Pnt2d(0.0, 1.0);
  poles(2) = gp_Pnt2d(0.0, 9.0);
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 1.0;
  knots(2) = 9.0;
  NCollection_Array1<int> mults(1, 2);
  mults(1) = 2;
  mults(2) = 2;
  const Handle(Geom2d_Curve) pcurve = new Geom2d_BSplineCurve(poles, knots, mults, 1);
  const Handle(Geom_Curve) contact = mitcad::brep::curve_on_surface(s, pcurve);
  CHECK(contact->Value(contact->FirstParameter()).Distance(gp_Pnt(2, 1, 0)) < 1e-5);
  CHECK(contact->Value(contact->LastParameter()).Distance(gp_Pnt(2, 9, 0)) < 1e-5);
  CHECK(contact->Value(5.0).Distance(gp_Pnt(2, 5, 0)) < 1e-5);
}

void between_a_plane_and_a_circle() {
  // The rim of a hole of radius 10 in a plate whose underside is z = 3,
  // the floor z = 0 below: a ball of radius 2 on the floor touching the
  // rim from inside has its centre at z = 2, 10 - sqrt(3) from the axis.
  const double r = 2.0;
  const double rho = 10.0 - std::sqrt(3.0);
  BlendSupport rim;
  rim.curve = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 3), gp_Dir(0, 0, 1)), 10.0);
  BlendSupport floor;
  floor.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  const Handle(Geom_Curve) spine = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), rho);
  const double quarter = test::kPi / 2;
  const Handle(Geom_Surface) s = mitcad::brep::rolling_ball_surface(rim, floor, spine, r, 0.0, quarter, 0.0, quarter);
  CHECK(off_the_ball(s, spine, r) < 1e-5);
  const double a = 0.3;
  // u = 0 on the rim (the first support), u = 1 on the floor.
  const gp_Pnt on_rim = s->Value(0.0, a);
  CHECK(on_rim.Distance(gp_Pnt(10 * std::cos(a), 10 * std::sin(a), 3)) < 1e-5);
  const gp_Pnt on_floor = s->Value(1.0, a);
  CHECK(on_floor.Distance(gp_Pnt(rho * std::cos(a), rho * std::sin(a), 0)) < 1e-5);
  // The minor arc (120 degrees): its middle away from the axis.
  const gp_Pnt middle = s->Value(0.5, a);
  CHECK(std::hypot(middle.X(), middle.Y()) > rho);
  // Supports the other way round.
  const Handle(Geom_Surface) t = mitcad::brep::rolling_ball_surface(floor, rim, spine, r, 0.0, quarter, 0.0, quarter);
  CHECK(t->Value(0.0, a).Distance(on_floor) < 1e-5);
  CHECK(t->Value(1.0, a).Distance(on_rim) < 1e-5);
}

void past_a_quarter_turn_and_over_a_seam() {
  // The floor z = 0 and the inside of a cylinder of radius 10 about z: a
  // ball of radius 2 in the corner has its centre on the circle of radius 8
  // at z = 2. Walked across the cylinder's seam (its angle 0).
  BlendSupport floor;
  floor.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  BlendSupport wall;
  wall.surface = new Geom_CylindricalSurface(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 10.0);
  const Handle(Geom_Curve) round = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), 8.0);
  const Handle(Geom_Surface) s =
      mitcad::brep::rolling_ball_surface(floor, wall, round, 2.0, -0.6, 0.6, -0.5, 0.5);
  CHECK(off_the_ball(s, round, 2.0) < 1e-5);
  CHECK(s->Value(0.0, 0.0).Distance(gp_Pnt(8, 0, 0)) < 1e-5);
  CHECK(s->Value(1.0, 0.0).Distance(gp_Pnt(10, 0, 2)) < 1e-5);
  CHECK(s->Value(0.5, 0.0).X() > 8.0);

  // A floor and a wall leaning over it at 60 degrees: the sections are
  // arcs of 120 degrees, still the minor ones (towards the corner).
  BlendSupport leaning;
  const gp_Dir leaning_normal(std::sin(test::kPi / 3), 0, std::cos(test::kPi / 3));
  leaning.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), leaning_normal));
  // The centre at 2 from both: on the bisector of the 60-degree wedge.
  const double d = 2.0 / std::tan(test::kPi / 6);
  const Handle(Geom_Curve) spine = new Geom_Line(gp_Pnt(-d, 0, 2), gp_Dir(0, 1, 0));
  const Handle(Geom_Surface) t = mitcad::brep::rolling_ball_surface(floor, leaning, spine, 2.0, 0.0, 5.0, 0.0, 5.0);
  CHECK(off_the_ball(t, spine, 2.0) < 1e-5);
  const gp_Pnt middle = t->Value(0.5, 2.0);
  CHECK(middle.X() > -d);
  CHECK_NEAR_TOL(middle.Distance(gp_Pnt(-d, 2, 2)), 2.0, 1e-5);
}

void round_a_whole_closed_spine() {
  // mitcad#622: a ball rolled once round a closed spine (a blend round the
  // whole rim of a hole, a face across the surface's seam) makes a surface
  // closed in v, periodic, its seam where the walk started; also where the
  // ball rolls on a circle, round its whole length (across its seam).
  const double r = 2.0;
  const double rho = 10.0 - std::sqrt(3.0);
  const double turn = 2 * test::kPi;
  BlendSupport rim;
  rim.curve = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 3), gp_Dir(0, 0, 1)), 10.0);
  BlendSupport floor;
  floor.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  const Handle(Geom_Curve) spine = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), rho);
  for (const double start : {0.0, 1.0}) {
    const Handle(Geom_Surface) s =
        mitcad::brep::rolling_ball_surface(rim, floor, spine, r, start, start + turn, start, start + turn);
    CHECK(s->IsVPeriodic());
    CHECK_NEAR_TOL(s->VPeriod(), turn, 1e-9);
    CHECK(off_the_ball(s, spine, r) < 1e-5);
    for (const double a : {start, start + 1.0, start + 3.0, start + 6.0}) {
      CHECK(s->Value(0.0, a).Distance(gp_Pnt(10 * std::cos(a), 10 * std::sin(a), 3)) < 1e-5);
      CHECK(s->Value(1.0, a).Distance(gp_Pnt(rho * std::cos(a), rho * std::sin(a), 0)) < 1e-5);
    }
    // The seam: the same section at both ends.
    for (const double u : {0.0, 0.3, 1.0}) {
      CHECK(s->Value(u, start).Distance(s->Value(u, start + turn)) < 1e-9);
    }
  }
  // Between two surfaces: the floor and the inside of a cylinder.
  BlendSupport wall;
  wall.surface = new Geom_CylindricalSurface(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 10.0);
  const Handle(Geom_Curve) round = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), 8.0);
  const Handle(Geom_Surface) t = mitcad::brep::rolling_ball_surface(floor, wall, round, r, 0.0, turn, 0.0, turn);
  CHECK(t->IsVPeriodic());
  CHECK(off_the_ball(t, round, r) < 1e-5);
  CHECK(t->Value(0.0, 4.0).Distance(gp_Pnt(8 * std::cos(4.0), 8 * std::sin(4.0), 0)) < 1e-5);
  CHECK(t->Value(1.0, 4.0).Distance(gp_Pnt(10 * std::cos(4.0), 10 * std::sin(4.0), 2)) < 1e-5);
  // Less than a whole turn: not closed.
  const Handle(Geom_Surface) part = mitcad::brep::rolling_ball_surface(floor, wall, round, r, 0.0, 3.0, 0.0, 3.0);
  CHECK(!part->IsVPeriodic());
}

void between_a_circle_and_a_torus() {
  // mitcad#639: a ball of radius 5 rolling on a circular edge of radius 13
  // (z = 0) and on the outside of a coaxial torus (tube radius 5 round the
  // circle of radius 10.8 at z = 9.6), its centre on the circle of radius 8
  // in the edge's own plane: the contact on the edge is straight out from
  // the centre, the one on the torus 5 along (2.8, 9.6) / 10 from it. Once
  // round the whole edge (as a rim's blend is), and over part of it; the
  // spine running either way round (whether OCCT took the ball for leaving
  // the edge at the start depended on that and on rounding).
  const double r = 5.0;
  const double rho = 8.0;
  BlendSupport edge;
  edge.curve = new Geom_Circle(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), 13.0);
  BlendSupport torus;
  torus.surface = new Geom_ToroidalSurface(gp_Ax3(gp_Pnt(0, 0, 9.6), gp_Dir(0, 0, 1)), 10.8, 5.0);
  const double turn = 2 * test::kPi;
  for (const double sense : {1.0, -1.0}) {
    const gp_Ax2 frame(gp_Pnt(0, 0, 0), gp_Dir(0, 0, sense), gp_Dir(1, 0, 0));
    const Handle(Geom_Curve) spine = new Geom_Circle(frame, rho);
    // At the spine's parameter a.
    const auto on_edge = [sense](double a) { return gp_Pnt(13 * std::cos(a), 13 * sense * std::sin(a), 0); };
    const auto on_torus = [sense](double a) {
      return gp_Pnt(9.4 * std::cos(a), 9.4 * sense * std::sin(a), 4.8);
    };
    const Handle(Geom_Surface) s =
        mitcad::brep::rolling_ball_surface(edge, torus, spine, r, 0.0, turn, 0.0, turn);
    CHECK(s->IsVPeriodic());
    CHECK(off_the_ball(s, spine, r) < 1e-5);
    for (const double a : {0.0, 1.0, 3.0, 6.0}) {
      // u = 0 on the edge (the first support), u = 1 on the torus.
      CHECK(s->Value(0.0, a).Distance(on_edge(a)) < 1e-5);
      CHECK(s->Value(1.0, a).Distance(on_torus(a)) < 1e-5);
      // The minor arc (74 degrees): its middle above the edge's plane,
      // away from the axis.
      const gp_Pnt middle = s->Value(0.5, a);
      CHECK(middle.Z() > 0.0 && std::hypot(middle.X(), middle.Y()) > rho);
    }
    // The supports the other way round, over part of the edge.
    const Handle(Geom_Surface) t = mitcad::brep::rolling_ball_surface(torus, edge, spine, r, 1.0, 3.0, 1.0, 3.0);
    CHECK(!t->IsVPeriodic());
    CHECK(off_the_ball(t, spine, r) < 1e-5);
    CHECK(t->Value(0.0, 2.0).Distance(on_torus(2.0)) < 1e-5);
    CHECK(t->Value(1.0, 2.0).Distance(on_edge(2.0)) < 1e-5);
  }
}

void a_blend_not_walked_is_its_stored_approximation_and_said() {
  // mitcad#622: a blend made from its definition with the file's
  // approximation (too far off) beside it, for when OCCT cannot walk it:
  // the approximation is then the surface, and the build's report says so
  // for that surface. Surface 0's spine lies off the supports (its walk
  // fails), surface 1's on them.
  mitcad::brep::Body body;
  const auto blend = [&](double spine_x) {
    mitcad::brep::Geometry g;
    g.kind = 9; // rolling ball
    g.int_offset = static_cast<std::uint32_t>(body.ints.size());
    g.real_offset = static_cast<std::uint32_t>(body.reals.size());
    // The two supports (planes), the spine (a line), then the stored
    // approximation (a plane).
    const std::vector<std::int32_t> ints{0, 0, 0, 0, 0, 1, 0};
    const std::vector<double> reals{2.0, 0.0, 10.0, 0.0, 10.0,       // radius, range, needed
                                    0,   0,   0,    0,   0,    1, 1, 0, 0, // the floor z = 0
                                    0,   0,   0,    1,   0,    0, 0, 0, 1, // the wall x = 0
                                    spine_x, 0, 2, 0, 1, 0,              // the spine
                                    0,   0,   0,    0,   0,    1, 1, 0, 0}; // stored
    body.ints.insert(body.ints.end(), ints.begin(), ints.end());
    body.reals.insert(body.reals.end(), reals.begin(), reals.end());
    g.int_count = static_cast<std::uint32_t>(ints.size());
    g.real_count = static_cast<std::uint32_t>(reals.size());
    body.surfaces.push_back(g);
  };
  blend(3.0);
  blend(2.0);
  const mitcad::brep::BuildResult built = mitcad::brep::build_body(body);
  int said = 0;
  for (const std::string& m : built.report.messages) {
    if (m.rfind("stored approximation of surface 0: ", 0) == 0) {
      CHECK(m.find("not the radius") != std::string::npos);
      ++said;
    }
    CHECK(m.rfind("stored approximation of surface 1", 0) != 0);
  }
  CHECK(said == 1);
  CHECK(built.report.surfaces_failed == 0);
}

void a_spine_off_the_supports_is_refused() {
  BlendSupport floor;
  floor.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)));
  BlendSupport wall;
  wall.surface = new Geom_Plane(gp_Ax3(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  const Handle(Geom_Curve) spine = new Geom_Line(gp_Pnt(3, 0, 2), gp_Dir(0, 1, 0));
  CHECK(test::throws_with([&] { mitcad::brep::rolling_ball_surface(floor, wall, spine, 2.0, 0.0, 10.0, 0.0, 10.0); },
                          "not the radius"));
}

} // namespace

MITCAD_BUNDLED_TEST(blend_surfaces) {
  try {
    test::guarded("between_two_planes", between_two_planes);
    test::guarded("between_a_plane_and_a_circle", between_a_plane_and_a_circle);
    test::guarded("past_a_quarter_turn_and_over_a_seam", past_a_quarter_turn_and_over_a_seam);
    test::guarded("round_a_whole_closed_spine", round_a_whole_closed_spine);
    test::guarded("between_a_circle_and_a_torus", between_a_circle_and_a_torus);
    test::guarded("a_spine_off_the_supports_is_refused", a_spine_off_the_supports_is_refused);
    test::guarded("a_blend_not_walked_is_its_stored_approximation_and_said",
                  a_blend_not_walked_is_its_stored_approximation_and_said);
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "blend surfaces threw: %s\n", e.what());
    ++test::failures;
  }
  return test::failures == 0 ? 0 : 1;
}
