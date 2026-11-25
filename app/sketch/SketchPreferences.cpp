// SPDX-License-Identifier: MIT
#include "SketchPreferences.hpp"

#include <QSettings>
#include <QString>

#include "mitcad_bridge/lib.h"

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

void resetScaleToFirstDimension() { QSettings().remove(QLatin1String(kSetting)); }

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

// Esc leaves an unchanged sketch (mitcad#1623).
namespace {

const char* const kEscLeavesSetting = "sketch/escLeavesUnchangedSketch";

} // namespace

bool escLeavesUnchangedSketch() { return QSettings().value(QLatin1String(kEscLeavesSetting), false).toBool(); }

void saveEscLeavesUnchangedSketch(bool on) { QSettings().setValue(QLatin1String(kEscLeavesSetting), on); }

// Show profiles when a sketch opens (mitcad#1670).
namespace {

const char* const kShowProfilesSetting = "sketch/showProfilesOnOpen";

} // namespace

bool showProfilesOnOpen() { return QSettings().value(QLatin1String(kShowProfilesSetting), true).toBool(); }

void saveShowProfilesOnOpen(bool on) { QSettings().setValue(QLatin1String(kShowProfilesSetting), on); }

// Project the face's edges into a sketch on a face (mitcad#2051).
namespace {

const char* const kProjectFaceSetting = "sketch/projectFaceOnCreate";

} // namespace

bool projectFaceOnCreate() {
  const QSettings settings;
  if (settings.contains(QLatin1String(kProjectFaceSetting))) {
    return settings.value(QLatin1String(kProjectFaceSetting)).toBool();
  }
  return qgetenv("MITCAD_SKETCH_PROJECT_FACE").trimmed() != "0";
}

void saveProjectFaceOnCreate(bool on) { QSettings().setValue(QLatin1String(kProjectFaceSetting), on); }

void resetProjectFaceOnCreate() { QSettings().remove(QLatin1String(kProjectFaceSetting)); }

// Hide a sketch when a feature uses it (mitcad#2393).
namespace {

const char* const kHideUsedSketchesSetting = "sketch/hideUsedSketches";

} // namespace

bool hideUsedSketches() { return QSettings().value(QLatin1String(kHideUsedSketchesSetting), true).toBool(); }

void saveHideUsedSketches(bool on) {
  QSettings().setValue(QLatin1String(kHideUsedSketchesSetting), on);
  applyHideUsedSketches();
}

void applyHideUsedSketches() { ::mitcad::set_hide_used_sketches(hideUsedSketches()); }

} // namespace mitcad::sketch
