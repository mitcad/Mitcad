// SPDX-License-Identifier: MIT
// Selecting the edited sketch's entities by id and bringing them into sight
// (the Sketch Palette's warnings, mitcad#264; live MCP, mitcad#265), and a
// curve's closed loop or open chain by a double-click (mitcad#1541), and
// the curves and points that are not fully constrained (Show
// Unconstrained, mitcad#2026).
#include "SketchController.hpp"

#include <algorithm>
#include <exception>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

#include <QDebug>
#include <QtLogging>

#include <gp_Pnt.hxx>

#include "../framework/CommandRegistry.hpp"
#include "../view/ViewReveal.hpp"
#include "SketchChains.hpp"
#include "SketchOverlay.hpp"

namespace mitcad::sketch {

int SketchController::selectEntities(const QStringList& ids, bool reveal, bool fit) {
  Selection items;
  std::vector<gp_Pnt> places;
  for (const QString& id : ids) {
    SelectionItem item;
    item.owner = m_uid;
    item.name = id;
    if (const PointData* point = m_model.point(id)) {
      item.kind = SelectKind::SketchPoint;
      item.geometry = QStringLiteral("point");
      places.push_back(m_model.place(point->at));
    } else if (const CurveData* curve = m_model.curve(id)) {
      item.kind = SelectKind::SketchCurve;
      item.geometry = curve->type;
      for (const V2& at : curve->curve.sample(5.0)) {
        places.push_back(m_model.place(at));
      }
    } else {
      continue;
    }
    if (!items.contains(item)) {
      items.append(item);
    }
  }
  m_host.setSelection(items);
  if (reveal) {
    const QString moved = revealInView(m_viewer, places, fit);
    if (moved != QStringLiteral("none")) {
      // "Sketch view to the selection: pan", for UI tests.
      qDebug().noquote() << QStringLiteral("Sketch view to the selection: %1").arg(moved);
    }
  }
  return static_cast<int>(items.size());
}

bool SketchController::selectionItemOf(const QString& id, SelectionItem& item) const {
  item = SelectionItem();
  item.owner = m_uid;
  item.name = id;
  if (m_model.point(id) != nullptr) {
    item.kind = SelectKind::SketchPoint;
    item.geometry = QStringLiteral("point");
    item.occurrence = m_host.sketchOccurrence(m_uid);
  } else if (const CurveData* curve = m_model.curve(id)) {
    item.kind = SelectKind::SketchCurve;
    item.geometry = curve->type;
    item.occurrence = m_host.sketchOccurrence(m_uid);
  } else if (m_model.constraint(id) != nullptr) {
    item.kind = SelectKind::SketchConstraint;
  } else if (m_model.dimension(id) != nullptr) {
    item.kind = SelectKind::SketchDimension;
  } else if (m_model.text(id) != nullptr) {
    // A text, as a click on its outline selects it (mitcad#1750).
    item.kind = SelectKind::SketchCurve;
    item.occurrence = m_host.sketchOccurrence(m_uid);
  } else {
    return false;
  }
  return true;
}

bool SketchController::selectItem(const QString& id, Qt::KeyboardModifiers modifiers) {
  return selectItems(QStringList{id}, modifiers);
}

bool SketchController::selectItems(const QStringList& ids, Qt::KeyboardModifiers modifiers) {
  Selection items;
  for (const QString& id : ids) {
    SelectionItem item;
    if (selectionItemOf(id, item) && !items.contains(item)) {
      items.append(item);
    }
  }
  if (items.isEmpty()) {
    return false;
  }
  Selection next;
  if (modifiers & (Qt::ControlModifier | Qt::ShiftModifier)) {
    next = m_host.selection();
    const bool all = std::all_of(items.cbegin(), items.cend(),
                                 [&next](const SelectionItem& item) { return next.contains(item); });
    for (const SelectionItem& item : std::as_const(items)) {
      if (all) {
        next.removeAll(item);
      } else if (!next.contains(item)) {
        next.append(item);
      }
    }
  } else {
    next = items;
  }
  // The view draws them (refreshView) and the list marks their cells.
  m_host.setSelection(next);
  return true;
}

QStringList SketchController::unconstrainedEntities() const {
  QStringList ids;
  // The axes and the origin stand for the sketch's frame: never free.
  for (const CurveData& curve : m_model.curves) {
    if (!curve.constrained && curve.axis.isEmpty()) {
      ids << curve.id;
    }
  }
  for (const PointData& point : m_model.points) {
    if (!point.constrained && point.axis.isEmpty()) {
      ids << point.id;
    }
  }
  return ids;
}

int SketchController::selectUnconstrained() {
  const QStringList ids = unconstrainedEntities();
  int curves = 0;
  int points = 0;
  for (const QString& id : ids) {
    if (m_model.point(id) != nullptr) {
      ++points;
    } else {
      ++curves;
    }
  }
  // As clicks would select them: Delete, Fix and the constraints take them.
  if (ids.isEmpty() || !selectItems(ids, Qt::NoModifier)) {
    m_host.setSelection({});
  }
  m_overlay->update();
  // "Show Unconstrained: 1 curve(s), 2 point(s): c4, p5, p6", for UI tests.
  qDebug().noquote() << QStringLiteral("Show Unconstrained: %1 curve(s), %2 point(s): %3")
                            .arg(curves)
                            .arg(points)
                            .arg(ids.isEmpty() ? QStringLiteral("none") : ids.join(QStringLiteral(", ")));
  // In words, without ids (docs/ui-conventions.md, What a message says).
  QString message;
  if (ids.isEmpty()) {
    message = m_model.conflicts.empty() && m_model.error.isEmpty()
                  ? tr("Everything in the sketch is fully constrained.")
                  : tr("Nothing selected: the sketch's constraints conflict, so what is free is not known.");
  } else if (points == 0) {
    message = tr("Selected %n curve(s) that are not fully constrained.", nullptr, curves);
  } else if (curves == 0) {
    message = tr("Selected %n point(s) that are not fully constrained.", nullptr, points);
  } else {
    message = tr("Selected what is not fully constrained: %1 and %2.")
                  .arg(tr("%n curve(s)", nullptr, curves), tr("%n point(s)", nullptr, points));
  }
  m_host.showStatus(message, NoticeLevel::Information);
  return curves + points;
}

const PatternData* SketchController::patternOfStep(const QString& kind, const QStringList& items) const {
  if (kind != QStringLiteral("rectangular_pattern") && kind != QStringLiteral("circular_pattern") &&
      kind != QStringLiteral("edit_pattern")) {
    return nullptr;
  }
  // By its copies: SketchModel::patternOf also matches the originals.
  for (const QString& id : items) {
    for (const PatternData& pattern : m_model.patterns) {
      if (pattern.copies.contains(id)) {
        return &pattern;
      }
    }
  }
  return nullptr;
}

bool SketchController::selectStepPattern(const QString& kind, const QStringList& items) {
  const PatternData* pattern = patternOfStep(kind, items);
  if (pattern == nullptr) {
    return false;
  }
  QString copy;
  for (const QString& id : pattern->copies) {
    if (m_model.curve(id) != nullptr) {
      copy = id;
      break;
    }
    if (copy.isEmpty() && m_model.point(id) != nullptr) {
      copy = id;
    }
  }
  return !copy.isEmpty() && selectItems(QStringList{copy}, Qt::NoModifier);
}

bool SketchController::editStepPattern(const QString& kind, const QStringList& items) {
  if (!selectStepPattern(kind, items)) {
    return false;
  }
  const Selection& selected = m_host.selection();
  const QString command = selected.size() == 1 ? editCommandOf(selected.first()) : QString();
  if (command.isEmpty()) {
    return false;
  }
  qDebug().noquote() << QStringLiteral("Double-click on %1: %2").arg(selected.first().name, command);
  m_host.trigger(command);
  return true;
}

QString SketchController::editCommandOf(const SelectionItem& item) const {
  const SelectKind kind = item.kind;
  if (item.owner == m_uid && kind == SelectKind::SketchConstraint) {
    return constraintEditCommandOf(item.name);
  }
  if (item.owner != m_uid ||
      (kind != SelectKind::SketchCurve && kind != SelectKind::SketchPoint && kind != SelectKind::Profile)) {
    return {};
  }
  // A region a text's contours bound (inside a letter or its counter,
  // mitcad#1369) stands for its text.
  if (const TextData* text = m_model.textOfPick(item); text != nullptr) {
    return text->code ? QStringLiteral("sketch.edit_code") : QStringLiteral("sketch.edit_text");
  }
  if (kind == SelectKind::Profile) {
    return {};
  }
  // An equation curve opens with its formulas (mitcad#2237).
  if (kind == SelectKind::SketchCurve && m_model.equationOf(item.name) != nullptr) {
    return QStringLiteral("sketch.edit_equation_curve");
  }
  if (m_model.offsetOf(item.name) != nullptr) {
    return QStringLiteral("sketch.edit_offset");
  }
  if (m_model.patternOf(item.name) != nullptr) {
    return QStringLiteral("sketch.edit_pattern");
  }
  // A conic's Rho (mitcad#2236).
  if (const CurveData* curve = m_model.curve(item.name);
      kind == SelectKind::SketchCurve && curve != nullptr && curve->isConic() && !curve->reference) {
    return QStringLiteral("sketch.edit_rho");
  }
  return {};
}

QString SketchController::constraintEditCommandOf(const QString& id) const {
  const ConstraintData* k = m_model.constraint(id);
  return k != nullptr && k->type == QStringLiteral("equal_spacing") ? QStringLiteral("sketch.edit_equal_spacing")
                                                                     : QString();
}

QString SketchController::spacingOfStep(const QString& kind, const QStringList& items) const {
  if (kind != QStringLiteral("add_equal_spacing") && kind != QStringLiteral("edit_equal_spacing")) {
    return {};
  }
  for (const QString& id : items) {
    if (const ConstraintData* k = m_model.constraint(id); k != nullptr && k->type == QStringLiteral("equal_spacing")) {
      return id;
    }
  }
  return {};
}

bool SketchController::selectStepSpacing(const QString& kind, const QStringList& items) {
  const QString id = spacingOfStep(kind, items);
  return !id.isEmpty() && selectItems(QStringList{id}, Qt::NoModifier);
}

bool SketchController::editStepSpacing(const QString& kind, const QStringList& items) {
  if (!selectStepSpacing(kind, items)) {
    return false;
  }
  const Selection& selected = m_host.selection();
  const QString command = selected.size() == 1 ? editCommandOf(selected.first()) : QString();
  if (command.isEmpty()) {
    return false;
  }
  qDebug().noquote() << QStringLiteral("Double-click on %1: %2").arg(selected.first().name, command);
  m_host.trigger(command);
  return true;
}

QAction* SketchController::commandAction(const QString& id) const { return m_registry.action(id); }

void SketchController::setHoverAnnotation(const QString& id) {
  if (id != m_hoverAnnotation) {
    m_hoverAnnotation = id;
    m_overlay->update();
  }
}

bool SketchController::selectChainAt(const QPointF& screen, Qt::KeyboardModifiers modifiers) {
  const QString id = entityAt(screen, PickCurves);
  const CurveData* picked = m_model.curve(id);
  // A text, a pattern's or an offset's curve and a spline that takes a
  // point keep their own double-click; the axes are no chain.
  if (picked == nullptr || !m_model.axisOf(id).isEmpty() || m_model.text(id) != nullptr ||
      m_model.offsetOf(id) != nullptr || m_model.patternOf(id) != nullptr ||
      (picked->isSpline() && splineAt(screen) == id)) {
    return false;
  }
  const Selection& selected = m_host.selection();
  // Where the view places the sketch's curves: as the first click picked
  // the curve, else as another curve of the sketch, else the sketch's.
  QString occurrence = m_host.sketchOccurrence(m_uid);
  bool clicked = false;
  for (const SelectionItem& item : selected) {
    if (item.owner == m_uid && item.kind == SelectKind::SketchCurve && !clicked) {
      occurrence = item.occurrence;
      clicked = item.name == id;
    }
  }
  const SelectionItem probe{SelectKind::SketchCurve, m_uid, id, picked->type, occurrence};
  if (!m_host.sketchClickable(probe, modifiers.testFlag(Qt::ShiftModifier))) {
    return false;
  }
  // The smallest closed loop the curve bounds, by the model (it finds the
  // loop as profiles are found, past points where more curves meet).
  QStringList loop;
  try {
    const QJsonObject answer = m_host.queryObject({{QStringLiteral("query"), QStringLiteral("sketch_loop")},
                                                   {QStringLiteral("sketch"), m_uid},
                                                   {QStringLiteral("curve"), id}});
    for (const QJsonValue& value : answer.value(QStringLiteral("curves")).toArray()) {
      if (m_model.curve(value.toString()) != nullptr) {
        loop << value.toString();
      }
    }
  } catch (const std::exception& failure) {
    // The open chain then.
    qDebug().noquote() << QStringLiteral("sketch_loop of %1: %2").arg(id, QString::fromUtf8(failure.what()));
  }
  const QStringList chain = doubleClickChain(m_model, id, loop);
  const bool adding = modifiers.testFlag(Qt::ControlModifier) || modifiers.testFlag(Qt::ShiftModifier);
  Selection items = adding ? selected : Selection();
  for (const QString& curve : chain) {
    const CurveData* c = m_model.curve(curve);
    if (c == nullptr) {
      continue;
    }
    const SelectionItem item{SelectKind::SketchCurve, m_uid, curve, c->type, occurrence};
    if (!items.contains(item)) {
      items.append(item);
    }
  }
  m_host.setSelection(items);
  // "Double-click on c1: loop c1, c2, c3, c4", for UI tests: the curves
  // by their numbers.
  QStringList named = chain;
  std::sort(named.begin(), named.end(), [](const QString& a, const QString& b) {
    return a.mid(1).toInt() < b.mid(1).toInt();
  });
  qDebug().noquote() << QStringLiteral("Double-click on %1: %2 %3%4")
                            .arg(id, loop.isEmpty() ? QStringLiteral("chain") : QStringLiteral("loop"),
                                 named.join(QStringLiteral(", ")),
                                 adding ? QStringLiteral(" (added)") : QString());
  return true;
}

} // namespace mitcad::sketch
