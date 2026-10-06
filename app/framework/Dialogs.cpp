// SPDX-License-Identifier: MIT
#include "Dialogs.hpp"

#include <QDialog>
#include <QWidget>
#include <QtGlobal>

namespace mitcad {

void prepareModal(QDialog* dialog) {
#ifdef Q_OS_MACOS
  const QWidget* parent = dialog != nullptr && dialog->parentWidget() != nullptr ? dialog->parentWidget()->window()
                                                                                 : nullptr;
  if (parent != nullptr && parent->isVisible() && qobject_cast<const QDialog*>(parent) == nullptr) {
    dialog->setWindowModality(Qt::WindowModal);
  }
#else
  Q_UNUSED(dialog);
#endif
}

namespace {

QMessageBox::StandardButton show(QMessageBox::Icon icon, QWidget* parent, const QString& title, const QString& text,
                                 QMessageBox::StandardButtons buttons, QMessageBox::StandardButton defaultButton) {
  QMessageBox box(icon, title, text, buttons, parent);
  if (defaultButton != QMessageBox::NoButton) {
    box.setDefaultButton(defaultButton);
  }
  prepareModal(&box);
  box.exec();
  return box.standardButton(box.clickedButton());
}

} // namespace

QMessageBox::StandardButton sheetWarning(QWidget* parent, const QString& title, const QString& text,
                                         QMessageBox::StandardButtons buttons,
                                         QMessageBox::StandardButton defaultButton) {
  return show(QMessageBox::Warning, parent, title, text, buttons, defaultButton);
}

QMessageBox::StandardButton sheetInformation(QWidget* parent, const QString& title, const QString& text,
                                             QMessageBox::StandardButtons buttons,
                                             QMessageBox::StandardButton defaultButton) {
  return show(QMessageBox::Information, parent, title, text, buttons, defaultButton);
}

QMessageBox::StandardButton sheetQuestion(QWidget* parent, const QString& title, const QString& text,
                                          QMessageBox::StandardButtons buttons,
                                          QMessageBox::StandardButton defaultButton) {
  return show(QMessageBox::Question, parent, title, text, buttons, defaultButton);
}

} // namespace mitcad
