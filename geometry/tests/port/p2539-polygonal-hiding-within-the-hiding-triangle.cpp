// SPDX-License-Identifier: MIT
// Port patch 2539 (mitcad#2539, GitHub mitcad/Mitcad#729 and #765): a
// triangle of OCCT's polygonal hidden-line removal hides nothing of a
// segment outside the triangle's own box in the view.
//
// A drawing's front view of a reported subassembly, at 1:2 and 1:5, lost
// the outline lines of two shafts from their ends to a clamping collar: a
// sliver of the collar's bore (its radius the shaft's, a generator of its
// triangulation on the shaft's top line) lay along the shaft's outline
// segment, almost edge on and almost in its plane. The test rebuilds that
// pair with flat faces, in the coordinates the hidden-line removal had
// (to the last digit, since the case turns on rounding): the sliver, a
// triangular face 9.5 long and 0.03 high in the view, and a plate whose
// long top edge is the outline segment, from x -72.8 to 535.5, rising
// 0.48, crossing the sliver's plane inside it with its points before the
// crossing behind the plane. The projector is OCC28784's (looking along
// -x, the view's x the model's -z, its y the model's y), so a point
// (X, Y, Z) of the view is (Z, Y, -X) in the model; the faces are meshed
// as a drawing view meshes at 1:5 (0.06667 mm, 30 degrees).
//
// HLRAlgo_PolyData::hideByOneTriangle met the lines of the sliver's sides
// at a shallow angle, kept one crossing and rejected the other just outside
// its side, took the edge's start for inside the triangle and hid the edge
// from its start to the plane's crossing: 130.8 mm. Expected: no part of
// the edge left of the sliver (x below 55) hidden, and the edge seen there.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <HLRAlgo_Projector.hxx>
#include <HLRBRep_PolyAlgo.hxx>
#include <HLRBRep_PolyHLRToShape.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "port_test.hpp"

namespace {

// A point of the view in the model.
gp_Pnt model(double x, double y, double z) { return gp_Pnt(z, y, -x); }

TopoDS_Face flat(const std::vector<gp_Pnt>& corners) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& p : corners) {
    polygon.Add(p);
  }
  polygon.Close();
  return BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
}

// The length of the lines of `lines` along the edge (in the view's plane:
// nearly level, between y 292 and 293.5) that start left of x = 50, up to
// x = 58 (the sliver begins at x 60.76).
double left_of_the_sliver(const TopoDS_Shape& lines) {
  double length = 0.0;
  for (TopExp_Explorer it(lines, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    const gp_Pnt       a    = BRep_Tool::Pnt(TopExp::FirstVertex(edge));
    const gp_Pnt       b    = BRep_Tool::Pnt(TopExp::LastVertex(edge));
    const double       x0 = std::min(a.X(), b.X()), x1 = std::max(a.X(), b.X());
    const double       y0 = std::min(a.Y(), b.Y()), y1 = std::max(a.Y(), b.Y());
    if (x0 < 50.0 && x1 - x0 > 1.0 && y1 - y0 < 1.0 && y0 > 292.0 && y1 < 293.5) {
      length += std::min(x1, 58.0) - x0;
    }
  }
  return length;
}

void test_sliver_along_an_edge() {
  // The sliver, its corners as the reported view's triangulation had them.
  const TopoDS_Face sliver = flat({model(70.281048951142338, 292.56369267883906, -6.110614811508924),
                                   model(65.515106144185836, 292.58938638847337, -5.4985921116239354),
                                   model(60.756160548933387, 292.58562185647162, -5.4962007581319243)});
  // The plate: its top edge the outline segment, its lower corners in the
  // plane of the edge and the view's y, wide enough that the view's box
  // (and so the algorithm's tolerance) is that of the reported pair.
  const TopoDS_Face plate = flat({model(-72.798859129719034, 292.47997831006904, -5.4693005600774764),
                                  model(535.53093648450135, 292.9611747229921, -5.6001362353900159),
                                  model(536.17565002796914, 282.9611747229921, -5.6002748962487328),
                                  model(-73.433499999999995, 282.47997831006904, -5.4691640655847804)});
  TopoDS_Compound pair;
  BRep_Builder    builder;
  builder.MakeCompound(pair);
  builder.Add(pair, sliver);
  builder.Add(pair, plate);
  const BRepMesh_IncrementalMesh mesh(pair, 0.06667, false, 0.52359877559829882 /* 30 degrees */, false);

  const HLRAlgo_Projector       projector(gp_Ax2(gp::Origin(), gp::DX(), -gp::DZ()));
  occ::handle<HLRBRep_PolyAlgo> algo = new HLRBRep_PolyAlgo(pair);
  algo->Projector(projector);
  algo->Update();
  HLRBRep_PolyHLRToShape lines;
  lines.Update(algo);

  const double hidden = left_of_the_sliver(lines.HCompound());
  const double seen   = left_of_the_sliver(lines.VCompound());
  std::fprintf(stderr, "the edge left of the sliver: %.1f mm hidden, %.1f mm seen\n", hidden, seen);
  CHECK(hidden < 0.1);
  CHECK(seen > 130.0);
}

} // namespace

MITCAD_PORT_TEST("2539", test_sliver_along_an_edge);
