// SPDX-License-Identifier: MIT
// Save Component as Design (mitcad#1057): a component's browser row writes
// the component as a design file of its own. A dialog lists what it refers
// to outside itself, which the new design freezes at its current geometry
// or leaves out (the model's `save_component_as_design` query), and offers
// Replace with a link to the new file; Save… asks for the file as Save As
// does (the project picker in a project with versions, else the file
// dialog in the design's folder), with the component's name proposed, and
// the model's `save_component_as_design` writes it.
#include "../MainWindow.hpp"

#include <optional>

#include <QCheckBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
#include <QListWidget>
#include <QPushButton>
#include <QStringList>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../projects/ProjectPicker.hpp"
#include "RemoteController.hpp"
#include "SaveComponentTarget.hpp"

namespace mitcad {
namespace {

// The dialog's answer: Replace with a link, or none when cancelled.
std::optional<bool> askSaveComponent(QWidget* parent, const QString& name, const QStringList& references) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Save Component as Design"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* intro = new QLabel(QObject::tr("Saves %1 as a design file of its own, with its features, sketches, "
                                       "parameters and the components in it.")
                               .arg(name));
  intro->setWordWrap(true);
  layout->addWidget(intro);
  if (references.isEmpty()) {
    layout->addWidget(new QLabel(QObject::tr("It refers to nothing outside itself.")));
  } else {
    auto* heading = new QLabel(QObject::tr("References to other components, which the new design freezes at "
                                           "their current geometry or leaves out:"));
    heading->setWordWrap(true);
    layout->addWidget(heading);
    auto* list = new QListWidget;
    list->addItems(references);
    list->setSelectionMode(QAbstractItemView::NoSelection);
    list->setObjectName(QStringLiteral("saveComponentReferences"));
    layout->addWidget(list);
  }
  auto* replace = new QCheckBox(QObject::tr("&Replace with a link to the new file"));
  replace->setToolTip(QObject::tr("This design shows the new file's bodies in place of the component"));
  layout->addWidget(replace);
  // Parented to the dialog before Save… is made its default: a default
  // button set outside a dialog is not the dialog's, and the dialog then
  // makes the first auto-default button (Cancel) its default, which Enter
  // pressed.
  auto* buttons = new QDialogButtonBox(&dialog);
  QPushButton* save = buttons->addButton(QObject::tr("&Save…"), QDialogButtonBox::AcceptRole);
  buttons->addButton(QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  save->setDefault(true);
  qDebug().noquote() << QStringLiteral("Save Component as Design dialog opened: %1; references: %2")
                            .arg(name, references.isEmpty() ? QStringLiteral("none")
                                                            : references.join(QStringLiteral(" | ")));
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << "Save Component as Design cancelled";
    return std::nullopt;
  }
  qDebug().noquote() << QStringLiteral("Save Component as Design dialog: replace with a link: %1")
                            .arg(replace->isChecked() ? QStringLiteral("yes") : QStringLiteral("no"));
  return replace->isChecked();
}

// The files the design's components are linked from (the `components`
// query's `link`), absolute: a relative one in the design's folder `base`.
QStringList linkedFiles(const QJsonObject& components, const QString& base) {
  QStringList files;
  for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
    const QString link = value.toObject().value(QStringLiteral("link")).toString();
    if (link.isEmpty()) {
      continue;
    }
    if (QDir::isAbsolutePath(link)) {
      files << link;
    } else if (!base.isEmpty()) {
      files << QDir(base).absoluteFilePath(link);
    }
  }
  return files;
}

} // namespace

void MainWindow::saveComponentAsDesign(const QString& component, const QString& name) {
  if (m_session && (m_nativeUi || modelBusy())) {
    showHint(tr("Finish the current command before saving."));
    return;
  }
  const QJsonObject listed =
      query({{QStringLiteral("query"), QStringLiteral("save_component_as_design")},
             {QStringLiteral("component"), component}})
          .toObject();
  const QString shown = listed.value(QStringLiteral("name")).toString(name);
  const QString blocked = listed.value(QStringLiteral("blocked")).toString();
  if (listed.isEmpty() || !blocked.isEmpty()) {
    const QString why = blocked.isEmpty() ? m_lastError : blocked;
    qDebug().noquote() << QStringLiteral("Save Component as Design blocked: %1").arg(why);
    sheetWarning(this, tr("Save Component as Design"), tr("Could not save %1 as a design: %2").arg(shown, why));
    return;
  }
  QStringList references;
  for (const QJsonValue& reference : listed.value(QStringLiteral("references")).toArray()) {
    references << reference.toObject().value(QStringLiteral("message")).toString();
  }
  const std::optional<bool> replace = askSaveComponent(this, shown, references);
  if (!replace) {
    return;
  }

  // The file, as Save As asks for it: the project picker in a project with
  // versions, else the file dialog in the design's folder.
  QString path;
  bool picked = false;
  const std::pair<QString, QString> place = saveAsProjectPlace();
  if (!place.first.isEmpty()) {
    ProjectPickerOptions options;
    options.title = tr("Save in a Project");
    options.text = tr("The project and the folder for %1, and its name there:").arg(shown);
    options.acceptText = tr("&Save");
    options.otherText = tr("&Another Folder…");
    options.root = place.first;
    options.folder = place.second;
    options.name = shown;
    const std::optional<ProjectPickerResult> chosen = pickInProjects(this, options);
    if (!chosen) {
      qDebug().noquote() << "Save Component as Design cancelled";
      return;
    }
    if (!chosen->other) {
      path = chosen->path;
      picked = true;
    }
  }
  if (path.isEmpty()) {
    const QString folder =
        m_filePath.isEmpty() ? dialogFolder(DialogGroup::Open) : QFileInfo(m_filePath).absolutePath();
    path = dialogSaveFileName(this, tr("Save Design File"),
                              QDir(folder).filePath(shown + QStringLiteral(".mitcad")),
                              tr("MitCad designs (*.mitcad)"));
    if (path.isEmpty()) {
      qDebug().noquote() << "Save Component as Design cancelled";
      return;
    }
    if (QFileInfo(path).suffix().isEmpty()) {
      path += QStringLiteral(".mitcad");
    }
    rememberDialogFolder(DialogGroup::Open, path);
  }
  const QString absolute = QFileInfo(path).absoluteFilePath();
  // Not over the design's own file or a file it links, however the path is
  // spelled (mitcad#1148): a name that differs only in case on a disk that
  // ignores it, a path through a link or with "..".
  const QString base = documentFolder();
  const QStringList linked = linkedFiles(queryObject({{QStringLiteral("query"), QStringLiteral("components")}}), base);
  const SaveComponentClash clash = saveComponentClash(absolute, m_filePath, linked);
  if (clash != SaveComponentClash::None) {
    const bool own = clash == SaveComponentClash::Design;
    qDebug().noquote() << QStringLiteral("Save Component as Design refused: %1 is %2")
                              .arg(absolute, own ? QStringLiteral("this design's own file")
                                                 : QStringLiteral("a linked component's file"));
    sheetWarning(this, tr("Save Component as Design"),
                 own ? tr("Could not save %1 over this design's own file: choose another file.").arg(shown)
                     : tr("Could not save %1 over %2, which a component of this design is linked from: "
                          "choose another file.")
                           .arg(shown, QDir::toNativeSeparators(absolute)));
    return;
  }

  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("save_component_as_design")},
                      {QStringLiteral("component"), component},
                      {QStringLiteral("path"), absolute},
                      {QStringLiteral("replace"), *replace}};
  if (!base.isEmpty()) {
    command.insert(QStringLiteral("base"), base);
  }
  // In a project with version history the new file is recorded as a
  // version: a sync moving the project's branch is waited for before a file
  // in it is written, as Save does.
  const std::optional<rust::Box<Project>> project = versionedProject(absolute);
  if (project) {
    m_remote->waitForBranch();
  }
  // Through runModelCommand, which refreshes the scene, the browser and
  // the timeline afterwards: a replaced component is linked from now on.
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    sheetCritical(this, tr("Save Component as Design"),
                  tr("Could not save %1 as %2:\n%3").arg(shown, QDir::toNativeSeparators(absolute), m_lastError));
    return;
  }
  const int count = static_cast<int>(result.value(QStringLiteral("references")).toArray().size());
  const bool replaced = result.value(QStringLiteral("replaced")).toBool();
  qInfo().noquote() << QStringLiteral("Saved component %1 as design %2: %3 reference(s), replaced: %4")
                           .arg(shown, absolute)
                           .arg(count)
                           .arg(replaced ? QStringLiteral("yes") : QStringLiteral("no"));

  // In a project with versions the new file is a version of its own.
  QString status = replaced ? tr("Saved %1 as %2 and replaced it with a link to it.")
                                  .arg(shown, QDir::toNativeSeparators(absolute))
                            : tr("Saved %1 as %2.").arg(shown, QDir::toNativeSeparators(absolute));
  bool problem = false;
  if (project) {
    if (const std::optional<QString> author = versionAuthor(**project)) {
      const VersionRecord record = recordVersion(**project, absolute, QStringList{absolute},
                                                 QStringLiteral("Save %1 as a design of its own").arg(shown), *author);
      if (record.problem) {
        status = record.status;
        problem = true;
      }
    }
  }
  if (picked) {
    showDesignInPanel(absolute);
  }
  if (problem) {
    showWarning(status);
  } else {
    showHint(status);
  }
}

} // namespace mitcad
