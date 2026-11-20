// SPDX-License-Identifier: MIT
// A torus corner whose two sides have different radii: a block standing on
// a plate, its vertical edge at one corner rounded 5 and the two foot
// edges there rounded 0.8 and 1. The corner is the torus of the larger
// radius round the rounding of 5 (ring 6, tube 1); the rounding of 1 meets
// it at its section, and past the section on the other side the torus runs
// on, cut by the block's wall, up to where it crosses the rounding of 0.8,
// which ends there. The material is the union of the blends' fills: the
// block rounded 5, the two foot roundings' fills along their edges, and
// the torus's fill round the rounding.
//
// The OCCT port took a torus only for equal radii and walked this corner
// with a radius running from 0.8 to 1: a free-form patch, the material
// 0.39 mm3 short. Patch 1860 of the OCCT port (third_party/vcpkg-ports,
// mitcad#2091). The same blends bound the complement (a pocket, convex
// foot edges round a concave vertical one), whose material is the box less
// the fills. A box's three convex edges are not of this kind (the torus's
// contact curves away from the smaller rounding's): the corner is walked
// as before, a valid body.

#include <algorithm>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kPivot = 5.0;   // the vertical edge's rounding
constexpr double kBlock = 20.0;  // the block's side and the foot edges' length
constexpr double kHeight = 10.0; // the block's height

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

TopoDS_Shape fuse(const TopoDS_Shape& a, const TopoDS_Shape& b) { return BRepAlgoAPI_Fuse(a, b).Shape(); }
TopoDS_Shape cut(const TopoDS_Shape& a, const TopoDS_Shape& b) { return BRepAlgoAPI_Cut(a, b).Shape(); }
TopoDS_Shape box(const gp_Pnt& a, const gp_Pnt& b) { return BRepPrimAPI_MakeBox(a, b).Shape(); }

// The straight edge of `shape` from a to b (either way), or a null edge.
TopoDS_Edge edge_between(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer x(shape, TopAbs_EDGE); x.More(); x.Next()) {
    const BRepAdaptor_Curve c(TopoDS::Edge(x.Current()));
    const gp_Pnt p = c.Value(c.FirstParameter()), q = c.Value(c.LastParameter());
    if ((p.Distance(a) < 1e-7 && q.Distance(b) < 1e-7) || (p.Distance(b) < 1e-7 && q.Distance(a) < 1e-7)) {
      return TopoDS::Edge(x.Current());
    }
  }
  return TopoDS_Edge();
}

// The plate x, y in [-20, 30], z in [-5, 0] with the block x, y in [0, 20]
// up to z = 10 on it.
TopoDS_Shape block_on_plate() {
  ShapeUpgrade_UnifySameDomain unify(fuse(box(gp_Pnt(-20, -20, -5), gp_Pnt(30, 30, 0)),
                                          box(gp_Pnt(0, 0, 0), gp_Pnt(kBlock, kBlock, kHeight))),
                                     true, true, false);
  unify.Build();
  return unify.Shape();
}

// The complement of the block over the plate: x, y in [-20, 30], z in
// [0, 20] less the block, a pocket open at z = 0.
TopoDS_Shape pocket() {
  return cut(box(gp_Pnt(-20, -20, 0), gp_Pnt(30, 30, 20)),
             box(gp_Pnt(0, 0, -1), gp_Pnt(kBlock, kBlock, kHeight)));
}

// The union of the fills of the block on the plate rounded 5 at its
// vertical edge at the origin and ra and rb at the foot edges along y and x
// there: the block rounded 5, each foot rounding's fill past the rounding
// of 5, and the fill of the torus of the larger foot radius round the
// rounding of 5, on the side of the corner (x or y below 5).
TopoDS_Shape fills_union(double ra, double rb) {
  const double rt = std::max(ra, rb);
  const gp_Pnt axis(kPivot, kPivot, 0);
  TopoDS_Shape block = cut(box(gp_Pnt(0, 0, 0), gp_Pnt(kBlock, kBlock, kHeight)),
                           box(gp_Pnt(-1, -1, -1), gp_Pnt(kPivot, kPivot, kHeight + 1)));
  block = fuse(block, BRepPrimAPI_MakeCylinder(gp_Ax2(axis, gp_Dir(0, 0, 1)), kPivot, kHeight).Shape());
  block = fuse(block, box(gp_Pnt(kPivot, 0, 0), gp_Pnt(kBlock, kBlock, kHeight)));
  block = fuse(block, box(gp_Pnt(0, kPivot, 0), gp_Pnt(kBlock, kBlock, kHeight)));
  const TopoDS_Shape fill_a =
    cut(box(gp_Pnt(-ra, kPivot, 0), gp_Pnt(0, kBlock, ra)),
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-ra, kPivot - 1, ra), gp_Dir(0, 1, 0)), ra, kBlock + 2).Shape());
  const TopoDS_Shape fill_b =
    cut(box(gp_Pnt(kPivot, -rb, 0), gp_Pnt(kBlock, 0, rb)),
        BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kPivot - 1, -rb, rb), gp_Dir(1, 0, 0)), rb, kBlock + 2).Shape());
  TopoDS_Shape ring = cut(BRepPrimAPI_MakeCylinder(gp_Ax2(axis, gp_Dir(0, 0, 1)), kPivot + rt, rt).Shape(),
                          BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kPivot, kPivot, -1), gp_Dir(0, 0, 1)), kPivot,
                                                   rt + 2).Shape());
  ring = cut(ring,
             BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(kPivot, kPivot, rt), gp_Dir(0, 0, 1)), kPivot + rt, rt)
               .Shape());
  ring = BRepAlgoAPI_Common(ring, fuse(box(gp_Pnt(-10, -10, -1), gp_Pnt(kPivot, 40, 5)),
                                       box(gp_Pnt(-10, -10, -1), gp_Pnt(40, kPivot, 5))))
           .Shape();
  return fuse(fuse(fuse(block, fill_a), fill_b), ring);
}

double fills_volume(double ra, double rb) { return volume_of(fills_union(ra, rb)); }

struct Rounded {
  bool done = false;
  TopoDS_Shape shape;
};

// `body` with its edge from the origin up to height rounded 5 and the
// edges from (0, 0, z) along y and x rounded ra and rb.
Rounded round_corner(const TopoDS_Shape& body, double height, double z, double ra, double rb) {
  Rounded result;
  const TopoDS_Edge pivot = edge_between(body, gp_Pnt(0, 0, 0), gp_Pnt(0, 0, height));
  const TopoDS_Edge along_y = edge_between(body, gp_Pnt(0, 0, z), gp_Pnt(0, kBlock, z));
  const TopoDS_Edge along_x = edge_between(body, gp_Pnt(0, 0, z), gp_Pnt(kBlock, 0, z));
  CHECK(!pivot.IsNull() && !along_y.IsNull() && !along_x.IsNull());
  if (pivot.IsNull() || along_y.IsNull() || along_x.IsNull()) {
    return result;
  }
  BRepFilletAPI_MakeFillet maker(body);
  maker.Add(kPivot, pivot);
  maker.Add(ra, along_y);
  maker.Add(rb, along_x);
  maker.Build();
  result.done = maker.IsDone();
  if (result.done) {
    result.shape = maker.Shape();
  }
  return result;
}

// The faces of `shape` whose surface is a torus of the given radii, and
// whether any face is a free-form surface.
int tori(const TopoDS_Shape& shape, double major, double minor, bool& free_form) {
  int count = 0;
  free_form = false;
  for (TopExp_Explorer x(shape, TopAbs_FACE); x.More(); x.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(x.Current()));
    if (surface.GetType() == GeomAbs_Torus && std::abs(surface.Torus().MajorRadius() - major) < 1e-9 &&
        std::abs(surface.Torus().MinorRadius() - minor) < 1e-9) {
      ++count;
    }
    free_form = free_form || surface.GetType() == GeomAbs_BSplineSurface ||
                surface.GetType() == GeomAbs_BezierSurface;
  }
  return count;
}

// Concave foot edges round the convex rounding of 5, either side the
// smaller; and radii a thousandth and two ten-thousandths apart (just over
// the fillet's 3D approximation tolerance, 1e-4).
void test_feet_round_a_rounded_block_edge() {
  const TopoDS_Shape plate = block_on_plate();
  const double plate_volume = 50.0 * 50.0 * 5.0;
  const struct {
    double ra, rb;
  } cases[] = {{0.8, 1.0}, {1.0, 0.8}, {0.5, 2.0}, {0.999, 1.0}, {0.9998, 1.0}};
  for (const auto& c : cases) {
    const Rounded rounded = round_corner(plate, kHeight, 0.0, c.ra, c.rb);
    CHECK(rounded.done);
    if (!rounded.done) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(rounded.shape).IsValid());
    const double got = volume_of(rounded.shape) - plate_volume;
    const double wanted = fills_volume(c.ra, c.rb);
    std::printf("feet %g and %g: material over the plate %.7f, %.7f wanted\n", c.ra, c.rb, got, wanted);
    CHECK(std::abs(got - wanted) < 1e-5);
    bool free_form = true;
    CHECK(tori(rounded.shape, kPivot + std::max(c.ra, c.rb), std::max(c.ra, c.rb), free_form) == 2);
    CHECK(!free_form);
  }
}

// Radii within the approximation tolerance apart (1e-4): not of this kind,
// the corner walked as before (a free-form face), a valid body.
void test_feet_within_the_tolerance_walked_as_before() {
  const Rounded rounded = round_corner(block_on_plate(), kHeight, 0.0, 0.9999, 1.0);
  CHECK(rounded.done);
  if (!rounded.done) {
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded.shape).IsValid());
  bool free_form = false;
  CHECK(tori(rounded.shape, kPivot + 1.0, 1.0, free_form) == 0);
  CHECK(free_form);
}

// The complement: convex foot edges of a pocket round its concave vertical
// edge rounded 5. Its material is the box less the fills.
void test_feet_round_a_rounded_pocket_edge() {
  const TopoDS_Shape body = pocket();
  const Rounded rounded = round_corner(body, kHeight, 0.0, 0.8, 1.0);
  CHECK(rounded.done);
  if (!rounded.done) {
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded.shape).IsValid());
  const double wanted = 50.0 * 50.0 * 20.0 - fills_volume(0.8, 1.0);
  const double got = volume_of(rounded.shape);
  std::printf("pocket: volume %.7f, %.7f wanted\n", got, wanted);
  CHECK(std::abs(got - wanted) < 1e-5);
  bool free_form = true;
  CHECK(tori(rounded.shape, kPivot + 1.0, 1.0, free_form) == 2);
  CHECK(!free_form);
}

// A box's corner of three convex edges, 5 up and 0.8 and 1 along the top:
// not of this kind, walked as before, a valid body.
void test_box_corner_walked_as_before() {
  const TopoDS_Shape body = box(gp_Pnt(0, 0, 0), gp_Pnt(kBlock, kBlock, kBlock));
  const Rounded rounded = round_corner(body, kBlock, kBlock, 0.8, 1.0);
  CHECK(rounded.done);
  if (rounded.done) {
    CHECK(BRepCheck_Analyzer(rounded.shape).IsValid());
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_feet_round_a_rounded_block_edge", test::test_feet_round_a_rounded_block_edge);
  test::guarded("test_feet_within_the_tolerance_walked_as_before",
                test::test_feet_within_the_tolerance_walked_as_before);
  test::guarded("test_feet_round_a_rounded_pocket_edge", test::test_feet_round_a_rounded_pocket_edge);
  test::guarded("test_box_corner_walked_as_before", test::test_box_corner_walked_as_before);
  if (test::failures == 0) {
    std::puts("test_fillet_unequal_torus_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
