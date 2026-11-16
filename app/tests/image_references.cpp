// SPDX-License-Identifier: MIT
// Image references (mitcad#782): the model's entries as the view draws
// them (hidden ones and those whose plane is gone left out), points across
// an image and where a ray meets its plane, the log line UI tests read and
// a calibration's distance as typed.
#include "UnitTests.hpp"
#include "view/ImageReferences.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "image_references.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(double a, double b) { return std::abs(a - b) < 1e-9; }

bool same(const mitcad::ImagePoint& a, const mitcad::ImagePoint& b) {
  return same(a[0], b[0]) && same(a[1], b[1]) && same(a[2], b[2]);
}

// A 200 x 100 pixel image, 50 x 25 mm on XY centred at (10, 10).
QJsonObject entry(const QString& name) {
  return {{QStringLiteral("name"), name},
          {QStringLiteral("sha256"), QStringLiteral("ab12")},
          {QStringLiteral("visible"), true},
          {QStringLiteral("opacity"), 0.5},
          {QStringLiteral("through"), false},
          {QStringLiteral("pixels"), QJsonArray{200, 100}},
          {QStringLiteral("corners"), QJsonArray{QJsonArray{-15, 22.5, 0}, QJsonArray{35, 22.5, 0},
                                                 QJsonArray{35, -2.5, 0}, QJsonArray{-15, -2.5, 0}}}};
}

void entryTests() {
  const auto image = mitcad::imageReferenceOf(entry(QStringLiteral("Image1")));
  CHECK(image.has_value());
  CHECK(image->name == QStringLiteral("Image1") && image->sha256 == QStringLiteral("ab12"));
  CHECK(image->width == 200 && image->height == 100);
  CHECK(same(image->opacity, 0.5) && !image->through && image->visible);
  CHECK(same(image->corners[2], {35, -2.5, 0}));

  // Its plane gone, or not four corners: not drawn.
  QJsonObject broken = entry(QStringLiteral("Gone"));
  broken.insert(QStringLiteral("error"), QStringLiteral("the image's plane: no such face"));
  CHECK(!mitcad::imageReferenceOf(broken));
  QJsonObject three = entry(QStringLiteral("Three"));
  three.insert(QStringLiteral("corners"), QJsonArray{QJsonArray{0, 0, 0}, QJsonArray{1, 0, 0}, QJsonArray{1, 1, 0}});
  CHECK(!mitcad::imageReferenceOf(three));

  // Hidden ones are not drawn either.
  QJsonObject hidden = entry(QStringLiteral("Hidden"));
  hidden.insert(QStringLiteral("visible"), false);
  QJsonObject through = entry(QStringLiteral("Plan"));
  through.insert(QStringLiteral("through"), true);
  through.insert(QStringLiteral("opacity"), 0.3);
  const auto shown = mitcad::shownImageReferences(QJsonArray{entry(QStringLiteral("Image1")), broken, hidden, through});
  CHECK(shown.size() == 2);
  CHECK(mitcad::imageReferencesLog(shown) ==
        QStringLiteral("Image references drawn: Image1 50.0 x 25.0 mm 50% behind, Plan 50.0 x 25.0 mm 30% through"));
  CHECK(mitcad::imageReferencesLog({}) == QStringLiteral("Image references drawn: none"));
}

void pointTests() {
  const auto image = mitcad::imageReferenceOf(entry(QStringLiteral("Image1")));
  CHECK(same(mitcad::imagePointAt(*image, 0.5, 0.5), {10, 10, 0}));
  CHECK(same(mitcad::imagePointAt(*image, 0.1, 0.5), {-10, 10, 0}));
  CHECK(same(mitcad::imagePointAt(*image, 0.5, 0.9), {10, 0, 0}));
  const std::array<double, 2> size = mitcad::imageSize(*image);
  CHECK(same(size[0], 50.0) && same(size[1], 25.0));

  // Where points are across and down the image: its middle, a corner,
  // off its left edge (Calibrate's first click takes the image it is on).
  const std::array<double, 2> middle = mitcad::imageFractions(*image, {10, 10, 5});
  CHECK(same(middle[0], 0.5) && same(middle[1], 0.5));
  const std::array<double, 2> corner = mitcad::imageFractions(*image, {35, -2.5, 0});
  CHECK(same(corner[0], 1.0) && same(corner[1], 1.0));
  CHECK(mitcad::imageFractions(*image, {-20, 10, 0})[0] < 0.0);

  // A ray from above, slanted: it meets the plane z = 0.
  const auto hit = mitcad::imagePlaneHit(*image, {1, 2, 10}, {1, 0, -2});
  CHECK(hit && same(*hit, {6, 2, 0}));
  // Along the plane: no point.
  CHECK(!mitcad::imagePlaneHit(*image, {1, 2, 10}, {1, 1, 0}));
}

void distanceTests() {
  CHECK(mitcad::calibrationExpression(QStringLiteral(" 40 "), QStringLiteral("mm")) == QStringLiteral("40 mm"));
  CHECK(mitcad::calibrationExpression(QStringLiteral("2.5"), QStringLiteral("in")) == QStringLiteral("2.5 in"));
  CHECK(mitcad::calibrationExpression(QStringLiteral("4 cm"), QStringLiteral("mm")) == QStringLiteral("4 cm"));
  CHECK(mitcad::calibrationExpression(QStringLiteral("d1 * 2"), QStringLiteral("mm")) == QStringLiteral("d1 * 2"));
}

} // namespace

int imageReferencesTests() {
  failures = 0;
  entryTests();
  pointTests();
  distanceTests();
  return failures;
}
MITCAD_UNIT_TEST(imageReferencesTests);
