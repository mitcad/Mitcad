// SPDX-License-Identifier: MIT
// Preferences › Display › Selection color and Selected line width
// (mitcad#1803): the selection role's blue and Normal by default; kept in
// the settings (view/selectionColor, view/selectedLineWidth) and read back,
// unknown values the defaults; the widths in logical pixels, every one
// wider than a plain edge, Thin under Normal under Thick.
#include "view/SelectionLook.hpp"

#include <cstdio>

#include <QSettings>
#include <QTemporaryDir>

#include "framework/Theme.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "selection_look.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int selectionLookTests() {
  using mitcad::SelectedLineWidth;
  using mitcad::SelectionLook;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);

  // The defaults: the selection role's blue, Normal.
  const SelectionLook defaults = SelectionLook::load(settings);
  CHECK(defaults.color == mitcad::viewColor(mitcad::ColorRole::Select, false));
  CHECK(defaults.color == mitcad::defaultSelectionColor());
  CHECK(defaults.width == SelectedLineWidth::Normal);
  CHECK(defaults.describe() ==
        QStringLiteral("color %1, line width normal").arg(mitcad::defaultSelectionColor().name()));

  // Kept and read back under their keys.
  SelectionLook chosen;
  chosen.color = QColor(QStringLiteral("#d02090"));
  chosen.width = SelectedLineWidth::Thick;
  chosen.save(settings);
  settings.sync();
  CHECK(settings.value(QStringLiteral("view/selectionColor")).toString() == QStringLiteral("#d02090"));
  CHECK(settings.value(QStringLiteral("view/selectedLineWidth")).toString() == QStringLiteral("thick"));
  CHECK(SelectionLook::load(settings) == chosen);
  CHECK(SelectionLook::load(settings) != defaults);
  CHECK(chosen.describe() == QStringLiteral("color #d02090, line width thick"));

  // Unknown values are the defaults.
  settings.setValue(QStringLiteral("view/selectionColor"), QStringLiteral("not a color"));
  settings.setValue(QStringLiteral("view/selectedLineWidth"), QStringLiteral("huge"));
  CHECK(SelectionLook::load(settings) == defaults);

  // The ids round trip.
  for (const SelectedLineWidth width : {SelectedLineWidth::Thin, SelectedLineWidth::Normal, SelectedLineWidth::Thick}) {
    CHECK(mitcad::selectedLineWidthFromId(mitcad::selectedLineWidthId(width)) == width);
  }
  CHECK(!mitcad::selectedLineWidthFromId(QStringLiteral("medium")).has_value());

  // The widths: every one wider than a plain edge and than the one before,
  // a face's boundary narrower than an edge.
  double previous = mitcad::kPlainEdgeWidth;
  for (const SelectedLineWidth width : {SelectedLineWidth::Thin, SelectedLineWidth::Normal, SelectedLineWidth::Thick}) {
    CHECK(mitcad::selectedEdgeWidth(width) > previous);
    CHECK(mitcad::selectedBoundaryWidth(width) > mitcad::kPlainEdgeWidth);
    CHECK(mitcad::selectedBoundaryWidth(width) < mitcad::selectedEdgeWidth(width));
    previous = mitcad::selectedEdgeWidth(width);
  }
  CHECK(mitcad::selectedEdgeWidth(SelectedLineWidth::Normal) == 4.0);
  CHECK(mitcad::selectedBoundaryWidth(SelectedLineWidth::Normal) == 3.0);
  return failures;
}
