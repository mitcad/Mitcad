// SPDX-License-Identifier: MIT
#include "NewDesignSave.hpp"

#include <QCheckBox>
#include <QMessageBox>
#include <QObject>
#include <QPushButton>

#include "../framework/Dialogs.hpp"

namespace mitcad {

QString newDesignSaveId(NewDesignSave choice) {
  switch (choice) {
  case NewDesignSave::Ask:
    return QStringLiteral("ask");
  case NewDesignSave::Computer:
    return QStringLiteral("computer");
  case NewDesignSave::Project:
    return QStringLiteral("project");
  }
  return QStringLiteral("ask");
}

std::optional<NewDesignSave> newDesignSaveFromId(const QString& id) {
  for (const NewDesignSave choice : {NewDesignSave::Ask, NewDesignSave::Computer, NewDesignSave::Project}) {
    if (id == newDesignSaveId(choice)) {
      return choice;
    }
  }
  return std::nullopt;
}

QString newDesignSaveLabel(NewDesignSave choice) {
  switch (choice) {
  case NewDesignSave::Ask:
    return QObject::tr("Ask");
  case NewDesignSave::Computer:
    return QObject::tr("On this computer");
  case NewDesignSave::Project:
    return QObject::tr("To a project");
  }
  return QObject::tr("Ask");
}

std::optional<NewDesignSaveAnswer> askNewDesignSave(QWidget* parent, const QString& name) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Save"),
                  QObject::tr("Save %1 on this computer or to a project?").arg(name), QMessageBox::NoButton, parent);
  box.setObjectName(QStringLiteral("newDesignSaveQuestion"));
  box.setInformativeText(QObject::tr("On this computer, it is a file in a folder you choose. In a project, every "
                                     "save records a version, which a project with a remote repository also "
                                     "sends there."));
  QPushButton* computer = box.addButton(QObject::tr("Save on This &Computer…"), QMessageBox::AcceptRole);
  computer->setObjectName(QStringLiteral("newDesignSaveComputer"));
  QPushButton* project = box.addButton(QObject::tr("Save to &Project…"), QMessageBox::AcceptRole);
  project->setObjectName(QStringLiteral("newDesignSaveProject"));
  QPushButton* cancel = box.addButton(QMessageBox::Cancel);
  auto* remember = new QCheckBox(QObject::tr("&Remember my choice"), &box);
  remember->setObjectName(QStringLiteral("newDesignSaveRemember"));
  remember->setToolTip(QObject::tr("Save of a new design on the General page of Tools › Preferences asks again"));
  box.setCheckBox(remember);
  box.setDefaultButton(project);
  box.setEscapeButton(cancel);
  prepareModal(&box);
  execDialog(&box);
  NewDesignSaveAnswer answer;
  if (box.clickedButton() == computer) {
    answer.choice = NewDesignSave::Computer;
  } else if (box.clickedButton() == project) {
    answer.choice = NewDesignSave::Project;
  } else {
    return std::nullopt;
  }
  answer.remember = remember->isChecked();
  return answer;
}

} // namespace mitcad
