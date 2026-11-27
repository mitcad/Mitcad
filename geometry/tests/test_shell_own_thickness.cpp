// SPDX-License-Identifier: MIT
// Shell with faces of their own wall thickness (mitcad#2443, GitHub
// mitcad/Mitcad#642's second request): the geometry kernel's offset takes a
// thickness for each face (BRepOffset_MakeOffset::SetOffsetOnFace).
//
// A 60 x 40 x 20 box, its faces named by where they lie (bottom, top,
// left x = 0, right x = 60, front y = 0, back y = 40), shelled with its top
// open and its bottom at a thickness of its own: Inside, Outside, Both,
// closed (no face removed) and Rounded inside; each one valid solid with the
// exact volume and the walls where they should be. Rounded outside, closed
// and Both (mitcad#2488): the rounding along the bottom's edges runs from
// the bottom's thickness to the sides' (OCCT port patch 2488), tangent to
// both outer faces (measured), the bottom's corners eighths of an
// ellipsoid. A cylinder's side at its own thickness, inside, outside and
// Rounded outside. A box with a rounded vertical edge: with the tangent
// chain the rounding and the face beyond it take the front face's
// thickness; without it the shell is refused (the kernel offsets tangent
// faces as one), and so are two tangent faces of different own
// thicknesses. Refused too: a rounded corner of walls of different
// thicknesses that are not square to each other (a wedge, Rounded
// outside), a removed face with its own thickness, and a bottom wall
// thicker than the box is high, whose error paints the bottom.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kW = 60.0, kD = 40.0, kH = 20.0;
constexpr double kBox = kW * kD * kH;

Bnd_Box bounds_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  return box;
}

// The faces of the shape named by where they lie in the box from (0, 0, 0)
// to (kW, kD, height): a face flat on one of its sides by that side, any
// other face (a rounding, a cylinder's side) "round", "round2", ...
ShapePtr named(const TopoDS_Shape& shape, double height = kH) {
  std::vector<Shape::NamedFace> faces;
  int rounds = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    bounds_of(f.Current()).Get(x1, y1, z1, x2, y2, z2);
    const auto at = [](double a, double b, double v) { return std::abs(a - v) < 1e-6 && std::abs(b - v) < 1e-6; };
    std::string name;
    if (at(z1, z2, 0.0)) {
      name = "bottom";
    } else if (at(z1, z2, height)) {
      name = "top";
    } else if (at(x1, x2, 0.0)) {
      name = "left";
    } else if (at(x1, x2, kW)) {
      name = "right";
    } else if (at(y1, y2, 0.0)) {
      name = "front";
    } else if (at(y1, y2, kD)) {
      name = "back";
    } else {
      name = ++rounds == 1 ? "round" : "round" + std::to_string(rounds);
    }
    faces.push_back({f.Current(), {name}});
  }
  return std::make_shared<Shape>(shape, faces);
}

ShapePtr box() { return named(BRepPrimAPI_MakeBox(kW, kD, kH).Shape()); }

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int solids(const TopoDS_Shape& shape) {
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
    ++count;
  }
  return count;
}

// The box of the faces of the shape named `name` exactly.
Bnd_Box face_bounds(const Shape& shape, const std::string& name) {
  Bnd_Box box;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& face : shape.face_names(i)) {
      if (face == name) {
        BRepBndLib::AddOptimal(shape.face(i), box, false, false);
      }
    }
  }
  return box;
}

// Where the flat faces named `name` lie along x (0), y (1) or z (2): their
// least and greatest coordinate, equal for a flat face across that axis.
void check_plane(const Shape& shape, const std::string& name, int axis, double value) {
  const Bnd_Box box = face_bounds(shape, name);
  CHECK(!box.IsVoid());
  if (box.IsVoid()) {
    std::fprintf(stderr, "no face %s\n", name.c_str());
    return;
  }
  double low[3] = {0, 0, 0};
  double high[3] = {0, 0, 0};
  box.Get(low[0], low[1], low[2], high[0], high[1], high[2]);
  CHECK_NEAR_TOL(low[axis] + 1.0, value + 1.0, 1e-7);
  CHECK_NEAR_TOL(high[axis] + 1.0, value + 1.0, 1e-7);
}

bool valid(const TopoDS_Shape& shape) { return BRepCheck_Analyzer(shape).IsValid(); }

// The normal of `face` out of the solid at the point of its edge `edge` at
// the edge's parameter `t`.
gp_Dir normal_at(const TopoDS_Face& face, const TopoDS_Edge& edge, double t) {
  double first = 0, last = 0;
  const Handle(Geom2d_Curve) pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  const gp_Pnt2d uv = pcurve->Value(t);
  gp_Pnt point;
  gp_Vec du, dv;
  BRepAdaptor_Surface(face).D1(uv.X(), uv.Y(), point, du, dv);
  gp_Dir normal(du.Crossed(dv));
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

// The roundings next to the flat face named `name` (its neighbours that are
// not flat; `count` of them), each tangent to it along their common edge
// (the normals of both measured at three points of the edge, within a
// microradian), together from z = `low` to `high`.
void check_roundings(const Shape& shape, const std::string& name, int count, double low, double high) {
  TopoDS_Face flat;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& face : shape.face_names(i)) {
      if (face == name) {
        flat = shape.face(i);
      }
    }
  }
  CHECK(!flat.IsNull());
  if (flat.IsNull()) {
    std::fprintf(stderr, "no face %s\n", name.c_str());
    return;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  int found = 0;
  double kink = 0.0;
  Bnd_Box rounds;
  for (TopExp_Explorer e(flat, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(e.Current());
    for (const TopoDS_Shape& other : faces_of.FindFromKey(edge)) {
      const TopoDS_Face round = TopoDS::Face(other);
      if (round.IsSame(flat) || BRepAdaptor_Surface(round).GetType() == GeomAbs_Plane) {
        continue;
      }
      ++found;
      double first = 0, last = 0;
      BRep_Tool::Range(edge, first, last);
      for (const double at : {0.25, 0.5, 0.75}) {
        const double t = first + at * (last - first);
        kink = std::max(kink, normal_at(flat, edge, t).Angle(normal_at(round, edge, t)));
      }
      rounds.Add(bounds_of(round));
    }
  }
  if (!rounds.IsVoid()) {
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    rounds.Get(x1, y1, z1, x2, y2, z2);
    CHECK_NEAR_TOL(z1 + 100.0, low + 100.0, 1e-7);
    CHECK_NEAR_TOL(z2 + 100.0, high + 100.0, 1e-7);
  }
  if (found != count || kink > 1e-6) {
    std::fprintf(stderr, "%s: %d roundings (expected %d), the largest kink %g rad\n", name.c_str(), found, count,
                 kink);
  }
  CHECK(found == count);
  CHECK(kink <= 1e-6);
}

// The shell, or null with its error printed.
ShapePtr shelled(const Shape& body, const ShellSpec& spec, const char* what) {
  try {
    const ShapePtr wall = shell("F9", body, spec);
    CHECK(solids(wall->occt()) == 1);
    return wall;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "%s: %s\n", what, error.what());
  }
  CHECK(false);
  return nullptr;
}

ShellSpec open_top(double inside, double outside) {
  ShellSpec spec;
  spec.faces = {"top"};
  spec.inside = inside;
  spec.outside = outside;
  return spec;
}

void test_box_bottom() {
  const ShapePtr body = box();

  // Inside 2, the bottom 5: the cavity 56 x 36 from z = 5 to the top.
  ShellSpec inside = open_top(2, 0);
  inside.own = {{"bottom", 5, 0}};
  if (const ShapePtr wall = shelled(*body, inside, "inside")) {
    CHECK_NEAR(volume_of(wall->occt()), kBox - 56.0 * 36.0 * 15.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 5.0);
    check_plane(*wall, "F9:offset(front)", 1, 2.0);
    check_plane(*wall, "F9:offset(right)", 0, 58.0);
    check_plane(*wall, "bottom", 2, 0.0);
  }

  // Outside 2, the bottom 5: the box grown 2 mm round its sides and 5 mm
  // down, less the box.
  ShellSpec outside = open_top(0, 2);
  outside.own = {{"bottom", 0, 5}};
  if (const ShapePtr wall = shelled(*body, outside, "outside")) {
    CHECK_NEAR(volume_of(wall->occt()), 64.0 * 44.0 * 25.0 - kBox);
    check_plane(*wall, "bottom", 2, -5.0);
    check_plane(*wall, "front", 1, -2.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 0.0);
  }

  // Both, 1 and 1, the bottom 3 inside and 2 outside: a 5 mm bottom.
  ShellSpec both = open_top(1, 1);
  both.own = {{"bottom", 3, 2}};
  if (const ShapePtr wall = shelled(*body, both, "both")) {
    CHECK_NEAR(volume_of(wall->occt()), 62.0 * 42.0 * 22.0 - 58.0 * 38.0 * 17.0);
    check_plane(*wall, "bottom", 2, -2.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 3.0);
    check_plane(*wall, "front", 1, -1.0);
    check_plane(*wall, "F9:offset(front)", 1, 1.0);
  }

  // Closed, inside 2, the bottom 5: a void 56 x 36 from z = 5 to z = 18.
  ShellSpec closed;
  closed.inside = 2;
  closed.own = {{"bottom", 5, 0}};
  if (const ShapePtr wall = shelled(*body, closed, "closed")) {
    CHECK_NEAR(volume_of(wall->occt()), kBox - 56.0 * 36.0 * 13.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 5.0);
    check_plane(*wall, "F9:offset(top)", 2, 18.0);
  }

  // Rounded inside: the box's edges are convex, so the inside of the wall
  // has no rounding, and the walls are as Sharp.
  ShellSpec rounded = inside;
  rounded.rounded = true;
  if (const ShapePtr wall = shelled(*body, rounded, "rounded inside")) {
    CHECK_NEAR(volume_of(wall->occt()), kBox - 56.0 * 36.0 * 15.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 5.0);
  }

  // Rounded outside (mitcad#2488): the rounding along the bottom's edges
  // runs from the 5 mm bottom to the 2 mm sides, a quarter ellipse 2 mm
  // across and 5 mm down, and the bottom's corners are eighths of an
  // ellipsoid 2, 2 and 5 mm; the vertical edges round 2 mm as before. The
  // walls 2 and 5 mm, the roundings tangent to the outer faces.
  ShellSpec rounded_out = outside;
  rounded_out.rounded = true;
  if (const ShapePtr wall = shelled(*body, rounded_out, "rounded outside")) {
    CHECK(valid(wall->occt()));
    CHECK_NEAR(volume_of(wall->occt()), 20000.0 + 1780.0 * kPi / 3.0);
    check_plane(*wall, "bottom", 2, -5.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 0.0);
    check_plane(*wall, "front", 1, -2.0);
    check_plane(*wall, "F9:offset(front)", 1, 0.0);
    check_plane(*wall, "right", 0, 62.0);
    check_plane(*wall, "F9:offset(right)", 0, kW);
    check_roundings(*wall, "bottom", 4, -5.0, 0.0);
    check_roundings(*wall, "front", 3, -5.0, kH);
  }

  // Rounded outside with no face removed: the whole box wrapped in its
  // offset, 2 mm round its sides and top, 5 mm under it.
  ShellSpec rounded_closed;
  rounded_closed.outside = 2;
  rounded_closed.rounded = true;
  rounded_closed.own = {{"bottom", 0, 5}};
  if (const ShapePtr wall = shelled(*body, rounded_closed, "rounded closed")) {
    CHECK(valid(wall->occt()));
    CHECK_NEAR(volume_of(wall->occt()), 24800.0 + 2396.0 * kPi / 3.0);
    check_plane(*wall, "bottom", 2, -5.0);
    check_plane(*wall, "top", 2, 22.0);
    check_roundings(*wall, "bottom", 4, -5.0, 0.0);
  }

  // Rounded both, 1 and 1, the bottom 3 inside and 2 outside: inside as
  // Sharp (the box's edges are convex), outside rounded from the 2 mm bottom
  // to the 1 mm sides.
  ShellSpec rounded_both = both;
  rounded_both.rounded = true;
  if (const ShapePtr wall = shelled(*body, rounded_both, "rounded both")) {
    CHECK(valid(wall->occt()));
    CHECK_NEAR(volume_of(wall->occt()), 19332.0 + 364.0 * kPi / 3.0);
    check_plane(*wall, "bottom", 2, -2.0);
    check_plane(*wall, "F9:offset(bottom)", 2, 3.0);
    check_plane(*wall, "front", 1, -1.0);
    check_plane(*wall, "F9:offset(front)", 1, 1.0);
    check_roundings(*wall, "bottom", 4, -2.0, 0.0);
  }

  // Refused: a removed face with its own thickness; a thickness on a side
  // the shell's wall has not.
  ShellSpec removed = inside;
  removed.own = {{"top", 5, 0}};
  CHECK(throws_with([&] { shell("F9", *body, removed); }, "a face the shell removes cannot have its own thickness"));
  ShellSpec wrong_side = inside;
  wrong_side.own = {{"bottom", 0, 5}};
  CHECK(throws_with([&] { shell("F9", *body, wrong_side); }, "must be on the sides of the shell's wall"));

  // The bottom 25 mm, thicker than the box is high: the walls cannot meet
  // it, the shell at its own thickness builds; the error paints the bottom.
  ShellSpec too_thick = inside;
  too_thick.own = {{"bottom", 25, 0}};
  CHECK(throws_with([&] { shell("F9", *body, too_thick); }, "cannot be built at its own thickness"));
  CHECK(throws_with([&] { shell("F9", *body, too_thick); }, "\"name\":\"bottom\""));
}

void test_cylinder_side() {
  // A cylinder of radius 20, 30 high, its top open, its side ("round") 4 mm.
  const ShapePtr body = named(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(30, 20, 0), gp::DZ()), 20, 30).Shape(), 30);
  const Shape& cylinder = *body;
  ShellSpec inside;
  inside.faces = {"top"};
  inside.inside = 2;
  inside.own = {{"round", 4, 0}};
  if (const ShapePtr wall = shelled(cylinder, inside, "cylinder inside")) {
    CHECK_NEAR(volume_of(wall->occt()), kPi * (20.0 * 20.0 * 30.0 - 16.0 * 16.0 * 28.0));
    check_plane(*wall, "F9:offset(bottom)", 2, 2.0);
  }
  ShellSpec outside;
  outside.faces = {"top"};
  outside.outside = 2;
  outside.own = {{"round", 0, 4}};
  if (const ShapePtr wall = shelled(cylinder, outside, "cylinder outside")) {
    CHECK_NEAR(volume_of(wall->occt()), kPi * (24.0 * 24.0 * 32.0 - 20.0 * 20.0 * 30.0));
    check_plane(*wall, "bottom", 2, -2.0);
  }
  // Rounded outside (mitcad#2488): the rounding along the bottom's circle
  // runs from the 2 mm bottom to the 4 mm side, a quarter ellipse 4 mm out
  // and 2 mm down revolved about the axis: by Pappus, its area 2 pi at
  // 20 + 16 / (3 pi) from the axis.
  ShellSpec rounded = outside;
  rounded.rounded = true;
  if (const ShapePtr wall = shelled(cylinder, rounded, "cylinder rounded outside")) {
    CHECK(valid(wall->occt()));
    CHECK_NEAR(volume_of(wall->occt()), 18304.0 * kPi / 3.0 + 80.0 * kPi * kPi);
    check_plane(*wall, "bottom", 2, -2.0);
    check_roundings(*wall, "bottom", 1, -2.0, 0.0);
  }
}

// A wedge: the box with its side x = 60 leaning in to x = 50 at the top,
// its top open, shelled Rounded outside 2 mm with its bottom at 5 mm. The
// corners at the foot of the leaning side are not square (the side leans
// on the bottom), and the kernel rounds a corner between walls of different
// thicknesses only where three walls meet square to each other: refused,
// the bottom and a face round such a corner painted.
void test_corner_not_square() {
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(0, 0, 0), gp_Pnt(kW, 0, 0), gp_Pnt(kW - 10, 0, kH), gp_Pnt(0, 0, kH),
                                     true);
  const TopoDS_Shape wedge =
      BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(outline.Wire(), true).Face(), gp_Vec(0, kD, 0)).Shape();
  const ShapePtr body = named(wedge);
  ShellSpec spec = open_top(0, 2);
  spec.rounded = true;
  spec.own = {{"bottom", 0, 5}};
  CHECK(throws_with([&] { shell("F9", *body, spec); }, "only where three walls meet square to each other"));
  CHECK(throws_with([&] { shell("F9", *body, spec); }, "\"name\":\"bottom\""));
  // Sharp, it builds.
  spec.rounded = false;
  shelled(*body, spec, "wedge sharp");
}

// The box with its vertical edge at x = 60, y = 0 rounded 10: front,
// round, right meet smoothly.
ShapePtr rounded_box() {
  BRepPrimAPI_MakeBox make(kW, kD, kH);
  BRepFilletAPI_MakeFillet round(make.Shape());
  for (TopExp_Explorer it(make.Shape(), TopAbs_EDGE); it.More(); it.Next()) {
    double x1 = 0, y1 = 0, z1 = 0, x2 = 0, y2 = 0, z2 = 0;
    bounds_of(it.Current()).Get(x1, y1, z1, x2, y2, z2);
    if (std::abs(x1 - kW) < 1e-6 && std::abs(x2 - kW) < 1e-6 && std::abs(y1) < 1e-6 && std::abs(y2) < 1e-6) {
      round.Add(10.0, TopoDS::Edge(it.Current()));
    }
  }
  return named(round.Shape());
}

void test_tangent_chain() {
  const ShapePtr body = rounded_box();
  // With the tangent chain the rounding and the right face take the front's
  // 3 mm: the cavity from x = 2 to 57 and y = 3 to 38, its corner rounded 7
  // about the rounding's axis, from z = 2 to the top.
  ShellSpec chain = open_top(2, 0);
  chain.own = {{"front", 3, 0}};
  if (const ShapePtr wall = shelled(*body, chain, "tangent chain")) {
    const double outer = kBox - (100.0 - 25.0 * kPi) * kH;
    const double cavity = (55.0 * 35.0 - (49.0 - 49.0 * kPi / 4.0)) * 18.0;
    CHECK_NEAR(volume_of(wall->occt()), outer - cavity);
    check_plane(*wall, "F9:offset(front)", 1, 3.0);
    check_plane(*wall, "F9:offset(right)", 0, 57.0);
    check_plane(*wall, "F9:offset(left)", 0, 2.0);
  }
  // Without it, refused: the kernel would offset the rounding with the front.
  ShellSpec alone = chain;
  alone.tangent_chain = false;
  CHECK(throws_with([&] { shell("F9", *body, alone); }, "turn on Tangent Chain"));
  // Two tangent faces of different own thicknesses, refused either way.
  ShellSpec two = chain;
  two.own = {{"front", 3, 0}, {"right", 4, 0}};
  CHECK(throws_with([&] { shell("F9", *body, two); }, "two faces that meet smoothly"));
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(shell_own_thickness) {
  test::guarded("test_box_bottom", test::test_box_bottom);
  test::guarded("test_cylinder_side", test::test_cylinder_side);
  test::guarded("test_tangent_chain", test::test_tangent_chain);
  test::guarded("test_corner_not_square", test::test_corner_not_square);
  if (test::failures == 0) {
    std::puts("test_shell_own_thickness: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
