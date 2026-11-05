// SPDX-License-Identifier: MIT
// Modelled threads: the volume against the profile's exact one (bolts and
// tapped holes, a class's diameters and the basic ones), where the helix
// starts, ends at shoulders and partial lengths, end chamfers, taper pipe
// threads, names, and the speed on a body of thousands of faces; the
// tools of faces far apart built in parallel and applied together
// (mitcad#843) with the result made face by face; a pitch of one's own,
// multi-start threads and the trapezoidal profile (mitcad#893); a shank
// whose end under a serrated head is uneven (mitcad#901); the run-outs
// into a countersink and along the chamfers of a partial thread's ends
// inside its face (mitcad#1300).

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <exception>
#include <limits>
#include <memory>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pln.hxx>

#include "../src/face_join.hpp"
#include "../src/history.hpp"
#include "../src/thread_options.hpp"
#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kTan30 = 0.57735026918962576451;
constexpr double kH = 0.8660254037844386; // H / P
constexpr double kCos30 = 0.8660254037844386;

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
// swept round the axis, integrated exactly. A flank clearance moves each
// flank `flank` perpendicular to itself into the tooth: the tooth is
// 2 flank / cos 30 narrower at every radius.
double teeth_per_mm(double pitch, const Diameters& d, bool internal, double flank = 0.0) {
  const double rp = d.pitch / 2.0;
  const double a = d.minor / 2.0;
  const double b = d.major / 2.0;
  // width(r) = c0 + c1 r
  const double c1 = internal ? 2.0 * kTan30 : -2.0 * kTan30;
  const double c0 = pitch / 2.0 - c1 * rp - 2.0 * flank / kCos30;
  const auto integral = [&](double r) { return c0 * r * r / 2.0 + c1 * r * r * r / 3.0; };
  return 2.0 * kPi / pitch * (integral(b) - integral(a));
}

// An external thread's section per millimetre: the core and the teeth.
double bolt_per_mm(double pitch, const Diameters& d, double flank = 0.0) {
  return kPi * d.minor * d.minor / 4.0 + teeth_per_mm(pitch, d, false, flank);
}

// What an internal thread takes from a solid per millimetre: everything
// inside the major diameter but the teeth.
double nut_hole_per_mm(double pitch, const Diameters& d, double flank = 0.0) {
  return kPi * d.major * d.major / 4.0 - teeth_per_mm(pitch, d, true, flank);
}

// A profile moved by a radial clearance away from the material: every
// diameter 2 radial larger inside, smaller outside.
Diameters loosened(const Diameters& d, double radial, bool internal) {
  const double shift = internal ? 2.0 * radial : -2.0 * radial;
  return {d.major + shift, d.minor + shift, d.pitch + shift};
}

// The centre of mass, by the analysis library's integrals (OCCT's own
// integration loses precision on faces that run many turns).
gp_Pnt centre(const Shape& shape) { return mass_properties(shape).center; }

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

// Extra clearance on bolts (mitcad#195): the radial one moves the crests
// and roots in, the flank one moves each flank perpendicular to itself;
// the volumes against the exact profile and points either side of the
// moved crests, roots and flanks.
void test_bolt_clearance() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 12);
  const double rp = k6g.pitch / 2.0;

  // Radial only: 0.2 per side takes 0.4 off every diameter.
  ThreadSpec radial = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  radial.radial_clearance = 0.2;
  const ShapePtr loose = modeled_thread(*rod, radial);
  CHECK_NEAR_TOL(volume(*loose), 12 * bolt_per_mm(1.5, loosened(k6g, 0.2, false)), 2e-5);
  CHECK(near(bounding_box(*loose).max.X(), 4.725, 1e-4));
  CHECK(near(2.0 * bounding_box(*loose).max.X(), k6g.major - 0.4, 1e-4));
  // The crest (the tooth's middle at angle 0 is 1.125 up) and the root (the
  // groove's middle 0.375 up) each 0.2 further in.
  CHECK(inside(*loose, gp_Pnt(4.725 - 0.01, 0, 1.125)));
  CHECK(!inside(*loose, gp_Pnt(4.725 + 0.01, 0, 1.125)));
  const double root = k6g.minor / 2.0 - 0.2;
  CHECK(inside(*loose, gp_Pnt(root - 0.01, 0, 0.375)));
  CHECK(!inside(*loose, gp_Pnt(root + 0.01, 0, 0.375)));

  // Flank only: the crests keep their radius; each flank moves 0.05 / cos 30
  // along the axis, the groove at the pitch diameter spanning
  // [-relief, 0.75 + relief] instead of [0, 0.75], every pitch.
  const double flank = 0.05;
  const double relief = flank / kCos30;
  ThreadSpec thin = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  thin.flank_clearance = flank;
  const ShapePtr relieved = modeled_thread(*rod, thin);
  CHECK_NEAR_TOL(volume(*relieved), 12 * bolt_per_mm(1.5, k6g, flank), 2e-5);
  CHECK(near(bounding_box(*relieved).max.X(), 4.925, 1e-4));
  for (const double turn : {0.0, 1.5, 3.0}) {
    CHECK(!inside(*relieved, gp_Pnt(rp, 0, turn + 0.75 + 0.9 * relief)));
    CHECK(inside(*relieved, gp_Pnt(rp, 0, turn + 0.75 + 1.1 * relief)));
    CHECK(!inside(*relieved, gp_Pnt(rp, 0, turn + 1.5 - 0.9 * relief)));
    CHECK(inside(*relieved, gp_Pnt(rp, 0, turn + 1.5 - 1.1 * relief)));
  }
  // The flank keeps its angle: 0.2 further out it lies 0.2 tan 30 further
  // along, moved by the same relief.
  const double out = rp + 0.2;
  const double edge = 0.75 + 0.2 * kTan30 + relief;
  CHECK(!inside(*relieved, gp_Pnt(out, 0, edge - 0.1 * relief)));
  CHECK(inside(*relieved, gp_Pnt(out, 0, edge + 0.1 * relief)));

  // Both, on a left-hand thread 8 long 2 below the top: the profile is
  // moved in and then relieved, the rest of the face stays.
  ThreadSpec both = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  both.radial_clearance = 0.2;
  both.flank_clearance = flank;
  both.right_handed = false;
  both.full_length = false;
  both.length = 8;
  both.offset = 2;
  const ShapePtr part = modeled_thread(*rod, both);
  CHECK_NEAR_TOL(volume(*part), 4 * kPi * 25 + 8 * bolt_per_mm(1.5, loosened(k6g, 0.2, false), flank), 2e-5);
  CHECK(!part->find_faces("F2:side(c1)").empty());
  CHECK(inside(*part, gp_Pnt(4.99, 0, 1)));
  CHECK(inside(*part, gp_Pnt(4.99, 0, 11)));
  CHECK(!inside(*part, gp_Pnt(4.8, 0, 5.625)));
  CHECK(inside(*part, gp_Pnt(4.725 - 0.01, 0, 5.625)));
  CHECK(!inside(*part, gp_Pnt(4.725 + 0.01, 0, 5.625)));
  const double moved = rp - 0.2;
  CHECK(!inside(*part, gp_Pnt(moved, 0, 5.25 + 0.9 * relief)));
  CHECK(inside(*part, gp_Pnt(moved, 0, 5.25 + 1.1 * relief)));
  // A quarter turn on, the left-hand groove has fallen a quarter pitch
  // (a right-hand one would have risen): the tooth's middle at 5.25.
  CHECK(inside(*part, gp_Pnt(0, moved, 5.25)));
  CHECK(!inside(*part, gp_Pnt(0, moved, 6.0)));

  // Values that leave no thread, and values that are none.
  ThreadSpec wrong = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  wrong.radial_clearance = -0.1;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "must not be negative"));
  wrong.radial_clearance = 0.0;
  wrong.flank_clearance = std::numeric_limits<double>::quiet_NaN();
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "finite"));
  // The crest of 6g is 0.218 wide: 0.1 per flank takes 0.231 off it.
  wrong.flank_clearance = 0.1;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "no usable tooth"));
  wrong.flank_clearance = 0.0;
  wrong.radial_clearance = 4.1;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "no core"));
}

// Extra clearance on tapped holes: the radial one moves the crests and
// roots out, the flank one widens the groove.
void test_hole_clearance() {
  const ShapePtr plate = extrude("F2", {rectangle(1, -15, -15, 30, 30)}, 0, 20);
  const ShapePtr drill = extrude("F3", {circle(5, 0, 0, 4.25)}, -1, 21);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *drill);
  CHECK(drilled.pieces.size() == 1);
  const ShapePtr nut = drilled.pieces.at(0).shape;
  const double flank = 0.03;
  const double relief = flank / kCos30;
  ThreadSpec spec = spec_of("F4", "F3:side(c5)", 1.5, k6H);
  spec.radial_clearance = 0.2;
  spec.flank_clearance = flank;
  const ShapePtr threaded = modeled_thread(*nut, spec);
  CHECK_NEAR_TOL(volume(*threaded), 30 * 30 * 20 - 20 * nut_hole_per_mm(1.5, loosened(k6H, 0.2, true), flank),
                 2e-5);
  CHECK(!threaded->find_faces("F4:thread(F3:side(c5))").empty());
  // The tooth at the moved pitch diameter spans [relief, 0.75 - relief].
  const double rp = k6H.pitch / 2.0 + 0.2;
  CHECK(inside(*threaded, gp_Pnt(rp, 0, 2.25 - 1.1 * relief)));
  CHECK(!inside(*threaded, gp_Pnt(rp, 0, 2.25 - 0.9 * relief)));
  CHECK(inside(*threaded, gp_Pnt(rp, 0, 1.5 + 1.1 * relief)));
  CHECK(!inside(*threaded, gp_Pnt(rp, 0, 1.5 + 0.9 * relief)));
  // The crest (the minor diameter, at the tooth's middle) and the root (the
  // major one, at the groove's middle) each 0.2 further out.
  const double crest = k6H.minor / 2.0 + 0.2;
  CHECK(inside(*threaded, gp_Pnt(crest + 0.01, 0, 1.875)));
  CHECK(!inside(*threaded, gp_Pnt(crest - 0.01, 0, 1.875)));
  const double root = k6H.major / 2.0 + 0.2;
  CHECK(!inside(*threaded, gp_Pnt(root - 0.01, 0, 2.625)));
  CHECK(inside(*threaded, gp_Pnt(root + 0.01, 0, 2.625)));
  // The crest of 6H is 0.409 wide: 0.17 per flank takes 0.393 off it.
  ThreadSpec wrong = spec_of("F4", "F3:side(c5)", 1.5, k6H);
  wrong.flank_clearance = 0.17;
  CHECK(throws_with([&] { modeled_thread(*nut, wrong); }, "no usable tooth"));

  // The basic M6x1 in a drilled hole through a 6 mm plate (the wall's axis
  // points down, into the material), as a tapped hole makes it.
  const ShapePtr block = extrude("F2", {rectangle(1, 100, 0, 30, 30)}, 0, 6);
  HoleSpec hole;
  hole.feature = "F4";
  hole.positions = {gp_Ax1(gp_Pnt(115, 15, 6), gp_Dir(0, 0, -1))};
  hole.diameter = basic(6, 1).minor;
  hole.extent = HoleExtent::ThroughAll;
  hole.bodies = {block};
  const BooleanResult holed = boolean(BooleanOp::Cut, {block.get()}, *hole_tool(hole));
  ThreadSpec tapped = spec_of("F5", "F4:hole0.wall", 1.0, basic(6, 1));
  tapped.radial_clearance = 0.15;
  tapped.flank_clearance = 0.02;
  const ShapePtr in_hole = modeled_thread(*holed.pieces.at(0).shape, tapped);
  CHECK_NEAR_TOL(volume(*in_hole), 30 * 30 * 6 - 6 * nut_hole_per_mm(1.0, loosened(basic(6, 1), 0.15, true), 0.02),
                 2e-5);
}

// End chamfers (mitcad#304). The material a chamfer takes off a thread is
// exact: at a radius r the cone cuts u(r) deep into the material, and over
// a whole turn the thread's material at that radius is the tooth's width
// over the pitch (the helix sweeps every phase), so the cut is the
// integral of 2 pi r u(r) times that fraction, piecewise a cubic in r
// (Simpson's rule is exact for it).
template <class F>
double integral(const F& f, double a, double b) {
  if (b <= a) {
    return 0.0;
  }
  const int n = 64;
  const double h = (b - a) / n;
  double sum = f(a) + f(b);
  for (int i = 1; i < n; ++i) {
    sum += f(a + i * h) * (i % 2 == 1 ? 4.0 : 2.0);
  }
  return sum * h / 3.0;
}

// The fraction of an external (or internal) thread's material at radius r:
// the core inside the minor diameter (outside the major one), the teeth
// between them, nothing beyond the crest.
double material_fraction(double pitch, const Diameters& d, bool internal, double r) {
  const double a = d.minor / 2.0;
  const double b = d.major / 2.0;
  if (r <= a) {
    return internal ? 0.0 : 1.0;
  }
  if (r >= b) {
    return internal ? 1.0 : 0.0;
  }
  const double c1 = internal ? 2.0 * kTan30 : -2.0 * kTan30;
  const double c0 = pitch / 2.0 - c1 * d.pitch / 2.0;
  return (c0 + c1 * r) / pitch;
}

// What one end chamfer of diameter `diameter` at `angle` to the axis takes
// off a thread (`threaded`) or off a plain face of `radius`.
double chamfer_cut(double pitch, const Diameters& d, bool internal, double diameter, double angle, bool threaded,
                   double radius = 0.0) {
  const double rc = diameter / 2.0;
  const double slope = std::tan(angle);
  const auto fraction = [&](double r) {
    if (threaded) {
      return material_fraction(pitch, d, internal, r);
    }
    return internal ? (r >= radius ? 1.0 : 0.0) : (r <= radius ? 1.0 : 0.0);
  };
  const auto cut = [&](double r) {
    const double u = (internal ? rc - r : r - rc) / slope;
    return 2.0 * kPi * r * u * fraction(r);
  };
  const double a = d.minor / 2.0;
  const double b = d.major / 2.0;
  if (!threaded) {
    return internal ? integral(cut, radius, rc) : integral(cut, rc, radius);
  }
  // Split where the fraction changes its formula.
  if (internal) {
    return integral(cut, a, std::min(b, rc)) + integral(cut, b, rc);
  }
  return integral(cut, rc, a) + integral(cut, std::max(a, rc), b);
}

// The smallest face of a shape.
double smallest_face(const Shape& shape) {
  double smallest = std::numeric_limits<double>::max();
  for (int i = 0; i < shape.face_count(); ++i) {
    GProp_GProps props;
    BRepGProp::SurfaceProperties(shape.face(i), props);
    smallest = std::min(smallest, props.Mass());
  }
  return smallest;
}

void test_end_chamfers() {
  const double pitch = 1.25;
  const Diameters m8 = basic(8, pitch);
  const double angle = kPi / 4.0;
  // The model's defaults: a tenth of a pitch inside the minor diameter
  // outside, outside the major diameter inside.
  const double d_ext = m8.minor - pitch / 10.0;
  const double d_int = m8.major + pitch / 10.0;

  // An M8 bolt: a shank 16 long on a 14 mm head, both ends asked for. Only
  // the tip is free: the head's end keeps its full thread.
  const ShapePtr head = extrude("F1", {circle(1, 0, 0, 7)}, -5, 0);
  const ShapePtr shank = extrude("F2", {circle(2, 0, 0, 4)}, 0, 16);
  const BooleanResult joined = boolean(BooleanOp::Join, {head.get()}, *shank);
  CHECK(joined.pieces.size() == 1);
  const ShapePtr bolt = joined.pieces.at(0).shape;
  ThreadSpec spec = spec_of("F3", "F2:side(c2)", pitch, m8);
  spec.chamfer = true;
  spec.chamfer_angle = angle;
  spec.chamfer_diameter = d_ext;
  spec.chamfer_ends = {{true, true}};
  const ShapePtr threaded = modeled_thread(*bolt, spec);
  const double plain = kPi * 49 * 5 + 16 * bolt_per_mm(pitch, m8);
  const double cut = chamfer_cut(pitch, m8, false, d_ext, angle, true);
  CHECK(cut > 1.0);
  CHECK_NEAR_TOL(volume(*threaded), plain - cut, 2e-5);
  const int low = static_cast<int>(threaded->find_faces("F3:thread_chamfer_low(F2:side(c2))").size());
  const int high = static_cast<int>(threaded->find_faces("F3:thread_chamfer_high(F2:side(c2))").size());
  CHECK((low == 0) != (high == 0));
  // The cone runs from the chamfer's diameter at the tip; no groove reaches
  // the tip's plane, and the first turn leaves no sliver.
  CHECK(near(bounding_box(*threaded).max.Z(), 16, 1e-6));
  CHECK(inside(*threaded, gp_Pnt(d_ext / 2.0 - 0.01, 0, 16 - 1e-3)));
  CHECK(!inside(*threaded, gp_Pnt(d_ext / 2.0 + 0.01, 0, 16 - 1e-3)));
  CHECK(smallest_face(*threaded) > 1e-4);
  // The input is left as it was.
  CHECK_NEAR_TOL(volume(*bolt), kPi * 49 * 5 + kPi * 16 * 16, 1e-9);

  // An M8 tapped hole through a plate 12 thick: both mouths countersunk to
  // the default diameter at 45 degrees (a 90-degree countersink).
  const ShapePtr plate = extrude("F2", {rectangle(1, -15, -15, 30, 30)}, 0, 12);
  const ShapePtr drill = extrude("F3", {circle(5, 0, 0, m8.minor / 2.0)}, -1, 13);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *drill);
  CHECK(drilled.pieces.size() == 1);
  const ShapePtr nut = drilled.pieces.at(0).shape;
  ThreadSpec tapped = spec_of("F4", "F3:side(c5)", pitch, m8);
  tapped.chamfer = true;
  tapped.chamfer_angle = angle;
  tapped.chamfer_diameter = d_int;
  tapped.chamfer_ends = {{true, true}};
  const ShapePtr countersunk = modeled_thread(*nut, tapped);
  const double sink = chamfer_cut(pitch, m8, true, d_int, angle, true);
  CHECK(sink > 1.0);
  CHECK_NEAR_TOL(volume(*countersunk), 30 * 30 * 12 - 12 * nut_hole_per_mm(pitch, m8) - 2 * sink, 2e-5);
  CHECK(!countersunk->find_faces("F4:thread_chamfer_low(F3:side(c5))").empty());
  CHECK(!countersunk->find_faces("F4:thread_chamfer_high(F3:side(c5))").empty());
  CHECK(!inside(*countersunk, gp_Pnt(d_int / 2.0 - 0.01, 0, 12 - 1e-3)));
  CHECK(inside(*countersunk, gp_Pnt(d_int / 2.0 + 0.01, 0, 12 - 1e-3)));
  CHECK(smallest_face(*countersunk) > 1e-4);
  // Only the end asked for.
  tapped.chamfer_ends = {{true, false}};
  const ShapePtr one = modeled_thread(*nut, tapped);
  CHECK_NEAR_TOL(volume(*one), 30 * 30 * 12 - 12 * nut_hole_per_mm(pitch, m8) - sink, 2e-5);
  CHECK(one->find_faces("F4:thread_chamfer_high(F3:side(c5))").empty());

  // Cosmetic: the same cones on the plain faces, no profile.
  ThreadSpec cosmetic = spec;
  cosmetic.groove = false;
  const ShapePtr chamfered = modeled_thread(*bolt, cosmetic);
  CHECK_NEAR_TOL(volume(*chamfered),
                 kPi * 49 * 5 + kPi * 16 * 16 - chamfer_cut(pitch, m8, false, d_ext, angle, false, 4.0), 1e-6);
  CHECK(chamfered->find_faces("F3:thread(F2:side(c2))").empty());
  CHECK(!chamfered->find_faces("F2:side(c2)").empty());
  ThreadSpec cosmetic_hole = tapped;
  cosmetic_hole.groove = false;
  cosmetic_hole.chamfer_ends = {{true, true}};
  const ShapePtr sunk = modeled_thread(*nut, cosmetic_hole);
  CHECK_NEAR_TOL(volume(*sunk),
                 volume(*nut) - 2 * chamfer_cut(pitch, m8, true, d_int, angle, false, m8.minor / 2.0), 1e-6);

  // A partial thread: an end inside the face gets no chamfer.
  ThreadSpec partial = spec;
  partial.full_length = false;
  partial.length = 8;
  partial.offset = 2;
  const ShapePtr inner = modeled_thread(*bolt, partial);
  CHECK_NEAR_TOL(volume(*inner), kPi * 49 * 5 + 8 * kPi * 16 + 8 * bolt_per_mm(pitch, m8), 2e-5);

  // Sizes that cut nothing, or more than the thread.
  ThreadSpec wrong = spec;
  wrong.chamfer_diameter = 8.5;
  CHECK(throws_with([&] { modeled_thread(*bolt, wrong); }, "must be smaller"));
  wrong = tapped;
  wrong.chamfer_diameter = 6.0;
  CHECK(throws_with([&] { modeled_thread(*nut, wrong); }, "must be larger"));
  wrong = spec;
  wrong.chamfer_angle = 0.02;
  CHECK(throws_with([&] { modeled_thread(*bolt, wrong); }, "deeper than the thread"));
  wrong.chamfer_angle = kPi / 2.0;
  CHECK(throws_with([&] { modeled_thread(*bolt, wrong); }, "between 0 and 90"));
}

// A rod of a radius from 0 to 20 with a 45-degree chamfer `size` wide at
// its top end (`F4:chamfer(E{F2:end(r{c1})|F2:side(c1)})`).
ShapePtr chamfered_rod(double radius, double size) {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, radius)}, 0, 20);
  ChamferSpec equal;
  equal.distance = size;
  return chamfer("F4", *rod, {"E{F2:end(r{c1})|F2:side(c1)}"}, equal);
}

void test_sized_chamfered_end() {
  // The bodies stored after a modelled thread on a shaft sized to the
  // major diameter keep a chamfer at its free end on the sized face: the
  // cone moves along the radius with the face, its edge with the face
  // stays, and its end circle shrinks by as much (mitcad#335). Sizing the
  // face and moving the cone by the radial step times cos 45 along its
  // normal (as the .f3d import does) gives the thread on a rod made at the
  // major diameter with the same chamfer.
  const double step = (k6g.major - 10.0) / 2.0;
  const std::string cone = "F4:chamfer(E{F2:end(r{c1})|F2:side(c1)})";
  const ShapePtr rod = chamfered_rod(5.0, 1.5);
  CHECK(faces_named(*rod, cone) == 1);
  const ShapePtr sized = offset_faces("F5", *rod, {"F2:side(c1)"}, step);
  const ShapePtr moved = offset_faces("F6", *sized, {cone}, step * std::sqrt(0.5));
  const ThreadSpec spec = spec_of("F7", "F2:side(c1)", 1.5, k6g);
  const ShapePtr threaded = modeled_thread(*moved, spec);
  const ShapePtr made = modeled_thread(*chamfered_rod(k6g.major / 2.0, 1.5), spec);
  CHECK_NEAR_TOL(volume(*threaded), volume(*made), 1e-7);
  // The cone from the crest at 18.5 to a radius of 4.925 - 1.5 at the end.
  const double end = k6g.major / 2.0 - 1.5;
  CHECK(inside(*threaded, gp_Pnt(end - 0.01, 0, 20 - 1e-3)));
  CHECK(!inside(*threaded, gp_Pnt(end + 0.01, 0, 20 - 1e-3)));
  CHECK(near(bounding_box(*threaded).max.Z(), 20, 1e-6));
  // With the face sized alone the cone stays where it was: 1.92 mm3 more,
  // 0.15 % of the bolt, before the grooves ran out into the cone
  // (mitcad#1300), which takes part of the difference.
  const ShapePtr alone = modeled_thread(*sized, spec);
  CHECK(volume(*alone) - volume(*made) > 0.5);
  // The two moves alone give the rod made at the major diameter.
  CHECK_NEAR_TOL(volume(*moved), volume(*chamfered_rod(k6g.major / 2.0, 1.5)), 1e-9);
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
// A piece of a thread, by its number (`#k` after the thread's name), and a
// point on it.
struct NumberedPiece {
  const char* number;
  double x;
  double y;
  double z;
};

// The largest tolerance of the shape's vertices and edges.
double max_tolerance(const Shape& shape) {
  double tolerance = 0.0;
  for (TopExp_Explorer v(shape.occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
  }
  for (TopExp_Explorer e(shape.occt(), TopAbs_EDGE); e.More(); e.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  return tolerance;
}

// The 57 pieces of the M10x1.5 thread on the rod of
// test_pieces_on_screw_surfaces_and_legacy_names, right-hand, by their
// numbers along the axis (mitcad#490), with a point on each.
const std::vector<NumberedPiece> kRightHandPieces = {
    {"#57", 4.283936773, 1.370270339, 0.054420733},
    {"#58", 3.876997309, -1.240105686, 0.399027853},
    {"#59", -4.179411981, -2.605406013, 0.616920758},
    {"#60", 1.016436963, -4.381393724, 0.448906333},
    {"#61", 0.000000000, -4.497750051, 1.105514399},
    {"#62", -0.000000000, -4.070500000, 1.500000000},
    {"#63", -4.925000000, 0.000000000, 1.874999998},
    {"#64", 0.000000000, -4.497750051, 1.894485601},
    {"#65", 0.000000000, -4.497750051, 2.605514399},
    {"#66", -0.000000000, -4.070500000, 3.000000000},
    {"#67", -4.925000000, 0.000000000, 3.374999998},
    {"#68", 0.000000000, -4.497750051, 3.394485601},
    {"#69", -0.000000000, -4.497750051, 4.105514399},
    {"#70", -0.000000000, -4.070500000, 4.500000000},
    {"#71", -4.925000000, 0.000000000, 4.874999998},
    {"#72", -0.000000000, -4.497750051, 4.894485601},
    {"#73", 0.000000000, -4.497750051, 5.605514399},
    {"#74", -0.000000000, -4.070500000, 6.000000000},
    {"#75", -4.925000000, 0.000000000, 6.374999998},
    {"#76", 0.000000000, -4.497750051, 6.394485601},
    {"#77", -0.000000000, -4.497750051, 7.105514399},
    {"#78", -0.000000000, -4.070500000, 7.500000000},
    {"#79", -4.925000000, 0.000000000, 7.874999998},
    {"#80", -0.000000000, -4.497750051, 7.894485601},
    {"#81", 0.000000000, -4.497750051, 8.605514399},
    {"#82", -0.000000000, -4.070500000, 9.000000000},
    {"#83", -4.925000000, 0.000000000, 9.374999998},
    {"#84", 0.000000000, -4.497750051, 9.394485601},
    {"#85", 0.000000000, -4.497750051, 10.105514399},
    {"#86", -0.000000000, -4.070500000, 10.500000000},
    {"#87", -4.925000000, 0.000000000, 10.874999998},
    {"#88", 0.000000000, -4.497750051, 10.894485601},
    {"#89", -0.000000000, -4.497750051, 11.605514399},
    {"#90", -0.000000000, -4.070500000, 12.000000000},
    {"#91", -4.925000000, 0.000000000, 12.374999998},
    {"#92", -0.000000000, -4.497750051, 12.394485601},
    {"#93", 0.000000000, -4.497750051, 13.105514399},
    {"#94", -0.000000000, -4.070500000, 13.500000000},
    {"#95", -4.925000000, 0.000000000, 13.874999998},
    {"#96", 0.000000000, -4.497750051, 13.894485601},
    {"#97", 0.000000000, -4.497750051, 14.605514399},
    {"#98", -0.000000000, -4.070500000, 15.000000000},
    {"#99", -4.925000000, 0.000000000, 15.374999998},
    {"#100", 0.000000000, -4.497750051, 15.394485601},
    {"#101", -0.000000000, -4.497750051, 16.105514399},
    {"#102", -0.000000000, -4.070500000, 16.500000000},
    {"#103", -4.925000000, 0.000000000, 16.874999998},
    {"#104", -0.000000000, -4.497750051, 16.894485601},
    {"#105", -0.000000000, -4.497750051, 17.605514399},
    {"#106", -0.000000000, -4.070500000, 18.000000000},
    {"#107", -4.925000000, 0.000000000, 18.374999998},
    {"#108", -0.000000000, -4.497750051, 18.394485601},
    {"#109", -0.000000000, -4.497750051, 19.105514399},
    {"#110", -0.864535662, -3.977630996, 19.301093665},
    {"#111", -2.316774659, 4.346053403, 19.562737102},
    {"#112", -3.328657291, -3.024862926, 19.695579269},
    {"#113", -3.052319047, 3.423011597, 19.978268946},
};

// The same rod's left-hand thread.
const std::vector<NumberedPiece> kLeftHandPieces = {
    {"#57", -4.490574524, -0.931880037, 0.021731054},
    {"#58", 0.955279491, -4.395133236, 0.304420732},
    {"#59", -4.179412411, 2.605405323, 0.548374005},
    {"#60", 3.012461658, -2.737525344, 0.698906335},
    {"#61", 3.895165804, -2.248875026, 0.894485601},
    {"#62", 3.895165805, -2.248875026, 1.605514398},
    {"#63", -4.925000000, 0.000000000, 1.875000000},
    {"#64", 3.525156406, -2.035250000, 2.000000000},
    {"#65", 3.895165804, -2.248875026, 2.394485601},
    {"#66", 3.895165805, -2.248875026, 3.105514398},
    {"#67", -4.925000000, 0.000000000, 3.375000000},
    {"#68", 3.525156406, -2.035250000, 3.500000000},
    {"#69", 3.895165804, -2.248875026, 3.894485601},
    {"#70", 3.895165805, -2.248875026, 4.605514398},
    {"#71", -4.925000000, 0.000000000, 4.875000000},
    {"#72", 3.525156406, -2.035250000, 5.000000000},
    {"#73", 3.895165804, -2.248875026, 5.394485601},
    {"#74", 3.895165805, -2.248875026, 6.105514398},
    {"#75", -4.925000000, 0.000000000, 6.375000000},
    {"#76", 3.525156406, -2.035250000, 6.500000000},
    {"#77", 3.895165804, -2.248875026, 6.894485601},
    {"#78", 3.895165805, -2.248875026, 7.605514398},
    {"#79", -4.925000000, 0.000000000, 7.875000000},
    {"#80", 3.525156406, -2.035250000, 8.000000000},
    {"#81", 3.895165804, -2.248875026, 8.394485601},
    {"#82", 3.895165805, -2.248875026, 9.105514398},
    {"#83", -4.925000000, 0.000000000, 9.375000000},
    {"#84", 3.525156406, -2.035250000, 9.500000000},
    {"#85", 3.895165804, -2.248875026, 9.894485601},
    {"#86", 3.895165805, -2.248875026, 10.605514398},
    {"#87", -4.925000000, 0.000000000, 10.875000000},
    {"#88", 3.525156406, -2.035250000, 11.000000000},
    {"#89", 3.895165811, -2.248875030, 11.394485606},
    {"#90", 3.895165805, -2.248875026, 12.105514398},
    {"#91", -4.925000000, 0.000000000, 12.375000000},
    {"#92", 3.525156406, -2.035250000, 12.500000000},
    {"#93", 3.895165804, -2.248875026, 12.894485601},
    {"#94", 3.895165805, -2.248875026, 13.605514398},
    {"#95", -4.925000000, 0.000000000, 13.875000000},
    {"#96", 3.525156406, -2.035250000, 14.000000000},
    {"#97", 3.895165804, -2.248875026, 14.394485601},
    {"#98", 3.895165805, -2.248875026, 15.105514398},
    {"#99", -4.925000000, 0.000000000, 15.375000000},
    {"#100", 3.525156406, -2.035250000, 15.500000000},
    {"#101", 3.895165811, -2.248875030, 15.894485606},
    {"#102", 3.895165805, -2.248875026, 16.605514398},
    {"#103", -4.925000000, 0.000000000, 16.875000000},
    {"#104", 3.525156406, -2.035250000, 17.000000000},
    {"#105", 3.895165804, -2.248875026, 17.394485601},
    {"#106", 3.895165805, -2.248875026, 18.105514398},
    {"#107", -4.925000000, 0.000000000, 18.375000000},
    {"#108", 3.525156406, -2.035250000, 18.500000000},
    {"#109", 3.895165804, -2.248875026, 18.894485601},
    {"#110", 4.302616752, -1.310436631, 19.551093667},
    {"#111", -2.316773942, -4.346053785, 19.508079244},
    {"#112", 3.012461682, 2.737525317, 19.600972147},
    {"#113", 0.955279462, 4.395133243, 19.945579267},
};

// A name the pieces had before mitcad#490 (taken from a build of that
// time, on Linux, with a point on the piece), and whether it still names
// that piece: the pieces whose number was the same on every platform keep
// it, the pieces of whole turns, whose numbers the rounding noise of their
// centres on the axis decided, lose it.
struct LegacyPiece {
  const char* number;
  double x;
  double y;
  double z;
  bool kept;
};

void test_pieces_on_screw_surfaces_and_legacy_names() {
  // The thread's pieces, a crest, two flanks and a root a turn, are put on
  // the exact screw surfaces of the groove's section (mitcad#343), each
  // flank's pieces on one surface, where they were sweeps of their own.
  // They are numbered by their places along the axis, from the pieces'
  // count on, the same on every platform (mitcad#490): each number names
  // the piece that holds its point (taken on Linux), and only it. The
  // numbers of the order before that hold where they held on every
  // platform; references to the others find no face, so that they are
  // reported rather than moved to another turn.
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  ThreadSpec spec = spec_of("F3", "F2:side(c1)", 1.5, k6g);
  const std::string thread = "F3:thread(F2:side(c1))";
  const auto distance_to = [](double x, double y, double z, const TopoDS_Shape& face) {
    const TopoDS_Vertex at = BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, z));
    BRepExtrema_DistShapeShape between(at, face);
    return between.IsDone() ? between.Value() : 1.0;
  };
  // (Whether the name finds one face, the one with the point on it.)
  const auto holds = [&](const Shape& threaded, const char* number, double x, double y, double z) {
    const std::vector<int> faces = threaded.find_faces(thread + number);
    if (faces.size() != 1) {
      std::printf("  %s%s names %zu faces\n", thread.c_str(), number, faces.size());
      return false;
    }
    const double distance = distance_to(x, y, z, threaded.face(faces.front()));
    if (distance >= 2e-4) {
      std::printf("  %s%s is %g from its point\n", thread.c_str(), number, distance);
      return false;
    }
    return true;
  };
  const auto check_pieces = [&](const Shape& threaded, const std::vector<NumberedPiece>& pieces,
                                const std::vector<LegacyPiece>& legacy) {
    CHECK(threaded.find_faces(thread).size() == 57);
    CHECK(pieces.size() == 57);
    for (const NumberedPiece& piece : pieces) {
      CHECK(holds(threaded, piece.number, piece.x, piece.y, piece.z));
    }
    for (const LegacyPiece& piece : legacy) {
      if (piece.kept) {
        CHECK(holds(threaded, piece.number, piece.x, piece.y, piece.z));
      } else {
        CHECK(threaded.find_faces(thread + piece.number).empty());
      }
    }
  };
  const ShapePtr bolt = modeled_thread(*rod, spec);
  check_pieces(*bolt, kRightHandPieces,
               {
                   {"#56", 4.006096174, 2.182098960, 0.062583800, true},
                   {"#55", 4.068958467, -0.112013955, 0.346257715, true},
                   {"#54", 2.735840279, -3.488917118, 0.516242316, true},
                   {"#2", -4.854156205, -0.832341600, 0.370152455, true},
                   {"#18", 2.071030938, -4.064627041, 1.181013414, false},
                   {"#19", 1.847968323, -3.626842013, 1.590328100, false},
                   {"#20", 2.012840691, -3.950422251, 1.969984694, false},
                   {"#6", -4.388207132, 2.235903211, 1.789115846, false},
                   {"#33", 2.071030938, -4.064627041, 8.681013414, false},
                   {"#53", 2.012840691, -3.950422251, 18.469984694, false},
                   {"#17", -4.388207132, 2.235903211, 18.289115846, false},
                   {"#5", 2.071030938, -4.064627041, 19.181013414, true},
                   {"#4", 0.908611370, -3.967794710, 19.531570384, true},
                   {"#1", -2.074516276, -3.918385626, 19.741242409, true},
                   {"#3", -0.884984551, 4.844835121, 19.581867354, true},
                   {"#0", -3.750502502, 2.726913306, 19.999999926, true},
               });
  // The faces put on the exact screw surfaces of the section's edges, the
  // per-turn pieces were approximations of: the shape valid, its tolerances
  // no larger, and its volume and area those of the per-turn thread (taken
  // from a build of that time) within 1e-6, as with the clearances and the
  // end chamfers.
  const auto as_before = [](const Shape& threaded, double volume, double area) {
    CHECK(BRepCheck_Analyzer(threaded.occt()).IsValid());
    CHECK(max_tolerance(threaded) <= 5.0e-6 + 1.0e-12);
    const MassProperties properties = mass_properties(threaded);
    CHECK_NEAR_TOL(properties.volume, volume, 1e-6);
    CHECK_NEAR_TOL(properties.area, area, 1e-6);
  };
  as_before(*bolt, 1264.964921282, 1061.486141608);
  ThreadSpec loose = spec;
  loose.radial_clearance = 0.2;
  loose.flank_clearance = 0.05;
  as_before(*modeled_thread(*rod, loose), 1119.469785640, 997.042469442);
  ThreadSpec chamfered = spec;
  chamfered.chamfer = true;
  chamfered.chamfer_angle = kPi / 4.0;
  chamfered.chamfer_diameter = 7.8;
  chamfered.chamfer_ends = {{true, true}};
  as_before(*modeled_thread(*rod, chamfered), 1252.800363702, 1017.873373268);
  spec.right_handed = false;
  check_pieces(*modeled_thread(*rod, spec), kLeftHandPieces,
               {
                   {"#0", -4.361517055, 1.243916527, 0.107568637, true},
                   {"#53", 4.427586400, -0.232039955, 0.744984711, true},
                   {"#54", 3.890516701, -1.197016826, 0.424085815, true},
                   {"#3", 2.424278895, -3.864354288, 0.184755699, true},
                   {"#1", -2.884529314, 3.991881215, 0.555228680, true},
                   {"#20", -4.388207132, 2.235903211, 2.003826227, false},
                   {"#7", 4.427586400, -0.232039955, 2.244984711, false},
                   {"#19", 4.555585836, -0.238748121, 1.456013361, false},
                   {"#52", -4.388207132, 2.235903211, 5.003826227, false},
                   {"#5", -4.388207132, 2.235903211, 11.003826227, false},
                   {"#17", 4.427586496, -0.232039960, 11.244984767, false},
                   {"#55", 4.516306830, 0.642912705, 19.409755738, true},
                   {"#4", -3.528993778, -3.435378860, 19.581867357, true},
                   {"#2", 0.110111675, 4.432295064, 19.863414325, true},
                   {"#56", 2.131486164, 3.467814422, 19.609398484, true},
               });
}

// The bolt's crest edge between z 3 and 5 (a turn's piece of it).
std::string crest_edge_of(const Shape& bolt) {
  for (int i = 0; i < bolt.edge_count(); ++i) {
    BRepAdaptor_Curve c(bolt.edge(i));
    const gp_Pnt m = c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
    if (std::abs(std::hypot(m.X(), m.Y()) - 4.925) < 1e-4 && c.GetType() == GeomAbs_BSplineCurve && m.Z() > 3 &&
        m.Z() < 5) {
      return bolt.edge_name(i);
    }
  }
  return {};
}

void test_thread_pieces_join_smoothly() {
  // The crest's and the flanks' pieces of a thread joined for a rounding
  // along them: their edges along the thread joined into edges that keep
  // their speed across the joints, as one curve swept at one pace does (a
  // rounding's guide along a curve whose speed jumps at its joints ends off
  // the face at its end, mitcad#343).
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 8);
  const ShapePtr bolt = modeled_thread(*rod, spec_of("F3", "F2:side(c1)", 1.5, k6g));
  std::vector<int> pieces;
  for (int i = 0; i < bolt->face_count(); ++i) {
    for (const std::string& name : bolt->face_names(i)) {
      if (name.rfind("F3:thread(", 0) == 0) {
        pieces.push_back(i);
        break;
      }
    }
  }
  const ShapePtr joined = detail::join_surface_pieces(*bolt, pieces);
  CHECK(joined != nullptr);
  CHECK(joined->face_count() < bolt->face_count());
  int long_edges = 0;
  for (int e = 0; e < joined->edge_count(); ++e) {
    BRepAdaptor_Curve c(joined->edge(e));
    if (c.GetType() != GeomAbs_BSplineCurve || GCPnts_AbscissaPoint::Length(c) < 40.0) {
      continue;
    }
    ++long_edges;
    double low = std::numeric_limits<double>::max();
    double high = 0.0;
    for (int k = 0; k <= 400; ++k) {
      gp_Pnt p;
      gp_Vec d;
      c.D1(c.FirstParameter() + (c.LastParameter() - c.FirstParameter()) * k / 400.0, p, d);
      low = std::min(low, d.Magnitude());
      high = std::max(high, d.Magnitude());
    }
    CHECK(high < 1.05 * low);
  }
  CHECK(long_edges > 0);
}

void test_rounding_along_a_thread_keeps_its_pieces() {
  // A rounding along a thread's crest edge: the crest's and the flanks'
  // pieces, a turn each on one surface, are joined for the rounding's walk,
  // and its result split again where they met, so that the body keeps its
  // thread a turn a face and each piece its name, and the rounding is a face
  // a piece of the edge, named after that piece (mitcad#343).
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 8);
  const ShapePtr bolt = modeled_thread(*rod, spec_of("F3", "F2:side(c1)", 1.5, k6g));
  const std::string crest_edge = crest_edge_of(*bolt);
  CHECK(!crest_edge.empty());
  FilletSet set;
  set.edges = {crest_edge};
  set.radius = 0.05;
  set.tangent_chain = true;
  const ShapePtr rounded = fillet("F4", *bolt, std::vector<FilletSet>{set});
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  for (int i = 0; i < bolt->face_count(); ++i) {
    for (const std::string& name : bolt->face_names(i)) {
      CHECK(rounded->find_faces(name).size() == 1);
    }
  }
  // (No face over more than one piece of the thread. A piece may carry two
  // names, its number and the one it had before mitcad#490.)
  const std::string thread = "F3:thread(F2:side(c1))";
  for (int i = 0; i < rounded->face_count(); ++i) {
    std::set<int> pieces;
    for (const std::string& name : rounded->face_names(i)) {
      if (name.rfind(thread + '#', 0) == 0) {
        for (const int piece : bolt->find_faces(name.substr(0, name.find('#', thread.size() + 1)))) {
          pieces.insert(piece);
        }
      }
    }
    CHECK(pieces.size() <= 1);
  }
  int roundings = 0;
  for (int i = 0; i < rounded->face_count(); ++i) {
    for (const std::string& name : rounded->face_names(i)) {
      roundings += name.rfind("F4:fillet(", 0) == 0 ? 1 : 0;
    }
  }
  CHECK(roundings > 2);
  const double removed = volume(*bolt) - volume(*rounded);
  CHECK(removed > 0.0 && removed < 0.1);
}

} // namespace

// Taper pipe threads (mitcad#733): 1/2-14 NPT, its diameters 1:16 on the
// diameter from where it starts. A cosmetic one in a drilled hole cuts the
// bore's taper from the mouth until it meets the drill; on a shaft it cuts
// what lies outside the cone of its major diameter from the free end. A
// modelled one leaves the profile at the local diameters along the taper.
void test_taper_pipe_threads() {
  constexpr double kInch = 25.4;
  const double pitch = kInch / 14.0;
  const double h = 0.8 * pitch;
  // ASME B1.20.1: E0 = D - (0.05 D + 1.1) P, E1 = E0 + L1 / 16.
  const double e0 = (0.84 - (0.05 * 0.84 + 1.1) / 14.0) * kInch;
  const double e1 = e0 + 0.32 * kInch / 16.0;
  const auto taper_spec = [&](const std::string& face, double start_pitch) {
    ThreadSpec spec = spec_of("F4", face, pitch, {start_pitch + h, start_pitch - h, start_pitch});
    spec.taper = 1.0 / 16.0;
    spec.groove = false;
    return spec;
  };
  const double k = 1.0 / 32.0;

  // A blind bore of the tap drill 25 deep into a block 30 high: the cone
  // narrows from E1 - h at the mouth (z = 30) and meets the bore below.
  const double drill = 17.4752;
  const ShapePtr block = extrude("F2", {rectangle(1, -20, -20, 40, 40)}, 0, 30);
  const ShapePtr bore = extrude("F3", {circle(5, 0, 0, drill / 2.0)}, 5, 31);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {block.get()}, *bore);
  const ShapePtr hole = drilled.pieces.at(0).shape;
  const ShapePtr tapered = modeled_thread(*hole, taper_spec("F3:side(c5)", e1));
  const double a = drill / 2.0;
  const double length = ((e1 - h) / 2.0 - a) / k;
  const double cone = kPi * (a * k * length * length + k * k * length * length * length / 3.0);
  CHECK_NEAR_TOL(volume(*hole) - volume(*tapered), cone, 1e-6);
  CHECK(!tapered->find_faces("F4:thread_taper(F3:side(c5))").empty());
  // At the mouth the bore is E1 - h wide; 10 below it 10/16 narrower.
  CHECK(!inside(*tapered, gp_Pnt((e1 - h) / 2.0 - 0.01, 0, 29.99)));
  CHECK(inside(*tapered, gp_Pnt((e1 - h - 10.0 / 16.0) / 2.0 + 0.01, 0, 20)));
  CHECK(!inside(*tapered, gp_Pnt((e1 - h - 10.0 / 16.0) / 2.0 - 0.01, 0, 20)));
  // A drilled hole, its wall's axis pointing down into the material, with
  // a flat bottom: the mouth is the free end whichever way the axis runs.
  HoleSpec drilling;
  drilling.feature = "F5";
  drilling.positions = {gp_Ax1(gp_Pnt(0, 0, 30), gp_Dir(0, 0, -1))};
  drilling.diameter = drill;
  drilling.flat = true;
  drilling.depth = 25;
  drilling.bodies = {block};
  const ShapePtr hole_down = boolean(BooleanOp::Cut, {block.get()}, *hole_tool(drilling)).pieces.at(0).shape;
  ThreadSpec down = taper_spec("F5:hole0.wall", e1);
  // (Which end it names does not matter where only one end is free.)
  down.taper_high_end = false;
  const ShapePtr tapered_down = modeled_thread(*hole_down, down);
  CHECK_NEAR_TOL(volume(*hole_down) - volume(*tapered_down), cone, 1e-6);

  // A rod of the pipe's outside diameter standing on a plate: its top is
  // the free end, where the major diameter is E0 + h.
  const double d = 0.84 * kInch;
  const ShapePtr plate = extrude("F2", {rectangle(1, -20, -20, 40, 40)}, -5, 0);
  const ShapePtr rod = extrude("F3", {circle(5, 0, 0, d / 2.0)}, 0, 30);
  const ShapePtr pipe = boolean(BooleanOp::Join, {plate.get()}, *rod).pieces.at(0).shape;
  const ShapePtr cut_pipe = modeled_thread(*pipe, taper_spec("F3:side(c5)", e0));
  const double r0 = (e0 + h) / 2.0;
  const double outer = d / 2.0;
  const double run = (outer - r0) / k;
  // The ring outside the cone r0 + k s up to the rod, over `run`.
  const double ring = kPi * (outer * outer * run - (r0 * r0 * run + r0 * k * run * run + k * k * run * run * run / 3.0));
  CHECK_NEAR_TOL(volume(*pipe) - volume(*cut_pipe), ring, 1e-6);
  CHECK(!inside(*cut_pipe, gp_Pnt(r0 + 0.01, 0, 29.99)));
  CHECK(inside(*cut_pipe, gp_Pnt(r0 - 0.01, 0, 29.99)));

  // Modelled in a plate 10 thick with a bore inside the minor cone all
  // the way: what it removes is the nut's hole at the local diameters
  // (the profile per millimetre) less the bore, integrated along the
  // taper.
  const ShapePtr thin = extrude("F2", {rectangle(1, -20, -20, 40, 40)}, 0, 10);
  const ShapePtr small = extrude("F3", {circle(5, 0, 0, 8.0)}, -1, 11);
  const ShapePtr nut = boolean(BooleanOp::Cut, {thin.get()}, *small).pieces.at(0).shape;
  ThreadSpec modelled = taper_spec("F3:side(c5)", e1);
  modelled.groove = true;
  // Both ends are free: it starts at the high end (the top).
  modelled.taper_high_end = true;
  const ShapePtr threaded = modeled_thread(*nut, modelled);
  BRepCheck_Analyzer valid(threaded->occt());
  CHECK(valid.IsValid());
  const double removed = integral(
      [&](double s) {
        const double p = e1 - s / 16.0;
        return nut_hole_per_mm(pitch, {p + h, p - h, p}) - kPi * 8.0 * 8.0;
      },
      0.0, 10.0);
  CHECK_NEAR_TOL(volume(*nut) - volume(*threaded), removed, 1e-3);
  CHECK(!threaded->find_faces("F4:thread(F3:side(c5))").empty());
}


// The material of a thread per millimetre as teeth_per_mm, for flanks at
// an angle to the radius whose tangent is `tan` (tan 15 degrees for the
// trapezoidal profile, mitcad#893): the tooth P/2 wide at the pitch
// diameter, narrowing outwards (an external thread) by 2 tan per unit of
// radius.
double flanked_teeth_per_mm(double pitch, const Diameters& d, bool internal, double tan) {
  const double rp = d.pitch / 2.0;
  const double a = d.minor / 2.0;
  const double b = d.major / 2.0;
  const double c1 = internal ? 2.0 * tan : -2.0 * tan;
  const double c0 = pitch / 2.0 - c1 * rp;
  const auto integral = [&](double r) { return c0 * r * r / 2.0 + c1 * r * r * r / 3.0; };
  return 2.0 * kPi / pitch * (integral(b) - integral(a));
}

double flanked_bolt_per_mm(double pitch, const Diameters& d, double tan) {
  return kPi * d.minor * d.minor / 4.0 + flanked_teeth_per_mm(pitch, d, false, tan);
}

double flanked_nut_hole_per_mm(double pitch, const Diameters& d, double tan) {
  return kPi * d.major * d.major / 4.0 - flanked_teeth_per_mm(pitch, d, true, tan);
}

// A point at the radius `r`, at the angle `theta` about the Z axis from the
// X axis, `z` up.
gp_Pnt at_angle(double r, double theta, double z) { return gp_Pnt(r * std::cos(theta), r * std::sin(theta), z); }

// A pitch of one's own (mitcad#893): 12 x 3, coarser than any ISO metric
// thread of 12 mm, on the basic 60-degree profile. The volume against the
// exact profile, and the groove where the helix puts it: at angle 0 over
// the first half pitch, a quarter turn on risen a quarter pitch.
void test_custom_pitch() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 6)}, 0, 18);
  const Diameters d = basic(12, 3);
  const ShapePtr bolt = modeled_thread(*rod, spec_of("F3", "F2:side(c1)", 3, d));
  CHECK_NEAR_TOL(volume(*bolt), 18 * bolt_per_mm(3, d), 2e-5);
  CHECK(BRepCheck_Analyzer(bolt->occt()).IsValid());
  CHECK(!bolt->find_faces("F3:thread(F2:side(c1))").empty());
  const double rp = d.pitch / 2.0;
  for (const double turn : {3.0, 6.0, 9.0}) {
    // The groove's middle and the tooth's at angle 0, then a quarter turn
    // on.
    CHECK(!inside(*bolt, gp_Pnt(rp, 0, turn + 0.75)));
    CHECK(inside(*bolt, gp_Pnt(rp, 0, turn + 2.25)));
    CHECK(!inside(*bolt, gp_Pnt(0, rp, turn + 1.5)));
    CHECK(inside(*bolt, gp_Pnt(0, rp, turn + 3.0)));
  }
  // The root at the minor diameter, the crest at the major one.
  CHECK(inside(*bolt, gp_Pnt(d.minor / 2.0 - 0.02, 0, 6.75)));
  CHECK(!inside(*bolt, gp_Pnt(d.minor / 2.0 + 0.02, 0, 6.75)));
  CHECK(inside(*bolt, gp_Pnt(5.98, 0, 8.25)));
}

// A two-start thread (mitcad#893): two grooves half a turn apart, each on
// a helix of lead 2 P. Its profile in every section through the axis is
// the single-start thread's of the same pitch, so it has the same volume;
// its helices rise a lead, 3 mm, a turn: a quarter turn on the groove has
// risen half a pitch, half a turn on a whole pitch (where a single-start
// thread's has risen half a pitch). Left-handed, it falls instead.
void test_multi_start_threads() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 12);
  const Diameters d = basic(10, 1.5);
  ThreadSpec two = spec_of("F3", "F2:side(c1)", 1.5, d);
  two.starts = 2;
  const ShapePtr bolt = modeled_thread(*rod, two);
  CHECK_NEAR_TOL(volume(*bolt), 12 * bolt_per_mm(1.5, d), 2e-5);
  CHECK(BRepCheck_Analyzer(bolt->occt()).IsValid());
  CHECK(!bolt->find_faces("F3:thread(F2:side(c1))").empty());
  const double rp = d.pitch / 2.0;
  for (const double z : {3.0, 4.5, 6.0}) {
    // At angle 0 a groove over [0, P/2] of every pitch.
    CHECK(!inside(*bolt, at_angle(rp, 0.0, z + 0.375)));
    CHECK(inside(*bolt, at_angle(rp, 0.0, z + 1.125)));
    // A quarter turn on, risen a quarter lead: half a pitch.
    CHECK(!inside(*bolt, at_angle(rp, kPi / 2.0, z + 1.125)));
    CHECK(inside(*bolt, at_angle(rp, kPi / 2.0, z + 0.375)));
    // Half a turn on, risen a whole pitch: where it was at angle 0 (a
    // single-start thread's groove would be half a pitch higher).
    CHECK(!inside(*bolt, at_angle(rp, kPi, z + 0.375)));
    CHECK(inside(*bolt, at_angle(rp, kPi, z + 1.125)));
    // An eighth of a turn on, risen a quarter pitch.
    CHECK(!inside(*bolt, at_angle(rp, kPi / 4.0, z + 0.75)));
    CHECK(inside(*bolt, at_angle(rp, kPi / 4.0, z)));
  }
  // The same thread single-start differs half a turn on.
  const ShapePtr single = modeled_thread(*rod, spec_of("F3", "F2:side(c1)", 1.5, d));
  CHECK_NEAR_TOL(volume(*single), volume(*bolt), 1e-6);
  CHECK(inside(*single, at_angle(rp, kPi, 4.5 + 0.375)));
  CHECK(!inside(*single, at_angle(rp, kPi, 4.5 + 1.125)));

  // Left-handed: an eighth of a turn on the groove has fallen a quarter
  // pitch.
  ThreadSpec left = two;
  left.right_handed = false;
  const ShapePtr left_bolt = modeled_thread(*rod, left);
  CHECK_NEAR_TOL(volume(*left_bolt), 12 * bolt_per_mm(1.5, d), 2e-5);
  CHECK(!inside(*left_bolt, at_angle(rp, kPi / 4.0, 4.5)));
  CHECK(inside(*left_bolt, at_angle(rp, kPi / 4.0, 4.5 + 0.75)));

  // Three starts in a nut: the same hole as a single-start thread's, its
  // tooth at angle 0 over the first half pitch, a third of a turn on risen
  // a pitch (the next start's).
  const ShapePtr plate = extrude("F2", {rectangle(1, -15, -15, 30, 30)}, 0, 12);
  const ShapePtr drill = extrude("F3", {circle(5, 0, 0, 4.25)}, -1, 13);
  const ShapePtr nut = boolean(BooleanOp::Cut, {plate.get()}, *drill).pieces.at(0).shape;
  ThreadSpec three = spec_of("F4", "F3:side(c5)", 1.5, k6H);
  three.starts = 3;
  const ShapePtr threaded = modeled_thread(*nut, three);
  CHECK_NEAR_TOL(volume(*threaded), 30 * 30 * 12 - 12 * nut_hole_per_mm(1.5, k6H), 2e-5);
  const double hole_rp = k6H.pitch / 2.0;
  CHECK(inside(*threaded, at_angle(hole_rp, 0.0, 4.5 + 0.375)));
  CHECK(!inside(*threaded, at_angle(hole_rp, 0.0, 4.5 + 1.125)));
  // A sixth of a turn on, risen half a pitch (a lead of 4.5 a turn).
  CHECK(inside(*threaded, at_angle(hole_rp, kPi / 3.0, 4.5 + 1.125)));
  CHECK(!inside(*threaded, at_angle(hole_rp, kPi / 3.0, 4.5 + 0.375)));

  // Starts that are none.
  ThreadSpec wrong = two;
  wrong.starts = 0;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "number of starts"));
  wrong.starts = 65;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "number of starts"));
}

// The trapezoidal profile of ISO 2901 (mitcad#893): flanks at 15 degrees
// to the radius, flat crests and roots, Tr10x2's design profile (the
// external thread's root d3 = 7.5 with the crest clearance 0.25, the
// internal one's D4 = 10.5, both pitch diameters 9). The volumes against
// the exact profile; the depth and the flank angle at points either side
// of the root and of the flanks (a 60-degree flank 0.4 above the pitch
// diameter lies 0.124 nearer the tooth's middle than a 15-degree one);
// Tr10x4(P2), two starts.
void test_trapezoidal_threads() {
  const double flank = 15.0 * kPi / 180.0;
  const double tan15 = std::tan(flank);
  const Diameters outside{10.0, 7.5, 9.0};
  const Diameters inside_thread{10.5, 8.0, 9.0};
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 12);
  ThreadSpec tr = spec_of("F3", "F2:side(c1)", 2.0, outside);
  tr.flank_angle = flank;
  const ShapePtr screw = modeled_thread(*rod, tr);
  CHECK_NEAR_TOL(volume(*screw), 12 * flanked_bolt_per_mm(2.0, outside, tan15), 2e-5);
  CHECK(BRepCheck_Analyzer(screw->occt()).IsValid());
  CHECK(!screw->find_faces("F3:thread(F2:side(c1))").empty());
  // The 60-degree profile of the same diameters has less material.
  CHECK(std::abs(flanked_bolt_per_mm(2.0, outside, tan15) - flanked_bolt_per_mm(2.0, outside, kTan30)) > 0.5);
  for (const double z : {4.0, 6.0}) {
    // At angle 0 the groove's middle at a quarter pitch, the tooth's at
    // three quarters; the root at 3.75.
    CHECK(inside(*screw, gp_Pnt(3.74, 0, z + 0.5)));
    CHECK(!inside(*screw, gp_Pnt(3.76, 0, z + 0.5)));
    // 0.4 above the pitch radius the tooth is 2 (0.5 - 0.4 tan 15) =
    // 0.786 wide (a 60-degree one 0.538); 0.4 below it 1.214 (1.462).
    const double above = 0.5 - 0.4 * tan15;
    CHECK(inside(*screw, gp_Pnt(4.9, 0, z + 1.5 + above - 0.02)));
    CHECK(!inside(*screw, gp_Pnt(4.9, 0, z + 1.5 + above + 0.02)));
    CHECK(inside(*screw, gp_Pnt(4.9, 0, z + 1.5 - above + 0.02)));
    CHECK(!inside(*screw, gp_Pnt(4.9, 0, z + 1.5 - above - 0.02)));
    const double below = 0.5 + 0.4 * tan15;
    CHECK(inside(*screw, gp_Pnt(4.1, 0, z + 1.5 + below - 0.02)));
    CHECK(!inside(*screw, gp_Pnt(4.1, 0, z + 1.5 + below + 0.02)));
    // The crest's flat, 0.366 P wide, on the major diameter.
    CHECK(inside(*screw, gp_Pnt(4.98, 0, z + 1.5 + 0.3)));
  }

  // In a nut: an 8 mm bore (the minor diameter) through a plate 12 thick.
  const ShapePtr plate = extrude("F2", {rectangle(1, -15, -15, 30, 30)}, 0, 12);
  const ShapePtr drill = extrude("F3", {circle(5, 0, 0, 4.0)}, -1, 13);
  const ShapePtr nut = boolean(BooleanOp::Cut, {plate.get()}, *drill).pieces.at(0).shape;
  ThreadSpec tapped = spec_of("F4", "F3:side(c5)", 2.0, inside_thread);
  tapped.flank_angle = flank;
  const ShapePtr threaded = modeled_thread(*nut, tapped);
  CHECK_NEAR_TOL(volume(*threaded), 30 * 30 * 12 - 12 * flanked_nut_hole_per_mm(2.0, inside_thread, tan15), 2e-5);
  CHECK(BRepCheck_Analyzer(threaded->occt()).IsValid());
  // Its tooth over the first half pitch at angle 0, its root at 5.25.
  CHECK(inside(*threaded, gp_Pnt(4.5, 0, 6.5)));
  CHECK(!inside(*threaded, gp_Pnt(4.5, 0, 7.5)));
  CHECK(!inside(*threaded, gp_Pnt(5.24, 0, 7.5)));
  CHECK(inside(*threaded, gp_Pnt(5.26, 0, 7.5)));

  // Tr10x4(P2): two starts, the same profile and volume; half a turn on
  // the groove has risen a whole pitch (a single-start one's half a
  // pitch).
  ThreadSpec two = tr;
  two.starts = 2;
  const ShapePtr lead_screw = modeled_thread(*rod, two);
  CHECK_NEAR_TOL(volume(*lead_screw), 12 * flanked_bolt_per_mm(2.0, outside, tan15), 2e-5);
  CHECK(BRepCheck_Analyzer(lead_screw->occt()).IsValid());
  for (const double z : {4.0, 6.0}) {
    CHECK(!inside(*lead_screw, at_angle(4.5, kPi, z + 0.5)));
    CHECK(inside(*lead_screw, at_angle(4.5, kPi, z + 1.5)));
    CHECK(!inside(*screw, at_angle(4.5, kPi, z + 1.5)));
    CHECK(inside(*screw, at_angle(4.5, kPi, z + 0.5)));
    // A quarter turn on, risen a quarter lead: half a pitch.
    CHECK(!inside(*lead_screw, at_angle(4.5, kPi / 2.0, z + 1.5)));
    CHECK(inside(*lead_screw, at_angle(4.5, kPi / 2.0, z + 0.5)));
  }

  // Flank angles that are none.
  ThreadSpec wrong = tr;
  wrong.flank_angle = 0.0;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "flank angle"));
  wrong.flank_angle = 50.0 * kPi / 180.0;
  CHECK(throws_with([&] { modeled_thread(*rod, wrong); }, "flank angle"));
}

namespace {

// The faces' names of a shape: each face's sorted and joined, all of them
// sorted (the faces' order in the shape aside).
std::vector<std::string> name_lists(const Shape& shape) {
  std::vector<std::string> lists;
  for (int i = 0; i < shape.face_count(); ++i) {
    NameList names = shape.face_names(i);
    std::sort(names.begin(), names.end());
    std::string joined;
    for (const std::string& name : names) {
      joined += name + ";";
    }
    lists.push_back(joined);
  }
  std::sort(lists.begin(), lists.end());
  return lists;
}

// Whether two threads' results are the same: the volume, the faces and
// their names.
bool same_result(const Shape& a, const Shape& b) {
  return near(volume(a), volume(b), 1e-6) && a.face_count() == b.face_count() && name_lists(a) == name_lists(b);
}

// A boss of radius `radius` standing on `body` at (x, y) from z 0 to
// `height`, its side named <feature>:side(c<curve>).
ShapePtr with_boss(const ShapePtr& body, const std::string& feature, int curve, double x, double y, double radius,
                   double height) {
  const ShapePtr boss = extrude(feature, {circle(curve, x, y, radius)}, 0, height);
  const BooleanResult joined = boolean(BooleanOp::Join, {body.get()}, *boss);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.at(0).shape;
}

// A block with ports as threaded designs have them (mitcad#843): four
// M45x3 bosses (one thinner than the major diameter: its teeth are added)
// for one thread feature, an M48x5 boss and an M48x5 bore through the
// block (wider than the minor diameter) for a feature each, and a strip
// of small holes away from them, so that the block has the 200 faces the
// local booleans need.
ShapePtr ported_block() {
  TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-150, -100, -20), 300, 200, 20).Shape();
  BRep_Builder builder;
  TopoDS_Compound holes;
  builder.MakeCompound(holes);
  for (int i = 0; i < 25; ++i) {
    for (int j = 0; j < 9; ++j) {
      const gp_Pnt at(-140 + 11.5 * i, -95 + 2.5 * j, -21);
      builder.Add(holes, BRepPrimAPI_MakeCylinder(gp_Ax2(at, gp_Dir(0, 0, 1)), 1.0, 22).Shape());
    }
  }
  plate = BRepAlgoAPI_Cut(plate, holes).Shape();
  ShapePtr block = std::make_shared<Shape>(plate);
  const double xs[] = {-100, -40, 20, 80};
  for (int k = 0; k < 4; ++k) {
    const std::string feature = "B" + std::to_string(k + 1);
    block = with_boss(block, feature, k + 1, xs[k], 30, k == 3 ? 22.3 : 22.5, 12);
  }
  block = with_boss(block, "B5", 5, -100, -35, 24, 15);
  const ShapePtr drill = extrude("H", {circle(6, 40, -35, 21.5)}, -21, 1);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {block.get()}, *drill);
  CHECK(drilled.pieces.size() == 1);
  return drilled.pieces.at(0).shape;
}

// A thread on the faces with 45-degree chamfers of the given diameter at
// every free end.
ThreadSpec chamfered(const std::string& feature, const std::vector<std::string>& faces, double size, double pitch,
                     double chamfer) {
  ThreadSpec spec = spec_of(feature, faces.front(), pitch, basic(size, pitch));
  spec.faces = faces;
  spec.chamfer = true;
  spec.chamfer_angle = kPi / 4.0;
  spec.chamfer_diameter = chamfer;
  spec.chamfer_ends.assign(faces.size(), {true, true});
  return spec;
}

double ms_since(std::chrono::steady_clock::time_point start) {
  return std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
}

void test_thread_tools_in_parallel() {
  // The threads of the ported block made face by face (as before
  // mitcad#843), planned with the tools built one after another, and
  // planned with them built on all cores: the same results, feature after
  // feature. The four bosses' tools go into one boolean per stage. The
  // times are printed (MITCAD_BOOLEAN_TRACE adds each face's and phase's).
  const ShapePtr block = ported_block();
  CHECK(block->face_count() >= 200);
  const std::vector<ThreadSpec> specs = {
      chamfered("T1", {"B1:side(c1)", "B2:side(c2)", "B3:side(c3)", "B4:side(c4)"}, 45, 3, 41),
      chamfered("T2", {"B5:side(c5)"}, 48, 5, 43),
      chamfered("T3", {"H:side(c6)"}, 48, 5, 50),
  };
  struct Mode {
    const char* name;
    detail::ThreadOptions options;
  };
  const Mode modes[] = {
      {"face by face", {false, false, false}},
      {"planned on one thread", {true, false, true}},
      {"planned in parallel", {true, true, true}},
  };
  constexpr std::size_t kModes = sizeof modes / sizeof modes[0];
  std::vector<ShapePtr> results(kModes, block);
  const detail::ThreadCounts before = detail::thread_counts();
  for (const ThreadSpec& spec : specs) {
    std::string line = "thread tools: " + spec.feature + " (" + std::to_string(spec.faces.size()) + " faces):";
    for (std::size_t m = 0; m < kModes; ++m) {
      const detail::ThreadOptionsScope scope(modes[m].options);
      const auto start = std::chrono::steady_clock::now();
      results[m] = modeled_thread(*results[m], spec);
      char took[96];
      std::snprintf(took, sizeof took, " %s %.0f ms", modes[m].name, ms_since(start));
      line += took;
    }
    std::printf("%s\n", line.c_str());
    for (std::size_t m = 1; m < kModes; ++m) {
      CHECK(same_result(*results[m], *results[0]));
      CHECK_NEAR(volume(*results[m]), volume(*results[0]));
      CHECK(results[m]->face_count() == results[0]->face_count());
    }
    // (A boss's free end is the low or the high one along its face's axis.)
    for (const std::string& face : spec.faces) {
      const Shape& result = *results.back();
      CHECK(!result.find_faces(face_name(spec.feature, "thread", face)).empty());
      CHECK(!result.find_faces(face_name(spec.feature, "thread_chamfer_low", face)).empty() ||
            !result.find_faces(face_name(spec.feature, "thread_chamfer_high", face)).empty());
    }
  }
  const ShapePtr& last = results.back();
  CHECK(!last->find_faces("T3:thread_chamfer_low(H:side(c6))").empty());
  CHECK(BRepCheck_Analyzer(last->occt()).IsValid());
  // Both planned runs put all six faces ahead and merged the four bosses'
  // stages (chamfers, beyond the crest, grooves; one band).
  const detail::ThreadCounts after = detail::thread_counts();
  std::printf("thread tools: %zu faces ahead, %zu in order, %zu merged booleans of %zu tools\n",
              after.planned_faces - before.planned_faces, after.ordered_faces - before.ordered_faces,
              after.merged_booleans - before.merged_booleans, after.merged_tools - before.merged_tools);
  CHECK(after.planned_faces - before.planned_faces == 12);
  CHECK(after.ordered_faces == before.ordered_faces);
  CHECK(after.merged_booleans - before.merged_booleans >= 2);
}

void test_thread_faces_close_together_in_order() {
  // Two bosses so close that their threads' reaches meet are threaded one
  // after the other, a third far from them ahead: the same result as face
  // by face.
  ShapePtr body = extrude("P", {rectangle(1, -20, -20, 70, 40)}, -10, 0);
  body = with_boss(body, "A", 2, 0, 0, 5, 10);
  body = with_boss(body, "B", 3, 7.5, 7.5, 5, 10);
  body = with_boss(body, "C", 4, 35, 0, 5, 10);
  const ThreadSpec spec = chamfered("T", {"A:side(c2)", "B:side(c3)", "C:side(c4)"}, 10, 1.5, 9);
  ShapePtr in_order;
  {
    const detail::ThreadOptionsScope scope({false, false, false});
    in_order = modeled_thread(*body, spec);
  }
  const detail::ThreadCounts before = detail::thread_counts();
  ShapePtr planned;
  {
    const detail::ThreadOptionsScope scope({true, true, true});
    planned = modeled_thread(*body, spec);
  }
  const detail::ThreadCounts after = detail::thread_counts();
  CHECK(after.planned_faces - before.planned_faces == 1);
  CHECK(after.ordered_faces - before.ordered_faces == 2);
  CHECK(same_result(*planned, *in_order));
  CHECK_NEAR(volume(*planned), volume(*in_order));
  CHECK(BRepCheck_Analyzer(planned->occt()).IsValid());
}

// A screw shaped as the parts library's self-forming screws (mitcad#901):
// a shank of radius `radius` from 0 into a head of radius `head` (named S,
// its side S:side(c1)) whose underside is serrated as the supplier's
// solids are: `teeth` planar ramps round the axis, each falling from a
// radial step, where it lies level at `length` from the axis to the rim,
// to `depth` lower at the rim by the step before it. The shank's side ends
// against the ramps: its top edge is a sawtooth from `length - depth *
// radius / head` up to `length`, and the steps' top edges lie level at
// `length`, the shank's highest point.
ShapePtr serrated_screw(double radius, double length, double head, int teeth, double depth) {
  const double low = length - depth;
  TopoDS_Shape cap = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, low), gp_Dir(0, 0, 1)), head, 4.0).Shape();
  const double reach = 1.5 * head;
  for (int k = 0; k < teeth; ++k) {
    const double a = 2.0 * kPi * k / teeth;
    const double b = 2.0 * kPi * (k + 1) / teeth;
    // The tooth's sector, out past the rim, from below the head up through
    // the ramp.
    const gp_Pnt centre(0, 0, low - 1.0);
    BRepBuilderAPI_MakePolygon sector(centre, gp_Pnt(reach * std::cos(a), reach * std::sin(a), low - 1.0),
                                      gp_Pnt(reach * std::cos(b), reach * std::sin(b), low - 1.0), true);
    const TopoDS_Shape prism =
        BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(sector.Wire(), true).Face(), gp_Vec(0, 0, depth + 2.0)).Shape();
    // What lies below the ramp's plane: through the step's radial line at
    // `length`, and `depth` lower at the rim by the step before.
    const gp_Pnt axis(0, 0, length);
    const gp_Pnt on_step(head * std::cos(b), head * std::sin(b), length);
    const gp_Pnt at_rim(head * std::cos(a), head * std::sin(a), low);
    gp_Dir normal(gp_Vec(axis, on_step).Crossed(gp_Vec(axis, at_rim)));
    if (normal.Z() < 0.0) {
      normal.Reverse();
    }
    const TopoDS_Face plane = BRepBuilderAPI_MakeFace(gp_Pln(axis, normal)).Face();
    const TopoDS_Shape below = BRepPrimAPI_MakeHalfSpace(plane, centre).Solid();
    cap = BRepAlgoAPI_Cut(cap, BRepAlgoAPI_Common(prism, below).Shape()).Shape();
  }
  const Shape head_shape(cap);
  const ShapePtr shank = extrude("S", {circle(1, 0, 0, radius)}, 0, length + 1.0);
  const BooleanResult joined = boolean(BooleanOp::Join, {&head_shape}, *shank);
  CHECK(joined.pieces.size() == 1);
  return joined.pieces.at(0).shape;
}

void test_thread_under_a_serrated_head() {
  // The parts library's self-forming screws (mitcad#901): S7x25 (M7 as its
  // stand-in) and S12x30 (M12), their shanks under heads serrated as the
  // supplier's: 24 ramps 0.2 mm deep at the rim. The shank's top end is
  // uneven, so the grooves stop a hundredth of a pitch short of its lowest
  // point. Up to its highest point they would cut into the head where the
  // ramps lie lower, and their end would lie on the steps' level top edges,
  // which left the shell open ("the thread produced an invalid shape").
  struct Case {
    double d;
    double pitch;
    double length;
    double head;
  };
  const Case cases[] = {{7, 1.0, 25, 8.85}, {12, 1.75, 30, 9.8}};
  const int teeth = 24;
  const double step = 2.0 * kPi / teeth;
  for (const Case& c : cases) {
    const double radius = c.d / 2.0;
    const double depth = 0.2;
    const ShapePtr screw = serrated_screw(radius, c.length, c.head, teeth, depth);
    // The ramp's height at a radius, at `f` of a tooth after its low side.
    const auto ramp = [&](double r, double f) {
      return c.length - depth * r / c.head * std::sin((1.0 - f) * step) / std::sin(step);
    };
    const double lowest = ramp(radius, 0.0);
    const Diameters size = basic(c.d, c.pitch);
    ShapePtr threaded;
    try {
      threaded = modeled_thread(*screw, spec_of("T", "S:side(c1)", c.pitch, size));
    } catch (const std::exception& error) {
      std::fprintf(stderr, "M%g under a serrated head: %s\n", c.d, error.what());
    }
    CHECK(threaded != nullptr);
    if (!threaded) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(threaded->occt()).IsValid());
    CHECK(!threaded->find_faces("T:thread(S:side(c1))").empty());
    // The thread from the tip to just under the shank's lowest point at the
    // head: the shank's section less the bolt's.
    const double removed = volume(*screw) - volume(*threaded);
    const double per_mm = kPi * radius * radius - bolt_per_mm(c.pitch, size);
    CHECK(removed > per_mm * (lowest - 0.02 * c.pitch));
    CHECK(removed < per_mm * lowest);
    // A groove and a tooth at the pitch diameter halfway up, on the X axis
    // (the groove spans the first half pitch from the tip there).
    const double rp = size.pitch / 2.0;
    const double middle = std::floor(0.5 * lowest / c.pitch) * c.pitch;
    CHECK(!inside(*threaded, gp_Pnt(rp, 0, middle + 0.25 * c.pitch)));
    CHECK(inside(*threaded, gp_Pnt(rp, 0, middle + 0.75 * c.pitch)));
    // The head is whole: over the threaded radii, between the ramps low
    // in each tooth and the steps' level, the material stays.
    for (int k = 0; k < teeth; k += 2) {
      const double angle = step * (k + 0.2);
      for (const double r : {size.minor / 2.0 + 0.05, rp, radius - 0.05}) {
        const gp_Pnt point(r * std::cos(angle), r * std::sin(angle), 0.5 * (ramp(r, 0.2) + c.length));
        CHECK(inside(*screw, point));
        CHECK(inside(*threaded, point));
      }
    }
  }
}

// The pieces of a thread counted from each end of its face (mitcad#1231):
// a part of a rod's face, 2 from its high end, 8, 9.5 and 12 long (other
// piece counts). Each piece's first name, which its edges are named after,
// is its place from the nearer end, `F3:low_thread(F2:side(c1))#k` or
// `F3:high_thread(…)#k`; its number along the thread (mitcad#490) stays
// as another name. The edges where the thread meets the plain face at
// either end keep their names whatever the count.
void test_thread_pieces_counted_from_their_ends() {
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  const std::string low = "F3:low_thread(F2:side(c1))#";
  const std::string high = "F3:high_thread(F2:side(c1))#";
  for (const double length : {8.0, 9.5, 12.0}) {
    ThreadSpec spec = spec_of("F3", "F2:side(c1)", 1.5, k6g);
    spec.full_length = false;
    spec.length = length;
    spec.offset = 2;
    const ShapePtr bolt = modeled_thread(*rod, spec);
    const int count = static_cast<int>(bolt->find_faces("F3:thread(F2:side(c1))").size());
    CHECK(count > 20);
    // Each piece's first name from the nearer end, each place once.
    int named = 0;
    double low_z = 0.0;
    double high_z = 0.0;
    double lowest = 100.0;
    double highest = -100.0;
    for (int i = 0; i < bolt->face_count(); ++i) {
      const NameList& names = bolt->face_names(i);
      if (!face_matches(names, "F3:thread(F2:side(c1))")) {
        continue;
      }
      const std::string& first = names.front();
      const bool from_low = first.rfind(low, 0) == 0;
      CHECK(from_low || first.rfind(high, 0) == 0);
      const int place = std::stoi(first.substr(first.find('#') + 1));
      CHECK(2 * place <= count);
      ++named;
      GProp_GProps properties;
      BRepGProp::SurfaceProperties(bolt->face(i), properties);
      const double z = properties.CentreOfMass().Z();
      lowest = std::min(lowest, z);
      highest = std::max(highest, z);
      if (first == low + "0") {
        low_z = z;
      }
      if (first == high + "0") {
        high_z = z;
      }
    }
    CHECK(named == count);
    CHECK(bolt->find_faces(low + "0").size() == 1);
    CHECK(bolt->find_faces(high + "0").size() == 1);
    // The end pieces at the ends: the face's cylinder's axis points down
    // here, so its high end, from which the part is measured, is the rod's
    // base, and the high end's piece lies 2 above it.
    CHECK(near(high_z, 2.0, 1e-6) && near(high_z, lowest, 1e-6));
    CHECK(near(low_z, 2.0 + length, 1e-6) && near(low_z, highest, 1e-6));
    // The edges with the plain face at both ends, by the same names.
    CHECK(!bolt->find_edges("E{F2:side(c1)|" + high + "0}").empty());
    CHECK(!bolt->find_edges("E{F2:side(c1)|" + low + "0}").empty());
  }
}

// The point at radius `r` from the Z axis, `degrees` about it from X, at
// height `z`.
gp_Pnt polar(double r, double degrees, double z) {
  const double a = degrees * kPi / 180.0;
  return gp_Pnt(r * std::cos(a), r * std::sin(a), z);
}

// The angle about the Z axis (degrees, 0 to 360) where the groove of a
// right-hand internal thread of `pitch` at angle 0, threaded from z = 0,
// has its middle at the pitch diameter at height `z`: it lies 3/4 pitch up
// at angle 0 and rises a pitch a turn.
double internal_groove_at(double pitch, double z) {
  const double turns = (z - 0.75 * pitch) / pitch;
  return 360.0 * (turns - std::floor(turns));
}

void test_thread_runs_out_into_a_countersink() {
  // (mitcad#1300) A Ø41 bore over z 0 … 15 through a plate 25 thick, a
  // 90-degree countersink above it (the cone from radius 20.5 at z = 15 to
  // 30.5 at the top), M45x3 internal over the whole bore. Past the bore's
  // top the cone lies beyond the crest (radius 20.876) everywhere: the
  // grooves run out into it by a pitch instead of stopping at z = 15 and
  // leaving the cone's material between radius 20.5 and the major radius
  // 22.5 over z 15 … 17 as a flat, unthreaded lip.
  const double pitch = 3.0;
  const Diameters m45 = basic(45, pitch);
  const ShapePtr plate = extrude("F2", {rectangle(1, -50, -50, 100, 100)}, 0, 25);
  HoleSpec sink;
  sink.feature = "F3";
  sink.positions = {gp_Ax1(gp_Pnt(0, 0, 25), gp_Dir(0, 0, -1))};
  sink.type = HoleType::Countersink;
  sink.diameter = 41;
  sink.countersink_diameter = 61;
  sink.countersink_angle = kPi / 2.0;
  sink.extent = HoleExtent::ThroughAll;
  sink.bodies = {plate};
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *hole_tool(sink));
  CHECK(drilled.pieces.size() == 1);
  const ShapePtr nut = drilled.pieces.at(0).shape;
  const ThreadSpec spec = spec_of("F4", "F3:hole0.wall", pitch, m45);
  const ShapePtr threaded = modeled_thread(*nut, spec);
  CHECK(BRepCheck_Analyzer(threaded->occt()).IsValid());
  // Where the grooves pass above the bore's top, in the cone's material
  // (the cone at radius 21 at z = 15.5, 22 at z = 16.5), nothing is left.
  const gp_Pnt lip_low = polar(21.3, internal_groove_at(pitch, 15.5), 15.5);
  const gp_Pnt lip_high = polar(22.2, internal_groove_at(pitch, 16.5), 16.5);
  CHECK(inside(*nut, lip_low) && inside(*nut, lip_high));
  CHECK(!inside(*threaded, lip_low));
  CHECK(!inside(*threaded, lip_high));
  // The tooth at the bore's top between them stays.
  CHECK(inside(*threaded, polar(21.3, internal_groove_at(pitch, 16.5), 15.0)));
  // Within the bore, the thread as before: a groove and a tooth halfway.
  CHECK(!inside(*threaded, polar(21.3, internal_groove_at(pitch, 7.5), 7.5)));
  CHECK(inside(*threaded, polar(21.3, internal_groove_at(pitch, 7.5), 9.0)));

  // The ends as cut before (an import that keeps them, `flat_ends`): the
  // grooves stop at z = 15, the lip stays, and the bore loses exactly the
  // profile over its 15 mm.
  ThreadSpec flat = spec;
  flat.flat_ends = true;
  const ShapePtr lipped = modeled_thread(*nut, flat);
  CHECK(inside(*lipped, lip_low));
  CHECK(inside(*lipped, lip_high));
  const double bore = kPi * 20.5 * 20.5;
  CHECK_NEAR_TOL(volume(*lipped), volume(*nut) - 15 * (nut_hole_per_mm(pitch, m45) - bore), 2e-5);
  // The run-out takes the grooves' share of the lip's ring.
  CHECK(volume(*lipped) - volume(*threaded) > 20.0);

  // Chamfer Ends asked for both ends: only the plate's bottom, the free
  // mouth, is countersunk; the cone at the top is its chamfer already, and
  // the grooves still run out into it.
  ThreadSpec chamfered = spec;
  chamfered.chamfer = true;
  chamfered.chamfer_angle = kPi / 4.0;
  chamfered.chamfer_diameter = m45.major + pitch / 10.0;
  chamfered.chamfer_ends = {{true, true}};
  const ShapePtr sunk = modeled_thread(*nut, chamfered);
  const bool low = !sunk->find_faces("F4:thread_chamfer_low(F3:hole0.wall)").empty();
  const bool high = !sunk->find_faces("F4:thread_chamfer_high(F3:hole0.wall)").empty();
  CHECK(low != high);
  CHECK(!inside(*sunk, lip_low));
  CHECK(!inside(*sunk, lip_high));
  CHECK(!inside(*sunk, polar(m45.major / 2.0 + 0.1, 0, 1e-3)));

  // A blind bore with a flat floor: its material lies past the bore's end
  // inside the crest, so the grooves stop at the floor as before. The
  // same bore 15 deep from the top of a block 25 high: exactly the profile
  // over the 15 mm, with flat ends or without.
  const ShapePtr block = extrude("F2", {rectangle(1, -50, -50, 100, 100)}, 0, 25);
  HoleSpec blind;
  blind.feature = "F3";
  blind.positions = {gp_Ax1(gp_Pnt(0, 0, 25), gp_Dir(0, 0, -1))};
  blind.diameter = 41;
  blind.flat = true;
  blind.extent = HoleExtent::Distance;
  blind.depth = 15;
  blind.bodies = {block};
  const BooleanResult bored = boolean(BooleanOp::Cut, {block.get()}, *hole_tool(blind));
  CHECK(bored.pieces.size() == 1);
  const ShapePtr cup = bored.pieces.at(0).shape;
  const ShapePtr floored = modeled_thread(*cup, spec);
  CHECK_NEAR_TOL(volume(*floored), volume(*cup) - 15 * (nut_hole_per_mm(pitch, m45) - bore), 2e-5);
  CHECK_NEAR_TOL(volume(*floored), volume(*modeled_thread(*cup, flat)), 1e-9);
  // The floor is whole under the grooves.
  CHECK(inside(*floored, polar(21.3, internal_groove_at(pitch, 9.5), 9.5)));
}

void test_thread_runs_out_along_its_chamfers() {
  // (mitcad#1300) A partial thread with Chamfer Ends: at an end inside its
  // face the chamfer is the run-out. M45x3 10 long in the middle of a Ø41
  // bore 15 deep (z 2.5 … 12.5): past each end the grooves go on inside
  // the 45-degree cone that narrows from the major radius 22.5 at the end
  // to the bore's 20.5, 2 mm further, and so does the cut of the bore to
  // the minor radius 20.876 (a chamfer of that step).
  const double pitch = 3.0;
  const Diameters m45 = basic(45, pitch);
  const ShapePtr plate = extrude("F2", {rectangle(1, -50, -50, 100, 100)}, 0, 15);
  const ShapePtr drill = extrude("F3", {circle(5, 0, 0, 20.5)}, -1, 16);
  const BooleanResult drilled = boolean(BooleanOp::Cut, {plate.get()}, *drill);
  CHECK(drilled.pieces.size() == 1);
  const ShapePtr nut = drilled.pieces.at(0).shape;
  ThreadSpec spec = spec_of("F4", "F3:side(c5)", pitch, m45);
  spec.full_length = false;
  spec.length = 10;
  spec.offset = 2.5;
  spec.chamfer = true;
  spec.chamfer_angle = kPi / 4.0;
  spec.chamfer_diameter = m45.major + pitch / 10.0;
  spec.chamfer_ends = {{true, true, true, true}};
  const ShapePtr threaded = modeled_thread(*nut, spec);
  CHECK(BRepCheck_Analyzer(threaded->occt()).IsValid());
  // Neither end is free: no chamfer's cone of its own.
  CHECK(threaded->find_faces("F4:thread_chamfer_low(F3:side(c5))").empty());
  CHECK(threaded->find_faces("F4:thread_chamfer_high(F3:side(c5))").empty());
  // Half a millimetre past each end the cone is at radius 22: the groove
  // is cut inside it, the material outside it stays.
  for (const double z : {2.0, 13.0}) {
    const double groove = internal_groove_at(pitch, z);
    CHECK(inside(*nut, polar(21.3, groove, z)));
    CHECK(!inside(*threaded, polar(21.3, groove, z)));
    CHECK(inside(*threaded, polar(22.2, groove, z)));
  }
  // The step from the bore to the minor radius: cut inside the cone (at
  // radius 21 a millimetre and a half past the end), the bore untouched
  // where the cone has passed it (radius 20.3, 2.2 past).
  for (const double z : {1.0, 14.0}) {
    CHECK(!inside(*threaded, polar(20.7, 0, z)));
  }
  for (const double z : {0.3, 14.7}) {
    CHECK(inside(*threaded, polar(20.7, 0, z)));
  }

  // Without the run-outs the ends stay flat, as before: the bore's
  // material past them is whole.
  ThreadSpec plain = spec;
  plain.chamfer_ends = {{true, true}};
  const ShapePtr flat = modeled_thread(*nut, plain);
  for (const double z : {2.0, 13.0}) {
    CHECK(inside(*flat, polar(21.3, internal_groove_at(pitch, z), z)));
  }
  CHECK(inside(*flat, polar(20.7, 0, 1.0)));
  const double bore = kPi * 20.5 * 20.5;
  CHECK_NEAR_TOL(volume(*flat), volume(*nut) - 10 * (nut_hole_per_mm(pitch, m45) - bore), 2e-5);
  CHECK(volume(*flat) - volume(*threaded) > 5.0);
  // `flat_ends` keeps them flat too.
  ThreadSpec kept = spec;
  kept.flat_ends = true;
  CHECK_NEAR_TOL(volume(*modeled_thread(*nut, kept)), volume(*flat), 1e-9);

  // An external thread: M10x1.5 8 long on a 10 mm rod 20 long (z 6 … 14).
  // Past each end the grooves go on outside the cone that widens from the
  // minor radius at the end to the rod's 5: half a millimetre past, at
  // radius 4.688, a groove's material at 4.85 is cut, at 4.5 it stays.
  const Diameters m10 = basic(10, 1.5);
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 5)}, 0, 20);
  ThreadSpec shaft = spec_of("F3", "F2:side(c1)", 1.5, m10);
  shaft.full_length = false;
  shaft.length = 8;
  shaft.offset = 6;
  shaft.chamfer = true;
  shaft.chamfer_angle = kPi / 4.0;
  shaft.chamfer_diameter = m10.minor - 0.15;
  shaft.chamfer_ends = {{true, true, true, true}};
  const ShapePtr bolt = modeled_thread(*rod, shaft);
  CHECK(BRepCheck_Analyzer(bolt->occt()).IsValid());
  // The groove of a right-hand external thread at angle 0 lies a quarter
  // pitch up at angle 0: its middle at z is a quarter turn earlier than an
  // internal one's.
  for (const double z : {5.5, 14.5}) {
    const double turns = (z - 0.25 * 1.5) / 1.5;
    const double groove = 360.0 * (turns - std::floor(turns));
    CHECK(!inside(*bolt, polar(4.85, groove, z)));
    CHECK(inside(*bolt, polar(4.5, groove, z)));
  }
  ThreadSpec square = shaft;
  square.chamfer_ends = {{true, true}};
  const ShapePtr squared = modeled_thread(*rod, square);
  CHECK_NEAR_TOL(volume(*squared), 12 * kPi * 25 + 8 * bolt_per_mm(1.5, m10), 2e-5);
  CHECK(volume(*squared) - volume(*bolt) > 1.0);
}

} // namespace

// The suite's parts thread.1-thread.3 (test_main.cpp), each a ctest of its
// own so that they run beside each other (mitcad#870). Every test is in
// exactly one part; the slow ones (the large body, the taper pipe threads,
// the rounding along a thread, the parallel tools) are in different parts.
// guarded() prints the time of each test of a second or more, for moving a
// line when the parts drift apart.
void thread_tests_1() {
  guarded("test_bolt", test_bolt);
  guarded("test_bolt_profiles", test_bolt_profiles);
  guarded("test_shoulder_and_partial", test_shoulder_and_partial);
  guarded("test_large_body", test_large_body);
  guarded("test_thread_pieces_join_smoothly", test_thread_pieces_join_smoothly);
  guarded("test_custom_pitch", test_custom_pitch);
  guarded("test_thread_runs_out_into_a_countersink", test_thread_runs_out_into_a_countersink);
}

void thread_tests_2() {
  guarded("test_taper_pipe_threads", test_taper_pipe_threads);
  guarded("test_tapped_holes", test_tapped_holes);
  guarded("test_sized_chamfered_end", test_sized_chamfered_end);
  guarded("test_rounding_along_a_thread_keeps_its_pieces", test_rounding_along_a_thread_keeps_its_pieces);
  guarded("test_thread_faces_close_together_in_order", test_thread_faces_close_together_in_order);
  guarded("test_multi_start_threads", test_multi_start_threads);
  guarded("test_thread_under_a_serrated_head", test_thread_under_a_serrated_head);
}

void thread_tests_3() {
  guarded("test_bolt_clearance", test_bolt_clearance);
  guarded("test_hole_clearance", test_hole_clearance);
  guarded("test_end_chamfers", test_end_chamfers);
  guarded("test_pieces_on_screw_surfaces_and_legacy_names", test_pieces_on_screw_surfaces_and_legacy_names);
  guarded("test_thread_tools_in_parallel", test_thread_tools_in_parallel);
  guarded("test_trapezoidal_threads", test_trapezoidal_threads);
  guarded("test_thread_pieces_counted_from_their_ends", test_thread_pieces_counted_from_their_ends);
  guarded("test_thread_runs_out_along_its_chamfers", test_thread_runs_out_along_its_chamfers);
}

} // namespace test
