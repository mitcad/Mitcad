// SPDX-License-Identifier: MIT
// The sketch's origin and its x and y axes as picks of the tools that
// constrain and dimension (mitcad#500): the entities the model makes for
// them (`sketch.axis`, core/sketch/src/axes.rs), and the pick ids
// that stand for them until a step makes them. Only the sketch model's
// data: the app's unit tests take this file alone. With no tool running
// they are selected like construction geometry (mitcad#899): the selection
// keeps them by their pick ids, and the tools that take them make their
// entities when they use them. The drawing tools snap onto the axes
// (mitcad#948, SketchSnap.cpp); a point placed there is held on the axis's
// entity, made in the same step.
#include "SketchModel.hpp"

#include <cmath>

#include <QPointF>

#include "../framework/Selection.hpp"

namespace mitcad::sketch {

QString SketchModel::originPoint() const {
  for (const PointData& p : points) {
    if (p.fixed && p.axis.isEmpty() && norm(p.at) < 1e-9) {
      return p.id;
    }
  }
  return QString();
}

QString SketchModel::axisLine(const QString& axis) const {
  for (const CurveData& c : curves) {
    if (c.axis == axis && c.isLine()) {
      return c.id;
    }
  }
  return QString();
}

QString SketchModel::axisOf(const QString& id) const {
  for (const CurveData& c : curves) {
    if (c.id == id) {
      return c.axis;
    }
  }
  if (id == axisPick(QStringLiteral("x"))) {
    return QStringLiteral("x");
  }
  if (id == axisPick(QStringLiteral("y"))) {
    return QStringLiteral("y");
  }
  return QString();
}

bool SketchModel::isAxisOrOrigin(const QString& id) const {
  if (id.isEmpty()) {
    return false;
  }
  return id == originPick() || !axisOf(id).isEmpty() || id == originPoint();
}

void SketchModel::addAxisPicks() {
  QString origin = originPoint();
  if (origin.isEmpty()) {
    PointData p;
    p.id = originPick();
    p.fixed = true;
    p.constrained = true;
    points.push_back(p);
    origin = p.id;
  }
  for (const QString& axis : {QStringLiteral("x"), QStringLiteral("y")}) {
    if (!axisLine(axis).isEmpty()) {
      continue;
    }
    const V2 end = axis == QStringLiteral("x") ? V2{kAxisLineLength, 0.0} : V2{0.0, kAxisLineLength};
    CurveData c;
    c.id = axisPick(axis);
    c.type = QStringLiteral("line");
    c.curve = Curve2::line(V2{}, end);
    c.start = origin;
    c.end = c.id + QStringLiteral(":end");
    c.points << c.start << c.end;
    c.construction = true;
    c.fixed = true;
    c.constrained = true;
    c.axis = axis;
    PointData far;
    far.id = c.end;
    far.at = end;
    far.fixed = true;
    far.constrained = true;
    far.axis = axis;
    points.push_back(far);
    curves.push_back(c);
  }
}

QString SketchModel::axisEntity(const QString& id) const {
  const QString axisName = axisPickName(id);
  if (axisName.isEmpty()) {
    return id;
  }
  const QString made = axisName == QStringLiteral("origin") ? originPoint() : axisLine(axisName);
  return made.isEmpty() ? id : made;
}

QString SketchModel::axisSelectionPick(const QString& entity) const {
  if (entity.isEmpty()) {
    return QString();
  }
  if (entity == originPick()) {
    return entity;
  }
  const QString axis = axisOf(entity);
  return axis.isEmpty() ? QString() : axisPick(axis);
}

QStringList SketchModel::selectionPicks(const QStringList& names, bool axes) const {
  QStringList picks;
  for (const QString& pick : names) {
    if (!isAxisPick(pick)) {
      picks << pick;
    } else if (axes) {
      picks << axisEntity(pick);
    }
  }
  return picks;
}

QString SketchModel::pointAxes(const QString& id) const {
  if (id.isEmpty()) {
    return QString();
  }
  if (id == originPoint()) {
    return QStringLiteral("xy");
  }
  bool onX = false;
  bool onY = false;
  for (const ConstraintData& k : constraints) {
    if (!k.active || k.type != QStringLiteral("coincident") || !k.refs.contains(id)) {
      continue;
    }
    for (const QString& ref : k.refs) {
      if (ref == id) {
        continue;
      }
      const QString axis = axisOf(ref);
      onX = onX || axis == QStringLiteral("x");
      onY = onY || axis == QStringLiteral("y");
    }
  }
  QString axes;
  if (onX) {
    axes += QLatin1Char('x');
  }
  if (onY) {
    axes += QLatin1Char('y');
  }
  return axes;
}

QString originPick() { return QStringLiteral("origin"); }

QString axisPick(const QString& axis) { return axis + QStringLiteral("_axis"); }

bool isAxisPick(const QString& id) { return !axisPickName(id).isEmpty(); }

QString axisPickName(const QString& id) {
  if (id == originPick()) {
    return QStringLiteral("origin");
  }
  if (id == axisPick(QStringLiteral("x"))) {
    return QStringLiteral("x");
  }
  if (id == axisPick(QStringLiteral("y"))) {
    return QStringLiteral("y");
  }
  return QString();
}

SelectionItem axisSelectionItem(const QString& sketch, const QString& pick, const QString& occurrence) {
  SelectionItem item;
  const bool origin = pick == originPick();
  item.kind = origin ? SelectKind::SketchPoint : SelectKind::SketchCurve;
  item.owner = sketch;
  item.name = pick;
  item.geometry = origin ? QStringLiteral("point") : QStringLiteral("line");
  item.occurrence = occurrence;
  return item;
}

bool isAxisSelection(const SelectionItem& item) {
  return (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) && isAxisPick(item.name);
}

std::optional<double> axisScreenDistance(const QPointF& screen, const QPointF& origin, const QPointF& along) {
  const double length = std::hypot(along.x(), along.y());
  if (length < 1.0) {
    return std::nullopt; // the sketch plane seen edge on
  }
  const QPointF off = screen - origin;
  return std::abs(along.x() * off.y() - along.y() * off.x()) / length;
}

} // namespace mitcad::sketch
