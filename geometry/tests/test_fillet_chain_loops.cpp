// SPDX-License-Identifier: MIT
// A fillet contour round a closed chain of edges that stops at a turn of
// the chain short of where it started, built here with OCCT's fillet alone
// on a synthetic shape and with Mitcad's fillet: the case behind the patch
// 0840 of the OCCT port (third_party/vcpkg-ports, mitcad#434). Without the
// patch, BRepFilletAPI_MakeFillet::Add never returns (no progress range
// reaches it, so a request to stop does not either); each check runs on a
// thread of its own and the test fails when it does not end in time.

#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <future>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <NCollection_Map.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

#include "../src/util.hpp"
#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using mitcad::geometry::FilletSet;
using mitcad::geometry::import_body;

constexpr double kHeight = 5.0;

// The outline of a plate 5 thick: a line from A (0, 0) to B (10, 0), a half
// circle of radius 5 about (10, 5) up to C (10, 10), a line from C to
// D (0, 10.4) and a half circle of radius 5.2 about (0, 5.2) back down to A.
// The outline runs on tangentially at A and B and turns by 0.04 rad at C
// and at D; OCCT takes the walls for tangent across the creases there (to
// 0.1 rad), so the top's four edges are one chain whose edges turn twice.
TopoDS_Shape creased_plate() {
  const gp_Pnt a(0, 0, 0);
  const gp_Pnt b(10, 0, 0);
  const gp_Pnt c(10, 10, 0);
  const gp_Pnt d(0, 10.4, 0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(b, gp_Pnt(15, 5, 0), c).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, d).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(d, gp_Pnt(-5.2, 5.2, 0), a).Value()).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kHeight)).Shape();
}

// The edge of the shape whose middle (by parameter) lies nearest to p.
TopoDS_Edge edge_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    const double distance = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p);
    if (distance < nearest) {
      nearest = distance;
      best = TopoDS::Edge(e.Current());
    }
  }
  return best;
}

// The middles of the top's edges: the line A-B, the arc B-C, the line C-D
// (the three rounded) and the arc D-A (not).
const gp_Pnt kBottomLine(5, 0, kHeight);
const gp_Pnt kRightArc(15, 5, kHeight);
const gp_Pnt kTopLine(5, 10.2, kHeight);
const gp_Pnt kLeftArc(-5.2, 5.2, kHeight);

// Runs `check` on a thread of its own; when it has not ended after
// `seconds`, the test fails at once (the thread cannot be stopped).
template <class F>
void within(const char* name, double seconds, F&& check) {
  std::future<void> done = std::async(std::launch::async, std::forward<F>(check));
  if (done.wait_for(std::chrono::duration<double>(seconds)) != std::future_status::ready) {
    std::fprintf(stderr, "%s: did not end within %.0f s\n", name, seconds);
    std::fflush(stderr);
    std::_Exit(1);
  }
  done.get();
}

// The top's line A-B rounded with 1, the line and arc after it selected for
// the turns too (StopChainsAtTurns, patch 0180), the arc D-A not. The
// contour runs from the line A-B downstream over the arc and the line C-D
// and stops at D, where the outline turns into the arc not selected. The
// upstream walk from A takes the arc D-A (no turn at A) and comes to D from
// the other side, where the edge beyond is the line C-D, selected: it went
// on over the contour's edges for ever. The contour now stops there, with
// every edge of the outline once.
void test_contour_round_a_closed_chain_stops_where_it_turns() {
  const TopoDS_Shape plate = creased_plate();
  const TopoDS_Edge bottom = edge_near(plate, kBottomLine);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  selected.Add(bottom);
  selected.Add(edge_near(plate, kRightArc));
  selected.Add(edge_near(plate, kTopLine));
  BRepFilletAPI_MakeFillet fillet(plate);
  fillet.StopChainsAtTurns(selected);
  within("contour round a closed chain", 30.0, [&] { fillet.Add(1.0, bottom); });
  CHECK(fillet.NbContours() == 1);
  CHECK(fillet.NbEdges(1) == 4);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> once;
  for (int i = 1; i <= fillet.NbEdges(1); ++i) {
    CHECK(once.Add(fillet.Edge(1, i)));
  }
  CHECK(once.Contains(edge_near(plate, kLeftArc)));
}

// Mitcad's fillet of the three edges (no tangent chain), whose contour
// starts from whichever of them comes first among the body's edges (each
// loops the same way without the patch): it ends, with a valid result or
// with an error.
void test_mitcad_fillet_round_a_closed_chain_ends() {
  const mitcad::geometry::ShapePtr body = import_body(creased_plate(), "F1", 0);
  const auto name_near = [&](const gp_Pnt& p) {
    const TopoDS_Edge edge = edge_near(body->occt(), p);
    for (int i = 0; i < body->edge_count(); ++i) {
      if (body->edge(i).IsSame(edge)) {
        return body->edge_name(i);
      }
    }
    return std::string();
  };
  const std::vector<std::string> names = {name_near(kBottomLine), name_near(kRightArc), name_near(kTopLine)};
  for (const std::string& name : names) {
    CHECK(!name.empty());
  }
  FilletSet set;
  set.edges = names;
  set.radius = 1.0;
  set.tangent_chain = false;
  within("Mitcad's fillet round a closed chain", 120.0, [&] {
    try {
      const mitcad::geometry::ShapePtr rounded = mitcad::geometry::fillet("F2", *body, std::vector<FilletSet>{set});
      CHECK(mitcad::geometry::detail::is_valid(rounded->occt()));
    } catch (const std::exception& error) {
      std::printf("fillet round a closed chain: %s\n", error.what());
    }
  });
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_chain_loops) {
  test::guarded("test_contour_round_a_closed_chain_stops_where_it_turns",
                test::test_contour_round_a_closed_chain_stops_where_it_turns);
  test::guarded("test_mitcad_fillet_round_a_closed_chain_ends", test::test_mitcad_fillet_round_a_closed_chain_ends);
  if (test::failures == 0) {
    std::puts("test_fillet_chain_loops: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
