// SPDX-License-Identifier: MIT
#pragma once

// The plain logic of gravity in the view (mitcad#1713; app/COMMANDS.md
// "Hanging cables"; core's commands.md "Gravity" and "Hanging cables"):
// the direction of gravity as the model gives it, a Flex item let hang as
// the model answers (its chain, its nodes and pins), the commands both
// take, what the Let It Hang panel wants, where a left press goes while an
// item hangs, the one step in flight and the time a step simulates. Qt
// Core only, no view:
// the controller (GravityController.hpp) draws and drives it, the unit
// tests (app/tests/gravity_logic.cpp) check it.

#include <array>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

namespace mitcad::gravity {

using Vec3 = std::array<double, 3>;

Vec3 add(const Vec3& a, const Vec3& b);
Vec3 sub(const Vec3& a, const Vec3& b);
Vec3 scaled(const Vec3& a, double k);
double dot(const Vec3& a, const Vec3& b);
Vec3 cross(const Vec3& a, const Vec3& b);
double length(const Vec3& a);
// The unit vector of `a`; none for a zero or non-finite one.
std::optional<Vec3> unit(const Vec3& a);
// [x, y, z] of the model's JSON; none when it is not three finite numbers.
std::optional<Vec3> vecOf(const QJsonValue& value);
QJsonArray toJson(const Vec3& a);

// The design's direction of gravity as the gravity query gives it.
struct Direction {
  std::optional<Vec3> direction; // none: the design has none
  // What it was taken from (sourceKind): face, plane, axis, edge, line,
  // construction, or empty for a direction given as a vector.
  QString source;
  bool reversed = false;
  // The source gives another direction now (it moved since); the Let It
  // Hang panel takes it again.
  bool sourceMoved = false;
  QJsonObject sourceJson; // `source` as the query gives it (for the panel)
  // The uid of the Flex item that hangs (the query's `hanging`), empty
  // when none does.
  QString hanging;

  static Direction read(const QJsonObject& gravityQuery);
};

// What a reference of the model's JSON is (commands.md, "References to
// geometry"): face, plane (an origin or a fixed plane), axis (an origin or
// a fixed axis), edge, line (a sketch line), construction (a construction
// feature's datum), empty for anything else. The UI says it in words.
QString sourceKind(const QJsonValue& geometry);

// --- A Flex item let hang

// A node of the hanging item (the answers' `nodes`): its point of the
// chain, where it is now, whether it is pinned, and what it was attached
// to before (free, face, edge, point or port).
struct HangNode {
  QString id;
  int index = 0;
  Vec3 point{};
  double along = 0.0;
  bool pinned = false;
  QString support;
  bool moved = false;
};

// The hanging item, as flex.hang, flex.hang_step and flex.hang_set answer
// (and the flex_hang query), in the item's component's coordinates: its
// frames from the answer's `polyline` and `normals` (the lengths along the
// chain and its directions worked out here).
struct Hanging {
  QString flex;
  struct Frame {
    double along = 0.0;
    Vec3 point{};
    Vec3 tangent{0.0, 0.0, 1.0};
    Vec3 normal{0.0, 1.0, 0.0}; // the section's y
  };
  std::vector<Frame> frames;
  std::vector<bool> touching;
  std::vector<HangNode> nodes;
  double length = 0.0;
  double segment = 0.0;
  std::optional<double> grabbed; // the held point's length along the chain
  bool atRest = false;
  double time = 0.0;
  bool taut = false;
  double damping = 0.5; // how strongly the air holds it back, 0 to 1

  // None when the answer is no hanging item (no `flex` or fewer than two
  // points).
  static std::optional<Hanging> read(const QJsonObject& answer);
  // The ids of the pinned nodes, in order.
  QStringList pins() const;
  // The pins with `node` pinned when it was free, let go when it was
  // pinned.
  QStringList toggled(const QString& node) const;
  // The pins with its first node pinned as `start` says and its last as
  // `end` says, the nodes between as they are (the panel's Start pinned and
  // End pinned).
  QStringList pinsWithEnds(bool start, bool end) const;
  // Whether its first and its last node are pinned (false without nodes).
  bool startPinned() const;
  bool endPinned() const;
  const HangNode* node(const QString& id) const;
  // How many of its points lie on a body.
  int touchingCount() const;
  // The chain's point nearest to `point` (its index; -1 without points).
  int nearest(const Vec3& point) const;
};

// flex.hang: both ends pinned (the model's default); `damping` sent when
// given.
QJsonObject hangCommand(const QString& flex, std::optional<double> damping = std::nullopt);
// One step: `seconds` simulated; with `grab` (where the pointer took the
// chain and where the pointer is now) that point pulled there.
QJsonObject hangStepCommand(double seconds, const std::optional<std::pair<Vec3, Vec3>>& grab);
QJsonObject hangPinsCommand(const QStringList& pins);
QJsonObject hangLengthCommand(double length);
QJsonObject hangDampingCommand(double damping);
QJsonObject hangLockCommand();
QJsonObject hangCancelCommand();

// --- What the Let It Hang panel wants (commands/GravityCommands.cpp)

// The panel's definition, which the controller makes so while the panel is
// open: the item, the direction of gravity, its length, damping and end
// pins.
struct HangWish {
  QString flex; // the Flex item's uid
  // The gravity.set command of the panel's Direction From and Reverse;
  // none: the design's direction as it is.
  std::optional<QJsonObject> gravity;
  std::optional<double> length; // mm; none: as it is
  double damping = 0.5;
  bool startPinned = true;
  bool endPinned = true;

  QJsonObject toJson() const;
  // None without an item.
  static std::optional<HangWish> read(const QJsonObject& definition);
};

// Whether the design's direction is the one a gravity.set command of a
// geometry gives: taken from the same geometry where the same occurrence
// shows it, the same way, its source not moved since (setCommand).
bool sameDirection(const Direction& design, const QJsonObject& setCommand);

// --- The steps

// The most a step simulates: one frame of the model's (1/30 s).
constexpr double kStepMostSeconds = 1.0 / 30.0;
// The simulated seconds of a step for the real time since the last one:
// as long, within 1/240 s and kStepMostSeconds (1/60 s for a time that is
// not finite), so that the item moves at the speed it would; a slow frame
// leaves the time out rather than catching up (a step's work is in
// proportion to the time it simulates: a slower step must not ask for
// more).
double stepSeconds(double elapsedSeconds);

// How long the steps took (milliseconds, from the request to the answer),
// for the log line of its rest: "steps of 4.2 ms on average, 9.8 ms at
// most".
class StepTimes {
public:
  void add(double milliseconds);
  QString text() const;
  int count() const { return m_count; }

private:
  int m_count = 0;
  double m_total = 0.0;
  double m_most = 0.0;
};

// One step in flight at a time, never more queued: a tick while a step is
// computed only notes that another is wanted.
class StepGate {
public:
  // True when a step may start now; it is then in flight until end().
  bool begin();
  void end();
  bool busy() const { return m_busy; }
  // A tick came while one was in flight.
  bool missed() const { return m_missed; }
  int started() const { return m_started; }
  int skipped() const { return m_skipped; }
  void reset();

private:
  bool m_busy = false;
  bool m_missed = false;
  int m_started = 0;
  int m_skipped = 0;
};

// --- The model's commands of the direction (commands.md, "Gravity")

QJsonObject setCommand(const QJsonValue& geometry, const QString& occurrence, bool reverse);
QJsonObject clearCommand();

// --- Where a left press in the view goes while an item hangs

struct PressInput {
  bool dragging = false; // the left button already holds the chain
  bool left = false;     // the left button, no keyboard modifier
  bool viewArea = true;  // not on the orientation cube, not under a panel
  bool onNode = false;   // on a node's handle
  bool onChain = false;  // on the hanging item
};
enum class PressRoute {
  View,      // the view's own: navigation, a pick (the panel's), a context menu
  Swallow,   // another button while dragging: the drag goes on
  TogglePin, // a node's handle: pinned or let go
  Grab,      // the chain grabbed where the press hit it
};
// A left press on the item or a handle is the hanging item's before the
// view's; anywhere else it is the view's, so that the panel's Direction
// From takes a face there.
PressRoute routePress(const PressInput& press);

// --- What is drawn and logged

// The direction on the screen for the marker: its share across (x, to the
// right) and up the screen (y, upwards), and whether it points into the
// screen (away from the viewer). `view` is the camera's direction (from
// the eye into the model), `up` its up.
struct ScreenArrow {
  double x = 0.0;
  double y = 0.0;
  bool away = false;
  // Seen nearly end on: drawn as a dot (towards the viewer) or a cross
  // (away from the viewer) instead of an arrow.
  bool endOn = false;
};
ScreenArrow screenArrow(const Vec3& direction, const Vec3& view, const Vec3& up);

// "0.000,1.000,0.000": a vector in a log line (logs are for tests and
// agents, never shown to a person).
QString vectorText(const Vec3& a, int decimals = 3);
// "points 31, touching 7, pins n1,n2, length 300.0, lowest 12.4 along
// 0.000,0.000,-1.000, at rest yes": the hanging item in a log line;
// `down` is gravity's direction in the item's coordinates.
QString hangText(const Hanging& hanging, const Vec3& down);

} // namespace mitcad::gravity
