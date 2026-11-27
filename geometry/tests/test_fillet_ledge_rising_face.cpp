// SPDX-License-Identifier: MIT
// A rounding wider than the ledge beside its edge, where the face beyond
// the ledge rises from it (GitHub mitcad/Mitcad#620's simpler case,
// mitcad#2352).
//
// A block 30 (x) by 20 (y) by 10 (z) with a spike on top: its foot 1 in
// from all four sides (x 1 to 29, y 1 to 19 at z 10), tapered so that its
// faces rise 12 while running 8.5 inwards. The block's top is a frame 1
// wide round the spike, one face. The front top edge (y 0, z 10) is
// rounded with 1.5, 2 and 4: the ball takes the ledge (the frame's front
// strip) away and touches the block's front and the spike's front face,
// continued down past its foot. In the section across the edge, the
// spike's front, continued, meets the front (y 0) at
// V = (0, 10 - 12 / 8.5); the rounding is tangent to both lines, its
// contacts at the distance r / tan(alpha / 2) from V (alpha the angle
// between them inside the material), the same all along the edge (the
// contact on the spike's front stays below the frame up to a rounding of
// about 5.4, so the rounding ends on the block's side faces as on a
// prism). The volume taken away: the section between the front, the
// ledge, the spike's front and the ball's arc, along the 30 of the edge.
//
// Before, the rounding was built with the ledge's plane continued under
// the spike, and the spike's front grown down to meet it: at 4 the
// rounding lay wholly inside that material, a sharp edge left where the
// spike's front met the block's front (no error); at 2 a step was left.
// Each: one valid closed solid without a note, the rounding's face named
// after the edge, the volume exact within 1e-7, and the section at x 15
// holding the rounding's arc about its exact centre between its exact
// contacts.

#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/profile.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kLength = 30.0; // the edge's length (x)
constexpr double kTop = 10.0;    // the block's top
constexpr double kLedge = 1.0;   // the ledge's width
constexpr double kRun = 8.5;     // the spike's faces run inwards ...
constexpr double kRise = 12.0;   // ... while they rise

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return std::abs(props.Mass());
}

bool closed(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1;
}

ShapePtr spiked_block() {
  ExtrudeSpec block;
  block.feature = "F1";
  block.regions = {rectangle_region(0, 0, kLength, 20)};
  block.direction = gp_Dir(0, 0, 1);
  block.start = 0.0;
  block.end = kTop;
  const ShapePtr base = extrude(block);
  ExtrudeFeatureSpec spike;
  spike.feature = "F2";
  spike.frame.origin = gp_Pnt(0, 0, kTop);
  spike.regions = {rectangle_region(kLedge, kLedge, kLength - 2.0 * kLedge, 20 - 2.0 * kLedge)};
  spike.direction = gp_Dir(0, 0, 1);
  spike.side1.distance = kRise;
  spike.side1.taper = -std::atan(kRun / kRise);
  const ShapePtr tip = extrude_feature(spike);
  return boolean(BooleanOp::Join, {base.get()}, *tip).pieces.at(0).shape;
}

// The rounding by r in the section (y, z): its contacts on the front
// (0, z1) and on the spike's front continued (y2, z2), its centre, and the
// area it takes away.
struct Section {
  double z1 = 0.0;
  double y2 = 0.0;
  double z2 = 0.0;
  double cy = 0.0;
  double cz = 0.0;
  double removed = 0.0;
};

Section rounding_of(double r) {
  const double len = std::sqrt(kRun * kRun + kRise * kRise);
  const double uy = kRun / len;
  const double uz = kRise / len;
  // V, where the spike's front continued meets y = 0.
  const double vz = kTop - kLedge * kRise / kRun;
  // The angle at V inside the material, between down the front (0, -1)
  // and up the spike's front (uy, uz).
  const double alpha = std::acos(-uz);
  const double reach = r / std::tan(0.5 * alpha);
  Section s;
  s.z1 = vz - reach;
  s.y2 = reach * uy;
  s.z2 = vz + reach * uz;
  s.cy = r;
  s.cz = s.z1;
  // The quadrilateral (0, z1), (0, top), (ledge, top), (y2, z2), less the
  // circular segment between the contacts.
  const double ys[4] = {0.0, 0.0, kLedge, s.y2};
  const double zs[4] = {s.z1, kTop, kTop, s.z2};
  double twice = 0.0;
  for (int i = 0; i < 4; ++i) {
    twice += ys[i] * zs[(i + 1) % 4] - ys[(i + 1) % 4] * zs[i];
  }
  const double theta = kPi - alpha;
  s.removed = 0.5 * std::abs(twice) - 0.5 * r * r * (theta - std::sin(theta));
  return s;
}

// The front top edge's name.
std::string front_edge(const Shape& body) {
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.X() - 0.5 * kLength) < 1e-6 && std::abs(m.Y()) < 1e-6 && std::abs(m.Z() - kTop) < 1e-6) {
      return body.edge_name(e);
    }
  }
  return {};
}

bool holds_face(const Shape& shape, const std::string& name) {
  for (int f = 0; f < shape.face_count(); ++f) {
    for (const std::string& held : shape.face_names(f)) {
      if (held == name || held.compare(0, name.size() + 1, name + "#") == 0) {
        return true;
      }
    }
  }
  return false;
}

// Whether the section at x 15 holds a circle of radius r about (15, cy, cz)
// from one contact to the other.
bool arc_in_section(const TopoDS_Shape& shape, double r, const Section& s) {
  BRepAlgoAPI_Section section(shape, gp_Pln(gp_Pnt(0.5 * kLength, 0, 0), gp_Dir(1, 0, 0)), false);
  section.ComputePCurveOn1(false);
  section.Approximation(false);
  section.Build();
  if (!section.IsDone()) {
    return false;
  }
  const gp_Pnt c1(0.5 * kLength, 0.0, s.z1);
  const gp_Pnt c2(0.5 * kLength, s.y2, s.z2);
  for (TopExp_Explorer ex(section.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
    if (curve.GetType() != GeomAbs_Circle) {
      continue;
    }
    const gp_Circ circle = curve.Circle();
    const gp_Pnt a = curve.Value(curve.FirstParameter());
    const gp_Pnt b = curve.Value(curve.LastParameter());
    std::printf("  section's arc: about (%.6f, %.6f, %.6f) radius %.6f from (%.6f, %.6f) to (%.6f, %.6f)\n",
                circle.Location().X(), circle.Location().Y(), circle.Location().Z(), circle.Radius(), a.Y(), a.Z(),
                b.Y(), b.Z());
    if (std::abs(circle.Radius() - r) < 1e-6 &&
        circle.Location().Distance(gp_Pnt(0.5 * kLength, s.cy, s.cz)) < 1e-5 &&
        ((a.Distance(c1) < 1e-5 && b.Distance(c2) < 1e-5) || (a.Distance(c2) < 1e-5 && b.Distance(c1) < 1e-5))) {
      return true;
    }
  }
  return false;
}

void rounded_at(double r) {
  const ShapePtr body = spiked_block();
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  const std::string edge = front_edge(*body);
  CHECK(!edge.empty());
  if (edge.empty()) {
    return;
  }
  ShapePtr made;
  try {
    made = fillet("F9", *body, {edge}, r);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "rounding %g: fails: %s\n", r, error.what());
    CHECK(false);
    return;
  }
  CHECK(made != nullptr);
  if (!made) {
    return;
  }
  const TopoDS_Shape& shape = made->occt();
  CHECK(closed(shape));
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  for (const std::string& note : made->notes()) {
    std::fprintf(stderr, "rounding %g: note: %s\n", r, note.c_str());
  }
  CHECK(made->notes().empty());
  CHECK(holds_face(*made, "F9:fillet(" + edge + ")"));
  const Section s = rounding_of(r);
  const double before = volume_of(body->occt());
  const double after = volume_of(shape);
  const double exact = before - kLength * s.removed;
  std::printf("rounding %g: volume %.9f -> %.9f, exact %.9f\n", r, before, after, exact);
  CHECK_NEAR_TOL(after, exact, 1e-7);
  CHECK(arc_in_section(shape, r, s));
}

void rounded_at_1_5() { rounded_at(1.5); }
void rounded_at_2() { rounded_at(2.0); }
void rounded_at_4() { rounded_at(4.0); }

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_ledge_rising_face) {
  test::guarded("ledge rounded at 1.5", test::rounded_at_1_5);
  test::guarded("ledge rounded at 2", test::rounded_at_2);
  test::guarded("ledge rounded at 4", test::rounded_at_4);
  if (test::failures == 0) {
    std::puts("test_fillet_ledge_rising_face: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
