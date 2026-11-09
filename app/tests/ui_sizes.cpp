// SPDX-License-Identifier: MIT
// Preferences › Display › Text size, Icon size and Font (mitcad#1556,
// framework/UiSizes.hpp): 100 % and the system's font by default, a kept
// size read back, anything outside 80 to 200 % in steps of 5 % 100 %;
// uiIconSize and uiRoomSize at a run's sizes; the application font that
// applyTextAndIconSize gives. The application font and the run's sizes are
// put back afterwards.
#include "framework/UiSizes.hpp"

#include <cmath>
#include <cstdio>

#include <QApplication>
#include <QFont>
#include <QFontDatabase>
#include <QSettings>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "ui_sizes.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

void settingTests() {
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);
  // None kept: 100 % and the system's font.
  CHECK(mitcad::savedTextSize(settings) == 100);
  CHECK(mitcad::savedIconSize(settings) == 100);
  CHECK(mitcad::savedFontFamily(settings).isEmpty());
  // Kept as the percent and read back.
  CHECK(mitcad::saveTextSize(settings, 80));
  CHECK(mitcad::saveIconSize(settings, 150));
  CHECK(settings.value(QStringLiteral("appearance/textSize")).toInt() == 80);
  CHECK(settings.value(QStringLiteral("appearance/iconSize")).toInt() == 150);
  CHECK(mitcad::savedTextSize(settings) == 80);
  CHECK(mitcad::savedIconSize(settings) == 150);
  // The same again changes nothing; a size off the steps is not kept.
  CHECK(!mitcad::saveTextSize(settings, 80));
  CHECK(!mitcad::saveIconSize(settings, 152));
  CHECK(!mitcad::saveIconSize(settings, 75));
  CHECK(!mitcad::saveIconSize(settings, 205));
  CHECK(mitcad::savedIconSize(settings) == 150);
  // The family: kept trimmed, empty is the system's (no key).
  CHECK(mitcad::saveFontFamily(settings, QStringLiteral(" DejaVu Sans ")));
  CHECK(settings.value(QStringLiteral("appearance/fontFamily")).toString() == QStringLiteral("DejaVu Sans"));
  CHECK(mitcad::savedFontFamily(settings) == QStringLiteral("DejaVu Sans"));
  CHECK(!mitcad::saveFontFamily(settings, QStringLiteral("DejaVu Sans")));
  CHECK(mitcad::saveFontFamily(settings, QString()));
  CHECK(!settings.contains(QStringLiteral("appearance/fontFamily")));
  // Every step round trips; anything else is 100 %.
  for (int percent = mitcad::kUiSizeMinimum; percent <= mitcad::kUiSizeMaximum; percent += mitcad::kUiSizeStep) {
    CHECK(mitcad::uiSizeFromValue(percent) == percent);
  }
  CHECK(mitcad::uiSizeFromValue(QVariant()) == 100);
  CHECK(mitcad::uiSizeFromValue(QStringLiteral("135")) == 135);
  CHECK(mitcad::uiSizeFromValue(QStringLiteral("large")) == 100);
  CHECK(mitcad::uiSizeFromValue(77) == 100);
  CHECK(mitcad::uiSizeFromValue(75) == 100);
  CHECK(mitcad::uiSizeFromValue(210) == 100);
}

void sizeTests() {
  namespace detail = mitcad::ui_sizes_detail;
  const int text = detail::textPercent;
  const int icon = detail::iconPercent;
  // 100 %: the sizes as they are.
  detail::textPercent = 100;
  detail::iconPercent = 100;
  CHECK(mitcad::uiIconSize(18) == 18);
  CHECK(mitcad::uiRoomSize(26) == 26);
  // Icons at 150 %, text at 80 %: icons grow, rooms with the larger.
  detail::textPercent = 80;
  detail::iconPercent = 150;
  CHECK(mitcad::uiIconSize(18) == 27);
  CHECK(mitcad::uiIconSize(QSize(18, 12)) == QSize(27, 18));
  CHECK(mitcad::uiRoomSize(26) == 39);
  CHECK(std::abs(mitcad::uiTextScale() - 0.8) < 1e-9);
  // Icons at 80 %: rounded, never smaller than 1; rooms never below 100 %.
  detail::textPercent = 80;
  detail::iconPercent = 80;
  CHECK(mitcad::uiIconSize(18) == 14);
  CHECK(mitcad::uiIconSize(28) == 22);
  CHECK(mitcad::uiIconSize(1) == 1);
  CHECK(mitcad::uiRoomSize(26) == 26);
  // Text at 200 %: rooms double.
  detail::textPercent = 200;
  CHECK(mitcad::uiRoomSize(26) == 52);
  detail::textPercent = text;
  detail::iconPercent = icon;
}

void applyTests() {
  namespace detail = mitcad::ui_sizes_detail;
  const QFont kept = QApplication::font();
  const int text = detail::textPercent;
  const int icon = detail::iconPercent;
  const double platform = detail::platformPointSize;
  const bool chosen = detail::fontChosen;

  QFont start = kept;
  start.setPointSizeF(10.0);
  QApplication::setFont(start);
  // 80 % of 10 pt, icons at 150 %, the system's font.
  mitcad::TextAndIconSize sizes = mitcad::applyTextAndIconSize(80, 150, QString());
  CHECK(sizes.textPercent == 80);
  CHECK(sizes.iconPercent == 150);
  CHECK(std::abs(QApplication::font().pointSizeF() - 8.0) < 1e-6);
  CHECK(std::abs(detail::platformPointSize - 10.0) < 1e-6);
  CHECK(mitcad::uiIconSize(18) == 27);
  CHECK(!detail::fontChosen);
  CHECK(mitcad::textAndIconSizeDescription(sizes) ==
        QStringLiteral("Text size 80%, Icon size 150%, Font the system's"));
  CHECK(mitcad::applicationFontDescription().contains(QStringLiteral(", 8 pt (Text size 80% of 10 pt)")));

  // Sizes that are not steps: 100 %; a family that is not installed leaves
  // the system's.
  QApplication::setFont(start);
  sizes = mitcad::applyTextAndIconSize(81, 300, QStringLiteral("No Such Family 1556"));
  CHECK(sizes.textPercent == 100);
  CHECK(sizes.iconPercent == 100);
  CHECK(!sizes.familyInstalled);
  CHECK(!detail::fontChosen);
  CHECK(std::abs(QApplication::font().pointSizeF() - 10.0) < 1e-6);
  CHECK(mitcad::textAndIconSizeDescription(sizes) ==
        QStringLiteral("Text size 100%, Icon size 100%, Font No Such Family 1556 (not installed; the system's)"));

  // An installed family at 200 %: the application font's family and 20 pt.
  const QStringList families = QFontDatabase::families();
  if (!families.isEmpty()) {
    const QString family = families.constFirst();
    QApplication::setFont(start);
    sizes = mitcad::applyTextAndIconSize(200, 80, family);
    CHECK(sizes.familyInstalled);
    CHECK(detail::fontChosen);
    CHECK(QApplication::font().family() == family);
    CHECK(std::abs(QApplication::font().pointSizeF() - 20.0) < 1e-6);
    CHECK(mitcad::uiIconSize(28) == 22);
  }

  QApplication::setFont(kept);
  detail::textPercent = text;
  detail::iconPercent = icon;
  detail::platformPointSize = platform;
  detail::fontChosen = chosen;
}

} // namespace

int uiSizesTests() {
  settingTests();
  sizeTests();
  applyTests();
  return failures;
}
