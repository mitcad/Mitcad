// SPDX-License-Identifier: MIT
// A sketch on a cylindrical or conical face (mitcad#1768,
// sketch/SketchSurface.hpp): the face read from the `sketch` query's
// `surface` (none on a plane), the map from the sketch's developed
// coordinates onto the face as the model's Development::point makes it
// (around a cylinder at its radius, along a cone's generator), the face's
// outward normal (into the bore for a bore's wall), the polylines put
// between their points, a flat profile laid onto the face, and the angle
// Create Sketch sends for where a face was clicked.
#include "UnitTests.hpp"
#include "sketch/SketchSurface.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <optional>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "framework/Selection.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_surface.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

constexpr double kPi = 3.14159265358979323846;

bool isNear(const gp_Pnt& a, const gp_Pnt& b, double tolerance = 1e-9) { return a.Distance(b) <= tolerance; }

bool isNear(double a, double b, double tolerance = 1e-9) { return std::abs(a - b) <= tolerance; }

QJsonArray xyz(double x, double y, double z) { return QJsonArray{x, y, z}; }

// A cylinder of radius 10 about Z, the sketch's origin at (10, 0, 0) on it:
// its x axis along +Y there (to the right seen from outside), its y axis
// along +Z, its normal +X out of the material.
QJsonObject cylinder(bool internal) {
  const double side = internal ? -1.0 : 1.0;
  return QJsonObject{
      {QStringLiteral("kind"), QStringLiteral("cylinder")},
      {QStringLiteral("axis"),
       QJsonObject{{QStringLiteral("origin"), xyz(0, 0, 0)}, {QStringLiteral("direction"), xyz(0, 0, 1)}}},
      {QStringLiteral("radius"), 10.0},
      {QStringLiteral("internal"), internal},
      {QStringLiteral("frame"), QJsonObject{{QStringLiteral("origin"), xyz(10, 0, 0)},
                                            {QStringLiteral("x_axis"), xyz(0, side, 0)},
                                            {QStringLiteral("y_axis"), xyz(0, 0, 1)}}}};
}

// A cone of half angle 45° with its apex at the origin, opening along +Z,
// the sketch's origin on it at height 10 (radius 10, 14.14 mm along the
// generator from the apex).
QJsonObject cone() {
  return QJsonObject{
      {QStringLiteral("kind"), QStringLiteral("cone")},
      {QStringLiteral("axis"),
       QJsonObject{{QStringLiteral("origin"), xyz(0, 0, 0)}, {QStringLiteral("direction"), xyz(0, 0, 1)}}},
      {QStringLiteral("radius"), 10.0},
      {QStringLiteral("half_angle"), kPi / 4.0},
      {QStringLiteral("apex"), xyz(0, 0, 0)},
      {QStringLiteral("internal"), false},
      {QStringLiteral("frame"), QJsonObject{{QStringLiteral("origin"), xyz(10, 0, 10)},
                                            {QStringLiteral("x_axis"), xyz(0, 1, 0)},
                                            {QStringLiteral("y_axis"), xyz(0, 0, 1)}}}};
}

void readTests() {
  CHECK(!SketchSurface::fromJson(QJsonValue()).has_value());
  CHECK(!SketchSurface::fromJson(QJsonObject{{QStringLiteral("kind"), QStringLiteral("plane")}}).has_value());
  QJsonObject broken = cylinder(false);
  broken.remove(QStringLiteral("frame"));
  CHECK(!SketchSurface::fromJson(broken).has_value());
  const std::optional<SketchSurface> surface = SketchSurface::fromJson(cylinder(false));
  CHECK(surface.has_value());
  if (surface) {
    CHECK(surface->kind() == SketchSurface::Kind::Cylinder);
    CHECK(isNear(surface->radius(), 10.0));
    CHECK(!surface->internal());
    CHECK(surface->describe() == QStringLiteral("cylinder, radius 10.000 mm"));
    CHECK(isNear(surface->step(), 0.5));
  }
  const std::optional<SketchSurface> conical = SketchSurface::fromJson(cone());
  CHECK(conical.has_value());
  if (conical) {
    CHECK(conical->describe() == QStringLiteral("cone, radius 10.000 mm, half angle 45.000 deg"));
  }
}

void cylinderTests() {
  const std::optional<SketchSurface> surface = SketchSurface::fromJson(cylinder(false));
  if (!surface) {
    CHECK(false);
    return;
  }
  // The origin on the face; a quarter turn around it is 5π mm along x; y
  // runs along the axis.
  CHECK(isNear(surface->point(V2{}), gp_Pnt(10, 0, 0)));
  CHECK(isNear(surface->point(V2{5.0 * kPi, 3.0}), gp_Pnt(0, 10, 3)));
  CHECK(isNear(surface->point(V2{-5.0 * kPi, 0.0}), gp_Pnt(0, -10, 0)));
  // Once around is the circumference: back where it started.
  CHECK(isNear(surface->point(V2{20.0 * kPi, 7.0}), gp_Pnt(10, 0, 7), 1e-8));
  // Every point at the radius.
  for (const V2& p : {V2{1.0, 2.0}, V2{13.0, -4.0}, V2{-40.0, 9.0}}) {
    const gp_Pnt at = surface->point(p);
    CHECK(isNear(std::hypot(at.X(), at.Y()), 10.0));
    CHECK(isNear(at.Z(), p.y));
  }
  CHECK(surface->outward(V2{}).IsEqual(gp_Dir(1, 0, 0), 1e-9));
  CHECK(surface->outward(V2{5.0 * kPi, 0.0}).IsEqual(gp_Dir(0, 1, 0), 1e-9));

  // A bore's wall: the material outside, the normal into the bore.
  const std::optional<SketchSurface> bore = SketchSurface::fromJson(cylinder(true));
  CHECK(bore.has_value());
  if (bore) {
    CHECK(bore->internal());
    CHECK(isNear(bore->point(V2{}), gp_Pnt(10, 0, 0)));
    CHECK(bore->outward(V2{}).IsEqual(gp_Dir(-1, 0, 0), 1e-9));
    // Seen from the axis its x axis runs the other way round.
    CHECK(isNear(bore->point(V2{5.0 * kPi, 0.0}), gp_Pnt(0, -10, 0)));
  }

  // Moved with its component's occurrence.
  SketchSurface moved = *surface;
  gp_Trsf shift;
  shift.SetTranslation(gp_Vec(100, 0, 0));
  moved.transform(shift);
  CHECK(isNear(moved.point(V2{5.0 * kPi, 3.0}), gp_Pnt(100, 10, 3)));
  CHECK(isNear(moved.frame().origin, gp_Pnt(110, 0, 0)));
}

void coneTests() {
  const std::optional<SketchSurface> surface = SketchSurface::fromJson(cone());
  if (!surface) {
    CHECK(false);
    return;
  }
  CHECK(isNear(surface->point(V2{}), gp_Pnt(10, 0, 10)));
  // Along the generator under the origin: √2 mm is 1 mm out and 1 mm up.
  CHECK(isNear(surface->point(V2{0.0, std::sqrt(2.0)}), gp_Pnt(11, 0, 11)));
  // An arc about the apex in the development is a circle around the cone:
  // every point of it at the origin's height.
  const double rho = 10.0 * std::sqrt(2.0);
  for (const double turn : {0.2, 0.5, -0.4}) {
    const V2 p{rho * std::sin(turn), rho * std::cos(turn) - rho};
    const gp_Pnt at = surface->point(p);
    CHECK(isNear(at.Z(), 10.0, 1e-8));
    CHECK(isNear(std::hypot(at.X(), at.Y()), 10.0, 1e-8));
  }
  // Out of the material and tilted towards the apex.
  CHECK(surface->outward(V2{}).IsEqual(gp_Dir(1, 0, -1), 1e-9));
}

void densifyTests() {
  const std::vector<V2> line = densified({V2{0.0, 0.0}, V2{1.0, 0.0}}, 0.25);
  CHECK(line.size() == 5);
  CHECK(isNear(line[2].x, 0.5));
  const std::vector<V2> square =
      densified({V2{0.0, 0.0}, V2{1.0, 0.0}, V2{1.0, 1.0}, V2{0.0, 1.0}}, 0.5, true);
  CHECK(square.size() == 9);
  CHECK(square.back() == V2{});
  CHECK(densified({}, 1.0).empty());
}

void wrapTests() {
  const std::optional<SketchSurface> surface = SketchSurface::fromJson(cylinder(false));
  if (!surface) {
    CHECK(false);
    return;
  }
  // A 20 x 10 profile in the carrier plane x = 10, from the origin.
  BRepBuilderAPI_MakePolygon outline(gp_Pnt(10, 0, 0), gp_Pnt(10, 20, 0), gp_Pnt(10, 20, 10), gp_Pnt(10, 0, 10),
                                     true);
  const TopoDS_Face flat = BRepBuilderAPI_MakeFace(outline.Wire(), true).Face();
  const TopoDS_Shape wrapped = wrappedFace(flat, *surface);
  CHECK(!wrapped.IsNull());
  int nodes = 0;
  bool onFace = true;
  double farthest = 0.0;
  for (TopExp_Explorer faces(wrapped, TopAbs_FACE); faces.More(); faces.Next()) {
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> mesh = BRep_Tool::Triangulation(TopoDS::Face(faces.Current()), location);
    if (mesh.IsNull()) {
      onFace = false;
      continue;
    }
    for (int i = 1; i <= mesh->NbNodes(); ++i) {
      const gp_Pnt at = mesh->Node(i);
      onFace = onFace && isNear(std::hypot(at.X(), at.Y()), 10.0, 1e-7);
      farthest = std::max(farthest, std::atan2(at.Y(), at.X()));
      ++nodes;
    }
  }
  CHECK(nodes > 4);
  CHECK(onFace);
  // 20 mm around a radius of 10: two radians.
  CHECK(isNear(farthest, 2.0, 1e-7));
  CHECK(wrappedFace(TopoDS_Shape(), *surface).IsNull());
}

void angleTests() {
  // About +Z (or -Z, counted towards +Z) from the X axis.
  CHECK(isNear(developedAngle(gp_Dir(0, 0, 1), gp_Pnt(0, 0, 0), gp_Pnt(0, 10, 3)), kPi / 2.0));
  CHECK(isNear(developedAngle(gp_Dir(0, 0, -1), gp_Pnt(0, 0, 0), gp_Pnt(0, 10, 3)), kPi / 2.0));
  CHECK(isNear(developedAngle(gp_Dir(0, 0, 1), gp_Pnt(5, 5, 0), gp_Pnt(15, 5, 0)), 0.0));
  CHECK(isNear(developedAngle(gp_Dir(0, 0, 1), gp_Pnt(0, 0, 0), gp_Pnt(0, -10, 0)), -kPi / 2.0));
  // About X, from the Y axis.
  CHECK(isNear(developedAngle(gp_Dir(1, 0, 0), gp_Pnt(0, 0, 0), gp_Pnt(5, 0, 10)), kPi / 2.0));
  // On the axis: no angle to take.
  CHECK(isNear(developedAngle(gp_Dir(0, 0, 1), gp_Pnt(0, 0, 0), gp_Pnt(0, 0, 7)), 0.0));

  mitcad::SelectionItem side{mitcad::SelectKind::Face, QStringLiteral("F1.b0"), QStringLiteral("F1:side(c1)"),
                             QStringLiteral("cylinder")};
  CHECK(isDevelopedFace(side));
  mitcad::SelectionItem top = side;
  top.geometry = QStringLiteral("plane");
  CHECK(!isDevelopedFace(top));
  mitcad::SelectionItem ball = side;
  ball.geometry = QStringLiteral("sphere");
  CHECK(!isDevelopedFace(ball));

  // Without a click's place: the angle 0, no query.
  bool asked = false;
  const auto none = [&asked](const QJsonObject&) {
    asked = true;
    return QJsonValue();
  };
  QJsonObject plane = developedPlane(side, none);
  CHECK(!asked);
  CHECK(plane.value(QStringLiteral("face")).toString() == QStringLiteral("F1:side(c1)"));
  CHECK(plane.value(QStringLiteral("body")).toString() == QStringLiteral("F1.b0"));
  CHECK(isNear(plane.value(QStringLiteral("developed")).toObject().value(QStringLiteral("angle")).toDouble(), 0.0));

  // Clicked at +Y on a cylinder about Z: a quarter turn.
  side.at = std::array<double, 3>{0.0, 10.0, 4.0};
  QJsonObject asking;
  const auto faces = [&asking](const QJsonObject& request) {
    asking = request;
    const QJsonObject other{{QStringLiteral("names"), QJsonArray{QStringLiteral("F1:side(c2)")}},
                            {QStringLiteral("origin"), xyz(50, 0, 0)},
                            {QStringLiteral("axis"), xyz(1, 0, 0)}};
    const QJsonObject face{{QStringLiteral("names"), QJsonArray{QStringLiteral("F1:side(c1)")}},
                           {QStringLiteral("origin"), xyz(0, 0, 2)},
                           {QStringLiteral("axis"), xyz(0, 0, 1)}};
    return QJsonValue(QJsonObject{{QStringLiteral("total"), 2}, {QStringLiteral("faces"), QJsonArray{other, face}}});
  };
  plane = developedPlane(side, faces);
  CHECK(asking.value(QStringLiteral("query")).toString() == QStringLiteral("faces"));
  CHECK(asking.value(QStringLiteral("surface")).toString() == QStringLiteral("cylinder"));
  CHECK(isNear(plane.value(QStringLiteral("developed")).toObject().value(QStringLiteral("angle")).toDouble(),
             kPi / 2.0));
}

} // namespace

int sketchSurfaceTests() {
  failures = 0;
  readTests();
  cylinderTests();
  coneTests();
  densifyTests();
  wrapTests();
  angleTests();
  return failures;
}
MITCAD_UNIT_TEST(sketchSurfaceTests);
