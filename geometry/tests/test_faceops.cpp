// SPDX-License-Identifier: MIT
// Shell, draft, offset, delete and replace on named faces: volumes of the
// T0 models and analytic cases, the names of the new faces, and failures.

#include <algorithm>
#include <cmath>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Precision.hxx>
#include <gp_Ax3.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// Sketch1's 60 x 40 rectangle (c1..c4) extruded 20 mm by F2: side 0 is
// y = 0, side 1 x = 60, side 2 y = 40, side 3 x = 0.
ShapePtr block() { return extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20); }

const double kBlock = 60.0 * 40.0 * 20.0;
const double kDraftWedge = 20.0 * 20.0 * std::tan(5.0 * kPi / 180.0) / 2.0 * 60.0;

gp_Pln plane(double x, double y, double z, double nx, double ny, double nz) {
  return gp_Pln(gp_Pnt(x, y, z), gp_Dir(nx, ny, nz));
}

void test_shell() {
  ShellSpec open;
  open.faces = {end_cap("F2", 1)};
  open.inside = 2;
  // T0 shell_inside_open_top.
  const ShapePtr box = shell("F5", *block(), open);
  CHECK_NEAR(volume(*box), kBlock - 56.0 * 36.0 * 18.0);
  // The outside keeps the names, the inside is offset(<face>), the rim where
  // the top was offset_cap(<top>).
  CHECK(faces_named(*box, side("F2", 1, 0)) == 1);
  CHECK(faces_named(*box, "F5:offset(" + side("F2", 1, 0) + ")") == 1);
  CHECK(faces_named(*box, "F5:offset(" + start_cap("F2", 1) + ")") == 1);
  CHECK(faces_named(*box, end_cap("F2", 1)) == 0);
  CHECK(faces_named(*box, "F5:offset_cap(" + end_cap("F2", 1) + ")") == 1);
  CHECK(box->face_count() == 11);
  CHECK(edge_names_unique(*box));

  // T0 shell_outside_open_top: the block becomes the cavity.
  ShellSpec outside = open;
  outside.inside = 0;
  outside.outside = 2;
  const ShapePtr grown = shell("F5", *block(), outside);
  CHECK_NEAR(volume(*grown), 64.0 * 44.0 * 22.0 - kBlock);
  CHECK(faces_named(*grown, side("F2", 1, 1)) == 1);
  CHECK(faces_named(*grown, "F5:offset(" + side("F2", 1, 1) + ")") == 1);
  const BoundingBox bounds = bounding_box(*grown);
  CHECK(near(bounds.min.X(), -2) && near(bounds.max.Z(), 20) && near(bounds.min.Z(), -2));

  // Both sides: the wall spans -1 .. +1 around the surface, the rim stays at z = 20.
  ShellSpec both = open;
  both.inside = 1;
  both.outside = 1;
  const ShapePtr wall = shell("F5", *block(), both);
  CHECK_NEAR(volume(*wall), 62.0 * 42.0 * 21.0 - 58.0 * 38.0 * 19.0);
  CHECK(faces_named(*wall, side("F2", 1, 2)) == 1);
  CHECK(faces_named(*wall, "F5:offset(" + side("F2", 1, 2) + ")") == 1);

  // T0 shell_two_faces: the top and the front removed.
  ShellSpec two = open;
  two.faces = {end_cap("F2", 1), side("F2", 1, 0)};
  CHECK_NEAR(volume(*shell("F5", *block(), two)), kBlock - 56.0 * 38.0 * 18.0);

  // T0 shell_closed: no face removed leaves a closed void.
  ShellSpec closed;
  closed.inside = 2;
  const ShapePtr hollow = shell("F5", *block(), closed);
  CHECK_NEAR(volume(*hollow), kBlock - 56.0 * 36.0 * 16.0);
  CHECK(hollow->face_count() == 12);
  CHECK(faces_named(*hollow, "F5:offset(" + end_cap("F2", 1) + ")") == 1);
  closed.outside = 1;
  CHECK_NEAR(volume(*shell("F5", *block(), closed)), 62.0 * 42.0 * 22.0 - 56.0 * 36.0 * 16.0);

  // A shell of a rounded block takes the rounding along (tangent chain).
  const ShapePtr rounded =
      fillet("F3", *block(), {edge(side("F2", 1, 0), side("F2", 1, 1))}, 5);
  ShellSpec chain;
  chain.faces = {side("F2", 1, 0)};
  chain.inside = 1;
  const ShapePtr opened = shell("F5", *rounded, chain);
  CHECK(faces_named(*opened, side("F2", 1, 1)) == 0);
  chain.tangent_chain = false;
  CHECK(faces_named(*shell("F5", *rounded, chain), side("F2", 1, 1)) == 1);

  // Rounded outside (K45 shell_rounded): the Minkowski sum of the block and a
  // 2 mm ball below the open top, less the block.
  ShellSpec rounded_out = outside;
  rounded_out.rounded = true;
  const ShapePtr smooth = shell("F5", *block(), rounded_out);
  const double t = 2;
  const double full = kBlock + 2 * t * (2400 + 800 + 1200) + kPi * t * t * 120 + 4.0 / 3 * kPi * t * t * t;
  const double above = 2400 * t + kPi * t * t / 4 * 200 + 2.0 / 3 * kPi * t * t * t;
  CHECK_NEAR(volume(*smooth), full - above - kBlock);
  CHECK(faces_named(*smooth, side("F2", 1, 0)) == 1);

  ShellSpec none = open;
  none.inside = 0;
  CHECK(throws_with([&] { shell("F5", *block(), none); }, "needs an inside or an outside"));
  none.inside = 30;
  CHECK(throws_with([&] { shell("F5", *block(), none); }, "shell"));
  none.inside = 2;
  none.faces = {"F2:end(r{c9})"};
  CHECK(throws_with([&] { shell("F5", *block(), none); }, "the body has no face F2:end(r{c9})"));
}

void test_draft() {
  // T0 draft_face: the front face about the bottom face, 5 degrees. The
  // pull direction points into the material from the bottom face (+z), and
  // a positive angle removes material above it.
  DraftSpec spec;
  spec.faces = {side("F2", 1, 0)};
  spec.plane = Tool::of_face(block(), start_cap("F2", 1));
  spec.angle = 5.0 * kPi / 180.0;
  const ShapePtr drafted = draft("F5", *block(), spec);
  CHECK_NEAR(volume(*drafted), kBlock - kDraftWedge);
  CHECK(faces_named(*drafted, side("F2", 1, 0)) == 1);
  CHECK(edge_names_unique(*drafted));

  // The same with a plane; flipped it adds material.
  spec.plane = Tool::of_plane(plane(0, 0, 0, 0, 0, 1));
  CHECK_NEAR(volume(*draft("F5", *block(), spec)), kBlock - kDraftWedge);
  spec.flip = true;
  CHECK_NEAR(volume(*draft("F5", *block(), spec)), kBlock + kDraftWedge);
  spec.flip = false;
  spec.angle = -spec.angle;
  CHECK_NEAR(volume(*draft("F5", *block(), spec)), kBlock + kDraftWedge);

  // Symmetric about the middle plane: both halves narrow away from it.
  DraftSpec both;
  both.faces = {side("F2", 1, 0)};
  both.plane = Tool::of_plane(plane(0, 0, 10, 0, 0, 1));
  both.angle = 5.0 * kPi / 180.0;
  both.angle2 = both.angle;
  const ShapePtr waisted = draft("F5", *block(), both);
  CHECK_NEAR(volume(*waisted), kBlock - 2 * 10.0 * 10.0 * std::tan(both.angle) / 2.0 * 60.0);
  CHECK(faces_named(*waisted, side("F2", 1, 0) + "#0") == 1);
  CHECK(faces_named(*waisted, side("F2", 1, 0) + "#1") == 1);

  // A rounded corner is tangent to the drafted face: OCCT drafts it along.
  const ShapePtr rounded = fillet("F3", *block(), {edge(side("F2", 1, 0), side("F2", 1, 1))}, 5);
  DraftSpec chain;
  chain.faces = {side("F2", 1, 0)};
  chain.plane = Tool::of_plane(plane(0, 0, 0, 0, 0, 1));
  chain.angle = 3.0 * kPi / 180.0;
  CHECK(volume(*draft("F5", *rounded, chain)) < volume(*rounded));
  chain.tangent_chain = false;
  CHECK(throws_with([&] { draft("F5", *rounded, chain); }, "unsupported: face"));

  spec.angle = kPi / 2;
  CHECK(throws_with([&] { draft("F5", *block(), spec); }, "between -90 and 90"));
  spec.angle = 0.1;
  spec.plane = Tool::of_face(block(), side("F2", 1, 1));
  spec.faces = {side("F2", 1, 1)};
  CHECK(throws_with([&] { draft("F5", *block(), spec); }, "draft"));
}

void test_offset_faces() {
  // T0 offset_face_top: the top 5 mm up.
  const ShapePtr taller = offset_faces("F5", *block(), {end_cap("F2", 1)}, 5);
  CHECK_NEAR(volume(*taller), 60.0 * 40.0 * 25.0);
  CHECK(faces_named(*taller, end_cap("F2", 1)) == 1);
  CHECK(taller->face_count() == 6);
  CHECK(edge_names_unique(*taller));
  CHECK_NEAR(volume(*offset_faces("F5", *block(), {end_cap("F2", 1)}, -5)), 60.0 * 40.0 * 15.0);

  // T0 presspull_hole_wall: a 10 mm hole's wall -2 mm, away from the axis.
  const ShapePtr drill = extrude("F4", {circle(5, 30, 20, 5)}, -5, 30);
  const ShapePtr holed = boolean(BooleanOp::Cut, {block().get()}, *drill).pieces.at(0).shape;
  const ShapePtr wider = offset_faces("F5", *holed, {"F4:side(c5)"}, -2);
  CHECK_NEAR(volume(*wider), kBlock - kPi * 49.0 * 20.0);
  CHECK(faces_named(*wider, "F4:side(c5)") == 1);

  // A slanted neighbour extends along its own plane: the top of a drafted
  // block raised by 5 mm keeps the 5 degree side.
  DraftSpec slant;
  slant.faces = {side("F2", 1, 0)};
  slant.plane = Tool::of_plane(plane(0, 0, 0, 0, 0, 1));
  slant.angle = 5.0 * kPi / 180.0;
  const ShapePtr drafted = draft("F4", *block(), slant);
  const double t = std::tan(slant.angle);
  CHECK(near(volume(*offset_faces("F5", *drafted, {end_cap("F2", 1)}, 5)),
             60.0 * 40.0 * 25.0 - 25.0 * 25.0 * t / 2.0 * 60.0));

  CHECK(throws_with([&] { offset_faces("F5", *block(), {end_cap("F2", 1)}, 0); }, "not be zero"));
}

void test_delete_faces() {
  // T0 delface_fillet: deleting the rounding brings the sharp corner back.
  const ShapePtr rounded = fillet("F3", *block(), {edge(side("F2", 1, 3), side("F2", 1, 0))}, 5);
  const std::string face = "F3:fillet(" + edge(side("F2", 1, 3), side("F2", 1, 0)) + ")";
  const ShapePtr healed = delete_faces("F5", *rounded, {face});
  CHECK_NEAR(volume(*healed), kBlock);
  CHECK(healed->face_count() == 6);
  CHECK(healed->find_edges(edge(side("F2", 1, 3), side("F2", 1, 0))).size() == 1);

  // T0 delface_pocket: a 10 mm pocket 10 mm deep, its wall and floor deleted.
  const ShapePtr cutter = extrude("F4", {circle(5, 30, 20, 5)}, 10, 30);
  const ShapePtr pocket = boolean(BooleanOp::Cut, {block().get()}, *cutter).pieces.at(0).shape;
  CHECK_NEAR(volume(*pocket), kBlock - kPi * 25.0 * 10.0);
  const ShapePtr filled = delete_faces("F5", *pocket, {"F4:side(c5)", "F4:start(r{c5})"});
  CHECK_NEAR(volume(*filled), kBlock);
  CHECK(filled->face_count() == 6);

  // Two opposite sides leave a gap nothing closes.
  CHECK(throws_with([&] { delete_faces("F5", *block(), {side("F2", 1, 0), side("F2", 1, 2)}); },
                    "cannot be deleted"));
}

void test_replace_faces() {
  // T0 replface_surface: the top onto a plane at z = 25.
  const ShapePtr raised =
      replace_faces("F5", *block(), {end_cap("F2", 1)}, Tool::of_plane(plane(0, 0, 25, 0, 0, 1)));
  CHECK_NEAR(volume(*raised), 60.0 * 40.0 * 25.0);
  const std::string name = "F5:replace(" + end_cap("F2", 1) + ")";
  CHECK(faces_named(*raised, name) == 1);
  CHECK(faces_named(*raised, end_cap("F2", 1)) == 0);
  CHECK(edge_names_unique(*raised));

  // A face of another body as the target: the bottom of a box at z = 12.
  const ShapePtr above = extrude("F7", {rectangle(11, -10, -10, 100, 100)}, 12, 30);
  CHECK(near(volume(*replace_faces("F5", *block(), {end_cap("F2", 1)},
                                   Tool::of_face(above, start_cap("F7", 11)))),
             60.0 * 40.0 * 12.0));

  // A slanted target, from z = 30 at y = 0 down to z = 20 at y = 40, and
  // one that cuts into the body (z = 18 to 16).
  const gp_Dir slope(0, 10, 40);
  const ShapePtr wedge = replace_faces("F5", *block(), {end_cap("F2", 1)},
                                       Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 30), slope)));
  CHECK_NEAR_TOL(volume(*wedge), 60.0 * 40.0 * 25.0, 1e-6);
  CHECK(faces_named(*wedge, name) == 1);
  CHECK(wedge->face_count() == 6);
  const gp_Dir shallow(0, 2, 40);
  const ShapePtr lower = replace_faces("F5", *block(), {end_cap("F2", 1)},
                                       Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 18), shallow)));
  CHECK_NEAR_TOL(volume(*lower), 60.0 * 40.0 * 17.0, 1e-6);

  CHECK(throws_with([&] {
    replace_faces("F5", *block(), {end_cap("F2", 1)}, Tool::of_plane(plane(0, 0, 25, 1, 0, 0)));
  },
                    "perpendicular"));
}

// A cylinder along X through (y, z) = (y0, z0) of radius r, from x = -10 to
// 70, with its curved face F9:side0.
ShapePtr cylinder_along_x(double y0, double z0, double r) {
  PrimitiveSpec spec;
  spec.feature = "F9";
  spec.kind = PrimitiveKind::Cylinder;
  spec.frame.origin = gp_Pnt(-10, y0, z0);
  spec.frame.x_axis = gp_Dir(0, 1, 0);
  spec.frame.y_axis = gp_Dir(0, 0, 1);
  spec.a = r;
  spec.c = 80;
  return primitive(spec);
}

// The integral of sqrt(r^2 - t^2) from a to b.
double circle_area(double r, double a, double b) {
  const auto f = [r](double t) {
    return 0.5 * (t * std::sqrt(r * r - t * t) + r * r * std::asin(t / r));
  };
  return f(b) - f(a);
}

void test_replace_curved_faces() {
  const std::string top = end_cap("F2", 1);
  const std::string name = "F5:replace(" + top + ")";

  // The top onto the upper side of a cylinder along X through y = 20,
  // z = 0, R = 30: the new top is z = sqrt(900 - (y - 20)^2).
  const ShapePtr arched =
      replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, 0, 30), "F9:side0"));
  CHECK_NEAR(volume(*arched), 60.0 * circle_area(30, -20, 20));
  CHECK(faces_named(*arched, name) == 1);
  CHECK(faces_named(*arched, top) == 0);
  CHECK(arched->face_count() == 6);
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*arched, side("F2", 1, i)) == 1);
  }
  CHECK(edge_names_unique(*arched));

  // The lower side of one through z = 55: a hollow top.
  const ShapePtr hollow =
      replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, 55, 30), "F9:side0"));
  CHECK_NEAR(volume(*hollow), 60.0 * (40.0 * 55.0 - circle_area(30, -20, 20)));
  CHECK(faces_named(*hollow, name) == 1);

  // One that crosses the top (z = -5, R = 27): material added in the
  // middle and removed at the sides; the new face is one.
  const ShapePtr crossing =
      replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, -5, 27), "F9:side0"));
  CHECK_NEAR(volume(*crossing), 60.0 * (-5.0 * 40.0 + circle_area(27, -20, 20)));
  CHECK(faces_named(*crossing, name) == 1);
  CHECK(faces_named(*crossing, top) == 0);
  CHECK(edge_names_unique(*crossing));

  // A curved face onto a plane: the round end of a 40 x 20 x 10 block with
  // a half disc at x = 40 onto x = 55; the tangent sides continue.
  const ShapePtr slot =
      boolean(BooleanOp::Join, {extrude("F2", {rectangle(1, 0, -10, 40, 20)}, 0, 10).get()},
              *extrude("F3", {circle(5, 40, 0, 10)}, 0, 10))
          .pieces.at(0)
          .shape;
  CHECK_NEAR(volume(*slot), 8000.0 + 500.0 * kPi);
  const ShapePtr squared = replace_faces("F5", *slot, {"F3:side(c5)"},
                                         Tool::of_plane(plane(55, 0, 0, 1, 0, 0)), false);
  CHECK_NEAR(volume(*squared), 55.0 * 20.0 * 10.0);
  CHECK(faces_named(*squared, "F5:replace(F3:side(c5))") == 1);
  CHECK(squared->face_count() == 6);
  CHECK(edge_names_unique(*squared));
  // With the tangent chain the sides go too, and nothing closes the end.
  CHECK(throws_with([&] {
    replace_faces("F5", *slot, {"F3:side(c5)"}, Tool::of_plane(plane(55, 0, 0, 1, 0, 0)), true);
  },
                    "does not meet"));

  // A cylinder onto a coaxial one: the ends grow with it.
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  const ShapePtr wider = replace_faces("F5", *pin, {"F2:side(c5)"},
                                       Tool::of_face(extrude("F7", {circle(8, 0, 0, 12)}, -5, 30),
                                                     "F7:side(c8)"));
  CHECK_NEAR(volume(*wider), kPi * 144.0 * 20.0);
  CHECK(faces_named(*wider, "F5:replace(F2:side(c5))") == 1);
  CHECK(faces_named(*wider, "F2:end(r{c5})") == 1);
  CHECK(faces_named(*wider, "F2:start(r{c5})") == 1);
  const ShapePtr thinner = replace_faces("F5", *pin, {"F2:side(c5)"},
                                         Tool::of_face(extrude("F7", {circle(8, 0, 0, 8)}, 5, 10),
                                                       "F7:side(c8)"));
  CHECK_NEAR(volume(*thinner), kPi * 64.0 * 20.0);
  // The pin's side onto a plane across its axis: nothing between them.
  CHECK(throws_with([&] {
    replace_faces("F5", *pin, {"F2:side(c5)"}, Tool::of_plane(plane(0, 0, 25, 0, 0, 1)));
  },
                    "does not meet"));

  // The top and the roundings of its long edges onto z = 25; the sides
  // they were tangent to continue up.
  const std::string front = edge(top, side("F2", 1, 0));
  const std::string back = edge(top, side("F2", 1, 2));
  const ShapePtr rounded = fillet("F3", *block(), {front, back}, 3);
  const std::vector<std::string> rounded_top = {top, "F3:fillet(" + front + ")",
                                                "F3:fillet(" + back + ")"};
  const ShapePtr raised = replace_faces("F5", *rounded, rounded_top,
                                        Tool::of_plane(plane(0, 0, 25, 0, 0, 1)), false);
  CHECK_NEAR(volume(*raised), 60.0 * 40.0 * 25.0);
  CHECK(raised->face_count() == 6);
  CHECK(faces_named(*raised, "F3:fillet(" + front + ")") == 0);
  CHECK(faces_named(*raised, name) == 1);
  CHECK(edge_names_unique(*raised));

  // A target around the top: its lower side would pass the bottom in the
  // middle, so the top goes up to its upper side.
  const ShapePtr around =
      replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, 35, 40), "F9:side0"));
  CHECK_NEAR(volume(*around), 60.0 * (35.0 * 40.0 + circle_area(40, -20, 20)));
  // A target beside the body never meets it; one below the bottom at the
  // sides passes beyond the other faces.
  CHECK(throws_with([&] {
    replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(200, 0, 30), "F9:side0"));
  },
                    "does not meet"));
  CHECK(throws_with([&] {
    replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, -25, 30), "F9:side0"));
  },
                    "beyond the other faces"));
  // A body cannot be its own target.
  const ShapePtr own = block();
  CHECK(throws_with([&] { replace_faces("F5", *own, {top}, Tool::of_body(own)); }, "itself"));
}

// The integral of f over [a, b] by Simpson's rule.
template <class F>
double integral(F f, double a, double b, int steps = 2000) {
  const double h = (b - a) / steps;
  double sum = f(a) + f(b);
  for (int i = 1; i < steps; ++i) {
    sum += f(a + i * h) * (i % 2 == 1 ? 4.0 : 2.0);
  }
  return sum * h / 3.0;
}

// The upper side of the cylinder along X through y = 20, z = 0, R = 30 as
// a face of a body of its own, over 0.5 < u < 2.64 (y from -6 to 46) and
// 10 < x < 50: on its surface, or as a B-spline.
ShapePtr arch_sheet(bool spline) {
  const gp_Ax3 axes(gp_Pnt(0, 20, 0), gp_Dir(1, 0, 0), gp_Dir(0, 1, 0));
  const occ::handle<Geom_Surface> surface = new Geom_CylindricalSurface(axes, 30);
  TopoDS_Face face;
  if (spline) {
    const occ::handle<Geom_Surface> patch = GeomConvert::SurfaceToBSplineSurface(
        new Geom_RectangularTrimmedSurface(surface, 0.5, 2.64, 10.0, 50.0));
    face = BRepBuilderAPI_MakeFace(patch, Precision::Confusion()).Face();
  } else {
    face = BRepBuilderAPI_MakeFace(surface, 0.5, 2.64, 10, 50, Precision::Confusion()).Face();
  }
  return std::make_shared<Shape>(face, std::vector<Shape::NamedFace>{{face, {"F8:arch"}}});
}

void test_replace_curved_targets() {
  const std::string top = end_cap("F2", 1);
  const double arch = 60.0 * circle_area(30, -20, 20);

  // A surface body of one face, continued along its surface; as a face or
  // the body, and as a B-spline continued past its edges.
  CHECK_NEAR(volume(*replace_faces("F5", *block(), {top}, Tool::of_body(arch_sheet(false)))), arch);
  const ShapePtr spline = arch_sheet(true);
  CHECK_NEAR_TOL(volume(*replace_faces("F5", *block(), {top}, Tool::of_face(spline, "F8:arch"))),
                 arch, 1e-5);

  // The walls of a hole through the top continue up to the target.
  const ShapePtr holed =
      boolean(BooleanOp::Cut, {block().get()}, *extrude("F4", {circle(5, 30, 20, 5)}, -1, 30))
          .pieces.at(0)
          .shape;
  const ShapePtr arched =
      replace_faces("F5", *holed, {top}, Tool::of_face(cylinder_along_x(20, 0, 30), "F9:side0"));
  // The hole's column: its width at t times the arch's height there.
  const double hole = integral(
      [](double t) {
        return 2.0 * std::sqrt(std::max(0.0, 25.0 - t * t)) * std::sqrt(900.0 - t * t);
      },
      -5, 5, 20000);
  CHECK_NEAR_TOL(volume(*arched), arch - hole, 1e-6);
  CHECK(faces_named(*arched, "F4:side(c5)") == 1);
  CHECK(edge_names_unique(*arched));

  // Points inside the faces, by which the .f3d import finds the faces a
  // replace face replaced: nine on each face (none in the hole), and those
  // of the replaced top are off the result.
  const std::vector<FacePoints> points = face_points(*holed);
  CHECK(static_cast<int>(points.size()) == holed->face_count());
  const auto distance = [](const gp_Pnt& p, const TopoDS_Shape& shape) {
    BRepExtrema_DistShapeShape measure(BRepBuilderAPI_MakeVertex(p).Vertex(), shape);
    return measure.IsDone() ? measure.Value() : 1e9;
  };
  for (const FacePoints& face : points) {
    CHECK(face.points.size() == 9);
    const TopoDS_Face& own = holed->face(holed->find_faces(face.name).front());
    for (const gp_Pnt& p : face.points) {
      CHECK(distance(p, own) < 1e-7);
      CHECK(std::hypot(p.X() - 30, p.Y() - 20) > 5 - 1e-7 || std::abs(p.Z() - 20) > 1e-7);
      const bool on_result = distance(p, arched->occt()) < 1e-6;
      CHECK(on_result == (face.name != top));
    }
  }

  // The end of a pin onto a sphere that dips into it: of the two regions
  // between them the nearer one goes (the lens above the end stays out).
  PrimitiveSpec ball;
  ball.feature = "F9";
  ball.kind = PrimitiveKind::Sphere;
  ball.frame.origin = gp_Pnt(0, 0, 30);
  ball.a = 15;
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  const ShapePtr dished =
      replace_faces("F5", *pin, {"F2:end(r{c5})"}, Tool::of_face(primitive(ball), "F9:side0"));
  CHECK_NEAR(volume(*dished),
             3000.0 * kPi - 2.0 * kPi / 3.0 * (3375.0 - 125.0 * std::sqrt(125.0)));
  CHECK(faces_named(*dished, "F5:replace(F2:end(r{c5}))") == 1);
  CHECK(faces_named(*dished, "F2:side(c5)") == 1);

  // The pin with B-spline faces (as imported bodies have them): its side
  // continues as a B-spline and keeps its name.
  BRepBuilderAPI_NurbsConvert nurbs(pin->occt(), true);
  std::vector<Shape::NamedFace> named;
  for (int i = 0; i < pin->face_count(); ++i) {
    named.push_back({nurbs.ModifiedShape(pin->face(i)), pin->face_names(i)});
  }
  const Shape spline_pin(nurbs.Shape(), named);
  const ShapePtr taller = replace_faces("F5", spline_pin, {"F2:end(r{c5})"},
                                        Tool::of_plane(plane(0, 0, 25, 0, 0, 1)));
  // The continuation is an extrapolation of the rational B-spline.
  CHECK_NEAR_TOL(volume(*taller), kPi * 100.0 * 25.0, 1e-4);
  CHECK(faces_named(*taller, "F2:side(c5)") == 1);
  CHECK(faces_named(*taller, "F5:replace(F2:end(r{c5}))") == 1);
  CHECK(edge_names_unique(*taller));
}

} // namespace

void faceops_tests() {
  guarded("test_shell", test_shell);
  guarded("test_draft", test_draft);
  guarded("test_offset_faces", test_offset_faces);
  guarded("test_delete_faces", test_delete_faces);
  guarded("test_replace_faces", test_replace_faces);
  guarded("test_replace_curved_faces", test_replace_curved_faces);
  guarded("test_replace_curved_targets", test_replace_curved_targets);
}

} // namespace test
