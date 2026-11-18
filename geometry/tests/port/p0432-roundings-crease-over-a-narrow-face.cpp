// SPDX-License-Identifier: MIT
// Port patch 0432: roundings of edges meeting at an angle over a common
// face narrower than the radius. Each rounding ran over the common face
// onto the face beyond it (0011), the two onto different faces, and at
// their vertex the two had no face in common: OCCT's corner of two
// roundings failed ("Failed to find edge"). With the patch both roll on the
// common face's far edges (RollsInsteadOfOverflow) and meet in a crease,
// the intersection of the two blends from their point on the edge through
// the corner's vertex (the pivot) to the far edges' common vertex V2
// (PerformTwoRolledCorner).
//
// A second geometry to geometry.fillet_corners' lipped rim (lines and arcs
// meeting at 53 degrees, the arcs' blends tori): a square rim, lines only,
// meeting at 90 degrees. A frame 60 square round holes measured in from
// the sides of a square 40 (the frame 10 thicker, so that the balls stay
// inside it): a wall (z from 0 to 10) round a hole 4 in, a slab (z from 10 to 11) whose hole narrows
// at 45 degrees to 5 in at its top, and a lip (z from 11 to 16) round a
// hole `lip` in. With lip = 6 the lip overhangs a ring 1 wide (z = 11, from
// 5 to 6 in) and the chamfer beyond it; the lip's four inner bottom edges
// are rounded with 4. Each ball, 4 in from the lip's face, passes through
// the ring's far edge (its centre sqrt 7 above the ring); each pair meets
// in the plane of the corner's diagonal, from the lip's vertical edge
// sqrt 7 above the ring down to the ring's corner. Built, valid, both ends
// of each crease vertices of the result, the material taken off each
// side's section swept along its edge up to the diagonals, exactly. The
// boundary: lip = 10, a ring 5 wide, which holds the radius: analytic
// roundings meeting in a crease as stock, the material taken off likewise.

#include <cmath>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

constexpr double kSize   = 40.0; // the frame's hole is measured in from 0 and 40
constexpr double kMargin = 10.0; // the frame reaches that far out of the square
constexpr double kRing = 11.0; // the ring's height
constexpr double kRadius = 4.0;

// A square prism, its sides `in` in from the frame's, from z0 to z1.
TopoDS_Shape square_prism(double in, double z0, double z1) {
  BRepBuilderAPI_MakePolygon square(gp_Pnt(in, in, z0), gp_Pnt(kSize - in, in, z0), gp_Pnt(kSize - in, kSize - in, z0),
                                    gp_Pnt(in, kSize - in, z0), true);
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(square.Wire()).Face(), gp_Vec(0, 0, z1 - z0)).Shape();
}

// The frame between z0 and z1 round the hole `in` in.
TopoDS_Shape frame_part(double in, double z0, double z1) {
  const TopoDS_Shape outer =
    BRepPrimAPI_MakeBox(gp_Pnt(-kMargin, -kMargin, z0), gp_Pnt(kSize + kMargin, kSize + kMargin, z1)).Shape();
  return BRepAlgoAPI_Cut(outer, square_prism(in, z0, z1)).Shape();
}

// The rim with its lip `lip` in.
TopoDS_Shape square_rim(double lip) {
  const TopoDS_Shape       hole = square_prism(4.0, kRing - 1.0, kRing);
  BRepOffsetAPI_DraftAngle draft(hole);
  for (TopExp_Explorer f(hole, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(f.Current()));
    if (std::abs(surface.Plane().Axis().Direction().Z()) < 0.5) {
      draft.Add(TopoDS::Face(f.Current()), gp_Dir(0, 0, 1), test::kPi / 4.0,
                gp_Pln(gp_Pnt(0, 0, kRing - 1.0), gp_Dir(0, 0, 1)));
    }
  }
  draft.Build();
  const TopoDS_Shape slab =
    BRepAlgoAPI_Cut(BRepPrimAPI_MakeBox(gp_Pnt(-kMargin, -kMargin, kRing - 1.0), gp_Pnt(kSize + kMargin, kSize + kMargin, kRing))
                      .Shape(),
                    draft.Shape())
      .Shape();
  const TopoDS_Shape wall = BRepAlgoAPI_Fuse(frame_part(4.0, 0.0, kRing - 1.0), slab).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(frame_part(lip, kRing, kRing + 5.0), wall).Shape(), true, true,
                                     false);
  unify.Build();
  return unify.Shape();
}

// The lip's four inner bottom edges (z = 11, `lip` in) rounded with 4.
TopoDS_Shape lip_rounded(const TopoDS_Shape& body, double lip) {
  const std::vector<TopoDS_Edge> edges = port::edges_where(body, [lip](const gp_Pnt& m) {
    const double in = std::min(std::min(m.X(), kSize - m.X()), std::min(m.Y(), kSize - m.Y()));
    return std::abs(m.Z() - kRing) < 1e-9 && std::abs(in - lip) < 1e-9;
  });
  CHECK(edges.size() == 4);
  if (edges.size() != 4) {
    return TopoDS_Shape();
  }
  return port::rounded(body, edges, kRadius);
}

// The material the four roundings take off, the lip `lip` in: across each
// edge (t from the lip's face, u = r - t from the ball's centre) the region
// between the lip's face, the ring and the ball, of height
// h(u) = c - sqrt(r^2 - u^2) for t from 0 to w (w the ring's width, or r
// where the ball touches the ring), c = sqrt(r^2 - (r - w)^2) the centre's
// height; at t it runs along the edge up to the diagonals, 2 t longer than
// the lip's side. A = integral of h, U = integral of u h = c^3 / 6.
double taken_off(double lip, double w) {
  const double r = kRadius, c = std::sqrt(r * r - (r - w) * (r - w));
  const double a = w * c - (0.25 * test::kPi * r * r - 0.5 * (r - w) * c - 0.5 * r * r * std::asin((r - w) / r));
  const double moment = r * a - c * c * c / 6.0; // integral of t h
  return 4.0 * ((kSize - 2.0 * lip) * a + 2.0 * moment);
}

// A vertex of the shape at p, within its tolerance.
bool has_vertex_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(v.Current());
    if (BRep_Tool::Pnt(vertex).Distance(p) <= BRep_Tool::Tolerance(vertex) + 1e-6) {
      return true;
    }
  }
  return false;
}

// The ring 1 wide: the roundings roll on its far edges and meet in creases.
void test_roundings_crease_over_a_narrow_ring() {
  const TopoDS_Shape body = square_rim(6.0);
  CHECK(port::valid(body));
  const TopoDS_Shape rounded = lip_rounded(body, 6.0);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded), taken_off(6.0, 1.0), 1e-6);
  const double up = kRing + std::sqrt(7.0); // the contacts on the lip's faces
  for (const double x : {0.0, kSize}) {
    for (const double y : {0.0, kSize}) {
      const double sx = x == 0.0 ? 1.0 : -1.0, sy = y == 0.0 ? 1.0 : -1.0;
      CHECK(has_vertex_at(rounded, gp_Pnt(x + 6.0 * sx, y + 6.0 * sy, up)));     // on the pivot
      CHECK(has_vertex_at(rounded, gp_Pnt(x + 5.0 * sx, y + 5.0 * sy, kRing))); // V2
    }
  }
}

// The boundary: a ring 5 wide; each ball touches it 1 short of its far
// edge, the roundings analytic as stock.
void test_roundings_over_a_wide_ring_are_as_stock() {
  const TopoDS_Shape body    = square_rim(10.0);
  const TopoDS_Shape rounded = lip_rounded(body, 10.0);
  CHECK(port::valid(rounded));
  if (!port::valid(rounded)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(body) - port::volume_of(rounded), taken_off(10.0, kRadius), 1e-6);
}

} // namespace

MITCAD_PORT_TEST("0432", test_roundings_crease_over_a_narrow_ring);
MITCAD_PORT_TEST("0432", test_roundings_over_a_wide_ring_are_as_stock);
