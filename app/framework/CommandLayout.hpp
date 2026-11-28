// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QJsonObject>
#include <QMap>
#include <QObject>
#include <QString>
#include <QStringList>

namespace mitcad {

// Where the commands sit (mitcad#1951, app/COMMANDS.md "The layout file"):
// the toolbar's tabs, their groups and the groups' menus, and the menu
// bar's menus, as data. The shipped default is one JSON file
// (app/resources/layout/default.json, `:/layout/default.json`); the user's
// changes are a layer over it (LayoutUserLayer), kept in the settings as a
// difference, so that commands a later version adds still appear. Nothing
// here is visible text: tabs, groups, menus and submenus are keys whose
// captions the code translates (displayName, the menus' own). Plain Qt
// Core.

// One entry of a place: a command (with or without a button of its own), a
// separator, a submenu, or a key of entries the code provides that are not
// commands (the Select group's filters: an extra).
struct LayoutEntry {
  enum class Kind { Command, Separator, Submenu, Extra };
  Kind kind = Kind::Command;
  // The command's id, the submenu's key ("Pattern", "PROJECT") or the
  // extra's key ("solid.selection").
  QString id;
  // A command: a button of its own in the toolbar group (in a submenu: the
  // submenu's split button, the first such command).
  bool button = false;
  // A command's second place (mitcad#1975, a named repeat: Create Sketch
  // on the Surface tab, Material in Solid › Modify): shown there as
  // anywhere, but never the command's home while it has a place without
  // this mark.
  bool repeat = false;
  // A submenu of the menu bar: its icon's name (app/icons without .svg).
  QString icon;
  // A submenu's entries.
  std::vector<LayoutEntry> entries;

  static LayoutEntry command(const QString& id, bool button = false);
  static LayoutEntry separator();
};

// A toolbar group and a tab of them.
struct LayoutGroup {
  QString key; // "CREATE", "SHEET PARTS"
  std::vector<LayoutEntry> entries;
};
struct LayoutTab {
  QString key; // "SOLID"
  std::vector<LayoutGroup> groups;
};

// A command's first place on a toolbar tab, in the tabs' order, that is no
// repeat (LayoutEntry::repeat; the first repeat when it has only those):
// its "home", by which a pin goes to its group and the window tells a BUILD
// command from a SOLID one. Empty for a command on no tab.
struct LayoutPlace {
  QString tab;
  QString group;
  bool isEmpty() const { return tab.isEmpty(); }
};

class CommandLayout {
public:
  // The tabs in order, and the menu bar's menus (each a Submenu entry: its
  // key "FILE" and its entries).
  std::vector<LayoutTab> tabs;
  std::vector<LayoutEntry> menus;
  // The commands deliberately on no toolbar and in no menu of the layout:
  // reached by the command search, context menus, shortcuts, or placed by
  // the code that owns them (a panel's toggle in the View menu).
  QStringList unplaced;
  // Commands that a build or a run may lack (the render worker's, the test
  // hooks'): left out without a log line when they are not registered.
  QStringList optional;

  // The layout of a JSON document; on a malformed one (not the format, a
  // wrong type) an empty layout and `error` in words.
  static CommandLayout fromJson(const QJsonObject& json, QString* error);
  QJsonObject toJson() const;

  const LayoutTab* tab(const QString& key) const;
  const LayoutGroup* group(const QString& tab, const QString& group) const;
  const LayoutEntry* menu(const QString& key) const;
  // The entries of a place: "SOLID/CREATE", "SOLID/CREATE/Pattern",
  // "menu/FILE", "menu/FILE/PROJECT"; null for none.
  const std::vector<LayoutEntry>* entries(const QString& place) const;
  std::vector<LayoutEntry>* entries(const QString& place);
  // The places a layout has, in order (groups, their submenus, the menus
  // and theirs).
  QStringList places() const;

  // The command's first place on a tab (its group's, also from a
  // submenu) that is no repeat, else its first repeat; empty when on no
  // tab.
  LayoutPlace home(const QString& id) const;
  // Every place that holds the command, the home first: the tabs' places
  // where it is no repeat, then its repeats, then the menus, each in the
  // layout's order, as
  // entries() takes them ("SOLID/CREATE", "SOLID/CREATE/Pattern",
  // "menu/TOOLS"); empty for a command in none (mitcad#1983: the command
  // search's place line).
  QStringList placesOf(const QString& id) const;
  // Every command id of the tabs and the menus, once each, in order.
  QStringList placedCommands() const;
  // Every toolbar group's and submenu's key, for the captions to translate.
  QStringList groupKeys() const;
  QStringList submenuKeys() const;
  QStringList extraKeys() const;

  // The layout with each command id as `canonical` gives it: an alias
  // becomes its command, an empty answer (no such command) drops the
  // entry with a log line ("Layout: dropped solid.gone from SOLID ›
  // MODIFY: no such command"), silently for an `optional` command.
  CommandLayout resolved(const std::function<QString(const QString& id)>& canonical) const;

  // The place that holds the command's own entry at `place` or in one of
  // its submenus ("SOLID/CREATE" or "SOLID/CREATE/Pattern"), the place
  // itself first; empty when neither has it (mitcad#1991: a group menu's
  // row of a submenu of one command is the group's row).
  QString commandPlace(const QString& place, const QString& id) const;
  // Whether the command's entry at the place (no submenu of it) has a
  // button of its own.
  bool hasButton(const QString& place, const QString& id) const;
};

// The user's changes, a difference from the default (app/COMMANDS.md "The
// user layer"): commands hidden from a place, added to one (after a given
// command, else at its end), their buttons switched on or off, an order of
// some commands of a place, and a tab's groups moved (mitcad#1998:
// "group", its place the tab's key, its id the group and `after` the group
// it follows, empty: first; only the moved groups are recorded, so a group
// a later version adds keeps the default's place). A move of a command is
// a hide and an add. Kept in the settings ("commandLayout/user") as JSON.
struct LayoutChange {
  enum class Op { Hide, Add, Button, Order, Group };
  Op op = Op::Add;
  QString place; // "SOLID/CREATE", "menu/FILE"; Group: the tab, "SOLID"
  QString id;     // the command; Group: the group, "INSPECT"
  QString after;  // Add: the command it goes after, empty: at the end;
                  // Group: the group it follows, empty: first
  bool on = true; // Button
  QStringList ids; // Order
};

// A command's button at a place (mitcad#1991): "SOLID/MODIFY" and
// "solid.shell", or a submenu's split button at "SOLID/CREATE/Pattern".
struct LayoutButton {
  QString place;
  QString id;
};

class LayoutUserLayer {
public:
  std::vector<LayoutChange> changes;
  // The commands pinned to the toolbar (mitcad#1001, mitcad#1084), per tab
  // in the order pinned: each a button in its home group after the group's
  // own buttons (ToolbarPins, the Ribbon). Kept here since mitcad#1951; an
  // earlier version's "toolbarPins/<TAB>" settings move in at the start.
  QMap<QString, QStringList> pins;
  // The model tabs the user hid (mitcad#1975, the tabs' menu: a right-click
  // on a tab or the + after the last one), by key ("SURFACE"); every tab
  // is shown when none is listed. Start and the modes' tabs are never
  // hidden (Ribbon::hideableTabs).
  QStringList hiddenTabs;

  bool isEmpty() const { return changes.empty() && pins.isEmpty() && hiddenTabs.isEmpty(); }
  // An empty layer and `error` in words for a malformed document.
  static LayoutUserLayer fromJson(const QJsonObject& json, QString* error);
  QJsonObject toJson() const;

  // The convenience changes; each replaces an earlier change of the same
  // command and place that it makes moot.
  void hide(const QString& place, const QString& id);
  void add(const QString& place, const QString& id, const QString& after = QString());
  void move(const QString& from, const QString& to, const QString& id, const QString& after = QString());
  void setButton(const QString& place, const QString& id, bool on);
  void setOrder(const QString& place, const QStringList& ids);
  // The command's button at the place as the default has it again (its
  // Button change dropped).
  void resetButton(const QString& place, const QString& id);
  // A tab hidden or shown again.
  void setTabHidden(const QString& tab, bool hidden);
  // Reset Toolbar of one tab (mitcad#1975): every change of a place on the
  // tab ("SOLID/CREATE", its submenus, the order of its groups) and the
  // tab's pins dropped; the other tabs, the menus and the hidden tabs stay
  // as they are.
  void resetTab(const QString& tab);

  // Remove from Toolbar and Show on Toolbar (mitcad#1991): the command's
  // button at the place switched as `on` says, kept as a change only where
  // that differs from `defaults` (else the change is dropped).
  void setButtonShown(const CommandLayout& defaults, const QString& place, const QString& id, bool on);
  // Whether the user removed the button `defaults` gives the command at the
  // place.
  bool buttonRemoved(const CommandLayout& defaults, const QString& place, const QString& id) const;
  // The buttons the user removed in a group ("SOLID/MODIFY") and its
  // submenus, in the order removed: what the group caption's Show on
  // Toolbar lists.
  std::vector<LayoutButton> removedButtons(const CommandLayout& defaults, const QString& group) const;
  // The order of a tab's groups (mitcad#1991, mitcad#1998): `groups` every
  // group key of the tab in the order wanted, `defaultOrder` the
  // default's. Kept as the fewest group moves that turn the default's order
  // into it (the tab's earlier moves replaced): the groups that keep their
  // order among themselves are not recorded, and of two choices the one
  // that records `moved` (the group the user moved) is taken. The default's
  // order leaves no change.
  void setGroupOrder(const QString& tab, const QStringList& groups, const QStringList& defaultOrder,
                     const QString& moved = QString());
  // `order` with `group` and its neighbour among `shown` (the groups the
  // toolbar shows, in order) swapped: `step` -1 one place to the left, +1
  // to the right; empty when there is no neighbour that way.
  static QStringList movedGroup(const QStringList& order, const QStringList& shown, const QString& group, int step);

  // `base` with the changes applied, in order; a change of a place the
  // layout does not have is skipped with a log line.
  CommandLayout applied(const CommandLayout& base) const;
};

// The application's layout, read once (mitcad#1951): the default file and
// the user's layer at the start, and again only when the layer changes
// (edited, imported with the settings, restored). Opening a menu, a
// group's menu, another tab or the command search uses what is held here
// and reads no file. Every window's toolbar and menu bar follow changed().
class CommandLayoutStore : public QObject {
  Q_OBJECT

public:
  // `defaultPath`: the default layout file (the resource unless a test
  // gives another); the user's layer from the settings.
  explicit CommandLayoutStore(const QString& defaultPath = QString(), QObject* parent = nullptr);

  // The application's (made at its first use, the start).
  static CommandLayoutStore& instance();
  // How many times a layout was read and validated since the program
  // started, by every store: the start's, then once per change.
  static int loadCount();

  const CommandLayout& defaults() const { return m_defaults; }
  // The default with the user's layer.
  const CommandLayout& layout() const { return m_layout; }
  const LayoutUserLayer& userLayer() const { return m_user; }
  // The notice in words when the user's layer could not be read and the
  // default is used (empty when it was read).
  QString notice() const { return m_notice; }

  // A new layer, saved and applied; changed() follows.
  void setUserLayer(const LayoutUserLayer& layer);
  // The user's layer removed (Preferences › Restore Default Layout).
  void restoreDefault();
  // A tab's pinned commands (LayoutUserLayer::pins), saved at once; the
  // layout is not read again and changed() is not emitted (the toolbar
  // shows its pins itself). An empty list removes the tab's pins.
  QStringList pins(const QString& tab) const { return m_user.pins.value(tab); }
  QStringList pinTabs() const { return m_user.pins.keys(); }
  void setPins(const QString& tab, const QStringList& ids);

  // The layer read again from the settings (Import Settings); the
  // IfChanged one only when the settings hold another than was read or
  // saved last, or an earlier version's toolbar pins (false: nothing to
  // do).
  void reload();
  bool reloadIfChanged();

  // The settings' key of the user's layer, and the group an earlier
  // version kept the toolbar's pins in ("toolbarPins/<TAB>", moved into the
  // layer at the start: "Layout: moved N toolbar pins into the user
  // layer").
  static QString settingsKey();
  static QString oldPinsGroup();

signals:
  void changed();

private:
  void load();
  void apply();
  void save();
  // An earlier version's toolbar pins moved into m_user; true when there
  // were any (their keys are gone then).
  bool migratePins();

  QString m_defaultPath;
  CommandLayout m_defaults;
  LayoutUserLayer m_user;
  CommandLayout m_layout;
  QString m_notice;
  QString m_stored; // the user's layer as the settings had it, read or saved last
};

// The application's effective layout (CommandLayoutStore::instance()).
const CommandLayout& commandLayout();
// A command's home in the default layout: what kind of command it is (a
// BUILD command, a CONSTRUCT one) does not change when the user moves it.
LayoutPlace defaultHome(const QString& id);

} // namespace mitcad
