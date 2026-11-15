// SPDX-License-Identifier: MIT
// Joint, As-Built Joint, Rigid Group and Assembly Constraint on picks that
// lie under another component than the active one (mitcad#1856;
// app/COMMANDS.md "The component that holds the picks"). A feature of these
// goes into one component and moves, joins or holds only that component's
// own occurrences: one made in a sub-assembly cannot mount it in its
// parent. Right after a pick, the picks so far are given to the model's
// `joint_holder` query; when the lowest component that holds them all is
// not the active one, the preference (browser/JointHolderQuestion.hpp)
// decides, or a question asks, whether the feature goes there. The answer
// is kept in the panel's hidden inputs: "holder" (the component, which the
// command's add_feature then names, the active one staying active) or
// "kept" (declined: not asked again for that component while the panel is
// open). The pick goes in after it and the preview follows.
//
// The selection made before the command (mitcad#1864) is decided the same
// way, once its picks are in the panel's inputs and before the panel opens:
// its first preview is computed in the component chosen.
#include "MainWindow.hpp"

#include <exception>

#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "browser/JointHolderQuestion.hpp"

namespace mitcad {

namespace {

const QString kHolder = QStringLiteral("holder");
const QString kKept = QStringLiteral("kept");

} // namespace

bool MainWindow::decideJointHolder(CommandSession& session, const QJsonArray& paths, QString& notice) {
  if (paths.size() < 2) {
    return false;
  }
  // Copied: the question's own event loop may end the panel.
  const QString command = session.def().id;
  const QString name = session.def().name;
  const QPointer<CommandSession> guard(&session);
  QJsonObject answer;
  try {
    answer = queryObject({{QStringLiteral("query"), QStringLiteral("joint_holder")},
                          {QStringLiteral("occurrences"), paths}});
  } catch (const std::exception& e) {
    qDebug().noquote() << QStringLiteral("%1 component not found: %2").arg(name, QString::fromUtf8(e.what()));
    return false;
  }
  const QString component = answer.value(QStringLiteral("component")).toString();
  const QString held = session.state().text(kHolder);
  const JointHolderStep step = jointHolderStep(answer, held, session.state().text(kKept), jointHolderPreference(),
                                               m_mcpRequestActive || m_mcpPicking);
  if (step == JointHolderStep::None) {
    return false;
  }
  if (step == JointHolderStep::Clear) {
    session.setHiddenText(kHolder, QString());
    notice.clear();
    return true;
  }
  const JointHolderQuestion question = jointHolderQuestion(answer, command);
  bool useHolder = step == JointHolderStep::UseHolder;
  if (step == JointHolderStep::UseHolder) {
    qDebug().noquote() << QStringLiteral("%1 component by the preference: %2 (holds the picks; %3 active)")
                              .arg(name, question.holder, question.active);
  } else if (step == JointHolderStep::KeepActive) {
    qDebug().noquote() << QStringLiteral("%1 component by the preference: keep %2 (%3 holds the picks)")
                              .arg(name, question.active, question.holder);
  } else {
    QString logged = question.text();
    logged.replace(QLatin1Char('\n'), QLatin1Char(' '));
    qDebug().noquote() << QStringLiteral("%1 component asked: %2 [%3, %4]")
                              .arg(name, logged, question.makeButton(), question.keepButton())
                              .remove(QLatin1Char('&'));
    const JointHolderAnswer reply = askJointHolder(this, question, name);
    const QString again = reply.dontAskAgain ? QStringLiteral(", don't ask again") : QString();
    qDebug().noquote() << QStringLiteral("%1 component answer: %2%3")
                              .arg(name,
                                   reply.useHolder ? QStringLiteral("make it in %1").arg(question.holder)
                                                   : QStringLiteral("keep %1").arg(question.active),
                                   again);
    if (reply.dontAskAgain) {
      // Preferences > Assembly asks again.
      const JointHolderChoice choice = reply.useHolder ? JointHolderChoice::Holder : JointHolderChoice::Active;
      saveJointHolderPreference(choice);
      qDebug().noquote() << QStringLiteral("Preferences: when the picks are in another component %1")
                                .arg(jointHolderChoiceId(choice));
    }
    useHolder = reply.useHolder;
  }
  // The question's own event loop: the panel may have ended meanwhile.
  if (!guard) {
    return false;
  }
  if (useHolder) {
    session.setHiddenText(kKept, QString());
    session.setHiddenText(kHolder, component);
    notice = question.notice(name);
    return true;
  }
  session.setHiddenText(kHolder, QString());
  session.setHiddenText(kKept, component);
  if (!held.isEmpty()) {
    notice.clear();
    return true;
  }
  return false;
}

bool MainWindow::holderPickComing(const CommandDef& def, const InputDef& input, const SelectionItem& item) {
  const QStringList inputs = jointHolderInputs(def.id);
  if (inputs.isEmpty() || !m_session) {
    return false;
  }
  if (!inputs.contains(input.id)) {
    return true; // a motion origin, a value: where the feature goes stays
  }
  // The picks once this one is in: an input of one item takes it instead
  // of what it holds (not a constraint's row, whose inputs share their ids
  // with the other rows').
  QStringList before = inputs;
  if (input.max == 1 && def.id != QLatin1String("assemble.constraint")) {
    before.removeAll(input.id);
  }
  QJsonArray paths;
  for (const SelectionItem& pick : m_session->picksOf(before)) {
    paths.append(pick.occurrence);
  }
  paths.append(item.occurrence);
  QString notice;
  if (decideJointHolder(*m_session, paths, notice) && m_session) {
    m_session->setActivationNotice(notice);
  }
  return true;
}

QString MainWindow::holderForPreselection(CommandSession& session) {
  const QStringList inputs = jointHolderInputs(session.def().id);
  if (inputs.isEmpty() || session.isEditing()) {
    return QString();
  }
  QJsonArray paths;
  for (const SelectionItem& pick : session.picksOf(inputs)) {
    paths.append(pick.occurrence);
  }
  if (paths.size() >= 2) {
    qDebug().noquote() << QStringLiteral("%1 component of the selection made before it: %2 pick(s)")
                              .arg(session.def().name)
                              .arg(paths.size());
  }
  QString notice;
  decideJointHolder(session, paths, notice);
  return notice;
}

} // namespace mitcad
