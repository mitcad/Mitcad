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

// The Browser rows' spacing (mitcad#2513).
namespace {
const char* const kRowSpacingSetting = "browser/rowSpacing";
} // namespace

QString browserRowSpacingId(BrowserRowSpacing spacing) {
  switch (spacing) {
  case BrowserRowSpacing::Compact:
    return QStringLiteral("compact");
  case BrowserRowSpacing::Comfortable:
    return QStringLiteral("comfortable");
  case BrowserRowSpacing::Standard:
    break;
  }
  return QStringLiteral("standard");
}

BrowserRowSpacing browserRowSpacingFromId(const QString& id) {
  if (id == QLatin1String("compact")) {
    return BrowserRowSpacing::Compact;
  }
  if (id == QLatin1String("comfortable")) {
    return BrowserRowSpacing::Comfortable;
  }
  return BrowserRowSpacing::Standard;
}

BrowserRowSpacing browserRowSpacing() {
  return browserRowSpacingFromId(QSettings().value(QLatin1String(kRowSpacingSetting)).toString());
}

void setBrowserRowSpacing(BrowserRowSpacing spacing) {
  const QString key = QLatin1String(kRowSpacingSetting);
  if (QSettings().contains(key) && browserRowSpacing() == spacing) {
    return;
  }
  const bool changed = browserRowSpacing() != spacing;
  QSettings().setValue(key, browserRowSpacingId(spacing));
  if (changed) {
    emit BrowserPreferencesNotifier::instance().rowSpacingChanged();
  }
}

// The active component's highlight (mitcad#2618).
namespace {
const char* const kHighlightActiveSetting = "browser/highlightActiveComponent";
} // namespace

bool browserHighlightActive() {
  return QSettings().value(QLatin1String(kHighlightActiveSetting), false).toBool();
}

void setBrowserHighlightActive(bool on) {
  const QString key = QLatin1String(kHighlightActiveSetting);
  if (QSettings().contains(key) && browserHighlightActive() == on) {
    return;
  }
  const bool changed = browserHighlightActive() != on;
  QSettings().setValue(key, on);
  if (changed) {
    emit BrowserPreferencesNotifier::instance().highlightActiveChanged();
  }
}

BrowserPreferencesNotifier& BrowserPreferencesNotifier::instance() {
  static BrowserPreferencesNotifier notifier;
  return notifier;
}

} // namespace mitcad
