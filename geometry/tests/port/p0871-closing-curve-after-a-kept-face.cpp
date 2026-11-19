// SPDX-License-Identifier: MIT
// Port patch 0871: where patch 0870 (KeepFaceAcrossTangentEdge) walks a
// run of a stripe's pieces again on one surface and the run holds the
// stripe's first or last piece, the stripe's curve at that end gets its
// parameter curve on the new surface. Before, it kept the one made on the
// replaced piece's surface: on a closed stripe the edge where the blend
// closes on itself lay up to the section's length off the new surface, its
// tolerance and its vertices' that wide (4.3 mm and 174 mm here, 4.79 mm
// on a reported nut design whose pieces could then not be closed).
//
// A stadium-shaped block, a through hole in its middle whose rim is rounded
// with 6, so that the top is a strip only 2 wide in front of the hole and
// behind it. The top's outer rim, one closed chain of tangent edges, is
// rounded with 3: the contact on the top crosses the rounded rim's tangent
// edge in front and behind and comes back (0870's runs). The chain's
// closed stripe starts where its first edge does: from the front edge, the
// front run holds the stripe's first pieces; from the right arc, the back
// run holds its last ones. Each: one valid solid, every tolerance within
// the walk's 3D tolerance (1e-4), the same volume either way. The
// boundary: a hole of radius 9, the strip 5 wide all round, the contact on
// the top all along (no run): as before. 0870's own case, a run inside
// an open stripe, is geometry/tests/test_fillet_tangent_overflow.cpp.

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kHalfLength = 20.0; // the straight sides' half length
constexpr double kDepth      = 40.0; // front (y = 0) to back (y = 40)
constexpr double kHeight     = 20.0;
constexpr double kRim        = 6.0;  // the hole's rim rounding
constexpr double kRadius     = 3.0;  // the top's outer rim rounding
// The walk's 3D tolerance (ChFi3d's tolapp3d), which a blend's edges keep.
constexpr double kWalkTolerance = 1e-4;

// The edge of the shape that passes nearest to p (its curve sampled).
TopoDS_Edge edge_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double      nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge&            edge  = TopoDS::Edge(e.Current());
    double                        first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (curve.IsNull()) {
      continue;
    }
    for (int k = 0; k <= 32; ++k) {
      const double d = curve->Value(first + (last - first) * k / 32.0).Distance(p);
      if (d < nearest) {
        nearest = d;
        best    = edge;
      }
    }
  }
  return best;
}

// The block with a hole of the radius about its middle, the hole's rim
// rounded; null where that fails.
TopoDS_Shape holed_block(double hole_radius) {
  const double            hole_y = kDepth / 2;
  const double            L = kHalfLength, D = kDepth;
  BRepBuilderAPI_MakeWire plan;
  plan.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(-L, 0, 0), gp_Pnt(L, 0, 0)).Edge());
  plan.Add(BRepBuilderAPI_MakeEdge(
             GC_MakeArcOfCircle(gp_Pnt(L, 0, 0), gp_Pnt(L + D / 2, D / 2, 0), gp_Pnt(L, D, 0)).Value())
             .Edge());
  plan.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(L, D, 0), gp_Pnt(-L, D, 0)).Edge());
  plan.Add(BRepBuilderAPI_MakeEdge(
             GC_MakeArcOfCircle(gp_Pnt(-L, D, 0), gp_Pnt(-L - D / 2, D / 2, 0), gp_Pnt(-L, 0, 0)).Value())
             .Edge());
  const TopoDS_Shape block =
    BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(plan.Wire(), true).Face(), gp_Vec(0, 0, kHeight)).Shape();
  const TopoDS_Shape hole =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, hole_y, -1), gp_Dir(0, 0, 1)), hole_radius, kHeight + 2).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(block, hole).Shape();
  return port::rounded(holed, {edge_near(holed, gp_Pnt(0, hole_y - hole_radius, kHeight))}, kRim);
}

// The top's outer rim rounded with kRadius, the chain given by its edge
// through `on`.
TopoDS_Shape rim_rounded(const TopoDS_Shape& block, const gp_Pnt& on) {
  return port::rounded(block, {edge_near(block, on)}, kRadius);
}

void check_rounded(const TopoDS_Shape& result, const char* what) {
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  std::printf("%s: valid %d, largest tolerance %.3g, volume %.6f\n", what, port::valid(result) ? 1 : 0,
              port::max_tolerance(result), port::volume_of(result));
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  CHECK(port::max_tolerance(result) <= kWalkTolerance);
}

// The front run holds the stripe's first pieces, the back run its last
// ones, whichever edge starts the chain; the same body either way.
void test_closed_stripe_closes_on_a_kept_face() {
  // (The strip 20 - 12 - 6 = 2 wide in front and behind, the contact 3 in.)
  const TopoDS_Shape block = holed_block(12.0);
  CHECK(!block.IsNull());
  if (block.IsNull()) {
    return;
  }
  const TopoDS_Shape from_front = rim_rounded(block, gp_Pnt(0, 0, kHeight));
  const TopoDS_Shape from_arc   = rim_rounded(block, gp_Pnt(kHalfLength + kDepth / 2, kDepth / 2, kHeight));
  check_rounded(from_front, "chain from the front edge");
  check_rounded(from_arc, "chain from the right arc");
  if (!from_front.IsNull() && !from_arc.IsNull()) {
    const double a = port::volume_of(from_front), b = port::volume_of(from_arc);
    CHECK(std::abs(a - b) <= 1e-6 * a);
  }
}

// The boundary: the strip 20 - 9 - 6 = 5 wide, the contact on the top all
// round: no run, the stripe as walked.
void test_closed_stripe_with_no_run() {
  const TopoDS_Shape block = holed_block(9.0);
  CHECK(!block.IsNull());
  if (block.IsNull()) {
    return;
  }
  check_rounded(rim_rounded(block, gp_Pnt(0, 0, kHeight)), "no run");
}

} // namespace

MITCAD_PORT_TEST("0871", test_closed_stripe_closes_on_a_kept_face);
MITCAD_PORT_TEST("0871", test_closed_stripe_with_no_run);
