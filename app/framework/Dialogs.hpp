// SPDX-License-Identifier: MIT
#pragma once

#include <QMessageBox>
#include <QString>

class QDialog;
class QWidget;

namespace mitcad {

// On macOS a dialog that is modal to a window of the application is a sheet
// that hangs from the window's title bar, and Qt makes one of a dialog that
// is window modal. This makes the dialog window modal before exec() or
// open(); elsewhere, and for a dialog with no parent window or whose parent
// is a dialog itself (sheets do not nest) or not shown yet, it does nothing and the dialog
// stays application modal.
void prepareModal(QDialog* dialog);

// QMessageBox::warning, information and question as sheets (prepareModal),
// with the same arguments and result.
QMessageBox::StandardButton sheetWarning(QWidget* parent, const QString& title, const QString& text,
                                         QMessageBox::StandardButtons buttons = QMessageBox::Ok,
                                         QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);
QMessageBox::StandardButton sheetInformation(QWidget* parent, const QString& title, const QString& text,
                                             QMessageBox::StandardButtons buttons = QMessageBox::Ok,
                                             QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);
QMessageBox::StandardButton sheetQuestion(QWidget* parent, const QString& title, const QString& text,
                                          QMessageBox::StandardButtons buttons = QMessageBox::StandardButtons(
                                              QMessageBox::Yes | QMessageBox::No),
                                          QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);

} // namespace mitcad
