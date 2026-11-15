// SPDX-License-Identifier: MIT
// Press Pull on a face continued Straight or Along the walls (press_pull.hpp,
// mitcad#1850): a bent tube's end carried round the bend, past the walls'
// inner radius and the bend's own arc up to a closed ring, square to the
// run, with the volume of the revolution it continues; Straight adds a
// cylinder; Offset is as before. Draft and Scale tapers on a T-slot bar's
// end and a round rod's, straight and along a bend, outward and inward, by
// their analytic volumes: the two the same cone on the rod, different on
// the T-slot. Refusals in words: walls that neither turn nor run straight,
// a turn into the body, a taper that closes the face.

#include <cmath>
#include <exception>
#include <initializer_list>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <GProp_GProps.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/press_pull.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/revolve.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

double volume_of(const Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape.occt(), props);
  return props.Mass();
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

int face_of(const Shape& shape, const std::string& name) {
  for (int f = 0; f < shape.face_count(); ++f) {
    if (has_name(shape.face_names(f), name)) {
      return f;
    }
  }
  return -1;
}

gp_Pnt middle_of(const Shape& shape, int f) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape.face(f), props);
  return props.CentreOfMass();
}

PressPullSpec pull(const std::string& face, PressPullRun run, double distance,
                   PressPullTaper taper = PressPullTaper::None, double value = 0.0) {
  PressPullSpec spec;
  spec.feature = "F3";
  spec.faces = {face};
  spec.run = run;
  spec.distance = distance;
  spec.taper = taper;
  if (taper == PressPullTaper::Draft) {
    spec.taper_angle = value;
  } else if (taper == PressPullTaper::Scale) {
    spec.end_scale = value;
  }
  return spec;
}

constexpr double kDegree = kPi / 180.0;

// A circle of radius r at (R, 0) on the XY plane turned a quarter about the
// Y axis: a bent tube from the XY plane round to the plane x = 0, its end
// facing -x.
constexpr double kBend = 30.0;
constexpr double kTube = 10.0;

ShapePtr bent(const std::vector<Region>& regions, double angle = kPi / 2.0) {
  RevolveSpec spec;
  spec.feature = "F2";
  spec.regions = regions;
  spec.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0));
  spec.angle1 = angle;
  return revolve(spec);
}

const std::string kTubeEnd = face_name("F2", "end", "r{c1}");

// A 20 x 20 bar with a T-slot in its top side (a 6 wide opening 2 deep over
// a 12 wide undercut 4 high) and a bore of radius 2.5 below the slot,
// centred on (cx, cy) and symmetric about the line x = cx. Lines c1..c12,
// the bore c13. Its area, outline length and the corners' term of an offset
// by d of its loops, sharp corners (8 convex, 4 concave) and the bore
// shrinking: A(d) = A0 + P d + (4 - pi) d^2.
Region tslot(double cx, double cy) {
  const double points[12][2] = {{-10, -10}, {10, -10}, {10, 10}, {3, 10}, {3, 8},   {6, 8},
                                {6, 4},     {-6, 4},   {-6, 8},  {-3, 8}, {-3, 10}, {-10, 10}};
  Region region;
  region.name = "r{c1,c13}";
  Loop outline;
  for (int i = 0; i < 12; ++i) {
    const int j = (i + 1) % 12;
    Segment line;
    line.name = "c" + std::to_string(i + 1);
    line.kind = SegmentKind::Line;
    line.start = gp_Pnt2d(cx + points[i][0], cy + points[i][1]);
    line.end = gp_Pnt2d(cx + points[j][0], cy + points[j][1]);
    outline.segments.push_back(line);
  }
  region.loops.push_back(outline);
  Region bore = circle_region(cx, cy - 2.0, 2.5);
  bore.loops.front().segments.front().name = "c13";
  region.loops.push_back(bore.loops.front());
  return region;
}

const double kSlotArea = 400.0 - 12.0 - 48.0 - kPi * 2.5 * 2.5;
const double kSlotPerimeter = 104.0 + 5.0 * kPi;
const double kSlotCorners = 4.0 - kPi;
const std::string kSlotEnd = face_name("F2", "end", "r{c1,c13}");

// The volume of a run of length `length` whose loops move out by
// t tan(angle) at run length t (Draft), from the face's area, outline
// length and corners' term.
double drafted(double area, double perimeter, double corners, double length, double angle) {
  const double d = length * std::tan(angle);
  return length * (area + perimeter * d / 2.0 + corners * d * d / 3.0);
}

// The volume of a run whose face scales about its area centre from 1 to s.
double scaled(double area, double length, double s) { return area * length * (1.0 + s + s * s) / 3.0; }

void test_bent_tube_along() {
  const ShapePtr tube = bent({circle(1, kBend, 0.0, kTube)});
  const double area = kPi * kTube * kTube;
  CHECK_NEAR(volume_of(*tube), area * kBend * kPi / 2.0);

  // How the walls continue: a turn about the Y axis, the centre 30 from it.
  const FaceRun run = face_run(*tube, kTubeEnd);
  CHECK(run.along == FaceRun::Along::Turn);
  CHECK(std::abs(std::abs(run.axis.Direction().Y()) - 1.0) < 1e-9);
  CHECK(std::abs(run.radius - kBend) < 1e-7);
  CHECK(run.centre.Distance(gp_Pnt(0.0, 0.0, -kBend)) < 1e-7);

  // Short, past the walls' inner radius (R - r = 20), past the bend's own
  // arc: the revolution's volume, the end square to the run where the
  // centre's arc puts it.
  for (const double distance : {5.0, 25.0, 60.0}) {
    const ShapePtr longer = press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, distance));
    CHECK(valid(*longer));
    CHECK_NEAR(volume_of(*longer), area * (kBend * kPi / 2.0 + distance));
    const int end = face_of(*longer, kTubeEnd);
    CHECK(end >= 0);
    CHECK(end == face_of(*longer, face_name("F3", "end", kTubeEnd)));
    if (end >= 0) {
      const double angle = kPi / 2.0 + distance / kBend;
      const gp_Pnt where(kBend * std::cos(angle), 0.0, -kBend * std::sin(angle));
      CHECK(middle_of(*longer, end).Distance(where) < 1e-6);
      const gp_Dir normal = BRepAdaptor_Surface(longer->face(end)).Plane().Axis().Direction();
      const gp_Dir tangent(-std::sin(angle), 0.0, -std::cos(angle));
      CHECK(std::abs(std::abs(normal.Dot(tangent)) - 1.0) < 1e-9);
    }
    CHECK(faces_named(*longer, face_name("F3", "side", edge(kTubeEnd, face_name("F2", "side", "c1")))) >= 1);
  }

  // Round to the tube's other end: a closed ring without ends.
  const ShapePtr ring = press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, kBend * 1.5 * kPi));
  CHECK(valid(*ring));
  CHECK_NEAR(volume_of(*ring), area * 2.0 * kPi * kBend);
  CHECK(faces_named(*ring, kTubeEnd) == 0);

  // Further, the turn runs into the tube itself.
  CHECK(throws_with([&] { press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, kBend * (1.5 * kPi + 0.3))); },
                    "the turn runs into the body itself"));
  try {
    press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, kBend * (1.5 * kPi + 0.3)));
  } catch (const std::exception& error) {
    // The message has no names; the tube's start is painted.
    CHECK(error_message(error.what()).find("F2") == std::string::npos);
    CHECK(std::string(error.what()).find(face_name("F2", "start", "r{c1}")) != std::string::npos);
  }

  // Inward, a cut along the bend.
  const ShapePtr shorter = press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, -10.0));
  CHECK(valid(*shorter));
  CHECK_NEAR(volume_of(*shorter), area * (kBend * kPi / 2.0 - 10.0));

  // Straight: the cylinder added at the end.
  const ShapePtr straight = press_pull(*tube, pull(kTubeEnd, PressPullRun::Straight, 20.0));
  CHECK(valid(*straight));
  CHECK_NEAR(volume_of(*straight), area * (kBend * kPi / 2.0 + 20.0));

  // Offset as before: the end plane moved 5 mm along its normal (to
  // x = -5), the torus wall extended along its own surface to it. Exactly
  // the tube and the slab of the whole torus between x = -5 and x = 0 below
  // z = 0, with the tube's faces (the end moved, not added).
  const ShapePtr offset = offset_faces("F3", *tube, {kTubeEnd}, 5.0);
  CHECK(valid(*offset));
  const TopoDS_Shape torus =
      BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0)), kBend, kTube).Shape();
  const TopoDS_Shape slab = BRepPrimAPI_MakeBox(gp_Pnt(-5.0, -20.0, -50.0), gp_Pnt(0.0, 20.0, 0.0)).Shape();
  BRepAlgoAPI_Common grown(torus, slab);
  CHECK(grown.IsDone());
  GProp_GProps added;
  BRepGProp::VolumeProperties(grown.Shape(), added);
  CHECK_NEAR(volume_of(*offset), volume_of(*tube) + added.Mass());
  CHECK(offset->face_count() == tube->face_count());
  CHECK(faces_named(*offset, kTubeEnd) == 1);

  // The tube is left as it was.
  CHECK_NEAR(volume_of(*tube), area * kBend * kPi / 2.0);
}

// A bar's walls run straight: Along carries the end along them.
void test_straight_bar_along() {
  const ShapePtr bar = extrude("F2", {rectangle(1, 0, 0, 20, 10)}, 0, 30);
  const std::string top = end_cap("F2", 1);
  const FaceRun run = face_run(*bar, top);
  CHECK(run.along == FaceRun::Along::Straight);
  CHECK(run.direction.IsEqual(gp_Dir(0.0, 0.0, 1.0), 1e-9));
  const ShapePtr taller = press_pull(*bar, pull(top, PressPullRun::Along, 5.0));
  CHECK(valid(*taller));
  CHECK_NEAR(volume_of(*taller), 20.0 * 10.0 * 35.0);
}

// Draft and Scale on a straight T-slot bar's end, outward and inward, and
// a cut; the two differ on the T-slot.
void test_tslot_straight() {
  const ShapePtr bar = extrude("F2", {tslot(0.0, 0.0)}, 0, 40);
  const double body = kSlotArea * 40.0;
  CHECK_NEAR(volume_of(*bar), body);
  for (const double angle : {3.0 * kDegree, -3.0 * kDegree}) {
    const ShapePtr drafted_bar =
        press_pull(*bar, pull(kSlotEnd, PressPullRun::Straight, 10.0, PressPullTaper::Draft, angle));
    CHECK(valid(*drafted_bar));
    CHECK_NEAR(volume_of(*drafted_bar), body + drafted(kSlotArea, kSlotPerimeter, kSlotCorners, 10.0, angle));
  }
  for (const double s : {1.1, 0.9}) {
    const ShapePtr scaled_bar = press_pull(*bar, pull(kSlotEnd, PressPullRun::Straight, 10.0, PressPullTaper::Scale, s));
    CHECK(valid(*scaled_bar));
    CHECK_NEAR_TOL(volume_of(*scaled_bar), body + scaled(kSlotArea, 10.0, s), 1e-5);
  }
  // The bar's outline grown alike both ways (half width 10 by 10 tan 3°):
  // the slot narrows with a draft, keeps its proportions with a scale.
  const double grown = 10.0 * std::tan(3.0 * kDegree);
  const double s = (10.0 + grown) / 10.0;
  const double draft_volume = volume_of(
      *press_pull(*bar, pull(kSlotEnd, PressPullRun::Straight, 10.0, PressPullTaper::Draft, 3.0 * kDegree)));
  const double scale_volume =
      volume_of(*press_pull(*bar, pull(kSlotEnd, PressPullRun::Straight, 10.0, PressPullTaper::Scale, s)));
  CHECK(std::abs(draft_volume - scale_volume) > 1.0);
  // Inward, narrowing into the bar: the piece cut away.
  const ShapePtr cut =
      press_pull(*bar, pull(kSlotEnd, PressPullRun::Straight, -10.0, PressPullTaper::Draft, -3.0 * kDegree));
  CHECK(valid(*cut));
  CHECK_NEAR(volume_of(*cut), body - drafted(kSlotArea, kSlotPerimeter, kSlotCorners, 10.0, -3.0 * kDegree));
}

// Draft and Scale along a bent T-slot bar: by Pappus the straight run's
// volumes over the centre's arc, as the slot is symmetric about the line
// through its area centre parallel to the axis.
void test_tslot_along() {
  constexpr double kRadius = 40.0;
  const ShapePtr bar = bent({tslot(kRadius, 0.0)});
  const double body = kSlotArea * kRadius * kPi / 2.0;
  CHECK_NEAR(volume_of(*bar), body);
  const double length = 20.0;
  const ShapePtr plain = press_pull(*bar, pull(kSlotEnd, PressPullRun::Along, length));
  CHECK(valid(*plain));
  CHECK_NEAR(volume_of(*plain), body + kSlotArea * length);
  for (const double angle : {2.0 * kDegree, -2.0 * kDegree}) {
    const ShapePtr drafted_bar =
        press_pull(*bar, pull(kSlotEnd, PressPullRun::Along, length, PressPullTaper::Draft, angle));
    CHECK(valid(*drafted_bar));
    CHECK_NEAR_TOL(volume_of(*drafted_bar), body + drafted(kSlotArea, kSlotPerimeter, kSlotCorners, length, angle),
                   1e-4);
    CHECK(faces_named(*drafted_bar, kSlotEnd) == 1);
  }
  for (const double s : {1.1, 0.9}) {
    const ShapePtr scaled_bar = press_pull(*bar, pull(kSlotEnd, PressPullRun::Along, length, PressPullTaper::Scale, s));
    CHECK(valid(*scaled_bar));
    CHECK_NEAR_TOL(volume_of(*scaled_bar), body + scaled(kSlotArea, length, s), 1e-4);
  }
}

// On a round rod and a bent tube the two tapers make the same cone.
void test_round_tapers_agree() {
  const double length = 20.0;
  const double angle = 5.0 * kDegree;
  const double grown = length * std::tan(angle);
  const double s = (kTube + grown) / kTube;
  const double cone = kPi * length * (kTube * kTube + kTube * grown + grown * grown / 3.0);

  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, kTube)}, 0, 40);
  const double rod_volume = kPi * kTube * kTube * 40.0;
  const std::string top = face_name("F2", "end", "r{c1}");
  const double draft_rod =
      volume_of(*press_pull(*rod, pull(top, PressPullRun::Straight, length, PressPullTaper::Draft, angle)));
  const double scale_rod =
      volume_of(*press_pull(*rod, pull(top, PressPullRun::Straight, length, PressPullTaper::Scale, s)));
  CHECK_NEAR(draft_rod, rod_volume + cone);
  CHECK_NEAR_TOL(scale_rod, rod_volume + cone, 1e-5);

  const ShapePtr tube = bent({circle(1, kBend, 0.0, kTube)});
  const double tube_volume = kPi * kTube * kTube * kBend * kPi / 2.0;
  const ShapePtr draft_tube =
      press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, length, PressPullTaper::Draft, angle));
  const ShapePtr scale_tube = press_pull(*tube, pull(kTubeEnd, PressPullRun::Along, length, PressPullTaper::Scale, s));
  CHECK(valid(*draft_tube));
  CHECK(valid(*scale_tube));
  CHECK_NEAR_TOL(volume_of(*draft_tube), tube_volume + cone, 1e-4);
  CHECK_NEAR_TOL(volume_of(*scale_tube), tube_volume + cone, 1e-4);
  CHECK_NEAR_TOL(volume_of(*draft_tube), volume_of(*scale_tube), 1e-4);
}

// Refusals in words, with no names in the message.
void test_refusals() {
  // A tapered block's top: its leaning sides neither turn nor run straight.
  ExtrudeFeatureSpec frustum;
  frustum.feature = "F2";
  frustum.regions = {rectangle(1, 0, 0, 20, 20)};
  frustum.side1.distance = 10.0;
  frustum.side1.taper = -10.0 * kDegree;
  const ShapePtr block = extrude_feature(frustum);
  const std::string top = face_name("F2", "end", rectangle_region_name(1));
  const FaceRun run = face_run(*block, top);
  CHECK(run.along == FaceRun::Along::None);
  CHECK(error_message(run.refusal) == "the walls do not continue as a turn or a straight run; use Offset or Straight");
  CHECK(throws_with([&] { press_pull(*block, pull(top, PressPullRun::Along, 5.0)); },
                    "the walls do not continue as a turn or a straight run"));
  // Straight still works there.
  CHECK(valid(*press_pull(*block, pull(top, PressPullRun::Straight, 5.0))));

  // A draft that closes a rod's end, a scale of nothing.
  const ShapePtr rod = extrude("F2", {circle(1, 0, 0, 10)}, 0, 40);
  const std::string end = face_name("F2", "end", "r{c1}");
  CHECK(throws_with(
      [&] { press_pull(*rod, pull(end, PressPullRun::Straight, 20.0, PressPullTaper::Draft, -40.0 * kDegree)); },
      "the taper closes the face to nothing"));
  CHECK(throws_with([&] { press_pull(*rod, pull(end, PressPullRun::Straight, 20.0, PressPullTaper::Scale, 0.0)); },
                    "the end scale must be more than 0"));
  // Not flat, no distance.
  CHECK(throws_with([&] { press_pull(*rod, pull(face_name("F2", "side", "c1"), PressPullRun::Straight, 5.0)); },
                    "is not flat"));
  CHECK(throws_with([&] { press_pull(*rod, pull(end, PressPullRun::Straight, 0.0)); }, "must not be zero"));
}

} // namespace

void press_pull_tests() {
  guarded("test_bent_tube_along", test_bent_tube_along);
  guarded("test_straight_bar_along", test_straight_bar_along);
  guarded("test_tslot_straight", test_tslot_straight);
  guarded("test_tslot_along", test_tslot_along);
  guarded("test_round_tapers_agree", test_round_tapers_agree);
  guarded("test_refusals", test_refusals);
}

} // namespace test
