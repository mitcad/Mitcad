// SPDX-License-Identifier: MIT
// File › Print… (mitcad#1089, app/COMMANDS.md "Print"): the command
// `file.print` (Ctrl+P), enabled while something can be printed:
// - the Cut List or Bill of Materials window (modal: its own Print… button
//   and Ctrl+P print it, and this command does while it is open): the tab
//   shown as its PDF lays it out;
// - an open drawing: its sheets as the model writes them (SVG, the same
//   sheets as its PDF), each on a page of its size;
// - an open cut sheet or sketch: the Print panel (1:1 tiled, or fit to
//   page; a sketch's dimensions and constraint symbols), then the printer.
// The system's print dialog chooses the printer, the paper and the copies
// (printing::askPrinter, which a UI test replaces with a PDF file).
#include "../MainWindow.hpp"

#include <memory>
#include <utility>
#include <vector>

#include <QApplication>
#include <QDir>
#include <QPrinter>
#include <QStringList>
#include <QTemporaryDir>
#include <QVector>
#include <QtLogging>

#include "../build/BuildLists.hpp"
#include "../drawings/DrawingWorkspace.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../sketch/SketchOverlay.hpp"
#include "PaperPrint.hpp"
#include "PrintPanel.hpp"
#include "SketchPicture.hpp"

namespace mitcad {
namespace {

sketch::V2 v2(const QPointF& p) { return {p.x(), p.y()}; }

QString millimetres(double value) { return QString::number(value, 'g', 6); }

} // namespace

MainWindow::PrintTarget MainWindow::printTarget() const {
  if (qobject_cast<build::BuildListsDialog*>(QApplication::activeModalWidget()) != nullptr) {
    return PrintTarget::Lists;
  }
  if (drawingOpen()) {
    return m_drawingWorkspace->isCutSheet() ? PrintTarget::CutSheet : PrintTarget::Drawing;
  }
  // A sketch with something to draw: a fresh one would print a blank page
  // (mitcad#1156).
  if (m_sketch != nullptr && m_sketch->isActive() && printing::sketchPrintsAnything(m_sketch->model())) {
    return PrintTarget::Sketch;
  }
  return PrintTarget::None;
}

CommandDef MainWindow::paperPrintCommand() {
  CommandDef def;
  def.id = QStringLiteral("file.print");
  def.name = tr("Print…");
  def.icon = QStringLiteral("print");
  def.tooltip = tr("Prints the drawing, the cut list or bill of materials, or the open sketch");
  def.shortcut = QKeySequence(QKeySequence::Print);
  def.kind = CommandDef::Kind::Action;
  def.mode = CommandDef::Mode::Any;
  def.tab.clear(); // the File menu
  def.group = QStringLiteral("INSERT");
  def.keywords = {QStringLiteral("printer"), QStringLiteral("paper"), QStringLiteral("page"),
                  QStringLiteral("tile"),    QStringLiteral("template"), QStringLiteral("1:1"),
                  QStringLiteral("drawing"), QStringLiteral("sketch"), QStringLiteral("cut list"),
                  QStringLiteral("bill of materials")};
  def.enabled = [this] { return printTarget() != PrintTarget::None; };
  def.run = [this] { printCurrent(); };
  return def;
}

void MainWindow::logPrintTarget() {
  QString line;
  switch (printTarget()) {
  case PrintTarget::None:
    line = QStringLiteral("nothing");
    break;
  case PrintTarget::Lists:
    line = QStringLiteral("lists");
    break;
  case PrintTarget::Drawing:
    line = QStringLiteral("drawing %1").arg(m_drawingWorkspace->name());
    break;
  case PrintTarget::CutSheet:
    line = QStringLiteral("cut sheet %1").arg(m_drawingWorkspace->name());
    break;
  case PrintTarget::Sketch:
    line = QStringLiteral("sketch %1").arg(m_sketch->model().name);
    break;
  }
  if (line != m_loggedPrintTarget) {
    m_loggedPrintTarget = line;
    qInfo().noquote() << QStringLiteral("Print available: %1").arg(line);
  }
}

void MainWindow::printCurrent() {
  const PrintTarget target = printTarget();
  if (target == PrintTarget::None) {
    showStatus(tr("There is nothing to print: open a drawing, a sketch with something drawn, or the Cut List."),
               NoticeLevel::Information);
    return;
  }
  if (target == PrintTarget::Lists) {
    if (auto* lists = qobject_cast<build::BuildListsDialog*>(QApplication::activeModalWidget())) {
      lists->print();
    }
    return;
  }
  const auto failed = [this](const QString& what, const QString& error) {
    qWarning().noquote() << QStringLiteral("Print of %1 failed: %2").arg(what, error);
    showStatus(tr("Could not print %1: %2").arg(what, error), NoticeLevel::Error);
  };

  if (target == PrintTarget::Drawing) {
    // The sheets as the model writes them, each on a page of its size.
    const QString name = m_drawingWorkspace->name();
    QTemporaryDir folder;
    if (!folder.isValid()) {
      failed(name, folder.errorString());
      return;
    }
    QString error;
    const QStringList files = m_drawingWorkspace->writeSheetsSvg(folder.path(), error);
    if (files.isEmpty()) {
      failed(name, error);
      return;
    }
    QVector<printing::PrintPicture> sheets;
    QStringList sizes;
    for (const QString& file : files) {
      printing::PrintPicture sheet = printing::svgPicture(file, &error);
      if (!sheet.valid()) {
        failed(name, error);
        return;
      }
      sizes << QStringLiteral("%1 x %2 mm").arg(millimetres(sheet.size.width()), millimetres(sheet.size.height()));
      sheets << sheet;
    }
    // Found without freezing the window (mitcad#1591).
    const std::unique_ptr<QPrinter> made = printing::newPrinter(this);
    QPrinter& printer = *made;
    printer.setDocName(name);
    printer.setCreator(QStringLiteral("MitCad"));
    printer.setFullPage(true);
    printer.setPageLayout(printing::sheetLayout(sheets.first().size));
    if (!printing::askPrinter(printer, this, static_cast<int>(sheets.size()))) {
      return;
    }
    int fitted = 0;
    const int pages = printing::printSheets(printer, sheets, error, &fitted);
    if (pages < 0) {
      failed(name, error);
      return;
    }
    qInfo().noquote() << QStringLiteral("Printed drawing %1: %2 page(s), sheets %3%4 to %5")
                             .arg(name)
                             .arg(pages)
                             .arg(sizes.join(QStringLiteral(", ")),
                                  fitted > 0 ? QStringLiteral(", %1 fitted to the paper").arg(fitted) : QString(),
                                  printing::printerTarget(printer));
    showStatus(tr("Printed %1 on %n page(s).", nullptr, pages).arg(name), NoticeLevel::Information);
    return;
  }

  // A cut sheet or a sketch: one picture, tiled at 1:1 or fitted.
  printing::PrintPanel::PictureFor pictureFor;
  QString title;
  bool sketchOptions = false;
  QTemporaryDir folder;
  if (target == PrintTarget::CutSheet) {
    const QString name = m_drawingWorkspace->name();
    QString error;
    const QStringList files =
        folder.isValid() ? m_drawingWorkspace->writeSheetsSvg(folder.path(), error) : QStringList();
    printing::PrintPicture sheet = files.isEmpty() ? printing::PrintPicture() : printing::svgPicture(files.first(), &error);
    if (!sheet.valid()) {
      failed(name, error.isEmpty() ? folder.errorString() : error);
      return;
    }
    sheet.name = name;
    pictureFor = [sheet](const printing::SketchPrintOptions&) { return sheet; };
    title = tr("Print Cut Sheet");
  } else {
    // The dimensions as sketch mode lays them out, in sketch millimetres:
    // its offsets of about a printed point at 1:1.
    pictureFor = [this](const printing::SketchPrintOptions& options) {
      const sketch::SketchModel& model = m_sketch->model();
      std::vector<printing::SketchDimensionDrawing> dimensions;
      if (options.dimensions) {
        sketch::Projection projection;
        projection.toScreen = [](const sketch::V2& p) { return QPointF(p.x, p.y); };
        projection.pixel = 0.35;
        for (const sketch::DimensionData& d : model.dimensions) {
          const sketch::DimensionShape shape =
              sketch::dimensionShape(d, model, projection, m_sketch->overlay().dimensionText(d));
          if (!shape.valid) {
            continue;
          }
          printing::SketchDimensionDrawing drawing;
          for (const QLineF& line : shape.lines) {
            drawing.lines.emplace_back(v2(line.p1()), v2(line.p2()));
          }
          for (const QPolygonF& arc : shape.arcs) {
            std::vector<sketch::V2> points;
            for (const QPointF& p : arc) {
              points.push_back(v2(p));
            }
            drawing.arcs.push_back(points);
          }
          for (const auto& arrow : shape.arrows) {
            drawing.arrows.emplace_back(v2(arrow.first), v2(arrow.second));
          }
          drawing.textAt = v2(shape.textAt);
          drawing.text = shape.text;
          dimensions.push_back(drawing);
        }
      }
      return printing::sketchPicture(model, options, dimensions);
    };
    title = tr("Print Sketch");
    sketchOptions = true;
  }
  printing::PrintPanel panel(title, pictureFor, sketchOptions, this);
  prepareModal(&panel);
  if (execDialog(&panel) != QDialog::Accepted) {
    return;
  }
  const printing::PrintPicture picture = panel.picture();
  const std::unique_ptr<QPrinter> made = printing::newPrinter(this);
  QPrinter& printer = *made;
  printer.setDocName(picture.name);
  printer.setCreator(QStringLiteral("MitCad"));
  printer.setPageLayout(panel.pageLayout());
  // The dialog's page range counts the panel's pages; printPicture maps it
  // onto the pages of the paper the dialog leaves (mitcad#1156).
  const printing::PrintPlan offered = panel.plan();
  if (!printing::askPrinter(printer, this, offered.pageCount())) {
    return;
  }
  QString error;
  const int pages = printing::printPicture(printer, picture, panel.scale(), error, &offered);
  if (pages < 0) {
    failed(picture.name, error);
    return;
  }
  // The pages as printed: the dialog may have changed the paper.
  const printing::PrintPlan printed =
      printing::plan(panel.scale(), picture, printing::printableSize(printer.pageLayout()));
  qInfo().noquote() << QStringLiteral("Printed %1: %2 page(s), %3, %4 x %5 to %6")
                           .arg(picture.name)
                           .arg(pages)
                           .arg(panel.scale() == printing::PrintScale::Fit ? QStringLiteral("fit") : QStringLiteral("1:1"))
                           .arg(printed.rows)
                           .arg(printed.columns)
                           .arg(printing::printerTarget(printer));
  showStatus(tr("Printed %1 on %n page(s).", nullptr, pages).arg(picture.name), NoticeLevel::Information);
}

} // namespace mitcad
