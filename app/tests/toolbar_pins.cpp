// SPDX-License-Identifier: MIT
// The commands pinned to the toolbar (mitcad#1001): pin, reorder and unpin
// update the preference (toolbarPins/<TAB>) at once, per tab; the pins are
// read back as saved; an unknown command id in the preference is dropped
// with a log line, an alias becomes its command and a repeated id is kept
// once, and the settings are written again without them.
#include "framework/ToolbarPins.hpp"

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
    std::fprintf(stderr, "toolbar_pins.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kSolid = QStringLiteral("SOLID");
const QString kBuild = QStringLiteral("BUILD");
const QString kSweep = QStringLiteral("solid.sweep");
const QString kLoft = QStringLiteral("solid.loft");
const QString kExtrude = QStringLiteral("solid.extrude");

// The commands that exist: the three above, and an old id of Loft's.
QString resolve(const QString& id) {
  if (id == kSweep || id == kLoft || id == kExtrude || id == QStringLiteral("build.profile")) {
    return id;
  }
  if (id == QStringLiteral("solid.old_loft")) {
    return kLoft;
  }
  return {};
}

// The saved value of a tab's pins, as text ("" none).
QString saved(const QString& tab) {
  const QVariant value = QSettings().value(mitcad::ToolbarPins::settingsKey(tab));
  return value.metaType().id() == QMetaType::QStringList ? value.toStringList().join(QLatin1Char(','))
                                                          : value.toString();
}

bool hasKey(const QString& tab) { return QSettings().contains(mitcad::ToolbarPins::settingsKey(tab)); }

// The log lines while the pins are read (the dropped ids).
QStringList logged;
void collect(QtMsgType, const QMessageLogContext&, const QString& message) { logged.append(message); }

void pinReorderUnpinTests() {
  using namespace mitcad;
  ToolbarPins pins = ToolbarPins::load(resolve);
  CHECK(pins.pins(kSolid).isEmpty());
  CHECK(pins.tabs().isEmpty());

  // Pin: at the end, saved at once; once only.
  CHECK(pins.pin(kSolid, kSweep));
  CHECK(saved(kSolid) == kSweep);
  CHECK(pins.pin(kSolid, kLoft));
  CHECK(pins.pin(kSolid, kExtrude));
  CHECK(!pins.pin(kSolid, kLoft));
  CHECK(!pins.pin(kSolid, QString()));
  CHECK(pins.pins(kSolid) == (QStringList{kSweep, kLoft, kExtrude}));
  CHECK(saved(kSolid) == QStringLiteral("solid.sweep,solid.loft,solid.extrude"));
  CHECK(pins.isPinned(kSolid, kLoft));
  // Per tab: pinned on BUILD, not on SOLID's list or key.
  CHECK(pins.pin(kBuild, kSweep));
  CHECK(pins.isPinned(kBuild, kSweep));
  CHECK(!pins.isPinned(kBuild, kLoft));
  CHECK(saved(kBuild) == kSweep);
  CHECK(pins.tabs() == (QStringList{kBuild, kSolid}));

  // Reorder (a drag): Extrude first, then Loft last; clamped to the list;
  // nothing to do at its own place or for a command not pinned.
  CHECK(pins.move(kSolid, kExtrude, 0));
  CHECK(pins.pins(kSolid) == (QStringList{kExtrude, kSweep, kLoft}));
  CHECK(saved(kSolid) == QStringLiteral("solid.extrude,solid.sweep,solid.loft"));
  CHECK(!pins.move(kSolid, kLoft, 99));
  CHECK(pins.move(kSolid, kExtrude, 99));
  CHECK(pins.pins(kSolid) == (QStringList{kSweep, kLoft, kExtrude}));
  CHECK(pins.move(kSolid, kExtrude, -3));
  CHECK(pins.pins(kSolid) == (QStringList{kExtrude, kSweep, kLoft}));
  CHECK(!pins.move(kSolid, QStringLiteral("solid.revolve"), 0));
  CHECK(!pins.move(QStringLiteral("SKETCH"), kSweep, 0));
  CHECK(saved(kSolid) == QStringLiteral("solid.extrude,solid.sweep,solid.loft"));

  // Kept across sessions: read back as saved.
  const ToolbarPins again = ToolbarPins::load(resolve);
  CHECK(again.pins(kSolid) == (QStringList{kExtrude, kSweep, kLoft}));
  CHECK(again.pins(kBuild) == QStringList{kSweep});

  // Unpin: removed and saved; the last one takes the tab's key away.
  CHECK(pins.unpin(kSolid, kSweep));
  CHECK(!pins.unpin(kSolid, kSweep));
  CHECK(pins.pins(kSolid) == (QStringList{kExtrude, kLoft}));
  CHECK(saved(kSolid) == QStringLiteral("solid.extrude,solid.loft"));
  CHECK(pins.unpin(kSolid, kExtrude));
  CHECK(pins.unpin(kSolid, kLoft));
  CHECK(pins.pins(kSolid).isEmpty());
  CHECK(!hasKey(kSolid));
  CHECK(pins.tabs() == QStringList{kBuild});
  CHECK(pins.unpin(kBuild, kSweep));
  CHECK(!hasKey(kBuild));
  CHECK(ToolbarPins::load(resolve).tabs().isEmpty());
}

void unknownCommandTests() {
  using namespace mitcad;
  // A command that no longer exists, an alias, a repeated id and spaces.
  QSettings().setValue(ToolbarPins::settingsKey(kSolid),
                       QStringLiteral("solid.sweep, solid.gone,solid.old_loft,solid.sweep"));
  // A list, as an INI file's unquoted "a,b" is read; only unknown ids.
  QStringList build;
  build.append(QStringLiteral("build.profile"));
  build.append(QStringLiteral("build.gone"));
  QSettings().setValue(ToolbarPins::settingsKey(kBuild), build);
  QSettings().setValue(ToolbarPins::settingsKey(QStringLiteral("SKETCH")), QStringLiteral("sketch.gone"));

  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(collect);
  const ToolbarPins pins = ToolbarPins::load(resolve);
  qInstallMessageHandler(previous);

  CHECK(pins.pins(kSolid) == (QStringList{kSweep, kLoft}));
  CHECK(pins.pins(kBuild) == QStringList{QStringLiteral("build.profile")});
  CHECK(pins.pins(QStringLiteral("SKETCH")).isEmpty());
  // Dropped with a log line each, and the settings without them.
  CHECK(logged.contains(QStringLiteral("Toolbar pins: dropped solid.gone from SOLID: no such command")));
  CHECK(logged.contains(QStringLiteral("Toolbar pins: dropped build.gone from BUILD: no such command")));
  CHECK(logged.contains(QStringLiteral("Toolbar pins: dropped sketch.gone from SKETCH: no such command")));
  CHECK(saved(kSolid) == QStringLiteral("solid.sweep,solid.loft"));
  CHECK(saved(kBuild) == QStringLiteral("build.profile"));
  CHECK(!hasKey(QStringLiteral("SKETCH")));
  // Read again: nothing more to drop.
  logged.clear();
  const QtMessageHandler before = qInstallMessageHandler(collect);
  const ToolbarPins again = ToolbarPins::load(resolve);
  qInstallMessageHandler(before);
  CHECK(logged.isEmpty());
  CHECK(again.pins(kSolid) == (QStringList{kSweep, kLoft}));
  QSettings().remove(QStringLiteral("toolbarPins"));
}

} // namespace

int toolbarPinsTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("toolbar_pins"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  pinReorderUnpinTests();
  unknownCommandTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
