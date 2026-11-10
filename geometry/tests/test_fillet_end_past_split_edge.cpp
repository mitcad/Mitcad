// SPDX-License-Identifier: MIT
// A fillet's end cut by the faces round a notch, crossing the straight
// boundary of two of them past a vertex that splits it, with OCCT's fillet
// alone (mitcad#1615, port patch 1670).
//
// A flange 2 thick (y from 0 to 2, x from -29 to 3, z from 0 to 60) with a
// web at x from 0 to 3 up to y = 40, and an end cap 2 thick (z from 0 to 2)
// between the web and x = -27, 2 short of the flange's tip: a notch 2 wide
// over the flange's end. The cap's inner corner with the flange is rounded
// with 5 first, so the boundary of the flange's inside (y = 2) and the
// cap's side (x = -27) is split at z = 2: the edge below from the cap, the
// one above from that rounding. Then the edge of the flange's foot with the
// end face (y = 0, z = 0) is rounded with r all along. Past the cap, over
// the notch, the end face is the flange's end, 2 wide, narrower than the
// rounding's contact (r from the edge); the rounding's end is cut by the
// cap's side, the flange's inside and its tip, and crosses the split
// boundary at z = r - sqrt(4 r - 4): on the edge below for r = 6.5, on the
// one above for r = 7 (the outer rounding then concentric with the inner,
// a wall 2 thick round the corner) and r = 8. Before 1670 the crossing past
// the lower edge's end was taken on that edge's prolongation, the upper
// edge left whole, and the result had a free edge (from r = 4 + 2 sqrt(2)
// on).
//
// Each result is one valid closed solid, every edge on two faces, its
// volume the body's less r^2 (1 - pi / 4) along the 30 of the edge beside
// the cap and the web and, over the notch's 2, the flange's part outside
// the rounding's circle: 2 r - (pi r^2 / 4 - (r - 2) / 2 sqrt(4 r - 4) -
// r^2 / 2 asin((r - 2) / r)) of its section, within 1e-6 mm3.

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kTip = -29.0;  // the flange's tip
constexpr double kCap = -27.0;  // the cap's side
constexpr double kWall = 2.0;   // the flange's and the cap's thickness
constexpr double kInner = 5.0;  // the inner corner's rounding

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

TopoDS_Shape box(double x0, double y0, double z0, double x1, double y1, double z1) {
  return BRepPrimAPI_MakeBox(gp_Pnt(x0, y0, z0), gp_Pnt(x1, y1, z1)).Shape();
}

// Whether the edge is a straight line along x at (y, z).
bool along_x_at(const TopoDS_Edge& edge, double y, double z) {
  const BRepAdaptor_Curve curve(edge);
  if (curve.GetType() != GeomAbs_Line) {
    return false;
  }
  const gp_Pnt a = curve.Value(curve.FirstParameter());
  const gp_Pnt b = curve.Value(curve.LastParameter());
  return std::abs(a.Y() - y) < 1e-7 && std::abs(b.Y() - y) < 1e-7 && std::abs(a.Z() - z) < 1e-7 &&
         std::abs(b.Z() - z) < 1e-7;
}

// The flange, the web and the cap, their inner corner rounded.
TopoDS_Shape body() {
  TopoDS_Shape shape = BRepAlgoAPI_Fuse(box(kTip, 0, 0, 3, kWall, 60), box(0, 0, 0, 3, 40, 60)).Shape();
  shape = BRepAlgoAPI_Fuse(shape, box(kCap, kWall, 0, 0, 40, kWall)).Shape();
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, true);
  unify.Build();
  shape = unify.Shape();
  BRepFilletAPI_MakeFillet inner(shape);
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    if (along_x_at(TopoDS::Edge(e.Current()), kWall, kWall)) {
      inner.Add(kInner, TopoDS::Edge(e.Current()));
    }
  }
  inner.Build();
  CHECK(inner.IsDone());
  return inner.IsDone() ? inner.Shape() : shape;
}

// One valid solid, every edge on two faces.
void check_closed(const TopoDS_Shape& shape) {
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
  }
  CHECK(solids == 1);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  int free_edges = 0;
  for (int i = 1; i <= faces.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) && faces(i).Extent() != 2) {
      ++free_edges;
    }
  }
  CHECK(free_edges == 0);
  if (free_edges != 0) {
    std::fprintf(stderr, "%d edges not on two faces\n", free_edges);
  }
}

// The flange's section outside the rounding's circle (centre (r, r)) over
// its thickness, y from 0 to 2.
double notch_loss(double r) {
  const double u = r - kWall;
  const double under = kPi * r * r / 4.0 - u / 2.0 * std::sqrt(r * r - u * u) - r * r / 2.0 * std::asin(u / r);
  return kWall * r - under;
}

void test_end_past_split_edge() {
  const TopoDS_Shape input = body();
  const double before = volume_of(input);
  for (const double r : {6.5, 7.0, 8.0}) {
    std::printf(" radius %.1f\n", r);
    try {
      OCC_CATCH_SIGNALS
      BRepFilletAPI_MakeFillet fillet(input);
      NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
      TopExp::MapShapes(input, TopAbs_EDGE, all);
      int edges = 0;
      for (int i = 1; i <= all.Extent(); ++i) {
        if (along_x_at(TopoDS::Edge(all(i)), 0.0, 0.0)) {
          fillet.Add(r, TopoDS::Edge(all(i)));
          ++edges;
        }
      }
      CHECK(edges == 1);
      fillet.Build();
      CHECK(fillet.IsDone());
      if (!fillet.IsDone()) {
        continue;
      }
      check_closed(fillet.Shape());
      const double expected = before - (3.0 - kCap) * r * r * (1.0 - kPi / 4.0) - (kCap - kTip) * notch_loss(r);
      const double got = volume_of(fillet.Shape());
      if (std::abs(got - expected) > 1e-6) {
        std::fprintf(stderr, "check failed: volume %.9f != %.9f\n", got, expected);
        ++failures;
      } else {
        std::printf("  volume %.9f (expected %.9f)\n", got, expected);
      }
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
      CHECK(false);
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("end_past_split_edge", test::test_end_past_split_edge);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("all fillet end past split edge checks passed\n");
  return 0;
}
