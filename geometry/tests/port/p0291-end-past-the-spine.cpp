// SPDX-License-Identifier: MIT
// Port patch 0291: a rounding along a top face's edge with a wall, where
// the top narrows to nothing at the spine's end and the wall runs on past
// that vertex into a face tangent to it. The top's contact runs off the top
// across its edge with a sloped face before the end; the piece walked from
// there past the spine's end has that side given up, and the corner dropped
// that piece and failed. With the patch the piece continues the blend
// before it, cut by the sloped face and by the face beyond the wall.
//
// A block outside a wall (the plane y = -20 for x <= 0, then the cylinder
// of radius 20 about the z axis, turning at angle 0.36 into its tangent
// plane), its top cut by a plane falling away from the wall at a slope of
// 1/2 through the wall's top at the angles 0.3 and 0.7: the top a strip
// narrowing to nothing at angle 0.3. test_fillet_corners.cpp rounds the
// top's edge with the wall with 1 and checks the volume against the block
// whose wall stays the cylinder. Here the same body mirrored in the plane
// x = 0 (the blend's faces the other way round): built, valid, the same
// volume removed as from the body itself. The boundary: the cut plane
// lifted by 0.75 at the wall, so that the top stays at least 1.1 wide and
// the contact of a rounding of 1 stays on it up to the block's end: built,
// valid, as stock.

#include <cmath>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

// The point at angle phi round the wall's circle (radius 20 about the z
// axis) at z, the angle from -y towards +x.
gp_Pnt on_wall(double phi, double z) {
  return gp_Pnt(20.0 * std::sin(phi), -20.0 * std::cos(phi), z);
}

// The block x in [20 sin(-0.5), 12], y in [-30, -5], z in [-10, 0] outside
// the wall; with `turn` the wall turns at angle 0.36 into its tangent
// plane, else it stays the cylinder. The top cut by the plane through the
// wall's points at the angles 0.3 and 0.7, `lift` above the top, falling
// away from the wall at a slope of 1/2.
TopoDS_Shape strip_on_a_wall(bool turn, double lift) {
  const double x0   = 20.0 * std::sin(-0.5);
  TopoDS_Shape body = BRepPrimAPI_MakeBox(gp_Pnt(x0, -30, -10), gp_Pnt(12, -5, 0)).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -20), gp_Dir(0, 0, 1)), 20, 40).Shape())
           .Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Pnt(x0 - 1, -20, -11), gp_Pnt(0, 0, 1)).Shape()).Shape();
  if (turn) {
    const gp_Pnt p = on_wall(0.36, -11);
    const gp_Dir t(std::cos(0.36), std::sin(0.36), 0);
    body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeBox(gp_Ax2(p, gp_Dir(0, 0, 1), t), 30, 30, 22).Shape()).Shape();
  }
  const gp_Pnt a = on_wall(0.7, lift), b = on_wall(0.3, lift);
  const gp_Vec along(a, b);
  gp_Vec       away(-along.Y(), along.X(), 0);
  if (away.X() * (a.X() + b.X()) + away.Y() * (a.Y() + b.Y()) < 0) {
    away.Reverse();
  }
  gp_Dir normal(along.Crossed(away.Normalized() + gp_Vec(0, 0, -0.5)));
  if (normal.Z() < 0) {
    normal.Reverse();
  }
  const TopoDS_Face            cut = BRepBuilderAPI_MakeFace(gp_Pln(a, normal), -80, 80, -80, 80).Face();
  ShapeUpgrade_UnifySameDomain unify(
    BRepAlgoAPI_Cut(body, BRepPrimAPI_MakePrism(cut, gp_Vec(normal) * 60.0).Shape()).Shape(), false, true, true);
  unify.Build();
  return unify.Shape();
}

TopoDS_Shape mirrored_in_x(const TopoDS_Shape& shape) {
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  return BRepBuilderAPI_Transform(shape, mirror, true).Shape();
}

// The top's edges with the wall (z = 0, on the wall's plane or cylinder)
// on the side of x = 6 where the wall's plane is (`side` 1: x < 6, -1 for
// the mirrored body: x > -6), rounded with 1; their tangent chain is one
// contour. A null shape when that fails.
TopoDS_Shape wall_top_rounded(const TopoDS_Shape& body, double side) {
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const bool on_cylinder    = curve.GetType() == GeomAbs_Circle && std::abs(std::hypot(m.X(), m.Y()) - 20) < 1e-7;
    const bool on_plane       = curve.GetType() == GeomAbs_Line && std::abs(m.Y() + 20) < 1e-7;
    if (std::abs(m.Z()) < 1e-7 && side * m.X() < 6 && (on_cylinder || on_plane)) {
      edges.push_back(edge);
    }
  }
  CHECK(!edges.empty());
  if (edges.empty()) {
    return TopoDS_Shape();
  }
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(1.0, edge);
  }
  CHECK(fillet.NbContours() == 1);
  return port::built(fillet, "wall top");
}

// The tapering strip mirrored: the rounding runs on past the strip's end
// as the blend before it, cut by the sloped face and the wall's tangent
// plane, and removes what it removes from the strip itself.
void test_mirrored_rounding_past_the_end_of_a_tapering_strip() {
  const TopoDS_Shape body     = strip_on_a_wall(true, 0.0);
  const TopoDS_Shape mirrored = mirrored_in_x(body);
  CHECK(port::valid(mirrored));
  const TopoDS_Shape result = wall_top_rounded(mirrored, -1.0);
  CHECK(port::valid(result));
  const TopoDS_Shape reference = wall_top_rounded(body, 1.0);
  CHECK(port::valid(reference));
  if (!port::valid(result) || !port::valid(reference)) {
    return;
  }
  CHECK_NEAR_TOL(port::volume_of(mirrored) - port::volume_of(result),
                 port::volume_of(body) - port::volume_of(reference), 1e-5);
  CHECK(port::count_of(result, TopAbs_FACE) == port::count_of(reference, TopAbs_FACE));
}

// The boundary: the cut lifted by 0.75 at the wall, the top at least 1.1
// wide; the wall stays the cylinder up to the block's end x = 12. The
// contact of 1 stays on the top all along: no side given up, the rounding
// ends at the block's end faces as stock. It takes the section of a
// straight rounding of 1 along the plane's 9.6 and about the arc's
// 0.64 x 20.2 (within a fifth: the end at x = 12 is oblique).
void test_rounding_whose_contact_stays_on_the_top_is_as_stock() {
  const TopoDS_Shape body = strip_on_a_wall(false, 0.75);
  CHECK(port::valid(body));
  const TopoDS_Shape result = wall_top_rounded(body, 1.0);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  const double removed = port::volume_of(body) - port::volume_of(result);
  const double length  = 20.0 * std::sin(0.5) + 20.3 * std::asin(0.6);
  CHECK(removed > 0.8 * test::fillet_loss(1.0) * length);
  CHECK(removed < 1.2 * test::fillet_loss(1.0) * length);
}

} // namespace

MITCAD_PORT_TEST("0291", test_mirrored_rounding_past_the_end_of_a_tapering_strip);
MITCAD_PORT_TEST("0291", test_rounding_whose_contact_stays_on_the_top_is_as_stock);
