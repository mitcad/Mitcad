// SPDX-License-Identifier: MIT
// Port patch 0110: ShapeUpgrade_UnifySameDomain keeps a chain of edges
// whose curves cannot be concatenated (the end of one lies farther from the
// start of the next than the tolerance of the vertex between them) as its
// edges, and unifies the rest of the shape. Before, the concatenation's
// exception ("Courbes non jointives") ended the whole unification.
//
// A shell of two coplanar squares, [0, 1] x [0, 1] and [0, 1] x [1, 2],
// sharing their middle edge: the unification merges them into one face and
// their collinear sides into one line each. The first square's bottom is
// two B-spline pieces in line (degree 2) that meet at a vertex at
// (0.5, 0, 0): the first ends 1e-6 below it, the second starts 1e-6 above
// it, both with the same direction, so that the chain is mergeable but its
// curves lie 2e-6 apart. With the vertex's tolerance 1.5e-6 the curves do
// not join: one face of five edges, the two pieces kept (stock: the
// unification throws). With the vertex's tolerance 1e-5 they join and the
// chain becomes one edge: one face of four edges, as stock. The existing
// geometry/tests/test_large_bodies.cpp checks the chains on one face.

#include <cstdio>
#include <initializer_list>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRep_Builder.hxx>
#include <GeomAbs_CurveType.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pln.hxx>

#include "port_test.hpp"

namespace {

// A B-spline edge of degree 2 from a to b (its middle pole halfway), built
// as given between the vertices.
TopoDS_Edge spline(const gp_Pnt& a, const gp_Pnt& b, const TopoDS_Vertex& va, const TopoDS_Vertex& vb) {
  NCollection_Array1<gp_Pnt> poles(1, 3);
  poles(1) = a;
  poles(2) = gp_Pnt((a.XYZ() + b.XYZ()) / 2.0);
  poles(3) = b;
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> mults(1, 2);
  mults(1) = 3;
  mults(2) = 3;
  const occ::handle<Geom_BSplineCurve> curve = new Geom_BSplineCurve(poles, knots, mults, 2);
  BRep_Builder                         builder;
  TopoDS_Edge                          edge;
  builder.MakeEdge(edge, curve, 1e-7);
  builder.Add(edge, va.Oriented(TopAbs_FORWARD));
  builder.Add(edge, vb.Oriented(TopAbs_REVERSED));
  builder.Range(edge, 0.0, 1.0);
  return edge;
}

TopoDS_Face face_of(std::initializer_list<TopoDS_Shape> edges) {
  BRep_Builder builder;
  TopoDS_Wire  wire;
  builder.MakeWire(wire);
  for (const TopoDS_Shape& edge : edges) {
    builder.Add(wire, edge);
  }
  return BRepBuilderAPI_MakeFace(gp_Pln(gp::XOY()), wire).Face();
}

// The two squares as one shell; the vertex between the bottom's two pieces
// has the tolerance `joint`.
TopoDS_Shell two_squares(double joint) {
  BRep_Builder builder;
  const auto   vertex = [&builder](double x, double y, double tolerance) {
    TopoDS_Vertex v;
    builder.MakeVertex(v, gp_Pnt(x, y, 0), tolerance);
    return v;
  };
  const TopoDS_Vertex v0 = vertex(0, 0, 1e-7);
  const TopoDS_Vertex vm = vertex(0.5, 0, joint);
  const TopoDS_Vertex v1 = vertex(1, 0, 1e-7);
  const TopoDS_Vertex v2 = vertex(1, 1, 1e-7);
  const TopoDS_Vertex v3 = vertex(0, 1, 1e-7);
  const TopoDS_Vertex v4 = vertex(1, 2, 1e-7);
  const TopoDS_Vertex v5 = vertex(0, 2, 1e-7);
  // (Both pieces of slope -2e-6: the same direction at the vertex.)
  const TopoDS_Edge a      = spline(gp_Pnt(0, 0, 0), gp_Pnt(0.5, -1e-6, 0), v0, vm);
  const TopoDS_Edge b      = spline(gp_Pnt(0.5, 1e-6, 0), gp_Pnt(1, 0, 0), vm, v1);
  const TopoDS_Edge right1 = BRepBuilderAPI_MakeEdge(v1, v2).Edge();
  const TopoDS_Edge middle = BRepBuilderAPI_MakeEdge(v2, v3).Edge();
  const TopoDS_Edge left1  = BRepBuilderAPI_MakeEdge(v3, v0).Edge();
  const TopoDS_Edge right2 = BRepBuilderAPI_MakeEdge(v2, v4).Edge();
  const TopoDS_Edge top    = BRepBuilderAPI_MakeEdge(v4, v5).Edge();
  const TopoDS_Edge left2  = BRepBuilderAPI_MakeEdge(v5, v3).Edge();
  TopoDS_Shell      shell;
  builder.MakeShell(shell);
  builder.Add(shell, face_of({a, b, right1, middle, left1}));
  builder.Add(shell, face_of({middle.Reversed(), right2, top, left2}));
  return shell;
}

// The shell unified (edges, faces, B-spline chains); a null shape when the
// unification throws.
TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  try {
    OCC_CATCH_SIGNALS
    unify.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "unify: %s\n", failure.what());
    return TopoDS_Shape();
  }
  return unify.Shape();
}

int bspline_edges_of(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    if (BRepAdaptor_Curve(TopoDS::Edge(e.Current())).GetType() == GeomAbs_BSplineCurve) {
      ++count;
    }
  }
  return count;
}

// The pieces 2e-6 apart at a vertex of tolerance 1.5e-6: the faces and
// the sides merged, the bottom kept as its two pieces.
void test_chain_whose_curves_do_not_join_stays_as_its_edges() {
  const TopoDS_Shell shell = two_squares(1.5e-6);
  CHECK(port::count_of(shell, TopAbs_FACE) == 2);
  const TopoDS_Shape result = unified(shell);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_FACE) == 1);
  CHECK(port::count_of(result, TopAbs_EDGE) == 5);
  CHECK(bspline_edges_of(result) == 2);
  CHECK_NEAR(port::area_of(result), 2.0);
}

// The boundary: the same pieces at a vertex of tolerance 1e-5, within which
// their curves join: the chain is concatenated into one edge, as stock.
void test_chain_whose_curves_join_within_the_tolerance_is_merged() {
  const TopoDS_Shape result = unified(two_squares(1e-5));
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_FACE) == 1);
  CHECK(port::count_of(result, TopAbs_EDGE) == 4);
  CHECK(bspline_edges_of(result) == 1);
  CHECK_NEAR(port::area_of(result), 2.0);
}

} // namespace

MITCAD_PORT_TEST("0110", test_chain_whose_curves_do_not_join_stays_as_its_edges);
MITCAD_PORT_TEST("0110", test_chain_whose_curves_join_within_the_tolerance_is_merged);
