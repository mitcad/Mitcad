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
//
// A feature that changes the picked body (Fillet, Chamfer, Shell, ...;
// mitcad#1634, browser/ActivateOnPick) asks nothing: such a feature goes
// only into the body's own component, so its first pick of another
// component's body activates that component, as the same undo step of its
// own outside the panel's range, and the panel says where the feature goes.
// Picks of several components keep the refusal (the command's or the
// model's), beside which the panel offers to activate one of them, keeping
// only its picks. A part placed by a pick (Gear, Pulley, ...; mitcad#1752)
// activates the same way by its Place On, except on another component's
// shaft, which it takes linked, the panel saying so.
#include "MainWindow.hpp"

#include <exception>

#include <QCursor>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "browser/ActivateOnPick.hpp"
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

void MainWindow::pickComing(const CommandDef& def, const InputDef& input, const SelectionItem& item) {
  if (!m_session) {
    return;
  }
  // A feature that changes the picked body goes only into the body's own
  // component (mitcad#1634): activated without a question.
  if (activatingInputs(def.id).contains(input.id)) {
    // A part placed by a pick goes into the picked body's component too
    // (mitcad#1752), but on another component's shaft it goes in linked,
    // into the active component, and the panel says why.
    const bool activates = pickActivates(def.id, item);
    const bool activated = activates && activateForPick(def, input, item);
    if (activated || !linksShafts(def.id) || !m_session) {
      return;
    }
    const QString shown = m_session->activationNotice();
    const QString linked = activates ? QString() : linkedShaftNotice(m_snapshot, def.id, def.name, item);
    const QString notice = linked.isEmpty() ? keptNotice(m_snapshot, def.name, shown) : linked;
    if (notice != shown) {
      if (!linked.isEmpty()) {
        qDebug().noquote() << QStringLiteral("Not activated for %1: %2 is a shaft, linked")
                                  .arg(def.name, item.logDescription());
      }
      m_session->setActivationNotice(notice);
    }
    return;
  }
  // Construction planes, axes and points ask (mitcad#960); other features
  // on another component's geometry go on as before.
  if (def.group != QStringLiteral("CONSTRUCT")) {
    return;
  }
  askToActivatePicked(item, QString(), madeBy(def.featureType));
}

bool MainWindow::activateForPick(const CommandDef& def, const InputDef& input, const SelectionItem& item) {
  // An agent's request never activates by itself: it gets the model's
  // refusal, and ui_state names the component to activate.
  if (m_mcpRequestActive || m_mcpPicking || !m_session) {
    return false;
  }
  // The picks made before, but not the one an input of one item holds:
  // the new pick takes its place (mitcad#1743: a hole's Place On moved to
  // another component's face goes into that component, not the one of
  // the face it leaves).
  QStringList before = activatingInputs(def.id);
  if (input.max == 1) {
    before.removeAll(input.id);
  }
  const PickActivation activation =
      activationForPick(m_snapshot, item, activatingPicks(def.id, m_session->picksOf(before)));
  if (activation.isEmpty()) {
    return false;
  }
  QJsonArray commands;
  commands.append(activateCommand(activation.component));
  // Outside the panel's undo range: its own undo step, kept on Cancel; the
  // previews cached for the old component dropped.
  if (!m_session->runActivation(commands, tr("Activate %1").arg(activation.name))) {
    qDebug().noquote() << QStringLiteral("Activate %1 for %2 refused: %3")
                              .arg(activation.name, def.name, m_lastError);
    return false;
  }
  qDebug().noquote() << QStringLiteral("Activated %1 for %2 (picked %3)")
                            .arg(activation.name, def.name, item.logDescription());
  m_session->setActivationNotice(activatedNotice(def.name, activation.name, 0));
  return true;
}

QString MainWindow::activateForPreselection(const CommandDef& def, const Selection& preselection) {
  if (m_mcpRequestActive || m_mcpPicking || activatingInputs(def.id).isEmpty()) {
    return QString();
  }
  // As the picks one by one: the first body pick, then each further one
  // with those before it.
  PickActivation activation;
  Selection before;
  QString linked;
  for (const SelectionItem& item : preselection) {
    if (bodyComponent(m_snapshot, item).isEmpty()) {
      continue;
    }
    if (!pickActivates(def.id, item)) {
      // A part on another component's shaft goes in linked (mitcad#1752).
      if (linked.isEmpty()) {
        linked = linkedShaftNotice(m_snapshot, def.id, def.name, item);
      }
      continue;
    }
    const PickActivation next = activationForPick(m_snapshot, item, before);
    if (next.isEmpty() || (!activation.isEmpty() && next.component != activation.component)) {
      return QString(); // the active component's, or two components: the refusal
    }
    activation = next;
    before.append(item);
  }
  if (activation.isEmpty()) {
    return linked;
  }
  if (!runModelCommand(activateCommand(activation.component))) {
    return QString();
  }
  qDebug().noquote() << QStringLiteral("Activated %1 for %2 (picked %3)")
                            .arg(activation.name, def.name, activation.picked.logDescription());
  return activatedNotice(def.name, activation.name, 0);
}

QJsonObject MainWindow::activationOffer(const CommandSession& session) const {
  const QStringList inputs = activatingInputs(session.def().id);
  if (inputs.isEmpty() || session.isEditing()) {
    return {};
  }
  const PickActivation offer =
      activationForPicks(m_snapshot, activatingPicks(session.def().id, session.picksOf(inputs)));
  if (offer.isEmpty()) {
    return {};
  }
  return {{QStringLiteral("component"), offer.component},
          {QStringLiteral("name"), offer.name},
          {QStringLiteral("drops"), offer.dropped}};
}

void MainWindow::takeActivationOffer(CommandSession& session) {
  const QStringList inputs = activatingInputs(session.def().id);
  const PickActivation offer =
      activationForPicks(m_snapshot, activatingPicks(session.def().id, session.picksOf(inputs)));
  if (offer.isEmpty()) {
    return;
  }
  QJsonArray commands;
  commands.append(activateCommand(offer.component));
  if (!session.runActivation(commands, tr("Activate %1").arg(offer.name))) {
    qDebug().noquote() << QStringLiteral("Activate %1 for %2 refused: %3")
                              .arg(offer.name, session.def().name, m_lastError);
    return;
  }
  // Only the picks of the activated component stay (and those of no
  // component's body: a datum, a sketch's point; a part's linked shaft,
  // mitcad#1752).
  const QString component = offer.component;
  const QString command = session.def().id;
  const int dropped = session.keepPicks(inputs, [this, &component, &command](const SelectionItem& item) {
    const QString of = bodyComponent(m_snapshot, item);
    return of.isEmpty() || of == component || !pickActivates(command, item);
  });
  qDebug().noquote() << QStringLiteral("Activated %1 for %2 (picked %3), %4 pick(s) dropped")
                            .arg(offer.name, session.def().name, offer.picked.logDescription())
                            .arg(dropped);
  session.setActivationNotice(activatedNotice(session.def().name, offer.name, dropped));
}
} // namespace mitcad
