// SPDX-License-Identifier: MIT
// A planar face of a body extruded as a profile (face_extrude.hpp,
// mitcad#535): a block's top out of it, into it, to two sides and from an
// offset; an annular face with its hole; an oblique face along its own
// normal; faces that are not planar or not there refused; sides up to a
// target (mitcad#2033).

#include <cmath>
#include <initializer_list>
#include <string>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/face_extrude.hpp"
#include "mitcad/geometry/naming.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

double volume_of(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass();
}

gp_Pnt centre_of(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.CentreOfMass();
}

// The box of the shape, exact to its geometry (no tolerance added).
void bounds(const TopoDS_Shape& shape, double& x0, double& y0, double& z0, double& x1, double& y1,
            double& z1) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(shape, box, false, false);
  box.Get(x0, y0, z0, x1, y1, z1);
}

// The middle of the face named `name` (the centre of its area).
gp_Pnt face_middle(const Shape& shape, const std::string& name) {
  for (int f = 0; f < shape.face_count(); ++f) {
    if (has_name(shape.face_names(f), name)) {
      GProp_GProps props;
      BRepGProp::SurfaceProperties(shape.face(f), props);
      return props.CentreOfMass();
    }
  }
  return gp_Pnt(1e9, 1e9, 1e9);
}

FaceExtrudeSpec spec_of(const std::string& face, double distance1) {
  FaceExtrudeSpec spec;
  spec.feature = "F3";
  spec.face = face;
  spec.key = face;
  spec.distance1 = distance1;
  return spec;
}

// A 60 x 40 x 20 block's top extruded 5 out of it, 5 into it, 5 and 3 to
// two sides and 5 from 2 above it; its edges name the sides, the top the
// caps; the block stays as it was.
void test_block_top_extruded() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top = end_cap("F2", 1);
  const std::string start = face_name("F3", "start", top);
  const std::string end = face_name("F3", "end", top);
  double x0, y0, z0, x1, y1, z1;

  const ShapePtr out = extrude_face(*block, spec_of(top, 5.0));
  CHECK(BRepCheck_Analyzer(out->occt()).IsValid());
  CHECK_NEAR(volume_of(*out), 60.0 * 40.0 * 5.0);
  CHECK(out->face_count() == 6);
  bounds(out->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(x0, 0.0, 1e-7);
  CHECK_NEAR_TOL(x1, 60.0, 1e-7);
  CHECK_NEAR_TOL(y0, 0.0, 1e-7);
  CHECK_NEAR_TOL(y1, 40.0, 1e-7);
  CHECK_NEAR_TOL(z0, 20.0, 1e-7);
  CHECK_NEAR_TOL(z1, 25.0, 1e-7);
  CHECK(faces_named(*out, start) == 1);
  CHECK(faces_named(*out, end) == 1);
  CHECK_NEAR_TOL(face_middle(*out, start).Z(), 20.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*out, end).Z(), 25.0, 1e-7);
  for (int i = 0; i < 4; ++i) {
    CHECK(faces_named(*out, face_name("F3", "side", edge(top, side("F2", 1, i)))) == 1);
  }
  CHECK(edge_names_unique(*out));

  // Into the block (a cut's tool).
  FaceExtrudeSpec into = spec_of(top, 5.0);
  into.reversed = true;
  const ShapePtr in = extrude_face(*block, into);
  CHECK(BRepCheck_Analyzer(in->occt()).IsValid());
  CHECK_NEAR(volume_of(*in), 60.0 * 40.0 * 5.0);
  bounds(in->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 15.0, 1e-7);
  CHECK_NEAR_TOL(z1, 20.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*in, start).Z(), 20.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*in, end).Z(), 15.0, 1e-7);

  // Two sides: the far cap of side one is the start, of side two the end.
  FaceExtrudeSpec two = spec_of(top, 5.0);
  two.distance2 = 3.0;
  const ShapePtr both = extrude_face(*block, two);
  CHECK(BRepCheck_Analyzer(both->occt()).IsValid());
  CHECK_NEAR(volume_of(*both), 60.0 * 40.0 * 8.0);
  bounds(both->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 17.0, 1e-7);
  CHECK_NEAR_TOL(z1, 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, start).Z(), 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, end).Z(), 17.0, 1e-7);

  // From 2 above the face.
  FaceExtrudeSpec offset = spec_of(top, 5.0);
  offset.start_offset = 2.0;
  const ShapePtr above = extrude_face(*block, offset);
  bounds(above->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 22.0, 1e-7);
  CHECK_NEAR_TOL(z1, 27.0, 1e-7);

  CHECK_NEAR(volume_of(*block), 60.0 * 40.0 * 20.0);
  CHECK(block->face_count() == 6);
}

// A ring's top (a circle of radius 10 with a hole of radius 4) extruded 5:
// the hole goes through the prism, which has a side from each circle.
void test_annular_face_keeps_its_hole() {
  Region ring = circle(1, 0, 0, 10);
  ring.loops.push_back(circle(2, 0, 0, 4).loops.front());
  const ShapePtr body = extrude("F2", {ring}, 0, 10);
  const std::string top = face_name("F2", "end", "r{c1}");
  CHECK(faces_named(*body, top) == 1);
  const ShapePtr prism = extrude_face(*body, spec_of(top, 5.0));
  CHECK(BRepCheck_Analyzer(prism->occt()).IsValid());
  CHECK_NEAR(volume_of(*prism), kPi * (10.0 * 10.0 - 4.0 * 4.0) * 5.0);
  CHECK(prism->face_count() == 4);
  CHECK(faces_named(*prism, face_name("F3", "side", edge(top, face_name("F2", "side", "c1")))) == 1);
  CHECK(faces_named(*prism, face_name("F3", "side", edge(top, face_name("F2", "side", "c2")))) == 1);
  double x0, y0, z0, x1, y1, z1;
  bounds(prism->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(x0, -10.0, 1e-7);
  CHECK_NEAR_TOL(x1, 10.0, 1e-7);
  CHECK_NEAR_TOL(z0, 10.0, 1e-7);
  CHECK_NEAR_TOL(z1, 15.0, 1e-7);
  // The centre of mass of a ring lies in its hole.
  const gp_Pnt centre = centre_of(*prism);
  CHECK_NEAR_TOL(centre.X(), 0.0, 1e-7);
  CHECK_NEAR_TOL(centre.Y(), 0.0, 1e-7);
  CHECK_NEAR_TOL(centre.Z(), 12.5, 1e-7);
}

// A 20 x 10 plate on a plane tilted 30 degrees about X, 8 thick along its
// normal: its far face extruded 5 goes along that face's own normal.
void test_oblique_face_uses_its_own_normal() {
  const double a = 30.0 * kPi / 180.0;
  ExtrudeSpec plate;
  plate.feature = "F2";
  plate.frame.x_axis = gp_Dir(1.0, 0.0, 0.0);
  plate.frame.y_axis = gp_Dir(0.0, std::cos(a), std::sin(a));
  plate.regions = {rectangle(1, 0, 0, 20, 10)};
  plate.direction = plate.frame.normal();
  plate.start = 0.0;
  plate.end = 8.0;
  const ShapePtr body = mitcad::geometry::extrude(plate);
  const gp_Dir normal = plate.frame.normal();
  const std::string farCap = end_cap("F2", 1);
  // The far face's middle: the rectangle's middle moved 8 along the normal.
  const gp_Pnt middle = plate.frame.point(gp_Pnt2d(10.0, 5.0)).Translated(gp_Vec(normal) * 8.0);
  CHECK(face_middle(*body, farCap).Distance(middle) < 1e-7);

  const ShapePtr prism = extrude_face(*body, spec_of(farCap, 5.0));
  CHECK(BRepCheck_Analyzer(prism->occt()).IsValid());
  CHECK_NEAR(volume_of(*prism), 20.0 * 10.0 * 5.0);
  CHECK(prism->face_count() == 6);
  CHECK(centre_of(*prism).Distance(middle.Translated(gp_Vec(normal) * 2.5)) < 1e-7);
  CHECK(face_middle(*prism, face_name("F3", "start", farCap)).Distance(middle) < 1e-7);
  CHECK(face_middle(*prism, face_name("F3", "end", farCap)).Distance(middle.Translated(gp_Vec(normal) * 5.0)) <
        1e-7);
}

// A rod's side is no plane; a name the body has no face of, a distance of
// zero: errors, nothing made.
void test_refusals() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  CHECK(throws_with([&] { extrude_face(*rod, spec_of(face_name("F2", "side", "c1"), 5.0)); },
                    "is not planar"));
  CHECK(throws_with([&] { extrude_face(*rod, spec_of(face_name("F2", "top"), 5.0)); }, "has no face"));
  CHECK(throws_with([&] { extrude_face(*rod, spec_of(face_name("F2", "end", "r{c1}"), 0.0)); },
                    "must be greater than zero"));
}

// mitcad#541: tapers, directions and curved faces.

// The volume of a 60 x 40 rectangle tapered by t = tan(taper) over the
// height h: its sides grow by x t each at height x.
double tapered_block(double h, double t) {
  return 2400.0 * h + 100.0 * t * h * h + 4.0 / 3.0 * t * t * h * h * h;
}

// A 60 x 40 x 20 block's top extruded 5 with a taper of 5 degrees (wider
// away from the top), then 5 at +5 degrees and 3 at -5 degrees to two
// sides; the caps keep their names and the face between the sides goes.
void test_tapered_face() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top = end_cap("F2", 1);
  const std::string start = face_name("F3", "start", top);
  const std::string end = face_name("F3", "end", top);
  const double taper = 5.0 * kPi / 180.0;
  const double t = std::tan(taper);
  double x0, y0, z0, x1, y1, z1;

  FaceExtrudeSpec one = spec_of(top, 5.0);
  one.taper1 = taper;
  const ShapePtr out = extrude_face(*block, one);
  CHECK(BRepCheck_Analyzer(out->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*out), tapered_block(5.0, t), 1e-6);
  bounds(out->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(x0, -5.0 * t, 1e-7);
  CHECK_NEAR_TOL(x1, 60.0 + 5.0 * t, 1e-7);
  CHECK_NEAR_TOL(z0, 20.0, 1e-7);
  CHECK_NEAR_TOL(z1, 25.0, 1e-7);
  CHECK(faces_named(*out, start) == 1);
  CHECK(faces_named(*out, end) == 1);
  CHECK_NEAR_TOL(face_middle(*out, end).Z(), 25.0, 1e-7);

  FaceExtrudeSpec two = spec_of(top, 5.0);
  two.distance2 = 3.0;
  two.taper1 = taper;
  two.taper2 = -taper;
  const ShapePtr both = extrude_face(*block, two);
  CHECK(BRepCheck_Analyzer(both->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*both), tapered_block(5.0, t) + tapered_block(3.0, -t), 1e-6);
  bounds(both->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 17.0, 1e-7);
  CHECK_NEAR_TOL(z1, 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, start).Z(), 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, end).Z(), 17.0, 1e-7);
  CHECK(faces_named(*both, face_name("F3", "mid", top)) == 0);
  CHECK(edge_names_unique(*both));

  // mitcad#765: 5 and -3 lie 3 to 5 above the top, side one tapered from
  // the top as without the other's end (side two's taper shapes nothing):
  // the tapered block of 5 less that of 3. The cut at 3 is side two's end.
  FaceExtrudeSpec past = two;
  past.distance2 = -3.0;
  const ShapePtr above = extrude_face(*block, past);
  CHECK(BRepCheck_Analyzer(above->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*above), tapered_block(5.0, t) - tapered_block(3.0, t), 1e-6);
  bounds(above->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 23.0, 1e-7);
  CHECK_NEAR_TOL(z1, 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*above, start).Z(), 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*above, end).Z(), 23.0, 1e-7);
  CHECK(faces_named(*above, face_name("F3", "mid", top)) == 0);
  // Untapered: one prism between the ends.
  FaceExtrudeSpec straight = spec_of(top, 5.0);
  straight.distance2 = -3.0;
  const ShapePtr slab = extrude_face(*block, straight);
  CHECK_NEAR(volume_of(*slab), 60.0 * 40.0 * 2.0);
  CHECK_NEAR_TOL(face_middle(*slab, start).Z(), 25.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*slab, end).Z(), 23.0, 1e-7);
  // Ends that meet are refused.
  straight.distance2 = -5.0;
  CHECK(throws_with([&] { extrude_face(*block, straight); }, "overlap"));

  CHECK_NEAR(volume_of(*block), 60.0 * 40.0 * 20.0);
}

// The block's top extruded 5 along (0, 1, 1): a slanted prism of the
// top's area times 5 cos 45 degrees, its centre 2.5 along the direction
// from the top's middle; tapered along the direction, its top grows in
// the top's plane by 5 tan(taper).
void test_face_along_a_direction() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const std::string top = end_cap("F2", 1);
  const gp_Dir along(0.0, 1.0, 1.0);
  const double c = std::sqrt(0.5);
  FaceExtrudeSpec slanted = spec_of(top, 5.0);
  slanted.direction = along;
  const ShapePtr out = extrude_face(*block, slanted);
  CHECK(BRepCheck_Analyzer(out->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*out), 2400.0 * 5.0 * c, 1e-6);
  const gp_Pnt middle(30.0, 20.0, 20.0);
  CHECK(centre_of(*out).Distance(middle.Translated(gp_Vec(along) * 2.5)) < 1e-7);
  CHECK(face_middle(*out, face_name("F3", "end", top)).Distance(middle.Translated(gp_Vec(along) * 5.0)) <
        1e-7);

  // Reversed: into the block along -(0, 1, 1).
  FaceExtrudeSpec into = slanted;
  into.reversed = true;
  const ShapePtr in = extrude_face(*block, into);
  CHECK(centre_of(*in).Distance(middle.Translated(gp_Vec(along) * -2.5)) < 1e-7);

  // Tapered along the direction: the same height, the top grown in its
  // plane by 5 tan(taper) (the volume of the upright taper over the
  // height 5 c with the tangent over c).
  const double taper = 5.0 * kPi / 180.0;
  FaceExtrudeSpec tapered = slanted;
  tapered.taper1 = taper;
  const ShapePtr grown = extrude_face(*block, tapered);
  CHECK(BRepCheck_Analyzer(grown->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*grown), tapered_block(5.0 * c, std::tan(taper) / c), 1e-6);
  double x0, y0, z0, x1, y1, z1;
  bounds(grown->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z1, 20.0 + 5.0 * c, 1e-7);
  CHECK_NEAR_TOL(x1, 60.0 + 5.0 * std::tan(taper), 1e-7);

  // A direction in the face's plane.
  FaceExtrudeSpec flat = spec_of(top, 5.0);
  flat.direction = gp_Dir(1.0, 0.0, 0.0);
  CHECK(throws_with([&] { extrude_face(*block, flat); }, "lies in the plane"));
}

// A quarter of a disc of radius 5 extruded 10: its arc's side, a quarter
// of a cylinder, extruded 5 along (1, 1, 0): a prism whose cross-section
// across the direction is the arc's chord r sqrt(2) by 10. Without a
// direction, or tapered, it is refused.
void test_curved_face_along_a_direction() {
  Region quarter;
  quarter.name = "r{c1,c2,c3}";
  Loop loop;
  Segment a;
  a.name = "c1";
  a.kind = SegmentKind::Line;
  a.start = gp_Pnt2d(0.0, 0.0);
  a.end = gp_Pnt2d(5.0, 0.0);
  Segment arc;
  arc.name = "c2";
  arc.kind = SegmentKind::Arc;
  arc.start = gp_Pnt2d(5.0, 0.0);
  arc.end = gp_Pnt2d(0.0, 5.0);
  arc.center = gp_Pnt2d(0.0, 0.0);
  arc.radius = 5.0;
  arc.start_angle = 0.0;
  arc.end_angle = kPi / 2.0;
  Segment b;
  b.name = "c3";
  b.kind = SegmentKind::Line;
  b.start = gp_Pnt2d(0.0, 5.0);
  b.end = gp_Pnt2d(0.0, 0.0);
  loop.segments = {a, arc, b};
  quarter.loops = {loop};
  const ShapePtr body = extrude("F2", {quarter}, 0, 10);
  const std::string curved = face_name("F2", "side", "c2");
  CHECK(faces_named(*body, curved) == 1);

  FaceExtrudeSpec along = spec_of(curved, 5.0);
  along.direction = gp_Dir(1.0, 1.0, 0.0);
  const ShapePtr out = extrude_face(*body, along);
  CHECK(BRepCheck_Analyzer(out->occt()).IsValid());
  CHECK_NEAR_TOL(volume_of(*out), 5.0 * std::sqrt(2.0) * 10.0 * 5.0, 1e-6);
  CHECK(faces_named(*out, face_name("F3", "start", curved)) == 1);
  CHECK(faces_named(*out, face_name("F3", "end", curved)) == 1);

  CHECK(throws_with([&] { extrude_face(*body, spec_of(curved, 5.0)); }, "is not planar"));
  FaceExtrudeSpec tapered = along;
  tapered.taper1 = 0.1;
  CHECK(throws_with([&] { extrude_face(*body, tapered); }, "is not planar"));
}

// mitcad#2033: sides up to a target.

// A 60 x 40 x 20 block with a step x = 30..60 up to z = 30, joined. Its
// lower top (x = 0..30 at z = 20) is `end_cap("F2", 1)`, the step's top
// `end_cap("F4", 5)`.
ShapePtr stepped_block() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const ShapePtr step = extrude("F4", {rectangle(5, 30, 0, 30, 40)}, 20, 30);
  return boolean(BooleanOp::Join, {block.get()}, *step).pieces.at(0).shape;
}

// The stepped block's lower top extruded up to the step's top, its
// surface continued (To Selected Face): a 30 x 40 x 10 prism whose far cap
// is the target's, and joined into the block one level body 60 x 40 x 30;
// the block stays as it was. With an offset of 2 it goes past the face, 3
// short of it with -3; up to a plane.
void test_lower_step_up_to_the_higher_top() {
  const ShapePtr body = stepped_block();
  const std::string lower = end_cap("F2", 1);
  const std::string higher = end_cap("F4", 5);
  CHECK(faces_named(*body, lower) == 1);
  CHECK(faces_named(*body, higher) == 1);
  const std::string start = face_name("F3", "start", lower);
  const std::string end = face_name("F3", "end", lower);
  double x0, y0, z0, x1, y1, z1;

  Target top;
  top.kind = Target::Kind::Face;
  top.body = body;
  top.face = higher;
  FaceExtrudeSpec up = spec_of(lower, 0.0);
  up.target1 = top;
  const ShapePtr prism = extrude_face(*body, up);
  CHECK(BRepCheck_Analyzer(prism->occt()).IsValid());
  CHECK_NEAR(volume_of(*prism), 30.0 * 40.0 * 10.0);
  bounds(prism->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(x0, 0.0, 1e-7);
  CHECK_NEAR_TOL(x1, 30.0, 1e-7);
  CHECK_NEAR_TOL(z0, 20.0, 1e-7);
  CHECK_NEAR_TOL(z1, 30.0, 1e-7);
  CHECK(faces_named(*prism, start) == 1);
  CHECK(faces_named(*prism, end) == 1);
  CHECK_NEAR_TOL(face_middle(*prism, start).Z(), 20.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*prism, end).Z(), 30.0, 1e-7);
  CHECK(faces_named(*prism, face_name("F3", "side", edge(lower, side("F2", 1, 3)))) == 1);

  // Joined: one level body.
  const BooleanResult joined = boolean(BooleanOp::Join, {body.get()}, *prism);
  CHECK(joined.pieces.size() == 1);
  const ShapePtr level = joined.pieces.at(0).shape;
  CHECK(BRepCheck_Analyzer(level->occt()).IsValid());
  CHECK_NEAR(volume_of(*level), 60.0 * 40.0 * 30.0);
  bounds(level->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 0.0, 1e-7);
  CHECK_NEAR_TOL(z1, 30.0, 1e-7);
  CHECK_NEAR(volume_of(*body), 60.0 * 40.0 * 20.0 + 30.0 * 40.0 * 10.0);

  // Past the face by 2, short of it by 3.
  for (const double offset : {2.0, -3.0}) {
    FaceExtrudeSpec moved = up;
    moved.target1->offset = offset;
    const ShapePtr longer = extrude_face(*body, moved);
    CHECK(BRepCheck_Analyzer(longer->occt()).IsValid());
    CHECK_NEAR(volume_of(*longer), 30.0 * 40.0 * (10.0 + offset));
    CHECK_NEAR_TOL(face_middle(*longer, end).Z(), 30.0 + offset, 1e-7);
  }

  // Up to a plane at z = 25.
  Target plane;
  plane.kind = Target::Kind::Plane;
  plane.plane = gp_Pln(gp_Pnt(0.0, 0.0, 25.0), gp_Dir(0.0, 0.0, 1.0));
  FaceExtrudeSpec to_plane = spec_of(lower, 0.0);
  to_plane.target1 = plane;
  const ShapePtr slab = extrude_face(*body, to_plane);
  CHECK_NEAR(volume_of(*slab), 30.0 * 40.0 * 5.0);
  CHECK_NEAR_TOL(face_middle(*slab, end).Z(), 25.0, 1e-7);

  // A target behind the face is refused.
  Target below = plane;
  below.plane = gp_Pln(gp_Pnt(0.0, 0.0, 10.0), gp_Dir(0.0, 0.0, 1.0));
  FaceExtrudeSpec behind = spec_of(lower, 0.0);
  behind.target1 = below;
  CHECK(throws_with([&] { extrude_face(*body, behind); }, "behind"));
}

// Two sides with one up to the step's top: the other a distance of 5 down
// into the block (side one's far cap the start, side two's the end, the
// face between them gone), or -4, ending on the target's side 4 above the
// face; tapered by 5 degrees up to a plane.
void test_two_sides_and_a_taper_up_to_a_target() {
  const ShapePtr body = stepped_block();
  const std::string lower = end_cap("F2", 1);
  const std::string start = face_name("F3", "start", lower);
  const std::string end = face_name("F3", "end", lower);
  Target top;
  top.kind = Target::Kind::Face;
  top.body = body;
  top.face = end_cap("F4", 5);
  double x0, y0, z0, x1, y1, z1;

  FaceExtrudeSpec two = spec_of(lower, 0.0);
  two.target1 = top;
  two.distance2 = 5.0;
  const ShapePtr both = extrude_face(*body, two);
  CHECK(BRepCheck_Analyzer(both->occt()).IsValid());
  CHECK_NEAR(volume_of(*both), 30.0 * 40.0 * 15.0);
  bounds(both->occt(), x0, y0, z0, x1, y1, z1);
  CHECK_NEAR_TOL(z0, 15.0, 1e-7);
  CHECK_NEAR_TOL(z1, 30.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, start).Z(), 30.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*both, end).Z(), 15.0, 1e-7);
  CHECK(faces_named(*both, face_name("F3", "mid", lower)) == 0);

  FaceExtrudeSpec past = two;
  past.distance2 = -4.0;
  const ShapePtr above = extrude_face(*body, past);
  CHECK(BRepCheck_Analyzer(above->occt()).IsValid());
  CHECK_NEAR(volume_of(*above), 30.0 * 40.0 * 6.0);
  CHECK_NEAR_TOL(face_middle(*above, start).Z(), 30.0, 1e-7);
  CHECK_NEAR_TOL(face_middle(*above, end).Z(), 24.0, 1e-7);

  // Tapered by 5 degrees up to the plane z = 30: the 30 x 40 face grows by
  // h tan(taper) on each side at height h.
  const double t = std::tan(5.0 * kPi / 180.0);
  Target plane;
  plane.kind = Target::Kind::Plane;
  plane.plane = gp_Pln(gp_Pnt(0.0, 0.0, 30.0), gp_Dir(0.0, 0.0, 1.0));
  FaceExtrudeSpec tapered = spec_of(lower, 0.0);
  tapered.target1 = plane;
  tapered.taper1 = 5.0 * kPi / 180.0;
  const ShapePtr grown = extrude_face(*body, tapered);
  CHECK(BRepCheck_Analyzer(grown->occt()).IsValid());
  const double h = 10.0;
  CHECK_NEAR_TOL(volume_of(*grown), 1200.0 * h + 70.0 * t * h * h + 4.0 / 3.0 * t * t * h * h * h, 1e-6);
  CHECK_NEAR_TOL(face_middle(*grown, end).Z(), 30.0, 1e-7);
}

// A curved face along a direction up to a plane: a quarter cylinder's arc
// side along +x to the plane x = 10, each line from the arc to the plane;
// a side that ends on the other side's side is refused for it.
void test_curved_face_up_to_a_plane() {
  Region quarter;
  quarter.name = "r{c1,c2,c3}";
  Loop loop;
  Segment a;
  a.name = "c1";
  a.kind = SegmentKind::Line;
  a.start = gp_Pnt2d(0.0, 0.0);
  a.end = gp_Pnt2d(5.0, 0.0);
  Segment arc;
  arc.name = "c2";
  arc.kind = SegmentKind::Arc;
  arc.start = gp_Pnt2d(5.0, 0.0);
  arc.end = gp_Pnt2d(0.0, 5.0);
  arc.center = gp_Pnt2d(0.0, 0.0);
  arc.radius = 5.0;
  arc.start_angle = 0.0;
  arc.end_angle = kPi / 2.0;
  Segment b;
  b.name = "c3";
  b.kind = SegmentKind::Line;
  b.start = gp_Pnt2d(0.0, 5.0);
  b.end = gp_Pnt2d(0.0, 0.0);
  loop.segments = {a, arc, b};
  quarter.loops = {loop};
  const ShapePtr body = extrude("F2", {quarter}, 0, 10);
  const std::string curved = face_name("F2", "side", "c2");

  Target plane;
  plane.kind = Target::Kind::Plane;
  plane.plane = gp_Pln(gp_Pnt(10.0, 0.0, 0.0), gp_Dir(1.0, 0.0, 0.0));
  FaceExtrudeSpec along = spec_of(curved, 0.0);
  along.direction = gp_Dir(1.0, 0.0, 0.0);
  along.target1 = plane;
  const ShapePtr out = extrude_face(*body, along);
  CHECK(BRepCheck_Analyzer(out->occt()).IsValid());
  // 10 high, over y = 0..5 from x = sqrt(25 - y^2) to 10.
  CHECK_NEAR_TOL(volume_of(*out), 10.0 * (50.0 - 25.0 * kPi / 4.0), 1e-6);
  CHECK(faces_named(*out, face_name("F3", "end", curved)) == 1);

  FaceExtrudeSpec short_side = along;
  short_side.distance2 = -2.0;
  CHECK(throws_with([&] { extrude_face(*body, short_side); }, "ends on the other side"));
}

} // namespace

void face_extrude_tests() {
  guarded("test_block_top_extruded", test_block_top_extruded);
  guarded("test_annular_face_keeps_its_hole", test_annular_face_keeps_its_hole);
  guarded("test_oblique_face_uses_its_own_normal", test_oblique_face_uses_its_own_normal);
  guarded("test_refusals", test_refusals);
  guarded("test_tapered_face", test_tapered_face);
  guarded("test_face_along_a_direction", test_face_along_a_direction);
  guarded("test_curved_face_along_a_direction", test_curved_face_along_a_direction);
  guarded("test_lower_step_up_to_the_higher_top", test_lower_step_up_to_the_higher_top);
  guarded("test_two_sides_and_a_taper_up_to_a_target", test_two_sides_and_a_taper_up_to_a_target);
  guarded("test_curved_face_up_to_a_plane", test_curved_face_up_to_a_plane);
}

} // namespace test
