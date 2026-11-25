// SPDX-License-Identifier: MIT
#pragma once

// The guide step shown in a design window (mitcad#2012, docs/tutorials.md
// "Guide steps"): one engine for a tutorial page's bridge (window.mitcad)
// and live MCP's ui_guide. The step's target is outlined and a callout
// beside it says what to do; the window's other input is held back with a
// short note (clicks in the Browser, the timeline, the Projects and
// Configuration panels, picks in the view the step does not take; commands
// the step does not allow are refused by MainWindow::trigger through
// allowsCommand); view navigation stays free unless the step says
// otherwise. Escape with no command open, the callout's Leave Tutorial and
// Help › Leave Tutorial end it (Escape in a popup, a menu or the workspace
// selector's list, closes the popup). What the user did that completes it, and
// its end, are events (guideEvent()): to the tutorial page and to MCP's
// mitcad://ui-guide resource.
//
// Input it holds back is an event too ("held", mitcad#2366), so that the
// page can help at once; and the page may play an input demonstration of
// its own pictures over the step's target (demonstrate(),
// GuideDemonstration.hpp) on a layer that takes no input, until the step
// ends.
//
// A step with "hold" (mitcad#2368) keeps holding the input once it is done,
// while the page shows what comes next: its callout hidden, no command but
// Leave Tutorial and the camera's, no picks in the view (a sketch's drawing
// tool still open too), the other places held as during a step; the
// tutorial pane and view navigation stay free. step_done is sent at once;
// the hold ends at the next step, the page's release_hold, end_step, Leave
// Tutorial or the tutorial closed.
//
// Its log lines are the UI tests' ("Guide: step <id> started: ...",
// "Guide: command <id> refused during step <id>", "Guide: input held back
// on <name>", "Guide: step <id> done: ...", "Guide: step <id> left
// (<reason>)", "Guide: holding input after step <id>", "Guide: command <id>
// refused during the hold after step <id>", "Guide: hold after step <id>
// released (<reason>)", "Guide: step <id> view zoomed, not its orbit"
// (orbited, panned, zoomed; once an action a step),
// "Guide callout at x,y", "Guide leave button at x,y"; the target's
// rectangle as "Guide: step <id> target at x,y wxh" and, in the top
// window's coordinates as the toolbar's places, "Guide: step <id> outline
// at x,y wxh"; "Guide: demonstration on step <id> started: ...", "Guide:
// demonstration frame <n> at x,y" (the frame's place, in the top window's
// coordinates), "Guide: demonstration on step <id> ended (<reason>)").

#include <functional>

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QTimer>

#include "GuideDemonstration.hpp"
#include "GuideStep.hpp"

class QLabel;
class QMouseEvent;
class QPushButton;
class QWidget;

namespace mitcad {

class GuideHighlight;
class GuideDemonstrationLayer;

// What the guide asks of its window (MainWindowGuide.cpp).
struct GuideHost {
  QWidget* window = nullptr; // the design window: its coordinates are the guide's
  QWidget* viewer = nullptr; // the 3D view
  // The target's rectangle in the window's coordinates; null when it is not
  // shown now (a command with no visible button, an input of a panel that is
  // not open).
  std::function<QRect(const GuideStep& step)> targetRect;
  // The open command's id (its panel), empty when none is open.
  std::function<QString()> openCommand;
  // The text of the open command's input `key`, empty when it has none.
  std::function<QString(const QString& key)> inputText;
  // The kinds of what is picked now (GuideStep's names: "face", "sketch_curve").
  std::function<QStringList()> pickedKinds;
  // Whether no command or tool is open (Escape then ends the step).
  std::function<bool()> idle;
  // The window's mode (guideModes(): idle, pick_plane, sketch, command).
  std::function<QString()> mode;
  // The callout's Leave Tutorial: Help › Leave Tutorial.
  std::function<void()> leave;
  // The toolbar (mitcad#2370): whether the workspace selector's list is
  // open, the workspace shown ("DESIGN") and the tab shown ("SOLID").
  std::function<bool()> workspaceListOpen;
  std::function<QString()> workspace;
  std::function<QString()> tab;
  // Whether a press of `button` with `keys` at `position` (in the view's
  // coordinates) navigates the view by the active navigation scheme (an
  // Alt + left orbit, the orientation cube's drag): a left press that does
  // is free while the step allows navigation (mitcad#2369).
  std::function<bool(const QPoint& position, Qt::MouseButton button, Qt::KeyboardModifiers keys)> navigationPress;
};

class TutorialGuide : public QObject {
  Q_OBJECT

public:
  explicit TutorialGuide(GuideHost host, QObject* parent = nullptr);
  ~TutorialGuide() override;

  // Shows `step`, ending one shown before ("replaced") or releasing the hold
  // after one done ("next step").
  void start(const GuideStep& step);
  // Ends the step shown, if any: "escape", "leave", "ended" (by the page or
  // MCP), "replaced", "closed" (the window or its tutorial); or releases the
  // hold after a step done, for the same reason.
  void end(const QString& reason);
  // Releases the hold after a step done (mitcad#2368), if one holds:
  // "released" (the page's release_hold), "next step", "ended", "leave",
  // "closed".
  void releaseHold(const QString& reason);
  bool active() const { return m_active; }
  // Whether a step done with "hold" still holds the input.
  bool holding() const { return m_holding; }
  const GuideStep& step() const { return m_step; }

  // MainWindow::trigger's check: whether the command may run now.
  bool allowsCommand(const QString& id) const {
    if (m_holding) {
      return m_step.allowsCommandWhileHeld(id);
    }
    return !m_active || m_step.allowsCommand(id);
  }
  // A command the step does not allow was asked for: logged, said in the
  // callout, an event.
  void refuseCommand(const QString& id, const QString& name);
  // A command finished (a feature's panel closed: `committed` for OK) or an
  // action ran: a Finished step on it is done.
  void commandFinished(const QString& id, bool committed);
  // The user navigated the view and the camera moved (mitcad#2369,
  // OcctViewer::viewNavigated): `action` is "orbit", "pan" or "zoom", as
  // the active navigation scheme bound the gesture (a mouse drag, the
  // wheel, a trackpad scroll, an arrow key's turn). A Navigate step on that
  // action is done; another action is logged once a step and leaves it.
  void viewNavigated(const QString& action);

  // The view's picks held back, with a step or without one, as a step that
  // takes no picks holds them (mitcad#2367: while the tutorial pane is
  // centred over the view); navigation stays as the step has it (free
  // without one). Logged "Guide: view held (<why>)" and "Guide: view free".
  void holdView(bool held, const QString& why = QString());
  bool viewHeld() const { return m_viewHeld; }

  // Plays the page's demonstration over the step's target (mitcad#2366),
  // replacing one playing; false and the reason when no step is shown.
  bool demonstrate(const GuideDemonstration& demonstration, QString* error);
  // Ends the demonstration playing, if any: "stopped" (by the page),
  // "replaced", "played" (its plays ended), "step" (the step ended).
  void stopDemonstration(const QString& reason);
  bool demonstrating() const { return m_demonstrating; }

  // For MCP's ui_guide and its resource: whether a step is shown, the step,
  // its sequence number (one per step started), the last event, and while a
  // step is shown its target's rectangle and whether a demonstration plays.
  QJsonObject state() const;
  // The step's callout in the window's coordinates while it is shown, else
  // a null rectangle: a click there reaches the callout, not what lies
  // under it (the places logged for UI tests keep clear of it, mitcad#2311).
  QRect calloutRect() const;

signals:
  // {"event": "step_done" | "step_left" | "refused" | "held" |
  //  "demonstration_ended", "step": <id>, "sequence": n, ...}
  // (docs/tutorials.md "Events").
  void guideEvent(const QJsonObject& event);
  // The state changed (a step started or ended, an event).
  void changed();
  // The callout was placed elsewhere, shown or hidden (calloutRect).
  void calloutMoved();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void tick();
  void placeCallout(const QRect& target);
  void showNote(const QString& note);
  void finish(const QJsonObject& done, const QString& what);
  void emitEvent(QJsonObject event);
  // Whether the press `mouse` on `widget` (of the window) is held back now,
  // and the name of the place for the log.
  bool holdsBack(QWidget* widget, const QMouseEvent& mouse, QString* place) const;
  // The application's event filter while a step is shown, its hold lasts
  // or the view is held (holdView), else none.
  void updateEventFilter();
  // Without a step, the view held (holdView): whether `event` on `widget`
  // is held back.
  bool holdsViewInput(QWidget* widget, QEvent* event);
  // The page hears of input held back (mitcad#2366): `place` as holdsBack
  // names it, `input` "click", "double_click" or "wheel".
  void emitHeld(const QString& place, const QString& input, Qt::MouseButton button, Qt::KeyboardModifiers modifiers);
  // A frame of the demonstration: where it is now over the target.
  void demonstrationTick();
  // `point` of the window in its top window's coordinates (the test
  // driver's), as "x,y".
  QString topText(const QPointF& point) const;

  GuideHost m_host;
  GuideStep m_step;
  bool m_active = false;
  // A step with "hold" is done and its hold not yet released (mitcad#2368).
  bool m_holding = false;
  int m_sequence = 0;
  QJsonObject m_lastEvent;
  QTimer m_tick;
  QPointer<GuideHighlight> m_highlight;
  QPointer<QWidget> m_callout;
  QLabel* m_text = nullptr;
  QLabel* m_note = nullptr;
  QPushButton* m_leaveButton = nullptr;
  QTimer m_noteTimer;
  QRect m_target;
  bool m_targetLogged = false;
  QString m_calloutLogged;
  // A Value step: since when the typed text has matched (the value is
  // taken once it stayed so a moment, not while it is being typed).
  QElapsedTimer m_valueMatched;
  QString m_typedLogged; // the input's text the log said last
  // A Pick step: what was picked as it started (a pick is a new one).
  QStringList m_pickedAtStart;
  QString m_lastHeldBack;
  // The widget whose press was held back: its release and drags too.
  QPointer<QWidget> m_heldWidget;
  // A Navigate step: the other navigations logged during it.
  QStringList m_navigationsLogged;
  bool m_viewHeld = false; // holdView
  bool m_filtering = false; // the application's event filter installed
  // A wheel held back in the view is said to the page at most once a second.
  QElapsedTimer m_wheelHeld;
  // The demonstration playing (mitcad#2366), its layer, its clock and the
  // frame logged last.
  GuideDemonstration m_demonstration;
  bool m_demonstrating = false;
  QPointer<GuideDemonstrationLayer> m_demonstrationLayer;
  QTimer m_demonstrationTimer;
  QElapsedTimer m_demonstrationClock;
  int m_demonstrationFrame = -1;
  bool m_demonstrationHidden = false;
};

} // namespace mitcad
