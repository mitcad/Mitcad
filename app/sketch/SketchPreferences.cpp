// SPDX-License-Identifier: MIT
#include "SketchPreferences.hpp"

#include <QSettings>
#include <QString>

namespace mitcad::sketch {

namespace {

const char* const kSetting = "sketch/scaleToFirstDimension";

} // namespace

bool scaleToFirstDimension() {
  const QSettings settings;
  if (settings.contains(QLatin1String(kSetting))) {
    return settings.value(QLatin1String(kSetting)).toBool();
  }
  return qgetenv("MITCAD_SKETCH_FIRST_DIMENSION_SCALE").trimmed() != "0";
}

void saveScaleToFirstDimension(bool on) { QSettings().setValue(QLatin1String(kSetting), on); }

QJsonObject withFirstDimensionScale(const QJsonObject& command) {
  if (command.value(QStringLiteral("cmd")).toString() != QStringLiteral("sketch.add_dimension") ||
      !scaleToFirstDimension()) {
    return command;
  }
  QJsonObject asked = command;
  asked.insert(QStringLiteral("scale_sketch"), true);
  return asked;
}

// Click adds to the selection in sketches (mitcad#1403).
namespace {

const char* const kClickAddsSetting = "sketch/clickAddsToSelection";

} // namespace

bool clickAddsToSelection() { return QSettings().value(QLatin1String(kClickAddsSetting), false).toBool(); }

void saveClickAddsToSelection(bool on) { QSettings().setValue(QLatin1String(kClickAddsSetting), on); }

} // namespace mitcad::sketch
