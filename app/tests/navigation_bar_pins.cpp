// SPDX-License-Identifier: MIT
// The commands pinned to the navigation bar (mitcad#1589): without the
// setting Edit Parameters (solid.parameters) is pinned by default and the
// settings are not written; pin, reorder and unpin update the preference
// (navigationBarPins) at once and are read back as saved, also when the
// default was unpinned (an empty value: no pins, the default not back); an
// unknown command id in the preference is dropped with a log line, an alias
// becomes its command and a repeated id is kept once, and the settings are
// written again without them.
#include "framework/NavigationBarPins.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QMetaType>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QVariant>
#include <QtLogging>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "navigation_bar_pins.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kParameters = QStringLiteral("solid.parameters");
const QString kMeasure = QStringLiteral("inspect.measure");
const QString kSweep = QStringLiteral("solid.sweep");
const QString kProfile = QStringLiteral("build.profile");

// The commands that exist: the ones above and an old id of Measure's; the
// sketch's Edit Parameters is the model's (the navigation bar's one button
// for both).
QString resolve(const QString& id) {
  if (id == kParameters || id == kMeasure || id == kSweep || id == kProfile) {
    return id;
  }
  if (id == QStringLiteral("sketch.parameters")) {
    return kParameters;
  }
  if (id == QStringLiteral("inspect.old_measure")) {
    return kMeasure;
  }
  return {};
}

// The saved value, as text; "<none>" without the key.
QString saved() {
  QSettings settings;
  if (!settings.contains(mitcad::NavigationBarPins::settingsKey())) {
    return QStringLiteral("<none>");
  }
  const QVariant value = settings.value(mitcad::NavigationBarPins::settingsKey());
  return value.metaType().id() == QMetaType::QStringList ? value.toStringList().join(QLatin1Char(','))
                                                          : value.toString();
}

// The log lines while the pins are read (the dropped ids).
QStringList logged;
void collect(QtMsgType, const QMessageLogContext&, const QString& message) { logged.append(message); }

void defaultTests() {
  using namespace mitcad;
  CHECK(NavigationBarPins::defaults() == QStringList{kParameters});
  // No setting: Edit Parameters pinned, nothing written.
  const NavigationBarPins fresh = NavigationBarPins::load(resolve);
  CHECK(fresh.pins() == QStringList{kParameters});
  CHECK(fresh.isPinned(kParameters));
  CHECK(saved() == QStringLiteral("<none>"));

  // The default unpinned like any other: an empty value, read back as no
  // pins (the default does not come back).
  NavigationBarPins pins = NavigationBarPins::load(resolve);
  CHECK(pins.unpin(kParameters));
  CHECK(!pins.unpin(kParameters));
  CHECK(pins.pins().isEmpty());
  CHECK(saved().isEmpty());
  CHECK(NavigationBarPins::load(resolve).pins().isEmpty());
  // Pinned again: kept as any pin.
  CHECK(pins.pin(kParameters));
  CHECK(saved() == kParameters);
  CHECK(NavigationBarPins::load(resolve).pins() == QStringList{kParameters});
  QSettings().remove(NavigationBarPins::settingsKey());
}

void pinReorderUnpinTests() {
  using namespace mitcad;
  NavigationBarPins pins = NavigationBarPins::load(resolve);
  // Pin: at the end, after the default, saved at once; once only.
  CHECK(pins.pin(kMeasure));
  CHECK(pins.pins() == (QStringList{kParameters, kMeasure}));
  CHECK(saved() == QStringLiteral("solid.parameters,inspect.measure"));
  CHECK(!pins.pin(kMeasure));
  CHECK(!pins.pin(QString()));
  CHECK(pins.pin(kSweep));
  CHECK(pins.pins() == (QStringList{kParameters, kMeasure, kSweep}));

  // Kept across sessions: read back as saved.
  CHECK(NavigationBarPins::load(resolve).pins() == (QStringList{kParameters, kMeasure, kSweep}));

  // Reorder (a drag): Measure first, then Edit Parameters last; clamped to
  // the list; nothing to do at its own place or for a command not pinned.
  CHECK(pins.move(kMeasure, 0));
  CHECK(pins.pins() == (QStringList{kMeasure, kParameters, kSweep}));
  CHECK(saved() == QStringLiteral("inspect.measure,solid.parameters,solid.sweep"));
  CHECK(pins.move(kParameters, 99));
  CHECK(pins.pins() == (QStringList{kMeasure, kSweep, kParameters}));
  CHECK(!pins.move(kParameters, 2));
  CHECK(pins.move(kParameters, -4));
  CHECK(pins.pins() == (QStringList{kParameters, kMeasure, kSweep}));
  CHECK(!pins.move(kProfile, 0));
  CHECK(NavigationBarPins::load(resolve).pins() == (QStringList{kParameters, kMeasure, kSweep}));

  // Unpin: removed and saved.
  CHECK(pins.unpin(kMeasure));
  CHECK(!pins.unpin(kMeasure));
  CHECK(!pins.unpin(kProfile));
  CHECK(pins.pins() == (QStringList{kParameters, kSweep}));
  CHECK(saved() == QStringLiteral("solid.parameters,solid.sweep"));
  CHECK(pins.unpin(kParameters));
  CHECK(pins.unpin(kSweep));
  CHECK(saved().isEmpty());
  CHECK(NavigationBarPins::load(resolve).pins().isEmpty());
  QSettings().remove(NavigationBarPins::settingsKey());
}

void unknownCommandTests() {
  using namespace mitcad;
  // A command that no longer exists, an alias, the sketch's Edit
  // Parameters beside the model's, a repeated id and spaces.
  QSettings().setValue(NavigationBarPins::settingsKey(),
                       QStringLiteral("inspect.measure, solid.gone,solid.parameters,sketch.parameters,"
                                      "inspect.old_measure,solid.sweep"));
  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(collect);
  const NavigationBarPins pins = NavigationBarPins::load(resolve);
  qInstallMessageHandler(previous);
  CHECK(pins.pins() == (QStringList{kMeasure, kParameters, kSweep}));
  CHECK(logged.contains(QStringLiteral("Navigation bar pins: dropped solid.gone: no such command")));
  CHECK(saved() == QStringLiteral("inspect.measure,solid.parameters,solid.sweep"));

  // Read again: nothing more to drop.
  logged.clear();
  const QtMessageHandler before = qInstallMessageHandler(collect);
  const NavigationBarPins again = NavigationBarPins::load(resolve);
  qInstallMessageHandler(before);
  CHECK(logged.isEmpty());
  CHECK(again.pins() == (QStringList{kMeasure, kParameters, kSweep}));

  // A list, as an INI file's unquoted "a,b" is read.
  QStringList list;
  list.append(kProfile);
  list.append(kMeasure);
  QSettings().setValue(NavigationBarPins::settingsKey(), list);
  CHECK(NavigationBarPins::load(resolve).pins() == (QStringList{kProfile, kMeasure}));

  // Only unknown ids: no pins, the default not back.
  QSettings().setValue(NavigationBarPins::settingsKey(), QStringLiteral("solid.gone"));
  logged.clear();
  const QtMessageHandler third = qInstallMessageHandler(collect);
  const NavigationBarPins none = NavigationBarPins::load(resolve);
  qInstallMessageHandler(third);
  CHECK(none.pins().isEmpty());
  CHECK(saved().isEmpty());

  // Without a resolve every id is kept as it is.
  QSettings().setValue(NavigationBarPins::settingsKey(), QStringLiteral("a.b,c.d"));
  CHECK(NavigationBarPins::load({}).pins() == (QStringList{QStringLiteral("a.b"), QStringLiteral("c.d")}));
  QSettings().remove(NavigationBarPins::settingsKey());
}

} // namespace

int navigationBarPinsTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("navigation_bar_pins"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  QSettings().remove(mitcad::NavigationBarPins::settingsKey());
  defaultTests();
  pinReorderUnpinTests();
  unknownCommandTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
