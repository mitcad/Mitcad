// SPDX-License-Identifier: MIT
// Insert another document's saved state through the normal component workflow.
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"
#include "../framework/Dialogs.hpp"
#include "SaveComponentTarget.hpp"

#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QHash>
#include <QLabel>
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QVBoxLayout>

namespace mitcad {

void MainWindow::insertOpenDesign() {
  if (!canChangeModel() || !m_windowManager) return;
  const QPointer<MainWindow> destination = this;
  const QPointer<WindowManager> manager = m_windowManager;
  QPointer<QDialog> dialog = new QDialog(this);
  dialog->setObjectName(QStringLiteral("insertOpenDesign"));
  dialog->setWindowTitle(tr("Insert from an Open Design"));
  auto* layout = new QVBoxLayout(dialog);
  auto* description = new QLabel(tr("Choose another open design to insert as a component."), dialog);
  description->setWordWrap(true);
  layout->addWidget(description);
  auto* choices = new QComboBox(dialog);
  choices->setObjectName(QStringLiteral("openDesignChoice"));
  choices->setAccessibleName(tr("Open design"));
  layout->addWidget(choices);
  auto* details = new QLabel(dialog);
  details->setObjectName(QStringLiteral("openDesignDetails"));
  details->setWordWrap(true);
  details->setTextInteractionFlags(Qt::TextSelectableByMouse);
  layout->addWidget(details);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, dialog);
  auto* insert = buttons->button(QDialogButtonBox::Ok);
  insert->setText(tr("&Insert"));
  insert->setObjectName(QStringLiteral("openDesignInsert"));
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
  const auto updateDetails = [manager, destination, choices, details, insert] {
    MainWindow* source = manager ? manager->findByHandle(choices->currentData().toString()) : nullptr;
    insert->setEnabled(source && source != destination && !source->modelBusy());
    if (!source) {
      details->setText(tr("Open another design first."));
    } else {
      const QString path = source->documentFilePath();
      const bool needsSave = path.isEmpty() || source->isWindowModified() || !QFileInfo(path).isFile();
      details->setText((path.isEmpty() ? tr("This design has not been saved.") : QDir::toNativeSeparators(path)) +
                       (needsSave ? tr("\nSave this design before inserting it.") : tr("\nInserts the saved design.")));
    }
  };
  const auto refresh = [manager, destination, choices, updateDetails] {
    const QString selected = choices->currentData().toString();
    choices->clear();
    if (manager) {
      QHash<QString, int> names;
      for (MainWindow* source : manager->windows()) {
        if (source != destination) ++names[source->documentName()];
      }
      for (MainWindow* source : manager->windows()) {
        if (source == destination) continue;
        const QString path = source->documentFilePath();
        QString label = source->documentName();
        if (path.isEmpty()) {
          label += QStringLiteral(" (%1)").arg(manager->handle(source));
        } else if (names.value(label) > 1) {
          label += QStringLiteral(" — %1").arg(QDir::toNativeSeparators(QFileInfo(path).absolutePath()));
        }
        choices->addItem(label, manager->handle(source));
        choices->setItemData(choices->count() - 1, QDir::toNativeSeparators(path), Qt::ToolTipRole);
      }
    }
    const int retained = choices->findData(selected);
    if (retained >= 0) choices->setCurrentIndex(retained);
    updateDetails();
  };
  connect(choices, &QComboBox::currentIndexChanged, dialog, updateDetails);
  connect(manager, &WindowManager::windowsChanged, dialog, refresh);
  refresh();
  prepareModal(dialog);
  const bool accepted = execDialog(dialog) == QDialog::Accepted;
  const QString handle = dialog ? choices->currentData().toString() : QString();
  if (dialog) dialog->deleteLater();
  if (!accepted || !destination || !manager || !manager->windows().contains(destination)) return;
  QPointer<MainWindow> source = manager->findByHandle(handle);
  if (!source || source == destination || source->modelBusy() || source->m_closePending || !source->m_import.isNull()) return;

  if (sameFilePath(source->m_filePath, destination->m_filePath)) {
    destination->showHint(tr("A design cannot be inserted into itself."));
    return;
  }
  const bool needsSave = source->m_filePath.isEmpty() || source->isModified() || !QFileInfo(source->m_filePath).isFile();
  if (needsSave) {
    // Save belongs to the source document, including its sheet and Save As
    // dialogs. Return to the original destination before component insertion.
    manager->present(source);
    bool saved = false;
    if (destination && source && manager && manager->windows().contains(source)) {
      const auto answer = sheetQuestion(source, tr("Save Design Before Inserting"),
          tr("Save %1 before inserting it?").arg(source->documentName()),
          QMessageBox::Save | QMessageBox::Cancel, QMessageBox::Save);
      if (answer == QMessageBox::Save && destination && source && manager &&
          manager->windows().contains(source) && !source->modelBusy()) saved = source->save();
    }
    if (destination && manager && manager->windows().contains(destination)) manager->present(destination);
    if (!saved) return;
  }
  if (!destination || !source || !manager || !manager->windows().contains(destination) ||
      !manager->windows().contains(source) || source->modelBusy() || source->isModified()) return;
  const QString path = source->m_filePath;
  if (path.isEmpty() || !QFileInfo(path).isFile()) return;
  if (source == destination || sameFilePath(path, destination->m_filePath)) {
    destination->showHint(tr("A design cannot be inserted into itself."));
    return;
  }
  destination->insertComponent(path);
}

} // namespace mitcad
