// SPDX-License-Identifier: MIT
// Motion studies and contact sets in the window (mitcad#454; commands.md,
// "Motion studies and contact sets"):
//   - Motion Study (ASSEMBLE): the panel of a study (motion/
//     MotionStudyDialog), which plays it through the model's previews;
//     the browser's Motion Studies folder opens it too.
//   - Contact Set: the command's panel (commands/JointCommands.cpp) on a
//     set the browser's Contact Sets folder edits.
//   - Where a contact stops a motion (Drive Joint, Animate Joint, a drag,
//     a playback) the status bar says so; MainWindowJoints.cpp logs it.
#include "MainWindow.hpp"

#include <exception>

#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "framework/CommandRegistry.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "motion/MotionStudyDialog.hpp"

namespace mitcad {

QString MainWindow::contactText(const QJsonObject& contact) {
  QStringList bodies;
  for (const QJsonValue& value : contact.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    bodies << QStringLiteral("%1 (%2)").arg(body.value(QStringLiteral("name")).toString(),
                                            body.value(QStringLiteral("occurrence")).toString());
  }
  return tr("%1 in %2").arg(bodies.join(tr(" and ")), contact.value(QStringLiteral("set")).toString());
}

void MainWindow::showPreviewPlacements(const QJsonObject& report, const std::vector<BodyDisplay>& shown) {
  QHash<QString, QJsonArray> placed;
  for (const QJsonValue& value : report.value(QStringLiteral("placements")).toArray()) {
    placed.insert(value.toObject().value(QStringLiteral("path")).toString(),
                  value.toObject().value(QStringLiteral("transform")).toArray());
  }
  std::vector<BodyDisplay> bodies = shown;
  for (BodyDisplay& body : bodies) {
    const auto moved = placed.constFind(QString::fromStdString(body.occurrence));
    if (moved != placed.cend()) {
      body.placement = isIdentity(moved.value()) ? TopLoc_Location() : TopLoc_Location(trsfOf(moved.value()));
    }
  }
  m_viewer->setBodies(bodies);
}

void MainWindow::openMotionStudy(const QString& name) {
  if (m_mode != Mode::Idle || m_session) {
    showStatus(tr("Close the command to open a motion study."), NoticeLevel::Error);
    return;
  }
  // Its playback shows previews too (mitcad#1790).
  stopAnimation(QStringLiteral("a motion study opened"));
  if (m_motionStudy) {
    auto* open = static_cast<MotionStudyDialog*>(m_motionStudy.data());
    if (!name.isEmpty() && name != open->study()) {
      open->showStudy(name);
    }
    open->raise();
    open->activateWindow();
    return;
  }
  m_motionBodies = m_viewer->shownBodies();
  MotionStudyHost host;
  host.model = this;
  host.run = [this](const QJsonObject& command, QJsonObject* result) { return runModelCommand(command, result); };
  host.preview = [this](const QJsonObject& request) {
    const QJsonObject report = modelPreview(compactJson(request), tr("Motion Study"));
    showPreviewPlacements(report, m_motionBodies);
    return report;
  };
  host.endPreview = [this] {
    if (!modelBusy()) {
      clearModelPreview();
      if (m_mode == Mode::Idle) {
        m_viewer->setBodies(modelBodies()); // the design as it is
      }
    }
  };
  host.ready = [this] { return m_mode == Mode::Idle && !m_session && !modelBusy(); };
  host.status = [this](const QString& message, bool error) {
    showStatus(message, error ? NoticeLevel::Error : NoticeLevel::Information);
  };
  auto* dialog = new MotionStudyDialog(std::move(host), name, this);
  m_motionStudy = dialog;
  // The design changed while it was open (a key, Keep): the bodies shown
  // as they are now are the next frames' base.
  connect(m_documentSession, &DocumentSession::busyChanged, dialog, [this](bool busy) {
    if (!busy && m_motionStudy && m_mode == Mode::Idle) {
      m_motionBodies = modelBodies();
    }
  });
  dialog->show();
}

void MainWindow::editContactSet(const QString& name) {
  if (m_mode != Mode::Idle) {
    return;
  }
  const QJsonObject set = m_snapshot.contactSet(name);
  const CommandDef* def = m_registry->find(QStringLiteral("assemble.contact_set"));
  if (set.isEmpty() || def == nullptr) {
    return;
  }
  startFeatureCommand(*def, QString(), set);
}

} // namespace mitcad
