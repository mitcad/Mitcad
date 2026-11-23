// SPDX-License-Identifier: MIT
#include "JointHolderQuestion.hpp"

#include <QCheckBox>
#include <QCoreApplication>
#include <QJsonArray>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>

#include "../framework/Dialogs.hpp"

namespace mitcad {

namespace {

const char* const kSetting = "assembly/jointComponent";

// A name for a button: its own ampersands are no mnemonics.
QString literal(const QString& name) {
  QString escaped = name;
  escaped.replace(QLatin1Char('&'), QStringLiteral("&&"));
  return escaped;
}

// "Frame", "Frame and in Shaft Roller", "A, in B and in C": the names after
// "in" of the question's first sentence.
QString inList(const QStringList& names) {
  if (names.size() < 2) {
    return names.value(0);
  }
  const QStringList inFront = names.mid(0, names.size() - 1);
  return QCoreApplication::translate("JointHolder", "%1 and in %2")
      .arg(inFront.join(QCoreApplication::translate("JointHolder", ", in ")), names.last());
}

} // namespace

QString jointHolderChoiceId(JointHolderChoice choice) {
  switch (choice) {
  case JointHolderChoice::Holder:
    return QStringLiteral("holder");
  case JointHolderChoice::Active:
    return QStringLiteral("active");
  case JointHolderChoice::Ask:
    break;
  }
  return QStringLiteral("ask");
}

QString jointHolderChoiceText(JointHolderChoice choice) {
  switch (choice) {
  case JointHolderChoice::Holder:
    return QCoreApplication::translate("JointHolder", "Use the component that holds both");
  case JointHolderChoice::Active:
    return QCoreApplication::translate("JointHolder", "Keep the active component");
  case JointHolderChoice::Ask:
    break;
  }
  return QCoreApplication::translate("JointHolder", "Ask");
}

JointHolderChoice jointHolderChoiceOf(const QString& id) {
  if (id == QLatin1String("holder")) {
    return JointHolderChoice::Holder;
  }
  if (id == QLatin1String("active")) {
    return JointHolderChoice::Active;
  }
  return JointHolderChoice::Ask;
}

QStringList jointHolderInputs(const QString& command) {
  if (command == QLatin1String("assemble.joint") || command == QLatin1String("assemble.as_built_joint")) {
    return {QStringLiteral("a"), QStringLiteral("b")};
  }
  if (command == QLatin1String("assemble.rigid_group")) {
    return {QStringLiteral("occurrences")};
  }
  if (command == QLatin1String("assemble.constraint") || command == QLatin1String("assemble.constraint_one")) {
    return {QStringLiteral("a"), QStringLiteral("b"), QStringLiteral("b2")};
  }
  return {};
}

JointHolderStep jointHolderStep(const QJsonObject& answer, const QString& held, const QString& kept,
                                JointHolderChoice preference, bool agent) {
  // The active component holds them as it should, or no component joins
  // them (the panel's refusal says why): the active one, as before.
  if (answer.value(QStringLiteral("relation")).toString() == QLatin1String("same") ||
      answer.contains(QStringLiteral("error"))) {
    return held.isEmpty() ? JointHolderStep::None : JointHolderStep::Clear;
  }
  const QString component = answer.value(QStringLiteral("component")).toString();
  if (held == component || kept == component) {
    return JointHolderStep::None; // answered for this component already
  }
  switch (preference) {
  case JointHolderChoice::Holder:
    return JointHolderStep::UseHolder;
  case JointHolderChoice::Active:
    return JointHolderStep::KeepActive;
  case JointHolderChoice::Ask:
    break;
  }
  // An agent's request is never held by a question it cannot answer: the
  // active component, and its refusal.
  return agent ? JointHolderStep::None : JointHolderStep::Ask;
}

JointHolderChoice jointHolderPreference() {
  return jointHolderChoiceOf(QSettings().value(QLatin1String(kSetting)).toString());
}

void saveJointHolderPreference(JointHolderChoice choice) {
  QSettings().setValue(QLatin1String(kSetting), jointHolderChoiceId(choice));
}

QString madeWhat(const QString& command) {
  if (command == QLatin1String("assemble.joint")) {
    return QCoreApplication::translate("JointHolder", "A joint");
  }
  if (command == QLatin1String("assemble.as_built_joint")) {
    return QCoreApplication::translate("JointHolder", "An as-built joint");
  }
  if (command == QLatin1String("assemble.rigid_group")) {
    return QCoreApplication::translate("JointHolder", "A rigid group");
  }
  if (command == QLatin1String("assemble.constraint") || command == QLatin1String("assemble.constraint_one")) {
    return QCoreApplication::translate("JointHolder", "A constraint");
  }
  return QString();
}

JointHolderQuestion jointHolderQuestion(const QJsonObject& answer, const QString& command) {
  JointHolderQuestion question;
  question.component = answer.value(QStringLiteral("component")).toString();
  question.holder = answer.value(QStringLiteral("name")).toString();
  question.active = answer.value(QStringLiteral("active_name")).toString();
  question.relation = answer.value(QStringLiteral("relation")).toString();
  question.whole = answer.value(QStringLiteral("whole")).toString();
  question.made = madeWhat(command);
  question.joins = command == QLatin1String("assemble.rigid_group");
  const QJsonArray picks = answer.value(QStringLiteral("picks")).toArray();
  question.picks = static_cast<int>(picks.size());
  // By the components' names; by the occurrences' when two of them have
  // one name (two placements of one component, or two components named
  // alike), so that the question tells them apart.
  QStringList names;
  QStringList occurrences;
  for (const QJsonValue& value : picks) {
    const QJsonObject pick = value.toObject();
    names << pick.value(QStringLiteral("name")).toString();
    const QString occurrence = pick.value(QStringLiteral("occurrence")).toString();
    occurrences << (occurrence.isEmpty() ? question.holder : occurrence);
  }
  for (int i = 0; i < names.size(); ++i) {
    bool shared = false;
    for (int j = 0; j < names.size(); ++j) {
      shared = shared || (j != i && names.at(j) == names.at(i) && occurrences.at(j) != occurrences.at(i));
    }
    const QString shown = shared ? occurrences.at(i) : names.at(i);
    if (!question.picked.contains(shown)) {
      question.picked << shown;
    }
  }
  return question;
}

QString JointHolderQuestion::text() const {
  const bool two = picks == 2;
  QString where;
  if (picked.size() == 1) {
    where = two ? QCoreApplication::translate("JointHolder", "Both picks are in %1.").arg(picked.first())
                : QCoreApplication::translate("JointHolder", "The picks are all in %1.").arg(picked.first());
  } else if (two) {
    where = QCoreApplication::translate("JointHolder", "The two picks are in %1.").arg(inList(picked));
  } else {
    where = QCoreApplication::translate("JointHolder", "The picks are in %1.").arg(inList(picked));
  }
  QString why;
  if (relation == QLatin1String("above")) {
    // The active component holds them, but only inside one of its
    // occurrences, which it moves as a whole.
    why = QCoreApplication::translate("JointHolder", "%1 made in %2 cannot join them: they lie inside %3, which it "
                                                     "moves only as a whole.")
              .arg(made, active, holder);
  } else if (joins) {
    why = QCoreApplication::translate("JointHolder", "%1 made in %2 can join only %2's own parts.").arg(made, active);
  } else {
    why = QCoreApplication::translate("JointHolder", "%1 made in %2 can move only %2's own parts.").arg(made, active);
  }
  QString offer;
  if (relation == QLatin1String("inside") && !whole.isEmpty()) {
    offer = two ? QCoreApplication::translate("JointHolder", "Make it in %1, which holds both, so that %2 moves as a "
                                                             "whole?")
                      .arg(holder, whole)
                : QCoreApplication::translate("JointHolder", "Make it in %1, which holds them all, so that %2 moves "
                                                             "as a whole?")
                      .arg(holder, whole);
  } else {
    offer = two ? QCoreApplication::translate("JointHolder", "Make it in %1, which holds both?").arg(holder)
                : QCoreApplication::translate("JointHolder", "Make it in %1, which holds them all?").arg(holder);
  }
  return where + QLatin1Char(' ') + why + QLatin1Char(' ') + offer;
}

QString JointHolderQuestion::makeButton() const {
  return QCoreApplication::translate("JointHolder", "&Make it in %1").arg(literal(holder));
}

QString JointHolderQuestion::keepButton() const {
  return QCoreApplication::translate("JointHolder", "&Keep %1").arg(literal(active));
}

QString JointHolderQuestion::notice(const QString& command) const {
  return picks == 2 ? QCoreApplication::translate("JointHolder", "%1 goes into %2, which holds both picks; %3 stays "
                                                                 "active.")
                          .arg(command, holder, active)
                    : QCoreApplication::translate("JointHolder", "%1 goes into %2, which holds all the picks; %3 "
                                                                 "stays active.")
                          .arg(command, holder, active);
}

JointHolderAnswer askJointHolder(QWidget* parent, const JointHolderQuestion& question, const QString& command) {
  QMessageBox box(QMessageBox::Question, QCoreApplication::translate("JointHolder", "Component for the %1").arg(command),
                  question.text(), QMessageBox::NoButton, parent);
  box.setObjectName(QStringLiteral("jointComponentQuestion"));
  QPushButton* make = box.addButton(question.makeButton(), QMessageBox::AcceptRole);
  QPushButton* keep = box.addButton(question.keepButton(), QMessageBox::RejectRole);
  auto* again = new QCheckBox(QCoreApplication::translate("JointHolder", "&Don't ask again"), &box);
  again->setToolTip(QCoreApplication::translate("JointHolder", "The Assembly page of Tools › Preferences asks again"));
  box.setCheckBox(again);
  box.setDefaultButton(make);
  box.setEscapeButton(keep);
  prepareModal(&box);
  execDialog(&box);
  JointHolderAnswer answer;
  answer.useHolder = box.clickedButton() == make;
  answer.dontAskAgain = again->isChecked();
  return answer;
}

} // namespace mitcad
