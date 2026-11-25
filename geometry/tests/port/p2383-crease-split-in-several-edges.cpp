// SPDX-License-Identifier: MIT
// Port patch 2383 (mitcad#2383, GitHub mitcad/Mitcad#642): the offsets of
// two faces meeting along a crease split into several edges, intersected
// once for all of them, are trimmed once and oriented in each face as
// their own edge is there.
//
// A cone (radius 52.7 at z = 8 to 23 at z = 80) standing on a ring (radius
// 59, z 0..8), with a step and a boss (radius 15 up to z = 92), every axis
// pointing down as a revolve's do (their seams at +x). Notches are cut from
// the ring's top up through the cone's wall, their floors in the ring's
// top: the cone's foot, the crease between the cone and the ring's top, is
// split by the notches into arcs and by the cone's seam once more. Shelled
// 2 mm inside with the boss top and the foot open, Sharp (intersection
// join):
// - a box notch, on the body with its axes pointing up: before, the second
//   piece of the foot's intersection, cut at its circle's origin, was
//   reversed in the cone's offset, which closed its foot loop alone; the
//   offset shell fell in two and the thick solid was not valid;
// - one notch of the reported shape (a round inner end, walls 13.4 degrees
//   off its tangent): the wall's offset meets the cone's along two branches
//   meeting at an apex, and a branch with one neighbour's vertex was not
//   trimmed;
// - five such notches 72 degrees apart: the arcs on both sides of the seam
//   share one piece, trimmed a second time the other way round the circle,
//   and the arc across a notch's gap was taken for one over the foot's
//   arcs; the cone's inside ran across a notch (a valid solid, about 3000
//   mm3 short) or the offset failed.
// Expected: one valid solid each time; the notches, rotations of each other
// that do not touch, each add the same wall, so five add five times what
// one adds (within 1e-5); a point 1 mm behind each notch's round end, in
// the wall, is inside the solid, and one 4 mm behind, in the cavity, not.

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "port_test.hpp"

namespace {

constexpr double kPi   = 3.14159265358979323846;
constexpr double kWall = 2.0;
constexpr double kTop  = 40.95; // the notches' tops (cut 32.95 up from the ring's top at 8)

const gp_Dir kDown(0, 0, -1);
const gp_Dir kX(1, 0, 0);

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.Build();
  return unify.Shape();
}

TopoDS_Shape fused(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Fuse fuse(a, b);
  return fuse.IsDone() && !fuse.HasErrors() ? fuse.Shape() : TopoDS_Shape();
}

TopoDS_Shape cut(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut cutter(a, b);
  return cutter.IsDone() && !cutter.HasErrors() ? cutter.Shape() : TopoDS_Shape();
}

// The ring, the cone and the boss, fused: their axes pointing down as a
// revolve's do, or with `up` pointing up as OCCT's primitives come.
TopoDS_Shape body(bool up = false) {
  const gp_Dir axis = up ? gp_Dir(0, 0, 1) : kDown;
  const TopoDS_Shape ring =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, up ? 0 : 8), axis, kX), 59, 8).Shape();
  const TopoDS_Shape cone = up ? BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 8), axis, kX), 52.7, 23, 72).Shape()
                               : BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 80), axis, kX), 23, 52.7, 72).Shape();
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, up ? 80 : 92), axis, kX), 15, 12).Shape();
  const TopoDS_Shape lower = fused(ring, cone);
  return lower.IsNull() ? TopoDS_Shape() : unified(fused(lower, boss));
}

// The reported notch turned `degrees` about the axis: a round inner end of
// 9.5 about x = -49.5 and walls leaving it 13.4 degrees off its tangent,
// from the ring's top (z = 8) up to kTop.
TopoDS_Shape round_notch(double degrees) {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(-42.29, 6.19, kTop), gp_Pnt(-66.56, 23.83, kTop),
                                     gp_Pnt(-66.56, -23.83, kTop), gp_Pnt(-42.29, -6.19, kTop), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape walls = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 8 - kTop)).Shape();
  const TopoDS_Shape end =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-49.5, 0, kTop), kDown, kX), 9.5, kTop - 8).Shape();
  const TopoDS_Shape notch = unified(fused(walls, end));
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), degrees * kPi / 180.0);
  return BRepBuilderAPI_Transform(notch, turn, true).Shape();
}

// The flat face of the shape lying at height z (null if none).
TopoDS_Shape flat_face_at(const TopoDS_Shape& shape, double z) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(faces.Current(), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - z) < 1e-6 && std::abs(z2 - z) < 1e-6) {
      return faces.Current();
    }
  }
  return TopoDS_Shape();
}

// The body shelled kWall inside with the boss top and the foot open,
// Sharp: its solid when done and valid, else null.
TopoDS_Shape shelled(const TopoDS_Shape& shape, const char* what) {
  NCollection_List<TopoDS_Shape> open;
  for (const double z : {92.0, 0.0}) {
    const TopoDS_Shape face = flat_face_at(shape, z);
    CHECK(!face.IsNull());
    if (face.IsNull()) {
      return TopoDS_Shape();
    }
    open.Append(face);
  }
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(shape, open, -kWall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s: the thick solid raised '%s'\n", what, failure.what());
    return TopoDS_Shape();
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s: the thick solid not done: error %d\n", what,
                 static_cast<int>(maker.MakeOffset().Error()));
    return TopoDS_Shape();
  }
  const TopoDS_Shape result = maker.Shape();
  if (!port::valid(result)) {
    std::fprintf(stderr, "%s: the thick solid not valid\n", what);
    return TopoDS_Shape();
  }
  if (port::count_of(result, TopAbs_SOLID) != 1) {
    std::fprintf(stderr, "%s: %d solids\n", what, port::count_of(result, TopAbs_SOLID));
    return TopoDS_Shape();
  }
  return result;
}

TopAbs_State state_of(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State();
}

// Behind the round end of the notch turned `degrees`, at height 20: 1 mm
// in the wall, 4 mm in the cavity (its end's inner point is 40 from the
// axis; the cone's inside there is further out than 36).
void check_wall_behind_notch(const TopoDS_Shape& wall, double degrees) {
  const double a = (180.0 + degrees) * kPi / 180.0;
  const gp_Dir out(std::cos(a), std::sin(a), 0);
  const gp_Pnt in_wall(39.0 * out.X(), 39.0 * out.Y(), 20.0);
  const gp_Pnt in_cavity(36.0 * out.X(), 36.0 * out.Y(), 20.0);
  const TopAbs_State wall_state = state_of(wall, in_wall);
  const TopAbs_State cavity_state = state_of(wall, in_cavity);
  if (wall_state != TopAbs_IN || cavity_state != TopAbs_OUT) {
    std::fprintf(stderr, "behind the notch at %g degrees: wall %d, cavity %d\n", degrees,
                 static_cast<int>(wall_state), static_cast<int>(cavity_state));
  }
  CHECK(wall_state == TopAbs_IN);
  CHECK(cavity_state == TopAbs_OUT);
}

void test_box_notch_floor_in_the_ring_top() {
  // (The axes up: the cone's foot split by its seam this way failed before;
  // with them down the box notch alone built.)
  const TopoDS_Shape plain = body(true);
  CHECK(port::valid(plain));
  const TopoDS_Shape notch = BRepPrimAPI_MakeBox(gp_Pnt(-70, -8, 8), 30, 16, kTop - 8).Shape();
  const TopoDS_Shape notched = unified(cut(plain, notch));
  CHECK(port::valid(notched));
  const TopoDS_Shape wall = shelled(notched, "box notch");
  CHECK(!wall.IsNull());
  if (wall.IsNull()) {
    return;
  }
  CHECK(port::max_tolerance(wall) <= 1e-3);
  check_wall_behind_notch(wall, 0.0);
  // The notch adds its walls to the wall of the body without it.
  const TopoDS_Shape wall_plain = shelled(plain, "no notch");
  CHECK(!wall_plain.IsNull());
  if (!wall_plain.IsNull()) {
    CHECK(port::volume_of(wall) > port::volume_of(wall_plain));
  }
}

void test_round_notches_one_and_five() {
  const TopoDS_Shape plain = body();
  const TopoDS_Shape wall_plain = shelled(plain, "no notch");
  CHECK(!wall_plain.IsNull());
  const TopoDS_Shape one = unified(cut(plain, round_notch(0.0)));
  const TopoDS_Shape wall_one = shelled(one, "one notch");
  CHECK(!wall_one.IsNull());
  TopoDS_Shape five = plain;
  for (int k = 0; k < 5 && !five.IsNull(); ++k) {
    five = cut(five, round_notch(72.0 * k));
  }
  CHECK(!five.IsNull());
  if (five.IsNull()) {
    return;
  }
  five = unified(five);
  CHECK(port::valid(five));
  const TopoDS_Shape wall_five = shelled(five, "five notches");
  CHECK(!wall_five.IsNull());
  if (wall_plain.IsNull() || wall_one.IsNull() || wall_five.IsNull()) {
    return;
  }
  CHECK(port::max_tolerance(wall_five) <= 1e-3);
  const double v0 = port::volume_of(wall_plain);
  const double v1 = port::volume_of(wall_one);
  const double v5 = port::volume_of(wall_five);
  const double expected = v0 + 5.0 * (v1 - v0);
  if (std::abs(v5 - expected) > 1e-5 * expected) {
    std::fprintf(stderr, "five notches: %.3f mm3, expected %.3f (none %.3f, one %.3f)\n", v5, expected, v0, v1);
  }
  CHECK(std::abs(v5 - expected) <= 1e-5 * expected);
  check_wall_behind_notch(wall_one, 0.0);
  for (int k = 0; k < 5; ++k) {
    check_wall_behind_notch(wall_five, 72.0 * k);
  }
}

} // namespace

MITCAD_PORT_TEST("2383", test_box_notch_floor_in_the_ring_top);
MITCAD_PORT_TEST("2383", test_round_notches_one_and_five);
