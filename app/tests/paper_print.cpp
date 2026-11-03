// SPDX-License-Identifier: MIT
// File › Print (mitcad#1089) through QPrinter into PDF files, no printer:
// a two-sheet drawing gives two pages at the sheets' sizes (and its page
// range one of them); a 300 x 200 mm sketch at 1:1 on A4 gives two tiles,
// with overlapping sources, alignment marks at the same places of the
// picture on both and a 100 mm line 100 mm long on the page (1e-6 mm in the
// page's coordinates, painted on a device that records the lines); fit to
// page gives one page; the orientation chosen for each. mitcad#1156: an
// empty sketch prints nothing; the page range of the panel's paper mapped
// onto another paper's pages; the paper remembered by its key whatever the
// language's name for it.
#include "printing/PaperPrint.hpp"
#include "printing/SketchPicture.hpp"

#include <climits>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <vector>

#include <QCoreApplication>
#include <QFile>
#include <QLineF>
#include <QPageRanges>
#include <QPaintDevice>
#include <QPaintEngine>
#include <QPainter>
#include <QPainterPath>
#include <QPixmap>
#include <QPrinter>
#include <QRegularExpression>
#include <QSizeF>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QTranslator>
#include <QVector>
#include <QTransform>

#include "sketch/SketchModel.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "paper_print.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::printing;
namespace sk = mitcad::sketch;

bool near(double a, double b, double tolerance) { return std::abs(a - b) <= tolerance; }

constexpr double kPointsPerMm = 72.0 / 25.4;

// A PDF's pages and their media boxes (points), as Qt writes them: each
// page's dictionary in plain text.
struct PdfPages {
  int count = 0;
  std::vector<QSizeF> boxes;
};

PdfPages pdfPages(const QString& path) {
  PdfPages out;
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    return out;
  }
  const QString text = QString::fromLatin1(file.readAll());
  static const QRegularExpression page(QStringLiteral("/Type\\s*/Page(?![A-Za-z])"));
  for (auto it = page.globalMatch(text); it.hasNext(); it.next()) {
    ++out.count;
  }
  static const QRegularExpression box(
      QStringLiteral("/MediaBox\\s*\\[\\s*0\\s+0\\s+([0-9.]+)\\s+([0-9.]+)\\s*\\]"));
  for (auto it = box.globalMatch(text); it.hasNext();) {
    const QRegularExpressionMatch m = it.next();
    out.boxes.emplace_back(m.captured(1).toDouble(), m.captured(2).toDouble());
  }
  return out;
}

// A page of `mm` size in points, within a point (Qt rounds its media boxes).
bool boxIs(const PdfPages& pages, std::size_t index, double widthMm, double heightMm) {
  return index < pages.boxes.size() && near(pages.boxes[index].width(), widthMm * kPointsPerMm, 1.0) &&
         near(pages.boxes[index].height(), heightMm * kPointsPerMm, 1.0);
}

// A paint device that records the lines painted on it in its pixels.
class RecordingEngine : public QPaintEngine {
public:
  RecordingEngine() : QPaintEngine(QPaintEngine::AllFeatures) {}
  bool begin(QPaintDevice*) override { return true; }
  bool end() override { return true; }
  void updateState(const QPaintEngineState& changed) override {
    if (changed.state() & QPaintEngine::DirtyTransform) {
      m_transform = changed.transform();
    }
  }
  using QPaintEngine::drawLines;
  void drawLines(const QLineF* lines, int count) override {
    for (int i = 0; i < count; ++i) {
      recorded.push_back(m_transform.map(lines[i]));
    }
  }
  using QPaintEngine::drawPolygon;
  void drawPolygon(const QPointF*, int, PolygonDrawMode) override {}
  void drawPath(const QPainterPath&) override {}
  void drawPixmap(const QRectF&, const QPixmap&, const QRectF&) override {}
  Type type() const override { return QPaintEngine::User; }

  std::vector<QLineF> recorded;

private:
  QTransform m_transform;
};

class RecordingDevice : public QPaintDevice {
public:
  RecordingDevice(int dpi, const QSizeF& mm) : m_dpi(dpi), m_mm(mm) {}
  QPaintEngine* paintEngine() const override { return &m_engine; }
  const std::vector<QLineF>& lines() const { return m_engine.recorded; }

protected:
  int metric(PaintDeviceMetric which) const override {
    switch (which) {
    case PdmWidth:
      return static_cast<int>(std::ceil(m_mm.width() * m_dpi / 25.4));
    case PdmHeight:
      return static_cast<int>(std::ceil(m_mm.height() * m_dpi / 25.4));
    case PdmWidthMM:
      return static_cast<int>(std::ceil(m_mm.width()));
    case PdmHeightMM:
      return static_cast<int>(std::ceil(m_mm.height()));
    case PdmDpiX:
    case PdmDpiY:
    case PdmPhysicalDpiX:
    case PdmPhysicalDpiY:
      return m_dpi;
    case PdmDepth:
      return 32;
    case PdmNumColors:
      return INT_MAX;
    case PdmDevicePixelRatio:
      return 1;
    case PdmDevicePixelRatioScaled:
      return static_cast<int>(QPaintDevice::devicePixelRatioFScale());
    default:
      return QPaintDevice::metric(which);
    }
  }

private:
  int m_dpi;
  QSizeF m_mm;
  mutable RecordingEngine m_engine;
};

// A sketch of a 300 x 200 mm rectangle from the origin and a 100 mm line
// across it, from (20, 100) to (120, 100).
sk::SketchModel templateSketch() {
  sk::SketchModel model;
  model.name = QStringLiteral("Sketch1");
  const sk::V2 corners[] = {{0, 0}, {300, 0}, {300, 200}, {0, 200}, {20, 100}, {120, 100}};
  for (int i = 0; i < 6; ++i) {
    sk::PointData p;
    p.id = QStringLiteral("p%1").arg(i + 1);
    p.at = corners[i];
    model.points.push_back(p);
  }
  const int ends[][2] = {{0, 1}, {1, 2}, {2, 3}, {3, 0}, {4, 5}};
  int n = 0;
  for (const auto& end : ends) {
    sk::CurveData c;
    c.id = QStringLiteral("c%1").arg(++n);
    c.type = QStringLiteral("line");
    c.curve = sk::Curve2::line(corners[end[0]], corners[end[1]]);
    c.start = QStringLiteral("p%1").arg(end[0] + 1);
    c.end = QStringLiteral("p%1").arg(end[1] + 1);
    c.points = QStringList{c.start, c.end};
    model.curves.push_back(c);
  }
  return model;
}

void tilePlanTests() {
  // A4 portrait with 10 mm margins: 190 x 277 mm to print on.
  const QSizeF printable = printableSize(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
  CHECK(near(printable.width(), 190.0, 1e-6) && near(printable.height(), 277.0, 1e-6));
  const PrintPlan tiles = planActual(QSizeF(304.0, 204.0), printable);
  CHECK(tiles.pageCount() == 2 && tiles.rows == 1 && tiles.columns == 2);
  CHECK(near(tiles.overlap, kTileOverlap, 1e-12) && near(tiles.scale, 1.0, 1e-12));
  if (tiles.pageCount() != 2) {
    return;
  }
  const PrintPage& first = tiles.pages[0];
  const PrintPage& second = tiles.pages[1];
  CHECK(first.source == QRectF(0.0, 0.0, 190.0, 204.0));
  CHECK(second.source == QRectF(180.0, 0.0, 124.0, 204.0));
  CHECK(first.label == QStringLiteral("Row 1, column 1 (page 1 of 2)"));
  CHECK(second.label == QStringLiteral("Row 1, column 2 (page 2 of 2)"));
  // Under the label band, at the page's left edge.
  CHECK(first.area == QRectF(0.0, kLabelBand, 190.0, 204.0));
  CHECK(second.toPage.map(QPointF(180.0, 0.0)) == QPointF(0.0, kLabelBand));
  // The crosses of the overlap come out at the same places of the picture
  // on both pages, and both pages show both edges of it.
  CHECK(first.marks.size() == 4 && second.marks.size() == 4);
  for (std::size_t i = 0; i < first.marks.size() && i < second.marks.size(); ++i) {
    const QPointF a = first.toPage.inverted().map(first.marks[i]);
    const QPointF b = second.toPage.inverted().map(second.marks[i]);
    CHECK(near(a.x(), b.x(), 1e-9) && near(a.y(), b.y(), 1e-9));
    CHECK(a.x() >= 180.0 - 1e-9 && a.x() <= 190.0 + 1e-9);
  }
  CHECK(first.guides.size() == 2 && second.guides.size() == 2);
  // Landscape: two rows of two.
  const QSizeF wide = printableSize(pictureLayout(QPageSize::A4, QPageLayout::Landscape));
  const PrintPlan four = planActual(QSizeF(304.0, 204.0), wide);
  CHECK(four.pageCount() == 4 && four.rows == 2 && four.columns == 2);
  CHECK(bestOrientation(PrintScale::Actual, QSizeF(304.0, 204.0), QPageSize::A4) == QPageLayout::Portrait);
  // A picture that fits: one page, in its middle.
  const PrintPlan one = planActual(QSizeF(100.0, 50.0), printable);
  CHECK(one.pageCount() == 1 && one.pages[0].label.isEmpty());
  CHECK(one.pages[0].area == QRectF(45.0, 113.5, 100.0, 50.0));
  // Fit to page: one page, landscape the larger.
  const PrintPlan fit = planFit(QSizeF(304.0, 204.0), wide);
  CHECK(fit.pageCount() == 1 && near(fit.scale, 277.0 / 304.0, 1e-12));
  CHECK(bestOrientation(PrintScale::Fit, QSizeF(304.0, 204.0), QPageSize::A4) == QPageLayout::Landscape);
}

void sketchTileTests() {
  const PrintPicture picture = sketchPicture(templateSketch(), SketchPrintOptions());
  // The sketch and its 2 mm margin.
  CHECK(near(picture.size.width(), 300.0 + 2.0 * kSketchMargin, 1e-9));
  CHECK(near(picture.size.height(), 200.0 + 2.0 * kSketchMargin, 1e-9));
  CHECK(picture.name == QStringLiteral("Sketch1"));
  const QSizeF printable = printableSize(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
  const PrintPlan tiles = planActual(picture.size, printable);
  CHECK(tiles.pageCount() == 2);
  if (tiles.pageCount() != 2) {
    return;
  }
  // The first tile painted at 1200 dpi: the 100 mm line is 100 mm long on
  // the page, at (22, 108) to (122, 108) of the printable area (the margin,
  // the label band, the picture's top 202 mm above the line).
  const int dpi = 1200;
  const double pixelsPerMm = dpi / 25.4;
  RecordingDevice device(dpi, printable);
  QPainter painter(&device);
  paintPage(painter, pixelsPerMm, tiles, tiles.pages[0], picture);
  painter.end();
  bool found = false;
  for (const QLineF& line : device.lines()) {
    const QLineF mm(line.p1() / pixelsPerMm, line.p2() / pixelsPerMm);
    if (!near(mm.p1().y(), kLabelBand + 102.0, 1e-3) || !near(mm.p2().y(), kLabelBand + 102.0, 1e-3)) {
      continue;
    }
    found = true;
    CHECK(near(mm.length(), 100.0, 1e-6));
    CHECK(near(mm.p1().x(), 22.0, 1e-6) && near(mm.p2().x(), 122.0, 1e-6));
    CHECK(near(mm.p1().y(), kLabelBand + 102.0, 1e-6) && near(mm.p2().y(), kLabelBand + 102.0, 1e-6));
  }
  CHECK(found);
  // The rectangle's lower edge runs over both tiles: on the second it starts
  // 180 mm further left (the first tile's width less the overlap).
  RecordingDevice second(dpi, printable);
  QPainter secondPainter(&second);
  paintPage(secondPainter, pixelsPerMm, tiles, tiles.pages[1], picture);
  secondPainter.end();
  bool edge = false;
  for (const QLineF& line : second.lines()) {
    const QLineF mm(line.p1() / pixelsPerMm, line.p2() / pixelsPerMm);
    if (near(mm.p1().y(), kLabelBand + 202.0, 1e-6) && near(mm.p2().y(), kLabelBand + 202.0, 1e-6) &&
        near(mm.length(), 300.0, 1e-6)) {
      edge = true;
      CHECK(near(mm.p1().x(), 2.0 - 180.0, 1e-6));
    }
  }
  CHECK(edge);
}

void sketchPdfTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const PrintPicture picture = sketchPicture(templateSketch(), SketchPrintOptions());
  const QString tiled = folder.filePath(QStringLiteral("tiled.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(tiled);
    printer.setPageLayout(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
    QString error;
    CHECK(printPicture(printer, picture, PrintScale::Actual, error) == 2);
    CHECK(error.isEmpty());
  }
  const PdfPages tiles = pdfPages(tiled);
  CHECK(tiles.count == 2);
  CHECK(boxIs(tiles, 0, 210.0, 297.0) && boxIs(tiles, 1, 210.0, 297.0));
  const QString fitted = folder.filePath(QStringLiteral("fit.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(fitted);
    printer.setPageLayout(pictureLayout(QPageSize::A4, QPageLayout::Landscape));
    QString error;
    CHECK(printPicture(printer, picture, PrintScale::Fit, error) == 1);
  }
  const PdfPages fit = pdfPages(fitted);
  CHECK(fit.count == 1);
  CHECK(boxIs(fit, 0, 297.0, 210.0));
  // Dimensions and constraints are off unless asked for; with constraints
  // the picture grows by the symbols only where they stand.
  sk::SketchModel constrained = templateSketch();
  sk::ConstraintData horizontal;
  horizontal.id = QStringLiteral("k1");
  horizontal.type = QStringLiteral("horizontal");
  horizontal.refs = QStringList{QStringLiteral("c5")};
  constrained.constraints.push_back(horizontal);
  SketchPrintOptions withConstraints;
  withConstraints.constraints = true;
  CHECK(sketchPicture(constrained, SketchPrintOptions()).size == picture.size);
  CHECK(sketchPicture(constrained, withConstraints).size == picture.size);
  CHECK(!printsConstraint(QStringLiteral("coincident")) && printsConstraint(QStringLiteral("horizontal")));
}

// A fresh sketch prints nothing, not a blank page of its margins
// (mitcad#1156): no picture, "There is nothing to print.", no file.
void emptySketchTests() {
  const PrintPicture empty = sketchPicture(sk::SketchModel{}, SketchPrintOptions());
  CHECK(!empty.valid());
  CHECK(!sketchPrintsAnything(sk::SketchModel{}));
  // The origin and an axis's line, as a new sketch has them: still nothing.
  sk::SketchModel axes;
  sk::PointData origin;
  origin.id = QStringLiteral("p1");
  origin.fixed = true;
  axes.points.push_back(origin);
  sk::PointData axisEnd;
  axisEnd.id = QStringLiteral("p2");
  axisEnd.at = {50.0, 0.0};
  axisEnd.axis = QStringLiteral("x");
  axes.points.push_back(axisEnd);
  sk::CurveData axis;
  axis.id = QStringLiteral("c1");
  axis.type = QStringLiteral("line");
  axis.axis = QStringLiteral("x");
  axis.curve = sk::Curve2::line(origin.at, axisEnd.at);
  axis.start = origin.id;
  axis.end = axisEnd.id;
  axis.points = QStringList{axis.start, axis.end};
  axes.curves.push_back(axis);
  CHECK(!sketchPrintsAnything(axes));
  CHECK(!sketchPicture(axes, SketchPrintOptions(), {}).valid());
  // A lone point prints its cross; a rectangle its lines.
  sk::SketchModel lone;
  sk::PointData point;
  point.id = QStringLiteral("p1");
  point.at = {5.0, 5.0};
  lone.points.push_back(point);
  CHECK(sketchPrintsAnything(lone) && sketchPicture(lone, SketchPrintOptions()).valid());
  CHECK(sketchPrintsAnything(templateSketch()));
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const QString path = folder.filePath(QStringLiteral("empty.pdf"));
  for (const PrintScale scale : {PrintScale::Actual, PrintScale::Fit}) {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(path);
    printer.setPageLayout(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
    QString error;
    CHECK(printPicture(printer, empty, scale, error) == -1);
    CHECK(error == QStringLiteral("There is nothing to print."));
  }
  CHECK(!QFile::exists(path));
}

// The print dialog's page range counts the Print panel's pages; when the
// dialog changes the paper, it picks the pages that show those pages' parts
// of the picture (mitcad#1156).
void pageRangeMapTests() {
  const QSizeF picture(304.0, 204.0);
  // A4 portrait: x 0-190 and 180-304, one row.
  const PrintPlan a4 = planActual(picture, printableSize(pictureLayout(QPageSize::A4, QPageLayout::Portrait)));
  CHECK(a4.pageCount() == 2);
  // The same paper: the same pages.
  CHECK(mapPages(a4, {1}, a4) == std::vector<int>{1});
  CHECK(mapPages(a4, {2}, a4) == std::vector<int>{2});
  CHECK(mapPages(a4, {1, 2}, a4) == (std::vector<int>{1, 2}));
  // A3 landscape: all on one page.
  const PrintPlan a3 = planActual(picture, printableSize(pictureLayout(QPageSize::A3, QPageLayout::Landscape)));
  CHECK(a3.pageCount() == 1);
  CHECK(mapPages(a4, {2}, a3) == std::vector<int>{1});
  // A4 landscape: two rows of x 0-277 and 267-304. Page 1's part (x 0-190)
  // is in the first column of both rows; page 2's (x 180-304) needs both
  // columns.
  const PrintPlan wide = planActual(picture, printableSize(pictureLayout(QPageSize::A4, QPageLayout::Landscape)));
  CHECK(wide.pageCount() == 4 && wide.rows == 2 && wide.columns == 2);
  CHECK(mapPages(a4, {1}, wide) == (std::vector<int>{1, 3}));
  CHECK(mapPages(a4, {2}, wide) == (std::vector<int>{1, 2, 3, 4}));
  // And back: the first column of landscape (x 0-277) needs both portrait
  // pages, the second (x 267-304) only the second.
  CHECK(mapPages(wide, {2}, a4) == std::vector<int>{2});
  CHECK(mapPages(wide, {1, 3}, a4) == (std::vector<int>{1, 2}));
  // Fit to page: one page whatever the paper.
  const PrintPlan fit = planFit(picture, printableSize(pictureLayout(QPageSize::A3, QPageLayout::Landscape)));
  CHECK(mapPages(planFit(picture, a4.printable), {1}, fit) == std::vector<int>{1});
  // Pages the plan does not have are left out.
  CHECK(mapPages(a4, {3}, a3).empty());

  // Printed: page 2 of the A4 tiles, the dialog's paper A3 landscape. The
  // range of the panel's plan prints the one A3 page; without that plan the
  // range is refused, as page 2 of one page.
  const PrintPicture sketch = sketchPicture(templateSketch(), SketchPrintOptions());
  CHECK(sketch.size == picture);
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QPageRanges second;
  second.addPage(2);
  const QString mapped = folder.filePath(QStringLiteral("mapped.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(mapped);
    printer.setPageLayout(pictureLayout(QPageSize::A3, QPageLayout::Landscape));
    printer.setPrintRange(QPrinter::PageRange);
    printer.setPageRanges(second);
    QString error;
    CHECK(printPicture(printer, sketch, PrintScale::Actual, error, &a4) == 1);
    CHECK(error.isEmpty());
  }
  const PdfPages one = pdfPages(mapped);
  CHECK(one.count == 1);
  CHECK(boxIs(one, 0, 420.0, 297.0));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(folder.filePath(QStringLiteral("refused.pdf")));
    printer.setPageLayout(pictureLayout(QPageSize::A3, QPageLayout::Landscape));
    printer.setPrintRange(QPrinter::PageRange);
    printer.setPageRanges(second);
    QString error;
    CHECK(printPicture(printer, sketch, PrintScale::Actual, error) == -1);
    CHECK(error.contains(QStringLiteral("none of the 1 page")));
  }
  // The paper kept: the range's own page.
  const QString kept = folder.filePath(QStringLiteral("kept.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(kept);
    printer.setPageLayout(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
    printer.setPrintRange(QPrinter::PageRange);
    printer.setPageRanges(second);
    QString error;
    CHECK(printPicture(printer, sketch, PrintScale::Actual, error, &a4) == 1);
  }
  CHECK(pdfPages(kept).count == 1);
}

// A translator that names every paper otherwise, as another language does.
class RenamingTranslator : public QTranslator {
public:
  bool isEmpty() const override { return false; }
  QString translate(const char* context, const char* source, const char*, int) const override {
    if (context != nullptr && source != nullptr && std::strcmp(context, "QPageSize") == 0) {
      return QStringLiteral("Paper ") + QString::fromLatin1(source);
    }
    return QString();
  }
};

// The Print panel remembers its paper by the paper's key, not by the name
// the UI shows, which changes with its language (mitcad#1156).
void paperSettingTests() {
  const std::vector<QPageSize::PageSizeId> papers{QPageSize::A4, QPageSize::A3, QPageSize::Letter};
  const QString saved = paperSetting(QPageSize::Letter);
  CHECK(saved == QStringLiteral("Letter"));
  CHECK(paperFromSetting(saved, papers, QPageSize::A4) == QPageSize::Letter);
  const QString shown = QPageSize::name(QPageSize::Letter);
  RenamingTranslator renaming;
  CHECK(QCoreApplication::installTranslator(&renaming));
  CHECK(QPageSize::name(QPageSize::Letter) != shown);
  CHECK(paperSetting(QPageSize::Letter) == saved);
  CHECK(paperFromSetting(saved, papers, QPageSize::A4) == QPageSize::Letter);
  // A display name remembered before, in the language shown: still found.
  CHECK(paperFromSetting(QPageSize::name(QPageSize::A3), papers, QPageSize::A4) == QPageSize::A3);
  QCoreApplication::removeTranslator(&renaming);
  // None or another paper: the fallback.
  CHECK(paperFromSetting(QString(), papers, QPageSize::A4) == QPageSize::A4);
  CHECK(paperFromSetting(QStringLiteral("B5"), papers, QPageSize::Letter) == QPageSize::Letter);
}

// A sheet as the model writes it: millimetres in its view box.
bool writeSheet(const QString& path, double width, double height) {
  QFile file(path);
  if (!file.open(QIODevice::WriteOnly)) {
    return false;
  }
  const QString svg =
      QStringLiteral("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
                     "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"%1mm\" height=\"%2mm\" viewBox=\"0 0 %1 %2\">\n"
                     "<rect x=\"0\" y=\"0\" width=\"%1\" height=\"%2\" fill=\"white\"/>\n"
                     "<line x1=\"10\" y1=\"10\" x2=\"%3\" y2=\"10\" fill=\"none\" stroke=\"black\" stroke-width=\"0.35\"/>\n"
                     "</svg>\n")
          .arg(width)
          .arg(height)
          .arg(width - 10.0);
  return file.write(svg.toUtf8()) > 0;
}

void drawingPdfTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const QString first = folder.filePath(QStringLiteral("sheet.svg"));
  const QString second = folder.filePath(QStringLiteral("sheet-2.svg"));
  CHECK(writeSheet(first, 297.0, 210.0));
  CHECK(writeSheet(second, 420.0, 297.0));
  QString error;
  QVector<PrintPicture> sheets{svgPicture(first, &error), svgPicture(second, &error)};
  CHECK(sheets[0].valid() && sheets[1].valid());
  CHECK(sheets[0].size == QSizeF(297.0, 210.0) && sheets[1].size == QSizeF(420.0, 297.0));
  CHECK(!svgPicture(folder.filePath(QStringLiteral("missing.svg")), &error).valid() && !error.isEmpty());
  // The A3 sheet takes an A3 page in landscape.
  const QPageLayout a3 = sheetLayout(QSizeF(420.0, 297.0));
  CHECK(a3.orientation() == QPageLayout::Landscape && a3.pageSize().id() == QPageSize::A3);
  const QString both = folder.filePath(QStringLiteral("drawing.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(both);
    printer.setFullPage(true);
    printer.setPageLayout(sheetLayout(sheets[0].size));
    int fitted = -1;
    CHECK(printSheets(printer, sheets, error, &fitted) == 2);
    CHECK(fitted == 0);
  }
  const PdfPages pages = pdfPages(both);
  CHECK(pages.count == 2);
  CHECK(boxIs(pages, 0, 297.0, 210.0));
  CHECK(boxIs(pages, 1, 420.0, 297.0));
  // The page range picks the sheets: the second alone.
  const QString range = folder.filePath(QStringLiteral("second.pdf"));
  {
    QPrinter printer(QPrinter::HighResolution);
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(range);
    printer.setPrintRange(QPrinter::PageRange);
    QPageRanges ranges;
    ranges.addPage(2);
    printer.setPageRanges(ranges);
    CHECK(!pageWanted(printer, 1) && pageWanted(printer, 2));
    CHECK(printSheets(printer, sheets, error) == 1);
  }
  const PdfPages one = pdfPages(range);
  CHECK(one.count == 1);
  CHECK(boxIs(one, 0, 420.0, 297.0));
}

} // namespace

int paperPrintTests() {
  failures = 0;
  tilePlanTests();
  sketchTileTests();
  sketchPdfTests();
  emptySketchTests();
  pageRangeMapTests();
  paperSettingTests();
  drawingPdfTests();
  return failures;
}
