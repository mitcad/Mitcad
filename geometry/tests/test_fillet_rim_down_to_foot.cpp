// SPDX-License-Identifier: MIT
// A boss's top rim rounded down to its foot's earlier rounding, where the
// foot has one, with OCCT's fillet alone (mitcad#1419).
//
// A cylindrical boss of radius 5 stands 2 high on a plate that covers only
// part of its foot: past the plate's side the boss's wall runs on down
// along the plate's side. The foot, an arc where the plate is, is rounded
// with rf first; the top rim, a whole circle, is then rounded with rt, and
// rf + rt = 2: the rim's contact on the wall, a circle at the height rf,
// runs along the foot rounding's edge on the wall where the plate is, and
// on inside the wall where it is not. The wall keeps nothing between the
// two roundings over that arc, and only the part past the plate's side
// below the rim's contact.
//
// Without the port's patch 1560 the wall kept a strip of no width along the
// foot rounding's edge (its wire held the whole contact circle and that
// edge, intersecting itself: not valid), and the closed contact's start on
// the wall's seam was cut at the parameter of the line hatched a little
// inside the wall (1e-4 off the contact), so that the wall's wire had a
// gap; the material taken was 5e-3 mm3 short of the rounding's. Now both
// roundings meet along that edge, the shape is valid, and the material
// taken is the rim rounding's all round within 1e-6 mm3.

#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

using ShapeIndex = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

constexpr double kBossRadius = 5.0;
constexpr double kBossHeight = 2.0;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9);
  return props.Mass();
}

// The circular edge of `shape` of radius kBossRadius whose middle lies at
// height z; null if none.
TopoDS_Edge circle_at(const TopoDS_Shape& shape, double z) {
  ShapeIndex edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() != GeomAbs_Circle || std::abs(curve.Circle().Radius() - kBossRadius) > 1e-7) {
      continue;
    }
    if (std::abs(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Z() - z) < 1e-7) {
      return TopoDS::Edge(edges(i));
    }
  }
  return TopoDS_Edge();
}

// The boss on a plate whose side is at x = side (inside the boss's foot),
// its foot rounded with `foot`; null if that failed.
TopoDS_Shape boss_with_rounded_foot(double side, double foot) {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20.0, -20.0, -3.0), gp_Pnt(side, 20.0, 0.0)).Shape();
  const TopoDS_Shape boss =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -3.0), gp::DZ()), kBossRadius, 3.0 + kBossHeight).Shape();
  BRepAlgoAPI_Fuse fuse(plate, boss);
  fuse.Build();
  if (!fuse.IsDone()) {
    return TopoDS_Shape();
  }
  ShapeUpgrade_UnifySameDomain unify(fuse.Shape());
  unify.Build();
  const TopoDS_Shape body = unify.Shape();
  const TopoDS_Edge  edge = circle_at(body, 0.0);
  if (edge.IsNull()) {
    return TopoDS_Shape();
  }
  BRepFilletAPI_MakeFillet fillet(body);
  fillet.Add(foot, edge);
  fillet.Build();
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// The material a rounding of radius r takes from a whole circular rim of
// radius kBossRadius between the boss's wall and its top: the region the
// ball leaves in the corner (r^2 (1 - pi/4)) turned about the axis at its
// centroid's radius.
double rim_rounding(double r) {
  const double centroid = r * (10.0 - 3.0 * kPi) / (12.0 - 3.0 * kPi);
  return 2.0 * kPi * (kBossRadius - centroid) * r * r * (1.0 - kPi / 4.0);
}

void round_rim(double side, double foot) {
  const TopoDS_Shape body = boss_with_rounded_foot(side, foot);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const double      rim  = kBossHeight - foot;
  const TopoDS_Edge edge = circle_at(body, kBossHeight);
  CHECK(!edge.IsNull());
  if (edge.IsNull()) {
    return;
  }
  try {
    BRepFilletAPI_MakeFillet fillet(body);
    fillet.Add(rim, edge);
    fillet.Build();
    CHECK(fillet.IsDone());
    if (!fillet.IsDone()) {
      std::fprintf(stderr, "side %g, foot %g: the rim's rounding failed\n", side, foot);
      return;
    }
    const bool   valid = BRepCheck_Analyzer(fillet.Shape()).IsValid();
    const double taken = volume(body) - volume(fillet.Shape());
    CHECK(valid);
    CHECK(std::abs(taken - rim_rounding(rim)) <= 1e-6);
    if (!valid || std::abs(taken - rim_rounding(rim)) > 1e-6) {
      std::fprintf(stderr, "side %g, foot %g: valid %d, taken %.9f, expected %.9f\n", side, foot, valid ? 1 : 0, taken,
                   rim_rounding(rim));
    }
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "side %g, foot %g: %s\n", side, foot, failure.what());
    CHECK(false);
  }
}

// The plate's side at two places, the foot rounded with two radii.
void test_rim_rounded_down_to_the_foot() {
  for (const double side : {2.0, -1.5}) {
    for (const double foot : {0.5, 0.8}) {
      round_rim(side, foot);
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rim_rounded_down_to_the_foot", test::test_rim_rounded_down_to_the_foot);
  if (test::failures == 0) {
    std::puts("test_fillet_rim_down_to_foot: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
