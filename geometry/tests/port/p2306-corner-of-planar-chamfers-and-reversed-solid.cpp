// SPDX-License-Identifier: MIT
// Port patch 2306 (mitcad#2306), with OCCT's dressups alone:
// - the corner where four or more planar chamfers meet round a vertex,
//   their sections there in one plane, is that plane
//   (ChFi3d_Builder::PerformMoreThreeCorner): the octahedron
//   |x| + |y| + |z| <= 10 with all twelve edges chamfered sqrt(6) is one
//   valid solid of 26 planar faces with the exact volume and area. Before,
//   the corner's plate, held G1 to each chamfer's plane, was not done at
//   any vertex and the chamfer failed;
// - a solid whose own orientation is reversed around a shell facing in
//   (what BRepLib::OrientClosedSolid makes of such a shell) is blended as
//   the same solid oriented forward (ChFi3d_Builder's constructor): a
//   box's edge and a corner's three edges chamfered, the three rounded,
//   valid and with the volumes of the box given forward. Before, every
//   face of the input the blends did not make came out facing in: an
//   invalid shape, its volume negative.

#include <cmath>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>

#include "port_test.hpp"

namespace {

bool at(double value, double target) { return std::abs(value - target) < 1e-7; }

// The octahedron |x| + |y| + |z| <= 10, its eight triangles sewn into one
// shell; null when the sewing gives no shell.
TopoDS_Shell octahedron_shell() {
  BRepBuilderAPI_Sewing sewing;
  for (const double sx : {-1.0, 1.0}) {
    for (const double sy : {-1.0, 1.0}) {
      for (const double sz : {-1.0, 1.0}) {
        BRepBuilderAPI_MakePolygon wire(gp_Pnt(10.0 * sx, 0, 0), gp_Pnt(0, 10.0 * sy, 0), gp_Pnt(0, 0, 10.0 * sz),
                                        true);
        sewing.Add(BRepBuilderAPI_MakeFace(wire.Wire(), true).Face());
      }
    }
  }
  sewing.Perform();
  const TopoDS_Shape sewn = sewing.SewedShape();
  if (sewn.IsNull() || sewn.ShapeType() != TopAbs_SHELL) {
    return TopoDS_Shell();
  }
  return TopoDS::Shell(sewn);
}

// The octahedron's twelve edges chamfered sqrt(6): one valid solid of 26
// planar faces (each corner a square in the plane |x| = 6 and so on), its
// volume and area the exact ones within 1e-9.
void check_octahedron_chamfered(const TopoDS_Shape& body) {
  CHECK(std::abs(port::volume_of(body) - 4000.0 / 3.0) < 1e-6);
  const std::vector<TopoDS_Edge> edges = port::edges_where(body, [](const gp_Pnt&) { return true; });
  CHECK(edges.size() == 12);
  const TopoDS_Shape chamfered = port::chamfered(body, edges, std::sqrt(6.0));
  CHECK(port::valid(chamfered));
  if (!port::valid(chamfered)) {
    return;
  }
  CHECK(port::count_of(chamfered, TopAbs_SOLID) == 1);
  CHECK(port::count_of(chamfered, TopAbs_FACE) == 26);
  int planes = 0;
  for (TopExp_Explorer f(chamfered, TopAbs_FACE); f.More(); f.Next()) {
    planes += BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Plane ? 1 : 0;
  }
  CHECK(planes == 26);
  CHECK_NEAR_TOL(port::volume_of(chamfered), 2752.0 / 3.0, 1e-9);
  CHECK_NEAR_TOL(port::area_of(chamfered), 64.0 * std::sqrt(3.0) + 192.0 * std::sqrt(2.0) + 96.0, 1e-9);
}

// The octahedron as a solid oriented forward whose shell, facing in as
// sewn, it holds reversed.
void test_octahedron_chamfered_all_round_forward_solid() {
  const TopoDS_Shell shell = octahedron_shell();
  CHECK(!shell.IsNull());
  if (shell.IsNull()) {
    return;
  }
  BRep_Builder builder;
  TopoDS_Solid body;
  builder.MakeSolid(body);
  builder.Add(body, shell);
  if (port::volume_of(body) < 0.0) {
    builder.MakeSolid(body);
    builder.Add(body, shell.Reversed());
  }
  CHECK(body.Orientation() == TopAbs_FORWARD);
  check_octahedron_chamfered(body);
}

// The octahedron as the benchmark case's test makes it (mitcad#2273): the
// solid of the sewn shell, then BRepLib::OrientClosedSolid, which reverses
// the solid where the shell faces in.
void test_octahedron_chamfered_all_round_oriented_solid() {
  const TopoDS_Shell shell = octahedron_shell();
  CHECK(!shell.IsNull());
  if (shell.IsNull()) {
    return;
  }
  BRepBuilderAPI_MakeSolid solid(shell);
  TopoDS_Solid body = solid.Solid();
  BRepLib::OrientClosedSolid(body);
  check_octahedron_chamfered(body);
}

// A box 10 x 10 x 10 at the origin as a solid oriented reversed around its
// shell turned to face in: the same body.
TopoDS_Solid reversed_box() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
  TopoDS_Iterator    it(box);
  BRep_Builder       builder;
  TopoDS_Solid       body;
  builder.MakeSolid(body);
  builder.Add(body, it.Value().Reversed());
  BRepLib::OrientClosedSolid(body);
  return body;
}

// The box's edge along x at the origin, and the three edges at the origin.
std::vector<TopoDS_Edge> box_edges(const TopoDS_Shape& body, bool corner) {
  return port::edges_where(body, [corner](const gp_Pnt& m) {
    const int zeros = (at(m.X(), 0) ? 1 : 0) + (at(m.Y(), 0) ? 1 : 0) + (at(m.Z(), 0) ? 1 : 0);
    return zeros == 2 && (corner || at(m.X(), 5));
  });
}

void test_reversed_box_blended_as_the_box() {
  const TopoDS_Shape forward = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
  const TopoDS_Solid body    = reversed_box();
  CHECK(body.Orientation() == TopAbs_REVERSED);
  CHECK_NEAR_TOL(port::volume_of(body), 1000.0, 1e-9);
  for (int kind = 0; kind < 3; ++kind) {
    const bool                     corner = kind > 0;
    const std::vector<TopoDS_Edge> edges  = box_edges(body, corner);
    const std::vector<TopoDS_Edge> plain  = box_edges(forward, corner);
    CHECK(edges.size() == (corner ? 3u : 1u));
    CHECK(plain.size() == edges.size());
    const TopoDS_Shape made = kind == 2 ? port::rounded(body, edges, 1.0) : port::chamfered(body, edges, 1.0);
    const TopoDS_Shape expected =
      kind == 2 ? port::rounded(forward, plain, 1.0) : port::chamfered(forward, plain, 1.0);
    CHECK(port::valid(expected));
    CHECK(port::valid(made));
    if (!port::valid(made) || !port::valid(expected)) {
      continue;
    }
    CHECK(port::count_of(made, TopAbs_SOLID) == 1);
    CHECK(port::count_of(made, TopAbs_FACE) == port::count_of(expected, TopAbs_FACE));
    CHECK_NEAR_TOL(port::volume_of(made), port::volume_of(expected), 1e-9);
    CHECK_NEAR_TOL(port::area_of(made), port::area_of(expected), 1e-9);
  }
  // The edge's chamfer removes the prism 1 x 1 / 2 x 10.
  const std::vector<TopoDS_Edge> one = box_edges(body, false);
  if (one.size() == 1) {
    const TopoDS_Shape made = port::chamfered(body, one, 1.0);
    if (port::valid(made)) {
      CHECK_NEAR_TOL(port::volume_of(made), 995.0, 1e-9);
    }
  }
}

} // namespace

MITCAD_PORT_TEST("2306", test_octahedron_chamfered_all_round_forward_solid);
MITCAD_PORT_TEST("2306", test_octahedron_chamfered_all_round_oriented_solid);
MITCAD_PORT_TEST("2306", test_reversed_box_blended_as_the_box);
