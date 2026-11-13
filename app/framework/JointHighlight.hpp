// SPDX-License-Identifier: MIT
#pragma once

// The joint highlight (mitcad#523; app/COMMANDS.md, "Joints"): what the
// view shows of a selected joint, read from the `joints` query's entry of
// it (commands.md, "What each side is on"). Plain Qt Core: the main window
// makes the shapes and draws them (MainWindowJointHighlight.cpp).

#include <array>
#include <functional>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// A side's frame in the coordinates of the joint's component.
struct JointFrame {
  std::array<double, 3> origin{};
  std::array<double, 3> x{};
  std::array<double, 3> y{};
  std::array<double, 3> z{};
};

// One side of a joint as the view shows it.
struct JointSide {
  // "a", "b", "origin" (an as-built joint's motion origin); a constraint
  // set's rows "a", "b", "a2", "b2" with `row` set.
  QString key;
  int row = 0;
  // The model's `on`: face, edge, vertex, body, sketch_curve, sketch_point,
  // origin, construction, joint_origin, fixed, mount, occurrence.
  QString on;
  // The reference as the definition stores it.
  QJsonValue geometry;
  // The occurrence path from the root that places the side's geometry
  // ("O5/O1"); the joint's placing occurrence for its component's own
  // geometry (empty in the root).
  QString path;
  // The side's own path from the joint's component, by names ("Pin:1",
  // "Arm:1/Pin:2") and uids; empty for the component's own geometry.
  QString occurrenceName;
  QStringList ownPath;
  bool frameOverride = false;
  // What is drawn: the geometry itself (false: a fixed reference or an
  // override that moves the origin or turns the axes shows its frame
  // only); the occurrence the side is (an as-built joint's) or the part a
  // mount frame belongs to.
  bool showsGeometry = true;
  bool showsPart = false;
  // Where the model resolved it at the marker; none for a joint that did
  // not recompute and for constraint rows.
  std::optional<JointFrame> frame;
};

struct JointHighlight {
  QString uid;
  QString name;
  // rigid, revolute, ...; "constraint" for a constraint set.
  QString kind;
  // The occurrence that places the joint's component in the view ("" for
  // the root's joints).
  QString placing;
  QVector<JointSide> sides;
  // The joined occurrences, paths from the root, each once, a's first;
  // their names from the joint's component.
  QStringList occurrences;
  QStringList occurrenceNames;

  bool isValid() const { return !uid.isEmpty(); }
};

// The highlight of a `joints` query entry: a joint, an as-built joint or a
// constraint set (its rows' references); `placing` is the occurrence that
// places the joint's component, as the selected item has it.
JointHighlight jointHighlightOf(const QJsonObject& entry, const QString& placing);

// `placing` and a path of uids below it: "O5" and ["O1", "O4"] give
// "O5/O1/O4"; an empty path gives `placing`.
QString joinedPath(const QString& placing, const QStringList& path);

// What a side is on, for the log: "face F4:start(r{...}) of F4.b0", "edge
// E{...} of F2.b0", "origin plane xy", "joint origin F9", "fixed point",
// "mount frame end_a of F7", "occurrence".
QString jointSideGeometryText(const JointSide& side);

// The log lines of a joint's highlight (UI tests): per side "Joint
// highlight a: face F4:start(...) of F4.b0 in Pin:1 (O2) [Joint1]" (", frame
// only", ", with its part" when so; a constraint row's sides "row 1 a"),
// then "Joint highlight occurrences: Pin:1 (O2), Plate:1 (O1) [Joint1]".
QStringList jointHighlightLog(const JointHighlight& joint);

// The selection card's text of a joint (rich text) in words (mitcad#1741):
// "<b>Joint1</b>: revolute, a = Pin:1, face of Body2, b = Plate:1, face of
// Body1"; `name` names a body or feature uid (empty when it cannot: "a
// body", "a sketch"). Never a topological name or an id.
QString jointCardText(const JointHighlight& joint, const std::function<QString(const QString&)>& name);

} // namespace mitcad
