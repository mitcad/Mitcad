// SPDX-License-Identifier: MIT
// Selecting the edited sketch's entities by id and bringing them into sight
// (the Sketch Palette's warnings, mitcad#264; live MCP, mitcad#265), and a
// curve's closed loop or open chain by a double-click (mitcad#1541).
#include "SketchController.hpp"

#include <algorithm>
#include <exception>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

#include <QDebug>
#include <QtLogging>

#include <gp_Pnt.hxx>

#include "../view/ViewReveal.hpp"
#include "SketchChains.hpp"

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
      places.push_back(toModel(m_model.frame, point->at));
    } else if (const CurveData* curve = m_model.curve(id)) {
      item.kind = SelectKind::SketchCurve;
      item.geometry = curve->type;
      for (const V2& at : curve->curve.sample(5.0)) {
        places.push_back(toModel(m_model.frame, at));
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
