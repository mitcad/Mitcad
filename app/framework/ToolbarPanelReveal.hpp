// SPDX-License-Identifier: MIT
#pragma once

#include <QList>
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QRect>
#include <QTimer>
#include <QWidget>

#include <functional>

namespace mitcad {

// The toolbar panel hidden by View › Toolbar Panel (mitcad#2426, GitHub
// mitcad/Mitcad#571) and shown for a while at the view's top edge: a band a
// few pixels high under the top bar (a widget of its own over the view,
// which takes the pointer there and nothing else); the pointer resting on it
// for kRestMs shows the panel over the view, which keeps its place, and the
// panel goes again once the pointer has left it and no menu, pop-up,
// dialog or command panel opened from it is open (a command started from
// it keeps it while its command panel is open, mitcad#2472: keepFor,
// release). Nothing shows while a mouse button is held
// (an orbit or a sketch drag crossing the edge: the view holds the pointer,
// so the band never hears of it, and a release on the band waits for a
// rest without buttons).
//
// The window says where things are and does the showing (Hooks); this keeps
// the band, the rest and the leave.
class ToolbarPanelReveal : public QObject {
public:
  struct Hooks {
    // The widget that shows over the view while revealed (the Floating
    // layout's panel row, the Docked layout's panel): the pointer is in it
    // while it is over its rectangle.
    std::function<QWidget*()> panel;
    // The band's place in the band's parent (the widget given to the
    // constructor); empty: no band (the panel shown, the Start page).
    std::function<QRect()> band;
    // Whether the panel may show now (hidden by the user, a design shown).
    std::function<bool()> canReveal;
    std::function<void()> reveal;
    // `how`: why it goes ("the pointer left it", "shown again", ...).
    std::function<void(const QString& how)> conceal;
    // While revealed, after a watched widget moved or was resized (the
    // window, the top bar): the panel placed again where it shows (the
    // Docked panel over the view across the window's width, mitcad#2527);
    // optional.
    std::function<void()> place;
  };

  // How long the pointer rests on the band before the panel shows, and how
  // long after it left the panel before the panel goes.
  static constexpr int kRestMs = 300;
  static constexpr int kLeaveMs = 400;
  // The band's height under the top bar.
  static constexpr int kBandHeight = 4;

  ToolbarPanelReveal(QWidget* bandParent, Hooks hooks);
  ~ToolbarPanelReveal() override;

  // The band placed again (shown, moved, or hidden when there is none):
  // after a layout, a resize, the panel hidden or shown. Logs "Toolbar
  // panel reveal band at x,y: wxh" (its middle, in the window's
  // coordinates) when it changed, and "Toolbar panel reveal band: none".
  void updateBand();
  // The band placed again (a moment later) when this widget moves, is
  // resized, shown or hidden: what the band's place follows (the top bar,
  // the view); while revealed, the panel too (Hooks::place).
  void watch(QWidget* widget);
  bool revealed() const { return m_revealed; }
  // The panel back at once without the leave's wait (shown for good, the
  // Start page, the window closing); nothing while it is not revealed.
  void concealNow(const QString& how);
  QWidget* band() const { return m_band; }
  // A command whose panel opened (mitcad#2472): when it was started from
  // the revealed panel (the last input while revealed was a press in the
  // panel or in a menu or pop-up opened from it, or a key in its search
  // field), the panel stays while the command's panel is open, wherever
  // the pointer goes; logs "Toolbar panel kept (command panel <name>
  // open)". `owner` (the command's session) is compared only; a deleted
  // owner releases it too. Nothing for a command started otherwise (a
  // key, the command search) or while the panel is not revealed.
  void keepFor(QObject* owner, const QString& name);
  // That command ended (OK, Cancel, Esc): logs "Toolbar panel released
  // (command <name> ended)"; with the pointer away from the panel it goes
  // kLeaveMs later ("hidden again (the command ended)"), else once the
  // pointer leaves it. Nothing for another owner (a command started from
  // the panel after it keeps it).
  void release(QObject* owner);
  bool kept() const;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void rested();
  void leaveCheck();
  // The pointer is over the panel or the band (global coordinates).
  bool overPanel(const QPoint& global) const;
  // A menu, pop-up or dialog is open, a button held, or the keyboard is in
  // a field of the panel (its search field): the panel stays.
  bool panelBusy() const;
  void finish(const QString& how);
  // The widget is in the panel, or in a menu or pop-up opened from it
  // (its parents, across windows, reach the panel).
  bool fromPanel(const QWidget* widget) const;

  QPointer<QWidget> m_parent;
  Hooks m_hooks;
  QPointer<QWidget> m_band;
  QTimer m_rest;
  QTimer m_leave;
  bool m_bandQueued = false;
  QList<QObject*> m_watched; // watch(); compared only, never dereferenced
  // The widget a press while revealed was logged for (a press propagates
  // to its parents: logged once); compared only.
  const QWidget* m_pressLogged = nullptr;
  bool m_revealed = false;
  bool m_inside = false; // the pointer's last move was over the panel or the band
  QString m_loggedBand;
  // The last press or key while revealed went to the panel or a menu or
  // pop-up of it: a command starting now was started from it (keepFor).
  bool m_inputInPanel = false;
  // The command whose panel keeps the panel (keepFor), and its name.
  QPointer<QObject> m_keeper;
  QString m_keptName;
  // A command that kept it ended with the pointer away: the next hide says
  // so ("the command ended").
  bool m_commandEnded = false;
};

} // namespace mitcad
