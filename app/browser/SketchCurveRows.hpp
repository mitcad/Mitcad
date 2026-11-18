// SPDX-License-Identifier: MIT
// A sketch's curves as Browser rows (mitcad#1978; app/COMMANDS.md,
// "Browser"): while a command's active input takes sketch curves (Patch's
// boundary, Boundary Surface's curves, ...), each sketch's row lists its
// curves under it, so that they can be picked there as in the view. A row
// is named in words, its kind and its number among the sketch's curves of
// that kind in the sketch's order ("Line 1", "Arc 2"), never by the curve's
// id. Plain Qt Core: the rows are read from the model's `sketch` and
// `sketch3d` answers.
#pragma once

#include <QJsonObject>
#include <QString>
#include <QVector>

namespace mitcad {

struct BrowserSketchCurve {
  QString id;       // the curve's id in its sketch (c3): the pick's name, never shown
  QString geometry; // its type (line, arc, ...), as the view's pick of it has it
  QString icon;     // the drawing tool's icon (line, circle, arc-3pt, ...)
  QString name;     // in words: "Line 1"
  bool construction = false;
  bool operator==(const BrowserSketchCurve& other) const {
    return id == other.id && geometry == other.geometry && icon == other.icon && name == other.name &&
           construction == other.construction;
  }
  bool operator!=(const BrowserSketchCurve& other) const { return !(*this == other); }
};

// The curves of a planar sketch, from its `sketch` query answer: its
// entities that are curves with a geometry (as the view draws them), not
// points, texts or the lines that stand for the sketch's axes.
QVector<BrowserSketchCurve> sketchCurveRows(const QJsonObject& sketch);
// The same of a 3D sketch, from its `sketch3d` query answer's curves.
QVector<BrowserSketchCurve> spatialSketchCurveRows(const QJsonObject& sketch);
// A curve's kind in words, as the sketch's item list names it: "Line",
// "Centerline", "Arc", "Fit Point Spline", ..., "Curve" for another type.
QString sketchCurveKind(const QString& type, bool centerline = false);

} // namespace mitcad
