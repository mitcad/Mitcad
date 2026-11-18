// SPDX-License-Identifier: MIT
// Port patch 0172: a blend rolling on an edge whose contact on its face
// leaves that face before the end of the spine is walked on, still rolling
// on the edge, until the ball reaches the face at the end, and cut there;
// the stripe ends in a point on the edge, with no corner at the end vertex.
// Before, the corner at the end intersected an extension of the blend with
// the face at the end, and the result was invalid.
//
// A post of radius 10 with a round pocket of radius 6.5 centred at
// (0, 15), cut down to z = 6: the pocket's floor is a lens between its
// wall and the post's rim, at most 1.5 wide, with tips where the wall
// meets the post's side. The floor's edge at the wall rounded with 2.5 and
// with 2: the ball touches the wall and rolls on the rim; near each tip its
// contact on the wall leaves the wall across the vertical edge where wall
// and post meet, and the blend is cut by the post's side. Built, valid, the
// lens consumed, no vertex left at either tip (the post's side covers the
// corner the blend fills there), the volume added between nothing and the
// full rounding's. The boundary: the pocket within the post (centred at
// (0, 0.5)), its floor's edge a closed circle and the floor wide enough
// for the ball all round: the torus of the rounding, exact volume, as
// stock. (geometry/tests/test_fillet_rolling_ball.cpp checks a post of
// radius 8 against the rolling ball's centre curve.)

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Vertex.hxx>

#include "port_test.hpp"

namespace {

constexpr double kPost = 10.0, kPocket = 6.5, kFloor = 6.0;

// The post with the pocket centred at (0, y) cut into it down to the floor.
TopoDS_Shape post_with_pocket(double y) {
  const TopoDS_Shape post   = BRepPrimAPI_MakeCylinder(gp_Ax2(gp::Origin(), gp::DZ()), kPost, 30.0).Shape();
  const TopoDS_Shape pocket = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, y, kFloor), gp::DZ()), kPocket, 30.0).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Cut(post, pocket).Shape(), true, true, false);
  unify.Build();
  return unify.Shape();
}

// The floor's edge at the pocket's wall rounded with r; null when it fails.
TopoDS_Shape floor_edge_rounded(const TopoDS_Shape& body, double y, double r) {
  const std::vector<TopoDS_Edge> edge = port::edges_where(body, [y](const gp_Pnt& p) {
    return std::abs(p.Z() - kFloor) < 1e-6 && std::abs(std::hypot(p.X(), p.Y() - y) - kPocket) < 1e-6;
  });
  CHECK(edge.size() == 1u);
  return port::rounded(body, edge, r);
}

int floor_faces_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() != GeomAbs_Plane) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    if (std::abs(props.CentreOfMass().Z() - kFloor) < 1e-6) {
      ++count;
    }
  }
  return count;
}

bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < 1e-3) {
      return true;
    }
  }
  return false;
}

// The lens's floor rounded at the wall with 2.5 and 2: the ball rolls on
// the rim and the blend is cut by the post's side at each tip.
void test_blend_rolling_on_the_rim_is_cut_at_the_lens_tips() {
  const double       y    = 15.0;
  const TopoDS_Shape body = post_with_pocket(y);
  CHECK(port::valid(body));
  CHECK(floor_faces_of(body) == 1);
  // The tips: where the pocket's circle meets the post's.
  const double tipY = (kPost * kPost - kPocket * kPocket + y * y) / (2.0 * y);
  const double tipX = std::sqrt(kPost * kPost - tipY * tipY);
  CHECK(has_vertex_near(body, gp_Pnt(tipX, tipY, kFloor)));
  CHECK(has_vertex_near(body, gp_Pnt(-tipX, tipY, kFloor)));
  // The wall's arc between the tips.
  const double arc = 2.0 * kPocket * std::atan2(tipX, y - tipY);
  for (const double r : {2.5, 2.0}) {
    const TopoDS_Shape result = floor_edge_rounded(body, y, r);
    CHECK(port::valid(result));
    if (!port::valid(result)) {
      std::fprintf(stderr, "radius %g\n", r);
      continue;
    }
    CHECK(floor_faces_of(result) == 0);
    CHECK(!has_vertex_near(result, gp_Pnt(tipX, tipY, kFloor)));
    CHECK(!has_vertex_near(result, gp_Pnt(-tipX, tipY, kFloor)));
    const double added = port::volume_of(result) - port::volume_of(body);
    CHECK(added > 0.0);
    CHECK(added < test::fillet_loss(r) * arc);
  }
}

// The boundary: the pocket within the post, the floor's edge a closed
// circle, the floor at least 3 wide: the ball touches the floor all round
// and the rounding is the torus, filling the corner's section r^2 (1 -
// pi / 4) about the pocket's axis, its centroid (10 - 3 pi) / (12 - 3 pi) r
// in from the wall.
void test_rounding_of_a_pocket_within_the_post_is_as_stock() {
  const double       y    = 0.5;
  const double       r    = 2.5;
  const TopoDS_Shape body = post_with_pocket(y);
  CHECK(port::valid(body));
  const TopoDS_Shape result = floor_edge_rounded(body, y, r);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  const double centroid = kPocket - (10.0 - 3.0 * test::kPi) / (12.0 - 3.0 * test::kPi) * r;
  CHECK_NEAR_TOL(port::volume_of(result) - port::volume_of(body),
                 2.0 * test::kPi * centroid * test::fillet_loss(r), 1e-6);
  CHECK(port::count_of(result, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 1);
  CHECK(floor_faces_of(result) == 1);
}

} // namespace

MITCAD_PORT_TEST("0172", test_blend_rolling_on_the_rim_is_cut_at_the_lens_tips);
MITCAD_PORT_TEST("0172", test_rounding_of_a_pocket_within_the_post_is_as_stock);
