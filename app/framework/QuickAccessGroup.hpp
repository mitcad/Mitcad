// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QHash>
#include <QList>
#include <QPointer>
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
// menu (QuickAccessSettings; by default New, Open, Save, Print, Design
// Properties and Preferences), in the menus' order, and a ⋮ button. The
// Floating layout's title row has it after the workspace tabs (TitleBar),
// the Docked toolbar's tab row after the tabs, right of Undo and Redo
// (Ribbon::addTabRowWidget), where it takes no width from the groups. A button is the command's action
// (icon, enabled state, its run); its tooltip names the command, its key
// and, while it is disabled, why. The ⋮ (and a right-click on the group)
// opens its menu: the File, Edit, View and Tools menus' commands as checks
// (submenus as submenus), Reset to Default, and, while the group is folded,
// its commands first. A command this build does not register is left out.
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
  };

  QuickAccessGroup(CommandRegistry& registry, Hooks hooks, QWidget* parent = nullptr);

  // Reads the settings and the menus again and makes the buttons.
  void rebuild();
  // The commands shown, in order.
  const QStringList& shownIds() const { return m_shown; }

  // Folded, only the ⋮ shows, whose menu then lists the commands first
  // (TitleBar folds the group before the design's name would be cut).
  void setFolded(bool folded);
  bool isFolded() const { return m_folded; }
  // The group's width with every button shown.
  int unfoldedWidth() const;
  // At most `height` high, with smaller icons (the Docked tab row's height,
  // so that the toolbar does not grow).
  void setCompact(int height);

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
  void contextMenuEvent(QContextMenuEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // The registry commands of the menus, in their order; the default
  // commands this build registers but no menu has, at the end.
  QStringList availableIds() const;
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
  QWidget* m_divider = nullptr;
  QToolButton* m_menuButton = nullptr;
  QHash<QString, bool> m_loggedEnabled;
  QTimer* m_placesTimer = nullptr;
  QString m_lastPlaces;
  bool m_folded = false;
  int m_iconSize = 18;
};

} // namespace mitcad
