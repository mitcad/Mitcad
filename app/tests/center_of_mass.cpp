// SPDX-License-Identifier: MIT
// The Center of Mass marker (mitcad#1094): the browser row's state and
// tooltip from the `center_of_mass` query's answer, the UI tests' log line,
// and the marker's picture: a crossed circle, its upper right and lower left
// quadrants in the inspection's color, the others and a ring around it in
// the background's tone, the same size at every ratio in view pixels.
#include "view/CenterOfMass.hpp"
#include "framework/Theme.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>

#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "center_of_mass.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

int channelGap(const QColor& a, const QColor& b) {
  return std::max({std::abs(a.red() - b.red()), std::abs(a.green() - b.green()), std::abs(a.blue() - b.blue())});
}

QJsonObject answer(bool visible, int defaults) {
  QJsonArray point;
  point.append(15.0);
  point.append(-10.5);
  point.append(2.25);
  return {{QStringLiteral("center_of_mass"), point},
          {QStringLiteral("mass"), 0.0351},
          {QStringLiteral("volume"), 8400.0},
          {QStringLiteral("bodies"), 2},
          {QStringLiteral("default_material"), defaults},
          {QStringLiteral("marker"), QJsonObject{{QStringLiteral("visible"), visible}}}};
}

void stateTests() {
  // No marker: an empty snapshot entry, or the query's null marker.
  CHECK(!centerOfMassOf(QJsonObject()).has_value());
  QJsonObject none = answer(true, 0);
  none.insert(QStringLiteral("marker"), QJsonValue());
  CHECK(!centerOfMassOf(none).has_value());
  CHECK(centerOfMassLog(centerOfMassOf(none)) == QStringLiteral("Center of Mass: none"));

  const std::optional<CenterOfMassShown> shown = centerOfMassOf(answer(true, 1));
  CHECK(shown.has_value());
  CHECK(shown && shown->visible && shown->point.has_value());
  CHECK(shown && shown->point && (*shown->point)[1] == -10.5);
  CHECK(shown && shown->bodies == 2 && shown->defaultMaterial == 1);
  CHECK(shown && centerOfMassTip(*shown) ==
                     QStringLiteral("(15, -10.5, 2.25) mm, 0.0351 kg; 1 body without a material counted as steel"));
  CHECK(centerOfMassLog(shown) ==
        QStringLiteral("Center of Mass: at 15.000000000, -10.500000000, 2.250000000 mm, mass 0.035100000000 kg, "
                       "2 bodies, 1 without a material (shown)"));
  const std::optional<CenterOfMassShown> three = centerOfMassOf(answer(false, 3));
  CHECK(three && !three->visible);
  CHECK(three && centerOfMassTip(*three) ==
                     QStringLiteral("(15, -10.5, 2.25) mm, 0.0351 kg; 3 bodies without a material counted as steel"));
  CHECK(centerOfMassLog(three).endsWith(QStringLiteral("3 without a material (hidden)")));
  const std::optional<CenterOfMassShown> own = centerOfMassOf(answer(true, 0));
  CHECK(own && centerOfMassTip(*own) == QStringLiteral("(15, -10.5, 2.25) mm, 0.0351 kg"));

  // No bodies: a marker without a point.
  QJsonObject empty = answer(true, 0);
  empty.insert(QStringLiteral("center_of_mass"), QJsonValue());
  const std::optional<CenterOfMassShown> nothing = centerOfMassOf(empty);
  CHECK(nothing && !nothing->point);
  CHECK(centerOfMassLog(nothing) == QStringLiteral("Center of Mass: no bodies (shown)"));

  // A query that failed: why, and no point.
  const QJsonObject failed{{QStringLiteral("marker"), QJsonObject{{QStringLiteral("visible"), true}}},
                           {QStringLiteral("error"), QStringLiteral("Body1: no volume")}};
  const std::optional<CenterOfMassShown> broken = centerOfMassOf(failed);
  CHECK(broken && !broken->point);
  CHECK(broken && centerOfMassTip(*broken) == QStringLiteral("Could not compute the center of mass: Body1: no volume"));
  CHECK(centerOfMassLog(broken) == QStringLiteral("Center of Mass: failed: Body1: no volume (shown)"));
}

void imageTests() {
  const QColor ink = centerOfMassInk();
  CHECK(channelGap(ink, viewColor(ColorRole::PreviewTarget, false)) == 0);
  for (const bool dark : {false, true}) {
    const QColor disc = centerOfMassDisc(dark);
    CHECK(channelGap(disc, viewColor(ColorRole::SketchHalo, dark)) == 0);
    for (const double ratio : {1.0, 2.0}) {
      const QImage image = centerOfMassImage(dark, ratio);
      // Odd sized, the center on the middle pixel, the same size in view
      // pixels at both ratios.
      CHECK(image.width() == image.height() && image.width() % 2 == 1);
      CHECK(image.format() == QImage::Format_RGBA8888);
      const double extent = kCenterOfMassRadius + kCenterOfMassRing / 2.0 + kCenterOfMassHalo;
      CHECK(image.width() >= 2.0 * extent * ratio && image.width() <= 2.0 * (extent + 2.0) * ratio + 1.0);
      const int mid = image.width() / 2;
      const int quarter = static_cast<int>(std::lround(kCenterOfMassRadius * ratio * 0.45));
      const auto at = [&](int dx, int dy) { return image.pixelColor(mid + dx, mid + dy); };
      // Upper right and lower left filled in the ink, the others the disc.
      CHECK(channelGap(at(quarter, -quarter), ink) <= 3);
      CHECK(channelGap(at(-quarter, quarter), ink) <= 3);
      CHECK(channelGap(at(-quarter, -quarter), disc) <= 3);
      CHECK(channelGap(at(quarter, quarter), disc) <= 3);
      CHECK(at(quarter, quarter).alpha() == 255);
      // Transparent in the corners, outside the disc.
      CHECK(image.pixelColor(0, 0).alpha() == 0);
      CHECK(image.pixelColor(image.width() - 1, image.height() - 1).alpha() == 0);
    }
  }
  // The quadrants stand out from the disc on both backgrounds.
  CHECK(channelGap(ink, centerOfMassDisc(false)) > 100);
  CHECK(channelGap(ink, centerOfMassDisc(true)) > 100);
}

} // namespace

int centerOfMassTests() {
  stateTests();
  imageTests();
  return failures;
}
