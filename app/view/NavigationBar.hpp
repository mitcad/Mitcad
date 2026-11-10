// SPDX-License-Identifier: MIT
#pragma once

#include <QList>
#include <QPoint>
#include <QPointer>
#include <QString>
#include <QToolButton>
#include <QWidget>

#include "../framework/NavigationBarPins.hpp"

class QAction;
class QContextMenuEvent;
class QMenu;
class QTimer;

namespace mitcad {

class CommandRegistry;

// The navigation bar at the bottom of the view (mitcad#1086): Look At, Fit,
// Visual Style (a drop-down of the styles), Layout Grid, Snap to Grid and
// Comment Pins (mitcad#1119), and after a separator the commands the user
// pinned to it (mitcad#1589, NavigationBarPins: the user's preference, Edit
// Parameters by default, mitcad#1582), in every workspace and in sketches.
// Its buttons are the commands' own actions (QToolButton::setDefaultAction),
// so their checks, enabled states and tooltips (the command's name, its
// tooltip and its shortcut, as the registry gives them) are the menu's:
// Snap to Grid toggled in the bar, in the View menu, by its shortcut or in
// the Sketch Palette is the same action. The drop-down holds the View
// menu's style actions, in their group. Pinned Edit Parameters runs the
// entry that is enabled: in a sketch the SKETCH tab's sketch.parameters,
// elsewhere solid.parameters. A pinned button's right-click (also while it
// is disabled) has Unpin from Navigation Bar and Change Keyboard Shortcut…;
// dragged along the pinned buttons, it takes the place where it is dropped.
//
// Card: the buttons flat, for the Floating layout's capsule card at the
// bottom centre of the view (MainWindow puts it into a GlassCard,
// FloatingLayout::Anchor::BottomCenter). StatusBar: tool buttons for the
// Docked layout's status bar.
//
// For UI tests it logs, when they change: "Navigation bar shown (card) at
// x,y WxH" (or "(status bar)"; "Navigation bar hidden"), each button's
// middle as "Navigation bar Look At at x,y" (Fit, Visual Style, Layout
// Grid, Snap to Grid, Comment Pins, then the pinned ones by their names
// without the "…": Edit Parameters, Measure), all in the main window's
// coordinates, the states as "Navigation bar states: Layout Grid on (View
// menu on), Snap to Grid off (View menu off), Visual Style Shaded", the
// drop-down's entries as "Context menu: Shaded | …" when it opens, and
// "Navigation bar: <command>" for a button clicked. The pins: "Navigation
// bar pinned: solid.parameters, inspect.measure" ("... pinned: none") as
// the buttons are made, "Pinned to navigation bar: Measure", "Unpinned from
// navigation bar: Measure", "Navigation bar pinned order: Measure, Edit
// Parameters" (a drag), and a pinned button's menu as "Navigation bar menu
// inspect.measure: Unpin from Navigation Bar | Change Keyboard Shortcut…"
// with its entries' places, "Navigation bar menu entry Unpin from
// Navigation Bar at x,y".
class NavigationBar : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Card, StatusBar };

  // `styles`: the View menu's Visual Style actions in menu order, a null
  // entry for a separator (ViewController::visualStyleActions). The other
  // buttons are the registry's view.look_at, view.fit, view.grid and
  // view.snap_grid, view.comment_pins, and the pinned commands.
  NavigationBar(CommandRegistry& registry, const QList<QAction*>& styles, Presentation presentation,
                QWidget* parent = nullptr);

  Presentation presentation() const { return m_presentation; }
  // The widget whose place and visibility the log reports: the card that
  // holds the bar in the Floating layout (null: the bar itself).
  void setHolder(QWidget* holder);

  // The commands pinned to the bar (mitcad#1589), saved at once and logged
  // (above). An id is the command's or an alias's; sketch.parameters is
  // solid.parameters' pin (the one Edit Parameters button for both).
  // movePin's `index` is the place among the pinned buttons.
  const NavigationBarPins& pins() const { return m_commandPins; }
  bool isPinned(const QString& id) const;
  void pin(const QString& id);
  void unpin(const QString& id);
  void movePin(const QString& id, int index);

signals:
  // The pins changed (pinned, unpinned or reordered).
  void pinsChanged();
  // Change Keyboard Shortcut… of a pinned button's menu: Tools > Keyboard
  // Shortcuts at the command.
  void shortcutRequested(const QString& id);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  bool event(QEvent* event) override;
  void changeEvent(QEvent* event) override;
  void contextMenuEvent(QContextMenuEvent* event) override;

private:
  QToolButton* addCommand(const QString& id);
  // The pin's command for an id: the command's own (an alias's), the
  // model's Edit Parameters for the sketch's; "" for no command.
  QString pinOf(const QString& id) const;
  // The pinned buttons made again after the separator, in the pins' order
  // (the separator hidden without them).
  void updatePinnedButtons();
  // A pinned button's menu at `global`: Unpin from Navigation Bar and
  // Change Keyboard Shortcut….
  void showPinMenu(QToolButton* button, const QPoint& global);
  // A pinned button pressed, dragged and dropped among the pinned buttons
  // (eventFilter); true when the event is the drag's.
  bool dragPinned(QToolButton* button, QEvent* event);
  void styleButton(QToolButton* button) const;
  void updateStyleButton();
  // Edit Parameters' button runs the entry that is enabled: the sketch's
  // in a sketch, else the model's (mitcad#1582).
  void updateParametersButton();
  // Logs the places and the states once the event loop is back (watched by
  // the UI tests' sync).
  void requestLog();
  void logNow();

  CommandRegistry& m_registry;
  Presentation m_presentation;
  QWidget* m_holder = nullptr;
  QList<QToolButton*> m_buttons; // in the bar's order
  QToolButton* m_style = nullptr;
  QMenu* m_styleMenu = nullptr;
  QList<QAction*> m_styles;
  QAction* m_grid = nullptr;
  QAction* m_snap = nullptr;
  QAction* m_pins = nullptr; // Comment Pins (mitcad#1119)
  // Edit Parameters (mitcad#1582): its pinned button (null while it is not
  // pinned), solid.parameters and sketch.parameters.
  QToolButton* m_parametersButton = nullptr;
  QAction* m_parameters = nullptr;
  QAction* m_sketchParameters = nullptr;
  // The pins (mitcad#1589): the separator before them, their buttons in
  // order and the buttons before them; a pinned button being dragged.
  NavigationBarPins m_commandPins;
  QWidget* m_pinSeparator = nullptr;
  QList<QToolButton*> m_pinButtons;
  QList<QToolButton*> m_fixedButtons;
  QPointer<QToolButton> m_pinDrag;
  QPoint m_pinDragStart;
  bool m_pinDragging = false;
  QTimer* m_logTimer = nullptr;
  QString m_loggedPlace;
  QString m_loggedButtons;
  QString m_loggedStates;
};

} // namespace mitcad
