// SPDX-License-Identifier: MIT
// Shell with Sharp corners of a cone standing on a ring with notches cut up
// from the ring's top through the cone's wall (mitcad#2383, GitHub
// mitcad/Mitcad#642: the reporter's design is a revolve with five such
// notches, a pattern of a cut extruded up from a sketch on the ring's top).
// The shell failed ("the offset faces do not close up") at any thickness,
// with no face vanishing: the geometry kernel's offset split the cone's
// foot, cut by the notches and by the cone's seam, wrongly (OCCT port patch
// 2383). Rounded corners built.
//
// The bodies are made with OCCT as in the port's test of the patch
// (geometry/tests/port/p2383-crease-split-in-several-edges.cpp), their faces
// named N1, N2, ..., and shelled by MitCad's shell 2 mm inside with the
// boss top (z = 92) and the foot (z = 0) open, Sharp:
// - a box notch whose floor is the ring's top, on the body with its axes
//   pointing up;
// - one and five notches of the reported shape (a round inner end of 9.5
//   and walls 13.4 degrees off its tangent).
// Expected: the shell builds without notes, every face kept has the inside
// of its wall, the wall is 2 mm behind each notch's round end (a point 1 mm
// in it inside the solid, one 4 mm in the cavity), and five notches add
// five times the wall one adds.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kWall = 2.0;
constexpr double kTop = 40.95; // the notches' tops (cut 32.95 up from the ring's top at 8)

const gp_Dir kDown(0, 0, -1);
const gp_Dir kX(1, 0, 0);

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.Build();
  return unify.Shape();
}

TopoDS_Shape fused(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Fuse fuse(a, b);
  CHECK(fuse.IsDone() && !fuse.HasErrors());
  return fuse.Shape();
}

TopoDS_Shape cut(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut cutter(a, b);
  CHECK(cutter.IsDone() && !cutter.HasErrors());
  return cutter.Shape();
}

// The ring (radius 59, z 0..8), the cone (52.7 at z = 8 to 23 at z = 80)
// and the step's boss (15 up to z = 92), fused: every axis pointing down
// as a revolve's do, or with `up` pointing up as OCCT's primitives come.
TopoDS_Shape body(bool up = false) {
  const gp_Dir axis = up ? gp_Dir(0, 0, 1) : kDown;
  const TopoDS_Shape ring =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, up ? 0 : 8), axis, kX), 59, 8).Shape();
  const TopoDS_Shape cone = up ? BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 8), axis, kX), 52.7, 23, 72).Shape()
                               : BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 80), axis, kX), 23, 52.7, 72).Shape();
  const TopoDS_Shape boss =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, up ? 80 : 92), axis, kX), 15, 12).Shape();
  return unified(fused(fused(ring, cone), boss));
}

// The reported notch turned `degrees` about the axis, from the ring's top
// (z = 8) up to kTop.
TopoDS_Shape round_notch(double degrees) {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(-42.29, 6.19, kTop), gp_Pnt(-66.56, 23.83, kTop),
                                     gp_Pnt(-66.56, -23.83, kTop), gp_Pnt(-42.29, -6.19, kTop), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape walls = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 8 - kTop)).Shape();
  const TopoDS_Shape end =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(-49.5, 0, kTop), kDown, kX), 9.5, kTop - 8).Shape();
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), degrees * kPi / 180.0);
  return BRepBuilderAPI_Transform(unified(fused(walls, end)), turn, true).Shape();
}

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& shape) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(shape, faces);
}

// The name of the body's flat face at height z (empty if none).
std::string flat_face_at(const Shape& shape, double z) {
  for (int i = 0; i < shape.face_count(); ++i) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(shape.face(i), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - z) < 1e-6 && std::abs(z2 - z) < 1e-6 && !shape.face_names(i).empty()) {
      return shape.face_names(i).front();
    }
  }
  return std::string();
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The body shelled 2 mm inside with the boss top and the foot open,
// Sharp, as Shell's F9; checked to have no notes and the inside of the
// wall of every face kept.
ShapePtr shelled(const Shape& shape, const char* what) {
  ShellSpec spec;
  spec.faces = {flat_face_at(shape, 92.0), flat_face_at(shape, 0.0)};
  CHECK(!spec.faces[0].empty() && !spec.faces[1].empty());
  spec.inside = kWall;
  spec.rounded = false;
  spec.tangent_chain = false;
  ShapePtr wall;
  try {
    wall = shell("F9", shape, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: %s\n", what, error.what());
  }
  CHECK(wall != nullptr);
  if (!wall) {
    return nullptr;
  }
  for (const std::string& note : wall->notes()) {
    std::fprintf(stderr, "%s: note: %s\n", what, note.c_str());
  }
  CHECK(wall->notes().empty());
  for (int i = 0; i < shape.face_count(); ++i) {
    const std::string& name = shape.face_names(i).front();
    if (name == spec.faces[0] || name == spec.faces[1]) {
      continue;
    }
    const bool inside = !wall->find_faces("F9:offset(" + name + ")").empty();
    if (!inside) {
      std::fprintf(stderr, "%s: no inside of the wall along %s\n", what, name.c_str());
    }
    CHECK(inside);
  }
  return wall;
}

TopAbs_State state_of(const Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid.occt(), p, 1e-7);
  return classifier.State();
}

// Behind the round (or flat) end of the notch turned `degrees`, at height
// 20: 1 mm in the wall, 4 mm in the cavity (the end's inner point is 40
// from the axis).
void check_wall_behind_notch(const Shape& wall, double degrees) {
  const double a = (180.0 + degrees) * kPi / 180.0;
  const gp_Pnt in_wall(39.0 * std::cos(a), 39.0 * std::sin(a), 20.0);
  const gp_Pnt in_cavity(36.0 * std::cos(a), 36.0 * std::sin(a), 20.0);
  CHECK(state_of(wall, in_wall) == TopAbs_IN);
  CHECK(state_of(wall, in_cavity) == TopAbs_OUT);
}

void test_box_notch() {
  // (The axes up: the cone's foot split by its seam this way failed before.)
  const TopoDS_Shape plain = body(true);
  const ShapePtr notched = named(unified(cut(plain, BRepPrimAPI_MakeBox(gp_Pnt(-70, -8, 8), 30, 16, kTop - 8).Shape())));
  const ShapePtr wall = shelled(*notched, "box notch");
  const ShapePtr wall_plain = shelled(*named(plain), "no notch");
  if (!wall || !wall_plain) {
    return;
  }
  check_wall_behind_notch(*wall, 0.0);
  CHECK(volume_of(wall->occt()) > volume_of(wall_plain->occt()));
}

void test_round_notches() {
  const TopoDS_Shape plain = body();
  TopoDS_Shape five = plain;
  for (int k = 0; k < 5; ++k) {
    five = cut(five, round_notch(72.0 * k));
  }
  const ShapePtr wall_plain = shelled(*named(plain), "no notch");
  const ShapePtr wall_one = shelled(*named(unified(cut(plain, round_notch(0.0)))), "one notch");
  const ShapePtr wall_five = shelled(*named(unified(five)), "five notches");
  if (!wall_plain || !wall_one || !wall_five) {
    return;
  }
  const double v0 = volume_of(wall_plain->occt());
  const double v1 = volume_of(wall_one->occt());
  CHECK_NEAR_TOL(volume_of(wall_five->occt()), v0 + 5.0 * (v1 - v0), 1e-5);
  check_wall_behind_notch(*wall_one, 0.0);
  for (int k = 0; k < 5; ++k) {
    check_wall_behind_notch(*wall_five, 72.0 * k);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_box_notch", test::test_box_notch);
  test::guarded("test_round_notches", test::test_round_notches);
  if (test::failures == 0) {
    std::puts("test_shell_notched_cone: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
