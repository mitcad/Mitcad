// SPDX-License-Identifier: MIT
#pragma once

// File › Print… (mitcad#1089): what is printed as a picture in millimetres,
// how the pictures go on pages, and the system's print dialog.
//
// - A drawing's sheets: each sheet on a page of its own size, at its scale
//   (printSheets); a printer that cannot change its paper between pages gets
//   each sheet fitted to its paper.
// - A sketch or a cut sheet: one picture at 1:1, tiled over as many pages as
//   it needs with an overlap, alignment marks and each page's row and column
//   (planActual), or fitted to one page (planFit).
// - The Cut List and Bill of Materials print as their PDF lays them out
//   (build/BuildLists.hpp, paintListPages).
//
// The pages are painted in the printable area's millimetres (the paint
// rectangle of the printer's page layout), y down; paintPage scales them to
// the device. With MITCAD_TEST_PRINT_PDF set to a file, askPrinter skips
// the system's dialog and prints to that PDF file instead (the UI tests).

#include <functional>
#include <vector>

#include <QLineF>
#include <QPageLayout>
#include <QPointF>
#include <QRectF>
#include <QSizeF>
#include <QString>
#include <QTransform>
#include <QVector>

class QPainter;
class QPrinter;
class QWidget;

namespace mitcad::printing {

// Something to print: its size in millimetres and how it paints itself in
// them (x right, y down, the origin at its top left). `scale` is how many
// millimetres of paper a millimetre of the picture takes (1 at 1:1), so
// that texts, arrows and line widths can keep their size on paper.
struct PrintPicture {
  QString name;
  QSizeF size;
  std::function<void(QPainter& painter, double scale)> paint;
  bool valid() const { return size.width() > 0.0 && size.height() > 0.0 && paint != nullptr; }
};

enum class PrintScale { Actual, Fit };

// What a page shows, in the printable area's millimetres (y down).
struct PrintPage {
  int row = 0;    // from 0
  int column = 0; // from 0
  QRectF source;  // the part of the picture on the page (picture mm)
  QRectF area;    // where it goes on the page (page mm): the clip
  QTransform toPage; // picture mm to page mm
  QString label;     // "Row 1, column 2 (page 2 of 4)"; empty on a single page
  std::vector<QLineF> guides;  // the overlaps' edges (page mm), dashed
  std::vector<QPointF> marks;  // alignment crosses (page mm)
};

struct PrintPlan {
  int rows = 1;
  int columns = 1;
  double scale = 1.0;   // paper mm per picture mm
  double overlap = 0.0; // mm, between tiles
  QSizeF printable;
  std::vector<PrintPage> pages;
  int pageCount() const { return static_cast<int>(pages.size()); }
};

// The overlap of tiled pages and the band at the top of a tiled page that
// holds its label (mm).
constexpr double kTileOverlap = 10.0;
constexpr double kLabelBand = 6.0;
// The margins of a page that prints a picture (mm), before the printer's
// own minimum.
constexpr double kPageMargin = 10.0;

// A picture at 1:1 on pages of a printable area: one page, centred, when it
// fits; else tiles in rows and columns under a label band, each overlapping
// the next by `overlap`, with the overlaps' edges and alignment crosses
// that come out at the same place of the picture on both pages.
PrintPlan planActual(const QSizeF& picture, const QSizeF& printable, double overlap = kTileOverlap);
// A picture scaled to one page, centred, as large as fits.
PrintPlan planFit(const QSizeF& picture, const QSizeF& printable);
PrintPlan plan(PrintScale scale, const QSizeF& picture, const QSizeF& printable);

// The printable area of a page layout (mm), and a layout of a paper with
// the margins of a picture's page.
QSizeF printableSize(const QPageLayout& layout);
QPageLayout pictureLayout(QPageSize::PageSizeId paper, QPageLayout::Orientation orientation);
// The orientation of `paper` in which the picture needs fewer pages (1:1)
// or is larger (fit); portrait when both are the same.
QPageLayout::Orientation bestOrientation(PrintScale scale, const QSizeF& picture, QPageSize::PageSizeId paper);

// A paper as the Print panel remembers it (`print/paper`): its key ("A4",
// "Letter"), the same in every language of the UI (mitcad#1156).
QString paperSetting(QPageSize::PageSizeId paper);
// The paper of a remembered setting among `papers`: by its key, else (a
// setting from before mitcad#1156) by its display name; `fallback` when
// none matches.
QPageSize::PageSizeId paperFromSetting(const QString& setting, const std::vector<QPageSize::PageSizeId>& papers,
                                       QPageSize::PageSizeId fallback);

// Paints one page: `painter` has its origin at the printable area's top left
// in device pixels, `pixelsPerMm` of them a millimetre.
void paintPage(QPainter& painter, double pixelsPerMm, const PrintPlan& pagePlan, const PrintPage& page,
               const PrintPicture& picture);

// The pages of `to` (from 1, ascending) that print what pages `pages` (from
// 1) of `from` show, both plans of the same picture: in each of those
// pages' source, the fewest columns of `to` that cover its width and the
// fewest rows that cover its height (each the column or row that reaches
// furthest from where the cover has got to). The same pages when `to` lays
// the picture out as `from` does; pages out of `from` are left out.
std::vector<int> mapPages(const PrintPlan& from, const std::vector<int>& pages, const PrintPlan& to);

// Prints the picture on the printer's pages (its page layout as it is): the
// pages the printer's page range asks for, or all. With `offered` (the plan
// whose page count the print dialog's range was offered for, mitcad#1156),
// the range picks pages of that plan, mapped onto the pages printed
// (mapPages) when the dialog changed the paper or its orientation. The
// number of pages printed, or -1 with the reason.
int printPicture(QPrinter& printer, const PrintPicture& picture, PrintScale scale, QString& error,
                 const PrintPlan* offered = nullptr);

// A page layout of a sheet's size: the paper of that size, in the
// orientation the size gives, without margins.
QPageLayout sheetLayout(const QSizeF& sheet);
// Prints sheets each on a page of its size, at its scale, the printer set
// to full pages (the page range's sheets, or all). A printer that cannot
// change its paper while it prints (any but a PDF file) takes each sheet's
// size only for the first page; a sheet whose size its paper does not have
// is fitted to it, centred (`fitted` counts them). The number of pages, or
// -1 with the reason.
int printSheets(QPrinter& printer, const QVector<PrintPicture>& sheets, QString& error, int* fitted = nullptr);

// A drawing's sheet as the model wrote it to an SVG file (millimetres: its
// view box); invalid with the reason when the file cannot be read.
PrintPicture svgPicture(const QString& path, QString* error = nullptr);

// Whether page `number` (from 1) is to be printed: the printer's page
// range, or every page when it has none.
bool pageWanted(const QPrinter& printer, int number);

// The system's print dialog for a document of `pages` pages (0: unknown,
// then without a page range): printer, paper, copies. False when it was
// cancelled. With MITCAD_TEST_PRINT_PDF set, no dialog: the printer prints
// to that PDF file (logged "Print dialog skipped (test): <path>").
bool askPrinter(QPrinter& printer, QWidget* parent, int pages);
// Where the printer prints, for the log: the PDF file, or the printer's
// name.
QString printerTarget(const QPrinter& printer);

} // namespace mitcad::printing
