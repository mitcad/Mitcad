// SPDX-License-Identifier: MIT
// The timeline's History list (mitcad#2427): the preference that chooses it
// (Strip without a saved choice, History list saved and read back, the
// windows told once per change, an unknown value read as Strip, carried by
// settings export and import), and the bodies a History row's light bulb
// shows and hides: each body under the feature that made it (`F2` of
// `F2.b0`) and under the last feature that changed it when that is
// another one.
#include "UnitTests.hpp"
#include "browser/BrowserPreferences.hpp"
#include "browser/DocumentSnapshot.hpp"
#include "framework/SettingsTransfer.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QHash>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "timeline_presentation.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

const QString kKey = QStringLiteral("timeline/presentation");

// What the settings file holds now, as the next start reads it.
QString savedValue() {
  QSettings settings;
  settings.sync();
  return settings.value(kKey).toString();
}

void preferenceTests() {
  int told = 0;
  const QMetaObject::Connection counting =
      QObject::connect(&BrowserPreferencesNotifier::instance(),
                       &BrowserPreferencesNotifier::timelinePresentationChanged, [&told] { ++told; });

  // The default: the strip, nothing saved.
  CHECK(!QSettings().contains(kKey));
  CHECK(timelinePresentation() == TimelinePresentation::Strip);

  // The values' words, and back; an unknown one is the strip.
  CHECK(timelinePresentationId(TimelinePresentation::Strip) == QStringLiteral("strip"));
  CHECK(timelinePresentationId(TimelinePresentation::HistoryList) == QStringLiteral("list"));
  CHECK(timelinePresentationFromId(QStringLiteral("list")) == TimelinePresentation::HistoryList);
  CHECK(timelinePresentationFromId(QStringLiteral("strip")) == TimelinePresentation::Strip);
  CHECK(timelinePresentationFromId(QStringLiteral("tree")) == TimelinePresentation::Strip);

  // The History list saved and read back; the windows told once.
  setTimelinePresentation(TimelinePresentation::HistoryList);
  CHECK(savedValue() == QStringLiteral("list"));
  CHECK(timelinePresentation() == TimelinePresentation::HistoryList);
  CHECK(told == 1);
  setTimelinePresentation(TimelinePresentation::HistoryList); // no change
  CHECK(told == 1);

  // Restore Defaults saves the strip.
  setTimelinePresentation(TimelinePresentation::Strip);
  CHECK(savedValue() == QStringLiteral("strip"));
  CHECK(timelinePresentation() == TimelinePresentation::Strip);
  CHECK(told == 2);

  // The strip saved over no choice: written, the windows not told.
  QSettings().remove(kKey);
  setTimelinePresentation(TimelinePresentation::Strip);
  CHECK(savedValue() == QStringLiteral("strip"));
  CHECK(told == 2);

  // An unknown saved value is the strip.
  QSettings().setValue(kKey, QStringLiteral("columns"));
  CHECK(timelinePresentation() == TimelinePresentation::Strip);
  QSettings().remove(kKey);

  // Settings export and import carry it with the other preferences.
  CHECK(settingsGroupOf(kKey) == QStringLiteral("preferences"));
  QObject::disconnect(counting);
}

DocumentSnapshot::Body body(const QString& uid, const QString& lastFeature) {
  DocumentSnapshot::Body made;
  made.uid = uid;
  made.name = uid;
  made.component = QStringLiteral("C0");
  made.lastFeature = lastFeature;
  return made;
}

void bodiesTests() {
  DocumentSnapshot snapshot;
  // F1 made F1.b0, which the fillet F3 changed last; F2 made two bodies,
  // the second changed last by itself; F4 made F4.b0 and nothing changed
  // it since (no last feature told).
  for (const DocumentSnapshot::Body& made :
       {body(QStringLiteral("F1.b0"), QStringLiteral("F3")), body(QStringLiteral("F2.b1"), QStringLiteral("F2")),
        body(QStringLiteral("F2.b0"), QString()), body(QStringLiteral("F4.b0"), QString())}) {
    snapshot.bodies.insert(made.uid, made);
  }
  const QHash<QString, QStringList> bodies = snapshot.bodiesByFeature();
  CHECK(bodies.value(QStringLiteral("F1")) == QStringList{QStringLiteral("F1.b0")});
  CHECK(bodies.value(QStringLiteral("F3")) == QStringList{QStringLiteral("F1.b0")});
  // In the bodies' uid order, each once.
  CHECK((bodies.value(QStringLiteral("F2")) == QStringList{QStringLiteral("F2.b0"), QStringLiteral("F2.b1")}));
  CHECK(bodies.value(QStringLiteral("F4")) == QStringList{QStringLiteral("F4.b0")});
  CHECK(!bodies.contains(QStringLiteral("F5")));
  CHECK(bodies.size() == 4);
  // No bodies: none for any feature.
  CHECK(DocumentSnapshot().bodiesByFeature().isEmpty());
}

} // namespace

int timelinePresentationTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("timeline_presentation"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir directory;
  CHECK(directory.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());

  preferenceTests();
  bodiesTests();

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

MITCAD_UNIT_TEST(timelinePresentationTests);
