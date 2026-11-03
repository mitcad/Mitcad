// SPDX-License-Identifier: MIT
// CommandSession: the hover of a tangent chain (mitcad#290). With Tangent
// Chain on, Fillet, Chamfer and the face tools take the whole chain of a
// picked edge or face; the hover shows it before the click. And the point
// under the mouse for a command that previews a click there (mitcad#417),
// and the point of a chosen plane under the mouse anywhere in the view
// (mitcad#1196). The rest of the session is in CommandSession.cpp.
#include <exception>

#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>

#include "CommandSession.hpp"

namespace mitcad {
namespace {

// A query for the chains of a body slower than this is logged as slow; its
// answer is still used (the next hovers over the body only look it up).
constexpr qint64 kSlowChainsMs = 1000;

} // namespace

void CommandSession::hovered(const SelectionItem& item) {
  if (m_done) {
    return;
  }
  m_hovered = item;
  updateHoverChain();
  // What the command shows for the hover (mitcad#375: BUILD's junctions
  // of the profile under the mouse): its markers again.
  if (m_def.hover && m_def.hover(item, m_state, m_host)) {
    updateManipulators();
  }
  // Off the bodies: no point under the mouse for hoverPreview.
  if (!item.isValid()) {
    hoverMoved(item);
  }
}

void CommandSession::hoverMoved(const SelectionItem& item) {
  // A move while the model computes or the view turns is left out: the
  // next one comes soon enough.
  if (m_done || !m_def.hoverPreview || m_host.modelBusy() || m_host.viewer().isNavigating()) {
    return;
  }
  if (m_def.hoverPreview(item, m_state, m_host)) {
    schedulePreview();
  }
}

void CommandSession::pointerMoved(std::optional<QPointF> position) {
  if (m_done || !m_def.cursorPlane || !m_def.cursorPlaneMoved) {
    return;
  }
  m_pointer = position;
  if (m_host.viewer().isNavigating()) {
    return; // the next move comes soon enough
  }
  if (m_host.modelBusy()) {
    // The last place of the mouse once the model is idle: the preview ends
    // where the mouse stopped.
    if (!m_pointerWaits) {
      m_pointerWaits = true;
      m_host.whenIdle(this, [this] {
        m_pointerWaits = false;
        if (!m_done) {
          pointerMoved(m_pointer);
        }
      });
    }
    return;
  }
  const std::optional<gp_Ax3> plane = m_def.cursorPlane(m_state);
  if (!plane) {
    return;
  }
  const std::optional<gp_Pnt> point = position ? m_host.viewer().planePoint(*position, *plane) : std::nullopt;
  if (m_def.cursorPlaneMoved(point, m_state, m_host)) {
    schedulePreview();
  }
}

void CommandSession::clickedAt(const QPointF& position, Qt::KeyboardModifiers modifiers) {
  if (m_done || !m_def.cursorPlane || !m_def.cursorPlaneClicked) {
    return;
  }
  if (m_host.modelBusy()) {
    inputAction([this, position, modifiers] { clickedAt(position, modifiers); });
    return;
  }
  const std::optional<gp_Ax3> plane = m_def.cursorPlane(m_state);
  if (!plane) {
    return; // the pick input takes the click as usual
  }
  const std::optional<gp_Pnt> point = m_host.viewer().planePoint(position, *plane);
  if (!point) {
    qDebug().noquote() << QStringLiteral("%1: the click is not on the plane").arg(m_def.name);
    return;
  }
  ++m_generation;
  if (!m_def.cursorPlaneClicked(*point, m_state, m_host)) {
    return;
  }
  if (m_presenter) {
    m_presenter->refresh(m_state);
  }
  updateHighlights();
  inputsChanged();
  // Shift + click: the point and OK in one, as on a pick input that places
  // (InputDef::shiftClickApplies, mitcad#451).
  if (modifiers.testFlag(Qt::ShiftModifier) && !modifiers.testFlag(Qt::ControlModifier) && !m_done) {
    qDebug().noquote() << QStringLiteral("%1: Shift+click on the plane places at once").arg(m_def.name);
    commit();
  }
}

QStringList CommandSession::hoverChain(const SelectionItem& item) {
  const bool faces = item.kind == SelectKind::Face;
  const QString what = faces ? QStringLiteral("faces") : QStringLiteral("edges");
  const QString key = item.owner + QLatin1Char('/') + what;
  auto known = m_chains.constFind(key);
  if (known == m_chains.cend()) {
    if (m_host.modelBusy()) {
      // Asked again once the model is free; the item alone meanwhile.
      m_host.whenIdle(this, [this] { if (!m_done) { updateHoverChain(); } });
      return {item.name};
    }
    QHash<QString, QStringList> chains;
    QElapsedTimer timer;
    timer.start();
    try {
      QJsonObject request{{QStringLiteral("query"), QStringLiteral("tangent_chains")},
                          {QStringLiteral("body"), item.owner}};
      if (faces) {
        request.insert(QStringLiteral("faces"), true);
      }
      const QJsonArray found = m_host.queryObject(request).value(QStringLiteral("chains")).toArray();
      for (const QJsonValue& value : found) {
        QStringList chain;
        for (const QJsonValue& name : value.toArray()) {
          chain << name.toString();
        }
        for (const QString& name : chain) {
          chains.insert(name, chain);
        }
      }
      const qint64 ms = timer.elapsed();
      qDebug().noquote() << QStringLiteral("Tangent chains of %1 (%2): %3 in %4 ms%5")
                                .arg(item.owner, what)
                                .arg(found.size())
                                .arg(ms)
                                .arg(ms > kSlowChainsMs ? QStringLiteral(" (slow)") : QString());
    } catch (const std::exception& error) {
      // The hover then shows the item alone over this body.
      qWarning().noquote() << QStringLiteral("Tangent chains of %1 (%2) failed after %3 ms: %4")
                                  .arg(item.owner, what)
                                  .arg(timer.elapsed())
                                  .arg(QString::fromUtf8(error.what()));
      chains.clear();
    }
    known = m_chains.insert(key, chains);
  }
  return known->value(item.name, QStringList{item.name});
}

void CommandSession::updateHoverChain() {
  if (m_done) {
    return;
  }
  const InputDef* input = activeInput();
  const SelectionItem& item = m_hovered;
  QStringList chain;
  if (input != nullptr && item.isValid() && !item.owner.isEmpty() && !item.name.isEmpty() &&
      (item.kind == SelectKind::Edge || item.kind == SelectKind::Face) && input->chainKinds.testFlag(item.kind)) {
    chain = !input->chainCheck.isEmpty() && m_state.checked(input->chainCheck) ? hoverChain(item)
                                                                               : QStringList{item.name};
  }
  // The rest of the chain, the item under the mouse left to the view's own
  // hover.
  const QString key = chain.size() > 1 ? QStringList{item.occurrence, item.owner, item.name}.join(QLatin1Char('|'))
                                       : QString();
  if (key != m_chainKey) {
    m_chainKey = key;
    m_chainShape.Nullify();
    if (!key.isEmpty()) {
      BRep_Builder builder;
      TopoDS_Compound compound;
      builder.MakeCompound(compound);
      for (const QString& name : chain) {
        if (name == item.name) {
          continue;
        }
        const TopoDS_Shape shape = m_host.itemShape(SelectionItem{item.kind, item.owner, name, QString(), item.occurrence});
        if (!shape.IsNull()) {
          builder.Add(compound, shape);
        }
      }
      m_chainShape = compound;
    }
    m_host.viewer().setHoverChain(m_chainShape);
  }
  // For UI tests: how many edges or faces the hover shows.
  const QString line =
      chain.isEmpty()
          ? QString()
          : QStringLiteral("Hover chain: %1 %2 of %3")
                .arg(chain.size())
                .arg(item.kind == SelectKind::Face ? QStringLiteral("face(s)") : QStringLiteral("edge(s)"), item.owner);
  if (line != m_loggedHover) {
    m_loggedHover = line;
    if (!line.isEmpty()) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
