// SPDX-License-Identifier: MIT
// CommandSession: Ctrl (Cmd on macOS) held alone hides a feature command's
// preview while its inputs are picked, and its release shows it again
// (mitcad#1421; the keys are read by framework/PreviewHold). The rest of the
// session is in CommandSession.cpp.
#include <cmath>
#include <utility>
#include <vector>

#include <QApplication>
#include <QCoreApplication>
#include <QStringList>
#include <QWidget>
#include <QtLogging>

#include <TopoDS_Shape.hxx>
#include <gp_XYZ.hxx>

#include "CommandSession.hpp"
#include "GlassCard.hpp"
#include "ManipulatorOverlay.hpp"
#include "PreviewHold.hpp"

namespace mitcad {

void CommandSession::logPreviewBodies(const std::vector<BodyDisplay>& bodies) {
  // Each body with its occurrence, its placement's translation into the
  // design, "new" for what the command makes and "tool" for a consumed
  // tool's overlay (mitcad#561), as "Bodies shown" lists the normal view's.
  QStringList drawn;
  for (const BodyDisplay& body : bodies) {
    if (!body.shape) {
      continue;
    }
    const gp_XYZ at = body.placement.Transformation().TranslationPart();
    const auto coordinate = [](double value) {
      return QString::number(std::abs(value) < 1e-9 ? 0.0 : value, 'g', 10);
    };
    const QString occurrence = QString::fromStdString(body.occurrence);
    QString look;
    if (body.style == BodyDisplay::Style::New) {
      look = QStringLiteral(" new");
    } else if (body.style == BodyDisplay::Style::Tool || body.style == BodyDisplay::Style::CutTool) {
      look = QStringLiteral(" tool");
    }
    drawn << QStringLiteral("%1%2 at %3,%4,%5%6")
                 .arg(QString::fromStdString(body.uid),
                      occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + occurrence,
                      coordinate(at.X()), coordinate(at.Y()), coordinate(at.Z()), look);
  }
  const QString logged = drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral("; "));
  if (logged != m_loggedPreviewBodies) {
    m_loggedPreviewBodies = logged;
    qDebug().noquote() << QStringLiteral("Preview bodies: %1").arg(logged);
  }
}

void CommandSession::setUpPreviewHold() {
  // A feature's model preview only: a sketch command's preview is applied
  // to the model, an inspection and an application panel have none
  // (CommandSession::runPreview).
  if (m_previewHold || m_def.inspect || m_def.sketchCommand || m_def.modelCommand || m_def.accept) {
    return;
  }
  PreviewHold::Actions actions;
  actions.mayHide = [this] {
    // Why a Ctrl pressed alone does not hide it, for UI tests.
    const QString refusal = previewHoldRefusal();
    if (!refusal.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Preview hold refused: %1").arg(refusal);
    }
    return refusal.isEmpty();
  };
  actions.hide = [this] { hidePreviewHeld(); };
  actions.show = [this](const QString& why) { showPreviewHeld(why); };
  m_previewHold = new PreviewHold(std::move(actions), this);
  m_previewHold->install();
}

void CommandSession::endPreviewHold() {
  if (m_previewHold) {
    if (auto* application = QCoreApplication::instance()) {
      application->removeEventFilter(m_previewHold);
    }
    m_previewHold->deleteLater();
    m_previewHold = nullptr;
  }
  m_previewHidden = false;
  m_drawnPreview.reset();
}

QString CommandSession::previewHoldRefusal() const {
  if (m_done || m_committing || m_cancelRequested) {
    return QStringLiteral("the command is ending");
  }
  if (QApplication::activeModalWidget() != nullptr || QApplication::activePopupWidget() != nullptr) {
    return QStringLiteral("a dialog or a popup is open");
  }
  // The window of this command's view (a document of its own may be in
  // another), or one of its floating panels' windows.
  const QWidget* host = m_host.viewer().window();
  const QWidget* active = QApplication::activeWindow();
  if (active == nullptr || (active != host && GlassCard::hostWindow(active) != host)) {
    return QStringLiteral("the command's window is not active");
  }
  // In a text field Ctrl+A, C and V edit its text: not while the user
  // types in one. A panel's value field that has the keyboard counts as the
  // panel's until something is typed in it: the session puts the keyboard
  // there itself (as the panel opens, once the picks are done, mitcad#338),
  // also while the user picks in the view, and Ctrl+A, C or V pressed at
  // once hide nothing anyway. Whether the session or the user put it there
  // is not asked: a floating panel's window may get its focus later, when
  // the platform activates it.
  if (PreviewHold::editsText(QApplication::focusWidget()) && !(m_valueKeyboard && !m_valueTyped)) {
    return QStringLiteral("the keyboard is in a text field");
  }
  return QString();
}

void CommandSession::hidePreviewHeld() {
  if (m_done || m_previewHidden) {
    return;
  }
  m_previewHidden = true;
  if (m_manipulators) {
    m_manipulators->setHandlesShown(false);
  }
  drawHiddenPreview();
}

void CommandSession::drawHiddenPreview() {
  if (m_done || !m_previewHidden) {
    return;
  }
  if (m_host.modelBusy()) {
    // The bodies at the marker are read of the model: once it is idle.
    if (!m_hiddenWaits) {
      m_hiddenWaits = true;
      m_host.whenIdle(this, [this] {
        m_hiddenWaits = false;
        drawHiddenPreview();
      });
    }
    return;
  }
  // As clearPreview leaves the view, without taking the model's preview
  // away (OK uses it; the release draws it again).
  const std::vector<BodyDisplay> bodies = m_host.modelBodies();
  OcctViewer& viewer = m_host.viewer();
  viewer.setBodies(bodies);
  m_host.showSectionCapsOf(bodies, false);
  viewer.setPreview(TopoDS_Shape());
  // What it draws now, logged also when it is what the preview drew (a
  // fillet's body under its own uid), and again once it is shown.
  qDebug().noquote() << QStringLiteral("Preview hidden (Ctrl held)");
  m_loggedPreviewBodies.clear();
  logPreviewBodies(bodies);
  if (!m_loggedPreviewTool.isEmpty() && m_loggedPreviewTool != QStringLiteral("none")) {
    m_loggedPreviewTool = QStringLiteral("none");
    qDebug().noquote() << QStringLiteral("Preview tool at none");
  }
}

void CommandSession::showPreviewHeld(const QString& why) {
  if (!m_previewHidden) {
    return;
  }
  m_previewHidden = false;
  if (m_done) {
    return;
  }
  if (m_manipulators) {
    m_manipulators->setHandlesShown(true);
  }
  qDebug().noquote() << QStringLiteral("Preview shown (%1)").arg(why);
  m_loggedPreviewBodies.clear();
  if (m_drawnPreview) {
    // The latest: the one shown before, or the one computed meanwhile for
    // the inputs picked while Ctrl was held.
    const DrawnPreview again = *m_drawnPreview;
    showPreview(again.preview, again.operation);
  }
}

} // namespace mitcad
