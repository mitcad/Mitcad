// SPDX-License-Identifier: MIT
// A box's vertical edge rounded up through the mitre where two earlier
// roundings of its top edges, of the same radius, meet (mitcad#1397), with
// OCCT's fillet alone.
//
// The box's two top edges at a corner are rounded with r first; their
// cylinders meet in a mitre (an arc of an ellipse) from the top of the
// vertical edge, to which it is tangent, up to the point where both
// cylinders touch the top. The vertical edge and the mitre, one tangent
// chain, are then rounded with r too: over the mitre the ball does not move
// (its centre lies on both cylinders' axes), and the corner becomes an eighth
// of the ball, the rounding's section vanishing at the top. The chain's
// spine runs from the bottom up or from the top down, by the way the first
// edge given runs. From the top, the walk over the mitre goes back to the
// spine's first end, and without the port's patch 1490 its step was a fifth
// of the guide's extension at that end: the contacts, which leave the
// cylinders' edges tangentially, stayed on those edges within the tolerance
// at every first step it tried, and the walk failed.
//
// Each way round, each corner: valid, and the material taken is the
// vertical edge's rounding below the corner and the mitre's corner less the
// ball's eighth.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"

namespace test {
namespace {

using ShapeIndex = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

constexpr double kLength = 50.0;
constexpr double kWidth  = 40.0;
constexpr double kHeight = 43.0;
constexpr double kRadius = 1.0;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

gp_Pnt middle(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// The edge of `shape` whose middle lies within 1e-3 of `at`; null if none.
TopoDS_Edge edge_at(const TopoDS_Shape& shape, const gp_Pnt& at) {
  ShapeIndex edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    if (middle(edge).Distance(at) < 1e-3) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

// The box with its top edges at the corner (x0, y0) rounded with kRadius:
// the one along x at y = y0 and the one along y at x = x0. `mirrored`: the
// box mirrored about its middle height, its vertical edges running down.
TopoDS_Shape mitred(double x0, double y0, bool mirrored) {
  TopoDS_Shape box = BRepPrimAPI_MakeBox(kLength, kWidth, kHeight).Shape();
  if (mirrored) {
    gp_Trsf mirror;
    mirror.SetMirror(gp_Ax2(gp_Pnt(0.0, 0.0, 0.5 * kHeight), gp_Dir(0.0, 0.0, 1.0)));
    box = BRepBuilderAPI_Transform(box, mirror, true).Shape();
  }
  BRepFilletAPI_MakeFillet fillet(box);
  fillet.Add(kRadius, edge_at(box, gp_Pnt(0.5 * kLength, y0, kHeight)));
  fillet.Add(kRadius, edge_at(box, gp_Pnt(x0, 0.5 * kWidth, kHeight)));
  fillet.Build();
  return fillet.IsDone() ? fillet.Shape() : TopoDS_Shape();
}

// The vertical edge at the corner (x0, y0) and the mitre above it rounded
// with kRadius, the chain given by each of its edges first, each way round:
// `expected` the material taken.
void round_corner(double x0, double y0, bool mirrored, double expected) {
  const TopoDS_Shape body = mitred(x0, y0, mirrored);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Edge vertical = edge_at(body, gp_Pnt(x0, y0, 0.5 * (kHeight - kRadius)));
  // The mitre's middle: the ellipse x - x0 = +-(1 - cos t), likewise y,
  // z = kHeight - r + r sin t at t = 45 degrees from its lower end.
  const double      sx    = x0 == 0.0 ? 1.0 : -1.0;
  const double      sy    = y0 == 0.0 ? 1.0 : -1.0;
  const double      c     = kRadius * (1.0 - std::cos(kPi / 4.0));
  const TopoDS_Edge mitre = edge_at(body, gp_Pnt(x0 + sx * c, y0 + sy * c, kHeight - kRadius + kRadius * std::sin(kPi / 4.0)));
  CHECK(!vertical.IsNull());
  CHECK(!mitre.IsNull());
  if (vertical.IsNull() || mitre.IsNull()) {
    return;
  }
  const double before = volume(body);
  for (const TopoDS_Edge& first :
       {vertical, TopoDS::Edge(vertical.Reversed()), mitre, TopoDS::Edge(mitre.Reversed())}) {
    try {
      BRepFilletAPI_MakeFillet fillet(body);
      fillet.Add(kRadius, first);
      fillet.Build();
      CHECK(fillet.IsDone());
      if (!fillet.IsDone()) {
        std::fprintf(stderr, "corner %g %g (mirrored %d): the rounding failed\n", x0, y0, mirrored ? 1 : 0);
        continue;
      }
      CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
      CHECK_NEAR_TOL(before - volume(fillet.Shape()), expected, 1e-4);
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "corner %g %g (mirrored %d): %s\n", x0, y0, mirrored ? 1 : 0, failure.what());
      CHECK(false);
    }
  }
}

// Each corner of the box and of the box mirrored (the spine from the bottom
// up or from the top down).
void test_vertical_edge_through_the_mitre() {
  // The material the rounding takes: the vertical edge's below the corner,
  // and at the corner the mitre's (a unit cube's corner within both
  // cylinders, 2/3 r^3) less the ball's eighth (pi/6 r^3).
  const double r3       = kRadius * kRadius * kRadius;
  const double expected = (kHeight - kRadius) * kRadius * kRadius * (1.0 - kPi / 4.0) + r3 * (2.0 / 3.0 - kPi / 6.0);
  for (const bool mirrored : {false, true}) {
    for (const double x0 : {0.0, kLength}) {
      for (const double y0 : {0.0, kWidth}) {
        round_corner(x0, y0, mirrored, expected);
      }
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_vertical_edge_through_the_mitre", test::test_vertical_edge_through_the_mitre);
  if (test::failures == 0) {
    std::puts("test_fillet_mitre_same_radius: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
