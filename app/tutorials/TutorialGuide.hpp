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
// Help › Leave Tutorial end it. What the user did that completes it, and
// its end, are events (event()): to the tutorial page and to MCP's
// mitcad://ui-guide resource.
//
// Its log lines are the UI tests' ("Guide: step <id> started: ...",
// "Guide: command <id> refused during step <id>", "Guide: input held back
// on <name>", "Guide: step <id> done: ...", "Guide: step <id> left
// (<reason>)", "Guide callout at x,y", "Guide leave button at x,y").

#include <functional>

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QTimer>

#include "GuideStep.hpp"

class QLabel;
class QPushButton;
class QWidget;

namespace mitcad {

class GuideHighlight;

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
};

class TutorialGuide : public QObject {
  Q_OBJECT

public:
  explicit TutorialGuide(GuideHost host, QObject* parent = nullptr);
  ~TutorialGuide() override;

  // Shows `step`, ending one shown before ("replaced").
  void start(const GuideStep& step);
  // Ends the step shown, if any: "escape", "leave", "ended" (by the page or
  // MCP), "replaced", "closed" (the window or its tutorial).
  void end(const QString& reason);
  bool active() const { return m_active; }
  const GuideStep& step() const { return m_step; }

  // MainWindow::trigger's check: whether the command may run now.
  bool allowsCommand(const QString& id) const { return !m_active || m_step.allowsCommand(id); }
  // A command the step does not allow was asked for: logged, said in the
  // callout, an event.
  void refuseCommand(const QString& id, const QString& name);
  // A command finished (a feature's panel closed: `committed` for OK) or an
  // action ran: a Finished step on it is done.
  void commandFinished(const QString& id, bool committed);

  // For MCP's ui_guide and its resource: whether a step is shown, the step,
  // its sequence number (one per step started) and the last event.
  QJsonObject state() const;

signals:
  // {"event": "step_done" | "step_left" | "refused", "step": <id>,
  //  "sequence": n, ...} (docs/tutorials.md "Events").
  void event(const QJsonObject& event);
  // The state changed (a step started or ended, an event).
  void changed();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void tick();
  void placeCallout(const QRect& target);
  void showNote(const QString& note);
  void finish(const QJsonObject& done, const QString& what);
  void emitEvent(QJsonObject event);
  // Whether a click on `widget` (of the window) is held back now, and the
  // name of the place for the log.
  bool holdsBack(QWidget* widget, int button, QString* place) const;

  GuideHost m_host;
  GuideStep m_step;
  bool m_active = false;
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
};

} // namespace mitcad
