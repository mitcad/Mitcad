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
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>

#include "CommandSession.hpp"
#include "Json.hpp"

namespace mitcad {
namespace {

// A query for the chains of a body slower than this is logged as slow; its
// answer is still used (the next hovers over the body only look it up).
constexpr qint64 kSlowChainsMs = 1000;

// The pointer has rested this long after a light preview: the feature's
// own preview is computed (CommandDef::lightPreview, mitcad#1713).
constexpr int kPointerRestMs = 350;
// The pointer has rested this long: the light preview of a command whose
// light preview is of the pointer alone (CommandDef::lightPreviewLive).
constexpr int kLightRestMs = 300;

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
  // Nor once OK waits for the model (resumePendingCommit): the item ends
  // where it was confirmed, not where the pointer went after.
  if (m_done || m_commitRequested || !m_def.hoverPreview || m_host.modelBusy() || m_host.viewer().isNavigating()) {
    return;
  }
  if (m_def.hoverPreview(item, m_state, m_host)) {
    pointerPreview();
  }
}

void CommandSession::pointerMoved(std::optional<QPointF> position) {
  if (m_done || !m_def.cursorPlane || !m_def.cursorPlaneMoved) {
    return;
  }
  m_pointer = position;
  if (m_host.viewer().isNavigating() || m_commitRequested) {
    return; // the next move comes soon enough; after OK, none is wanted
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
    pointerPreview();
  }
}

void CommandSession::pointerPreview() {
  if (!m_def.lightPreview) {
    schedulePreview();
    return;
  }
  if (m_done || m_cancelRequested || m_committing || m_commitRequested || m_suspended) {
    return;
  }
  if (m_def.lightPreviewLive) {
    // Of the pointer alone (owner 14.11.2026): the feature's preview stays;
    // what costs nothing now, the route at once when live, else once the
    // pointer rests (each move starts the wait again).
    if (m_def.lightPreviewMoved) {
      m_def.lightPreviewMoved(m_state, m_host);
    }
    m_lightRestTimer.stop();
    if (m_def.lightPreviewLive(m_state, m_host) && !m_host.modelBusy() && !m_asyncPending) {
      runLightPreview();
    } else {
      m_lightRestTimer.start(kLightRestMs);
      updateManipulators();
    }
    return;
  }
  // The feature's own preview once the pointer rests: each move starts
  // the wait again.
  m_refineTimer.stop();
  m_previewTimer.start(kPointerRestMs);
  if (m_host.modelBusy() || m_asyncPending) {
    return; // the next move, or the rest, shows it
  }
  if (!m_shownKey.isEmpty()) {
    // The feature's preview is of where the pointer was.
    clearPreview();
    m_shownKey.clear();
    m_previewOk = false;
  }
  runLightPreview();
}

void CommandSession::runLightPreview() {
  const Built built = m_def.lightPreview(m_state, m_host);
  bool ok = false;
  QString error = built.error;
  if (error.isEmpty()) {
    const QJsonObject command = isEditing() ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
                                                          {QStringLiteral("uid"), m_editUid},
                                                          {QStringLiteral("def"), built.def}}
                                            : addFeatureCommand(built);
    try {
      m_host.modelPreview(compactJson(command), m_def.name);
      ok = true;
    } catch (const std::exception& failure) {
      error = errorText(failure);
    }
  }
  if (m_def.lightPreviewShown) {
    m_def.lightPreviewShown(ok, error, m_state, m_host);
  }
  if (ok) {
    try {
      m_host.clearModelPreview(); // the command has read it
    } catch (const std::exception&) {
      // Nothing was held then.
    }
  }
  updateManipulators();
}

// The view reports a click twice, here and by `picked` after it, both with
// the click's number; each is queued, and either may wait for the model
// (mitcad#1713: a click while the pointer's preview computed was taken by
// both, the second time as a click on the node the first had placed, which
// ended the item). Whichever takes the click marks its number, and the
// other report then leaves it.
void CommandSession::clickedAt(const QPointF& position, Qt::KeyboardModifiers modifiers, quint64 click,
                               const Selection& items) {
  if (m_done || !m_def.cursorPlane || !m_def.cursorPlaneClicked) {
    return;
  }
  if (m_host.modelBusy()) {
    inputAction([this, position, modifiers, click, items] { clickedAt(position, modifiers, click, items); });
    return;
  }
  if (click != 0 && click == m_clickTaken) {
    return; // its pick took it
  }
  const std::optional<gp_Ax3> plane = m_def.cursorPlane(m_state);
  if (!plane) {
    return; // the pick input takes the click as usual
  }
  const InputDef* input = activeInput();
  // A command whose plane is only for clicks off the model (Flex's free
  // nodes, mitcad#1713): what the click itself picked, if the input takes
  // it, is the click's, which `picked` takes (the hover that would have
  // said so was not known yet).
  if (input != nullptr) {
    for (const SelectionItem& item : items) {
      if (item.at && acceptsNow(input, item) && referable(item)) {
        qDebug().noquote() << QStringLiteral("%1: the click picked %2").arg(m_def.name, item.logDescription());
        return;
      }
    }
  }
  // What the pick input takes under the click is its pick, also when the
  // click itself picked nothing (the view's detection is cleared while the
  // preview under a resting pointer is computed).
  if (input != nullptr && items.isEmpty()) {
    if (const std::optional<SelectionItem> item = m_host.viewer().itemAt(position);
        item && item->at && acceptsNow(input, *item)) {
      qDebug().noquote() << QStringLiteral("%1: the click on %2 is its pick").arg(m_def.name, item->logDescription());
      m_clickTaken = click;
      picked(Selection{*item}, modifiers, false);
      return;
    }
  }
  const std::optional<gp_Pnt> point = m_host.viewer().planePoint(position, *plane);
  if (!point) {
    qDebug().noquote() << QStringLiteral("%1: the click is not on the plane").arg(m_def.name);
    return;
  }
  m_clickTaken = click;
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
  // (InputDef::shiftClickApplies, mitcad#451). Not where Shift is the live
  // light preview's (CommandDef::lightPreviewLive, Flex: owner 14.11.2026).
  if (modifiers.testFlag(Qt::ShiftModifier) && !modifiers.testFlag(Qt::ControlModifier) && !m_done &&
      !m_def.lightPreviewLive) {
    qDebug().noquote() << QStringLiteral("%1: Shift+click on the plane places at once").arg(m_def.name);
    commit();
    return;
  }
  // A click that ends the command (CommandDef::clickConfirms, mitcad#1713:
  // a double click at a route's last node).
  if (!m_done && m_def.clickConfirms && m_def.clickConfirms(m_state)) {
    qDebug().noquote() << QStringLiteral("%1: the click on the plane confirms").arg(m_def.name);
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
  // A free edge with Whole Opening on (mitcad#1955): the hover shows the
  // whole opening a click takes, the edge under the pointer the view's own.
  if (input != nullptr && isFreeEdge(item) && input->takesWholeOpenings(m_state)) {
    const SelectionItem whole = wholeOpeningItem(item);
    const QString key = QStringList{item.occurrence, item.owner, whole.name}.join(QLatin1Char('|'));
    if (key != m_chainKey) {
      m_chainKey = key;
      m_chainShape = m_host.itemShape(whole);
      m_host.viewer().setHoverChain(m_chainShape);
    }
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    if (!m_chainShape.IsNull()) {
      TopExp::MapShapes(m_chainShape, TopAbs_EDGE, edges);
    }
    const QString line = QStringLiteral("Hover opening: %1 edge(s) of %2").arg(edges.Extent()).arg(item.owner);
    if (line != m_loggedHover) {
      m_loggedHover = line;
      qDebug().noquote() << line;
    }
    return;
  }
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
