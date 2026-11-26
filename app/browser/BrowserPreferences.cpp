// SPDX-License-Identifier: MIT
#include "BrowserPreferences.hpp"

#include <QLatin1String>
#include <QSettings>

namespace mitcad {

namespace {

const char* const kConsumeSketchesSetting = "browser/consumeSketches";
const char* const kActivatePlaceSetting = "browser/activateButtonPlace";
const char* const kEyePlaceSetting = "browser/showEyePlace";

BrowserControlPlace savedPlace(const char* key) {
  return browserControlPlaceFromId(QSettings().value(QLatin1String(key)).toString());
}

void savePlace(const char* key, BrowserControlPlace place) {
  const bool same = QSettings().contains(QLatin1String(key)) && savedPlace(key) == place;
  if (same) {
    return;
  }
  const bool changed = savedPlace(key) != place;
  QSettings().setValue(QLatin1String(key), browserControlPlaceId(place));
  if (changed) {
    emit BrowserPreferencesNotifier::instance().controlPlacesChanged();
  }
}

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

QString browserControlPlaceId(BrowserControlPlace place) {
  return place == BrowserControlPlace::Right ? QStringLiteral("right") : QStringLiteral("classic");
}

BrowserControlPlace browserControlPlaceFromId(const QString& id) {
  return id == QLatin1String("right") ? BrowserControlPlace::Right : BrowserControlPlace::Classic;
}

BrowserControlPlace browserActivatePlace() { return savedPlace(kActivatePlaceSetting); }

BrowserControlPlace browserEyePlace() { return savedPlace(kEyePlaceSetting); }

void setBrowserActivatePlace(BrowserControlPlace place) { savePlace(kActivatePlaceSetting, place); }

void setBrowserEyePlace(BrowserControlPlace place) { savePlace(kEyePlaceSetting, place); }

// The timeline's presentation (mitcad#2427).
namespace {
const char* const kTimelinePresentationSetting = "timeline/presentation";
} // namespace

QString timelinePresentationId(TimelinePresentation presentation) {
  return presentation == TimelinePresentation::HistoryList ? QStringLiteral("list") : QStringLiteral("strip");
}

TimelinePresentation timelinePresentationFromId(const QString& id) {
  return id == QLatin1String("list") ? TimelinePresentation::HistoryList : TimelinePresentation::Strip;
}

TimelinePresentation timelinePresentation() {
  return timelinePresentationFromId(QSettings().value(QLatin1String(kTimelinePresentationSetting)).toString());
}

void setTimelinePresentation(TimelinePresentation presentation) {
  const QString key = QLatin1String(kTimelinePresentationSetting);
  if (QSettings().contains(key) && timelinePresentation() == presentation) {
    return;
  }
  const bool changed = timelinePresentation() != presentation;
  QSettings().setValue(key, timelinePresentationId(presentation));
  if (changed) {
    emit BrowserPreferencesNotifier::instance().timelinePresentationChanged();
  }
}

BrowserPreferencesNotifier& BrowserPreferencesNotifier::instance() {
  static BrowserPreferencesNotifier notifier;
  return notifier;
}

} // namespace mitcad
