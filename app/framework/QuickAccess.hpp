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
// empty value is none, no key the default set (defaults()). Ids of
// commands this build does not have are kept, so that a command a later
// build adds shows as soon as it is there. Plain Qt Core and QKeySequence.
struct QuickAccessSettings {
  // The commands checked, in no particular order: the group shows them in
  // the menus' order (quickAccessShown).
  QStringList chosen;

  // New, Open, Save, Print, Design Properties and Preferences; none in the
  // top bar of Windows and Linux, in either layout (setTopBarDefaults,
  // mitcad#1990, mitcad#2082), whose own buttons are the File menu, Save, the + for a
  // new design and Preferences: there the group is empty until the user
  // adds commands, and hidden while it is (QuickAccessGroup).
  static QStringList defaults();
  // The top bar's defaults from now on (the window sets it before the group
  // reads the settings); off at first.
  static void setTopBarDefaults(bool topBar);
  static bool topBarDefaults();
  static QString settingsKey();

  // The settings' choice, or the defaults when there is none.
  static QuickAccessSettings load();
  // Saves the choice at once (an empty list as none).
  void save() const;
  // Back to the defaults: the key removed, so that the default set of a
  // later build applies.
  static void reset();

  // Checks or unchecks a command; false when nothing changed.
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

// A button's tooltip (at most 80 characters, docs/development.md): the
// command's name without its ellipsis and mnemonic, its key, and why it is
// disabled when it is: "Save (Ctrl+S)", "Save (Ctrl+S): no unsaved changes".
QString quickAccessTooltip(const QString& name, const QKeySequence& key, const QString& reason = QString());

// A menu entry's name as the menus show it, without the mnemonic's "&".
QString quickAccessLabel(const QString& text);

} // namespace mitcad
