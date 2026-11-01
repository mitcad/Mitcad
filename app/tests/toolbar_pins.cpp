// SPDX-License-Identifier: MIT
// The commands pinned to the toolbar (mitcad#1001): pin, reorder and unpin
// update the preference (toolbarPins/<TAB>) at once, per tab; the pins are
// read back as saved; an unknown command id in the preference is dropped
// with a log line, an alias becomes its command and a repeated id is kept
// once, and the settings are written again without them. Each pin is in its
// own group (mitcad#1084): a Modify command pinned on the Solid tab is one
// of the MODIFY group's pins, not CREATE's; a group's pins reorder among
// themselves; an old per-tab pin list is read into the groups, a command
// kept under another tab than its own going to its own; unpinned, it is
// gone from its group.
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
const QString kMake = QStringLiteral("MAKE");
const QString kCreate = QStringLiteral("CREATE");
const QString kModify = QStringLiteral("MODIFY");
const QString kSweep = QStringLiteral("solid.sweep");
const QString kLoft = QStringLiteral("solid.loft");
const QString kExtrude = QStringLiteral("solid.extrude");
const QString kDraft = QStringLiteral("solid.draft");
const QString kShell = QStringLiteral("solid.shell");
// A command of the BUILD tab.
const QString kProfile = QStringLiteral("build.profile");
// A command whose group is on another tab than the one it was kept under
// (as commands that moved to MAKE, mitcad#1049).
const QString kMaterial = QStringLiteral("make.material");

// The commands that exist, with their tab and group: the ones above, a
// BUILD command and an old id of Loft's.
mitcad::ToolbarPins::Command resolve(const QString& id) {
  using Command = mitcad::ToolbarPins::Command;
  if (id == kSweep || id == kLoft || id == kExtrude) {
    return Command{id, kSolid, kCreate};
  }
  if (id == kDraft || id == kShell) {
    return Command{id, kSolid, kModify};
  }
  if (id == kMaterial) {
    return Command{id, kMake, QStringLiteral("MATERIALS")};
  }
  if (id == kProfile) {
    return Command{id, kBuild, QStringLiteral("PROFILE")};
  }
  if (id == QStringLiteral("solid.old_loft")) {
    return Command{kLoft, kSolid, kCreate};
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
  // Per tab: a BUILD command pinned on BUILD, not on SOLID's list or key.
  CHECK(pins.pin(kBuild, kProfile));
  CHECK(pins.isPinned(kBuild, kProfile));
  CHECK(!pins.isPinned(kBuild, kLoft));
  CHECK(saved(kBuild) == kProfile);
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
  CHECK(again.pins(kBuild) == QStringList{kProfile});

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
  CHECK(pins.unpin(kBuild, kProfile));
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

void groupTests() {
  using namespace mitcad;
  ToolbarPins pins = ToolbarPins::load(resolve);
  // A Modify command pinned on the Solid tab: one of MODIFY's pins, the
  // button the Ribbon adds after the group's own; none in CREATE.
  CHECK(pins.pin(kSolid, kDraft));
  CHECK(pins.pins(kSolid, kModify) == QStringList{kDraft});
  CHECK(pins.pins(kSolid, kCreate).isEmpty());
  CHECK(saved(kSolid) == kDraft);
  // More in both groups: each group's in the tab's order.
  CHECK(pins.pin(kSolid, kSweep));
  CHECK(pins.pin(kSolid, kShell));
  CHECK(pins.pin(kSolid, kLoft));
  CHECK(pins.pins(kSolid) == (QStringList{kDraft, kSweep, kShell, kLoft}));
  CHECK(pins.pins(kSolid, kCreate) == (QStringList{kSweep, kLoft}));
  CHECK(pins.pins(kSolid, kModify) == (QStringList{kDraft, kShell}));
  // A group of another tab has none of them.
  CHECK(pins.pins(kBuild, kModify).isEmpty());

  // A drag among the group's pinned buttons: Loft first in CREATE, the
  // MODIFY pins where they were; then last again (clamped); nothing to do
  // at its own place, for a command not pinned or alone in its group.
  CHECK(pins.moveInGroup(kSolid, kLoft, 0));
  CHECK(pins.pins(kSolid, kCreate) == (QStringList{kLoft, kSweep}));
  CHECK(pins.pins(kSolid, kModify) == (QStringList{kDraft, kShell}));
  CHECK(saved(kSolid) == QStringLiteral("solid.draft,solid.loft,solid.sweep,solid.shell"));
  CHECK(pins.moveInGroup(kSolid, kLoft, 9));
  CHECK(pins.pins(kSolid, kCreate) == (QStringList{kSweep, kLoft}));
  CHECK(!pins.moveInGroup(kSolid, kLoft, 1));
  CHECK(!pins.moveInGroup(kSolid, kExtrude, 0));
  CHECK(pins.unpin(kSolid, kShell));
  CHECK(!pins.moveInGroup(kSolid, kDraft, 0));

  // Unpinned: gone from its group, the other group as it was.
  CHECK(pins.unpin(kSolid, kDraft));
  CHECK(pins.pins(kSolid, kModify).isEmpty());
  CHECK(pins.pins(kSolid, kCreate) == (QStringList{kSweep, kLoft}));
  CHECK(saved(kSolid) == QStringLiteral("solid.sweep,solid.loft"));
  CHECK(pins.unpin(kSolid, kSweep));
  CHECK(pins.unpin(kSolid, kLoft));
  CHECK(!hasKey(kSolid));

  // Without the commands' places (not loaded) no group has pins.
  ToolbarPins unplaced;
  CHECK(unplaced.pin(kSolid, kDraft));
  CHECK(unplaced.pins(kSolid, kModify).isEmpty());
  CHECK(unplaced.unpin(kSolid, kDraft));
  QSettings().remove(QStringLiteral("toolbarPins"));
}

void oldPinsTests() {
  using namespace mitcad;
  // A per-tab list of an earlier version (the Pinned group's, mitcad#1001):
  // CREATE's, MODIFY's and a command whose group is on the Make tab now.
  QSettings().setValue(ToolbarPins::settingsKey(kSolid),
                       QStringLiteral("solid.sweep,solid.draft,make.material,solid.loft"));

  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(collect);
  ToolbarPins pins = ToolbarPins::load(resolve);
  qInstallMessageHandler(previous);

  // Each in its command's group.
  CHECK(pins.pins(kSolid, kCreate) == (QStringList{kSweep, kLoft}));
  CHECK(pins.pins(kSolid, kModify) == QStringList{kDraft});
  CHECK(pins.pins(kSolid) == (QStringList{kSweep, kDraft, kLoft}));
  CHECK(pins.pins(kMake, QStringLiteral("MATERIALS")) == QStringList{kMaterial});
  CHECK(pins.isPinned(kMake, kMaterial));
  CHECK(!pins.isPinned(kSolid, kMaterial));
  CHECK(logged.contains(QStringLiteral("Toolbar pins: moved make.material from SOLID to MAKE")));
  CHECK(saved(kSolid) == QStringLiteral("solid.sweep,solid.draft,solid.loft"));
  CHECK(saved(kMake) == kMaterial);

  // Read again: nothing more to move.
  logged.clear();
  const QtMessageHandler before = qInstallMessageHandler(collect);
  const ToolbarPins again = ToolbarPins::load(resolve);
  qInstallMessageHandler(before);
  CHECK(logged.isEmpty());
  CHECK(again.pins(kSolid, kModify) == QStringList{kDraft});
  CHECK(again.pins(kMake) == QStringList{kMaterial});

  // A moved command its own tab has already is kept there once; one moved
  // to a tab whose own pins were all dropped is that tab's pin; the tab it
  // was kept under loses its key.
  QSettings().setValue(ToolbarPins::settingsKey(kSolid), QStringLiteral("make.material,build.profile"));
  QSettings().setValue(ToolbarPins::settingsKey(kMake), QStringLiteral("make.material"));
  QSettings().setValue(ToolbarPins::settingsKey(kBuild), QStringLiteral("build.gone"));
  logged.clear();
  const QtMessageHandler third = qInstallMessageHandler(collect);
  const ToolbarPins moved = ToolbarPins::load(resolve);
  qInstallMessageHandler(third);
  CHECK(moved.pins(kMake) == QStringList{kMaterial});
  CHECK(moved.pins(kBuild) == QStringList{QStringLiteral("build.profile")});
  CHECK(moved.pins(kSolid).isEmpty());
  CHECK(!hasKey(kSolid));
  CHECK(saved(kBuild) == QStringLiteral("build.profile"));
  CHECK(logged.contains(QStringLiteral("Toolbar pins: moved build.profile from SOLID to BUILD")));

  // Unpinned: gone from its group and the settings.
  CHECK(pins.unpin(kSolid, kDraft));
  CHECK(pins.pins(kSolid, kModify).isEmpty());
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
  groupTests();
  oldPinsTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
