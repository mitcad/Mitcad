// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <QMessageBox>
#include <QColorDialog>
#include <QString>
#include <QStringList>

class QDialog;
class QFileDialog;
class QMenu;
class QAction;
class QPoint;
class QProgressDialog;
class QWidget;

namespace mitcad {

// On macOS a dialog that is modal to a window of the application is a sheet
// that hangs from the window's title bar, and Qt makes one of a dialog that
// is window modal. This makes the dialog window modal before exec() or
// open(); elsewhere, and for a dialog with no parent window or whose parent
// is a dialog itself (sheets do not nest) or not shown yet, it does nothing and the dialog
// stays application modal.
void prepareModal(QDialog* dialog);

// Shared dialog/menu dispatch. The native frontend uses AppKit controls;
// the other frontends retain their existing Qt presentations.
int execDialog(QDialog* dialog);
// Call before selecting a file or name filter. AppKit panels read the
// hidden widget presenter's initial values before QFileDialog is shown.
void prepareFileDialog(QFileDialog* dialog);
void showDialog(QWidget* widget);
// QDialog::open through the shared dispatch (mitcad#1353): window modal
// and shown without a loop of its own, its result through finished,
// accepted or rejected. The native frontend presents it as an AppKit sheet
// on its window; the others call open(), a sheet on macOS too.
void openDialog(QDialog* dialog);
void waitDialogUntil(const std::function<bool()>& completed);
// Call before setValue or the minimum-duration timer can show a progress
// dialog. Native mode keeps the Qt presenter hidden and forwards its show
// lifecycle to an AppKit progress sheet.
void prepareProgressDialog(QProgressDialog* dialog);
QAction* execMenu(QMenu* menu, const QPoint& position);
QString dialogText(QWidget* parent, const QString& title, const QString& label,
                   const QString& initial, bool* accepted);
QString dialogItem(QWidget* parent, const QString& title, const QString& label,
                   const QStringList& items, int current, bool* accepted);
QString dialogOpenFileName(QWidget* parent, const QString& title = {}, const QString& directory = {},
                           const QString& filter = {}, QString* selectedFilter = nullptr);
QString dialogSaveFileName(QWidget* parent, const QString& title = {}, const QString& directory = {},
                           const QString& filter = {}, QString* selectedFilter = nullptr);
QString dialogDirectory(QWidget* parent, const QString& title = {}, const QString& directory = {});
// The save dialog of a DXF export (mitcad#1408): its file types are the
// length units the file can be written in ("DXF drawing in inches
// (*.dxf)"), `unit` (mm, in, …: the design's) first and chosen; on
// return `unit` is the one chosen. Empty when cancelled.
QString dialogSaveDxf(QWidget* parent, const QString& title, const QString& suggested, QString& unit);
// The units a DXF export offers: `first` (the design's), then mm, cm, m,
// in and ft.
QStringList dxfUnits(const QString& first);
// "DXF drawing in inches (*.dxf)": the save dialog's file type of a unit.
QString dxfUnitFilter(const QString& unit);
// An existing file, with the dialog's buttons named (Use, Skip): a file the
// user may decline to give (mitcad#196). Empty when declined.
QString dialogChooseFile(QWidget* parent, const QString& title, const QString& directory, const QString& filter,
                         const QString& acceptLabel, const QString& rejectLabel);
QColor dialogColor(const QColor& initial, QWidget* parent, const QString& title = {},
                   QColorDialog::ColorDialogOptions options = {});

// QMessageBox::warning, information and question as sheets (prepareModal),
// with the same arguments and result.
QMessageBox::StandardButton sheetWarning(QWidget* parent, const QString& title, const QString& text,
                                         QMessageBox::StandardButtons buttons = QMessageBox::Ok,
                                         QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);
QMessageBox::StandardButton sheetInformation(QWidget* parent, const QString& title, const QString& text,
                                             QMessageBox::StandardButtons buttons = QMessageBox::Ok,
                                             QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);
// An error's dialog (mitcad#1040, docs/ui-conventions.md "Messages and
// severity"): what was asked did not happen (a file not read or not
// written, an internal error); the Critical icon.
QMessageBox::StandardButton sheetCritical(QWidget* parent, const QString& title, const QString& text,
                                          QMessageBox::StandardButtons buttons = QMessageBox::Ok,
                                          QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);
QMessageBox::StandardButton sheetQuestion(QWidget* parent, const QString& title, const QString& text,
                                          QMessageBox::StandardButtons buttons = QMessageBox::StandardButtons(
                                              QMessageBox::Yes | QMessageBox::No),
                                          QMessageBox::StandardButton defaultButton = QMessageBox::NoButton);

} // namespace mitcad
