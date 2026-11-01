// SPDX-License-Identifier: MIT
#pragma once

#include <QList>
#include <QString>
#include <QWidget>

class QAction;
class QMenu;
class QTimer;
class QToolButton;

namespace mitcad {

class CommandRegistry;

// The navigation bar at the bottom of the view (mitcad#1086): Look At, Fit,
// Visual Style (a drop-down of the styles), Layout Grid and Snap to Grid,
// in every workspace and in sketches. Its buttons are the View menu's own
// actions (QToolButton::setDefaultAction), so their checks, enabled states
// and tooltips (the command's name, its tooltip and its shortcut, as the
// registry gives them) are the menu's: Snap to Grid toggled in the bar, in
// the View menu, by its shortcut or in the Sketch Palette is the same
// action. The drop-down holds the View menu's style actions, in their group.
//
// Card: the buttons flat, for the Floating layout's capsule card at the
// bottom centre of the view (MainWindow puts it into a GlassCard,
// FloatingLayout::Anchor::BottomCenter). StatusBar: tool buttons for the
// Docked layout's status bar.
//
// For UI tests it logs, when they change: "Navigation bar shown (card) at
// x,y WxH" (or "(status bar)"; "Navigation bar hidden"), each button's
// middle as "Navigation bar Look At at x,y" (Fit, Visual Style, Layout
// Grid, Snap to Grid), all in the main window's coordinates, the states as
// "Navigation bar states: Layout Grid on (View menu on), Snap to Grid off
// (View menu off), Visual Style Shaded", the drop-down's entries as
// "Context menu: Shaded | …" when it opens, and "Navigation bar: <command>"
// for a button clicked.
class NavigationBar : public QWidget {
  Q_OBJECT

public:
  enum class Presentation { Card, StatusBar };

  // `styles`: the View menu's Visual Style actions in menu order, a null
  // entry for a separator (ViewController::visualStyleActions). The other
  // buttons are the registry's view.look_at, view.fit, view.grid and
  // view.snap_grid.
  NavigationBar(CommandRegistry& registry, const QList<QAction*>& styles, Presentation presentation,
                QWidget* parent = nullptr);

  Presentation presentation() const { return m_presentation; }
  // The widget whose place and visibility the log reports: the card that
  // holds the bar in the Floating layout (null: the bar itself).
  void setHolder(QWidget* holder);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  bool event(QEvent* event) override;
  void changeEvent(QEvent* event) override;

private:
  QToolButton* addCommand(const QString& id);
  void styleButton(QToolButton* button) const;
  void updateStyleButton();
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
  QTimer* m_logTimer = nullptr;
  QString m_loggedPlace;
  QString m_loggedButtons;
  QString m_loggedStates;
};

} // namespace mitcad
