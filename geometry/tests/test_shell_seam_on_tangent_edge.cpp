// SPDX-License-Identifier: MIT
// MitCad's Shell with Sharp corners on a body whose cylinder has its seam on
// the line where a flat side meets it tangentially (mitcad#2415, GitHub
// mitcad/Mitcad#642's second design; OCCT port patch 2415, whose own test
// is geometry.port.2415).
//
// A disk (a cylinder of radius 33, z 50..90) with an arm (z 60..80) whose
// flat sides are tangent to it, the hull of the disk and a circle of 15
// about (0, -82), a boss of 12 on the arm's round end (z 80..96) and the
// disk's rims rounded 3 mm; the disk's seam on the line where the arm's side
// at -x meets it, as in the reported design. Shelled 2 and 1 mm inside with
// the boss top removed, Sharp: it failed ("the offset faces do not close
// up") at any thickness. Expected: the shell builds without notes, with the
// inside of every face's wall (<feature>:offset(<face>)), of the volume the
// same body gives with the disk's seam at +x (within 1e-5), the arm's side
// on the seam with its wall: a point half the wall in from it in the solid,
// one the wall and 1 mm in, in the cavity, not. At 5 mm (mitcad#2464), a
// wall thicker than the rims' rounding: one solid of the kernel's volume.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

const double kSin = 18.0 / 82.0; // the arm's sides' slope: (33 - 15) / 82
const double kCos = std::sqrt(1.0 - kSin * kSin);

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

// The body; with `seam` the disk's seam on the arm's side at -x, else at +x.
TopoDS_Shape body(bool seam) {
  const gp_Dir x = seam ? gp_Dir(-kCos, -kSin, 0) : gp_Dir(1, 0, 0);
  const TopoDS_Shape disk = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 50), gp_Dir(0, 0, 1), x), 33, 40).Shape();
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(-33 * kCos, -33 * kSin, 60), gp_Pnt(-15 * kCos, -82 - 15 * kSin, 60),
                                     gp_Pnt(15 * kCos, -82 - 15 * kSin, 60), gp_Pnt(33 * kCos, -33 * kSin, 60), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape quad = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 20)).Shape();
  const TopoDS_Shape end = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -82, 60), gp_Dir(0, 0, 1)), 15, 20).Shape();
  const TopoDS_Shape boss = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -82, 80), gp_Dir(0, 0, 1)), 12, 16).Shape();
  const TopoDS_Shape shape = unified(fused(fused(fused(disk, quad), end), boss));
  // The disk's rims (the circles of 33 at z = 50 and z = 90) rounded 3 mm.
  BRepFilletAPI_MakeFillet fillet(shape);
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(TopoDS::Edge(e.Current()), first, last);
    if (curve.IsNull()) {
      continue;
    }
    const gp_Pnt p = curve->Value(0.5 * (first + last));
    if ((std::abs(p.Z() - 50) < 1e-6 || std::abs(p.Z() - 90) < 1e-6) && std::abs(std::hypot(p.X(), p.Y()) - 33) < 1e-6) {
      fillet.Add(3.0, TopoDS::Edge(e.Current()));
    }
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  return fillet.IsDone() ? fillet.Shape() : shape;
}

ShapePtr named(const TopoDS_Shape& shape) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(shape, faces);
}

// The name of the boss's top (the flat face at z = 96).
std::string boss_top(const Shape& shape) {
  for (int i = 0; i < shape.face_count(); ++i) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(shape.face(i), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - 96) < 1e-6 && std::abs(z2 - 96) < 1e-6 && !shape.face_names(i).empty()) {
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

// The body shelled `wall` inside with the boss top removed, Sharp; null
// when it fails. Checks it has no notes and the inside of every kept
// face's wall.
ShapePtr shelled(const Shape& shape, double wall, const char* what) {
  ShellSpec spec;
  spec.faces = {boss_top(shape)};
  CHECK(!spec.faces[0].empty());
  spec.inside = wall;
  spec.rounded = false;
  spec.tangent_chain = false;
  ShapePtr result;
  try {
    result = shell("F18", shape, spec);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s %g: %s\n", what, wall, error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return nullptr;
  }
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "%s %g: note: %s\n", what, wall, note.c_str());
  }
  CHECK(result->notes().empty());
  for (int i = 0; i < shape.face_count(); ++i) {
    const std::string& name = shape.face_names(i).front();
    if (name == spec.faces[0]) {
      continue;
    }
    const bool inside = !result->find_faces("F18:offset(" + name + ")").empty();
    if (!inside) {
      std::fprintf(stderr, "%s %g: no inside of the wall along %s\n", what, wall, name.c_str());
    }
    CHECK(inside);
  }
  return result;
}

TopAbs_State state_of(const Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid.occt(), p, 1e-7);
  return classifier.State();
}

void test_seam_on_tangent_line() {
  const ShapePtr on_seam = named(body(true));
  const ShapePtr off_seam = named(body(false));
  for (const double wall : {2.0, 1.0}) {
    const ShapePtr result = shelled(*on_seam, wall, "seam on the tangent line");
    const ShapePtr reference = shelled(*off_seam, wall, "seam at +x");
    if (!result || !reference) {
      continue;
    }
    CHECK_NEAR_TOL(volume_of(result->occt()), volume_of(reference->occt()), 1e-5);
    // The middle of the arm's side on the seam at z = 70, and points further
    // in along its inward normal (kCos, kSin).
    const gp_Pnt side(-24 * kCos, -41 - 24 * kSin, 70);
    const gp_Pnt in_wall(side.X() + 0.5 * wall * kCos, side.Y() + 0.5 * wall * kSin, 70);
    const gp_Pnt in_cavity(side.X() + (wall + 1) * kCos, side.Y() + (wall + 1) * kSin, 70);
    CHECK(state_of(*result, in_wall) == TopAbs_IN);
    CHECK(state_of(*result, in_cavity) == TopAbs_OUT);
  }
}

// 5 mm (mitcad#2464, GitHub #642's follow-up): the wall thicker than the
// disk's 3 mm rims, whose roundings vanish in the offset (the insides of the
// faces beside them meet, the OCCT port's patch 0100). A release without
// patch 2415 refused it ("nothing can be hollowed"). Expected: the shell
// builds into one solid of the geometry kernel's 98489.8 mm3 (the OCCT lab's
// thick solid of this body, the seam on the tangent line or at +x alike),
// the arm's side on the seam with its wall; notes only for the rims'
// roundings, which have no inside of the wall.
void test_wall_thicker_than_the_rims() {
  const double wall = 5.0;
  for (const bool seam : {true, false}) {
    const char* what = seam ? "5 mm, seam on the tangent line" : "5 mm, seam at +x";
    const ShapePtr shape = named(body(seam));
    ShellSpec spec;
    spec.faces = {boss_top(*shape)};
    spec.inside = wall;
    spec.rounded = false;
    spec.tangent_chain = false;
    ShapePtr result;
    try {
      result = shell("F18", *shape, spec);
    } catch (const std::exception& error) {
      std::fprintf(stderr, "%s: %s\n", what, error.what());
    }
    CHECK(result != nullptr);
    if (!result) {
      continue;
    }
    for (const std::string& note : result->notes()) {
      std::fprintf(stderr, "%s: note: %s\n", what, note.c_str());
    }
    const double volume = volume_of(result->occt());
    if (std::abs(volume - 98489.8) > 0.2) {
      std::fprintf(stderr, "%s: %.3f mm3, expected 98489.8\n", what, volume);
    }
    CHECK(std::abs(volume - 98489.8) <= 0.2);
    const gp_Pnt side(-24 * kCos, -41 - 24 * kSin, 70);
    CHECK(state_of(*result, gp_Pnt(side.X() + 0.5 * wall * kCos, side.Y() + 0.5 * wall * kSin, 70)) == TopAbs_IN);
    CHECK(state_of(*result, gp_Pnt(side.X() + (wall + 1) * kCos, side.Y() + (wall + 1) * kSin, 70)) == TopAbs_OUT);
  }
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(shell_seam_on_tangent_edge) {
  test::guarded("test_seam_on_tangent_line", test::test_seam_on_tangent_line);
  test::guarded("test_wall_thicker_than_the_rims", test::test_wall_thicker_than_the_rims);
  if (test::failures == 0) {
    std::puts("test_shell_seam_on_tangent_edge: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
