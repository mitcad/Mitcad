// SPDX-License-Identifier: MIT
// Where Fillet's and Chamfer's handles stand on an edge (mitcad#534;
// commands/ChamferHandles.hpp): the bevel's middle of an equal distance,
// the arrows along each face for a distance measured on it, at convex,
// concave and obtuse edges, as a placement moves them, and which face the
// model measures the first distance on; and the edges of a real L bracket
// read from its solid, an inside corner concave, the outside edges convex
// (mitcad#571).
#include "UnitTests.hpp"
#include "commands/ChamferHandles.hpp"

#include <cmath>
#include <cstddef>
#include <cstdio>
#include <optional>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Vec.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "chamfer_handles.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

constexpr double kTolerance = 1.0e-9;
constexpr double kPi = 3.14159265358979323846;

bool same(const gp_Dir& a, const gp_Dir& b) { return a.IsEqual(b, kTolerance); }
bool same(const gp_Pnt& a, const gp_Pnt& b) { return a.Distance(b) < kTolerance; }
bool same(double a, double b) { return std::abs(a - b) < kTolerance; }

// The knob of an arrow at a value (Manipulator: origin + value * factor).
gp_Pnt knob(const EdgeArrow& arrow, double value) {
  return arrow.origin.Translated(gp_Vec(arrow.direction) * (value * arrow.factor));
}

// The top front edge of a block: the top (+Z) its first face, the front
// (-Y) its second, along X at (30, 0, 20).
EdgeSides topFront() {
  EdgeSides sides;
  sides.middle = gp_Pnt(30, 0, 20);
  sides.tangent = gp_Dir(1, 0, 0);
  sides.normals = {gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)};
  sides.convex = true;
  return sides;
}

void convexTests() {
  const EdgeSides sides = topFront();
  CHECK(same(bendAngle(sides), kPi / 2));
  const auto toward = bendDirection(sides);
  CHECK(toward && same(*toward, gp_Dir(0, 1, -1)));
  // Equal distance 2: the bevel joins (30, 2, 20) and (30, 0, 18); its
  // middle is (30, 1, 19).
  const auto equal = equalChamferArrow(sides);
  CHECK(equal && same(equal->factor, std::sqrt(0.5)));
  CHECK(equal && same(knob(*equal, 2.0), gp_Pnt(30, 1, 19)));
  // On the top: back into it (+Y); on the front: down it (-Z).
  const auto top = faceDistanceArrow(sides, 0);
  CHECK(top && same(top->direction, gp_Dir(0, 1, 0)) && same(top->factor, 1.0));
  CHECK(top && same(knob(*top, 2.0), gp_Pnt(30, 2, 20)));
  const auto front = faceDistanceArrow(sides, 1);
  CHECK(front && same(front->direction, gp_Dir(0, 0, -1)));
  CHECK(!faceDistanceArrow(sides, 2));
  // A radius of 2: the rounding's axis at (30, 2, 18), its middle 2 from
  // it towards the edge, (sqrt 2 - 1) * 2 in from the edge.
  const auto fillet = filletArrow(sides);
  CHECK(fillet && same(fillet->direction, gp_Dir(0, 1, -1)));
  CHECK(fillet && same(fillet->factor, std::sqrt(2.0) - 1.0));
  CHECK(fillet && same(knob(*fillet, 2.0), gp_Pnt(30, 2 - std::sqrt(2.0), 18 + std::sqrt(2.0))));
  // The edge's direction either way: the same arrows.
  EdgeSides reversed = sides;
  reversed.tangent = gp_Dir(-1, 0, 0);
  const auto top2 = faceDistanceArrow(reversed, 0);
  const auto front2 = faceDistanceArrow(reversed, 1);
  CHECK(top2 && same(top2->direction, gp_Dir(0, 1, 0)));
  CHECK(front2 && same(front2->direction, gp_Dir(0, 0, -1)));
}

void concaveTests() {
  // An inside corner: a floor (+Z, running to +Y) meets a wall (+Y,
  // running up) along X; the material is under the floor and behind the
  // wall.
  EdgeSides sides;
  sides.middle = gp_Pnt(0, 0, 0);
  sides.tangent = gp_Dir(1, 0, 0);
  sides.normals = {gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)};
  sides.convex = false;
  const auto toward = bendDirection(sides);
  CHECK(toward && same(*toward, gp_Dir(0, 1, 1)));
  const auto onFloor = faceDistanceArrow(sides, 0);
  const auto onWall = faceDistanceArrow(sides, 1);
  CHECK(onFloor && same(onFloor->direction, gp_Dir(0, 1, 0)));
  CHECK(onWall && same(onWall->direction, gp_Dir(0, 0, 1)));
  const auto equal = equalChamferArrow(sides);
  CHECK(equal && same(knob(*equal, 2.0), gp_Pnt(0, 1, 1)));
  // A radius of 2: the rounding's axis at (0, 2, 2), its middle 2 nearer
  // the edge, out in the open corner.
  const auto fillet = filletArrow(sides);
  CHECK(fillet && same(fillet->direction, gp_Dir(0, 1, 1)));
  CHECK(fillet && same(knob(*fillet, 2.0), gp_Pnt(0, 2 - std::sqrt(2.0), 2 - std::sqrt(2.0))));
}

void obtuseTests() {
  // A top (+Z) and a face sloping down at 45 degrees towards -Y: the
  // equal distance's knob is the middle of the knobs on the two faces.
  EdgeSides sides;
  sides.middle = gp_Pnt(0, 0, 0);
  sides.tangent = gp_Dir(1, 0, 0);
  sides.normals = {gp_Dir(0, 0, 1), gp_Dir(0, -1, 1)};
  sides.convex = true;
  CHECK(same(bendAngle(sides), kPi / 4));
  const auto top = faceDistanceArrow(sides, 0);
  const auto slope = faceDistanceArrow(sides, 1);
  const auto equal = equalChamferArrow(sides);
  CHECK(top && same(top->direction, gp_Dir(0, 1, 0)));
  CHECK(slope && same(slope->direction, gp_Dir(0, -1, -1)));
  CHECK(equal && same(equal->factor, std::sin(kPi / 8)));
  if (top && slope && equal) {
    const gp_Pnt a = knob(*top, 3.0);
    const gp_Pnt b = knob(*slope, 3.0);
    CHECK(same(knob(*equal, 3.0), gp_Pnt((a.XYZ() + b.XYZ()) / 2)));
  }
}

void degenerateTests() {
  // Tangent faces: no bevel and no side to go to.
  EdgeSides tangent = topFront();
  tangent.normals = {gp_Dir(0, 0, 1), gp_Dir(0, 0, 1)};
  CHECK(!equalChamferArrow(tangent));
  CHECK(!faceDistanceArrow(tangent, 0));
  // Opposite normals (a knife edge): no bisector.
  EdgeSides knife = topFront();
  knife.normals = {gp_Dir(0, 0, 1), gp_Dir(0, 0, -1)};
  CHECK(!bendDirection(knife));
  CHECK(!equalChamferArrow(knife));
}

void placementTests() {
  // A component turned a quarter about Z and moved: the arrows turn and
  // move with it.
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), -kPi / 2);
  gp_Trsf move;
  move.SetTranslation(gp_Vec(100, 0, 0));
  const gp_Trsf placement = move * turn;
  const EdgeSides sides = placed(topFront(), placement);
  CHECK(same(sides.middle, gp_Pnt(100, -30, 20)));
  CHECK(sides.convex);
  const auto top = faceDistanceArrow(sides, 0);
  const auto front = faceDistanceArrow(sides, 1);
  const auto equal = equalChamferArrow(sides);
  CHECK(top && same(top->direction, gp_Dir(1, 0, 0)));
  CHECK(front && same(front->direction, gp_Dir(0, 0, -1)));
  CHECK(equal && same(knob(*equal, 2.0), gp_Pnt(101, -30, 19)));
}

// The L bracket of tools/ui-fillet-handle-test.sh: a floor 60 x 40 x 5 and
// a wall 60 x 5 x 40 at its back (y 35 to 40), fused into one solid.
TopoDS_Shape bracket() {
  const TopoDS_Shape base = BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), gp_Pnt(60, 40, 5)).Shape();
  const TopoDS_Shape upright = BRepPrimAPI_MakeBox(gp_Pnt(0, 35, 0), gp_Pnt(60, 40, 40)).Shape();
  BRepAlgoAPI_Fuse fuse(base, upright);
  if (!fuse.IsDone()) {
    return TopoDS_Shape();
  }
  TopExp_Explorer solids(fuse.Shape(), TopAbs_SOLID);
  return solids.More() ? solids.Current() : TopoDS_Shape();
}

// The edge of `solid` with the middle of its curve at `middle`, read as
// the handles read an edge (edgeSides), its faces in the solid's order.
std::optional<EdgeSides> edgeOf(const TopoDS_Shape& solid, const gp_Pnt& middle) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(solid, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(faces.FindKey(i));
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt at = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2);
    if (at.Distance(middle) > 1.0e-6 || faces(i).Size() != 2) {
      continue;
    }
    return edgeSides(solid, edge, TopoDS::Face(faces(i).First()), TopoDS::Face(faces(i).Last()));
  }
  return std::nullopt;
}

bool outside(const TopoDS_Shape& solid, const gp_Pnt& point) {
  BRepClass3d_SolidClassifier classifier(solid, point, 1.0e-7);
  return classifier.State() == TopAbs_OUT;
}

// Which of the edge's faces has the outward normal `normal` (-1: neither).
int faceWith(const EdgeSides& sides, const gp_Dir& normal) {
  for (int k = 0; k < 2; ++k) {
    if (same(sides.normals[static_cast<std::size_t>(k)], normal)) {
      return k;
    }
  }
  return -1;
}

void bracketTests() {
  const TopoDS_Shape solid = bracket();
  CHECK(!solid.IsNull());
  if (solid.IsNull()) {
    return;
  }
  // The inside corner between the floor's top (+Z) and the wall's front
  // (-Y), along X at (30, 35, 5): concave. Its arrows point into the open
  // corner and out along the faces, so pulling them that way makes the
  // rounding or the bevel larger (mitcad#571: the probe along the
  // normals' bisector took it for convex, and the arrows pointed into
  // the body).
  const auto inside = edgeOf(solid, gp_Pnt(30, 35, 5));
  CHECK(inside && !inside->convex);
  if (inside) {
    const auto fillet = filletArrow(*inside);
    CHECK(fillet && same(fillet->direction, gp_Dir(0, -1, 1)));
    // A radius of 2: the rounding's axis at (30, 33, 7), its middle in the
    // open corner, and farther out for a larger radius.
    CHECK(fillet && same(knob(*fillet, 2.0), gp_Pnt(30, 33 + std::sqrt(2.0), 7 - std::sqrt(2.0))));
    CHECK(fillet && outside(solid, knob(*fillet, 2.0)) && outside(solid, knob(*fillet, 5.0)));
    CHECK(fillet && knob(*fillet, 5.0).Distance(inside->middle) > knob(*fillet, 2.0).Distance(inside->middle));
    // Equal distance 2: the bevel joins (30, 33, 5) on the floor and
    // (30, 35, 7) on the wall; its middle is in the open corner.
    const auto equal = equalChamferArrow(*inside);
    CHECK(equal && same(equal->direction, gp_Dir(0, -1, 1)));
    CHECK(equal && same(knob(*equal, 2.0), gp_Pnt(30, 34, 6)));
    CHECK(equal && outside(solid, knob(*equal, 2.0)));
    // A distance on the floor: forward along it (-Y), its knob where the
    // bevel meets the floor; on the wall: up it (+Z). Whichever face the
    // solid has first.
    const int floorSide = faceWith(*inside, gp_Dir(0, 0, 1));
    const int wallSide = faceWith(*inside, gp_Dir(0, -1, 0));
    CHECK(floorSide >= 0 && wallSide >= 0 && floorSide != wallSide);
    if (floorSide >= 0 && wallSide >= 0) {
      const auto onFloor = faceDistanceArrow(*inside, floorSide);
      const auto onWall = faceDistanceArrow(*inside, wallSide);
      CHECK(onFloor && same(onFloor->direction, gp_Dir(0, -1, 0)));
      CHECK(onFloor && same(knob(*onFloor, 2.0), gp_Pnt(30, 33, 5)));
      CHECK(onWall && same(onWall->direction, gp_Dir(0, 0, 1)));
      CHECK(onWall && same(knob(*onWall, 2.0), gp_Pnt(30, 35, 7)));
      // Two Distances with the floor as the reference face: Distance on
      // the floor, Distance 2 up the wall; Flip swaps them. Distance and
      // Angle's distance as Distance.
      const auto measured = measuredSide(true, floorSide == 0, floorSide == 1, false);
      const auto flipped = measuredSide(true, floorSide == 0, floorSide == 1, true);
      CHECK(measured && *measured == floorSide);
      CHECK(flipped && *flipped == wallSide);
    }
  }
  // Convex edges keep their arrows into the material: the wall's top
  // front edge (the wall's front -Y, its top +Z) at (30, 35, 40) ...
  const auto wallTop = edgeOf(solid, gp_Pnt(30, 35, 40));
  CHECK(wallTop && wallTop->convex);
  if (wallTop) {
    const auto fillet = filletArrow(*wallTop);
    CHECK(fillet && same(fillet->direction, gp_Dir(0, 1, -1)));
    CHECK(fillet && !outside(solid, knob(*fillet, 2.0)));
    const int topSide = faceWith(*wallTop, gp_Dir(0, 0, 1));
    CHECK(topSide >= 0);
    const auto onTop = topSide >= 0 ? faceDistanceArrow(*wallTop, topSide) : std::nullopt;
    const auto onFront = topSide >= 0 ? faceDistanceArrow(*wallTop, 1 - topSide) : std::nullopt;
    CHECK(onTop && same(onTop->direction, gp_Dir(0, 1, 0)));
    CHECK(onFront && same(onFront->direction, gp_Dir(0, 0, -1)));
  }
  // ... and the floor's front edge at (30, 0, 5), whose faces have the
  // very normals of the inside corner's (+Z, -Y): only the solid tells
  // the two edges apart.
  const auto floorFront = edgeOf(solid, gp_Pnt(30, 0, 5));
  CHECK(floorFront && floorFront->convex);
  if (floorFront) {
    const auto equal = equalChamferArrow(*floorFront);
    CHECK(equal && same(equal->direction, gp_Dir(0, 1, -1)));
    CHECK(equal && !outside(solid, knob(*equal, 2.0)));
  }
  // The bracket placed (turned a quarter about Z, moved): classified in
  // its own coordinates, its inside corner's arrow turns with it.
  if (inside) {
    gp_Trsf turn;
    turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), kPi / 2);
    gp_Trsf move;
    move.SetTranslation(gp_Vec(0, 0, 10));
    const EdgeSides shown = placed(*inside, move * turn);
    CHECK(!shown.convex);
    CHECK(same(shown.middle, gp_Pnt(-35, 30, 15)));
    const auto fillet = filletArrow(shown);
    CHECK(fillet && same(fillet->direction, gp_Dir(1, 0, 1)));
  }
}

void measuredSideTests() {
  // Without a reference: the edge's first face, Flip the second.
  CHECK(measuredSide(false, false, false, false) == 0);
  CHECK(measuredSide(false, false, false, true) == 1);
  // The reference face's side, Flip the other.
  CHECK(measuredSide(true, true, false, false) == 0);
  CHECK(measuredSide(true, false, true, false) == 1);
  CHECK(measuredSide(true, true, false, true) == 1);
  CHECK(measuredSide(true, false, true, true) == 0);
  // A reference that does not border the edge, or on both sides.
  CHECK(!measuredSide(true, false, false, false));
  CHECK(!measuredSide(true, true, true, false));
}

} // namespace

int chamferHandleTests() {
  convexTests();
  concaveTests();
  obtuseTests();
  degenerateTests();
  placementTests();
  bracketTests();
  measuredSideTests();
  return failures;
}
MITCAD_UNIT_TEST(chamferHandleTests);
