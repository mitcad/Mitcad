// SPDX-License-Identifier: MIT
// The quick-access group (mitcad#1090): the File menu's Open, Close, Save,
// Save As and Exit are registry commands with their keys, tooltips and
// search words, in every mode and during commands, Save disabled while it
// has nothing to do and saying why; the group's chosen commands are read
// from the settings (the default set without a key, none for an empty
// value, unknown ids kept), written at once and reset; the group shows the
// chosen commands this build has, in the menus' order; its tooltips name
// the command and its key, and why it is disabled, in at most 80
// characters. Whether the group shows at all (mitcad#1189) is a setting of
// its own, shown by default, told to the group when it changes, and leaves
// the choice of commands as it is.
#include "files/FileCommands.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/QuickAccess.hpp"
#include "framework/ShortcutBindings.hpp"

#include <cstdio>

#include <QAction>
#include <QCoreApplication>
#include <QKeySequence>
#include <QList>
#include <QMetaType>
#include <QObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QVariant>
#include <QWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "quick_access.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandDef;
using mitcad::QuickAccessSettings;

// The registry's File commands and their keys.
void fileCommandTests() {
  int opened = 0;
  bool modified = false;
  mitcad::FileCommandRuns runs;
  runs.open = [&opened] { ++opened; };
  runs.close = [] {};
  runs.save = [] {};
  runs.saveAs = [] {};
  runs.exit = [] {};
  runs.saveEnabled = [&modified] { return modified; };
  const std::vector<CommandDef> commands = mitcad::fileDocumentCommands(runs);
  CHECK(commands.size() == 5);
  mitcad::CommandRegistry registry;
  for (const CommandDef& def : commands) {
    registry.add(def);
  }
  QWidget owner;
  registry.createActions(&owner, [&registry](const QString& id) {
    const CommandDef* def = registry.find(id);
    if (def != nullptr && def->run) def->run();
  });
  struct Expected {
    const char* id;
    const char* name;
    QKeySequence key;
  };
  const Expected expected[] = {
      {"file.open", "Open…", QKeySequence(QKeySequence::Open)},
      {"file.close", "Close", QKeySequence(Qt::CTRL | Qt::Key_W)},
      {"file.save", "Save", QKeySequence(QKeySequence::Save)},
      {"file.save_as", "Save As…", QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S)},
      {"file.exit", "Exit", QKeySequence(QKeySequence::Quit)},
  };
  for (const Expected& entry : expected) {
    const QString id = QString::fromLatin1(entry.id);
    const CommandDef* def = registry.find(id);
    CHECK(def != nullptr);
    if (def == nullptr) continue;
    CHECK(def->name == QString::fromUtf8(entry.name));
    CHECK(def->kind == CommandDef::Kind::Action);
    CHECK(def->mode == CommandDef::Mode::Any && def->duringCommands);
    CHECK(def->tab.isEmpty()); // the File menu, no toolbar group
    CHECK(!def->tooltip.isEmpty() && def->tooltip.size() <= 80 && !def->tooltip.endsWith(QLatin1Char('.')));
    CHECK(!def->icon.isEmpty() && !def->keywords.isEmpty());
    CHECK(def->shortcut == entry.key);
    CHECK(registry.defaultShortcut(id) == entry.key);
    QAction* action = registry.action(id);
    CHECK(action != nullptr && action->objectName() == QStringLiteral("command_") + id);
    // The key on the action, Ctrl+S on Save (the platform's Quit may be none).
    if (action != nullptr && !entry.key.isEmpty()) {
      CHECK(action->shortcut() == entry.key);
      CHECK(action->toolTip().contains(entry.key.toString(QKeySequence::NativeText)));
    }
  }
  CHECK(registry.action(QStringLiteral("file.save"))->shortcut().toString(QKeySequence::PortableText) ==
        QStringLiteral("Ctrl+S"));
  // A dialog command's tooltip names it without its ellipsis: "Save As: …".
  CHECK(registry.action(QStringLiteral("file.save_as"))->toolTip().startsWith(QStringLiteral("Save As: ")));
  // The commands run what they are given.
  registry.action(QStringLiteral("file.open"))->trigger();
  CHECK(opened == 1);
  // Save has something to do only after a change, and says why not.
  const CommandDef* save = registry.find(QStringLiteral("file.save"));
  CHECK(save != nullptr && save->enabled && !save->enabled());
  modified = true;
  CHECK(save != nullptr && save->enabled && save->enabled());
  CHECK(save != nullptr && save->disabledReason && save->disabledReason() == QStringLiteral("no unsaved changes"));
  // No longer reserved keys: no conflict with the reserved ones or among
  // themselves, and no reserved binding left under their ids.
  CHECK(registry.defaultShortcutConflicts().isEmpty());
  for (const mitcad::ShortcutBinding& binding : mitcad::fixedShortcutBindings()) {
    CHECK(!binding.id.startsWith(QLatin1String("file.")));
  }
}

// The group's choice in the settings.
void settingsTests() {
  const QStringList defaults = QuickAccessSettings::defaults();
  CHECK((defaults == QStringList{QStringLiteral("file.new_design"), QStringLiteral("file.open"),
                                 QStringLiteral("file.save"), QStringLiteral("file.print"),
                                 QStringLiteral("file.design_properties"), QStringLiteral("tools.preferences")}));
  const QString key = QuickAccessSettings::settingsKey();
  CHECK(key == QStringLiteral("quickAccess/commands"));
  QSettings().remove(key);
  // No key: the defaults.
  CHECK(QuickAccessSettings::load().chosen == defaults);
  // A choice is written at once and read back.
  QuickAccessSettings settings = QuickAccessSettings::load();
  CHECK(settings.setChosen(QStringLiteral("view.look_at"), true));
  CHECK(!settings.setChosen(QStringLiteral("view.look_at"), true));
  CHECK(settings.setChosen(QStringLiteral("file.open"), false));
  CHECK(!settings.setChosen(QStringLiteral("file.open"), false));
  settings.save();
  {
    // As text, or as the list an INI file's value may read back as.
    const QVariant value = QSettings().value(key);
    const QString stored = value.metaType().id() == QMetaType::QStringList
                               ? value.toStringList().join(QLatin1Char(','))
                               : value.toString();
    CHECK(stored ==
          QStringLiteral("file.new_design,file.save,file.print,file.design_properties,tools.preferences,view.look_at"));
  }
  CHECK(QuickAccessSettings::load().chosen == settings.chosen);
  // A list (an INI file's unquoted value) with spaces and a repeated id.
  QSettings().setValue(key, QStringList{QStringLiteral("file.save"), QStringLiteral(" view.fit "),
                                        QStringLiteral("file.save")});
  CHECK((QuickAccessSettings::load().chosen == QStringList{QStringLiteral("file.save"), QStringLiteral("view.fit")}));
  // None: an empty value, not the defaults.
  QuickAccessSettings none;
  none.save();
  CHECK(QSettings().contains(key));
  CHECK(QuickAccessSettings::load().chosen.isEmpty());
  // Reset to Default: no key, the defaults again.
  QuickAccessSettings::reset();
  CHECK(!QSettings().contains(key));
  CHECK(QuickAccessSettings::load().chosen == defaults);
}

// What the group shows and its tooltips.
void shownTests() {
  // The menus' commands in their order (File, Edit, View, Tools); this
  // build without Print and Design Properties.
  const QStringList available{QStringLiteral("file.new_design"), QStringLiteral("file.open"),
                              QStringLiteral("file.close"),      QStringLiteral("file.save"),
                              QStringLiteral("edit.undo"),       QStringLiteral("view.look_at"),
                              QStringLiteral("tools.preferences")};
  CHECK((mitcad::quickAccessShown(QuickAccessSettings::defaults(), available) ==
         QStringList{QStringLiteral("file.new_design"), QStringLiteral("file.open"), QStringLiteral("file.save"),
                     QStringLiteral("tools.preferences")}));
  // Look At added: a seventh entry in the menus' order, before Preferences.
  QStringList chosen = QuickAccessSettings::defaults();
  chosen.append(QStringLiteral("view.look_at"));
  CHECK((mitcad::quickAccessShown(chosen, available) ==
         QStringList{QStringLiteral("file.new_design"), QStringLiteral("file.open"), QStringLiteral("file.save"),
                     QStringLiteral("view.look_at"), QStringLiteral("tools.preferences")}));
  CHECK(mitcad::quickAccessShown({}, available).isEmpty());

  CHECK(mitcad::quickAccessLabel(QStringLiteral("Save &As…")) == QStringLiteral("Save As…"));
  CHECK(mitcad::quickAccessLabel(QStringLiteral("Fit && Fill")) == QStringLiteral("Fit & Fill"));
  const QKeySequence save(Qt::CTRL | Qt::Key_S);
  const QString key = save.toString(QKeySequence::NativeText);
  CHECK(mitcad::quickAccessTooltip(QStringLiteral("&Save"), save) == QStringLiteral("Save (%1)").arg(key));
  CHECK(mitcad::quickAccessTooltip(QStringLiteral("Save"), save, QStringLiteral("no unsaved changes")) ==
        QStringLiteral("Save (%1): no unsaved changes").arg(key));
  CHECK(mitcad::quickAccessTooltip(QStringLiteral("Preferences…"), QKeySequence()) == QStringLiteral("Preferences"));
  const QString longTip = mitcad::quickAccessTooltip(QStringLiteral("Save"), save, QString(100, QLatin1Char('x')));
  CHECK(longTip.size() == 80 && longTip.startsWith(QStringLiteral("Save (")));
}

// Whether the group shows at all (mitcad#1189): shown without a key, saved
// at once and told to the group, the choice of commands kept while hidden.
void visibilityTests() {
  const QString key = QuickAccessSettings::visibleKey();
  CHECK(key == QStringLiteral("quickAccess/visible"));
  QSettings().remove(key);
  QSettings().remove(QuickAccessSettings::settingsKey());
  // No key: shown.
  CHECK(QuickAccessSettings::loadVisible());
  QList<bool> told;
  const QMetaObject::Connection connection =
      QObject::connect(&mitcad::QuickAccessNotifier::instance(), &mitcad::QuickAccessNotifier::visibilityChanged,
                       [&told](bool visible) { told.append(visible); });
  // Shown already: nothing saved, nothing told.
  CHECK(!QuickAccessSettings::saveVisible(true));
  CHECK(!QSettings().contains(key));
  CHECK(told.isEmpty());
  // A choice of commands, then the group hidden: the choice stays.
  QuickAccessSettings chosen = QuickAccessSettings::load();
  CHECK(chosen.setChosen(QStringLiteral("view.look_at"), true));
  chosen.save();
  CHECK(QuickAccessSettings::saveVisible(false));
  CHECK(QSettings().contains(key) && !QSettings().value(key).toBool());
  CHECK(!QuickAccessSettings::loadVisible());
  CHECK((told == QList<bool>{false}));
  CHECK(QuickAccessSettings::load().chosen == chosen.chosen);
  CHECK(QuickAccessSettings::load().chosen.contains(QStringLiteral("view.look_at")));
  // Hidden again: no change, nothing told.
  CHECK(!QuickAccessSettings::saveVisible(false));
  CHECK(told.size() == 1);
  // Reset to Default changes the commands, not whether the group shows.
  QuickAccessSettings::reset();
  CHECK(!QuickAccessSettings::loadVisible());
  // Shown again, with the commands the settings have (the defaults after
  // the reset).
  CHECK(QuickAccessSettings::saveVisible(true));
  CHECK(QuickAccessSettings::loadVisible());
  CHECK((told == QList<bool>{false, true}));
  CHECK(QuickAccessSettings::load().chosen == QuickAccessSettings::defaults());
  QObject::disconnect(connection);
  QSettings().remove(key);
}

} // namespace

int quickAccessTests() {
  // The settings of the test's own: the registry and the group read and
  // write the application's default ones.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadQuickAccessTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadQuickAccessTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  QSettings().clear();

  fileCommandTests();
  settingsTests();
  shownTests();
  visibilityTests();

  QSettings().clear();
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}
