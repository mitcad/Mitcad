// SPDX-License-Identifier: MIT
// Shell, draft, offset, delete and replace on named faces: volumes of the
// T0 models and analytic cases, the names of the new faces, and failures.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax3.hxx>

#include "check.hpp"
#include "mitcad/geometry/face_extrude.hpp"
#include "mitcad/geometry/user_error.hpp"

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
  // Without the chain, the open front ends smoothly into the kept rounding,
  // where the offset cannot close the wall: an error (before the port's
  // patch 0100, the kernel's offset gave the block itself, unshelled).
  chain.tangent_chain = false;
  CHECK(throws_with([&] { shell("F5", *rounded, chain); }, "shell failed"));

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

  // A pin with B-spline faces (as imported bodies have them): the side is a
  // rational B-spline whose knots repeat as often as its degree, only C0
  // continuous, which the kernel's offset refuses; the shell splits it at
  // the creases, and the pieces keep the side's name (mitcad#158).
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  BRepBuilderAPI_NurbsConvert nurbs(pin->occt(), true);
  std::vector<Shape::NamedFace> named;
  bool creased = false;
  for (int i = 0; i < pin->face_count(); ++i) {
    const TopoDS_Shape face = nurbs.ModifiedShape(pin->face(i));
    named.push_back({face, pin->face_names(i)});
    creased = creased || BRep_Tool::Surface(TopoDS::Face(face))->Continuity() == GeomAbs_C0;
  }
  CHECK(creased);
  const Shape spline_pin(nurbs.Shape(), named);
  ShellSpec cup;
  cup.faces = {"F2:end(r{c5})"};
  cup.inside = 1;
  const ShapePtr cupped = shell("F5", spline_pin, cup);
  CHECK_NEAR_TOL(volume(*cupped), kPi * (100.0 * 20.0 - 81.0 * 19.0), 1e-6);
  CHECK(cupped->find_faces("F2:side(c5)").size() >= 1);
  CHECK(!cupped->find_faces("F5:offset(F2:side(c5))").empty());
  CHECK(faces_named(*cupped, "F2:end(r{c5})") == 0);
  CHECK(edge_names_unique(*cupped));

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

bool every_face_named(const Shape& shape) {
  for (int i = 0; i < shape.face_count(); ++i) {
    if (shape.face_names(i).empty()) {
      return false;
    }
  }
  return true;
}

// Wherever the fixed plane lies, the drafted faces keep their names and
// every face of the result has one: a sketch on a drafted face and a pick
// of it find the face by its name (mitcad#2351).
void test_draft_about_a_moved_plane() {
  const double t = std::tan(5.0 * kPi / 180.0);
  // The side x = 60 drafted 5 degrees about planes across y (pull +y): at
  // the front, through the middle, at the back and beyond the block. It
  // turns about its line at y = p, leaning in by t per mm towards +y.
  for (const double p : {0.0, 20.0, 40.0, 55.0}) {
    DraftSpec spec;
    spec.faces = {side("F2", 1, 1)};
    spec.plane = Tool::of_plane(plane(0, p, 0, 0, 1, 0));
    spec.angle = 5.0 * kPi / 180.0;
    const ShapePtr drafted = draft("F5", *block(), spec);
    CHECK_NEAR(volume(*drafted), kBlock - 20.0 * t * (800.0 - 40.0 * p));
    CHECK(faces_named(*drafted, side("F2", 1, 1)) == 1);
    CHECK(drafted->face_count() == 6);
    CHECK(every_face_named(*drafted));
    CHECK(edge_names_unique(*drafted));
  }

  // A round pin's side drafted about planes across z: the cylinder becomes
  // a cone, its seam along; the side keeps its name.
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  for (const double p : {0.0, 10.0, 25.0}) {
    DraftSpec cone;
    cone.faces = {"F2:side(c5)"};
    cone.plane = Tool::of_plane(plane(0, 0, p, 0, 0, 1));
    cone.angle = 5.0 * kPi / 180.0;
    const ShapePtr drafted = draft("F5", *pin, cone);
    const double low = 10.0 + p * t;
    const double high = 10.0 - (20.0 - p) * t;
    CHECK_NEAR(volume(*drafted), kPi * 20.0 / 3.0 * (low * low + low * high + high * high));
    CHECK(faces_named(*drafted, "F2:side(c5)") == 1);
    CHECK(every_face_named(*drafted));
  }
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

  // A body that is a compound holding one solid (as dress-ups and booleans
  // give theirs): a pin of diameter 15.9 whose side moves in to a thread's
  // major diameter of 15.822 (mitcad#326; OCCT's offset of the compound
  // gave a shell, no solid).
  const ShapePtr pin = extrude("F6", {circle(9, 0, 0, 7.95)}, 0, 16.7);
  BRep_Builder builder;
  TopoDS_Compound wrapped;
  builder.MakeCompound(wrapped);
  builder.Add(wrapped, pin->occt());
  std::vector<Shape::NamedFace> names;
  for (int i = 0; i < pin->face_count(); ++i) {
    names.push_back({pin->face(i), pin->face_names(i)});
  }
  const Shape compound(wrapped, names);
  const ShapePtr sized = offset_faces("F7", compound, {"F6:side(c9)"}, -0.039);
  CHECK_NEAR(volume(*sized), kPi * 7.911 * 7.911 * 16.7);
  CHECK(faces_named(*sized, "F6:side(c9)") == 1);
}

// A piece of a split face offset alone (mitcad#1928): the piece moves, the
// other piece stays, and the seam between them becomes a step,
// <feature>:step(<piece>). The geometry kernel's offset alone gave an
// invalid shape there: the piece that stays cannot extend along its own
// plane to meet the moved one.
void test_offset_split_faces() {
  const std::string top = end_cap("F2", 1);
  // The top split at x = 20: piece #0 is the 20 x 40 one (test_split).
  const Tool at20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const ShapePtr split = split_faces("F7", *block(), {top}, at20);
  const std::string piece = top + "#0";
  const std::string step = "F8:step(" + piece + ")";
  for (const double distance : {-1.0, 1.0}) {
    const ShapePtr moved = offset_faces("F8", *split, {piece}, distance);
    CHECK_NEAR(volume(*moved), kBlock + distance * 20.0 * 40.0);
    CHECK(faces_named(*moved, piece) == 1);
    CHECK(faces_named(*moved, top + "#1") == 1);
    CHECK(faces_named(*moved, step) == 1);
    CHECK(moved->face_count() == 8);
    CHECK(edge_names_unique(*moved));
    // The moved piece at its new height, the other at the old one.
    for (const FaceInfo& info : face_infos(*moved)) {
      if (has_name(info.names, piece)) {
        CHECK(near(info.area, 20.0 * 40.0));
      }
      if (has_name(info.names, step)) {
        CHECK(near(info.area, 40.0 * 1.0));
      }
    }
  }

  // A disc split out of the top by a circle: the disc pressed in and
  // pulled out, the ring around it stays; the step is a cylinder.
  Tool curves;
  curves.kind = Tool::Kind::Curves;
  curves.regions = {circle(9, 30, 20, 5)};
  curves.curves = {"c9"};
  const ShapePtr ringed = split_faces("F7", *block(), {top}, curves);
  std::string disc;
  for (const FaceInfo& info : face_infos(*ringed)) {
    for (const std::string& name : info.names) {
      if (near(info.area, kPi * 25.0) && (name == top + "#0" || name == top + "#1")) {
        disc = name;
      }
    }
  }
  CHECK(!disc.empty());
  if (disc.empty()) {
    return;
  }
  for (const double distance : {-1.0, 1.0}) {
    const ShapePtr moved = offset_faces("F8", *ringed, {disc}, distance);
    CHECK_NEAR(volume(*moved), kBlock + distance * kPi * 25.0);
    CHECK(faces_named(*moved, disc) == 1);
    CHECK(faces_named(*moved, "F8:step(" + disc + ")") == 1);
    CHECK(moved->face_count() == 8);
  }

  // Both pieces moved together are a whole face again: no step.
  const ShapePtr both = offset_faces("F8", *split, {piece, top + "#1"}, -1);
  CHECK_NEAR(volume(*both), kBlock - 60.0 * 40.0);
  CHECK(faces_named(*both, step) == 0);
}

// The name of the face of the shape on which the point lies (the first of
// its names), empty when there is none.
std::string face_at(const Shape& shape, const gp_Pnt& point) {
  const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
  for (int i = 0; i < shape.face_count(); ++i) {
    BRepExtrema_DistShapeShape distance(vertex, shape.face(i));
    if (distance.IsDone() && distance.Value() < 1.0e-6 && !shape.face_names(i).empty()) {
      return shape.face_names(i).front();
    }
  }
  return {};
}

// The faces named as steps after the moved face: <feature>:step(<face>),
// or with #k when it has several.
int steps_of(const Shape& shape, const std::string& face) {
  const std::string step = "F9:step(" + face + ")";
  int count = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& name : shape.face_names(i)) {
      if (name == step || name.rfind(step + "#", 0) == 0) {
        ++count;
        break;
      }
    }
  }
  return count;
}

// Two pieces of split faces that meet at a corner of the body, offset
// together (mitcad#1947): each piece moves along its own normal, the
// pieces' new positions are extended or trimmed to each other at the
// corner, and their other sides step down to the pieces that stay. By
// `t` in, the body loses t·(A1 + A2) less what the two slabs share at the
// corner, t²·L·tan(θ/2) (θ the angle between the pieces' normals, L the
// corner's length); by `t` out it gains t·(A1 + A2) and the corner's
// wedge, t²·L·tan(θ/2). At a concave corner tan(θ/2) is negative.
void check_corner(const Shape& body, const std::string& a, const std::string& b, double area,
                  double corner) {
  const double before = volume(body);
  for (const double t : {1.0, 4.5}) {
    for (const double sign : {-1.0, 1.0}) {
      const ShapePtr moved = offset_faces("F9", body, {a, b}, sign * t);
      CHECK_NEAR(volume(*moved), before + sign * t * area + t * t * corner);
      CHECK(faces_named(*moved, a) == 1);
      CHECK(faces_named(*moved, b) == 1);
      // Each step is named after the moved face whose seam is nearest; steps
      // in one plane are one face.
      CHECK(steps_of(*moved, a) + steps_of(*moved, b) >= 1);
      CHECK(edge_names_unique(*moved));
    }
  }
}

void test_offset_split_corners() {
  // A convex corner at 90 degrees: the top and the side x = 0 split at
  // y = 20, the pieces at y < 20 (60 x 20 and 20 x 20) moved.
  const std::string top = end_cap("F2", 1);
  const std::string left = side("F2", 1, 3);
  const Tool at_y20 = Tool::of_plane(gp_Pln(gp_Pnt(0, 20, 0), gp_Dir(0, 1, 0)));
  const ShapePtr convex = split_faces("F7", *block(), {top, left}, at_y20);
  const std::string top_piece = face_at(*convex, gp_Pnt(30, 10, 20));
  const std::string left_piece = face_at(*convex, gp_Pnt(0, 10, 10));
  CHECK(top_piece.rfind(top + "#", 0) == 0 && left_piece.rfind(left + "#", 0) == 0);
  check_corner(*convex, top_piece, left_piece, 1200.0 + 400.0, 20.0);

  // A concave corner: a 30 mm wide, 10 mm deep rebate along the block's
  // end; its floor and wall split at y = 20, the pieces at y < 20 (30 x 20
  // and 20 x 10) moved. The slabs miss the corner's square going in and
  // share it going out.
  const ShapePtr cutter = extrude("F4", {rectangle(5, 30, -1, 40, 42)}, 10, 30);
  const ShapePtr rebate = boolean(BooleanOp::Cut, {block().get()}, *cutter).pieces.at(0).shape;
  const std::string floor = face_at(*rebate, gp_Pnt(45, 20, 10));
  const std::string wall = face_at(*rebate, gp_Pnt(30, 20, 15));
  CHECK(!floor.empty() && !wall.empty());
  const ShapePtr concave = split_faces("F7", *rebate, {floor, wall}, at_y20);
  check_corner(*concave, face_at(*concave, gp_Pnt(45, 10, 10)), face_at(*concave, gp_Pnt(30, 10, 15)),
               600.0 + 200.0, -20.0);

  // A convex corner at 110 degrees: the side y = 0 drafted 20 degrees about
  // the base; the top and the side split at x = 20 and the side's piece
  // again at z = 5, so that all the pieces' other sides are steps or meet
  // faces square to them. The pieces at x < 20 above z = 5 moved.
  DraftSpec slant;
  slant.faces = {side("F2", 1, 0)};
  slant.plane = Tool::of_plane(plane(0, 0, 0, 0, 0, 1));
  slant.angle = 20.0 * kPi / 180.0;
  const ShapePtr drafted = draft("F4", *block(), slant);
  const double lean = std::tan(slant.angle);
  // The side runs from y = 0 at the base to y = 20·tan(20°) at the top, or
  // the other way.
  const bool narrower = !face_at(*drafted, gp_Pnt(10, 20.0 * lean / 2.0, 10)).empty();
  const double y_at = narrower ? 1.0 : -1.0;
  const Tool at_x20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const std::string slanted = face_at(*drafted, gp_Pnt(10, y_at * 10.0 * lean, 10));
  CHECK(!slanted.empty());
  const ShapePtr halves = split_faces("F7", *drafted, {top, slanted}, at_x20);
  const Tool at_z5 = Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 5), gp_Dir(0, 0, 1)));
  const ShapePtr quarters =
      split_faces("F8", *halves, {face_at(*halves, gp_Pnt(10, y_at * 10.0 * lean, 10))}, at_z5);
  const std::string slant_piece = face_at(*quarters, gp_Pnt(10, y_at * 12.0 * lean, 12));
  const std::string top_half = face_at(*quarters, gp_Pnt(10, 30, 20));
  CHECK(!slant_piece.empty() && !top_half.empty());
  // The top's piece is 20 wide and as deep as the top; the side's piece
  // 20 wide and 15 / cos(20°) high; the normals 90 ∓ 20 degrees apart
  // (70 where the top is the narrower end: the corner is 110 degrees).
  const double depth = narrower ? 40.0 - 20.0 * lean : 40.0 + 20.0 * lean;
  const double theta = (narrower ? 90.0 - 20.0 : 90.0 + 20.0) * kPi / 180.0;
  check_corner(*quarters, top_half, slant_piece, 20.0 * depth + 20.0 * 15.0 / std::cos(slant.angle),
               20.0 * std::tan(theta / 2.0));

  // A piece of a cylindrical face: a 10 mm hole's wall split at x = 30,
  // the half at x < 30 moved away from the axis (the hole widens there)
  // and towards it; the seams become two flat 1 x 20 steps.
  const ShapePtr drill = extrude("F4", {circle(5, 30, 20, 5)}, -5, 30);
  const ShapePtr holed = boolean(BooleanOp::Cut, {block().get()}, *drill).pieces.at(0).shape;
  const Tool at_x30 = Tool::of_plane(gp_Pln(gp_Pnt(30, 0, 0), gp_Dir(1, 0, 0)));
  const ShapePtr halved = split_faces("F7", *holed, {"F4:side(c5)"}, at_x30);
  const std::string half = face_at(*halved, gp_Pnt(25, 20, 10));
  CHECK(half.rfind("F4:side(c5)#", 0) == 0);
  const ShapePtr wider = offset_faces("F9", *halved, {half}, -1);
  CHECK_NEAR(volume(*wider), volume(*holed) - 20.0 * kPi * (36.0 - 25.0) / 2.0);
  CHECK(faces_named(*wider, half) == 1);
  CHECK(steps_of(*wider, half) == 2);
  const ShapePtr narrower_hole = offset_faces("F9", *halved, {half}, 1);
  CHECK_NEAR(volume(*narrower_hole), volume(*holed) + 20.0 * kPi * (25.0 - 16.0) / 2.0);

  // A pocket inside a moved piece keeps its wall and floor: the top split
  // at x = 20 with an 8 mm pocket 5 mm deep in the piece at x < 20; the
  // piece 1 mm in makes the pocket 1 mm shallower, 1 mm out 1 mm deeper.
  const ShapePtr cup = extrude("F4", {circle(5, 10, 20, 4)}, 15, 30);
  const ShapePtr pocketed = boolean(BooleanOp::Cut, {block().get()}, *cup).pieces.at(0).shape;
  const Tool at_x20_top = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const ShapePtr cut_top = split_faces("F7", *pocketed, {end_cap("F2", 1)}, at_x20_top);
  const std::string rim = face_at(*cut_top, gp_Pnt(2, 2, 20));
  CHECK(rim.rfind(end_cap("F2", 1) + "#", 0) == 0);
  for (const double distance : {-1.0, 1.0}) {
    const ShapePtr moved = offset_faces("F9", *cut_top, {rim}, distance);
    CHECK_NEAR(volume(*moved), volume(*pocketed) + distance * (20.0 * 40.0 - kPi * 16.0));
    CHECK(faces_named(*moved, rim) == 1);
    CHECK(faces_named(*moved, "F4:side(c5)") == 1);
    CHECK(faces_named(*moved, "F4:start(r{c5})") == 1);
  }

  // That half moved to the axis or past it cannot be built: the refusal
  // says why in words, the face goes to its places.
  bool refused = false;
  try {
    offset_faces("F9", *halved, {half}, 5);
  } catch (const std::exception& error) {
    refused = true;
    const std::string message = error_message(error.what());
    CHECK(message.find("a curved face would shrink to nothing") != std::string::npos);
    CHECK(message.find("F4") == std::string::npos && message.find("#") == std::string::npos);
    CHECK(std::string(error.what()).find("\"name\":\"" + half + "\"") != std::string::npos);
  }
  CHECK(refused);
}

// A piece of a split face that meets a face that stays at a sharp edge
// which is not square to it (mitcad#1973): the face that stays extends or
// trims along its own surface to the piece's new position, as for a whole
// face, going in and out.
void test_offset_split_slanted_neighbours() {
  // The side y = 0 drafted 30 degrees either way about the base, so that
  // it meets the top at 120 or 60 degrees; the top split at x = 20 and the
  // piece at x < 20 moved. The side runs y = s·z (s = ±tan 30°), so the
  // piece is 20 x (40 - 20 s) and by t in the body loses
  // 20·(t·(40 - 20 s) + s·t²/2); by t out it gains 20·(t·(40 - 20 s) - s·t²/2).
  const std::string top = end_cap("F2", 1);
  const Tool at_x20 = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  for (const double degrees : {30.0, -30.0}) {
    DraftSpec slant;
    slant.faces = {side("F2", 1, 0)};
    slant.plane = Tool::of_plane(plane(0, 0, 0, 0, 0, 1));
    slant.angle = degrees * kPi / 180.0;
    const ShapePtr drafted = draft("F4", *block(), slant);
    const double lean = std::tan(30.0 * kPi / 180.0);
    const double s = !face_at(*drafted, gp_Pnt(10, 10.0 * lean, 10)).empty() ? lean : -lean;
    CHECK(!face_at(*drafted, gp_Pnt(10, 10.0 * s, 10)).empty());
    const ShapePtr split = split_faces("F7", *drafted, {top}, at_x20);
    const std::string piece = face_at(*split, gp_Pnt(10, 30, 20));
    CHECK(piece.rfind(top + "#", 0) == 0);
    const double width = 40.0 - 20.0 * s;
    for (const double t : {1.0, 4.5}) {
      for (const double sign : {-1.0, 1.0}) {
        const ShapePtr moved = offset_faces("F9", *split, {piece}, sign * t);
        CHECK_NEAR(volume(*moved), volume(*drafted) + sign * 20.0 * t * width - 20.0 * s * t * t / 2.0);
        CHECK(faces_named(*moved, piece) == 1);
        CHECK(faces_named(*moved, side("F2", 1, 0)) == 1);
        CHECK(steps_of(*moved, piece) == 1);
        CHECK(edge_names_unique(*moved));
      }
    }
  }

  // A face that stays on a cylinder: a round bar of radius 20 along z,
  // 60 long, cut flat at x = 10, so that its flat meets the round at 120
  // degrees; the flat split at z = 20 and the piece at z < 20 moved. The
  // round extends or trims along itself: by t in the bar loses 20 times the
  // segment of the circle between x = 10 - t and 10, by t out gains the one
  // between 10 and 10 + t (F(x) = x·√(400 - x²) + 400·asin(x / 20) the
  // segment's area from x = 0).
  const ShapePtr bar = extrude("F2", {circle(1, 0, 0, 20)}, 0, 60);
  const ShapePtr cutter = extrude("F3", {rectangle(5, 10, -30, 30, 60)}, -1, 61);
  const ShapePtr flat = boolean(BooleanOp::Cut, {bar.get()}, *cutter).pieces.at(0).shape;
  const std::string face = face_at(*flat, gp_Pnt(10, 0, 30));
  CHECK(!face.empty());
  const Tool at_z20 = Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 20), gp_Dir(0, 0, 1)));
  const ShapePtr halves = split_faces("F7", *flat, {face}, at_z20);
  const std::string piece = face_at(*halves, gp_Pnt(10, 0, 10));
  CHECK(piece.rfind(face + "#", 0) == 0);
  const auto segment = [](double x) { return x * std::sqrt(400.0 - x * x) + 400.0 * std::asin(x / 20.0); };
  for (const double t : {1.0, 4.5}) {
    const ShapePtr in = offset_faces("F9", *halves, {piece}, -t);
    CHECK_NEAR(volume(*in), volume(*flat) - 20.0 * (segment(10.0) - segment(10.0 - t)));
    const ShapePtr out = offset_faces("F9", *halves, {piece}, t);
    CHECK_NEAR(volume(*out), volume(*flat) + 20.0 * (segment(10.0 + t) - segment(10.0)));
    for (const ShapePtr& moved : {in, out}) {
      CHECK(faces_named(*moved, piece) == 1);
      CHECK(faces_named(*moved, "F2:side(c1)") == 1);
      CHECK(steps_of(*moved, piece) == 1);
    }
  }
}

double solid_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The volume of `a` outside `b`, on copies (a boolean widens its inputs'
// tolerances).
double volume_outside(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  return solid_volume(BRepAlgoAPI_Cut(BRepBuilderAPI_Copy(a).Shape(), BRepBuilderAPI_Copy(b).Shape()).Shape());
}

// A piece of a rounding whose ends meet the next roundings at mitres
// (mitcad#1974): the block's top edges at y = 0, x = 0 and x = 60 rounded
// with r = 5, so that the rounding along y = 0 (its axis y = 5, z = 15)
// meets the other two at mitres in the planes x = y and x = 60 - y; the
// rounding split at z = 18 and the piece above, from its tangent edge to
// the top (an angle of 0 from the vertical about the axis) to the split
// (φ1 = acos 0.6), moved. Along each mitre the faces turn tangent at the
// top and meet at acos(cos²φ1) = 69° at the split.
//
// Inward the roundings that stay are trimmed back along their own
// cylinders to the piece's new position, up to the top, where the step
// along the piece's tangent edge closes against them: the result is the
// body less the part of the sector 0 ≤ φ ≤ φ1 about the piece's axis
// farther than r − t from it, exactly (the walls along the piece's normal
// at the mitres are gone). Outward a rounding that stays would have to
// extend past the line where it turns tangent to the top, where it has no
// surface to extend along; there the sides stay steps along the piece's
// normal, so the body gains the slab between r and r + t over
// 0 ≤ φ ≤ φ1, from x = r − r sin φ to 60 − r + r sin φ:
// ((r + t)² − r²) / 2 · (50 φ1 + 10 (1 − cos φ1)).
void test_offset_split_mitre() {
  const double r = 5.0;
  const std::string top = end_cap("F2", 1);
  const std::string front_edge = edge(top, side("F2", 1, 0));
  const ShapePtr rounded =
      fillet("F5", *block(), {front_edge, edge(top, side("F2", 1, 3)), edge(top, side("F2", 1, 1))}, r);
  const std::string front = "F5:fillet(" + front_edge + ")";
  CHECK(faces_named(*rounded, front) == 1);
  const Tool at_z18 = Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 18), gp_Dir(0, 0, 1)));
  const ShapePtr split = split_faces("F7", *rounded, {front}, at_z18);
  const double phi1 = std::acos(0.6);
  const std::string piece = face_at(*split, gp_Pnt(30, r - r * std::sin(0.3), 15 + r * std::cos(0.3)));
  CHECK(piece.rfind(front + "#", 0) == 0);
  // The sector 0 ≤ φ ≤ φ1 about the piece's axis, φ from the vertical
  // towards y = 0.
  const gp_Ax2 axis(gp_Pnt(-10, r, 20 - r), gp_Dir(1, 0, 0), gp_Dir(0, 0, 1));
  for (const double t : {1.0, 4.5}) {
    const ShapePtr in = offset_faces("F9", *split, {piece}, -t);
    const TopoDS_Shape far = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(axis, r + 1, 80, phi1).Shape(),
                                             BRepPrimAPI_MakeCylinder(axis, r - t, 80).Shape())
                                 .Shape();
    const TopoDS_Shape trimmed = BRepAlgoAPI_Cut(BRepBuilderAPI_Copy(split->occt()).Shape(), far).Shape();
    CHECK_NEAR(volume(*in), solid_volume(trimmed));
    CHECK(volume_outside(in->occt(), trimmed) < 1.0e-6);
    CHECK(volume_outside(trimmed, in->occt()) < 1.0e-6);
    CHECK(steps_of(*in, piece) == 2);

    const ShapePtr out = offset_faces("F9", *split, {piece}, t);
    CHECK_NEAR(volume(*out),
               volume(*split) + ((r + t) * (r + t) - r * r) / 2.0 * (50.0 * phi1 + 10.0 * (1.0 - std::cos(phi1))));
    CHECK(steps_of(*out, piece) == 4);
    for (const ShapePtr& moved : {in, out}) {
      CHECK(faces_named(*moved, piece) == 1);
      CHECK(faces_named(*moved, "F5:fillet(" + edge(top, side("F2", 1, 3)) + ")") == 1);
      CHECK(faces_named(*moved, "F5:fillet(" + edge(top, side("F2", 1, 1)) + ")") == 1);
      CHECK(edge_names_unique(*moved));
    }
  }

  // The same in a pocket: its floor edges at y = 10, x = 10 and x = 50
  // rounded with 3 (concave roundings, the one along y = 10 about the axis
  // y = 13, z = 13), split at z = 11.2 and the piece next to the floor,
  // from its tangent edge to the floor (ψ = 0 from straight down about the
  // axis) to the split (ψ1 = acos 0.6), moved. A concave rounding that stays
  // curves away from the piece's new position going in: it would have to
  // extend, so the sides at the mitres stay steps and the body loses the
  // slab between 3 and 3 + t over 0 ≤ ψ ≤ ψ1, from x = 13 − 3 sin ψ to
  // 47 + 3 sin ψ. Going out the roundings that stay are trimmed back to the
  // piece's new position: the body gains the part of the sector closer than
  // 3 to the axis and farther than 3 − t that was empty, exactly.
  const ShapePtr cutter = extrude("F4", {rectangle(5, 10, 10, 40, 20)}, 10, 21);
  const ShapePtr pocket = boolean(BooleanOp::Cut, {block().get()}, *cutter).pieces.at(0).shape;
  const std::string floor = start_cap("F4", 5);
  const std::string wall_edge = edge(floor, side("F4", 5, 0));
  const ShapePtr hollow = fillet(
      "F5", *pocket, {wall_edge, edge(floor, side("F4", 5, 3)), edge(floor, side("F4", 5, 1))}, 3);
  const std::string wall_rounding = "F5:fillet(" + wall_edge + ")";
  const Tool at_z = Tool::of_plane(gp_Pln(gp_Pnt(0, 0, 11.2), gp_Dir(0, 0, 1)));
  const ShapePtr cut = split_faces("F7", *hollow, {wall_rounding}, at_z);
  const std::string low = face_at(*cut, gp_Pnt(30, 13 - 3 * std::sin(0.3), 13 - 3 * std::cos(0.3)));
  CHECK(low.rfind(wall_rounding + "#", 0) == 0);
  const gp_Ax2 wall_axis(gp_Pnt(55, 13, 13), gp_Dir(-1, 0, 0), gp_Dir(0, 0, -1));
  for (const double t : {0.5, 1.0}) {
    const ShapePtr in = offset_faces("F9", *cut, {low}, -t);
    CHECK_NEAR(volume(*in), volume(*cut) - ((3 + t) * (3 + t) - 9) / 2.0 * (34.0 * phi1 + 6.0 * (1.0 - std::cos(phi1))));
    CHECK(steps_of(*in, low) == 4);

    const ShapePtr out = offset_faces("F9", *cut, {low}, t);
    const TopoDS_Shape slab = BRepAlgoAPI_Cut(BRepPrimAPI_MakeCylinder(wall_axis, 3, 50, phi1).Shape(),
                                              BRepPrimAPI_MakeCylinder(wall_axis, 3 - t, 50).Shape())
                                  .Shape();
    const TopoDS_Shape filled = BRepAlgoAPI_Fuse(BRepBuilderAPI_Copy(cut->occt()).Shape(), slab).Shape();
    CHECK_NEAR(volume(*out), solid_volume(filled));
    CHECK(volume_outside(out->occt(), filled) < 1.0e-6);
    CHECK(volume_outside(filled, out->occt()) < 1.0e-6);
    CHECK(steps_of(*out, low) == 2);
    for (const ShapePtr& moved : {in, out}) {
      CHECK(faces_named(*moved, low) == 1);
      CHECK(faces_named(*moved, "F5:fillet(" + edge(floor, side("F4", 5, 3)) + ")") == 1);
      CHECK(faces_named(*moved, "F5:fillet(" + edge(floor, side("F4", 5, 1)) + ")") == 1);
      CHECK(edge_names_unique(*moved));
    }
  }
}

// Offset Face with Tangent Chain (mitcad#2438): the front of the block,
// whose corner to the right side is rounded (5 mm), moved out by 2 mm takes
// the rounding and the right side along, as Replace Face takes them: the
// section grows to 62 x 42 with a 7 mm rounding. Without the chain the
// front moves alone and the seam to the rounding becomes a step.
void test_offset_tangent_chain() {
  const std::string front = side("F2", 1, 0);
  const std::string right = side("F2", 1, 1);
  const std::string corner = edge(front, right);
  const std::string rounding = "F3:fillet(" + corner + ")";
  const ShapePtr rounded = fillet("F3", *block(), {corner}, 5);
  const double cut = 1.0 - kPi / 4.0; // a rounded corner's loss per r²
  CHECK_NEAR(volume(*rounded), kBlock - 25.0 * cut * 20.0);

  const ShapePtr chained = offset_faces("F9", *rounded, {front}, 2, true);
  CHECK_NEAR(volume(*chained), (62.0 * 42.0 - 49.0 * cut) * 20.0);
  CHECK(chained->face_count() == rounded->face_count());
  for (const std::string& name : {front, right, rounding}) {
    CHECK(faces_named(*chained, name) == 1);
  }
  CHECK(steps_of(*chained, front) == 0);
  CHECK(edge_names_unique(*chained));
  const BoundingBox grown = bounding_box(*chained);
  CHECK(near(grown.min.Y(), -2) && near(grown.max.X(), 62) && near(grown.min.X(), 0) &&
        near(grown.max.Y(), 40));

  // Without the chain (the default, as stored offsets before mitcad#2438):
  // the front alone, 55 mm of it up to the rounding.
  const ShapePtr alone = offset_faces("F9", *rounded, {front}, 2);
  CHECK_NEAR(volume(*alone), volume(*rounded) + 55.0 * 2.0 * 20.0);
  CHECK(steps_of(*alone, front) == 1);
  CHECK(near(bounding_box(*alone).max.X(), 60));
  CHECK_NEAR(volume(*offset_faces("F9", *rounded, {front}, 2, false)), volume(*alone));
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
  // A target beside the body never meets it.
  CHECK(throws_with([&] {
    replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(200, 0, 30), "F9:side0"));
  },
                    "does not meet"));
  // One below the bottom at the sides: the region down to it is removed as
  // far as the body goes (mitcad#2381), and what stays is the block under
  // the target's upper side, z = sqrt(900 - (y - 20)^2) - 25, where that is
  // above the bottom (|y - 20| < sqrt(275)).
  const double reach = std::sqrt(275.0);
  const ShapePtr sunk =
      replace_faces("F5", *block(), {top}, Tool::of_face(cylinder_along_x(20, -25, 30), "F9:side0"));
  CHECK_NEAR_TOL(volume(*sunk), 60.0 * (circle_area(30, -reach, reach) - 50.0 * reach), 1e-6);
  CHECK(faces_named(*sunk, top) == 0);
  CHECK(faces_named(*sunk, name) == 1);
  CHECK(faces_named(*sunk, side("F2", 1, 0)) == 0);
  CHECK(faces_named(*sunk, side("F2", 1, 2)) == 0);
  CHECK(edge_names_unique(*sunk));
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

struct PlanarCase {
  ShapePtr body;
  std::string cap;
  std::string wall;
  gp_Dir outward;
  double area;
};

// Both ends exercise the face orientation as well as the planes' handedness.
std::vector<PlanarCase> left_handed_cases() {
  Affine mirror;
  mirror.linear = {{{-1, 0, 0}, {0, 1, 0}, {0, 0, 1}}};
  const ShapePtr mirrored = transform_shape(*block(), mirror);
  RevolveSpec spec;
  spec.feature = "F3";
  spec.regions = {rectangle(1, 0, 0, 20, 30)};
  spec.axis = gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0));
  spec.angle1 = 2 * kPi;
  const ShapePtr revolved = revolve(spec);
  return {{mirrored, start_cap("F2", 1), side("F2", 1, 0), gp_Dir(0, 0, -1), 60.0 * 40.0},
          {mirrored, end_cap("F2", 1), side("F2", 1, 0), gp_Dir(0, 0, 1), 60.0 * 40.0},
          {revolved, side("F3", 1, 0), side("F3", 1, 1), gp_Dir(0, -1, 0), kPi * 400},
          {revolved, side("F3", 1, 2), side("F3", 1, 1), gp_Dir(0, 1, 0), kPi * 400}};
}

void test_left_handed_face_extrusion() {
  bool indirect = false;
  for (const PlanarCase& sample : left_handed_cases()) {
    const TopoDS_Face& face = sample.body->face(sample.body->find_faces(sample.cap).at(0));
    indirect = indirect || !BRepAdaptor_Surface(face).Plane().Direct();
    const gp_Pln cap = face_plane(*sample.body, sample.cap);
    CHECK(cap.Axis().Direction().Dot(sample.outward) > 1 - 1e-6);
    FaceExtrudeSpec spec;
    spec.feature = "F7";
    spec.face = sample.cap;
    spec.key = sample.cap;
    spec.distance1 = 5;
    spec.start_offset = 2;
    const ShapePtr prism = extrude_face(*sample.body, spec);
    CHECK_NEAR(volume(*prism), sample.area * 5);
    CHECK_NEAR(gp_Vec(cap.Location(), mass_properties(*prism).center).Dot(gp_Vec(sample.outward)), 4.5);
    spec.reversed = true;
    const ShapePtr inward = extrude_face(*sample.body, spec);
    CHECK_NEAR(gp_Vec(cap.Location(), mass_properties(*inward).center).Dot(gp_Vec(sample.outward)), -0.5);
  }
  CHECK(indirect);
}

void test_left_handed_face_replacement() {
  for (const PlanarCase& sample : left_handed_cases()) {
    const gp_Pln cap = face_plane(*sample.body, sample.cap);
    const gp_Pnt target = cap.Location().Translated(gp_Vec(sample.outward) * 5);
    const ShapePtr taller = replace_faces("F7", *sample.body, {sample.cap},
                                         Tool::of_plane(gp_Pln(target, sample.outward)));
    CHECK_NEAR(volume(*taller), volume(*sample.body) + sample.area * 5);
    const gp_Pln made = face_plane(*taller, face_name("F7", "replace", sample.cap));
    CHECK_NEAR(gp_Vec(cap.Location(), made.Location()).Dot(gp_Vec(sample.outward)), 5);
  }
}

void test_left_handed_face_draft() {
  for (const PlanarCase& sample : left_handed_cases()) {
    const gp_Pln cap = face_plane(*sample.body, sample.cap);
    DraftSpec spec;
    spec.faces = {sample.wall};
    spec.plane = Tool::of_face(sample.body, sample.cap);
    spec.angle = 5 * kPi / 180;
    const ShapePtr from_face = draft("F7", *sample.body, spec);
    CHECK(volume(*from_face) < volume(*sample.body) - 1e-6);
    spec.plane = Tool::of_plane(gp_Pln(cap.Location(), sample.outward.Reversed()));
    const ShapePtr from_plane = draft("F7", *sample.body, spec);
    CHECK_NEAR(volume(*from_face), volume(*from_plane));
    CHECK(mass_properties(*from_face).center.Distance(mass_properties(*from_plane).center) < 1e-6);
  }
}

void test_left_handed_face_split_order() {
  for (const PlanarCase& sample : left_handed_cases()) {
    const gp_Pln cap = face_plane(*sample.body, sample.cap);
    const ShapePtr cube = extrude("F8", {rectangle(1, -5, -5, 10, 10)}, -5, 5);
    Affine move;
    move.translation = {cap.Location().X(), cap.Location().Y(), cap.Location().Z()};
    const ShapePtr across = transform_shape(*cube, move);
    const auto pieces = split_body("F7", *across, Tool::of_face(sample.body, sample.cap), true);
    CHECK(pieces.size() == 2);
    if (pieces.size() != 2) {
      continue;
    }
    CHECK_NEAR(volume(*pieces[0]), 500);
    CHECK_NEAR(volume(*pieces[1]), 500);
    CHECK_NEAR(gp_Vec(cap.Location(), mass_properties(*pieces[0]).center).Dot(gp_Vec(sample.outward)), -2.5);
    CHECK_NEAR(gp_Vec(cap.Location(), mass_properties(*pieces[1]).center).Dot(gp_Vec(sample.outward)), 2.5);
  }
}

// mitcad#2381: a pocket's floor replaced onto the outside face under it,
// or past it, removes the wall between: a through-hole with no face across
// the opening, the pocket's wall continued down to the bottom.
void test_replace_floor_through_the_body() {
  // The 10 mm pocket of the delete test, 10 deep: its floor at z = 10.
  const ShapePtr cutter = extrude("F4", {circle(5, 30, 20, 5)}, 10, 30);
  const ShapePtr pocket = boolean(BooleanOp::Cut, {block().get()}, *cutter).pieces.at(0).shape;
  const std::string floor = "F4:start(r{c5})";
  const std::string wall = "F4:side(c5)";
  const std::string name = "F5:replace(" + floor + ")";
  CHECK_NEAR(volume(*pocket), kBlock - kPi * 25.0 * 10.0);
  // The distance from the hole's axis to the body: the hole's radius at
  // every height when nothing closes the opening.
  const auto from_axis = [](const Shape& shape, double z) {
    BRepExtrema_DistShapeShape measure(BRepBuilderAPI_MakeVertex(gp_Pnt(30, 20, z)).Vertex(),
                                       shape.occt());
    return measure.IsDone() ? measure.Value() : -1.0;
  };
  const auto closed = [](const Shape& shape) {
    if (!BRepCheck_Analyzer(shape.occt()).IsValid()) {
      return false;
    }
    int shells = 0;
    for (TopExp_Explorer it(shape.occt(), TopAbs_SHELL); it.More(); it.Next()) {
      ++shells;
      if (!BRep_Tool::IsClosed(it.Current())) {
        return false;
      }
    }
    return shells > 0;
  };
  const struct {
    const char* what;
    Tool target;
  } through[] = {
      // Onto the bottom, a face of the body itself.
      {"onto the bottom", Tool::of_face(pocket, start_cap("F2", 1))},
      // Onto the bottom's plane, and onto a plane 5 mm below it.
      {"onto the bottom's plane", Tool::of_plane(plane(0, 0, 0, 0, 0, -1))},
      {"past the bottom", Tool::of_plane(plane(0, 0, -5, 0, 0, 1))},
  };
  for (const auto& sample : through) {
    std::fprintf(stderr, "  replacing the floor %s\n", sample.what);
    const ShapePtr open = replace_faces("F5", *pocket, {floor}, sample.target);
    CHECK_NEAR(volume(*open), kBlock - kPi * 25.0 * 20.0);
    CHECK(closed(*open));
    for (const double z : {-1.0, 0.0, 2.5, 5.0, 7.5, 10.0, 15.0, 20.0}) {
      CHECK(std::abs(from_axis(*open, z) - (z < 0.0 ? std::hypot(5.0, z) : 5.0)) < 1e-6);
    }
    CHECK(faces_named(*open, floor) == 0);
    CHECK(faces_named(*open, name) == 0);
    CHECK(faces_named(*open, wall) >= 1);
    CHECK(faces_named(*open, start_cap("F2", 1)) == 1);
    CHECK(faces_named(*open, end_cap("F2", 1)) == 1);
  }

  // Short of the bottom the floor moves down as before: a pocket 16 deep.
  const ShapePtr deeper =
      replace_faces("F5", *pocket, {floor}, Tool::of_plane(plane(0, 0, 4, 0, 0, 1)));
  CHECK_NEAR(volume(*deeper), kBlock - kPi * 25.0 * 16.0);
  CHECK(faces_named(*deeper, name) == 1);
  // The floor at z = 4: 2 below a point on the axis at z = 6 (the wall is
  // 5 away); the bottom under it stays closed.
  CHECK(std::abs(from_axis(*deeper, 6.0) - 2.0) < 1e-6);
  CHECK(from_axis(*deeper, 0.0) < 1e-6);
}

// The round top of a pin onto tilted planes (mitcad#2382, GitHub #643: a
// cylinder's top onto a slanted planar face failed with "the target removes
// the whole body"). The top has one vertex on its circle; moved only past
// the target there, it still crossed the target where the plane rises
// across the circle, and the cut left no piece without it. A pin of R = 10
// from z = 0 to 20 under planes rising 1 in 4 along y, 2.5 above and below
// their height at the axis on the circle: the pin keeps its section up to
// the plane, its volume pi R^2 times the plane's height at the axis, and its
// new top is the ellipse pi R^2 / cos(slope).
void test_replace_tilted_round_top() {
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  const std::string top = "F2:end(r{c5})";
  const std::string name = "F5:replace(" + top + ")";
  const gp_Dir slope(0, -1, 4);
  const double section = kPi * 100.0;
  const double ellipse = section * std::sqrt(17.0) / 4.0;
  // Above the top everywhere (z = 27.5 to 32.5), across it (17.5 to 22.5)
  // and below it everywhere (7.5 to 12.5).
  for (const double height : {30.0, 20.0, 10.0}) {
    const ShapePtr replaced =
        replace_faces("F5", *pin, {top}, Tool::of_plane(gp_Pln(gp_Pnt(0, 0, height), slope)), true);
    CHECK_NEAR_TOL(volume(*replaced), section * height, 1e-6);
    CHECK(replaced->face_count() == 3);
    CHECK(faces_named(*replaced, name) == 1);
    CHECK(faces_named(*replaced, top) == 0);
    CHECK(faces_named(*replaced, "F2:start(r{c5})") == 1);
    const std::vector<int> found = replaced->find_faces(name);
    if (found.size() == 1) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(replaced->face(found.front()), props);
      CHECK_NEAR_TOL(props.Mass(), ellipse, 1e-6);
    }
  }
}

// A face replaced onto a plane past a wall beside it (mitcad#2541, GitHub
// #717: on a shelled body, the inner ceiling 3 mm above the rim onto the
// rim, or the rim onto the ceiling, failed with "replace faces:
// NCollection_DataMap::Find"). The wall between them, as high as the move,
// shrinks to nothing, which the geometry kernel's offset cannot do; the
// region between the face and the target is added or removed, the wall
// goes with it and the face joins the target's face. The block with a
// 50 x 30 recess 3 deep cut into its bottom: the recess's ceiling at z = 3,
// its walls from z = 0 to 3, the bottom a 5 mm rim around it at z = 0.
void test_replace_past_a_wall() {
  const ShapePtr cutter = extrude("F4", {rectangle(5, 5, 5, 50, 30)}, -5, 3);
  const ShapePtr recessed = boolean(BooleanOp::Cut, {block().get()}, *cutter).pieces.at(0).shape;
  const double recess = 50.0 * 30.0 * 3.0;
  CHECK_NEAR(volume(*recessed), kBlock - recess);
  const std::string ceiling = end_cap("F4", 5);
  const std::string rim = start_cap("F2", 1);
  const auto valid = [](const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); };
  const auto no_walls = [](const Shape& shape) {
    int walls = 0;
    for (int i = 0; i < 4; ++i) {
      walls += faces_named(shape, side("F4", 5, i));
    }
    return walls == 0;
  };

  // The ceiling onto the rim, a face of the body itself, as reported (with
  // the tangent chain): the recess is filled, the block whole again, its
  // bottom one face.
  const ShapePtr filled = replace_faces("F5", *recessed, {ceiling}, Tool::of_face(recessed, rim), true);
  CHECK_NEAR(volume(*filled), kBlock);
  CHECK(valid(*filled));
  CHECK(filled->face_count() == 6);
  CHECK(no_walls(*filled));
  CHECK(faces_named(*filled, ceiling) == 0);
  CHECK(faces_named(*filled, rim) == 1);
  CHECK(faces_named(*filled, "F5:replace(" + ceiling + ")") == 1);

  // The rim onto the ceiling: the rim's 3 mm are removed, the block stands
  // from z = 3, its bottom one face.
  const ShapePtr lifted = replace_faces("F5", *recessed, {rim}, Tool::of_face(recessed, ceiling), true);
  CHECK_NEAR(volume(*lifted), 60.0 * 40.0 * 17.0);
  CHECK(valid(*lifted));
  CHECK(lifted->face_count() == 6);
  CHECK(no_walls(*lifted));
  CHECK(faces_named(*lifted, rim) == 0);
  CHECK(faces_named(*lifted, ceiling) == 1);
  CHECK(faces_named(*lifted, "F5:replace(" + rim + ")") == 1);
  CHECK(near(bounding_box(*lifted).min.Z(), 3.0));
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*lifted, side("F2", 1, i)) == 1);
  }

  // A pocket's floor onto the top: the pocket is filled.
  const ShapePtr pocketed =
      boolean(BooleanOp::Cut, {block().get()}, *extrude("F4", {circle(5, 30, 20, 5)}, 10, 30))
          .pieces.at(0)
          .shape;
  const std::string floor = "F4:start(r{c5})";
  const ShapePtr closed =
      replace_faces("F5", *pocketed, {floor}, Tool::of_face(pocketed, end_cap("F2", 1)), true);
  CHECK_NEAR(volume(*closed), kBlock);
  CHECK(valid(*closed));
  CHECK(closed->face_count() == 6);
  CHECK(faces_named(*closed, "F4:side(c5)") == 0);
  CHECK(faces_named(*closed, floor) == 0);

  // Short of the wall the ceiling moves as before (the walls shorten to
  // 1 mm): the recess 1 deep.
  const ShapePtr shallower =
      replace_faces("F5", *recessed, {ceiling}, Tool::of_plane(plane(0, 0, 1, 0, 0, -1)), true);
  CHECK_NEAR(volume(*shallower), kBlock - 50.0 * 30.0 * 1.0);
  CHECK(faces_named(*shallower, "F5:replace(" + ceiling + ")") == 1);
  CHECK(faces_named(*shallower, side("F4", 5, 0)) == 1);
}

// The end of a rib that sticks out of a pin replaced by the pin's own
// side, a B-spline (GitHub mitcad/Mitcad#674, mitcad#2538: the ends of ribs
// replaced by the rounding they stick out of). The new face lies on the
// side's surface continued, and the side moves onto that continuation too:
// the side and the new face are one face, without the outline of the end
// on it.
void test_replace_by_own_spline_face() {
  // A pin of R 10 along z (0..20) whose faces are B-splines, as imported
  // bodies have them.
  const ShapePtr pin = extrude("F2", {circle(5, 0, 0, 10)}, 0, 20);
  BRepBuilderAPI_NurbsConvert nurbs(pin->occt(), true);
  std::vector<Shape::NamedFace> named;
  for (int i = 0; i < pin->face_count(); ++i) {
    named.push_back({nurbs.ModifiedShape(pin->face(i)), pin->face_names(i)});
  }
  const Shape spline_pin(nurbs.Shape(), named);
  // A rib through its side, away from the side's seam: x -2..2, y 5..15,
  // z 5..15, its end at y = 15.
  const ShapePtr rib = extrude("F3", {rectangle(9, -2, 5, 4, 10)}, 5, 15);
  const BooleanResult joined = boolean(BooleanOp::Join, {&spline_pin}, *rib);
  CHECK(joined.pieces.size() == 1);
  if (joined.pieces.size() != 1) {
    return;
  }
  const ShapePtr ribbed = joined.pieces.front().shape;
  const std::string end = side("F3", 9, 2);
  CHECK(faces_named(*ribbed, end) == 1);
  CHECK(faces_named(*ribbed, "F2:side(c5)") == 1);

  const ShapePtr flush = replace_faces("F5", *ribbed, {end}, Tool::of_face(ribbed, "F2:side(c5)"));
  // The rib now ends at the side: the pin's volume.
  CHECK_NEAR_TOL(volume(*flush), kPi * 100.0 * 20.0, 1e-5);
  CHECK(BRepCheck_Analyzer(flush->occt()).IsValid());
  CHECK(faces_named(*flush, end) == 0);
  // One face holds the side's name and the new face's.
  const std::string replaced = "F5:replace(" + end + ")";
  const std::vector<int> found = flush->find_faces(replaced);
  CHECK(found.size() == 1);
  if (found.size() == 1) {
    CHECK(has_name(flush->face_names(found.front()), "F2:side(c5)"));
  }
  CHECK(faces_named(*flush, "F2:side(c5)") == 1);
}

} // namespace

void faceops_tests() {
  guarded("test_shell", test_shell);
  guarded("test_draft", test_draft);
  guarded("test_offset_faces", test_offset_faces);
  guarded("test_offset_split_faces", test_offset_split_faces);
  guarded("test_offset_split_corners", test_offset_split_corners);
  guarded("test_offset_split_slanted_neighbours", test_offset_split_slanted_neighbours);
  guarded("test_offset_split_mitre", test_offset_split_mitre);
  guarded("test_offset_tangent_chain", test_offset_tangent_chain);
  guarded("test_delete_faces", test_delete_faces);
  guarded("test_replace_faces", test_replace_faces);
  guarded("test_replace_curved_faces", test_replace_curved_faces);
  guarded("test_replace_curved_targets", test_replace_curved_targets);
  guarded("test_left_handed_face_extrusion", test_left_handed_face_extrusion);
  guarded("test_left_handed_face_replacement", test_left_handed_face_replacement);
  guarded("test_left_handed_face_draft", test_left_handed_face_draft);
  guarded("test_left_handed_face_split_order", test_left_handed_face_split_order);
  guarded("test_replace_tilted_round_top", test_replace_tilted_round_top);
  guarded("test_draft_about_a_moved_plane", test_draft_about_a_moved_plane);
  guarded("test_replace_floor_through_the_body", test_replace_floor_through_the_body);
  guarded("test_replace_past_a_wall", test_replace_past_a_wall);
  // A face replaced by a B-spline face of the same body merges with it
  // (GitHub mitcad/Mitcad#674, mitcad#2538).
  guarded("test_replace_by_own_spline_face", test_replace_by_own_spline_face);
}

} // namespace test
