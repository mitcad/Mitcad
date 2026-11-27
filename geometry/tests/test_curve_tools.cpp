// SPDX-License-Identifier: MIT
// The curve tools (mitcad#1777): a growing helix lies on its cone from its
// start to its end; a line from the Z axis moved by a screw motion is the
// helicoid of its area; that helicoid crossed with a cylinder about the
// axis is one helix on the cylinder; a face name that finds nothing is
// refused. A surface helix with a growth (mitcad#1797) has its outer edge
// on its cone; one with flat ends has planar ends.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <memory>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <GProp_GProps.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"
#include "mitcad/geometry/intersection_curve.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surface_sweeps.hpp"
#include "bundled_test.hpp"

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

// A line on X from `from` to `to` moved about +Z by `pitch` per turn for
// `revolutions` turns with `options` (mitcad#1797).
ShapePtr line_sheet(double from, double to, double pitch, double revolutions,
                    const mitcad::geometry::HelixSheetOptions& options) {
  PathCurve line;
  line.name = "c1";
  line.curve.kind = ModelCurveKind::Line;
  line.curve.start = gp_Pnt(from, 0.0, 0.0);
  line.curve.end = gp_Pnt(to, 0.0, 0.0);
  const std::vector<Path> chains{Path{line}};
  return mitcad::geometry::helix_sheet("F2", chains, gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), pitch,
                                       revolutions, false, options);
}

// Whether a face of the sheet is named "F2:side(c1)", whole or a piece.
bool named_side(const ShapePtr& sheet) {
  bool named = false;
  for (int f = 0; f < sheet->face_count(); ++f) {
    for (const std::string& name : sheet->names_of_face(sheet->face(f))) {
      named = named || name.rfind("F2:side(c1)", 0) == 0;
    }
  }
  return named;
}

// The line from the Z axis out to 15 on X, 4 turns of pitch 5 moving out
// 1 mm a turn (a conical helicoid): its outer end goes from (15, 0, 0) to
// (19, 0, 20), every point of that edge at height z is 15 + z / 5 from
// the axis (on the cone).
void test_growing_helicoid_has_its_edge_on_the_cone() {
  mitcad::geometry::HelixSheetOptions options;
  options.growth = 1.0;
  const ShapePtr sheet = line_sheet(0.0, 15.0, 5.0, 4.0, options);
  CHECK(BRepCheck_Analyzer(sheet->occt()).IsValid());
  CHECK(named_side(sheet));
  const gp_Pnt start(15.0, 0.0, 0.0);
  const gp_Pnt end(19.0, 0.0, 20.0);
  int found = 0;
  for (TopExp_Explorer e(sheet->occt(), TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    if (first.IsNull() || last.IsNull()) {
      continue;
    }
    const gp_Pnt a = BRep_Tool::Pnt(first);
    const gp_Pnt b = BRep_Tool::Pnt(last);
    const bool outer = (a.Distance(start) <= 1e-6 && b.Distance(end) <= 1e-6) ||
                       (a.Distance(end) <= 1e-6 && b.Distance(start) <= 1e-6);
    if (!outer) {
      continue;
    }
    ++found;
    const BRepAdaptor_Curve curve(edge);
    const double t0 = curve.FirstParameter();
    const double t1 = curve.LastParameter();
    for (int i = 0; i <= 256; ++i) {
      const gp_Pnt p = curve.Value(t0 + (t1 - t0) * i / 256.0);
      CHECK(p.Z() >= -1e-6 && p.Z() <= 20.0 + 1e-6);
      CHECK(std::abs(std::hypot(p.X(), p.Y()) - (15.0 + p.Z() / 5.0)) <= 1e-6);
    }
  }
  // The edge may be seen once per face that has it.
  CHECK(found >= 1);
}

// The distance from the point to the shape.
double distance_to(const gp_Pnt& point, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
  return distance.IsDone() ? distance.Value() : 1e9;
}

// The line from 5 to 15 on X, 2 turns of pitch 5 with flat ends: a
// quarter turn flat and a quarter turn of transition at each end (3 turns
// in all). The pitch changes linearly over each transition, which rises
// 5 / 2 pi * (pi / 2) / 2 = 0.625; so the helix rises 2 * 5 + 2 * 0.625 =
// 11.25. Its ends are planar: the quarter of the annulus 5 to 15 from the
// axis between 0 and 90 degrees lies in the sheet at z = 0, and the
// quarter between -90 and 0 degrees at z = 11.25.
void test_helicoid_with_flat_ends_has_planar_ends() {
  mitcad::geometry::HelixSheetOptions options;
  options.start_flat = kPi / 2.0;
  options.start_transition = kPi / 2.0;
  options.end_transition = kPi / 2.0;
  options.end_flat = kPi / 2.0;
  const ShapePtr sheet = line_sheet(5.0, 15.0, 5.0, 2.0, options);
  CHECK(BRepCheck_Analyzer(sheet->occt()).IsValid());
  CHECK(named_side(sheet));
  for (const double radius : {6.0, 8.5, 11.0, 14.0}) {
    for (int k = 1; k <= 5; ++k) {
      const double angle = kPi / 2.0 * k / 6.0;
      const gp_Pnt low(radius * std::cos(angle), radius * std::sin(angle), 0.0);
      const gp_Pnt high(radius * std::cos(angle), -radius * std::sin(angle), 11.25);
      CHECK(distance_to(low, sheet->occt()) <= 1e-6);
      CHECK(distance_to(high, sheet->occt()) <= 1e-6);
    }
  }
  // No corner of it lies below the start's plane or above the end's.
  for (TopExp_Explorer v(sheet->occt(), TopAbs_VERTEX); v.More(); v.Next()) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
    CHECK(p.Z() >= -1e-6 && p.Z() <= 11.25 + 1e-6);
  }
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

MITCAD_BUNDLED_TEST(curve_tools) {
  test::guarded("growing helix lies on its cone", test_growing_helix_lies_on_its_cone);
  test::guarded("helix of a line is a helicoid", test_helix_of_a_line_is_a_helicoid);
  test::guarded("helicoid and cylinder meet in a helix", test_helicoid_and_cylinder_meet_in_a_helix);
  test::guarded("missing face refused", test_missing_face_refused);
  test::guarded("growing helicoid has its edge on the cone", test_growing_helicoid_has_its_edge_on_the_cone);
  test::guarded("helicoid with flat ends has planar ends", test_helicoid_with_flat_ends_has_planar_ends);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("curve tools: all checks passed\n");
  return 0;
}
