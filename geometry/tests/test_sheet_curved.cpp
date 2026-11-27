// SPDX-License-Identifier: MIT
// Curved sheet panels (mitcad#2435): conical and cylindrical panels built
// from their developed charts with the analytic volumes, the development
// of a cone's and a cylinder's face, and how faces bend.
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/primitive.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/sheet.hpp"
#include "mitcad/geometry/sheet_develop.hpp"
#include "bundled_test.hpp"

namespace {
using namespace mitcad::geometry;

MaterialEdge line_edge(const std::string& curve, gp_Pnt2d a, gp_Pnt2d b) {
  Segment s;
  s.kind = SegmentKind::Line;
  s.start = a;
  s.end = b;
  return {{curve, 0}, {s, {0, 1}}};
}

MaterialEdge arc_edge(const std::string& curve, double radius, double from, double to) {
  Segment s;
  s.kind = SegmentKind::Arc;
  s.center = {0, 0};
  s.radius = radius;
  s.start_angle = std::min(from, to);
  s.end_angle = std::max(from, to);
  return {{curve, 0}, {s, {from, to}}};
}

// The annular sector rho1..rho2, 0..phi about the chart's origin.
MaterialRegion sector(double rho1, double rho2, double phi) {
  MaterialLoop loop;
  const gp_Pnt2d e(std::cos(phi), std::sin(phi));
  loop.edges.push_back(line_edge("F1:sheet_curve(0)", {rho1, 0}, {rho2, 0}));
  loop.edges.push_back(arc_edge("F1:sheet_curve(1)", rho2, 0, phi));
  loop.edges.push_back(line_edge("F1:sheet_curve(2)", {rho2 * e.X(), rho2 * e.Y()}, {rho1 * e.X(), rho1 * e.Y()}));
  loop.edges.push_back(arc_edge("F1:sheet_curve(3)", rho1, phi, 0));
  return {{loop}};
}

MaterialRegion rectangle(double width, double height) {
  MaterialLoop loop;
  loop.edges.push_back(line_edge("F1:sheet_curve(0)", {0, 0}, {width, 0}));
  loop.edges.push_back(line_edge("F1:sheet_curve(1)", {width, 0}, {width, height}));
  loop.edges.push_back(line_edge("F1:sheet_curve(2)", {width, height}, {0, height}));
  loop.edges.push_back(line_edge("F1:sheet_curve(3)", {0, height}, {0, 0}));
  return {{loop}};
}

const SheetDomainRef kPanel{SheetDomainKind::Panel, "F1", 0};

double built_volume(const SheetChartShapeSpec& spec) {
  const auto built = sheet_shape(spec);
  CHECK(built.solids.size() == 1);
  if (built.solids.size() != 1) return 0;
  CHECK(BRepCheck_Analyzer(built.solids[0].shape->occt()).IsValid());
  return volume(*built.solids[0].shape);
}

// A cone of half angle asin 0.6 about +z, its apex at the origin: the
// sector 50..100 of the development, half a turn (0.6 pi developed), 2
// thick. With the neutral surface in the middle (K = 0.5) the volume is
// the developed area times the thickness; with K = 0.3 the material
// reaches 0.4 further out on average: V = t (A + 0.4 cot(a) phi (rho2 -
// rho1)), as the surface at offset d grows by d cot(a) / rho round the
// axis.
void conical_panel_volumes() {
  const double phi = 0.6 * test::kPi, t = 2;
  for (const double k : {0.5, 0.3}) {
    for (const bool inner_front : {false, true}) {
      for (const double turn : {1.0, -1.0}) {
        SheetChartShapeSpec spec;
        spec.thickness = t;
        ConeEmbedding cone;
        cone.apex = {0, 0, 0};
        cone.axis = {0, 0, 1};
        cone.radial_start = {1, 0, 0};
        cone.angular_direction = {0, turn, 0};
        cone.half_angle = std::asin(0.6);
        cone.neutral_depth = k * t;
        cone.angle_extent = phi;
        cone.inner_is_front = inner_front;
        spec.patches.push_back({kPanel, {sector(50, 100, phi)}, cone});
        const double area = phi / 2 * (100 * 100 - 50 * 50);
        const double middle = t / 2 - k * t;
        CHECK_NEAR(built_volume(spec), t * (area + middle * (0.8 / 0.6) * phi * 50));
      }
    }
  }
}

// A cylindrical panel (not a bend): inside radius 18, 2 thick, K = 0.5:
// the chart half a turn of the neutral radius 19 by 50 holds 19 pi 50 2.
void cylindrical_panel_volume() {
  SheetChartShapeSpec spec;
  spec.thickness = 2;
  const CylinderEmbedding cylinder{{0, 0, 0}, {0, 0, 1}, {1, 0, 0}, {0, 1, 0}, 18, 19, test::kPi, false};
  spec.patches.push_back({kPanel, {rectangle(19 * test::kPi, 50)}, cylinder});
  CHECK_NEAR(built_volume(spec), 19 * test::kPi * 50 * 2);
}

// A cone's face developed: base radius 30, top 60, 40 high (half angle
// asin 0.6, the apex 40 below the base): the base circle 50 from the apex
// and the top 100, a full turn 1.2 pi developed, the seam a line on each
// side. Moved 1 towards the axis, the apex moves 1 / 0.6 up and the
// circles lie 4/3 nearer to it (1 / tan).
void cone_face_development() {
  PrimitiveSpec spec;
  spec.feature = "F2";
  spec.kind = PrimitiveKind::Cone;
  spec.a = 30;
  spec.b = 60;
  spec.c = 40;
  const ShapePtr cone = primitive(spec);
  for (const double offset : {0.0, -1.0}) {
    // The face is the outer surface of a sheet 2 thick: its neutral
    // surface 2 + offset from the inner one.
    const SheetFaceDevelopment developed = sheet_face_development(*cone, {"F2:side0"}, 2 + offset, 2);
    CHECK(developed.cone);
    CHECK(developed.closed);
    CHECK(!developed.inner_is_front);
    CHECK(developed.faces.size() == 1);
    CHECK_NEAR(developed.half_angle, std::asin(0.6));
    CHECK_NEAR(developed.turn, 2 * test::kPi);
    CHECK(std::abs(developed.origin.Z() - (-40 - offset / 0.6)) < 1e-9);
    CHECK(developed.axis.Z() > 1 - 1e-12);
    const double near = 50 + offset * 4.0 / 3.0, far = 100 + offset * 4.0 / 3.0;
    int arcs = 0, lines = 0;
    for (const auto& edge : developed.faces.front()) {
      const Segment& s = edge.geometry;
      if (s.kind == SegmentKind::Arc) {
        ++arcs;
        CHECK(std::abs(s.radius - near) < 1e-9 || std::abs(s.radius - far) < 1e-9);
        CHECK_NEAR(std::abs(s.end_angle - s.start_angle), 1.2 * test::kPi);
      } else if (s.kind == SegmentKind::Line) {
        ++lines;
        CHECK_NEAR(s.start.Distance(s.end), far - near);
      }
    }
    CHECK(arcs == 2);
    CHECK(lines == 2);
  }
}

// A cylinder's face, radius 20 and 50 high, at the neutral radius 19: a
// rectangle 38 pi by 50 of four lines.
void cylinder_face_development() {
  PrimitiveSpec spec;
  spec.feature = "F3";
  spec.kind = PrimitiveKind::Cylinder;
  spec.a = 20;
  spec.c = 50;
  const ShapePtr cylinder = primitive(spec);
  const SheetFaceDevelopment developed = sheet_face_development(*cylinder, {"F3:side0"}, 1, 2);
  CHECK(!developed.cone);
  CHECK(developed.closed);
  CHECK_NEAR(developed.neutral_radius, 19);
  CHECK_NEAR(developed.turn, 2 * test::kPi);
  double around = 0, along = 0;
  for (const auto& edge : developed.faces.front()) {
    CHECK(edge.geometry.kind == SegmentKind::Line);
    const gp_Vec2d d(edge.geometry.start, edge.geometry.end);
    around = std::max(around, std::abs(d.X()));
    along = std::max(along, std::abs(d.Y()));
  }
  CHECK(developed.faces.front().size() == 4);
  CHECK_NEAR(around, 38 * test::kPi);
  CHECK_NEAR(along, 50);
  // A sphere's face is neither.
  PrimitiveSpec ball;
  ball.feature = "F4";
  ball.kind = PrimitiveKind::Sphere;
  ball.a = 10;
  bool refused = false;
  try {
    (void)sheet_face_development(*primitive(ball), {"F4:side0"}, 1, 2);
  } catch (const std::exception&) {
    refused = true;
  }
  CHECK(refused);
}

// A sphere of radius 10 bends 1/100 both ways; a cylinder's and a cone's
// faces lie flat.
void face_bendings() {
  PrimitiveSpec ball;
  ball.feature = "F5";
  ball.kind = PrimitiveKind::Sphere;
  ball.a = 10;
  const FaceBending sphere = face_bending(*primitive(ball), "F5:side0");
  CHECK_NEAR(sphere.gaussian, 0.01);
  CHECK(sphere.size > 30);
  PrimitiveSpec rod;
  rod.feature = "F6";
  rod.kind = PrimitiveKind::Cylinder;
  rod.a = 20;
  rod.c = 50;
  CHECK(face_bending(*primitive(rod), "F6:side0").gaussian < 1e-12);
  PrimitiveSpec cone;
  cone.feature = "F7";
  cone.kind = PrimitiveKind::Cone;
  cone.a = 30;
  cone.b = 60;
  cone.c = 40;
  CHECK(face_bending(*primitive(cone), "F7:side0").gaussian < 1e-12);
}
} // namespace

MITCAD_BUNDLED_TEST(sheet_curved) {
  enable_input_checks(true);
  test::guarded("conical panel volumes", conical_panel_volumes);
  test::guarded("cylindrical panel volume", cylindrical_panel_volume);
  test::guarded("cone face development", cone_face_development);
  test::guarded("cylinder face development", cylinder_face_development);
  test::guarded("face bendings", face_bendings);
  CHECK(input_changes().empty());
  if (test::failures == 0) std::puts("test_sheet_curved: all checks passed");
  return test::failures == 0 ? 0 : 1;
}
