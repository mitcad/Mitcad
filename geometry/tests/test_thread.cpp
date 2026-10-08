// SPDX-License-Identifier: MIT
// Modelled threads: the volume against the profile's exact one (bolts and
// tapped holes, a class's diameters and the basic ones), where the helix
// starts, ends at shoulders and partial lengths, names, and the speed on a
// body of thousands of faces.

#include <chrono>
#include <cmath>
#include <cstdio>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Builder.hxx>
#include <GProp_GProps.hxx>
#include <TopoDS_Compound.hxx>

#include "../src/history.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kTan30 = 0.57735026918962576451;
constexpr double kH = 0.8660254037844386; // H / P

struct Diameters {
  double major;
  double minor;
  double pitch;
};

// The basic profile of a size (ISO 68-1).
Diameters basic(double d, double pitch) { return {d, d - 1.25 * kH * pitch, d - 0.75 * kH * pitch}; }

// M10x1.5 of class 6g and 6H: the middles of the tolerances, as the
// reference models store them.
constexpr Diameters k6g{9.85, 8.141, 8.928};
constexpr Diameters k6H{10.198, 8.526, 9.116};

ThreadSpec spec_of(const std::string& feature, const std::string& face, double pitch, const Diameters& d) {
  ThreadSpec spec;
  spec.feature = feature;
  spec.faces = {face};
  spec.pitch = pitch;
  spec.major = d.major;
  spec.minor = d.minor;
  spec.pitch_diameter = d.pitch;
  return spec;
}

// The material of the thread per millimetre of its length between the
// minor and major diameters: the teeth (an external thread's narrowing
// outwards from P/2 at the pitch diameter, an internal thread's widening)
// swept round the axis, integrated exactly.
double teeth_per_mm(double pitch, const Diameters& d, bool internal) {
  const double rp = d.pitch / 2.0;
  const double a = d.minor / 2.0;
  const double b = d.major / 2.0;
  // width(r) = c0 + c1 r
  const double c1 = internal ? 2.0 * kTan30 : -2.0 * kTan30;
  const double c0 = pitch / 2.0 - c1 * rp;
  const auto integral = [&](double r) { return c0 * r * r / 2.0 + c1 * r * r * r / 3.0; };
  return 2.0 * kPi / pitch * (integral(b) - integral(a));
}

// An external thread's section per millimetre: the core and the teeth.
double bolt_per_mm(double pitch, const Diameters& d) {
  return kPi * d.minor * d.minor / 4.0 + teeth_per_mm(pitch, d, false);
}

// What an internal thread takes from a solid per millimetre: everything
// inside the major diameter but the teeth.
double nut_hole_per_mm(double pitch, const Diameters& d) {
  return kPi * d.major * d.major / 4.0 - teeth_per_mm(pitch, d, true);
}

gp_Pnt centre(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.CentreOfMass();
}

bool inside(const Shape& shape, const gp_Pnt& point) {
  BRepClass3d_SolidClassifier classifier(shape.occt(), point, 1e-7);
  return classifier.State() == TopAbs_IN;
}

void test_bolt() {
  // The reference model's bolt: M10x1.5 6g over a 10 mm rod 20 long,
  // whose stored body has 1264.9244 mm3; the exact profile 1264.9649
  // (the stored flanks are fitted splines).
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  const ThreadSpec spec = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  const ShapePtr bolt = modeled_thread(*rod, spec);
  const double exact = 20 * bolt_per_mm(1.5, k6g);
  CHECK_NEAR_TOL(volume(*bolt), exact, 2e-5);
  CHECK_NEAR_TOL(volume(*bolt), 1264.9244242962456, 1e-4);
  CHECK(!bolt->find_faces("F3:thread(F2:side(c1))").empty());
  CHECK(bolt->find_faces("F2:end(r{c1})").size() == 1);
  CHECK(bolt->find_faces("F2:start(r{c1})").size() == 1);
  CHECK(bolt->find_faces("F2:side(c1)").empty());
  CHECK(near(bounding_box(*bolt).max.Z(), 20, 1e-6));
  CHECK(near(bounding_box(*bolt).max.X(), 4.925, 1e-4));

  // Where the helix starts: at angle 0, on the X axis from the face's low
  // end, the groove spans the first half pitch at the pitch diameter.
  const double rp = k6g.pitch / 2.0;
  CHECK(!inside(*bolt, gp_Pnt(rp, 0, 0.375)));
  CHECK(inside(*bolt, gp_Pnt(rp, 0, 1.125)));
  // A quarter turn on (right-hand) the groove has risen a quarter pitch.
  CHECK(!inside(*bolt, gp_Pnt(0, rp, 0.75)));
  CHECK(inside(*bolt, gp_Pnt(0, rp, 1.5)));

  // Turned by an angle: the same volume, the centre of mass turned with it.
  ThreadSpec turned = spec;
  turned.angle = kPi / 2.0;
  const ShapePtr other = modeled_thread(*rod, turned);
  CHECK_NEAR_TOL(volume(*other), volume(*bolt), 1e-6);
  const gp_Pnt a = centre(*bolt);
  const gp_Pnt b = centre(*other);
  CHECK(std::hypot(a.X(), a.Y()) > 1e-3);
  CHECK(std::abs(b.X() + a.Y()) < 1e-4 && std::abs(b.Y() - a.X()) < 1e-4);

  // A left-hand thread has the same volume; the reference model's centre
  // lies where the right-hand one's does.
  ThreadSpec left = spec;
  left.right_handed = false;
  CHECK_NEAR_TOL(volume(*modeled_thread(*rod, left)), exact, 2e-5);
  CHECK(std::abs(a.X() - (-0.00805680684860739)) < 2e-3 && std::abs(a.Y() - 0.004548209334390836) < 2e-3);
}

void test_bolt_profiles() {
  // The basic profile: crests on the 10 mm face itself.
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 12);
  const Diameters m10 = basic(10, 1.5);
  CHECK_NEAR_TOL(volume(*modeled_thread(*rod, spec_of("F3", "F2:side(c1)", 1.5, m10))), 12 * bolt_per_mm(1.5, m10),
                 2e-5);
  // A rod thinner than the major diameter gets its teeth added up to it.
  const ShapePtr thin = extrude("F2", {circle(1, 0, 0, 4.7)}, 0, 12);
  const ShapePtr added = modeled_thread(*thin, spec_of("F3", "F2:side(c1)", 1.5, k6g));
  CHECK_NEAR_TOL(volume(*added), 12 * bolt_per_mm(1.5, k6g), 2e-5);
  CHECK(near(bounding_box(*added).max.X(), 4.925, 1e-4));
  // M6x1 and a Unified 1/4-20 (6.35 mm, 20 threads per inch) basic.
  const ShapePtr m6 = extrude("F2", {circle(1, 0, 0, 3)}, 0, 6);
  CHECK_NEAR_TOL(volume(*modeled_thread(*m6, spec_of("F3", "F2:side(c1)", 1.0, basic(6, 1.0)))),
                 6 * bolt_per_mm(1.0, basic(6, 1.0)), 2e-5);
  const ShapePtr unc = extrude("F2", {circle(1, 0, 0, 3.175)}, 0, 10);
  const double pitch = 25.4 / 20;
  CHECK_NEAR_TOL(volume(*modeled_thread(*unc, spec_of("F3", "F2:side(c1)", pitch, basic(6.35, pitch)))),
                 10 * bolt_per_mm(pitch, basic(6.35, pitch)), 2e-5);
}

void test_shoulder_and_partial() {
  // A bolt: a 10 mm shank 20 long on a 16 mm head. The thread over the
  // shank stops at the head (no groove into it) and runs out at the tip.
  const ShapePtr head = extrude("F1", {circle(1, 0, 0, 8)}, -6, 0);
  const ShapePtr shank = extrude("F2", {circle(2, 0, 0, 5)}, 0, 20);
  const BooleanResult joined = boolean(BooleanOp::Join, {head.get()}, *shank);
  CHECK(joined.pieces.size() == 1);
  const ShapePtr bolt = joined.pieces.at(0).shape;
  const ShapePtr threaded = modeled_thread(*bolt, spec_of("F3", "F2:side(c2)", 1.5, k6g));
  CHECK_NEAR_TOL(volume(*threaded), kPi * 64 * 6 + 20 * bolt_per_mm(1.5, k6g), 2e-5);
  CHECK(near(bounding_box(*threaded).min.Z(), -6, 1e-6));

  // 8 mm of thread 2 mm below the top: the rest of the face stays.
  ThreadSpec partial = spec_of("F3", "F2:side(c2)", 1.5, k6g);
  partial.full_length = false;
  partial.length = 8;
  partial.offset = 2;
  const ShapePtr part = modeled_thread(*bolt, partial);
  CHECK_NEAR_TOL(volume(*part), kPi * 64 * 6 + 12 * kPi * 25 + 8 * bolt_per_mm(1.5, k6g), 2e-5);
  CHECK(!part->find_faces("F2:side(c2)").empty());
  partial.length = 19;
  CHECK(throws_with([&] { modeled_thread(*bolt, partial); }, "does not fit"));
  partial.faces = {"F2:end(r{c2})"};
  CHECK(throws_with([&] { modeled_thread(*bolt, partial); }, "not cylindrical"));
  // Diameters that do not make a profile.
  ThreadSpec wrong = spec_of("F3", "F2:side(c2)", 1.5, {9.85, 8.928, 8.141});
  CHECK(throws_with([&] { modeled_thread(*bolt, wrong); }, "diameters"));
  wrong = spec_of("F3", "F2:side(c2)", 0.5, k6g);
  CHECK(throws_with([&] { modeled_thread(*bolt, wrong); }, "too far apart"));
}

void test_tapped_holes() {
  // M10x1.5 6H through a 20 mm plate: a tap drill bore of 8.5 mm (inside
  // the minor diameter: the crests are cut to it) and a 10 mm bore (outside
  // it: the teeth are added). Both leave the same nut.
  const ShapePtr plate = extrude("F2", {rectangle(1, -15, -15, 30, 30)}, 0, 20);
  const double expected = 30 * 30 * 20 - 20 * nut_hole_per_mm(1.5, k6H);
  for (const double bore : {8.5, 10.0}) {
    const ShapePtr drill = extrude("F3", {circle(5, 0, 0, bore / 2.0)}, -1, 21);
    const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *drill);
    CHECK(drilled.pieces.size() == 1);
    const ShapePtr nut = drilled.pieces.at(0).shape;
    const ShapePtr threaded = modeled_thread(*nut, spec_of("F4", "F3:side(c5)", 1.5, k6H));
    CHECK_NEAR_TOL(volume(*threaded), expected, 2e-5);
    CHECK(!threaded->find_faces("F4:thread(F3:side(c5))").empty());
    // The internal thread's tooth spans the first half pitch at angle 0.
    const double rp = k6H.pitch / 2.0;
    CHECK(inside(*threaded, gp_Pnt(rp, 0, 0.375)));
    CHECK(!inside(*threaded, gp_Pnt(rp, 0, 1.125)));
  }

  // The basic M6x1 in a drilled hole through a 6 mm plate: the wall's axis
  // points down, into the material.
  const ShapePtr block = extrude("F2", {rectangle(1, 100, 0, 30, 30)}, 0, 6);
  HoleSpec hole;
  hole.feature = "F4";
  hole.positions = {gp_Ax1(gp_Pnt(115, 15, 6), gp_Dir(0, 0, -1))};
  hole.diameter = basic(6, 1).minor;
  hole.extent = HoleExtent::ThroughAll;
  hole.bodies = {block};
  const BooleanResult drilled = boolean(BooleanOp::Cut, {block.get()}, *hole_tool(hole));
  const ShapePtr nut = drilled.pieces.at(0).shape;
  const ShapePtr in_hole = modeled_thread(*nut, spec_of("F5", "F4:hole0.wall", 1.0, basic(6, 1)));
  CHECK_NEAR_TOL(volume(*in_hole), 30 * 30 * 6 - 6 * nut_hole_per_mm(1.0, basic(6, 1)), 2e-5);

  // A blind hole: the thread stops at the drill point's start.
  const ShapePtr blind_block = extrude("F2", {rectangle(1, 100, 0, 30, 30)}, 0, 20);
  HoleSpec blind = hole;
  blind.positions = {gp_Ax1(gp_Pnt(115, 15, 20), gp_Dir(0, 0, -1))};
  blind.extent = HoleExtent::Distance;
  blind.depth = 12;
  blind.bodies = {blind_block};
  const BooleanResult blind_drilled = boolean(BooleanOp::Cut, {blind_block.get()}, *hole_tool(blind));
  const ShapePtr blind_nut = blind_drilled.pieces.at(0).shape;
  const double before = volume(*blind_nut);
  const ShapePtr blind_threaded = modeled_thread(*blind_nut, spec_of("F5", "F4:hole0.wall", 1.0, basic(6, 1)));
  const double removed = 12 * (nut_hole_per_mm(1.0, basic(6, 1)) - kPi * hole.diameter * hole.diameter / 4.0);
  CHECK_NEAR_TOL(volume(*blind_threaded), before - removed, 2e-5);
}

// A plate with a grid of n x n small holes (some thousands of faces for n
// of 56) with a 10 mm shaft 20 long standing on a pad on it.
ShapePtr perforated(int n) {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, -10), 200, 200, 10).Shape();
  if (n > 0) {
    BRep_Builder builder;
    TopoDS_Compound holes;
    builder.MakeCompound(holes);
    for (int i = 0; i < n; ++i) {
      for (int j = 0; j < n; ++j) {
        const double x = -95 + 190.0 * (i + 0.5) / n;
        const double y = -95 + 190.0 * (j + 0.5) / n;
        if (std::abs(x) < 15 && std::abs(y) < 15) {
          continue;
        }
        builder.Add(holes, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(x, y, -11), gp_Dir(0, 0, 1)), 1.0, 12).Shape());
      }
    }
    plate = BRepAlgoAPI_Cut(plate, holes).Shape();
  }
  const Shape base(plate);
  const ShapePtr pad = extrude("P", {circle(2, 0, 0, 10)}, 0, 4);
  const BooleanResult padded = boolean(BooleanOp::Join, {&base}, *pad);
  CHECK(padded.pieces.size() == 1);
  const ShapePtr shaft = extrude("S", {circle(1, 0, 0, 5)}, 4, 24);
  const BooleanResult joined = boolean(BooleanOp::Join, {padded.pieces.at(0).shape.get()}, *shaft);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.at(0).shape;
}

void test_large_body() {
  // The thread changes the large body exactly as the plain one, in about
  // the same time: only the faces near the thread take part.
  using Clock = std::chrono::steady_clock;
  const ShapePtr small = perforated(0);
  const ShapePtr large = perforated(56);
  CHECK(large->face_count() > 3000);
  const ThreadSpec spec = spec_of("T", "S:side(c1)", 1.5, k6g);
  const auto t0 = Clock::now();
  const ShapePtr a = modeled_thread(*small, spec);
  const auto t1 = Clock::now();
  const ShapePtr b = modeled_thread(*large, spec);
  const auto t2 = Clock::now();
  CHECK_NEAR_TOL(volume(*small) - volume(*a), volume(*large) - volume(*b), 1e-5);
  CHECK(b->face_count() - large->face_count() == a->face_count() - small->face_count());
  CHECK(!b->find_faces("T:thread(S:side(c1))").empty());
  const double small_ms = std::chrono::duration<double, std::milli>(t1 - t0).count();
  const double large_ms = std::chrono::duration<double, std::milli>(t2 - t1).count();
  std::printf("thread: %d faces in %.0f ms, %d faces in %.0f ms\n", small->face_count(), small_ms,
              large->face_count(), large_ms);
  CHECK(large_ms < 4.0 * small_ms + 2000.0);
}

} // namespace

void thread_tests() {
  guarded("test_bolt", test_bolt);
  guarded("test_bolt_profiles", test_bolt_profiles);
  guarded("test_shoulder_and_partial", test_shoulder_and_partial);
  guarded("test_tapped_holes", test_tapped_holes);
  guarded("test_large_body", test_large_body);
}

} // namespace test
