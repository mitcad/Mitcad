// SPDX-License-Identifier: MIT
#pragma once

#include <QKeySequence>
#include <QObject>
#include <QString>
#include <QStringList>

namespace mitcad {

// The quick-access group of the title row (Floating) and the Docked toolbar
// (mitcad#1090, app/COMMANDS.md "The quick-access group"): buttons of the
// File, Edit, View and Tools menus' commands that the user checked in the
// group's menu. Which commands they are is a preference, kept in the
// settings: "quickAccess/commands", the command ids joined by commas; an
// empty value is none, no key the default set (defaults()); the top bar of
// Windows and Linux keeps its left group's ordered list in a key of its own
// (itemsKey, mitcad#2129). Ids of
// commands this build does not have are kept, so that a command a later
// build adds shows as soon as it is there. Plain Qt Core and QKeySequence.
struct QuickAccessSettings {
  // The commands checked, in no particular order: the group shows them in
  // the menus' order (quickAccessShown).
  QStringList chosen;

  // New, Open, Save, Print, Design Properties and Preferences; in the top
  // bar of Windows and Linux, in either layout (setTopBarDefaults), the left
  // group's default items instead (topBarItems, mitcad#2129).
  static QStringList defaults();
  // The top bar's left group from now on (the window sets it before the
  // group reads the settings); off at first. Then `chosen` is the group's
  // ordered list of items (itemsKey), not the old choice of commands.
  static void setTopBarDefaults(bool topBar);
  static bool topBarDefaults();
  // The choice of commands outside the top bar (the macOS toolbar), and the
  // top bar's before mitcad#2129 (migrated once, load).
  static QString settingsKey();

  // The top bar's left group (mitcad#2129): the ☰ and Home, which are fixed
  // and in no list, then these items in this order, each of them removable:
  // the File menu (fileMenuId), New Design, Open, Save, Design Properties,
  // Print, Undo and Redo. The commands the user adds follow them.
  static QStringList topBarItems();
  // The File menu's own id among the items: no command has it.
  static QString fileMenuId();
  // Whether an id is one of topBarItems (it stays while the user hides the
  // added commands).
  static bool isTopBarItem(const QString& id);
  // The group's ordered list of item ids ("quickAccess/items"): written at
  // each change; no key is the default order (Reset to Default removes it).
  static QString itemsKey();
  // Set once the old choice was migrated into the list
  // ("quickAccess/itemsMigrated"), so that a reset does not add it again.
  static QString migratedKey();
  // The list a migration makes: `defaults`, then the ids of `old` that are
  // not in it already, in their order.
  static QStringList migratedItems(const QStringList& defaults, const QStringList& old);
  // `items` with a default item put back at its default place: before the
  // first entry that is a later default item or an added command.
  static QStringList withDefaultItem(const QStringList& items, const QString& id);

  // The settings' choice, or the defaults when there is none. In the top
  // bar the ordered list; without its key, while the old choice
  // (settingsKey) holds commands and was not migrated yet, the defaults
  // with those commands after them, written at once (logged "Quick access
  // migrated: N commands").
  static QuickAccessSettings load();
  // Saves the choice at once (an empty list as none).
  void save() const;
  // Back to the defaults: the key removed, so that the default set of a
  // later build applies.
  static void reset();

  // Checks or unchecks a command; false when nothing changed. In the top
  // bar an added item goes to the end, a default item back to its default
  // place (withDefaultItem).
  bool setChosen(const QString& id, bool on);

  // Whether the settings' choice has the command.
  static bool isChosen(const QString& id);
  // Adds a command to the settings' choice or removes it (a command's Add
  // to Quick Access and Remove from Quick Access, the group's menu), saved
  // at once, logged ("Quick access added view.look_at") and told to every
  // group (QuickAccessNotifier::commandsChanged); false when nothing
  // changed.
  static bool choose(const QString& id, bool on);

  // Whether the group shows at all (mitcad#1189, Preferences › Display ›
  // Show quick-access toolbar, and the group's Hide Quick-Access Toolbar):
  // "quickAccess/visible", shown without a key. Hidden, the choice of
  // commands stays as it is, so that showing it again brings back the
  // same buttons.
  static QString visibleKey();
  static bool loadVisible();
  // Saves it at once and tells QuickAccessNotifier; false when it was
  // already so.
  static bool saveVisible(bool visible);
};

// Tells the group in either layout, the AppKit toolbar's group and an open
// Settings window that the group was shown or hidden (saveVisible), from
// Preferences or the group's own menu, and the groups that a command was
// added or removed (choose). Plain Qt Core.
class QuickAccessNotifier : public QObject {
  Q_OBJECT

public:
  static QuickAccessNotifier& instance();

signals:
  void visibilityChanged(bool visible);
  // The choice of commands changed (QuickAccessSettings::choose).
  void commandsChanged();
};

// The commands the group shows: those of `chosen` that are in `available`
// (the menus' commands this build has), in the order of `available`.
QStringList quickAccessShown(const QStringList& chosen, const QStringList& available);

// The top bar's left group's items (mitcad#2129): those of `items` that are
// in `available` (the commands this build has, and the File menu's id), in
// the list's order, without `rowCommands` (what the bar shows itself, such
// as Preferences: not there twice) and without repeats.
QStringList quickAccessItems(const QStringList& items, const QStringList& available, const QStringList& rowCommands);

// A button's tooltip (at most 80 characters, docs/development.md): the
// command's name without its ellipsis and mnemonic, its key, and why it is
// disabled when it is: "Save (Ctrl+S)", "Save (Ctrl+S): no unsaved changes".
QString quickAccessTooltip(const QString& name, const QKeySequence& key, const QString& reason = QString());

// A menu entry's name as the menus show it, without the mnemonic's "&".
QString quickAccessLabel(const QString& text);

} // namespace mitcad
