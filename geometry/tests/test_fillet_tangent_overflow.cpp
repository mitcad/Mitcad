// SPDX-License-Identifier: MIT
// A rounding whose contact runs off a narrow face of its edge across that
// face's tangent edge with a rounded rim and comes back, built here with
// OCCT's fillet alone on a synthetic shape: the case behind the patch 0870
// of the OCCT port (third_party/vcpkg-ports, mitcad#343). The bodies stored
// in the files keep the ball on the narrow face's surface run on past the
// edge, and the rim cuts the rounding where it runs into it; OCCT rolled the
// ball on over the rim instead, lower and wider.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// A block 20 high on a plan from y = 0 (its front wall) back to a circular
// arc through (-20, 14), (0, 8) and (20, 14), 40 long; the top's edge with
// the arc's wall rounded with 6. The top's flat part, between the front edge
// and the rounding's tangent edge, is 2 wide in the middle and 8 at the
// ends.
constexpr double kHeight = 20.0;
constexpr double kRim    = 6.0;
// The front edge's rounding: its contact on the top, 3 from the front
// edge, runs off the top's flat part across the tangent edge for |x| up
// to 8.47 and comes back.
constexpr double kRadius = 3.0;

// The edge of the shape whose middle (by parameter) lies nearest to p.
TopoDS_Edge edge_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  TopoDS_Edge best;
  double      nearest = 1e100;
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    double             a = 0.0, b = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, a, b);
    if (curve.IsNull()) {
      continue;
    }
    const double distance = curve->Value(0.5 * (a + b)).Distance(p);
    if (distance < nearest) {
      nearest = distance;
      best    = edge;
    }
  }
  return best;
}

TopoDS_Shape rimmed_block() {
  const gp_Pnt    a(-20, 0, 0), b(20, 0, 0), c(20, 14, 0), d(-20, 14, 0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(b, c).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(c, gp_Pnt(0, 8, 0), d).Value()).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(d, a).Edge());
  const TopoDS_Face  plan  = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape block = BRepPrimAPI_MakePrism(plan, gp_Vec(0, 0, kHeight)).Shape();
  BRepFilletAPI_MakeFillet rim(block);
  rim.Add(kRim, edge_near(block, gp_Pnt(0, 8, kHeight)));
  rim.Build();
  return rim.IsDone() ? rim.Shape() : TopoDS_Shape();
}

// The front edge rounded with kRadius, its contact on the top's surface run
// on past the rim's tangent edge: each point of the rounding's faces lies
// kRadius from the line y = kRadius, z = kHeight - kRadius (the ball rests
// on the front wall and the top's plane). OCCT's ball rolled on over the
// rim, up to 0.17 lower in the middle, so its rounding lay off that line.
void test_rounding_kept_on_the_narrow_face_cut_by_the_rim() {
  const TopoDS_Shape block = rimmed_block();
  CHECK(!block.IsNull());
  if (block.IsNull()) {
    return;
  }
  const TopoDS_Edge front = edge_near(block, gp_Pnt(0, 0, kHeight));
  BRepFilletAPI_MakeFillet fillet(block);
  fillet.Add(kRadius, front);
  try {
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet: %s\n", failure.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape& result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // Each face of the rounding, sampled inside: its distance from the
  // ball's path, and where in x the samples ran.
  double worst = 0.0, lowest = 1e100, highest = -1e100;
  int    samples = 0;
  for (NCollection_List<TopoDS_Shape>::Iterator it(fillet.Generated(front)); it.More(); it.Next()) {
    const TopoDS_Face&  face = TopoDS::Face(it.Value());
    BRepAdaptor_Surface surface(face);
    double              u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const int n = 24;
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        const gp_Pnt2d           uv(u0 + (u1 - u0) * (i + 0.5) / n, v0 + (v1 - v0) * (j + 0.5) / n);
        BRepClass_FaceClassifier inside(face, uv, 1e-9);
        if (inside.State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = surface.Value(uv.X(), uv.Y());
        const double r = std::hypot(p.Y() - kRadius, p.Z() - (kHeight - kRadius));
        worst          = std::max(worst, std::abs(r - kRadius));
        lowest         = std::min(lowest, p.X());
        highest        = std::max(highest, p.X());
        ++samples;
      }
    }
  }
  std::printf("rounding off its ball's path: %.3g (%d samples, x %.2f to %.2f)\n", worst, samples, lowest, highest);
  CHECK(samples > 100);
  CHECK(lowest < -19.0 && highest > 19.0);
  CHECK(worst < 1e-3);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_tangent_overflow) {
  test::guarded("test_rounding_kept_on_the_narrow_face_cut_by_the_rim",
                test::test_rounding_kept_on_the_narrow_face_cut_by_the_rim);
  if (test::failures == 0) {
    std::puts("test_fillet_tangent_overflow: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
