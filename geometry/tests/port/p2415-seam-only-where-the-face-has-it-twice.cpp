// SPDX-License-Identifier: MIT
// Port patch 2415 (mitcad#2415, GitHub mitcad/Mitcad#642's second design):
// an edge is a seam of a face only where the face has it twice.
//
// A disk (a cylinder of radius 33, z 50..90) with an arm (z 60..80) whose
// flat sides are tangent to it, the hull of the disk and a circle of 15
// about (0, -82); the disk's seam on the line where one of the arm's sides
// meets it. The fuse splits the seam there, and the piece between the arm's
// top and bottom becomes the edge between the disk and the arm's side,
// keeping both curves of the seam on the cylinder though the disk's face
// has it once. Shelled inside with Sharp corners (the intersection join):
// - plain: the disk's top open: before, a valid solid four times too heavy
//   (147950 mm3 at 2 mm instead of 36384.7: the arm's side on the seam had
//   no inside and the cavity stopped short);
// - boss: a boss of 12 on the arm's round end, its top open, the disk's
//   rims rounded 3 mm, as the reported design has it: before, no result.
// The offset took that edge for a seam (BRep_Tool::IsClosed) where a seam
// meets a tangential edge (SeamRunsOnFromTangentEdge, patch 2050), and put
// the offset of their common vertex on the tangential edge's offset, which
// then was not trimmed where the arm's top and bottom cut it.
// Expected: one valid solid, of the volume the same body gives with the
// disk's seam turned to +x (off the tangent lines, within 1e-5); a point
// half the wall inside the arm's side on the seam, in the wall, is in the
// solid, and one the wall and 1 mm inside, in the cavity, not. At 2 and 1 mm.

#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax2.hxx>

#include "port_test.hpp"

namespace {

const double kSin = 18.0 / 82.0; // the arm's sides' slope: (33 - 15) / 82
const double kCos = std::sqrt(1.0 - kSin * kSin);

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.Build();
  return unify.Shape();
}

TopoDS_Shape fused(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  if (a.IsNull() || b.IsNull()) {
    return TopoDS_Shape();
  }
  BRepAlgoAPI_Fuse fuse(a, b);
  return fuse.IsDone() && !fuse.HasErrors() ? fuse.Shape() : TopoDS_Shape();
}

// The disk and the arm; with `seam` the disk's seam on the arm's side at
// -x (its X direction there), else at +x; with `boss` the boss on the
// arm's end and the disk's rims rounded 3 mm.
TopoDS_Shape body(bool seam, bool boss) {
  const gp_Dir x = seam ? gp_Dir(-kCos, -kSin, 0) : gp_Dir(1, 0, 0);
  const TopoDS_Shape disk = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 50), gp_Dir(0, 0, 1), x), 33, 40).Shape();
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(-33 * kCos, -33 * kSin, 60), gp_Pnt(-15 * kCos, -82 - 15 * kSin, 60),
                                     gp_Pnt(15 * kCos, -82 - 15 * kSin, 60), gp_Pnt(33 * kCos, -33 * kSin, 60), true);
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape quad = BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 20)).Shape();
  const TopoDS_Shape end = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -82, 60), gp_Dir(0, 0, 1)), 15, 20).Shape();
  TopoDS_Shape shape = fused(fused(disk, quad), end);
  if (boss) {
    shape = fused(shape, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, -82, 80), gp_Dir(0, 0, 1)), 12, 16).Shape());
  }
  if (shape.IsNull()) {
    return shape;
  }
  shape = unified(shape);
  if (boss) {
    // The disk's rims: the circles of 33 at z = 50 and z = 90.
    shape = port::rounded(shape,
                          port::edges_where(shape,
                                            [](const gp_Pnt& p) {
                                              return (std::abs(p.Z() - 50) < 1e-6 || std::abs(p.Z() - 90) < 1e-6) &&
                                                     std::abs(std::hypot(p.X(), p.Y()) - 33) < 1e-6;
                                            }),
                          3.0);
  }
  return shape;
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

// The body shelled `wall` inside with its flat face at `open_z` removed,
// Sharp: its solid when done, valid and one, else null.
TopoDS_Shape shelled(const TopoDS_Shape& shape, double open_z, double wall, const char* what) {
  const TopoDS_Shape face = flat_face_at(shape, open_z);
  CHECK(!face.IsNull());
  if (face.IsNull()) {
    return TopoDS_Shape();
  }
  NCollection_List<TopoDS_Shape> open;
  open.Append(face);
  BRepOffsetAPI_MakeThickSolid maker;
  try {
    OCC_CATCH_SIGNALS
    maker.MakeThickSolidByJoin(shape, open, -wall, 1e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection,
                               false);
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "%s %g: the thick solid raised '%s'\n", what, wall, failure.what());
    return TopoDS_Shape();
  }
  if (!maker.IsDone()) {
    std::fprintf(stderr, "%s %g: the thick solid not done: error %d\n", what, wall,
                 static_cast<int>(maker.MakeOffset().Error()));
    return TopoDS_Shape();
  }
  const TopoDS_Shape result = maker.Shape();
  if (!port::valid(result) || port::count_of(result, TopAbs_SOLID) != 1) {
    std::fprintf(stderr, "%s %g: not one valid solid\n", what, wall);
    return TopoDS_Shape();
  }
  return result;
}

TopAbs_State state_of(const TopoDS_Shape& solid, const gp_Pnt& p) {
  BRepClass3d_SolidClassifier classifier(solid, p, 1e-7);
  return classifier.State();
}

// The body with its seam on the tangent line and with it at +x shelled
// alike: the same volume; the wall behind the arm's side on the seam.
void check_seam_on_tangent_line(bool boss) {
  const char* what = boss ? "boss" : "plain";
  const double open_z = boss ? 96.0 : 90.0;
  const TopoDS_Shape on_seam = body(true, boss);
  const TopoDS_Shape off_seam = body(false, boss);
  CHECK(port::valid(on_seam));
  CHECK(port::valid(off_seam));
  for (const double wall : {2.0, 1.0}) {
    const TopoDS_Shape result = shelled(on_seam, open_z, wall, what);
    const TopoDS_Shape reference = shelled(off_seam, open_z, wall, "seam at +x");
    CHECK(!result.IsNull());
    CHECK(!reference.IsNull());
    if (result.IsNull() || reference.IsNull()) {
      continue;
    }
    CHECK(port::max_tolerance(result) <= 1e-3);
    const double v = port::volume_of(result);
    const double expected = port::volume_of(reference);
    if (std::abs(v - expected) > 1e-5 * expected) {
      std::fprintf(stderr, "%s %g: %.3f mm3, with the seam at +x %.3f\n", what, wall, v, expected);
    }
    CHECK(std::abs(v - expected) <= 1e-5 * expected);
    // The middle of the arm's side on the seam, at z = 70, and points
    // further in along its inward normal (kCos, kSin): in the wall, then
    // in the cavity.
    const gp_Pnt side(-24 * kCos, -41 - 24 * kSin, 70);
    const gp_Pnt in_wall(side.X() + 0.5 * wall * kCos, side.Y() + 0.5 * wall * kSin, 70);
    const gp_Pnt in_cavity(side.X() + (wall + 1) * kCos, side.Y() + (wall + 1) * kSin, 70);
    CHECK(state_of(result, in_wall) == TopAbs_IN);
    CHECK(state_of(result, in_cavity) == TopAbs_OUT);
  }
}

void test_seam_on_tangent_line_plain() {
  check_seam_on_tangent_line(false);
}

void test_seam_on_tangent_line_boss_and_rims() {
  check_seam_on_tangent_line(true);
}

} // namespace

MITCAD_PORT_TEST("2415", test_seam_on_tangent_line_plain);
MITCAD_PORT_TEST("2415", test_seam_on_tangent_line_boss_and_rims);
