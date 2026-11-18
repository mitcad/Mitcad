// SPDX-License-Identifier: MIT
// Port patch 0143: the corner of two chamfers whose contacts on their
// common face leave that face across arcs into one face beyond before they
// cross. Each chamfer gets a piece cut by the face beyond, up to where the
// chamfers' intersection meets it, and the corner is computed between those
// pieces on the face beyond. Before, the corner by intersection found no
// crossing on the common face and the chamfer failed.
//
// A square prism (half-width 4, z 0..4) under a disc of radius 7 (z 4..6);
// the four edges where the prism's walls meet the disc's underside are
// chamfered 2 (equal distances). At each corner the two chamfers' contacts
// on the underside would cross 6 sqrt 2 = 8.49 from the axis, beyond the
// rim: the disc's cylinder, prolonged down, cuts the chamfers there, and
// their intersection runs from the prism's vertical edge up to that
// cylinder, which it meets at z = 2 + (7 / sqrt 2 - 4). The cylinder's
// seam away from the corners, or on one corner's diagonal (the corner's
// point on the seam's prolongation). Exact volume: at the height where the
// chamfers stand s out from the walls they fill the square of half-width
// 4 + s within the disc's circle, less the prism's. The boundary: chamfers
// of 0.8, whose contacts cross on the underside (4.8 sqrt 2 = 6.79 from the
// axis), as stock. (geometry/tests/test_chamfer_overflow.cpp checks a
// hexagonal prism with unequal distances.)

#include <algorithm>
#include <cmath>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <TopoDS_Vertex.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHalf = 4.0; // the prism's half-width
constexpr double kDisc = 7.0; // the disc's radius

// The prism under the disc, the disc's seam along `seam`.
TopoDS_Shape prism_under_disc(const gp_Dir& seam) {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(-kHalf, -kHalf, 0), gp_Pnt(kHalf, -kHalf, 0), gp_Pnt(kHalf, kHalf, 0),
                                     gp_Pnt(-kHalf, kHalf, 0), true);
  const TopoDS_Shape prism =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire()).Face(), gp_Vec(0, 0, 4)).Shape();
  const TopoDS_Shape disc = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 4), gp_Dir(0, 0, 1), seam), kDisc, 2).Shape();
  BRepAlgoAPI_Fuse fuse(prism, disc);
  fuse.SimplifyResult();
  return fuse.Shape();
}

// The area of the regular n-gon of inradius b within the circle of radius
// r about its centre: 2n times the triangle up to where its half-side
// leaves the circle and the sector beyond.
double polygon_in_disc(int n, double b, double r) {
  const double half = test::kPi / n;
  const double t    = std::acos(std::min(1.0, b / r));
  if (t >= half) {
    return n * b * b * std::tan(half);
  }
  return 2.0 * n * (0.5 * b * b * std::tan(t) + 0.5 * r * r * (half - t));
}

// Simpson's rule of f over [a, b] (f smooth there).
template <typename F>
double integral(const F& f, double a, double b) {
  const int n   = 2000;
  double    sum = 0.0;
  for (int i = 0; i <= n; ++i) {
    const double w = (i == 0 || i == n) ? 1.0 : (i % 2 == 1 ? 4.0 : 2.0);
    sum += w * f(a + (b - a) * i / n);
  }
  return sum * (b - a) / n / 3.0;
}

// The volume the chamfers of distance d add: the cross-section at s
// (0..d) out from the walls, integrated in two pieces at the kink where
// the square's corners reach the circle.
double added_by(double d) {
  const auto section = [](double s) { return polygon_in_disc(4, kHalf + s, kDisc) - 4.0 * kHalf * kHalf; };
  const double kink = kDisc / std::sqrt(2.0) - kHalf;
  if (kink >= d) {
    return integral(section, 0.0, d);
  }
  return integral(section, 0.0, kink) + integral(section, kink, d);
}

// The prism's feet under the disc chamfered d; a null shape when it fails.
TopoDS_Shape chamfered_feet(const TopoDS_Shape& body, double d) {
  const std::vector<TopoDS_Edge> feet = port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - 4) < 1e-6 && std::hypot(p.X(), p.Y()) < 5.0;
  });
  CHECK(feet.size() == 4u);
  return port::chamfered(body, feet, d);
}

bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < 1e-4) {
      return true;
    }
  }
  return false;
}

// Chamfers of 2: each corner computed on the disc's cylinder, which cuts
// the chamfers; the corner's vertex where their intersection meets it.
void check_corners_beyond_the_underside(const gp_Dir& seam) {
  const TopoDS_Shape body = prism_under_disc(seam);
  CHECK(port::valid(body));
  const TopoDS_Shape result = chamfered_feet(body, 2.0);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(result) - port::volume_of(body), added_by(2.0), 1e-6);
  // Four chamfers; the underside in four pieces between them and the rim.
  CHECK(port::count_of(result, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 7);
  const double s = kDisc / std::sqrt(2.0) - kHalf;
  for (int k = 0; k < 4; ++k) {
    const double angle = test::kPi / 4.0 + k * test::kPi / 2.0;
    CHECK(has_vertex_near(result, gp_Pnt(kDisc * std::cos(angle), kDisc * std::sin(angle), 2.0 + s)));
  }
}

void test_chamfer_corners_beyond_the_underside() {
  check_corners_beyond_the_underside(gp_Dir(1, 0, 0));
}

// The cylinder's seam on a corner's diagonal: that corner's point lies on
// the seam's prolongation.
void test_chamfer_corner_beyond_the_underside_on_the_seam() {
  check_corners_beyond_the_underside(gp_Dir(1, 1, 0));
}

// The boundary: chamfers of 0.8, whose contacts cross on the underside: an
// ordinary corner, the underside one face, as stock.
void test_chamfer_corners_crossing_on_the_underside_are_as_stock() {
  const TopoDS_Shape body   = prism_under_disc(gp_Dir(1, 0, 0));
  const TopoDS_Shape result = chamfered_feet(body, 0.8);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  const double d = 0.8;
  CHECK_NEAR_TOL(added_by(d), 4.0 * kHalf * d * d + 4.0 * d * d * d / 3.0, 1e-9);
  CHECK_NEAR_TOL(port::volume_of(result) - port::volume_of(body), added_by(d), 1e-6);
  CHECK(port::count_of(result, TopAbs_FACE) == port::count_of(body, TopAbs_FACE) + 4);
}

} // namespace

MITCAD_PORT_TEST("0143", test_chamfer_corners_beyond_the_underside);
MITCAD_PORT_TEST("0143", test_chamfer_corner_beyond_the_underside_on_the_seam);
MITCAD_PORT_TEST("0143", test_chamfer_corners_crossing_on_the_underside_are_as_stock);
