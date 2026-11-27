// SPDX-License-Identifier: MIT
// Port patch 2542 (mitcad#2542, GitHub mitcad/Mitcad#720): grooves that end
// inside a flat face, shelled inside with the intersection join (Sharp).
//
// A box 120 x 96 x 60 (x -60..60, y -48..48), on each long side a groove of
// radius 5 cut into the side (axis at x = 0 on the side's plane, vertical,
// from below the bottom up to z = 32), its seams on the far side of the axis
// (as MitCad's revolve of half a slot makes them); the ends x = +-60 open,
// 2 mm inside:
// - the groove ending in a flat end at z = 32. The side and the groove's wall
//   meet along two edges (x = +-5); the pair was intersected once, with the
//   first, and the intersection keeps the branch along it, so the second
//   edge got the first's line: the groove's offset closed no loop and was
//   left out, the side's offset ran straight through the groove (a valid
//   solid 3158.7 mm3 short, the walls round the grooves missing).
// - the groove ending in a quarter sphere, its pole on the side's edge,
//   which splits the edge where the side meets the sphere in two: each half
//   got a whole circle of its own as its image, nothing cut the circles at
//   the pole, and the side's offset was a disc (an invalid shape,
//   BRepCheck_BadOrientationOfSubshape on a face, GitHub #720's report).
// - the report's body: a disk of radius 72 cut flat at y = +-48, a channel of
//   radius 5 along y at the bottom, the ball-ended grooves, the top's long
//   edges rounded 12, the round ends open (before: invalid as above).
// Expected: one valid solid, the body less the cavity of the points 2 mm or
// more inside every kept face (hand computed for the boxes; for the report's
// body the closed offset 2 mm inside with the ends' offsets 0).

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

constexpr double kGroove = 5.0;
constexpr double kWall   = 2.0;
constexpr double kTop    = 32.0;

enum class GrooveEnd { Flat, Ball };

// The groove's tool at the side y: a cylinder from z = -10 up to kTop, and
// a sphere there for a ball end; the seams on the far side of the axis.
TopoDS_Shape groove(double y, GrooveEnd end) {
  const gp_Dir away(0, y < 0 ? -1 : 1, 0);
  TopoDS_Shape tool = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, y, -10), gp_Dir(0, 0, 1), away), kGroove,
                                               kTop + 10)
                        .Shape();
  if (end == GrooveEnd::Ball) {
    tool = BRepAlgoAPI_Fuse(tool,
                            BRepPrimAPI_MakeSphere(gp_Ax2(gp_Pnt(0, y, kTop), gp_Dir(0, 0, 1), away), kGroove)
                              .Shape())
             .Shape();
  }
  return tool;
}

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.Build();
  return unify.Shape();
}

TopoDS_Shape with_grooves(TopoDS_Shape body, GrooveEnd end) {
  for (const double y : {-48.0, 48.0}) {
    body = BRepAlgoAPI_Cut(body, groove(y, end)).Shape();
  }
  return unified(body);
}

// The box with its grooves.
TopoDS_Shape grooved_box(GrooveEnd end) {
  return with_grooves(BRepPrimAPI_MakeBox(gp_Pnt(-60, -48, 0), 120, 96, 60).Shape(), end);
}

// The report's body: the disk cut flat, the channel, the ball-ended grooves,
// the top's long edges rounded 12.
TopoDS_Shape report_body() {
  const TopoDS_Shape disk = BRepPrimAPI_MakeCylinder(72, 60).Shape();
  TopoDS_Shape body = BRepAlgoAPI_Common(disk, BRepPrimAPI_MakeBox(gp_Pnt(-80, -48, 0), 160, 96, 60).Shape()).Shape();
  body = BRepAlgoAPI_Cut(body, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 100, 0), gp_Dir(0, -1, 0), gp_Dir(1, 0, 0)),
                                                        kGroove, 200)
                                 .Shape())
           .Shape();
  body = with_grooves(body, GrooveEnd::Ball);
  return port::rounded(body, port::edges_where(body, [](const gp_Pnt& p) {
                         return std::abs(p.Z() - 60) < 1e-6 && std::abs(std::abs(p.Y()) - 48) < 1e-6;
                       }),
                       12.0);
}

// The faces of the shape beyond |x| = 50: the open ends.
NCollection_List<TopoDS_Shape> ends_of(const TopoDS_Shape& shape) {
  NCollection_List<TopoDS_Shape> ends;
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.Current(), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (x2 < -50 || x1 > 50) {
      ends.Append(faces.Current());
    }
  }
  return ends;
}

// The thick solid of the shape with its ends open, kWall inside, Sharp;
// null when it fails.
TopoDS_Shape shelled(const TopoDS_Shape& shape, const char* body) {
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(shape, ends_of(shape), -kWall, 1e-7, BRepOffset_Skin, false, false,
                               GeomAbs_Intersection, false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: the thick solid raised '%s'\n", body, failure.what());
    return TopoDS_Shape();
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s: the thick solid not done: error %d\n", body,
                 static_cast<int>(maker.MakeOffset().Error()));
    return TopoDS_Shape();
  }
  return maker.Shape();
}

void check_wall(const TopoDS_Shape& shape, double expected, double tolerance, const char* body) {
  const TopoDS_Shape result = shelled(shape, body);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(port::count_of(result, TopAbs_SOLID) == 1);
  const double v = port::volume_of(result);
  if (std::abs(v - expected) > tolerance * expected) {
    std::fprintf(stderr, "%s: %.4f mm3, expected %.4f\n", body, v, expected);
  }
  CHECK(std::abs(v - expected) <= tolerance * expected);
}

const double kPi = std::acos(-1.0);

// The cavity of the box without grooves: 120 x 92 x 56.
constexpr double kBoxCavity = 120.0 * (96 - 2 * kWall) * (60 - 2 * kWall);

// The cross-section the offset of a groove (radius 7) takes off the cavity
// beyond the side's offset (2 mm in): a circular segment.
double groove_segment() {
  const double r = kGroove + kWall;
  return r * r * std::acos(kWall / r) - kWall * std::sqrt(r * r - kWall * kWall);
}

void test_box_flat_ended_groove() {
  const TopoDS_Shape shape = grooved_box(GrooveEnd::Flat);
  CHECK(!shape.IsNull() && port::valid(shape));
  // The grooves' offsets reach from the bottom's offset (z = 2) to the flat
  // end's (z = kTop + 2).
  const double cavity = kBoxCavity - 2 * groove_segment() * kTop;
  check_wall(shape, port::volume_of(shape) - cavity, 1e-5, "the box, flat-ended grooves");
}

void test_box_ball_ended_groove() {
  const TopoDS_Shape shape = grooved_box(GrooveEnd::Ball);
  CHECK(!shape.IsNull() && port::valid(shape));
  // Up to z = kTop the grooves' cylinders' offsets; above, their spheres'
  // (radius 7): half the cap beyond the side's offset (its height 7 - 2).
  const double r      = kGroove + kWall;
  const double h      = r - kWall;
  const double cap    = kPi * h * h * (3 * r - h) / 3;
  const double cavity = kBoxCavity - 2 * (groove_segment() * (kTop - kWall) + cap / 2);
  check_wall(shape, port::volume_of(shape) - cavity, 1e-5, "the box, ball-ended grooves");
}

void test_report_body() {
  const TopoDS_Shape shape = report_body();
  CHECK(!shape.IsNull());
  if (shape.IsNull()) {
    return;
  }
  CHECK(port::valid(shape));
  // The cavity: the body offset 2 mm inside, closed, its ends kept where
  // they are (their offsets 0).
  BRepOffset_MakeOffset offset;
  offset.Initialize(shape, -kWall, 1e-7, BRepOffset_Skin, false, false, GeomAbs_Intersection);
  for (const TopoDS_Shape& end : ends_of(shape)) {
    offset.SetOffsetOnFace(TopoDS::Face(end), 0.0);
  }
  try {
    OCC_CATCH_SIGNALS
    offset.MakeOffsetShape();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "the report's body: the cavity raised '%s'\n", failure.what());
  }
  CHECK(offset.IsDone());
  if (!offset.IsDone()) {
    return;
  }
  check_wall(shape, port::volume_of(shape) - port::volume_of(offset.Shape()), 1e-5, "the report's body");
}

} // namespace

MITCAD_PORT_TEST("2542", test_box_flat_ended_groove);
MITCAD_PORT_TEST("2542", test_box_ball_ended_groove);
MITCAD_PORT_TEST("2542", test_report_body);
