// SPDX-License-Identifier: MIT
// The sketch's value fields over the view (mitcad#2086, GitHub
// mitcad/Mitcad#485): selected text is the palette's pair in its light
// values, the accent (Action) behind and AccentText on it, readable (at
// least 4.5:1), for a valid and an invalid value; never HoverFill, the
// hovered row's role, which hid the selection on the field's near-white
// background. Read from the field's palette, as the style sheet sets it.
#include "UnitTests.hpp"
#include "framework/Theme.hpp"
#include "sketch/SketchFieldStyle.hpp"

#include <cstdio>

#include <QColor>
#include <QLineEdit>
#include <QPalette>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_field_style.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

void selectionTests(bool invalid) {
  QLineEdit edit;
  edit.setStyleSheet(sketch::valueFieldStyle(invalid));
  edit.setText(QStringLiteral("25 mm"));
  edit.selectAll();
  edit.ensurePolished();
  const QColor behind = edit.palette().color(QPalette::Active, QPalette::Highlight);
  const QColor text = edit.palette().color(QPalette::Active, QPalette::HighlightedText);
  // The light values whatever the theme: the field is always light.
  CHECK(behind.rgb() == themeColor(ColorRole::Action, false).rgb());
  CHECK(text.rgb() == themeColor(ColorRole::AccentText, false).rgb());
  CHECK(behind.rgb() != themeColor(ColorRole::HoverFill, false).rgb());
  CHECK(contrastRatio(text, behind) >= 4.5);
  // The selection stands out from the field's own background.
  const QColor base = invalid ? themeColor(ColorRole::ErrorFill, false) : QColor(Qt::white);
  CHECK(contrastRatio(behind, base) >= 3.0);
}

} // namespace

int sketchFieldStyleTests() {
  failures = 0;
  selectionTests(false);
  selectionTests(true);
  return failures;
}
MITCAD_UNIT_TEST(sketchFieldStyleTests);
