// SPDX-License-Identifier: MIT
// Save of a new design (mitcad#2183): the preference's stored ids and
// texts, the question's message box (its texts, buttons, default, Esc and
// the check box Remember my choice) with each answer, and the setting
// documents/newDesignSave read back, an unknown value read as Ask.
#include "UnitTests.hpp"
#include "files/NewDesignSave.hpp"
#include "framework/AppSettings.hpp"

#include <cstdio>
#include <optional>

#include <QApplication>
#include <QCheckBox>
#include <QCoreApplication>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QTemporaryDir>
#include <QTimer>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "new_design_save.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

void idTests() {
  CHECK(newDesignSaveId(NewDesignSave::Ask) == QStringLiteral("ask"));
  CHECK(newDesignSaveId(NewDesignSave::Computer) == QStringLiteral("computer"));
  CHECK(newDesignSaveId(NewDesignSave::Project) == QStringLiteral("project"));
  for (const NewDesignSave choice : {NewDesignSave::Ask, NewDesignSave::Computer, NewDesignSave::Project}) {
    CHECK(newDesignSaveFromId(newDesignSaveId(choice)) == choice);
  }
  CHECK(!newDesignSaveFromId(QStringLiteral("cloud")).has_value());
  CHECK(!newDesignSaveFromId(QString()).has_value());
  CHECK(!newDesignSaveFromId(QStringLiteral("Ask")).has_value());
  CHECK(newDesignSaveLabel(NewDesignSave::Ask) == QStringLiteral("Ask"));
  CHECK(newDesignSaveLabel(NewDesignSave::Computer) == QStringLiteral("On this computer"));
  CHECK(newDesignSaveLabel(NewDesignSave::Project) == QStringLiteral("To a project"));
}

// What the test does with the box: press a button, or Esc.
enum class Press { Computer, Project, Cancel, Escape };

std::optional<NewDesignSaveAnswer> answerWith(Press press, bool remember) {
  bool seen = false;
  QTimer::singleShot(0, [&seen, press, remember] {
    auto* box = qobject_cast<QMessageBox*>(QApplication::activeModalWidget());
    CHECK(box != nullptr);
    if (box == nullptr) {
      return;
    }
    seen = true;
    CHECK(box->objectName() == QStringLiteral("newDesignSaveQuestion"));
#ifndef Q_OS_MACOS
    // QMessageBox ignores window titles on macOS.
    CHECK(box->windowTitle() == QStringLiteral("Save"));
#endif
    CHECK(box->icon() == QMessageBox::Question);
    CHECK(box->text() == QStringLiteral("Save Bracket on this computer or to a project?"));
    CHECK(!box->informativeText().isEmpty());
    QPushButton* computer = nullptr;
    QPushButton* project = nullptr;
    QAbstractButton* cancel = box->button(QMessageBox::Cancel);
    for (QAbstractButton* button : box->buttons()) {
      if (button->text() == QStringLiteral("Save on This &Computer…")) {
        computer = qobject_cast<QPushButton*>(button);
      } else if (button->text() == QStringLiteral("Save to &Project…")) {
        project = qobject_cast<QPushButton*>(button);
      }
    }
    CHECK(computer != nullptr && project != nullptr && cancel != nullptr);
    CHECK(box->buttons().size() == 3);
    CHECK(box->defaultButton() == project);
    CHECK(box->escapeButton() == cancel);
    QCheckBox* again = box->checkBox();
    CHECK(again != nullptr && again->text() == QStringLiteral("&Remember my choice"));
    CHECK(again != nullptr && !again->isChecked());
    if (again != nullptr) {
      again->setChecked(remember);
    }
    switch (press) {
    case Press::Computer:
      if (computer != nullptr) {
        computer->click();
      }
      break;
    case Press::Project:
      if (project != nullptr) {
        project->click();
      }
      break;
    case Press::Cancel:
      if (cancel != nullptr) {
        cancel->click();
      }
      break;
    case Press::Escape:
      box->reject();
      break;
    }
  });
  const std::optional<NewDesignSaveAnswer> answer = askNewDesignSave(nullptr, QStringLiteral("Bracket"));
  CHECK(seen);
  return answer;
}

void questionTests() {
  const std::optional<NewDesignSaveAnswer> computer = answerWith(Press::Computer, false);
  CHECK(computer && computer->choice == NewDesignSave::Computer && !computer->remember);
  const std::optional<NewDesignSaveAnswer> project = answerWith(Press::Project, false);
  CHECK(project && project->choice == NewDesignSave::Project && !project->remember);
  const std::optional<NewDesignSaveAnswer> remembered = answerWith(Press::Project, true);
  CHECK(remembered && remembered->choice == NewDesignSave::Project && remembered->remember);
  const std::optional<NewDesignSaveAnswer> rememberedComputer = answerWith(Press::Computer, true);
  CHECK(rememberedComputer && rememberedComputer->choice == NewDesignSave::Computer && rememberedComputer->remember);
  // Cancel and Esc save nothing, whatever the check box.
  CHECK(!answerWith(Press::Cancel, true).has_value());
  CHECK(!answerWith(Press::Escape, false).has_value());
}

void settingTests() {
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("new_design_save"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, folder.path());

  // Ask by default; each choice read back as saved.
  CHECK(GeneralSettings::load().newDesignSave == QStringLiteral("ask"));
  for (const NewDesignSave choice : {NewDesignSave::Computer, NewDesignSave::Project, NewDesignSave::Ask}) {
    GeneralSettings general = GeneralSettings::load();
    general.newDesignSave = newDesignSaveId(choice);
    general.save();
    CHECK(QSettings().value(QStringLiteral("documents/newDesignSave")).toString() == newDesignSaveId(choice));
    CHECK(GeneralSettings::load().newDesignSave == newDesignSaveId(choice));
  }
  // An unknown value (an edited settings file) is the question.
  QSettings().setValue(QStringLiteral("documents/newDesignSave"), QStringLiteral("somewhere"));
  CHECK(GeneralSettings::load().newDesignSave == QStringLiteral("ask"));
  // The defaults (Restore Defaults saves them) ask again.
  QSettings().setValue(QStringLiteral("documents/newDesignSave"), QStringLiteral("project"));
  GeneralSettings{}.save();
  CHECK(GeneralSettings::load().newDesignSave == QStringLiteral("ask"));

  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
}

} // namespace

int newDesignSaveTests() {
  failures = 0;
  idTests();
  questionTests();
  settingTests();
  return failures;
}
MITCAD_UNIT_TEST(newDesignSaveTests);
