// SPDX-License-Identifier: MIT
// A rounding that vanishes at its edge's end vertex, where the edge's two
// faces and a third face across the next edge come tangent, built with
// OCCT's fillet alone (mitcad#343). A tower stands on a slab flush with
// its side A; the tower's corner next to A is rounded (a cylinder C
// tangent to A) and the foot of that side and of the corner is rounded
// concave (a torus B round C). A cuts B along an edge E whose two faces
// come tangent at its upper end V, where B is tangent to C and C to A.
// Rounding E (and the slab's top edge it runs on into tangentially), the
// blend narrows to nothing at V. On a stored design the faces there meet
// only within their edges' tolerance: the walk's contact on B reached E
// itself 0.04 short of V, and OCCT capped that end with a plate that
// failed. The port's patch 0961 ends such a blend in a point at V, its
// surface bent there with its contact curves; the synthetic bodies here
// shift B by a gap within the edges' tolerance to reproduce that.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kCorner = 0.4; // the tower's corner rounding (C)
constexpr double kFoot = 3.0;   // the tower's foot rounding (B)
constexpr double kRound = 0.3;  // the rounding of A's edge with B

using EdgeMap = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// Rounds the edges `pick` selects with radius r; a null shape where OCCT
// fails.
template <typename Pick> TopoDS_Shape round_edges(const TopoDS_Shape& body, double r, Pick&& pick) {
  BRepFilletAPI_MakeFillet fillet(body);
  EdgeMap edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  int added = 0;
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    if (!BRep_Tool::Degenerated(e) && pick(BRepAdaptor_Curve(e))) {
      fillet.Add(r, e);
      ++added;
    }
  }
  if (added == 0) {
    std::printf("no edge to round\n");
    return {};
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
    return {};
  }
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

gp_Pnt mid(const BRepAdaptor_Curve& c) { return c.Value(0.5 * (c.FirstParameter() + c.LastParameter())); }

// The slab x in [-4, 0], y in [-10, 10], z in [0, 5] and the tower x in
// [-4, 0], y in [-3, 3], z in [5, 15]; the tower's corner at x = 0,
// y = -3 rounded with kCorner, the foot of its side y = -3 and of that
// corner with kFoot.
TopoDS_Shape tower_on_slab() {
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-4, -10, 0), gp_Pnt(0, 10, 5)).Shape();
  const TopoDS_Shape tower = BRepPrimAPI_MakeBox(gp_Pnt(-4, -3, 5), gp_Pnt(0, 3, 15)).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(slab, tower).Shape(), true, true, false);
  unify.Build();
  const TopoDS_Shape corner = round_edges(unify.Shape(), kCorner, [](const BRepAdaptor_Curve& c) {
    const gp_Pnt m = mid(c);
    return c.GetType() == GeomAbs_Line && std::abs(m.X()) < 1e-9 && std::abs(m.Y() + 3) < 1e-9 && m.Z() > 5;
  });
  if (corner.IsNull()) {
    return {};
  }
  return round_edges(corner, kFoot, [](const BRepAdaptor_Curve& c) {
    const gp_Pnt m = mid(c);
    return c.GetType() == GeomAbs_Line && std::abs(m.Y() + 3) < 1e-9 && std::abs(m.Z() - 5) < 1e-9;
  });
}

// The torus B moved away from A by `gap` (into the material), its edges'
// and vertices' tolerances widened to cover it: B's face now meets A only
// within their tolerance, as faces stored in a design may.
void shift_torus(const TopoDS_Shape& body, double gap) {
  BRep_Builder builder;
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Torus) {
      continue;
    }
    // (Moved in place: the edges keep their curves on it, which belong to
    // that surface.)
    TopLoc_Location loc;
    BRep_Tool::Surface(face, loc)->Translate(gp_Vec(-gap, 0, 0));
    for (TopExp_Explorer ee(face, TopAbs_EDGE); ee.More(); ee.Next()) {
      builder.UpdateEdge(TopoDS::Edge(ee.Current()), 2.0 * gap);
    }
    for (TopExp_Explorer ev(face, TopAbs_VERTEX); ev.More(); ev.Next()) {
      builder.UpdateVertex(TopoDS::Vertex(ev.Current()), 2.0 * gap);
    }
  }
}

// Rounds E on the body with B moved by `gap`; the material taken, or -1
// where the rounding fails.
double check_gap(double gap) {
  const TopoDS_Shape body = tower_on_slab();
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return -1;
  }
  if (gap > 0) {
    shift_torus(body, gap);
  }
  std::printf("gap %g:", gap);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  // E: A's edge with the torus B, its upper end V where B touches C.
  const gp_Pnt V(0, -3 + kCorner, 5 + kFoot);
  const TopoDS_Shape rounded = round_edges(body, kRound, [&V](const BRepAdaptor_Curve& c) {
    const gp_Pnt a = c.Value(c.FirstParameter()), b = c.Value(c.LastParameter());
    return std::abs(a.X()) < 1e-6 && std::abs(b.X()) < 1e-6 && (a.Distance(V) < 1e-6 || b.Distance(V) < 1e-6)
           && c.GetType() != GeomAbs_Line;
  });
  CHECK(!rounded.IsNull());
  if (rounded.IsNull()) {
    std::printf(" failed\n");
    return -1;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  // The blend ends in a point at V, its vertices' tolerances far below the
  // stretch the walk left short of V (0.05 to 0.11 here), which an end
  // moved to V without its surface would need.
  bool atV = false;
  double tolerance = 0;
  for (TopExp_Explorer ex(rounded, TopAbs_VERTEX); ex.More(); ex.Next()) {
    const TopoDS_Vertex& v = TopoDS::Vertex(ex.Current());
    atV = atV || BRep_Tool::Pnt(v).Distance(V) < 1e-6;
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(v));
  }
  CHECK(atV);
  CHECK(tolerance < 0.01);
  const double loss = volume_of(body) - volume_of(rounded);
  std::printf(" material taken %.7f, largest vertex tolerance %.2g\n", loss, tolerance);
  return loss;
}

void test_rounding_vanishing_where_its_faces_come_tangent() {
  // (The exact faces: the walk reaches V. The others' blends lie within
  // their gap of it; what they take differs by the gap along the blend.)
  const double exact = check_gap(0.0);
  CHECK(exact > 0);
  for (const double gap : {0.0002, 0.0005, 0.001}) {
    const double loss = check_gap(gap);
    CHECK(loss > 0);
    CHECK(std::abs(loss - exact) < 0.005 * exact);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_vanishing_own_edge) {
  test::guarded("test_rounding_vanishing_where_its_faces_come_tangent",
                test::test_rounding_vanishing_where_its_faces_come_tangent);
  if (test::failures == 0) {
    std::puts("test_fillet_vanishing_own_edge: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
