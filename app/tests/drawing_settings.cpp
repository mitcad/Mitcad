// SPDX-License-Identifier: MIT
// Export Drawing's choices (mitcad#471): the defaults, kept in the settings
// (group drawing) and read back, unknown values ignored, typed scales, and
// the export_drawing command they make.
#include "UnitTests.hpp"
#include "files/DrawingSettings.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_settings.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int drawingSettingsTests() {
  using mitcad::DrawingSettings;
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings settings(directory.filePath(QStringLiteral("settings.ini")), QSettings::IniFormat);

  // The defaults: A4 landscape, fit, third angle, three views, dimensions
  // and the hole table.
  DrawingSettings s = DrawingSettings::load(settings);
  CHECK(s.format == QStringLiteral("pdf") && s.sheet == QStringLiteral("A4") && !s.portrait);
  CHECK(s.scale == QStringLiteral("fit") && !s.firstAngle);
  CHECK(s.views() == QStringList({QStringLiteral("front"), QStringLiteral("top"), QStringLiteral("right")}));
  CHECK(s.dimensions && s.holeTable && !s.hiddenLines && !s.sketchDimensions && !s.perBody);

  // Kept and read back.
  s.format = QStringLiteral("svg");
  s.sheet = QStringLiteral("A3");
  s.scale = QStringLiteral("1:2");
  s.firstAngle = true;
  s.iso = true;
  s.hiddenLines = true;
  s.save(settings);
  const DrawingSettings back = DrawingSettings::load(settings);
  CHECK(back.format == QStringLiteral("svg") && back.sheet == QStringLiteral("A3"));
  CHECK(back.scale == QStringLiteral("1:2") && back.firstAngle && back.iso && back.hiddenLines);
  CHECK(back.views().size() == 4);

  // DWG is a format of its own (mitcad#728).
  settings.setValue(QStringLiteral("drawing/format"), QStringLiteral("dwg"));
  CHECK(DrawingSettings::load(settings).format == QStringLiteral("dwg"));

  // Unknown values are the defaults.
  settings.setValue(QStringLiteral("drawing/sheet"), QStringLiteral("B5"));
  settings.setValue(QStringLiteral("drawing/format"), QStringLiteral("png"));
  settings.setValue(QStringLiteral("drawing/scale"), QStringLiteral("huge"));
  const DrawingSettings odd = DrawingSettings::load(settings);
  CHECK(odd.sheet == QStringLiteral("A4") && odd.format == QStringLiteral("pdf") && odd.scale == QStringLiteral("fit"));

  // Typed scales.
  CHECK(mitcad::normalizedScale(QStringLiteral(" 1 : 5 ")) == QStringLiteral("1:5"));
  CHECK(mitcad::normalizedScale(QStringLiteral("Fit to sheet")) == QStringLiteral("fit"));
  // The dialog's own entry, worded as the drawing workspace's (mitcad#725).
  CHECK(mitcad::normalizedScale(QStringLiteral("Fit the Sheet")) == QStringLiteral("fit"));
  CHECK(mitcad::normalizedScale(QStringLiteral("2.5:1")) == QStringLiteral("2.5:1"));
  CHECK(mitcad::normalizedScale(QStringLiteral("1:0")).isEmpty());
  CHECK(mitcad::normalizedScale(QStringLiteral("half")).isEmpty());

  // The command.
  const QJsonObject command =
      back.command(QStringLiteral("/tmp/part.svg"), {QStringLiteral("F2.b0")}, QStringLiteral("Part"),
                   QStringLiteral("Section1"));
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("export_drawing"));
  CHECK(command.value(QStringLiteral("sheet")).toString() == QStringLiteral("A3"));
  CHECK(command.value(QStringLiteral("projection")).toString() == QStringLiteral("first_angle"));
  CHECK(command.value(QStringLiteral("orientation")).toString() == QStringLiteral("landscape"));
  CHECK(command.value(QStringLiteral("views")).toArray().size() == 4);
  CHECK(command.value(QStringLiteral("bodies")).toArray().at(0).toString() == QStringLiteral("F2.b0"));
  CHECK(command.value(QStringLiteral("section")).toObject().value(QStringLiteral("analysis")).toString() ==
        QStringLiteral("Section1"));
  CHECK(command.value(QStringLiteral("hidden_lines")).toBool());
  const QJsonObject all = s.command(QStringLiteral("a.pdf"), {}, QStringLiteral("A"), QString());
  CHECK(!all.contains(QStringLiteral("bodies")) && !all.contains(QStringLiteral("section")));
  return failures;
}
MITCAD_UNIT_TEST(drawingSettingsTests);
