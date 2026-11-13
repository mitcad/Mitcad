// SPDX-License-Identifier: MIT
#pragma once

// Paths along curves of 3D sketches in the Pipe and Sweep panels
// (mitcad#1527; app/COMMANDS.md, "Pipe and Sweep along 3D sketches") and
// every other path input (mitcad#1550: Loft, Pattern on Path, the
// construction features along a path): the
// typed path the model takes (commands.md, "Pipe and Sweep along 3D sketch
// curves"), the direction an edited path keeps, which picked curves can be
// path pieces, the route a whole-sketch pick takes and the panel's listing
// of a route. Plain Qt Core, read from the model's JSON answers.

#include <optional>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>

namespace mitcad::cmd {

// A path along curves of a 3D sketch as the model takes it:
// {"spatial_sketch", "curves"} with "chain" and "reversed" only when set.
QJsonObject spatialPath(const QString& sketch, const QStringList& curves, bool chain, bool reversed);

// The parts of such a path; none for another path form.
struct SpatialPathParts {
  QString sketch;
  QStringList curves;
  bool chain = false;
  bool reversed = false;
};
std::optional<SpatialPathParts> spatialPathParts(const QJsonValue& path);

// The direction an edited path keeps (mitcad#1527): its first curve and
// whether it runs against it, {"first", "reversed"}. The path runs through
// its first listed curve; the model keeps that curve and the direction
// across edits, and so does the panel while the first picked curve stays
// the same.
QJsonObject keptDirection(const SpatialPathParts& parts);
// Whether a path of `curves` runs reversed by the kept direction: only
// while its first curve is the kept one.
bool reversedBy(const QJsonObject& kept, const QStringList& curves);

// Why a curve of a 3D sketch (a curve of the `sketch3d` query) is no path
// piece, in the model's words: a reference curve; empty for every curve
// the sketch draws (a closed one, a circle, an ellipse or a closed spline,
// is a closed path by itself, mitcad#1550). Construction curves are pieces
// when they are picked.
QString notAPiece(const QJsonObject& curve);
// The curve `id` of a `sketch3d` answer; empty when it has none.
QJsonObject spatialCurve(const QJsonObject& sketch3d, const QString& id);

// The curves a pick of the whole 3D sketch takes (a Browser row or the
// timeline): its one chain without branches, open or closed (mitcad#1550),
// in order from its start (the `sketch3d` answer's `chains`). Empty, with
// the reason in `why`, when it has no such chain or more than one chain.
QStringList wholeRoute(const QJsonObject& sketch3d, QString* why = nullptr);

// The panel's listing of a route (the `sketch3d` query's `route`, from
// `path`): its curves in order, a long one shortened, and where it starts
// and ends, "c3, c6, c9 (p1 to p8)", a closed one "c3, c4, c5 (closed,
// from p1)" or "c7 (closed)"; for curves that are no path, "Not one path".
// The model's reason is in `why`.
QString routeText(const QJsonObject& route, QString* why = nullptr);
// The same route in words, as the panel shows it (mitcad#1741): its number
// of curves and its sketch's name, "5 curves of 3DSketch1", a closed one
// "3 curves of 3DSketch1, closed"; "Not one path". Never a curve's or a
// point's id: routeText is the log's.
QString routeWords(const QJsonObject& route, const QString& sketchName);

} // namespace mitcad::cmd
