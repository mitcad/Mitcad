// SPDX-License-Identifier: MIT
#include "BrowserPreferences.hpp"

#include <QLatin1String>
#include <QSettings>

namespace mitcad {

namespace {

const char* const kConsumeSketchesSetting = "browser/consumeSketches";

} // namespace

bool browserConsumeSketches() {
  return QSettings().value(QLatin1String(kConsumeSketchesSetting), false).toBool();
}

void setBrowserConsumeSketches(bool on) {
  if (on == browserConsumeSketches()) {
    return;
  }
  QSettings().setValue(QLatin1String(kConsumeSketchesSetting), on);
  emit BrowserPreferencesNotifier::instance().consumeSketchesChanged(on);
}

BrowserPreferencesNotifier& BrowserPreferencesNotifier::instance() {
  static BrowserPreferencesNotifier notifier;
  return notifier;
}

} // namespace mitcad
