// SPDX-License-Identifier: MIT
#pragma once

// The component a Move/Copy of components works in (mitcad#2302): a
// component picked by its browser row deeper than the move's component
// (the active one) moves in its own parent component, where the model
// lets a `move_occurrence` move it; and the occurrence of that component
// whose placement is the move's frame. Plain Qt Core on the `components`
// query's occurrence tree (`occurrences`: nodes with `uid`, `component`,
// `world` and `children`), so that it is tested without a model.

#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad::cmd {

// The node of the tree at an occurrence path ("O1/O4"); empty when the
// path leads nowhere (and for "", the root, which has no node).
QJsonObject occurrenceNodeAt(const QJsonArray& tree, const QString& path);

// The component placed at the path; "C0" (the root) for "", empty when
// the path leads nowhere.
QString componentAtPath(const QJsonArray& tree, const QString& path);

// The path of the occurrence a path's occurrence is placed in: the path
// without its last step ("" for an occurrence placed in the root).
QString parentPathOf(const QString& path);

// How many occurrences of the component the tree shows (every place it
// is seen at, through the occurrences of its parents too).
int occurrenceCountOf(const QJsonArray& tree, const QString& component);

// Where a Move/Copy of the occurrences at `paths` works, `home` being the
// component it starts in (the active one):
// - an occurrence placed directly in `home` moves there;
// - one whose path passes through `home` deeper (a component picked by its
//   browser row inside a sub-assembly) moves in the component it is
//   placed in, its parent: `component` is that parent, `parentPath` the
//   occurrence of the parent on the picked path;
// - one whose path does not pass through `home` stays with `home` (the
//   model then says why it cannot move it).
// `mixed`: the occurrences need different components, which one feature
// cannot move.
struct MoveTarget {
  QString component;
  QString parentPath;
  bool mixed = false;
};
MoveTarget moveTargetOf(const QStringList& paths, const QString& home, const QJsonArray& tree);

// The occurrence path whose placement (`world`) is the frame of a move in
// `component`: the parent occurrence of the first of `paths` placed in
// `component` (the occurrence on the picked path, so that a component
// placed twice is drawn where it was picked), else the component's first
// occurrence in the tree; empty for the root or when the tree has none.
QString frameOccurrenceOf(const QStringList& paths, const QString& component, const QJsonArray& tree);

// --- The paths a move's definition records (mitcad#2307)

// The `<key>_occurrence` a move's point or axis picked at `path` is
// written with, for a move in `component`: none for geometry of that
// component (its own, at an occurrence of it), else the path, "" (the
// root) for a root-level pick of a move that works in a deeper component,
// so that the model reads it in the design's coordinates, not as the
// move's own component's.
std::optional<QString> pickedOccurrenceToWrite(const QString& path, const QString& component, const QJsonArray& tree);

// A `move_occurrence`'s `parent_occurrence`: the occurrence of
// `component` (the moved occurrences' parent) the first of `paths` is
// placed in, when that component is used more than once, so that the
// model takes root-level points through the occurrence the parts were
// picked in; empty for the root, a component used once, or no such path.
QString parentOccurrenceToWrite(const QStringList& paths, const QString& component, const QJsonArray& tree);

// A moved occurrence's path as an edit loads it: `uid` under the recorded
// `parentOccurrence` when the tree has it there, else `fallback` (its
// first path), so that the edit keeps the occurrence it was picked in.
QString loadedOccurrencePath(const QString& uid, const QString& parentOccurrence, const QString& fallback,
                             const QJsonArray& tree);

} // namespace mitcad::cmd
