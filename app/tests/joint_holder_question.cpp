// SPDX-License-Identifier: MIT
// Joint's question when its picks lie under another component than the
// active one (mitcad#1856): its text from the model's `joint_holder`
// answer in each case (the active component inside the holder, above it,
// apart from it, a rigid group's picks, two placements of one component),
// what the panel does with the answer (mitcad#1864: also for the selection
// it opens with), the message box with its buttons, default and check box,
// and the preference's values.
#include "browser/JointHolderQuestion.hpp"

#include <cstdio>

#include <QApplication>
#include <QCheckBox>
#include <QJsonArray>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QTemporaryDir>
#include <QTimer>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "joint_holder_question.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject pick(const QString& name, const QString& occurrence) {
  QJsonObject made{{QStringLiteral("name"), name}};
  made.insert(QStringLiteral("occurrence"), occurrence.isEmpty() ? QJsonValue() : QJsonValue(occurrence));
  return made;
}

// The reported case: Shaft Roller active, a pick on a part inside it and
// one on the frame; Assembly (the root) holds both.
QJsonObject reported() {
  QJsonArray picks;
  picks.append(pick(QStringLiteral("Frame"), QStringLiteral("Frame:1")));
  picks.append(pick(QStringLiteral("Shaft Roller"), QStringLiteral("Shaft Roller:1")));
  return {{QStringLiteral("component"), QStringLiteral("C0")},
          {QStringLiteral("name"), QStringLiteral("Assembly")},
          {QStringLiteral("active"), QStringLiteral("C10")},
          {QStringLiteral("active_name"), QStringLiteral("Shaft Roller")},
          {QStringLiteral("relation"), QStringLiteral("inside")},
          {QStringLiteral("whole"), QStringLiteral("Shaft Roller")},
          {QStringLiteral("whole_occurrence"), QStringLiteral("O14")},
          {QStringLiteral("picks"), picks}};
}

void textTests() {
  const JointHolderQuestion question = jointHolderQuestion(reported(), QStringLiteral("assemble.joint"));
  CHECK(question.component == QStringLiteral("C0"));
  CHECK(question.holder == QStringLiteral("Assembly"));
  CHECK(question.active == QStringLiteral("Shaft Roller"));
  CHECK(question.picks == 2);
  CHECK(question.picked == QStringList({QStringLiteral("Frame"), QStringLiteral("Shaft Roller")}));
  CHECK(question.text() ==
        QStringLiteral("The two picks are in Frame and in Shaft Roller. A joint made in Shaft Roller can move only "
                       "Shaft Roller's own parts. Make it in Assembly, which holds both, so that Shaft Roller moves "
                       "as a whole?"));
  CHECK(question.makeButton() == QStringLiteral("&Make it in Assembly"));
  CHECK(question.keepButton() == QStringLiteral("&Keep Shaft Roller"));
  CHECK(question.notice(QStringLiteral("Joint")) ==
        QStringLiteral("Joint goes into Assembly, which holds both picks; Shaft Roller stays active."));
  // No ids or paths in what a person reads.
  for (const QString& text : {question.text(), question.makeButton(), question.keepButton()}) {
    CHECK(!text.contains(QStringLiteral("C0")) && !text.contains(QStringLiteral("C10")) &&
          !text.contains(QStringLiteral("O14")));
  }

  // The root active and both picks inside one sub-assembly.
  QJsonObject above = reported();
  QJsonArray inside;
  inside.append(pick(QStringLiteral("Bearing"), QStringLiteral("Bearing:1")));
  inside.append(pick(QStringLiteral("Tube"), QStringLiteral("Tube:1")));
  above.insert(QStringLiteral("picks"), inside);
  above.insert(QStringLiteral("name"), QStringLiteral("Shaft Roller"));
  above.insert(QStringLiteral("active_name"), QStringLiteral("Assembly"));
  above.insert(QStringLiteral("relation"), QStringLiteral("above"));
  above.remove(QStringLiteral("whole"));
  const JointHolderQuestion up = jointHolderQuestion(above, QStringLiteral("assemble.as_built_joint"));
  CHECK(up.text() == QStringLiteral("The two picks are in Bearing and in Tube. An as-built joint made in Assembly "
                                    "cannot join them: they lie inside Shaft Roller, which it moves only as a "
                                    "whole. Make it in Shaft Roller, which holds both?"));

  // A rigid group of three, the active component apart from them; two
  // placements of one component by their occurrences' names.
  QJsonObject apart = reported();
  QJsonArray three;
  three.append(pick(QStringLiteral("shaft"), QStringLiteral("shaft:1")));
  three.append(pick(QStringLiteral("shaft"), QStringLiteral("shaft:2")));
  three.append(pick(QStringLiteral("Frame"), QStringLiteral("Frame:1")));
  apart.insert(QStringLiteral("picks"), three);
  apart.insert(QStringLiteral("relation"), QStringLiteral("apart"));
  apart.insert(QStringLiteral("active_name"), QStringLiteral("Motor"));
  apart.remove(QStringLiteral("whole"));
  const JointHolderQuestion group = jointHolderQuestion(apart, QStringLiteral("assemble.rigid_group"));
  CHECK(group.picked ==
        QStringList({QStringLiteral("shaft:1"), QStringLiteral("shaft:2"), QStringLiteral("Frame")}));
  CHECK(group.text() == QStringLiteral("The picks are in shaft:1, in shaft:2 and in Frame. A rigid group made in "
                                       "Motor can join only Motor's own parts. Make it in Assembly, which holds "
                                       "them all?"));
  CHECK(group.notice(QStringLiteral("Rigid Group")) ==
        QStringLiteral("Rigid Group goes into Assembly, which holds all the picks; Motor stays active."));
  CHECK(madeWhat(QStringLiteral("assemble.constraint")) == QStringLiteral("A constraint"));
  CHECK(madeWhat(QStringLiteral("solid.extrude")).isEmpty());
}

// What the panel does with the query's answer, the same for a pick in the
// panel and for the selection it opens with (mitcad#1864), and the inputs
// whose picks decide.
void stepTests() {
  const QJsonObject answer = reported();
  // Nothing chosen yet: the preference decides, Ask asks a person only.
  CHECK(jointHolderStep(answer, QString(), QString(), JointHolderChoice::Ask, false) == JointHolderStep::Ask);
  CHECK(jointHolderStep(answer, QString(), QString(), JointHolderChoice::Ask, true) == JointHolderStep::None);
  CHECK(jointHolderStep(answer, QString(), QString(), JointHolderChoice::Holder, false) ==
        JointHolderStep::UseHolder);
  CHECK(jointHolderStep(answer, QString(), QString(), JointHolderChoice::Holder, true) ==
        JointHolderStep::UseHolder);
  CHECK(jointHolderStep(answer, QString(), QString(), JointHolderChoice::Active, false) ==
        JointHolderStep::KeepActive);
  // Answered for this component already, made in it or declined.
  CHECK(jointHolderStep(answer, QStringLiteral("C0"), QString(), JointHolderChoice::Ask, false) ==
        JointHolderStep::None);
  CHECK(jointHolderStep(answer, QString(), QStringLiteral("C0"), JointHolderChoice::Ask, false) ==
        JointHolderStep::None);
  // Another component than the one answered for: asked again.
  CHECK(jointHolderStep(answer, QStringLiteral("C7"), QString(), JointHolderChoice::Ask, false) ==
        JointHolderStep::Ask);
  // The active component holds the picks, or none joins them: a choice
  // made before goes, else nothing.
  QJsonObject same = answer;
  same.insert(QStringLiteral("relation"), QStringLiteral("same"));
  CHECK(jointHolderStep(same, QString(), QString(), JointHolderChoice::Ask, false) == JointHolderStep::None);
  CHECK(jointHolderStep(same, QStringLiteral("C0"), QString(), JointHolderChoice::Ask, false) ==
        JointHolderStep::Clear);
  const QJsonObject refused{{QStringLiteral("error"), QStringLiteral("both picks are in one unit")}};
  CHECK(jointHolderStep(refused, QStringLiteral("C0"), QString(), JointHolderChoice::Holder, false) ==
        JointHolderStep::Clear);
  CHECK(jointHolderStep(refused, QString(), QString(), JointHolderChoice::Holder, false) == JointHolderStep::None);

  CHECK(jointHolderInputs(QStringLiteral("assemble.joint")) == QStringList({QStringLiteral("a"), QStringLiteral("b")}));
  CHECK(jointHolderInputs(QStringLiteral("assemble.as_built_joint")) ==
        QStringList({QStringLiteral("a"), QStringLiteral("b")}));
  CHECK(jointHolderInputs(QStringLiteral("assemble.rigid_group")) == QStringList({QStringLiteral("occurrences")}));
  CHECK(jointHolderInputs(QStringLiteral("assemble.constraint")) ==
        QStringList({QStringLiteral("a"), QStringLiteral("b"), QStringLiteral("b2")}));
  CHECK(jointHolderInputs(QStringLiteral("assemble.drive_joint")).isEmpty());
  CHECK(jointHolderInputs(QStringLiteral("solid.extrude")).isEmpty());
}

// The message box: its texts, Make it in the default, and the answers.
void dialogTests() {
  const JointHolderQuestion question = jointHolderQuestion(reported(), QStringLiteral("assemble.joint"));
  const auto answerWith = [&question](bool make, bool dontAsk) {
    bool seen = false;
    QTimer::singleShot(0, [&seen, &question, make, dontAsk] {
      auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget());
      CHECK(box != nullptr);
      if (box == nullptr) {
        return;
      }
      seen = true;
      CHECK(box->objectName() == QStringLiteral("jointComponentQuestion"));
#ifndef Q_OS_MACOS
      CHECK(box->windowTitle() == QStringLiteral("Component for the Joint"));
#endif
      CHECK(box->icon() == QMessageBox::Question);
      CHECK(box->text() == question.text());
      QPushButton* makeButton = nullptr;
      QPushButton* keepButton = nullptr;
      for (QAbstractButton* button : box->buttons()) {
        if (button->text() == QStringLiteral("&Make it in Assembly")) {
          makeButton = qobject_cast<QPushButton*>(button);
        } else if (button->text() == QStringLiteral("&Keep Shaft Roller")) {
          keepButton = qobject_cast<QPushButton*>(button);
        }
      }
      CHECK(makeButton != nullptr && keepButton != nullptr);
      CHECK(box->defaultButton() == makeButton);
      CHECK(box->escapeButton() == keepButton);
      CHECK(box->checkBox() != nullptr && box->checkBox()->text() == QStringLiteral("&Don't ask again"));
      CHECK(box->checkBox() != nullptr && !box->checkBox()->isChecked());
      if (box->checkBox() != nullptr) {
        box->checkBox()->setChecked(dontAsk);
      }
      QPushButton* pressed = make ? makeButton : keepButton;
      if (pressed != nullptr) {
        pressed->click();
      } else {
        box->reject();
      }
    });
    const JointHolderAnswer answer = askJointHolder(nullptr, question, QStringLiteral("Joint"));
    CHECK(seen);
    return answer;
  };
  const JointHolderAnswer made = answerWith(true, false);
  CHECK(made.useHolder && !made.dontAskAgain);
  const JointHolderAnswer kept = answerWith(false, true);
  CHECK(!kept.useHolder && kept.dontAskAgain);
}

// The preference: Ask unless saved otherwise, each choice saved and read
// back by its value, an unknown value read as Ask.
void settingsTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("joint_holder_question"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, folder.path());
  QSettings().remove(QStringLiteral("assembly/jointComponent"));
  CHECK(jointHolderPreference() == JointHolderChoice::Ask);
  saveJointHolderPreference(JointHolderChoice::Holder);
  CHECK(jointHolderPreference() == JointHolderChoice::Holder);
  CHECK(QSettings().value(QStringLiteral("assembly/jointComponent")).toString() == QStringLiteral("holder"));
  saveJointHolderPreference(JointHolderChoice::Active);
  CHECK(jointHolderPreference() == JointHolderChoice::Active);
  CHECK(QSettings().value(QStringLiteral("assembly/jointComponent")).toString() == QStringLiteral("active"));
  saveJointHolderPreference(JointHolderChoice::Ask);
  CHECK(jointHolderPreference() == JointHolderChoice::Ask);
  QSettings().setValue(QStringLiteral("assembly/jointComponent"), QStringLiteral("sometimes"));
  CHECK(jointHolderPreference() == JointHolderChoice::Ask);
  CHECK(jointHolderChoiceText(JointHolderChoice::Holder) == QStringLiteral("Use the component that holds both"));
  CHECK(jointHolderChoiceText(JointHolderChoice::Active) == QStringLiteral("Keep the active component"));
  CHECK(jointHolderChoiceText(JointHolderChoice::Ask) == QStringLiteral("Ask"));
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
}

} // namespace

int jointHolderQuestionTests() {
  failures = 0;
  textTests();
  stepTests();
  dialogTests();
  settingsTests();
  return failures;
}
