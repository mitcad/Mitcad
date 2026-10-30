// SPDX-License-Identifier: MIT
// Create Sketch and the Construct commands on geometry of a component other
// than the active one (mitcad#960; app/COMMANDS.md, "Activating the picked
// component"): right after the pick, the question whether to activate that
// component (browser/ActivateQuestion). Activated, it is one undo step of
// its own, as the browser's radio button makes, and the command goes on
// with the same pick: Create Sketch makes the sketch there, edited where
// the pick was (the picked occurrence, also of a component placed several
// times); a Construct command's preview is computed again in the new
// component. Kept, the command goes on as before.
#include "MainWindow.hpp"

#include <exception>

#include <QCursor>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "browser/ActivateQuestion.hpp"

namespace mitcad {

namespace {

// The model's command that activates a component (the browser's radio).
QJsonObject activateCommand(const QString& component) {
  return {{QStringLiteral("cmd"), QStringLiteral("activate_component")}, {QStringLiteral("component"), component}};
}

} // namespace

bool MainWindow::askToActivatePicked(const SelectionItem& item, const QString& refused, const QString& made) {
  // An agent's request is never held by a question it cannot answer.
  if (m_mcpRequestActive || m_mcpPicking) {
    return false;
  }
  ActivateQuestion question = activateQuestion(m_snapshot, item, refused, askToActivateComponent());
  if (!question.ask) {
    return false;
  }
  question.made = made;
  if (made == QStringLiteral("sketch")) {
    // A component that takes no sketches (its design policy) would refuse
    // the sketch once active: nothing asked, the sketch goes as before.
    try {
      const QString takesNone = queryObject({{QStringLiteral("query"), QStringLiteral("sketch_supports")},
                                             {QStringLiteral("component"), question.component}})
                                    .value(QStringLiteral("sketches"))
                                    .toString();
      if (!takesNone.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Activate component not asked: %1 takes no sketches (%2)")
                                  .arg(question.picked, takesNone);
        return false;
      }
    } catch (const std::exception&) {
      return false;
    }
  }
  qDebug().noquote() << QStringLiteral("Activate component asked: %1 (active %2)")
                            .arg(question.picked, question.active);
  // Placed clear of the pick: the pointer is where the face was clicked.
  const ActivateAnswer answer = askToActivate(this, question, QCursor::pos());
  const QString again = answer.dontAskAgain ? QStringLiteral(", don't ask again") : QString();
  if (answer.activate) {
    qDebug().noquote() << QStringLiteral("Activate component answer: activate %1%2").arg(question.picked, again);
  } else {
    qDebug().noquote() << QStringLiteral("Activate component answer: keep in %1%2").arg(question.active, again);
  }
  if (answer.dontAskAgain) {
    // Preferences > General asks again.
    saveAskToActivateComponent(false);
    qDebug().noquote() << QStringLiteral("Preferences: ask to activate the component of a picked face off");
  }
  if (!answer.activate) {
    return false;
  }
  const QJsonObject command = activateCommand(question.component);
  bool activated = false;
  if (m_session) {
    // Inside a command's panel: outside its undo range, kept on Cancel.
    QJsonArray commands;
    commands.append(command);
    activated = m_session->runActivation(commands, tr("Activate %1").arg(question.picked));
  } else {
    activated = runModelCommand(command);
  }
  if (activated) {
    qDebug().noquote() << QStringLiteral("Activated %1").arg(m_snapshot.componentName(question.component));
  }
  return activated;
}

void MainWindow::pickComing(const CommandDef& def, const SelectionItem& item) {
  // Construction planes, axes and points only; other features on another
  // component's geometry go on as before.
  if (def.group != QStringLiteral("CONSTRUCT") || !m_session) {
    return;
  }
  askToActivatePicked(item, QString(), madeBy(def.featureType));
}

} // namespace mitcad
