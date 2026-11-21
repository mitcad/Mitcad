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
// language's name for it. mitcad#1308: a sketch fitted to its page keeps
// its constraint symbols whole inside the page's area. mitcad#2095: lists
// as pages with shared columns and continued captions (the Keyboard and
// Mouse Overview's cheat sheet) on a paged device that records the texts
// and pages, the BUILD lists' pages as before, the list PDF's paper (A4,
// Letter), and the cheat sheet's paper from File › Print's.
#include "UnitTests.hpp"
#include "build/ListPages.hpp"
#include "printing/PaperPrint.hpp"
#include "printing/SketchPicture.hpp"

#include <algorithm>
#include <atomic>
#include <climits>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <initializer_list>
#include <memory>
#include <tuple>
#include <vector>

#include <QColor>
#include <QCoreApplication>
#include <QFile>
#include <QFont>
#include <QFontMetricsF>
#include <QImage>
#include <QLineF>
#include <QLocale>
#include <QPageRanges>
#include <QPaintDevice>
#include <QPaintEngine>
#include <QPainter>
#include <QPainterPath>
#include <QPdfWriter>
#include <QTextItem>
#include <QPixmap>
#include <QPointF>
#include <QPrinter>
#include <QRectF>
#include <QRegularExpression>
#include <QSize>
#include <QElapsedTimer>
#include <QSizeF>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>
#include <QTranslator>
#include <QVector>
#include <QPageLayout>
#include <QPageSize>
#include <QMarginsF>
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

// A text painted: the page (from 1), where its baseline starts in the
// device's pixels, and the text.
struct PaintedText {
  int page = 1;
  QPointF at;
  QString text;
};

// A paint engine that records the texts painted and the pages begun.
class TextRecordingEngine : public QPaintEngine {
public:
  TextRecordingEngine() : QPaintEngine(QPaintEngine::AllFeatures) {}
  bool begin(QPaintDevice*) override { return true; }
  bool end() override { return true; }
  void updateState(const QPaintEngineState& changed) override {
    if (changed.state() & QPaintEngine::DirtyTransform) {
      m_transform = changed.transform();
    }
  }
  using QPaintEngine::drawLines;
  void drawLines(const QLineF*, int) override {}
  using QPaintEngine::drawPolygon;
  void drawPolygon(const QPointF*, int, PolygonDrawMode) override {}
  void drawPath(const QPainterPath&) override {}
  void drawPixmap(const QRectF&, const QPixmap&, const QRectF&) override {}
  void drawTextItem(const QPointF& p, const QTextItem& item) override {
    texts.push_back({page, m_transform.map(p), item.text()});
  }
  Type type() const override { return QPaintEngine::User; }

  std::vector<PaintedText> texts;
  int page = 1;

private:
  QTransform m_transform;
};

// A paged device (a PDF writer that writes nothing) of `mm` size at `dpi`
// whose painting and pages are recorded.
class RecordingPagedDevice : public QPdfWriter {
public:
  RecordingPagedDevice(const QString& path, int dpi, const QSizeF& mm) : QPdfWriter(path), m_dpi(dpi), m_mm(mm) {}
  QPaintEngine* paintEngine() const override { return &m_engine; }
  bool newPage() override {
    ++m_engine.page;
    return true;
  }
  const std::vector<PaintedText>& texts() const { return m_engine.texts; }
  int pages() const { return m_engine.page; }

protected:
  int metric(PaintDeviceMetric which) const override {
    switch (which) {
    case PdmWidth:
      return static_cast<int>(std::floor(m_mm.width() * m_dpi / 25.4));
    case PdmHeight:
      return static_cast<int>(std::floor(m_mm.height() * m_dpi / 25.4));
    case PdmWidthMM:
      return static_cast<int>(m_mm.width());
    case PdmHeightMM:
      return static_cast<int>(m_mm.height());
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
  mutable TextRecordingEngine m_engine;
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

// A sketch fitted to its page keeps its symbols at their paper size and
// whole (mitcad#1308): a 400 x 200 mm rectangle with Horizontal on its top
// line and Vertical on its right one, fitted to A4 portrait (a scale under
// a half) and painted into an image. Everything drawn lies inside the
// page's area, so the clip cuts nothing; the boxes' far sides stand 1.5 +
// 2.8 mm beyond the right and top lines on paper, each side drawn whole.
void fitSymbolTests() {
  sk::SketchModel model;
  model.name = QStringLiteral("Wide");
  const sk::V2 corners[] = {{0, 0}, {400, 0}, {400, 200}, {0, 200}};
  for (int i = 0; i < 4; ++i) {
    sk::PointData p;
    p.id = QStringLiteral("p%1").arg(i + 1);
    p.at = corners[i];
    model.points.push_back(p);
  }
  for (int i = 0; i < 4; ++i) {
    sk::CurveData c;
    c.id = QStringLiteral("c%1").arg(i + 1);
    c.type = QStringLiteral("line");
    c.curve = sk::Curve2::line(corners[i], corners[(i + 1) % 4]);
    c.start = QStringLiteral("p%1").arg(i + 1);
    c.end = QStringLiteral("p%1").arg((i + 1) % 4 + 1);
    c.points = QStringList{c.start, c.end};
    model.curves.push_back(c);
  }
  sk::ConstraintData horizontal;
  horizontal.id = QStringLiteral("k1");
  horizontal.type = QStringLiteral("horizontal");
  horizontal.refs = QStringList{QStringLiteral("c3")}; // the top line
  model.constraints.push_back(horizontal);
  sk::ConstraintData vertical;
  vertical.id = QStringLiteral("k2");
  vertical.type = QStringLiteral("vertical");
  vertical.refs = QStringList{QStringLiteral("c2")}; // the right line
  model.constraints.push_back(vertical);
  SketchPrintOptions options;
  options.constraints = true;
  const PrintPicture picture = sketchPicture(model, options);
  CHECK(picture.valid());
  // At 1:1 the picture is its 1:1 extent.
  CHECK(picture.extentAt(1.0) == QRectF(QPointF(0.0, 0.0), picture.size));
  const QSizeF printable = printableSize(pictureLayout(QPageSize::A4, QPageLayout::Portrait));
  const PrintPlan fit = plan(PrintScale::Fit, picture, printable);
  CHECK(fit.pageCount() == 1 && fit.scale > 0.4 && fit.scale < 0.5);
  if (fit.pageCount() != 1) {
    return;
  }
  const PrintPage& page = fit.pages[0];
  CHECK(page.area.left() >= -1e-9 && page.area.top() >= -1e-9);
  CHECK(page.area.right() <= printable.width() + 1e-9 && page.area.bottom() <= printable.height() + 1e-9);
  // Wider than tall: the page's width is filled.
  CHECK(near(page.area.width(), printable.width(), 1e-6));

  const double pixelsPerMm = 8.0;
  const QSize pixels(static_cast<int>(std::ceil(printable.width() * pixelsPerMm)),
                     static_cast<int>(std::ceil(printable.height() * pixelsPerMm)));
  QImage printed(pixels, QImage::Format_RGB32);
  printed.fill(Qt::white);
  {
    QPainter painter(&printed);
    paintPage(painter, pixelsPerMm, fit, page, picture);
  }
  // The same without the page's clip.
  QImage whole(pixels, QImage::Format_RGB32);
  whole.fill(Qt::white);
  {
    QPainter painter(&whole);
    painter.scale(pixelsPerMm, pixelsPerMm);
    painter.setTransform(page.toPage, true);
    picture.paint(painter, fit.scale);
  }
  CHECK(printed == whole);

  // The dark pixels per column and row, and where the drawing reaches.
  std::vector<int> columns(static_cast<std::size_t>(pixels.width()), 0);
  std::vector<int> rows(static_cast<std::size_t>(pixels.height()), 0);
  int left = pixels.width();
  int right = -1;
  int top = pixels.height();
  int bottom = -1;
  for (int y = 0; y < whole.height(); ++y) {
    for (int x = 0; x < whole.width(); ++x) {
      if (qGray(whole.pixel(x, y)) < 128) {
        ++columns[static_cast<std::size_t>(x)];
        ++rows[static_cast<std::size_t>(y)];
        left = std::min(left, x);
        right = std::max(right, x);
        top = std::min(top, y);
        bottom = std::max(bottom, y);
      }
    }
  }
  CHECK(right >= 0);
  if (right < 0) {
    return;
  }
  // Inside the page's area, a pixel's rounding aside.
  CHECK(left >= std::floor(page.area.left() * pixelsPerMm) && right <= std::ceil(page.area.right() * pixelsPerMm));
  CHECK(top >= std::floor(page.area.top() * pixelsPerMm) && bottom <= std::ceil(page.area.bottom() * pixelsPerMm));
  // The rectangle's right line: the last column dark over half its height;
  // its top line the first row dark over half its width.
  const double tall = 200.0 * fit.scale * pixelsPerMm;
  const double wide = 400.0 * fit.scale * pixelsPerMm;
  int rightLine = -1;
  for (int x = 0; x < pixels.width(); ++x) {
    if (columns[static_cast<std::size_t>(x)] > tall / 2.0) {
      rightLine = x;
    }
  }
  int topLine = -1;
  for (int y = pixels.height() - 1; y >= 0; --y) {
    if (rows[static_cast<std::size_t>(y)] > wide / 2.0) {
      topLine = y;
    }
  }
  CHECK(rightLine >= 0 && topLine >= 0);
  // From the line's outer edge (0.35 mm wide) to the box's (0.18 mm): 4.3
  // mm less half the one plus half the other, as at 1:1.
  const double expected = 1.5 + 2.8 - 0.35 / 2.0 + 0.18 / 2.0;
  CHECK(near((right - rightLine) / pixelsPerMm, expected, 0.3));
  CHECK(near((topLine - top) / pixelsPerMm, expected, 0.3));
  // The boxes' far sides drawn whole: 2.8 mm of dark pixels in the right
  // box's last column and the top box's first row.
  CHECK(columns[static_cast<std::size_t>(right)] >= 2.5 * pixelsPerMm);
  CHECK(rows[static_cast<std::size_t>(top)] >= 2.5 * pixelsPerMm);
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

// Three groups of 50 rows each, keys of different widths per group, as the
// cheat sheet's tables (mitcad#2095).
QVector<mitcad::build::ListTable> cheatTables() {
  QVector<mitcad::build::ListTable> tables;
  const QStringList columns{QStringLiteral("Keys"), QStringLiteral("Does")};
  for (int t = 0; t < 3; ++t) {
    mitcad::build::ListTable table{QStringLiteral("Group%1").arg(t), columns, {}};
    for (int i = 0; i < 50; ++i) {
      const QString keys = t == 1 ? QStringLiteral("Ctrl+Alt+Shift+K%1,N").arg(i) : QStringLiteral("K%1_%2").arg(t).arg(i);
      table.rows.append({keys, QStringLiteral("Does_%1_%2").arg(t).arg(i)});
    }
    tables.append(table);
  }
  return tables;
}

int countOf(const std::vector<PaintedText>& texts, const QString& text) {
  return static_cast<int>(std::count_if(texts.cbegin(), texts.cend(), [&text](const PaintedText& t) { return t.text == text; }));
}

// Shared columns and continued captions: every row once, the columns at
// the same place on every page, the header at the top of each page, a group
// cut by a page break continued under its caption; the default options as
// the BUILD lists have them.
void listPagesTests() {
  using mitcad::build::ListPageOptions;
  QTemporaryDir folder;
  CHECK(folder.isValid());
  const QVector<mitcad::build::ListTable> tables = cheatTables();
  {
    RecordingPagedDevice device(folder.filePath(QStringLiteral("unused.pdf")), 100, QSizeF(186.0, 273.0));
    const int pages = mitcad::build::paintListPages(device, QStringLiteral("Title"), QStringLiteral("Subtitle"), tables,
                                                    ListPageOptions{true, true});
    CHECK(pages >= 2);
    CHECK(pages == device.pages());
    const std::vector<PaintedText>& texts = device.texts();
    double doesX = -1.0;
    bool sameX = true;
    std::vector<int> pagesOfGroup(3, 0);
    std::vector<int> firstPageOf(3, 0);
    for (int t = 0; t < 3; ++t) {
      for (int i = 0; i < 50; ++i) {
        const QString does = QStringLiteral("Does_%1_%2").arg(t).arg(i);
        CHECK(countOf(texts, does) == 1);
        CHECK(countOf(texts, tables[t].rows[i][0]) == 1);
        for (const PaintedText& text : texts) {
          if (text.text == does) {
            if (doesX < 0.0) {
              doesX = text.at.x();
            }
            sameX = sameX && near(text.at.x(), doesX, 0.5);
            if (firstPageOf[t] == 0) {
              firstPageOf[t] = text.page;
            }
            pagesOfGroup[t] = std::max(pagesOfGroup[t], text.page);
          }
          if (text.text == tables[t].rows[i][0]) {
            sameX = sameX && near(text.at.x(), 0.0, 0.5);
          }
        }
      }
    }
    CHECK(sameX);
    // The header at the top of each page (on the first under the title).
    CHECK(countOf(texts, QStringLiteral("Keys")) == pages);
    for (int page = 2; page <= pages; ++page) {
      const PaintedText* top = nullptr;
      for (const PaintedText& text : texts) {
        if (text.page == page && (top == nullptr || text.at.y() < top->at.y())) {
          top = &text;
        }
      }
      CHECK(top != nullptr && (top->text == QStringLiteral("Keys") || top->text == QStringLiteral("Does")));
    }
    // A group cut by a page break: its caption with "(continued)" right
    // under the next page's header, above its rows there.
    int cut = -1;
    for (int t = 0; t < 3; ++t) {
      if (pagesOfGroup[t] > firstPageOf[t]) {
        cut = t;
      }
    }
    CHECK(cut >= 0);
    if (cut >= 0) {
      const int next = firstPageOf[cut] + 1;
      const QString caption = QStringLiteral("Group%1 (continued)").arg(cut);
      CHECK(countOf(texts, caption) == 1);
      double captionY = -1.0;
      double headerY = -1.0;
      double rowY = 1e9;
      for (const PaintedText& text : texts) {
        if (text.page != next) {
          continue;
        }
        if (text.text == caption) {
          captionY = text.at.y();
        } else if (text.text == QStringLiteral("Keys")) {
          headerY = text.at.y();
        } else if (text.text.startsWith(QStringLiteral("Does_%1_").arg(cut))) {
          rowY = std::min(rowY, text.at.y());
        }
      }
      CHECK(captionY > headerY && headerY >= 0.0);
      CHECK(captionY < rowY);
    }
    // At most one continued caption a page, none on the first.
    int continued = 0;
    for (const PaintedText& text : texts) {
      if (text.text.contains(QStringLiteral("(continued)"))) {
        ++continued;
        CHECK(text.page > 1);
      }
    }
    CHECK(continued >= 1 && continued <= pages - 1);
  }
  {
    // The default options (the BUILD lists'): a header under each caption,
    // each table's columns its own; a table cut by a page break goes on
    // under its caption with "(continued)" and its own header (mitcad#2116).
    RecordingPagedDevice device(folder.filePath(QStringLiteral("unused-2.pdf")), 100, QSizeF(186.0, 273.0));
    const int pages = mitcad::build::paintListPages(device, QStringLiteral("Cut List"), QStringLiteral("frame, 2026-11-19"),
                                                    tables);
    CHECK(pages >= 2);
    const std::vector<PaintedText>& texts = device.texts();
    for (int t = 0; t < 3; ++t) {
      CHECK(countOf(texts, QStringLiteral("Group%1").arg(t)) == 1);
    }
    // Each page after the first: its top line the continued table's
    // caption, then the header, then that table's rows (or a table that
    // starts there).
    int continued = 0;
    for (int page = 2; page <= pages; ++page) {
      const PaintedText* caption = nullptr;
      double headerY = -1.0;
      double rowY = 1e9;
      double topY = 1e9;
      for (const PaintedText& text : texts) {
        if (text.page != page) {
          continue;
        }
        topY = std::min(topY, text.at.y());
        if (text.text.endsWith(QStringLiteral(" (continued)"))) {
          CHECK(caption == nullptr);
          caption = &text;
        } else if (text.text == QStringLiteral("Keys") && headerY < 0.0) {
          headerY = text.at.y();
        } else if (text.text.startsWith(QStringLiteral("Does_")) && rowY > 1e8) {
          rowY = text.at.y();
        }
      }
      if (caption == nullptr) {
        // A page that starts a table: its caption at the top.
        bool starts = false;
        for (const PaintedText& text : texts) {
          starts = starts || (text.page == page && near(text.at.y(), topY, 0.5) && text.text.startsWith(QStringLiteral("Group")));
        }
        CHECK(starts);
        continue;
      }
      ++continued;
      const QString group = caption->text.left(caption->text.indexOf(QLatin1Char(' ')));
      CHECK(near(caption->at.y(), topY, 0.5));
      CHECK(headerY > caption->at.y() && headerY < rowY);
      // The first row under the header is the continued table's.
      for (const PaintedText& text : texts) {
        if (text.page == page && near(text.at.y(), rowY, 0.5) && text.text.startsWith(QStringLiteral("Does_"))) {
          CHECK(text.text.startsWith(QStringLiteral("Does_%1_").arg(group.mid(5))));
        }
      }
    }
    CHECK(continued >= 1);
    CHECK(countOf(texts, QStringLiteral("Keys")) == 3 + continued);
    // The header right under its caption.
    const PaintedText* caption = nullptr;
    for (const PaintedText& text : texts) {
      if (text.text == QStringLiteral("Group1")) {
        caption = &text;
      }
    }
    const PaintedText* header = nullptr;
    for (const PaintedText& text : texts) {
      if (caption != nullptr && text.text == QStringLiteral("Keys") && text.page == caption->page &&
          text.at.y() > caption->at.y() && (header == nullptr || text.at.y() < header->at.y())) {
        header = &text;
      }
    }
    CHECK(header != nullptr && header->at.y() - caption->at.y() < 40.0);
    // Group1's wider keys move its Does column, unlike shared columns.
    double first = -1.0;
    double second = -1.0;
    for (const PaintedText& text : texts) {
      if (text.text == QStringLiteral("Does_0_0")) {
        first = text.at.x();
      } else if (text.text == QStringLiteral("Does_1_0")) {
        second = text.at.x();
      }
    }
    CHECK(second > first + 1.0);
  }
  // The PDF's paper: A4 and Letter in portrait.
  for (const auto& [paper, width, height] : {std::tuple<QPageSize::PageSizeId, double, double>{QPageSize::A4, 595.0, 842.0},
                                             std::tuple<QPageSize::PageSizeId, double, double>{QPageSize::Letter, 612.0, 792.0}}) {
    const QString path = folder.filePath(QStringLiteral("sheet-%1.pdf").arg(QPageSize::key(paper)));
    QString error;
    int pages = 0;
    const QPageLayout layout(QPageSize(paper), QPageLayout::Portrait, QMarginsF(12, 12, 12, 12), QPageLayout::Millimeter);
    CHECK(mitcad::build::writeListPdf(path, QStringLiteral("Keyboard and Mouse Overview"), QStringLiteral("Mouse: MitCad"),
                                      tables, error, layout, ListPageOptions{true, true}, &pages));
    const PdfPages written = pdfPages(path);
    CHECK(pages >= 2 && written.count == pages);
    CHECK(!written.boxes.empty());
    for (const QSizeF& box : written.boxes) {
      CHECK(near(box.width(), width, 1.0) && near(box.height(), height, 1.0));
    }
  }
  // The cheat sheet's paper: File › Print's when A4 or Letter, else by the
  // measurement system.
  CHECK(listPaper(paperSetting(QPageSize::Letter), QLocale::MetricSystem) == QPageSize::Letter);
  CHECK(listPaper(QString(), QLocale::MetricSystem) == QPageSize::A4);
  CHECK(listPaper(QString(), QLocale::ImperialUSSystem) == QPageSize::Letter);
  CHECK(listPaper(paperSetting(QPageSize::A3), QLocale::MetricSystem) == QPageSize::A4);
  CHECK(listPaper(paperSetting(QPageSize::A4), QLocale::ImperialUSSystem) == QPageSize::A4);
}

// The same pages at every resolution (mitcad#2096: the PDF at 300 dpi and
// the print broke one row apart), and the space under the header's rule.
void listPageBreakTests() {
  using mitcad::build::ListPageOptions;
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QVector<mitcad::build::ListTable> tables = cheatTables();
  // Rows of their own count, so that a break lands at every place of a page.
  tables[2].rows.resize(37);
  // 254 mm: whole pixels at every resolution here, as a printer's page.
  const QSizeF paper(186.0, 254.0);
  std::vector<std::vector<int>> pagesOf;
  for (const int dpi : {100, 300, 600, 1200}) {
    RecordingPagedDevice device(folder.filePath(QStringLiteral("unused-%1.pdf").arg(dpi)), dpi, paper);
    const int pages = mitcad::build::paintListPages(device, QStringLiteral("Title"), QStringLiteral("Subtitle"), tables,
                                                    ListPageOptions{true, true});
    CHECK(pages >= 2);
    std::vector<int> rowPages;
    for (int t = 0; t < tables.size(); ++t) {
      for (const QStringList& row : tables[t].rows) {
        int page = 0;
        for (const PaintedText& text : device.texts()) {
          if (text.text == row[1]) {
            page = text.page;
          }
        }
        rowPages.push_back(page);
      }
    }
    pagesOf.push_back(rowPages);
    // The first caption one header line and the conventions' 4 pt under the
    // header: the rule between them does not touch it.
    double headerY = -1.0;
    double captionY = -1.0;
    for (const PaintedText& text : device.texts()) {
      if (text.page == 1 && text.text == QStringLiteral("Keys")) {
        headerY = text.at.y();
      } else if (text.page == 1 && text.text == QStringLiteral("Group0")) {
        captionY = text.at.y();
      }
    }
    QFont bold(QStringLiteral("Helvetica"));
    bold.setPointSizeF(8.5);
    bold.setBold(true);
    QImage reference(1, 1, QImage::Format_Mono);
    reference.setDotsPerMeterX(qRound(1200 / 0.0254));
    reference.setDotsPerMeterY(qRound(1200 / 0.0254));
    const double line = QFontMetricsF(bold, &reference).height() * 72.0 / 1200.0;
    CHECK(headerY >= 0.0 && captionY > headerY);
    CHECK(near((captionY - headerY) * 72.0 / dpi, line + 4.0, 1.5 * 72.0 / dpi));
  }
  for (std::size_t d = 1; d < pagesOf.size(); ++d) {
    CHECK(pagesOf[d] == pagesOf[0]);
  }
}

// A long name of `repeats` parts.
QString longName(int i, int repeats) {
  return QStringLiteral("Component %1 ").arg(i) + QString(QStringLiteral("with a very long name ")).repeated(repeats).trimmed();
}

// Whether `text` was cut: it ends in the ellipsis Qt's elidedText puts
// there, or in three dots when the font has no ellipsis (on Windows the
// offscreen platform's font database holds only the app's ISO lettering
// font, which has none: mitcad#2158).
bool cutText(const QString& text) {
  return text.endsWith(QChar(0x2026)) || text.endsWith(QStringLiteral("..."));
}

// Whether `name` is painted whole on two lines: a text that starts it and,
// at the same x a line lower, the rest.
bool paintedOnTwoLines(const std::vector<PaintedText>& texts, const QString& name) {
  for (const PaintedText& first : texts) {
    if (first.text.isEmpty() || !name.startsWith(first.text) || first.text == name) {
      continue;
    }
    for (const PaintedText& second : texts) {
      if (second.page == first.page && near(second.at.x(), first.at.x(), 0.5) && second.at.y() > first.at.y() &&
          first.text + QLatin1Char(' ') + second.text == name) {
        return true;
      }
    }
  }
  return false;
}

// The same columns and cells at every resolution (mitcad#2116: the PDF at
// 300 dpi made its font 8.4 pt and a 1200 dpi printer 8.52 pt, so their
// columns and cut cells differed): each text at the same place in points,
// the rows on the same pages, the header repeated on each continuation
// page. The name keeps its room on paper too (docs/ui-conventions.md,
// "Columns: the name keeps its room"): a name too long for the line wraps
// whole onto a second line while the other columns can give room, and only
// a name longer than two lines is cut, at the end of its second line; the
// other columns and the headers stay whole.
void listPageColumnsTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  QVector<mitcad::build::ListTable> tables;
  const QStringList columns{QStringLiteral("Quantity"), QStringLiteral("Component"), QStringLiteral("Supplier")};
  mitcad::build::ListTable stock{QStringLiteral("Stock"), columns, {}};
  stock.nameColumn = 1;
  for (int i = 0; i < 70; ++i) {
    const QString name = i == 27 ? longName(i, 30) : i % 9 == 0 ? longName(i, 8) : QStringLiteral("Component %1").arg(i);
    stock.rows.append({QString::number(i % 4 + 1), name, QStringLiteral("Supplier %1").arg(i)});
  }
  tables.append(stock);
  tables.append({QStringLiteral("Other Components"), {QStringLiteral("Quantity"), QStringLiteral("Part")},
                 {{QStringLiteral("1"), QStringLiteral("Bracket")}, {QStringLiteral("2"), QStringLiteral("Plate")}}});
  // 254 x 177.8 mm: whole pixels at every resolution here, as a printer's
  // page.
  const QSizeF paper(254.0, 177.8);
  struct Seen {
    int page;
    double x;
    double y;
  };
  std::vector<std::vector<std::pair<QString, Seen>>> runs;
  for (const int dpi : {100, 300, 600, 1200}) {
    RecordingPagedDevice device(folder.filePath(QStringLiteral("columns-%1.pdf").arg(dpi)), dpi, paper);
    const int pages = mitcad::build::paintListPages(device, QStringLiteral("Cut List"), QStringLiteral("frame"), tables);
    CHECK(pages >= 2);
    std::vector<std::pair<QString, Seen>> run;
    for (const PaintedText& text : device.texts()) {
      run.push_back({text.text, {text.page, text.at.x() * 72.0 / dpi, text.at.y() * 72.0 / dpi}});
    }
    // The long names whole on two lines; only the name longer than two
    // lines cut, on its second; the other columns and the headers whole.
    for (int i = 0; i < 70; i += 9) {
      CHECK(i == 27 || paintedOnTwoLines(device.texts(), longName(i, 8)));
    }
    int cut = 0;
    for (const PaintedText& text : device.texts()) {
      if (cutText(text.text)) {
        ++cut;
        // On the second line, under the first.
        CHECK(!text.text.startsWith(QStringLiteral("Component")));
      }
    }
    CHECK(cut == 1);
    bool firstLine = false;
    for (const PaintedText& text : device.texts()) {
      firstLine = firstLine || (text.text.startsWith(QStringLiteral("Component 27 with")) && longName(27, 30).startsWith(text.text));
    }
    CHECK(firstLine);
    for (int i = 0; i < 70; ++i) {
      CHECK(countOf(device.texts(), QStringLiteral("Supplier %1").arg(i)) == 1);
    }
    CHECK(countOf(device.texts(), QStringLiteral("Quantity")) >= pages);
    // The Stock header on every page that has its rows.
    for (int page = 1; page <= pages; ++page) {
      bool rows = false;
      bool header = false;
      for (const PaintedText& text : device.texts()) {
        rows = rows || (text.page == page && text.text.startsWith(QStringLiteral("Supplier ")));
        header = header || (text.page == page && text.text == QStringLiteral("Supplier"));
      }
      CHECK(!rows || header);
    }
    runs.push_back(run);
  }
  for (std::size_t d = 1; d < runs.size(); ++d) {
    CHECK(runs[d].size() == runs[0].size());
    for (std::size_t i = 0; i < runs[d].size() && i < runs[0].size(); ++i) {
      CHECK(runs[d][i].first == runs[0][i].first);
      CHECK(runs[d][i].second.page == runs[0][i].second.page);
      // Within one device pixel at 100 dpi.
      CHECK(near(runs[d][i].second.x, runs[0][i].second.x, 0.75));
      CHECK(near(runs[d][i].second.y, runs[0][i].second.y, 0.75));
    }
  }
  // Other columns too wide to fit beside the name: the name keeps a third
  // of the page, the others are cut.
  {
    QStringList wide{QStringLiteral("Quantity"), QStringLiteral("Component")};
    QStringList cells{QStringLiteral("1"), longName(0, 3)};
    for (int c = 0; c < 6; ++c) {
      wide.append(QStringLiteral("Column %1").arg(c));
      cells.append(QStringLiteral("A long note of column %1 that takes much of the page").arg(c));
    }
    // A name with spaces at its end: no empty second line.
    mitcad::build::ListTable table{QStringLiteral("Wide"), wide, {cells, {QStringLiteral("2"), QStringLiteral("Short   "), QStringLiteral("x")}}};
    table.nameColumn = 1;
    RecordingPagedDevice device(folder.filePath(QStringLiteral("wide.pdf")), 300, paper);
    CHECK(mitcad::build::paintListPages(device, QStringLiteral("Cut List"), QStringLiteral("frame"), {table}) == 1);
    double componentX = -1.0;
    double nextX = -1.0;
    for (const PaintedText& text : device.texts()) {
      if (text.text == QStringLiteral("Component")) {
        componentX = text.at.x();
      } else if (text.text == QStringLiteral("Column 0")) {
        nextX = text.at.x();
      }
    }
    // A third of the paint rectangle's width: the recording device's whole
    // page, in its pixels.
    CHECK(componentX >= 0.0 && nextX - componentX >= device.width() / 3.0 - 1.0);
    CHECK(paintedOnTwoLines(device.texts(), longName(0, 3)));
    int cut = 0;
    for (const PaintedText& text : device.texts()) {
      cut += text.text.startsWith(QStringLiteral("A long note")) && cutText(text.text) ? 1 : 0;
    }
    CHECK(cut == 6);
    for (const PaintedText& text : device.texts()) {
      CHECK(!text.text.trimmed().isEmpty());
    }
  }
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

// mitcad#1591: finding the printer waited on the print system with the
// window frozen (a network printer that does not answer: a minute). The
// work runs on another thread while the application's timers still fire,
// and waitForWork returns when it is done.
void waitForWorkTests() {
  std::atomic<bool> fired{false};
  QTimer::singleShot(0, [&fired] { fired = true; });
  std::atomic<bool> otherThread{false};
  std::atomic<bool> firedMeanwhile{false};
  std::atomic<bool> done{false};
  QThread* const uiThread = QThread::currentThread();
  waitForWork(nullptr, QStringLiteral("Print"), QStringLiteral("Looking for printers…"),
              [&fired, &otherThread, &firedMeanwhile, &done, uiThread] {
                otherThread = QThread::currentThread() != uiThread;
                // The timer fires while this waits (at most 10 s).
                QElapsedTimer waited;
                waited.start();
                while (!fired && waited.elapsed() < 10000) {
                  QThread::msleep(5);
                }
                firedMeanwhile = fired.load();
                done = true;
              },
              100);
  CHECK(otherThread);
  CHECK(firedMeanwhile);
  CHECK(done);
  // The printer itself: here a PDF one (no print server in the test,
  // app/CMakeLists.txt), found the same way.
  const std::unique_ptr<QPrinter> printer = newPrinter(nullptr);
  CHECK(printer != nullptr);
}

} // namespace

int paperPrintTests() {
  failures = 0;
  tilePlanTests();
  sketchTileTests();
  sketchPdfTests();
  fitSymbolTests();
  emptySketchTests();
  pageRangeMapTests();
  paperSettingTests();
  listPagesTests();
  listPageBreakTests();
  listPageColumnsTests();
  drawingPdfTests();
  waitForWorkTests();
  return failures;
}
MITCAD_UNIT_TEST(paperPrintTests);
