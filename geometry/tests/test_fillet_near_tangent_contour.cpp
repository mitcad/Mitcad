// SPDX-License-Identifier: MIT
// A fillet contour that runs on into an edge whose faces are only a few
// hundredths of a radian apart, built here with OCCT's fillet alone on a
// synthetic shape: the cases behind the patches 0860 and 0861 of the OCCT
// port (third_party/vcpkg-ports, mitcad#441). OCCT takes such faces for
// tangent (up to 0.1 rad): the contour from the edges before stopped at the
// edge, which then started a contour of its own that took those edges again
// from the other side; and where the blend is thinner than the tolerance of
// the vertex before that edge, the face beside it kept the edge the blend
// takes away, its wire open.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_Map.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The faces of the edge asked for meet at this angle (radians): above the
// fillet's angular tolerance (0.01), below the 0.1 within which OCCT takes
// faces for tangent.
constexpr double kKink = 0.05;
// The rounding's radius.
constexpr double kRadius = 0.5;

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The direction of the walls' rulings: along y, falling by kKink.
gp_Dir ruling() { return gp_Dir(0.0, std::cos(kKink), -std::sin(kKink)); }

// A point of the section through the origin across the rulings: u along x,
// w along the walls' normal at the origin (0, sin kKink, cos kKink).
gp_Pnt section_point(double u, double w) {
  return gp_Pnt(u, w * std::sin(kKink), w * std::cos(kKink));
}

// A block under the plane z = 0 whose top meets a wall along a chain of
// three edges, the wall made of three faces, each going on tangentially
// from the one before along a ruling: a plane B falling away from the top
// by kKink (the edge E1 along x from x = -3 to the origin V), a cylinder A
// of radius 1 turning down from it over 0.6 rad (E2, an arc of an ellipse
// from V) and a plane C going on from A (E3, up to the block's side
// y = -6). The faces of E1 meet at kKink all along it, those of E2 from
// kKink at V to 0.6 rad, those of E3 at 0.6 rad. (The walls are a prism of
// the section along the rulings, cut by the box x -3 to 3, y -6 to 3, z -2
// to 0.)
TopoDS_Shape block() {
  const double turn  = 0.6;
  const gp_Pnt a     = section_point(-4.0, 0.0);
  const gp_Pnt v     = section_point(0.0, 0.0);
  const gp_Pnt mid   = section_point(std::sin(0.5 * turn), std::cos(0.5 * turn) - 1.0);
  const gp_Pnt b     = section_point(std::sin(turn), std::cos(turn) - 1.0);
  const double along = 4.0;
  const double right = std::sin(turn) + along * std::cos(turn);
  const gp_Pnt c     = section_point(right, std::cos(turn) - 1.0 - along * std::sin(turn));
  const gp_Pnt d     = section_point(right, -3.0);
  const gp_Pnt e     = section_point(-4.0, -3.0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, v).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(v, mid, b).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(b, c).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, d).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(d, e).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(e, a).Edge());
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const double      reach   = 12.0;
  gp_Trsf           back;
  back.SetTranslation(gp_Vec(ruling()) * -reach);
  const TopoDS_Shape start = BRepBuilderAPI_Transform(section, back, true).Shape();
  const TopoDS_Shape walls = BRepPrimAPI_MakePrism(start, gp_Vec(ruling()) * (2.0 * reach)).Shape();
  const TopoDS_Shape box   = BRepPrimAPI_MakeBox(gp_Pnt(-3, -6, -2), gp_Pnt(3, 3, 0)).Shape();
  return BRepAlgoAPI_Common(walls, box).Shape();
}

// The top's edges, from x = -3 to the block's side y = -6: E1, E2, E3.
struct Chain {
  TopoDS_Edge   e1, e2, e3;
  TopoDS_Vertex v; // between E1 and E2
};

Chain chain_of(const TopoDS_Shape& body) {
  Chain chain;
  for (TopExp_Explorer ex(body, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge       edge = TopoDS::Edge(ex.Current());
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            p = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(p.Z()) > 1e-6) {
      continue;
    }
    if (std::abs(p.Y()) < 1e-6 && p.X() < 0.0) {
      chain.e1 = edge;
    } else if (curve.GetType() != GeomAbs_Line && p.Y() < -0.1 && p.Y() > -5.9) {
      chain.e2 = edge;
    } else if (curve.GetType() == GeomAbs_Line && p.X() > 0.0 && p.Y() < -0.1 && p.Y() > -5.9) {
      chain.e3 = edge;
    }
  }
  for (TopExp_Explorer ex(chain.e1, TopAbs_VERTEX); ex.More(); ex.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())).Distance(gp_Pnt(0, 0, 0)) < 1e-6) {
      chain.v = TopoDS::Vertex(ex.Current());
    }
  }
  return chain;
}

// The chain rounded with kRadius by OCCT's fillet, every edge asked for
// (StopChainsAtTurns), the contour started from `first`: the number of
// contours, of edges in the first, and the result (null where OCCT fails).
struct Rounding {
  int          contours = 0;
  int          edges    = 0;
  TopoDS_Shape result;
};

Rounding rounded(const TopoDS_Shape& body, const Chain& chain, const TopoDS_Edge& first) {
  Rounding                                               out;
  BRepFilletAPI_MakeFillet                               fillet(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> asked;
  asked.Add(chain.e1);
  asked.Add(chain.e2);
  asked.Add(chain.e3);
  fillet.StopChainsAtTurns(asked);
  fillet.Add(kRadius, first);
  for (const TopoDS_Edge& edge : {chain.e1, chain.e2, chain.e3}) {
    if (fillet.Contour(edge) == 0) {
      fillet.Add(kRadius, edge);
    }
  }
  out.contours = fillet.NbContours();
  out.edges    = out.contours > 0 ? fillet.NbEdges(1) : 0;
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "near-tangent contour test: %s\n", failure.what());
    return out;
  }
  if (fillet.IsDone()) {
    out.result = fillet.Shape();
  }
  return out;
}

// The volume a rounding of radius r removes along an edge whose faces'
// normals are `kink` apart, per unit length.
double section_area(double r, double kink) { return r * r * (std::tan(0.5 * kink) - 0.5 * kink); }

// Each fillet starts from the edge E2 between the ellipse's faces: the
// contour runs on over V into E1, whose faces are kKink apart, as one
// contour of the three edges, as it does when it starts from E1 (patch
// 0022 rounds an edge asked for between such faces). Without patch 0860 it
// stopped at V, and E1 started a second contour over E1, E2 and E3.
void test_contour_runs_on_into_a_near_tangent_edge() {
  const TopoDS_Shape body = block();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const Chain chain = chain_of(body);
  CHECK(!chain.e1.IsNull() && !chain.e2.IsNull() && !chain.e3.IsNull() && !chain.v.IsNull());
  if (chain.e1.IsNull() || chain.e2.IsNull() || chain.e3.IsNull() || chain.v.IsNull()) {
    return;
  }
  const Rounding from_e1 = rounded(body, chain, chain.e1);
  CHECK(from_e1.contours == 1);
  CHECK(from_e1.edges == 3);
  CHECK(!from_e1.result.IsNull());
  const Rounding from_e2 = rounded(body, chain, chain.e2);
  CHECK(from_e2.contours == 1);
  CHECK(from_e2.edges == 3);
  CHECK(!from_e2.result.IsNull());
  if (from_e1.result.IsNull() || from_e2.result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(from_e1.result).IsValid());
  CHECK(BRepCheck_Analyzer(from_e2.result).IsValid());
  const double removed = volume_of(body) - volume_of(from_e2.result);
  CHECK_NEAR_TOL(removed, volume_of(body) - volume_of(from_e1.result), 1e-6);
  // E1 (3 long) at kKink alone already removes this much; E2 and E3 more.
  CHECK(removed > 3.0 * section_area(kRadius, kKink));
}

// The same with the vertex V's tolerance as large as the stored bodies'
// (0.02, above the blend's half width at V, 0.0125): the contact on E2's
// wall ends at V across the wall's edge with E1's wall, not on E2. The wall
// keeps no part of E2 (patch 0861); it kept E2 whole beside the blend's
// end, its wire open, and the result was invalid.
void test_blend_thinner_than_the_vertex_tolerance() {
  const TopoDS_Shape body  = block();
  const Chain        chain = chain_of(body);
  if (chain.v.IsNull() || chain.e2.IsNull()) {
    CHECK(false);
    return;
  }
  BRep_Builder().UpdateVertex(chain.v, 0.02);
  const Rounding rounding = rounded(body, chain, chain.e2);
  CHECK(rounding.contours == 1);
  CHECK(!rounding.result.IsNull());
  if (rounding.result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(rounding.result).IsValid());
  // No edge of the result lies along E2 (its middle on the top's plane
  // and on E2's ellipse).
  const BRepAdaptor_Curve e2(chain.e2);
  const gp_Pnt           middle = e2.Value(0.5 * (e2.FirstParameter() + e2.LastParameter()));
  bool                   kept   = false;
  for (TopExp_Explorer ex(rounding.result, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
    kept = kept || curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(middle) < 1e-6;
  }
  CHECK(!kept);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_near_tangent_contour) {
  test::guarded("test_contour_runs_on_into_a_near_tangent_edge", test::test_contour_runs_on_into_a_near_tangent_edge);
  test::guarded("test_blend_thinner_than_the_vertex_tolerance", test::test_blend_thinner_than_the_vertex_tolerance);
  if (test::failures == 0) {
    std::puts("test_fillet_near_tangent_contour: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
