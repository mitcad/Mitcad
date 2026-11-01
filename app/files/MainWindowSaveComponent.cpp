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
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../projects/ProjectPicker.hpp"

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
  if (!m_filePath.isEmpty() && QFileInfo(m_filePath).absoluteFilePath() == absolute) {
    sheetWarning(this, tr("Save Component as Design"),
                 tr("Could not save %1 over this design's own file: choose another file.").arg(shown));
    return;
  }

  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("save_component_as_design")},
                      {QStringLiteral("component"), component},
                      {QStringLiteral("path"), absolute},
                      {QStringLiteral("replace"), *replace}};
  const QString base = documentFolder();
  if (!base.isEmpty()) {
    command.insert(QStringLiteral("base"), base);
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
  if (const std::optional<rust::Box<Project>> project = versionedProject(absolute)) {
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
