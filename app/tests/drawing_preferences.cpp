// SPDX-License-Identifier: MIT
// Preferences › Display › Always show corners in drawings and its Corner dot
// size (mitcad#1338): off and Medium by default, kept in the settings (group
// drawing) and read back, an unknown size Medium, the sizes' ids and radii.
#include "drawings/DrawingPreferences.hpp"
#include "drawings/DrawingSheetView.hpp"

#include <cstdio>
#include <initializer_list>

#include <QSettings>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_preferences.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int drawingPreferencesTests() {
  using mitcad::drawings::CornerDots;
  using mitcad::drawings::CornerDotSize;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);

  // The defaults: off, Medium.
  const CornerDots defaults = CornerDots::load(settings);
  CHECK(!defaults.always);
  CHECK(defaults.size == CornerDotSize::Medium);

  // Kept and read back, under its own keys.
  CornerDots on;
  on.always = true;
  on.size = CornerDotSize::Large;
  on.save(settings);
  settings.sync();
  CHECK(settings.value(QStringLiteral("drawing/alwaysShowCorners")).toBool());
  CHECK(settings.value(QStringLiteral("drawing/cornerDotSize")).toString() == QStringLiteral("large"));
  CHECK(CornerDots::load(settings) == on);
  // Read back by another QSettings of the same file.
  {
    const QSettings again(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);
    CHECK(CornerDots::load(again) == on);
  }
  // Off again, its size kept.
  CornerDots off = on;
  off.always = false;
  off.save(settings);
  const CornerDots back = CornerDots::load(settings);
  CHECK(!back.always && back.size == CornerDotSize::Large);
  CHECK(back != on);

  // An unknown size is Medium.
  settings.setValue(QStringLiteral("drawing/cornerDotSize"), QStringLiteral("huge"));
  CHECK(CornerDots::load(settings).size == CornerDotSize::Medium);

  // The ids round trip; the radii grow and stay under the marker of the
  // corner under the pointer.
  double last = 0.0;
  for (const CornerDotSize size : {CornerDotSize::Small, CornerDotSize::Medium, CornerDotSize::Large}) {
    CHECK(mitcad::drawings::cornerDotSizeFromId(mitcad::drawings::cornerDotSizeId(size)) == size);
    const double radius = mitcad::drawings::cornerDotRadius(size);
    CHECK(radius > last);
    CHECK(radius < mitcad::drawings::DrawingSheetView::kCornerMarkerRadius);
    last = radius;
  }
  CHECK(!mitcad::drawings::cornerDotSizeFromId(QStringLiteral("medium ")).has_value());
  return failures;
}
