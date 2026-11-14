// SPDX-License-Identifier: MIT
// The curve tools (mitcad#1777): a growing helix lies on its cone from its
// start to its end; a line from the Z axis moved by a screw motion is the
// helicoid of its area; that helicoid crossed with a cylinder about the
// axis is one helix on the cylinder; a face name that finds nothing is
// refused.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <memory>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/intersection_curve.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surface_sweeps.hpp"

namespace {

using mitcad::geometry::HelixCurve;
using mitcad::geometry::ModelCurve;
using mitcad::geometry::ModelCurveKind;
using mitcad::geometry::Path;
using mitcad::geometry::PathCurve;
using mitcad::geometry::SectionPart;
using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;
using test::kPi;

// The point of a curve at `fraction` (0 to 1) of its parameter range:
// lines, conics and B-splines.
gp_Pnt point_at(const ModelCurve& c, double fraction) {
  switch (c.kind) {
  case ModelCurveKind::Line:
    return gp_Pnt(c.start.XYZ() + (c.end.XYZ() - c.start.XYZ()) * fraction);
  case ModelCurveKind::Conic: {
    const double t = c.first + (c.last - c.first) * fraction;
    const gp_Vec x(c.x_axis);
    const gp_Vec y = gp_Vec(c.normal).Crossed(x);
    return c.center.Translated(x * (c.major * std::cos(t)) + y * (c.minor * std::sin(t)));
  }
  case ModelCurveKind::BSpline: {
    std::vector<double> distinct;
    std::vector<int> mults;
    for (const double k : c.knots) {
      if (!distinct.empty() && std::abs(k - distinct.back()) <= 1e-12) {
        ++mults.back();
      } else {
        distinct.push_back(k);
        mults.push_back(1);
      }
    }
    NCollection_Array1<gp_Pnt> poles(1, static_cast<int>(c.poles.size()));
    for (std::size_t i = 0; i < c.poles.size(); ++i) {
      poles.SetValue(static_cast<int>(i) + 1, c.poles[i]);
    }
    NCollection_Array1<double> knots(1, static_cast<int>(distinct.size()));
    NCollection_Array1<int> multiplicities(1, static_cast<int>(distinct.size()));
    for (std::size_t i = 0; i < distinct.size(); ++i) {
      knots.SetValue(static_cast<int>(i) + 1, distinct[i]);
      multiplicities.SetValue(static_cast<int>(i) + 1, mults[i]);
    }
    occ::handle<Geom_BSplineCurve> spline;
    if (c.weights.empty()) {
      spline = new Geom_BSplineCurve(poles, knots, multiplicities, c.degree);
    } else {
      NCollection_Array1<double> weights(1, static_cast<int>(c.weights.size()));
      for (std::size_t i = 0; i < c.weights.size(); ++i) {
        weights.SetValue(static_cast<int>(i) + 1, c.weights[i]);
      }
      spline = new Geom_BSplineCurve(poles, weights, knots, multiplicities, c.degree);
    }
    const double t0 = spline->FirstParameter();
    const double t1 = spline->LastParameter();
    return spline->Value(t0 + (t1 - t0) * fraction);
  }
  default:
    return c.start;
  }
}

// An angle brought within (-pi, pi].
double wrapped(double angle) { return std::remainder(angle, 2.0 * kPi); }

// A helix of radius 10 about +Z from (10, 0, 0), 4 turns of pitch 5,
// moving out 1 mm a turn: every point at height z is 10 + z / 5 from the
// axis (on the cone), at the angle 2 pi z / 5, and it ends at (14, 0, 20).
void test_growing_helix_lies_on_its_cone() {
  HelixCurve helix;
  helix.origin = gp_Pnt(0.0, 0.0, 0.0);
  helix.direction = gp_Dir(0.0, 0.0, 1.0);
  helix.x_axis = gp_Dir(1.0, 0.0, 0.0);
  helix.radius = 10.0;
  helix.pitch = 5.0;
  helix.revolutions = 4.0;
  helix.growth = 1.0;
  const std::vector<ModelCurve> curves = mitcad::geometry::helix_curve(helix);
  CHECK(curves.size() == 1);
  if (curves.empty()) {
    return;
  }
  const ModelCurve& curve = curves.front();
  CHECK(point_at(curve, 0.0).Distance(gp_Pnt(10.0, 0.0, 0.0)) <= 1e-6);
  CHECK(point_at(curve, 1.0).Distance(gp_Pnt(14.0, 0.0, 20.0)) <= 1e-6);
  for (int i = 0; i <= 256; ++i) {
    const gp_Pnt p = point_at(curve, i / 256.0);
    CHECK(p.Z() >= -1e-6 && p.Z() <= 20.0 + 1e-6);
    CHECK_NEAR_TOL(std::hypot(p.X(), p.Y()), 10.0 + p.Z() / 5.0, 1e-6);
    CHECK(std::abs(wrapped(std::atan2(p.Y(), p.X()) - 2.0 * kPi * p.Z() / 5.0)) <= 1e-6);
  }
  // Narrowing to the axis before the last turn is refused.
  helix.growth = -3.0;
  bool thrown = false;
  try {
    mitcad::geometry::helix_curve(helix);
  } catch (const std::exception&) {
    thrown = true;
  }
  CHECK(thrown);
}

// The line from the Z axis out to 15 on X, moved 4 turns of pitch 5 about
// +Z: the helicoid, of area 8 pi [F(15) - F(0)] with F(r) = r/2 sqrt(r^2 +
// c^2) + c^2/2 ln(r + sqrt(r^2 + c^2)), c = 5 / 2 pi.
ShapePtr helicoid() {
  PathCurve line;
  line.name = "c1";
  line.curve.kind = ModelCurveKind::Line;
  line.curve.start = gp_Pnt(0.0, 0.0, 0.0);
  line.curve.end = gp_Pnt(15.0, 0.0, 0.0);
  const std::vector<Path> chains{Path{line}};
  return mitcad::geometry::helix_sheet("F2", chains, gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), 5.0,
                                       4.0, false);
}

void test_helix_of_a_line_is_a_helicoid() {
  const ShapePtr sheet = helicoid();
  CHECK(BRepCheck_Analyzer(sheet->occt()).IsValid());
  GProp_GProps props;
  BRepGProp::SurfaceProperties(sheet->occt(), props);
  CHECK_NEAR_TOL(props.Mass(), 2860.298789266602, 1e-3);
  bool named = false;
  for (int f = 0; f < sheet->face_count(); ++f) {
    for (const std::string& name : sheet->names_of_face(sheet->face(f))) {
      named = named || name.rfind("F2:side(", 0) == 0;
    }
  }
  CHECK(named);
}

// The helicoid crossed with a solid cylinder of radius 10 about Z from
// z = -5 to 25: the helix of radius 10 from (10, 0, 0) up to (10, 0, 20),
// every point at the angle 2 pi z / 5.
void test_helicoid_and_cylinder_meet_in_a_helix() {
  const ShapePtr sheet = helicoid();
  BRepPrimAPI_MakeCylinder cylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -5.0), gp_Dir(0.0, 0.0, 1.0)), 10.0, 30.0);
  const ShapePtr body = std::make_shared<Shape>(cylinder.Shape());
  const std::vector<ModelCurve> pieces =
      mitcad::geometry::shape_section({SectionPart{sheet, {}}}, {SectionPart{body, {}}});
  CHECK(!pieces.empty());
  double low = 1e9;
  double high = -1e9;
  for (const ModelCurve& c : pieces) {
    for (int i = 0; i <= 32; ++i) {
      const gp_Pnt p = point_at(c, i / 32.0);
      low = std::min(low, p.Z());
      high = std::max(high, p.Z());
      CHECK(std::abs(std::hypot(p.X(), p.Y()) - 10.0) <= 1e-4);
      CHECK(std::abs(wrapped(std::atan2(p.Y(), p.X()) - 2.0 * kPi * p.Z() / 5.0)) <= 1e-4);
    }
  }
  CHECK(std::abs(low) <= 1e-4);
  CHECK(std::abs(high - 20.0) <= 1e-4);
}

// A face name that finds no face of its body is an error.
void test_missing_face_refused() {
  const ShapePtr sheet = helicoid();
  BRepPrimAPI_MakeCylinder cylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -5.0), gp_Dir(0.0, 0.0, 1.0)), 10.0, 30.0);
  const ShapePtr body = std::make_shared<Shape>(cylinder.Shape());
  bool thrown = false;
  try {
    mitcad::geometry::shape_section({SectionPart{sheet, {"F9:side(c1)"}}}, {SectionPart{body, {}}});
  } catch (const std::exception&) {
    thrown = true;
  }
  CHECK(thrown);
}

} // namespace

int main() {
  test::guarded("growing helix lies on its cone", test_growing_helix_lies_on_its_cone);
  test::guarded("helix of a line is a helicoid", test_helix_of_a_line_is_a_helicoid);
  test::guarded("helicoid and cylinder meet in a helix", test_helicoid_and_cylinder_meet_in_a_helix);
  test::guarded("missing face refused", test_missing_face_refused);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("curve tools: all checks passed\n");
  return 0;
}
