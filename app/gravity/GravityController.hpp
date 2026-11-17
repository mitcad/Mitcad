// SPDX-License-Identifier: MIT
#pragma once

// A Flex item let hang in the 3D view (mitcad#1713; app/COMMANDS.md
// "Hanging cables"; the model's side: core's commands.md "Gravity" and
// "Hanging cables"), driven by the Let It Hang panel
// (commands/GravityCommands.cpp), which carries the direction of gravity,
// the length, the damping and the end pins:
//
//  - apply(): what the panel wants made so: the direction set when it
//    differs from the design's (gravity.set; the item goes on hanging and
//    settles again), the item let hang (flex.hang, then flex.hang_step in
//    real-time steps until the model says it rests), its length, damping
//    and end pins (flex.hang_set). Its body is faded meanwhile and the item
//    drawn along its chain at its section's size and in its colour
//    (flex/FlexRope.hpp).
//  - Its nodes are handles, drawn as the model's answer pins them: a
//    filled one is pinned (it stays where it is; at first both ends and
//    every node attached to a face, an edge, a point or a port, as the
//    model's flex.hang pins them when no pins are sent), a ring hangs free;
//    a click toggles it (flex.hang_set) and the item settles again; an
//    end's click is the panel's Start pinned or End pinned too. The
//    panel's ends change only the ends' pins, the nodes between kept as
//    the model has them (Hanging::pinsWithEnds).
//  - A left press on the item grabs it where the press hit: while the
//    button is held that point follows the pointer, the rest hanging from
//    it (flex.hang_step with the grab, one step at a time at the frame
//    rate the model allows: a step in flight is never joined by a queued
//    one; a tick meanwhile is skipped); released, it settles again. The
//    steps run on the model's worker as runtime commands
//    (CommandHost::modelRuntimeCommandAsync): the view, the panel and the
//    mouse go on while one is computed, and every other use of the model
//    waits for it (takeModel) rather than being refused.
//  - lock() is the panel's OK: it keeps the shape it rests in as the item's
//    nodes (flex.hang_lock); a lock the model refuses leaves it hanging.
//    release() puts it back (flex.hang_cancel): the panel's Cancel; inputs
//    that are not complete while a value is typed leave it hanging.
//  - While the panel is open a small marker of the design's direction of
//    gravity shows in the view's corner (setPanelOpen).
//
// The left button on the hanging item or a handle is the item's, before the
// view's; a press anywhere else is the view's, so that the panel's
// Direction From takes a face there. The model computes, this only draws.
// The plain logic is GravityLogic.hpp.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QElapsedTimer>
#include <QObject>
#include <QPointF>
#include <QString>
#include <QTimer>

#include <gp_Trsf.hxx>

#include "../OcctViewer.hpp"
#include "../flex/FlexRope.hpp"
#include "GravityLogic.hpp"

class QEvent;

namespace mitcad {

class SketchHost;

namespace gravity {

class GravityOverlay;

class GravityController : public QObject {
  Q_OBJECT

public:
  GravityController(SketchHost& host, OcctViewer& viewer, QObject* parent = nullptr);
  ~GravityController() override;

  // What the main window gives.
  struct Hooks {
    // Why the design may not be changed now (a read-only window), in
    // words; empty when it may.
    std::function<QString()> blocked;
    // Shows the bodies as the model has them (the main window's
    // showBodies, which fades the hanging item's body by placeShown).
    std::function<void()> showBodies;
    // The state changed (hanging, kept, put back): the actions follow.
    std::function<void()> changed;
    // The words of the last command the model refused (the main window's
    // lastError).
    std::function<QString()> lastError;
    // Where the view shows a Flex item's component (its points are in the
    // component's coordinates).
    std::function<gp_Trsf(const QString&)> placement;
    // A click on an end's handle pinned or let go that end: the panel's
    // Start pinned and End pinned follow.
    std::function<void(bool first, bool last)> endsChanged;
  };
  void setHooks(Hooks hooks) { m_hooks = std::move(hooks); }

  // The design's direction of gravity and the hanging item read again
  // (after every change of the model, before the bodies are shown): the
  // marker follows; an item the model let go (the design changed) is no
  // longer held here.
  void reload();
  const Direction& direction() const { return m_direction; }

  bool isHanging() const { return m_phase != Phase::None; }
  bool isDragged() const { return m_phase == Phase::Dragging; }
  // The Flex item that hangs (its uid), empty when none does.
  const QString& hangingItem() const { return m_flex; }

  // The Let It Hang panel opened or closed: the marker of the direction of
  // gravity shows only while it is open.
  void setPanelOpen(bool open);
  // Makes what the panel wants so (see the top). Empty when done, else the
  // words of what was refused (the panel shows them; OK waits).
  QString apply(const HangWish& wish);
  // Puts the hanging item back (flex.hang_cancel), nothing of the design
  // changed; nothing when none hangs.
  void release();
  // The panel's Clear: the item put back first, then gravity.clear (one
  // undo step); false when the model refused it.
  bool clearGravity();
  // The panel's OK: keeps the shape the item rests in (settling still: what
  // is left of it first); false when none hangs or the model refused (it
  // still hangs, the model's words in lastError).
  bool lock();

  // Fades the body of the hanging item among those to show (it is drawn
  // along its chain meanwhile).
  void placeShown(std::vector<BodyDisplay>& bodies) const;

  // --- What the overlay draws (widget pixels).
  struct Scene {
    std::optional<ScreenArrow> marker; // the direction of gravity
    QPointF markerAt;
    double markerRadius = 0.0;
    bool sourceMoved = false;
    // The hanging item along its chain, in the design.
    std::vector<flex::RopeSample> rope;
    flex::RopeLook look;
    // Its nodes' handles: where, and whether each is pinned.
    struct Handle {
      QPointF at;
      bool pinned = false;
    };
    std::vector<Handle> handles;
    std::optional<QPointF> grabbed; // the held point
    std::optional<QPointF> pointer; // and the pointer
  };
  Scene scene() const;
  flex::RopeView ropeView() const;
  const OcctViewer& viewer() const { return m_viewer; }
  static double handleRadius();

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  enum class Phase {
    None,     // nothing hangs
    Held,     // hanging and at rest
    Dragging, // the left button holds a point of it
    Settling, // let go: it moves until it rests
  };

  bool mouse(QEvent* event);
  // The node whose handle is under a point of the view, empty for none.
  QString nodeAt(const QPointF& position) const;
  // The point of the chain under a point of the view (its index), -1 for
  // none.
  int chainAt(const QPointF& position) const;
  void setHand(OcctViewer::HandCursor hand);
  // What to do: the status line while it hangs.
  QString hint() const;
  // Lets the Flex item `flex` hang; false with the model's refusal.
  bool hang(const QString& flex, double damping);
  // One step of the drag or the settling, unless one is in flight.
  void tick();
  // Starts a step on the model's worker (a runtime command, never waited
  // for; at once where the host cannot); its answer comes to stepDone.
  void startStep(const QJsonObject& command);
  void stepDone(quint64 serial, Phase phase, const QJsonObject& answer, const QString& error, bool cancelled);
  // A step in flight no longer counts: its answer, when it comes, is
  // dropped (a command's answer came after it).
  void supersede();
  // The model for a command now: a step in flight is waited for (and
  // superseded); false when another job computes the model.
  bool takeModel();
  // Runs one model command of the hanging item and takes its answer;
  // false when the model refused it (the item is let go then when the
  // model no longer has it).
  bool step(const QJsonObject& command);
  // A step or command the model refused: held as it is, or let go.
  void refusedStep(const QString& why);
  // The model's answer of the hanging item taken; false when it is none.
  bool take(const QJsonObject& answer);
  // step() with the gate taken.
  bool send(const QJsonObject& command);
  void startSteps();
  // It settles again after a change (unless it is dragged).
  void settleAgain();
  void togglePin(const QString& node);
  // It comes to rest, or stops settling.
  void rested(bool atRest);
  // It no longer hangs here; the overlay and the actions follow.
  void end();
  void showFaded();
  void updateOverlay();
  gp_Pnt design(const Vec3& local) const;
  Vec3 local(const gp_Pnt& design) const;
  Vec3 down() const;
  QString refused() const;
  void status(const QString& text);
  void logHandles();
  void logMarker();
  void log(const QString& text) const;

  SketchHost& m_host;
  OcctViewer& m_viewer;
  Hooks m_hooks;
  GravityOverlay* m_overlay = nullptr;
  QTimer m_timer;
  Direction m_direction;
  bool m_panelOpen = false;
  QString m_loggedDirection;
  QString m_loggedMarker;
  QString m_loggedHandles;

  Phase m_phase = Phase::None;
  QString m_flex;
  QString m_name; // the item's own name ("Hose 10"), else the feature's
  std::optional<Hanging> m_hanging;
  gp_Trsf m_placement;
  flex::RopeLook m_look;
  // The grabbed point of the chain where the press took it (the item's
  // coordinates), the plane the pointer moves on (through it, facing the
  // viewer, in the design) and the pointer's point on it.
  Vec3 m_grab{};
  gp_Pnt m_through;
  std::optional<Vec3> m_target;
  std::optional<QPointF> m_pointer;
  StepGate m_gate;
  QElapsedTimer m_frame;  // since the last step began
  QElapsedTimer m_settle; // since the release
  // The step in flight (its serial), the serial of the last one started,
  // since when it is computed, and how long the steps took.
  std::optional<quint64> m_pending;
  quint64 m_serial = 0;
  QElapsedTimer m_stepClock;
  StepTimes m_times;
  bool m_reloadWaiting = false;
  int m_dragSteps = 0;
  int m_settleSteps = 0;
  // A left press on the item (or a double click's second press on it) was
  // taken: its moves and its release are the tool's too, so that the view
  // neither orbits nor picks.
  bool m_swallowing = false;
};

} // namespace gravity
} // namespace mitcad
