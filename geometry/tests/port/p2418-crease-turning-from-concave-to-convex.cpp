// SPDX-License-Identifier: MIT
// Port patch 2418 (mitcad#2418, GitHub mitcad/Mitcad#641): the Sharp shell
// (the thick solid, intersection join) of a body whose creases turn from
// concave to convex inside their edges.
//
// The body: MitCad's loft from a circle of radius 170 at z = 315 (on the z
// axis) to one of 85 at z = 0 centred at x = 225 (an oblique cone, its side
// a B-spline parametrized by arc length), a cylinder of 170 from z = 315 to
// 375 and one of 85 under the foot down to z = -15, joined; that mirrored
// about x = 0 and joined to it (a three-way pipe). The rims where the cone
// meets the cylinders are concave on one side and convex on the other,
// tangent at 67.8 degrees (cos = 85 / 225) round them; the crease between
// the two cones runs from (0, 170, 315) down to the crotch and up to
// (0, -170, 315), tangent at both ends. Shelled 5 and 2 mm inside with the
// three end caps open, Sharp, as MitCad's Shell calls it (tolerance 1e-6):
// - the crotch sharp: before, not done at either thickness (the offsets of
//   a rim's faces cross in an X at the tangency; the pieces kept were the
//   crossing branch's, and the crease between the cones gave a piece of a
//   few microns);
// - the crotch rounded 20 by the kernel's fillet (the rounding vanishes to
//   a point at (0, +-170, 315), where the cylinder meets it at that point
//   only): before, not done at 5 mm.
// Expected: one valid solid of the volume measured when the test was
// written (within 1e-5; the OCCT lab's case 2418-pipe-shell, a body built in
// DRAW from the stored loft, gives 0.04 to 0.1 % other volumes),
// a point half the wall inside the cone's side (at z = 200, y = 0, along
// its normal) and one half the wall inside the top cylinder in the solid,
// points a millimetre past the wall in the cavity.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <gp_Ax2.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/profile.hpp"
#include "port_test.hpp"

namespace {

namespace g = mitcad::geometry;

constexpr double kHeight = 315.0;
constexpr double kTop = 170.0;
constexpr double kBottom = 85.0;
constexpr double kShift = 225.0;
constexpr double kUp = 60.0;
constexpr double kDown = 15.0;

// MitCad's loft (geometry/src/loft_skin.cpp): its side parametrized by arc
// length.
TopoDS_Shape mitcad_loft() {
  const auto section = [](double cx, double z, double radius, const char* name) {
    g::LoftSection s;
    s.frame.origin = gp_Pnt(0, 0, z);
    s.region = g::circle_region(cx, 0, radius);
    s.region.loops[0].segments[0].name = name;
    s.region.name = std::string("r{") + name + "}";
    return s;
  };
  g::LoftSpec spec;
  spec.feature = "F4";
  spec.sections = {section(0.0, kHeight, kTop, "c2"), section(kShift, 0.0, kBottom, "c1")};
  spec.merge_tangent = true;
  return g::loft(spec)->occt();
}

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
  return fuse.IsDone() && !fuse.HasErrors() ? unified(fuse.Shape()) : TopoDS_Shape();
}

// The pipe; with `rounded` its crotch rounded 20.
TopoDS_Shape pipe(bool rounded) {
  const TopoDS_Shape up =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, kHeight), gp_Dir(0, 0, 1)), kTop, kUp).Shape();
  const TopoDS_Shape down =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(kShift, 0, -kDown), gp_Dir(0, 0, 1)), kBottom, kDown).Shape();
  const TopoDS_Shape right = fused(fused(mitcad_loft(), up), down);
  if (right.IsNull()) {
    return right;
  }
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)));
  const TopoDS_Shape body = fused(right, BRepBuilderAPI_Transform(right, mirror, true).Shape());
  if (!rounded || body.IsNull()) {
    return body;
  }
  // The crease between the two cones, in the plane x = 0 below the top.
  return port::rounded(body, port::edges_where(body, [](const gp_Pnt& p) {
                         return std::abs(p.X()) < 1e-3 && p.Z() < kHeight - 1.0;
                       }),
                       20.0);
}

// The flat faces of the shape lying at height z.
NCollection_List<TopoDS_Shape> flat_faces_at(const TopoDS_Shape& shape, double z) {
  NCollection_List<TopoDS_Shape> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    Bnd_Box box;
    BRepBndLib::AddOptimal(it.Current(), box, false, false);
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    box.Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(z1 - z) < 1e-6 && std::abs(z2 - z) < 1e-6) {
      faces.Append(it.Current());
    }
  }
  return faces;
}

// The pipe shelled `wall` inside with its three end caps open, Sharp: its
// solid when done, valid and one, else null.
TopoDS_Shape shelled(const TopoDS_Shape& shape, double wall, const char* what) {
  NCollection_List<TopoDS_Shape> open = flat_faces_at(shape, kHeight + kUp);
  NCollection_List<TopoDS_Shape> feet = flat_faces_at(shape, -kDown);
  open.Append(feet);
  CHECK(open.Extent() == 3);
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

void check_pipe(bool rounded, double wall, double expected) {
  const char* what = rounded ? "the crotch rounded" : "the crotch sharp";
  const TopoDS_Shape body = pipe(rounded);
  CHECK(!body.IsNull() && port::valid(body));
  if (body.IsNull()) {
    return;
  }
  const TopoDS_Shape result = shelled(body, wall, what);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  const double v = port::volume_of(result);
  std::printf("%s %g: %.3f mm3, expected %.3f\n", what, wall, v, expected);
  CHECK(std::abs(v - expected) <= 1e-5 * expected);
  // The cone's side at z = 200, y = 0 (x = 221.11 on the right), its
  // outward normal (315, 140) / |(315, 140)| in the plane y = 0; and the top
  // cylinder at (0, 170, 345).
  const double n = std::hypot(315.0, 140.0);
  const double nx = 315.0 / n;
  const double nz = 140.0 / n;
  const double side = 310.0 - 140.0 * 200.0 / kHeight;
  for (const double sign : {1.0, -1.0}) {
    const gp_Pnt in_wall(sign * (side - 0.5 * wall * nx), 0, 200 - 0.5 * wall * nz);
    const gp_Pnt in_cavity(sign * (side - (wall + 1) * nx), 0, 200 - (wall + 1) * nz);
    CHECK(state_of(result, in_wall) == TopAbs_IN);
    CHECK(state_of(result, in_cavity) == TopAbs_OUT);
  }
  CHECK(state_of(result, gp_Pnt(0, kTop - 0.5 * wall, 345)) == TopAbs_IN);
  CHECK(state_of(result, gp_Pnt(0, kTop - wall - 1, 345)) == TopAbs_OUT);
}

// (The volumes measured with port patches 2418 and 2433.)
void test_pipe_crotch_sharp_5() {
  check_pipe(false, 5.0, 2522285.162);
}

void test_pipe_crotch_sharp_2() {
  check_pipe(false, 2.0, 1019549.133);
}

void test_pipe_crotch_rounded_5() {
  check_pipe(true, 5.0, 2512530.981);
}

} // namespace

MITCAD_PORT_TEST("2418", test_pipe_crotch_sharp_5);
MITCAD_PORT_TEST("2418", test_pipe_crotch_sharp_2);
MITCAD_PORT_TEST("2418", test_pipe_crotch_rounded_5);
