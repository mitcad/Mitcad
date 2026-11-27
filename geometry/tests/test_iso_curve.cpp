// SPDX-License-Identifier: MIT
// Isoparametric curves (mitcad#2522): a cylinder's u curve is a line along
// its axis and its v curve a circle of its radius, at the place given or
// through a picked point, also on the seam; a B-spline face's curves lie on
// the face within 1e-6 mm and end on its boundary; a hole cuts a curve into
// two pieces; places outside the face's range and a face name that finds
// nothing are refused.

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <exception>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "bundled_test.hpp"
#include "check.hpp"
#include "mitcad/geometry/iso_curve.hpp"
#include "mitcad/geometry/shape.hpp"

namespace {

using mitcad::geometry::IsoCurve;
using mitcad::geometry::IsoParameter;
using mitcad::geometry::ModelCurve;
using mitcad::geometry::ModelCurveKind;
using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;

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

// The distance from the point to the shape.
double distance_to(const gp_Pnt& point, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
  return distance.IsDone() ? distance.Value() : 1e9;
}

// The first face of `shape` whose surface is of `type`.
TopoDS_Face face_of_type(const TopoDS_Shape& shape, GeomAbs_SurfaceType type) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    if (BRepAdaptor_Surface(face).GetType() == type) {
      return face;
    }
  }
  return TopoDS_Face();
}

// A solid cylinder of radius 10 about +Z from z = 0 to 20, its side face
// named F1:side0.
ShapePtr cylinder() {
  BRepPrimAPI_MakeCylinder make(gp_Ax2(gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 0.0, 1.0)), 10.0, 20.0);
  const TopoDS_Shape solid = make.Shape();
  const TopoDS_Face side = face_of_type(solid, GeomAbs_Cylinder);
  CHECK(!side.IsNull());
  return std::make_shared<Shape>(solid, std::vector<Shape::NamedFace>{{side, {"F1:side0"}}});
}

// The only piece of the only curve.
const ModelCurve* only_piece(const std::vector<IsoCurve>& curves) {
  CHECK(curves.size() == 1);
  if (curves.size() != 1) {
    return nullptr;
  }
  CHECK(curves.front().pieces.size() == 1);
  return curves.front().pieces.size() == 1 ? &curves.front().pieces.front() : nullptr;
}

// A u curve (along the axis) of the cylinder: a line 10 from the axis from
// z = 0 to 20, parallel to it.
void check_axial_line(const ModelCurve& line) {
  CHECK(line.kind == ModelCurveKind::Line);
  CHECK_NEAR_TOL(std::hypot(line.start.X(), line.start.Y()), 10.0, 1e-9);
  CHECK(std::abs(line.start.X() - line.end.X()) <= 1e-6 && std::abs(line.start.Y() - line.end.Y()) <= 1e-6);
  CHECK(std::abs(std::abs(line.end.Z() - line.start.Z()) - 20.0) <= 1e-6);
  CHECK(std::abs(std::min(line.start.Z(), line.end.Z())) <= 1e-6);
}

void test_cylinder_u_curve_is_a_line_along_its_axis() {
  const ShapePtr body = cylinder();
  for (const double fraction : {0.0, 0.25, 0.6, 1.0}) {
    const std::vector<IsoCurve> curves =
        mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::U, {fraction}, std::nullopt);
    if (const ModelCurve* line = only_piece(curves)) {
      check_axial_line(*line);
      // A quarter of the way round from the seam: a quarter turn from where
      // the face starts.
      CHECK_NEAR_TOL(curves.front().fraction, fraction, 1e-12);
      CHECK_NEAR_TOL(curves.front().parameter, 2.0 * test::kPi * fraction, 1e-9);
    }
  }
}

void test_cylinder_v_curve_is_a_circle_of_its_radius() {
  const ShapePtr body = cylinder();
  const std::vector<IsoCurve> curves =
      mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::V, {0.5}, std::nullopt);
  if (const ModelCurve* circle = only_piece(curves)) {
    CHECK(circle->kind == ModelCurveKind::Conic);
    CHECK_NEAR_TOL(circle->major, 10.0, 1e-12);
    CHECK_NEAR_TOL(circle->minor, 10.0, 1e-12);
    CHECK(circle->closed);
    CHECK(circle->center.Distance(gp_Pnt(0.0, 0.0, 10.0)) <= 1e-6);
    CHECK(std::abs(std::abs(gp_Dir(circle->normal).Z()) - 1.0) <= 1e-12);
    for (int i = 0; i <= 16; ++i) {
      const gp_Pnt p = point_at(*circle, i / 16.0);
      CHECK(std::abs(std::hypot(p.X(), p.Y()) - 10.0) <= 1e-6 && std::abs(p.Z() - 10.0) <= 1e-6);
    }
  }
  // Several at once, in the order given: the circles at z = 5 and 15.
  const std::vector<IsoCurve> two =
      mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::V, {0.25, 0.75}, std::nullopt);
  CHECK(two.size() == 2);
  if (two.size() == 2 && two[0].pieces.size() == 1 && two[1].pieces.size() == 1) {
    CHECK(std::abs(two[0].pieces.front().center.Z() - 5.0) <= 1e-6);
    CHECK(std::abs(two[1].pieces.front().center.Z() - 15.0) <= 1e-6);
  }
}

// Through a picked point: the cylinder's line nearest to it, round the axis
// at the point's angle.
void test_curve_through_a_point() {
  const ShapePtr body = cylinder();
  const std::vector<IsoCurve> curves = mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::U, {},
                                                                    gp_Pnt(0.0, 25.0, 7.0));
  if (const ModelCurve* line = only_piece(curves)) {
    check_axial_line(*line);
    CHECK(std::abs(line->start.X()) <= 1e-6 && std::abs(line->start.Y() - 10.0) <= 1e-6);
    CHECK_NEAR_TOL(curves.front().fraction, 0.25, 1e-9);
  }
  // The v curve through it: the circle at the point's height.
  const std::vector<IsoCurve> circles = mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::V, {},
                                                                     gp_Pnt(0.0, 25.0, 7.0));
  if (const ModelCurve* circle = only_piece(circles)) {
    CHECK(circle->kind == ModelCurveKind::Conic);
    CHECK(std::abs(circle->center.Z() - 7.0) <= 1e-6);
    CHECK_NEAR_TOL(circle->major, 10.0, 1e-12);
  }
}

// A wavy B-spline surface on [0, 1] x [0, 1], its face cut to u 0.1 to 0.9
// and v 0.2 to 0.8.
occ::handle<Geom_BSplineSurface> wavy() {
  NCollection_Array2<gp_Pnt> poles(1, 4, 1, 4);
  for (int i = 1; i <= 4; ++i) {
    for (int j = 1; j <= 4; ++j) {
      poles(i, j) = gp_Pnt(i * 10.0, j * 10.0, ((i + 2 * j) % 3) * 4.0);
    }
  }
  NCollection_Array1<double> knots(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  NCollection_Array1<int> multiplicities(1, 2);
  multiplicities(1) = 4;
  multiplicities(2) = 4;
  return new Geom_BSplineSurface(poles, knots, knots, multiplicities, multiplicities, 3, 3);
}

void test_bspline_face_curves_lie_on_the_face() {
  const occ::handle<Geom_BSplineSurface> surface = wavy();
  BRepBuilderAPI_MakeFace make(surface, 0.1, 0.9, 0.2, 0.8, 1e-7);
  CHECK(make.IsDone());
  const TopoDS_Face face = make.Face();
  const Shape body(face, std::vector<Shape::NamedFace>{{face, {"F3:patch"}}});
  for (const IsoParameter held : {IsoParameter::U, IsoParameter::V}) {
    const bool hold_u = held == IsoParameter::U;
    const std::vector<IsoCurve> curves =
        mitcad::geometry::iso_curves(body, "F3:patch", held, {0.0, 0.3, 0.7, 1.0}, std::nullopt);
    CHECK(curves.size() == 4);
    for (const IsoCurve& curve : curves) {
      CHECK(curve.pieces.size() == 1);
      if (curve.pieces.size() != 1) {
        continue;
      }
      const ModelCurve& piece = curve.pieces.front();
      // The surface's own iso curve: a B-spline, cut to the face.
      CHECK(piece.kind == ModelCurveKind::BSpline);
      const double value = hold_u ? 0.1 + 0.8 * curve.fraction : 0.2 + 0.6 * curve.fraction;
      CHECK_NEAR_TOL(curve.parameter, value, 1e-12);
      for (int i = 0; i <= 32; ++i) {
        CHECK(distance_to(point_at(piece, i / 32.0), face) <= 1e-6);
      }
      const gp_Pnt start = hold_u ? surface->Value(value, 0.2) : surface->Value(0.1, value);
      const gp_Pnt end = hold_u ? surface->Value(value, 0.8) : surface->Value(0.9, value);
      CHECK(point_at(piece, 0.0).Distance(start) <= 1e-6);
      CHECK(point_at(piece, 1.0).Distance(end) <= 1e-6);
      // Halfway along it, the surface's point there.
      const gp_Pnt middle = hold_u ? surface->Value(value, 0.5) : surface->Value(0.5, value);
      double nearest = 1e9;
      for (int i = 0; i <= 512; ++i) {
        nearest = std::min(nearest, point_at(piece, i / 512.0).Distance(middle));
      }
      CHECK(nearest <= 0.05);
    }
  }
}

// The top of a 20 x 20 x 10 box with a hole of radius 3 through its middle:
// the curve through the middle is cut by the hole into two pieces, 7 mm
// each; one beside the hole is one piece across the face.
void test_hole_cuts_a_curve() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-10.0, -10.0, 0.0), 20.0, 20.0, 10.0).Shape();
  const TopoDS_Shape hole =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0.0, 0.0, -1.0), gp_Dir(0.0, 0.0, 1.0)), 3.0, 12.0).Shape();
  BRepAlgoAPI_Cut cut(box, hole);
  CHECK(cut.IsDone());
  TopoDS_Face top;
  for (TopExp_Explorer it(cut.Shape(), TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln plane = surface.Plane();
    if (std::abs(std::abs(plane.Axis().Direction().Z()) - 1.0) <= 1e-9 &&
        std::abs(plane.Location().Z() - 10.0) <= 1e-9) {
      top = face;
    }
  }
  CHECK(!top.IsNull());
  if (top.IsNull()) {
    return;
  }
  const Shape body(cut.Shape(), std::vector<Shape::NamedFace>{{top, {"F2:top"}}});
  const std::vector<IsoCurve> through =
      mitcad::geometry::iso_curves(body, "F2:top", IsoParameter::U, {0.5}, std::nullopt);
  CHECK(through.size() == 1);
  if (through.size() == 1) {
    CHECK(through.front().pieces.size() == 2);
    for (const ModelCurve& piece : through.front().pieces) {
      CHECK(piece.kind == ModelCurveKind::Line);
      CHECK_NEAR_TOL(piece.start.Distance(piece.end), 7.0, 1e-9);
      CHECK(std::abs(piece.start.Z() - 10.0) <= 1e-9 && std::abs(piece.end.Z() - 10.0) <= 1e-9);
    }
  }
  const std::vector<IsoCurve> beside =
      mitcad::geometry::iso_curves(body, "F2:top", IsoParameter::V, {0.1}, std::nullopt);
  if (const ModelCurve* line = only_piece(beside)) {
    CHECK_NEAR_TOL(line->start.Distance(line->end), 20.0, 1e-9);
  }
}

void test_refusals() {
  const ShapePtr body = cylinder();
  CHECK(test::throws_with(
      [&] { mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::U, {1.5}, std::nullopt); },
      "from 0 to 1"));
  CHECK(test::throws_with(
      [&] { mitcad::geometry::iso_curves(*body, "F1:side0", IsoParameter::U, {}, std::nullopt); }, "a place"));
  CHECK(test::throws_with(
      [&] { mitcad::geometry::iso_curves(*body, "F9:side0", IsoParameter::V, {0.5}, std::nullopt); },
      "no such face"));
}

} // namespace

MITCAD_BUNDLED_TEST(iso_curve) {
  test::guarded("cylinder u curve is a line along its axis", test_cylinder_u_curve_is_a_line_along_its_axis);
  test::guarded("cylinder v curve is a circle of its radius", test_cylinder_v_curve_is_a_circle_of_its_radius);
  test::guarded("curve through a point", test_curve_through_a_point);
  test::guarded("B-spline face curves lie on the face", test_bspline_face_curves_lie_on_the_face);
  test::guarded("hole cuts a curve", test_hole_cuts_a_curve);
  test::guarded("refusals", test_refusals);
  if (test::failures != 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("isoparametric curves: all checks passed\n");
  return 0;
}
