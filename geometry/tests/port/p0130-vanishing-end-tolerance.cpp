// SPDX-License-Identifier: MIT
// Port patch 0130: a rounding whose two faces come tangent at the vertex
// where it ends vanishes there. Where the walk stops a little short of the
// vertex (its sections singular), or the faces are tangent there only to
// within micrometres, the stripe kept an end section of micrometres off
// the vertex: the corner at the vertex failed, or built a cap of
// micrometres. ChFi3d_ShrinkVanishingEnd moves the walked contact curves'
// ends to the vertex (within a hundredth of the radius, the faces' normals
// there within 0.002 rad), raises their tolerance by the distance their
// end moved, and the rounding ends in a point there.
//
// A block 40 x 20 x 25 whose four vertical edges are rounded with 2, cut to
// a vault above z = 10: a cylinder of radius r along x whose axis lies at
// y = 10, z = 10. With r = 10 the vault springs tangentially from the walls
// y = 0 and y = 20; a larger r makes it cross them at an angle of
// sqrt(2 (r - 10) / 10). The vault's end x = 0 rounded with 1 (the arc on
// the end face and the two curves where the vault meets the corner
// roundings, one chain): the rounding vanishes where the vault comes
// tangent to the corner roundings at the springing.
// - r = 10 (tangent: the walk stops short of the vertex), 10.00001 and
//   10.0001 (0.0014 and 0.0045 rad apart there): built, valid, each
//   removing what the tangent vault's rounding removes when it ends in a
//   point (4.8 to 4.9, within 1e-4), every tolerance within the hundredth
//   of the radius the ends may move.
// - The boundary: r = 10.001 (0.014 rad, past the 0.002): no end moved, a
//   corner of its own, built and valid as before (removing a little less).

#include <cmath>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>

#include "port_test.hpp"

namespace {

// The vaulted block for the vault's radius r.
TopoDS_Shape vaulted_block(double r) {
  const TopoDS_Shape       block = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(40, 20, 25)).Shape();
  BRepFilletAPI_MakeFillet corners(block);
  for (TopExp_Explorer e(block, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    if (std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9) {
      corners.Add(2.0, edge);
    }
  }
  const TopoDS_Shape rounded = port::built(corners, "vertical edges");
  if (rounded.IsNull()) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape lower = BRepPrimAPI_MakeBox(gp_Pnt(-1, -1, -1), gp_Pnt(41, 21, 10)).Shape();
  const TopoDS_Shape vault = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-1, 10, 10), gp_Dir(1, 0, 0)), r, 42).Shape();
  const TopoDS_Shape tool  = BRepAlgoAPI_Fuse(lower, vault).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Common(rounded, tool).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The vault's end x = 0 above the springing rounded with 1; a null shape
// where that fails.
TopoDS_Shape vault_end_rounded(const TopoDS_Shape& body) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&      edge = TopoDS::Edge(e.Current());
    const gp_Pnt            a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt            b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m        = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const bool              vertical = std::abs(a.X() - b.X()) < 1e-9 && std::abs(a.Y() - b.Y()) < 1e-9;
    if (!vertical && m.X() < 2 + 1e-6 && m.Z() > 10 + 1e-6 && a.Z() > 10 - 1e-6 && b.Z() > 10 - 1e-6 &&
        fillet.Contour(edge) == 0) {
      fillet.Add(1.0, edge);
    }
  }
  CHECK(fillet.NbContours() == 1);
  return port::built(fillet, "vault end");
}

// What the rounding of the vault's end removes; -1 where it fails or is
// invalid.
double removed_by_rounding(double r, double& tolerance) {
  const TopoDS_Shape body = vaulted_block(r);
  CHECK(port::valid(body));
  if (!port::valid(body)) {
    return -1.0;
  }
  const TopoDS_Shape result = vault_end_rounded(body);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return -1.0;
  }
  tolerance = port::max_tolerance(result);
  return port::volume_of(body) - port::volume_of(result);
}

// The tangent vault and vaults a hundredth and a tenth of a micrometre
// off: the rounding vanishes at the springing in a point.
void test_rounding_vanishing_where_its_faces_come_tangent() {
  double       tolerance = 0.0;
  const double tangent   = removed_by_rounding(10.0, tolerance);
  CHECK(tangent > 4.8 && tangent < 4.9);
  CHECK(tolerance < 0.01);
  for (const double r : {10.00001, 10.0001}) {
    const double removed = removed_by_rounding(r, tolerance);
    CHECK(removed > 0.0);
    if (removed <= 0.0) {
      continue;
    }
    CHECK_NEAR_TOL(removed, tangent, 1e-4);
    CHECK(tolerance < 0.01);
  }
}

// The boundary: the vault crosses the walls at 0.014 rad, past the 0.002:
// a corner of its own, as before, removing a little less.
void test_rounding_at_a_vault_crossing_the_walls_is_as_before() {
  double       tolerance = 0.0;
  const double removed   = removed_by_rounding(10.001, tolerance);
  CHECK(removed > 4.8 && removed < 4.9);
}

} // namespace

MITCAD_PORT_TEST("0130", test_rounding_vanishing_where_its_faces_come_tangent);
MITCAD_PORT_TEST("0130", test_rounding_at_a_vault_crossing_the_walls_is_as_before);
