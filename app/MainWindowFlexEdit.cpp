// SPDX-License-Identifier: MIT
// A Flex item's edit session (mitcad#1713; app/COMMANDS.md, "Flex editing";
// flex/FlexEditController.hpp; owner 14.11.2026): Edit Flex in the context
// menu, a double-click on the item in the view, or a double-click on its
// Timeline or Browser row opens the Flex panel on the item and, beside it,
// the item's handles in the view (nodes, "+", insert points, the twist
// ring), both changing one preview; OK keeps everything as one undo step,
// Esc or Cancel takes the whole session back, Undo takes its last change
// back. A single click only selects the item, as any body. Esc and Delete go
// to the handles first (MainWindow::cancel, keyPressEvent).
#include "MainWindow.hpp"

#include <QApplication>
#include <QJsonArray>

#include "commands/Commands.hpp"
#include "flex/FlexEditController.hpp"
#include "framework/CommandSession.hpp"

namespace mitcad {

namespace {

const QString kFlexCommand = QStringLiteral("solid.flex");

} // namespace

void MainWindow::setUpFlexEdit() {
  m_flexEdit = new flex::FlexEditController(*this, *m_viewer, this);
  flex::FlexEditController::Hooks hooks;
  hooks.placement = [this](const QString& uid) { return placementOf(placingOccurrence(uid)).Transformation(); };
  hooks.occurrence = [this](const QString& uid) { return placingOccurrence(uid); };
  hooks.paintPlaces = [this](const QString& source, const QJsonArray& places) {
    hoverNoticePlaces(source, places, {});
  };
  hooks.component = [this](const QString& uid) {
    const DocumentSnapshot::Feature* entry = m_snapshot.feature(uid);
    return entry != nullptr ? entry->component : QString();
  };
  hooks.refuseEdit = [this] { return refuseReadOnly(QStringLiteral("editing a Flex item")); };
  hooks.restoreView = [this] {
    if (m_mode == Mode::Idle) {
      updateIdleView();
    }
  };
  // The edit session's nodes, and the handles' changes into its panel.
  hooks.sessionDefinition = [this]() -> QJsonObject {
    return m_session && m_session->def().id == kFlexCommand ? flexSessionDefinition(m_session->state())
                                                            : QJsonObject();
  };
  hooks.sessionApply = [this](const QJsonArray& nodes, const QString& what) {
    if (!m_session || m_session->def().id != kFlexCommand) {
      return false;
    }
    return m_session->changeInputs(
        [nodes](CommandState& state, const CommandContext& model) { return setFlexSessionNodes(state, model, nodes); },
        QStringLiteral("Flex handles: %1").arg(what));
  };
  m_flexEdit->setHooks(std::move(hooks));
}

QString MainWindow::flexItemOf(const SelectionItem& item) const {
  QString feature;
  switch (item.kind) {
  case SelectKind::Feature:
    feature = item.owner;
    break;
  case SelectKind::Body:
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
    feature = item.owner.section(QLatin1Char('.'), 0, 0);
    break;
  default:
    return QString();
  }
  const DocumentSnapshot::Feature* entry = m_snapshot.feature(feature);
  if (entry == nullptr || entry->type != QStringLiteral("flex") || entry->suppressed ||
      entry->index >= m_snapshot.marker) {
    return QString();
  }
  return feature;
}

QString MainWindow::selectedFlexItem() const {
  // The feature each selected item belongs to: a Timeline or Browser row
  // is the feature itself; a body, or a face, an edge or a vertex of one,
  // the feature that made the body (F7 of F7.b0).
  QString uid;
  if (m_selection.isEmpty() && m_timelineSelection.size() == 1) {
    // A Timeline cell selected that gave the selection nothing of its own.
    const DocumentSnapshot::Feature* entry = m_snapshot.feature(m_timelineSelection.front());
    if (entry != nullptr && entry->type == QStringLiteral("flex") && !entry->suppressed &&
        entry->index < m_snapshot.marker) {
      return entry->uid;
    }
  }
  for (const SelectionItem& item : m_selection) {
    const QString feature = flexItemOf(item);
    if (feature.isEmpty()) {
      return QString(); // something else is selected too
    }
    if (!uid.isEmpty() && uid != feature) {
      return QString(); // two items: neither's
    }
    uid = feature;
  }
  return uid;
}

bool MainWindow::flexDoubleClicked(const Selection& items, Qt::KeyboardModifiers modifiers, quint64 click) {
  // (A click comes in two reports with one number: the second is no click.)
  if (click != 0 && click == m_flexClickNumber) {
    return false;
  }
  const QString uid = items.size() == 1 && modifiers == Qt::NoModifier ? flexItemOf(items.front()) : QString();
  // The second click of a double click as the system classed the mouse
  // events, by their own times (OcctViewer::clickWasDoubleClick), not by
  // when the picks are handled here: the first click's selection may take
  // longer than the double-click interval under load (mitcad#2189: half a
  // second for its card), and the second click's pick then came too late
  // by the clock and took the selection away instead.
  const bool second = !uid.isEmpty() && uid == m_flexClickUid && m_viewer->clickWasDoubleClick();
  m_flexClickNumber = click;
  if (!second) {
    m_flexClickUid = uid;
    return false;
  }
  m_flexClickUid.clear();
  qDebug().noquote() << QStringLiteral("Flex: double-click on %1 opens its edit session").arg(uid);
  editFeature(uid);
  return true;
}

void MainWindow::updateFlexEdit() {
  if (m_flexEdit == nullptr) {
    return;
  }
  // Only in the item's edit session (owner 14.11.2026): a selected item
  // shows no handles.
  const bool session = m_mode == Mode::Command && m_session && m_session->def().id == kFlexCommand &&
                       !m_session->editUid().isEmpty() && isFlexEditSession(m_session->state());
  if (!session) {
    m_flexEdit->hide();
  } else {
    m_flexEdit->show(m_session->editUid(), true);
  }
}

void MainWindow::flexSessionPreviewShown() {
  if (m_flexEdit == nullptr) {
    return;
  }
  if (!m_flexEdit->isActive()) {
    updateFlexEdit(); // the session's first preview: its handles come
    return;
  }
  if (m_flexEdit->inSession()) {
    m_flexEdit->reload();
  }
}

} // namespace mitcad
