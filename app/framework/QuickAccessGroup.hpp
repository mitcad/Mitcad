// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QHash>
#include <QList>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QStringList>

#include "QuickAccess.hpp"
#include "Ribbon.hpp"

class QContextMenuEvent;
class QHBoxLayout;
class QMenu;
class QTimer;
class QToolButton;

namespace mitcad {

class CommandRegistry;

// The quick-access group (mitcad#1090, app/COMMANDS.md "The quick-access
// group"): a capsule of buttons for the commands the user checked in its
// menu or with a command's Add to Quick Access (QuickAccessSettings; by
// default New, Open, Save, Print, Design Properties and Preferences), in the
// menus' order, then the commands no menu has in the order they were added,
// and a ⋮ button. The top bar of Windows and Linux, in the Floating and the
// Docked layout, has it as its one left group (setTopBar, mitcad#2129): ☰
// and Home, then its ordered items (the File menu, New Design, Open, Save,
// Design Properties, Print, Undo and Redo by default) and the commands the
// user added, flat on the bar, without the commands the bar shows itself
// (TitleBar). The macOS Docked toolbar's tab row has it at its right end,
// flat, with Undo and Redo first in it (setLeadingWidgets, mitcad#2082;
// Ribbon::addTabRowEndWidget), where it takes no width from the groups. A
// button is the command's action
// (icon, enabled state, its run); its tooltip names the command, its key
// and, while it is disabled, why. The ⋮ (and a right-click on the group)
// opens its menu: the File, Edit, View and Tools menus' commands as checks
// (submenus as submenus), Reset to Default, Hide Quick-Access Toolbar
// (mitcad#1189), and, while the group is folded, its commands first. A
// command this build does not register is left out. The group shows only
// while QuickAccessSettings::loadVisible() says so (applyVisible; in the top
// bar only its added commands hide).
class QuickAccessGroup : public CapsuleFrame {
  Q_OBJECT

public:
  struct Hooks {
    // The menus whose commands the group offers, in the menu bar's order
    // (File, Edit, View, Tools).
    std::function<QList<QMenu*>()> menus;
    // Why a command's action is disabled now (MainWindow's mode, the
    // read-only window, the command's own disabledReason).
    std::function<QString(const QString& id)> disabledReason;
    // The top bar's left group (setTopBar, mitcad#2129): why an item is not
    // shown now, empty while it is (Print: "nothing to print" while no
    // drawing or rendered view is open). Asked again whenever the item's
    // action changes.
    std::function<QString(const QString& id)> hiddenReason;
  };

  QuickAccessGroup(CommandRegistry& registry, Hooks hooks, QWidget* parent = nullptr);

  // Reads the settings and the menus again and makes the buttons.
  void rebuild();
  // The commands shown, in order.
  const QStringList& shownIds() const { return m_shown; }

  // Shown or hidden as the setting says (mitcad#1189), once the group has
  // its place in a row; then again whenever the setting changes
  // (QuickAccessNotifier). Logs "Quick access toolbar shown" or "hidden".
  void applyVisible();
  // What the setting asked the last time applyVisible ran.
  bool isWanted() const { return m_wanted; }
  // Hidden, its ⋮ too, while it has no button of its own (the Floating top
  // bar, mitcad#1990: empty until the user adds a command, and the bar's
  // own Save and Preferences are no buttons of the group's). Logs
  // "Quick access group hidden: no commands" or "... shown: it has
  // commands" when that changes.
  void setHiddenWhileEmpty(bool hidden);

  // Folded, only the ⋮ shows, whose menu then lists the commands first
  // (TitleBar folds the group when its buttons do not fit at its place).
  void setFolded(bool folded);
  bool isFolded() const { return m_folded; }
  // The group's width with every button shown.
  int unfoldedWidth() const;
  // At most `height` high, with smaller icons (the Docked tab row's height,
  // so that the toolbar does not grow).
  void setCompact(int height);
  // The buttons' icon size (setCompact).
  int iconSize() const { return m_iconSize; }
  // Widgets first in the group, before its commands, a hairline after them
  // (the Docked tab row's ☰, Undo and Redo, mitcad#2082, as the Floating
  // top bar has them before its commands). They stay while the user hides
  // the group's commands (Hide Quick-Access Toolbar): then only its own
  // buttons, its hairlines and its ⋮ hide. Logged with its places as
  // "Quick access leading ribbonToolbarUndo at x,y: inside the group,
  // first".
  void setLeadingWidgets(const QList<QWidget*>& widgets);
  // The commands the row shows with buttons of its own (the Floating title
  // row's Save and Preferences, mitcad#1990): no button of the group's
  // for them, so that none is there twice. They stay chosen and in the
  // "Quick access: " line, their states are logged as the others', and
  // their places are those buttons' ("Quick access file.save at x,y").
  void setRowCommands(const QHash<QString, QWidget*>& commands);

  // The top bar's one left group of Windows and Linux (mitcad#2129), in
  // both layouts; call it once, before setLeadingWidgets. Its items are the
  // ordered list of QuickAccessSettings (topBarItems by default: `fileMenu`
  // as a File ˅ text button, "titleFileMenu", then New Design, Open, Save,
  // Design Properties, Print, Undo and Redo), in that order, then the
  // commands the user added in the order added; the leading widgets (☰ and
  // Home) are fixed. An item's right-click offers Remove from Quick Access;
  // the ⋮ (the add button) lists the File menu and the menus' commands as
  // checks. Hide Quick-Access Toolbar hides only the added commands. An
  // item a hook says is not shown now (Hooks::hiddenReason: Print without
  // anything to print) has no button in sight. The group shows while
  // empty too, and folds its items from the end (fitWidth).
  void setTopBar(QMenu* fileMenu);
  bool isTopBar() const { return m_topBar; }
  // The File ˅ button (setTopBar); null while the File menu is not an item.
  QToolButton* fileMenuButton() const;
  // setTopBar: as many items from the start as fit in `width` (the leading
  // widgets and the ⋮ always), the rest folded into the ⋮'s menu, first in
  // it. Logs "Quick access folded: file.print, edit.undo, edit.redo" or
  // "Quick access unfolded" when that changes.
  void fitWidth(int width);
  // setTopBar: the items folded now, in order.
  const QStringList& foldedIds() const { return m_foldedIds; }

  // Opens the group's menu at a place on the screen.
  void openMenu(const QPoint& global);

  // The rules the AppKit toolbar's group shares (mitcad#1113): the
  // registry commands of `menus`, in their order, the default commands the
  // registry has but no menu at the end; a command's button tooltip, with
  // why it is disabled when `enabled` is false.
  static QStringList availableIds(const CommandRegistry& registry, const QList<QMenu*>& menus);
  static QString tooltipOf(const CommandRegistry& registry, const QString& id, bool enabled,
                           const std::function<QString(const QString&)>& disabledReason);

signals:
  // Buttons came or went, or the group folded or unfolded.
  void contentsChanged();

protected:
  // setTopBar: an item's right-click (its context menu event, or for a
  // disabled item, which gets none from Qt, its right button press) opens
  // its menu.
  bool eventFilter(QObject* watched, QEvent* event) override;
  void contextMenuEvent(QContextMenuEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // The registry commands of the menus, in their order; the default
  // commands this build registers but no menu has, and then the chosen
  // ones no menu has (added from a toolbar or the search), at the end.
  QStringList availableIds() const;
  // Hidden for having no buttons (setHiddenWhileEmpty).
  bool isEmptyHidden() const;
  // setTopBar: whether an item has its button now: not hidden with the
  // added commands (applyVisible) and not hidden by Hooks::hiddenReason.
  bool isPresent(const QString& id) const;
  // setTopBar: Hooks::hiddenReason asked again for every item, logged
  // ("Quick access file.print hidden: nothing to print", "... shown") when
  // it changed; the buttons follow.
  void updatePresence();
  // The group's width with its leading widgets, the first `count` present
  // items and its ⋮.
  int widthWith(int count) const;
  // setTopBar: an item's right-click menu (Remove from Quick Access).
  void openItemMenu(const QString& id, const QPoint& global);
  // Its own buttons, hairlines and ⋮ shown or hidden as m_ownShown and
  // m_folded say.
  void updateOwnParts();
  // A source menu's commands as checks in `mirror`; false when it has none.
  bool mirrorMenu(QMenu* mirror, const QMenu* source, const QString& path, QStringList& lines);
  void choose(const QString& id, bool on);
  QString tooltipOf(const QString& id) const;
  // "Quick access file.save disabled: no unsaved changes" when the state
  // changed since it was logged (or `always`).
  void logState(const QString& id, bool always);
  void schedulePlaces();
  void logPlaces();

  CommandRegistry& m_registry;
  Hooks m_hooks;
  QuickAccessSettings m_settings;
  QStringList m_shown;
  QHBoxLayout* m_row = nullptr;
  QList<QPointer<QToolButton>> m_buttons;
  QStringList m_buttonIds; // the item of each of m_buttons
  QHash<QString, QPointer<QWidget>> m_rowCommands; // setRowCommands
  // setTopBar (mitcad#2129).
  bool m_topBar = false;
  QPointer<QMenu> m_fileMenu;
  int m_fitCount = -1;             // the present items shown (fitWidth); -1 all
  QStringList m_foldedIds;          // the present items folded
  QHash<QString, QString> m_hidden; // Hooks::hiddenReason as logged
  QList<QMetaObject::Connection> m_rowStates;      // their states followed
  QWidget* m_divider = nullptr;
  QList<QPointer<QWidget>> m_leading; // setLeadingWidgets
  QWidget* m_leadingDivider = nullptr; // the hairline after them
  bool m_ownShown = true;              // its own parts shown (applyVisible)
  QToolButton* m_menuButton = nullptr;
  QHash<QString, bool> m_loggedEnabled;
  QSet<QString> m_menuIds; // the commands the open menu has under File, Edit, View, Tools
  QTimer* m_placesTimer = nullptr;
  QString m_lastPlaces;
  bool m_folded = false;
  bool m_wanted = true;
  bool m_visibleApplied = false;
  bool m_hiddenWhileEmpty = false;
  bool m_loggedEmpty = false;
  int m_iconSize = 18;
};

} // namespace mitcad
