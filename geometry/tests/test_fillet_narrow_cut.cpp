// SPDX-License-Identifier: MIT
// A rounding over a narrow face whose far edge is a concave crease, built
// with OCCT's fillet alone: a key standing in a spherical hollow, its top
// edge rounded with a radius larger than the key's side is high near the
// ends. The ball inside the key would run into the hollow's wall below
// the key's foot there; the rounding keeps to the side's plane, and the
// hollow's wall cuts the blend's side on the key's side, growing up to the
// rounding, and further on up to the key's top where the blend has run
// out. The material taken off is that of the corner cut by the rounding's
// cylinder, inside the hollow. A key in a bore of a stored design, its top
// edge rounded where the key's side runs out against the bore's wall, is
// the case behind the patch 0940 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343): there the ball rolled on the
// key's foot edge and the rounding failed at the key's ends. Here OCCT built
// the key whose side runs out before its ends without the patch as well;
// with end walls cutting the key off before that, its rounding came out
// invalid without the patch.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kRound  = 2.0;  // the rounding's radius
constexpr double kHollow = 15.0; // the hollow's radius

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The hollow: a ball of radius kHollow whose centre lies above the key's
// top (z = 0) and beyond its side (y = 0) at y = yc, so that the key's side
// is foot high at x = 0 and runs out towards both ends.
TopoDS_Shape hollow(double yc, double foot) {
  const double zc = -foot + std::sqrt(kHollow * kHollow - yc * yc);
  return BRepPrimAPI_MakeSphere(gp_Pnt(0, yc, zc), kHollow).Shape();
}

// A block (top z = 0) whose part at y > 0 inside the hollow is taken away:
// the key's top (z = 0, y < 0), its side (y = 0) and the hollow's wall
// beyond it.
TopoDS_Shape key_in_a_hollow(double yc, double foot, double half) {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-half, -6, -8), gp_Pnt(half, 6, 0)).Shape();
  const TopoDS_Shape beyond = BRepPrimAPI_MakeBox(gp_Pnt(-20, 0, -20), gp_Pnt(20, 20, 20)).Shape();
  const TopoDS_Shape pocket = BRepAlgoAPI_Common(hollow(yc, foot), beyond).Shape();
  return BRepAlgoAPI_Cut(block, pocket).Shape();
}

// The key's top edge: the line y = 0, z = 0.
std::vector<TopoDS_Edge> top_edge(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(body, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge edge = TopoDS::Edge(all(i));
    BRepAdaptor_Curve curve(edge);
    const gp_Pnt      a = curve.Value(curve.FirstParameter());
    const gp_Pnt      b = curve.Value(curve.LastParameter());
    if (curve.GetType() == GeomAbs_Line && std::abs(a.Y()) < 1e-6 && std::abs(b.Y()) < 1e-6
        && std::abs(a.Z()) < 1e-6 && std::abs(b.Z()) < 1e-6) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The material the rounding takes off: the corner of the key's top and
// side outside the rounding's cylinder, inside the hollow.
double removed_by_the_rounding(double yc, double foot, double half) {
  const TopoDS_Shape square =
    BRepPrimAPI_MakeBox(gp_Pnt(-half, -kRound, -kRound), gp_Pnt(half, 0, 0)).Shape();
  const TopoDS_Shape cylinder =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-half - 1, -kRound, -kRound), gp_Dir(1, 0, 0)), kRound, 2 * half + 2).Shape();
  const TopoDS_Shape corner = BRepAlgoAPI_Cut(square, cylinder).Shape();
  return volume_of(BRepAlgoAPI_Common(corner, hollow(yc, foot)).Shape());
}

int count_spheres(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType() == GeomAbs_Sphere) {
      ++n;
    }
  }
  return n;
}

void round_the_key(double yc, double foot, double half = 10.) {
  const TopoDS_Shape body = key_in_a_hollow(yc, foot, half);
  CHECK(BRepCheck_Analyzer(body).IsValid());
  const std::vector<TopoDS_Edge> edges = top_edge(body);
  CHECK(edges.size() == 1);
  if (edges.size() != 1) {
    return;
  }
  TopoDS_Shape rounded;
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(kRound, edges.front());
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      return;
    }
    rounded = fillet.Shape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
    CHECK(false);
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded).IsValid());
  CHECK_NEAR_TOL(volume_of(body) - volume_of(rounded), removed_by_the_rounding(yc, foot, half), 1e-6);
  // (The hollow's wall is still one face.)
  CHECK(count_spheres(rounded) == count_spheres(body));
}

// The side 2.2 high in the middle, the ball reaching its foot within 2.4 of
// the middle; the hollow's wall reaches the rounding's contact on the top
// before the key's ends.
void test_key_rounded_over_its_side_in_a_hollow() { round_the_key(5.0, 2.2); }

// The side 2.05 high in the middle, the hollow's centre further off.
void test_key_rounded_over_a_lower_side() { round_the_key(6.0, 2.05); }

// The key cut off by end walls before its side runs out, the hollow's wall
// reaching the rounding's contact on the top before them.
void test_key_rounded_up_to_end_walls() { round_the_key(5.0, 2.2, 6.5); }

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_narrow_cut) {
  test::guarded("test_key_rounded_over_its_side_in_a_hollow", test::test_key_rounded_over_its_side_in_a_hollow);
  test::guarded("test_key_rounded_over_a_lower_side", test::test_key_rounded_over_a_lower_side);
  test::guarded("test_key_rounded_up_to_end_walls", test::test_key_rounded_up_to_end_walls);
  if (test::failures == 0) {
    std::puts("test_fillet_narrow_cut: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
