// SPDX-License-Identifier: MIT
// Preferences › Display › Origin planes (mitcad#1748): Size Screen and the
// Corner style by default, 100 mm; kept in the settings (view/originSize,
// view/originSizeMm, view/originStyle) and read back, unknown values the
// defaults; and the shapes each style draws an origin datum with: Corner, a
// square from the origin into the quadrant of the positive model axes and an
// axis from the origin along its positive way past the planes; Centred, as
// before.
#include "view/OriginPreference.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QTemporaryDir>

#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "origin_preference.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// Within the shapes' tolerance, which their boxes include.
bool within(double a, double b) { return std::abs(a - b) <= 1e-5; }

QJsonArray xyz(double x, double y, double z) { return QJsonArray{x, y, z}; }

// The box of a shape: {xmin, ymin, zmin, xmax, ymax, zmax}.
struct Box {
  double x0, y0, z0, x1, y1, z1;
};
Box boxOf(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  box.SetGap(0.0);
  Box b{};
  box.Get(b.x0, b.y0, b.z0, b.x1, b.y1, b.z1);
  return b;
}

} // namespace

int originPreferenceTests() {
  using mitcad::OriginDisplay;
  using mitcad::OriginSize;
  using mitcad::OriginStyle;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);

  // The defaults: Screen, 100 mm, Corner.
  const OriginDisplay defaults = OriginDisplay::load(settings);
  CHECK(defaults.size == OriginSize::Screen);
  CHECK(within(defaults.millimetres, 100.0));
  CHECK(defaults.style == OriginStyle::Corner);
  CHECK(defaults.describe() == QStringLiteral("screen size, corner style"));

  // Kept and read back under their keys.
  OriginDisplay model;
  model.size = OriginSize::Model;
  model.millimetres = 25.0;
  model.style = OriginStyle::Centred;
  model.save(settings);
  settings.sync();
  CHECK(settings.value(QStringLiteral("view/originSize")).toString() == QStringLiteral("model"));
  CHECK(within(settings.value(QStringLiteral("view/originSizeMm")).toDouble(), 25.0));
  CHECK(settings.value(QStringLiteral("view/originStyle")).toString() == QStringLiteral("centred"));
  CHECK(OriginDisplay::load(settings) == model);
  CHECK(model.describe() == QStringLiteral("model size 25 mm, centred style"));

  // Unknown values and sizes out of range are the defaults.
  settings.setValue(QStringLiteral("view/originSize"), QStringLiteral("huge"));
  settings.setValue(QStringLiteral("view/originStyle"), QStringLiteral("round"));
  settings.setValue(QStringLiteral("view/originSizeMm"), -3);
  const OriginDisplay unknown = OriginDisplay::load(settings);
  CHECK(unknown == defaults);
  settings.setValue(QStringLiteral("view/originSizeMm"), QStringLiteral("ten"));
  CHECK(within(OriginDisplay::load(settings).millimetres, 100.0));

  // The ids round trip.
  for (const OriginSize size : {OriginSize::Screen, OriginSize::Model}) {
    CHECK(mitcad::originSizeFromId(mitcad::originSizeId(size)) == size);
  }
  for (const OriginStyle style : {OriginStyle::Corner, OriginStyle::Centred}) {
    CHECK(mitcad::originStyleFromId(mitcad::originStyleId(style)) == style);
  }

  // The positive way of a direction.
  CHECK(mitcad::positiveWay(gp_Dir(0, -1, 0)).IsEqual(gp_Dir(0, 1, 0), 1e-12));
  CHECK(mitcad::positiveWay(gp_Dir(0, 0, 1)).IsEqual(gp_Dir(0, 0, 1), 1e-12));

  // XZ as the datums query gives it, its y axis along -Z or +Z alike: the
  // Corner square spans +X and +Z from the origin.
  const QJsonObject xz{{QStringLiteral("type"), QStringLiteral("plane")},
                       {QStringLiteral("origin"), xyz(0, 0, 0)},
                       {QStringLiteral("normal"), xyz(0, 1, 0)},
                       {QStringLiteral("x_axis"), xyz(1, 0, 0)},
                       {QStringLiteral("y_axis"), xyz(0, 0, -1)}};
  const Box corner = boxOf(mitcad::originDatumShape(xz, OriginStyle::Corner, 25.0));
  CHECK(within(corner.x0, 0.0) && within(corner.x1, 25.0));
  CHECK(within(corner.z0, 0.0) && within(corner.z1, 25.0));
  CHECK(within(corner.y0, 0.0) && within(corner.y1, 0.0));
  int faces = 0;
  for (TopExp_Explorer it(mitcad::originDatumShape(xz, OriginStyle::Corner, 25.0), TopAbs_FACE); it.More();
       it.Next()) {
    ++faces;
  }
  CHECK(faces == 1);
  // Centred: as before, the square around the origin.
  const Box centred = boxOf(mitcad::originDatumShape(xz, OriginStyle::Centred, 25.0));
  CHECK(within(centred.x0, -12.5) && within(centred.x1, 12.5));
  CHECK(within(centred.z0, -12.5) && within(centred.z1, 12.5));

  // An axis: from the origin along +Y past the planes, or through it.
  const QJsonObject y{{QStringLiteral("type"), QStringLiteral("axis")},
                      {QStringLiteral("origin"), xyz(0, 0, 0)},
                      {QStringLiteral("direction"), xyz(0, -1, 0)}};
  const Box reach = boxOf(mitcad::originDatumShape(y, OriginStyle::Corner, 80.0));
  CHECK(within(reach.y0, 0.0) && within(reach.y1, 80.0 * mitcad::kCornerAxisReach));
  const Box through = boxOf(mitcad::originDatumShape(y, OriginStyle::Centred, 80.0));
  CHECK(within(through.y0, -40.0) && within(through.y1, 40.0));

  // The point, and what is not drawn.
  const QJsonObject point{{QStringLiteral("type"), QStringLiteral("point")}, {QStringLiteral("point"), xyz(0, 0, 0)}};
  const TopoDS_Shape vertex = mitcad::originDatumShape(point, OriginStyle::Corner, 80.0);
  CHECK(!vertex.IsNull() && vertex.ShapeType() == TopAbs_VERTEX);
  CHECK(mitcad::originDatumShape(xz, OriginStyle::Corner, 0.0).IsNull());
  QJsonObject degenerate = xz;
  degenerate.insert(QStringLiteral("normal"), xyz(0, 0, 0));
  CHECK(mitcad::originDatumShape(degenerate, OriginStyle::Corner, 10.0).IsNull());
  return failures;
}
