// SPDX-License-Identifier: MIT
// File › Drawing Book… (mitcad#2448, GitHub mitcad/Mitcad#687,
// app/COMMANDS.md "Drawing book"): the command `file.drawing_book`. Its
// dialog (drawings/DrawingBookDialog) offers the drawings of the design and
// of the designs it links, each linked design once, from the model's
// `drawing_book` query; Save and Print… keep the choice with the design as
// one undo step (`set_drawing_book`), and Print… then prints the book as
// one job: the model writes every sheet of the chosen drawings in the
// book's order (`drawing_book_sheets`, drawn from the linked files as they
// are now, read only), the cover page (printing/BookCover) goes first when
// it is checked, and the pages go to the system's print dialog as File ›
// Print… sends a drawing's (printing::printSheets: each sheet on a page of
// its size).
#include "../MainWindow.hpp"

#include <exception>
#include <memory>

#include <QApplication>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QPrinter>
#include <QStringList>
#include <QTemporaryDir>
#include <QVector>
#include <QtLogging>

#include "../drawings/DrawingBookDialog.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "BookCover.hpp"
#include "PaperPrint.hpp"

namespace mitcad {

CommandDef MainWindow::drawingBookCommand() {
  CommandDef def;
  def.id = QStringLiteral("file.drawing_book");
  def.name = tr("Drawing Book…");
  def.icon = QStringLiteral("print");
  def.tooltip = tr("Prints chosen drawings of this design and its linked designs as one book");
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Model;
  def.keywords = {QStringLiteral("drawing book"), QStringLiteral("print set"), QStringLiteral("cover page"),
                  QStringLiteral("linked designs"), QStringLiteral("drawings"), QStringLiteral("print")};
  def.run = [this] { editDrawingBook(); };
  return def;
}

void MainWindow::editDrawingBook() {
  if (!canChangeModel()) {
    showError(changeBlockedReason());
    return;
  }
  // The linked files are followed from the design file's folder.
  const QString base = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("drawing_book")}};
  if (!base.isEmpty()) {
    request.insert(QStringLiteral("base"), base);
  }
  QJsonObject book;
  QApplication::setOverrideCursor(Qt::WaitCursor);
  try {
    book = queryObject(request);
  } catch (const std::exception& e) {
    QApplication::restoreOverrideCursor();
    showError(tr("Could not read the drawing book: %1").arg(errorText(e)));
    return;
  }
  QApplication::restoreOverrideCursor();
  drawings::DrawingBookDialog dialog(book, drawingTitle(), this);
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Drawing Book cancelled");
    return;
  }
  QJsonObject result;
  if (!runModelCommand(dialog.command(), &result)) {
    return; // the model's refusal is shown
  }
  const QStringList chosen = dialog.chosenNames();
  qInfo().noquote() << QStringLiteral("Drawing Book saved: %1; cover %2; %3")
                           .arg(chosen.isEmpty() ? QStringLiteral("none") : chosen.join(QStringLiteral(" | ")),
                                dialog.command().value(QStringLiteral("cover")).toBool() ? QStringLiteral("on")
                                                                                          : QStringLiteral("off"),
                                result.value(QStringLiteral("changed")).toBool() ? QStringLiteral("changed")
                                                                                 : QStringLiteral("unchanged"));
  if (dialog.printAsked()) {
    printDrawingBook();
  }
}

void MainWindow::printDrawingBook() {
  const auto failed = [this](const QString& error) {
    qWarning().noquote() << QStringLiteral("Print of the drawing book failed: %1").arg(error);
    showStatus(tr("Could not print the drawing book: %1").arg(error), NoticeLevel::Error);
  };
  QTemporaryDir folder;
  if (!folder.isValid()) {
    failed(folder.errorString());
    return;
  }
  // Every sheet of the chosen drawings in the book's order, drawn by the
  // model from the designs' files as they are now.
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("drawing_book_sheets")},
                      {QStringLiteral("folder"), folder.path()},
                      {QStringLiteral("title"), drawingTitle()}};
  if (!m_filePath.isEmpty()) {
    request.insert(QStringLiteral("base"), QFileInfo(m_filePath).absolutePath());
  }
  QJsonObject sheets;
  QApplication::setOverrideCursor(Qt::WaitCursor);
  try {
    sheets = queryObject(request);
  } catch (const std::exception& e) {
    QApplication::restoreOverrideCursor();
    failed(errorText(e));
    return;
  }
  QApplication::restoreOverrideCursor();
  QString title = sheets.value(QStringLiteral("title")).toString();
  if (title.isEmpty()) {
    title = drawingTitle();
  }
  QVector<printing::PrintPicture> pages;
  QStringList printed;
  QString error;
  for (const QJsonValue& value : sheets.value(QStringLiteral("files")).toArray()) {
    const QJsonObject file = value.toObject();
    printing::PrintPicture sheet = printing::svgPicture(file.value(QStringLiteral("file")).toString(), &error);
    if (!sheet.valid()) {
      failed(error);
      return;
    }
    pages << sheet;
  }
  if (pages.isEmpty()) {
    failed(tr("The drawing book has no sheet."));
    return;
  }
  // The contents: each drawing with its design and its sheets.
  QStringList contents;
  for (const QJsonValue& value : sheets.value(QStringLiteral("drawings")).toArray()) {
    const QJsonObject drawing = value.toObject();
    const QString design = drawing.value(QStringLiteral("source")).isNull()
                               ? tr("This design")
                               : drawing.value(QStringLiteral("design")).toString();
    const int count = drawing.value(QStringLiteral("sheets")).toInt();
    contents << tr("%1 of %2, %n sheet(s)", nullptr, count)
                    .arg(drawing.value(QStringLiteral("name")).toString(), design);
    printed << QStringLiteral("%1/%2 (%3)")
                   .arg(drawing.value(QStringLiteral("source")).isNull() ? QStringLiteral("This design") : design,
                        drawing.value(QStringLiteral("name")).toString())
                   .arg(count);
  }
  const bool cover = sheets.value(QStringLiteral("cover")).toBool();
  if (cover) {
    pages.prepend(printing::bookCover(title, contents, pages.first().size));
  }
  // Found without freezing the window (mitcad#1591).
  const std::unique_ptr<QPrinter> made = printing::newPrinter(this);
  QPrinter& printer = *made;
  printer.setDocName(title);
  printer.setCreator(QStringLiteral("MitCad"));
  printer.setFullPage(true);
  printer.setPageLayout(printing::sheetLayout(pages.first().size));
  if (!printing::askPrinter(printer, this, static_cast<int>(pages.size()))) {
    return;
  }
  int fitted = 0;
  const int count = printing::printSheets(printer, pages, error, &fitted);
  if (count < 0) {
    failed(error);
    return;
  }
  qInfo().noquote() << QStringLiteral("Printed drawing book %1: %2 page(s), %3%4%5 to %6")
                           .arg(title)
                           .arg(count)
                           .arg(cover ? QStringLiteral("cover, ") : QString(), printed.join(QStringLiteral(", ")),
                                fitted > 0 ? QStringLiteral(", %1 fitted to the paper").arg(fitted) : QString(),
                                printing::printerTarget(printer));
  showStatus(tr("Printed the drawing book %1 on %n page(s).", nullptr, count).arg(title), NoticeLevel::Information);
}

} // namespace mitcad
