// SPDX-License-Identifier: MIT
#include "PaperPrint.hpp"

#include <algorithm>
#include <cmath>
#include <memory>
#include <utility>

#include <QDebug>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFileInfo>
#include <QFont>
#include <QMarginsF>
#include <QObject>
#include <QPageRanges>
#include <QPageSize>
#include <QPainter>
#include <QPen>
#include <QPrintDialog>
#include <QPrinter>
#include <QProgressDialog>
#include <QStringList>
#include <QSvgRenderer>
#include <QThread>
#include <QTimer>
#include <QtGlobal>

namespace mitcad::printing {
namespace {

constexpr double kEpsilon = 1e-9;
// How far the alignment crosses are from the ends of an overlap (mm), at
// most; a quarter of a short overlap.
constexpr double kMarkInset = 15.0;
constexpr double kMarkArm = 3.0;
constexpr double kGuideWidth = 0.15;

// The pages of a picture larger than one page, in rows and columns.
int tilesAlong(double length, double content, double step) {
  if (length <= content + kEpsilon) {
    return 1;
  }
  return static_cast<int>(std::ceil((length - content) / step - kEpsilon)) + 1;
}

// How far apart two edges of the same picture may be and still meet (mm).
constexpr double kCoverTolerance = 1e-6;

// The fewest of `spans` that cover [low, high], each the one that reaches
// furthest from where the cover has got to (indices into `spans`); fewer
// when the spans leave a gap.
std::vector<int> coverSpan(const std::vector<std::pair<double, double>>& spans, double low, double high) {
  std::vector<int> out;
  double reached = low;
  do {
    int best = -1;
    for (int i = 0; i < static_cast<int>(spans.size()); ++i) {
      const auto& span = spans[static_cast<std::size_t>(i)];
      if (span.first <= reached + kCoverTolerance &&
          (best < 0 || span.second > spans[static_cast<std::size_t>(best)].second)) {
        best = i;
      }
    }
    if (best < 0 || (!out.empty() && spans[static_cast<std::size_t>(best)].second <= reached + kCoverTolerance)) {
      break; // a gap: nothing reaches on
    }
    out.push_back(best);
    reached = spans[static_cast<std::size_t>(best)].second;
  } while (reached < high - kCoverTolerance);
  return out;
}

QString pageList(const std::vector<int>& pages) {
  QStringList out;
  for (const int page : pages) {
    out << QString::number(page);
  }
  return out.join(QStringLiteral(", "));
}

} // namespace

std::vector<int> mapPages(const PrintPlan& from, const std::vector<int>& pages, const PrintPlan& to) {
  std::vector<int> out;
  if (to.pageCount() != to.rows * to.columns || to.pageCount() == 0) {
    return out;
  }
  // The columns' spans across (the first row's pages) and the rows' down
  // (the first column's), in picture mm.
  std::vector<std::pair<double, double>> columns;
  for (int column = 0; column < to.columns; ++column) {
    const QRectF& s = to.pages[static_cast<std::size_t>(column)].source;
    columns.emplace_back(s.left(), s.right());
  }
  std::vector<std::pair<double, double>> rows;
  for (int row = 0; row < to.rows; ++row) {
    const QRectF& s = to.pages[static_cast<std::size_t>(row * to.columns)].source;
    rows.emplace_back(s.top(), s.bottom());
  }
  for (const int page : pages) {
    if (page < 1 || page > from.pageCount()) {
      continue;
    }
    const QRectF& s = from.pages[static_cast<std::size_t>(page - 1)].source;
    const std::vector<int> across = coverSpan(columns, s.left(), s.right());
    const std::vector<int> down = coverSpan(rows, s.top(), s.bottom());
    for (const int row : down) {
      for (const int column : across) {
        out.push_back(row * to.columns + column + 1);
      }
    }
  }
  std::sort(out.begin(), out.end());
  out.erase(std::unique(out.begin(), out.end()), out.end());
  return out;
}

PrintPlan planActual(const QSizeF& picture, const QSizeF& printable, double overlap) {
  PrintPlan out;
  out.printable = printable;
  const double width = picture.width();
  const double height = picture.height();
  const double pageWidth = printable.width();
  const double pageHeight = printable.height();
  if (width <= pageWidth + kEpsilon && height <= pageHeight + kEpsilon) {
    // One page, the picture in its middle.
    PrintPage page;
    page.source = QRectF(0.0, 0.0, width, height);
    const QPointF offset((pageWidth - width) / 2.0, (pageHeight - height) / 2.0);
    page.area = QRectF(offset, picture);
    page.toPage = QTransform::fromTranslate(offset.x(), offset.y());
    out.pages.push_back(page);
    return out;
  }
  const double contentWidth = std::max(pageWidth, 1.0);
  const double contentHeight = std::max(pageHeight - kLabelBand, 1.0);
  // An overlap of at most half a page, so that the tiles still move on.
  out.overlap = std::clamp(overlap, 0.0, std::min(contentWidth, contentHeight) / 2.0);
  const double stepX = contentWidth - out.overlap;
  const double stepY = contentHeight - out.overlap;
  out.columns = tilesAlong(width, contentWidth, stepX);
  out.rows = tilesAlong(height, contentHeight, stepY);
  const int count = out.rows * out.columns;
  for (int row = 0; row < out.rows; ++row) {
    for (int column = 0; column < out.columns; ++column) {
      PrintPage page;
      page.row = row;
      page.column = column;
      const double x0 = column * stepX;
      const double y0 = row * stepY;
      page.source = QRectF(x0, y0, std::min(contentWidth, width - x0), std::min(contentHeight, height - y0));
      page.area = QRectF(0.0, kLabelBand, page.source.width(), page.source.height());
      page.toPage = QTransform::fromTranslate(-x0, kLabelBand - y0);
      page.label = QObject::tr("Row %1, column %2 (page %3 of %4)")
                       .arg(row + 1)
                       .arg(column + 1)
                       .arg(row * out.columns + column + 1)
                       .arg(count);
      const QRectF& s = page.source;
      // The overlaps with the neighbours: both edges of each, which both
      // pages show, with crosses at the same places of the picture on both.
      std::vector<double> xs;
      if (column > 0) {
        xs.push_back(x0);
        xs.push_back(x0 + out.overlap);
      }
      if (column + 1 < out.columns) {
        xs.push_back(x0 + stepX);
        xs.push_back(x0 + contentWidth);
      }
      std::vector<double> ys;
      if (row > 0) {
        ys.push_back(y0);
        ys.push_back(y0 + out.overlap);
      }
      if (row + 1 < out.rows) {
        ys.push_back(y0 + stepY);
        ys.push_back(y0 + contentHeight);
      }
      const double insetY = std::min(kMarkInset, s.height() / 4.0);
      for (const double x : xs) {
        page.guides.push_back(page.toPage.map(QLineF(x, s.top(), x, s.bottom())));
        page.marks.push_back(page.toPage.map(QPointF(x, s.top() + insetY)));
        page.marks.push_back(page.toPage.map(QPointF(x, s.bottom() - insetY)));
      }
      const double insetX = std::min(kMarkInset, s.width() / 4.0);
      for (const double y : ys) {
        page.guides.push_back(page.toPage.map(QLineF(s.left(), y, s.right(), y)));
        page.marks.push_back(page.toPage.map(QPointF(s.left() + insetX, y)));
        page.marks.push_back(page.toPage.map(QPointF(s.right() - insetX, y)));
      }
      out.pages.push_back(page);
    }
  }
  return out;
}

PrintPlan planFit(const QSizeF& picture, const QSizeF& printable) {
  PrintPlan out;
  out.printable = printable;
  const double width = std::max(picture.width(), kEpsilon);
  const double height = std::max(picture.height(), kEpsilon);
  out.scale = std::min(printable.width() / width, printable.height() / height);
  PrintPage page;
  page.source = QRectF(0.0, 0.0, picture.width(), picture.height());
  const QSizeF shown(picture.width() * out.scale, picture.height() * out.scale);
  const QPointF offset((printable.width() - shown.width()) / 2.0, (printable.height() - shown.height()) / 2.0);
  page.area = QRectF(offset, shown);
  page.toPage = QTransform(out.scale, 0.0, 0.0, out.scale, offset.x(), offset.y());
  out.pages.push_back(page);
  return out;
}

PrintPlan plan(PrintScale scale, const QSizeF& picture, const QSizeF& printable) {
  return scale == PrintScale::Fit ? planFit(picture, printable) : planActual(picture, printable);
}

PrintPlan planFit(const PrintPicture& picture, const QSizeF& printable) {
  if (!picture.extent) {
    return planFit(picture.size, printable);
  }
  // The scales whose extent fits on the page run from 0 to the largest, as
  // the extent's size on paper grows with the scale: bracketed from the fit
  // of the 1:1 size, then halved down to it.
  const auto fits = [&](double scale) {
    const QRectF extent = picture.extentAt(scale);
    return extent.width() * scale <= printable.width() + kEpsilon &&
           extent.height() * scale <= printable.height() + kEpsilon;
  };
  bool found = false;
  double low = 0.0; // fits, when found
  double high = planFit(picture.size, printable).scale;
  for (int i = 0; i < 60 && fits(high); ++i) {
    found = true;
    low = high;
    high *= 2.0;
  }
  for (int i = 0; i < 60 && !found; ++i) {
    const double half = high / 2.0;
    if (fits(half)) {
      found = true;
      low = half;
    } else {
      high = half;
    }
  }
  if (!found) {
    // Its paper-sized parts alone are larger than the page: the 1:1 fit.
    return planFit(picture.size, printable);
  }
  for (int i = 0; i < 60; ++i) {
    const double middle = (low + high) / 2.0;
    if (fits(middle)) {
      low = middle;
    } else {
      high = middle;
    }
  }
  PrintPlan out;
  out.printable = printable;
  out.scale = low;
  PrintPage page;
  page.source = picture.extentAt(low);
  const QSizeF shown(page.source.width() * low, page.source.height() * low);
  const QPointF offset((printable.width() - shown.width()) / 2.0, (printable.height() - shown.height()) / 2.0);
  page.area = QRectF(offset, shown);
  page.toPage = QTransform(low, 0.0, 0.0, low, offset.x() - low * page.source.left(),
                           offset.y() - low * page.source.top());
  out.pages.push_back(page);
  return out;
}

PrintPlan plan(PrintScale scale, const PrintPicture& picture, const QSizeF& printable) {
  return scale == PrintScale::Fit ? planFit(picture, printable) : planActual(picture.size, printable);
}

QSizeF printableSize(const QPageLayout& layout) { return layout.paintRect(QPageLayout::Millimeter).size(); }

QPageLayout pictureLayout(QPageSize::PageSizeId paper, QPageLayout::Orientation orientation) {
  return QPageLayout(QPageSize(paper), orientation,
                     QMarginsF(kPageMargin, kPageMargin, kPageMargin, kPageMargin), QPageLayout::Millimeter);
}

QPageLayout::Orientation bestOrientation(PrintScale scale, const QSizeF& picture, QPageSize::PageSizeId paper) {
  const PrintPlan portrait = plan(scale, picture, printableSize(pictureLayout(paper, QPageLayout::Portrait)));
  const PrintPlan landscape = plan(scale, picture, printableSize(pictureLayout(paper, QPageLayout::Landscape)));
  if (scale == PrintScale::Fit) {
    return landscape.scale > portrait.scale + kEpsilon ? QPageLayout::Landscape : QPageLayout::Portrait;
  }
  return landscape.pageCount() < portrait.pageCount() ? QPageLayout::Landscape : QPageLayout::Portrait;
}

QPageLayout::Orientation bestOrientation(PrintScale scale, const PrintPicture& picture, QPageSize::PageSizeId paper) {
  if (scale != PrintScale::Fit) {
    return bestOrientation(scale, picture.size, paper);
  }
  const PrintPlan portrait = planFit(picture, printableSize(pictureLayout(paper, QPageLayout::Portrait)));
  const PrintPlan landscape = planFit(picture, printableSize(pictureLayout(paper, QPageLayout::Landscape)));
  return landscape.scale > portrait.scale + kEpsilon ? QPageLayout::Landscape : QPageLayout::Portrait;
}

QString paperSetting(QPageSize::PageSizeId paper) { return QPageSize::key(paper); }

QPageSize::PageSizeId paperFromSetting(const QString& setting, const std::vector<QPageSize::PageSizeId>& papers,
                                       QPageSize::PageSizeId fallback) {
  if (setting.isEmpty()) {
    return fallback;
  }
  for (const QPageSize::PageSizeId paper : papers) {
    if (QPageSize::key(paper) == setting) {
      return paper;
    }
  }
  for (const QPageSize::PageSizeId paper : papers) {
    if (QPageSize::name(paper) == setting) {
      return paper;
    }
  }
  return fallback;
}

void paintPage(QPainter& painter, double pixelsPerMm, const PrintPlan& pagePlan, const PrintPage& page,
               const PrintPicture& picture) {
  painter.save();
  painter.scale(pixelsPerMm, pixelsPerMm);
  // The picture's part, clipped to its place on the page.
  painter.save();
  painter.setClipRect(page.area);
  painter.setTransform(page.toPage, true);
  if (picture.paint) {
    picture.paint(painter, pagePlan.scale);
  }
  painter.restore();
  // The overlaps' edges, dashed, and the alignment crosses.
  painter.setClipping(false);
  painter.setBrush(Qt::NoBrush);
  QPen guide(Qt::black, kGuideWidth);
  guide.setDashPattern({12.0, 8.0}); // in pen widths: 1.8 mm, 1.2 mm
  painter.setPen(guide);
  for (const QLineF& line : page.guides) {
    painter.drawLine(line);
  }
  painter.setPen(QPen(Qt::black, kGuideWidth));
  for (const QPointF& at : page.marks) {
    painter.drawLine(QLineF(at.x() - kMarkArm, at.y(), at.x() + kMarkArm, at.y()));
    painter.drawLine(QLineF(at.x(), at.y() - kMarkArm, at.x(), at.y() + kMarkArm));
    painter.drawEllipse(at, kMarkArm / 2.0, kMarkArm / 2.0);
  }
  painter.restore();
  if (!page.label.isEmpty()) {
    // In device pixels, so that the point size is the paper's.
    painter.save();
    QFont font = painter.font();
    font.setPointSizeF(8.0);
    painter.setFont(font);
    painter.setPen(Qt::black);
    const QRectF band(0.0, 0.0, pagePlan.printable.width() * pixelsPerMm, (kLabelBand - 1.0) * pixelsPerMm);
    const QString text = picture.name.isEmpty() ? page.label
                                                : QStringLiteral("%1 — %2").arg(picture.name, page.label);
    painter.drawText(band, Qt::AlignLeft | Qt::AlignVCenter, text);
    painter.restore();
  }
}

int printPicture(QPrinter& printer, const PrintPicture& picture, PrintScale scale, QString& error,
                 const PrintPlan* offered) {
  if (!picture.valid()) {
    error = QObject::tr("There is nothing to print.");
    return -1;
  }
  const PrintPlan pages = plan(scale, picture, printableSize(printer.pageLayout()));
  std::vector<int> wanted; // from 1
  if (offered != nullptr && printer.printRange() == QPrinter::PageRange) {
    // The range counts the offered pages: mapped onto these, which differ
    // when the dialog changed the paper or its orientation.
    std::vector<int> picked;
    for (int i = 1; i <= offered->pageCount(); ++i) {
      if (pageWanted(printer, i)) {
        picked.push_back(i);
      }
    }
    if (picked.empty()) {
      error = QObject::tr("The page range has none of the %n page(s).", nullptr, offered->pageCount());
      return -1;
    }
    wanted = mapPages(*offered, picked, pages);
    if (wanted != picked || pages.pageCount() != offered->pageCount()) {
      qInfo().noquote() << QStringLiteral("Print page range mapped: pages %1 of %2 to pages %3 of %4")
                               .arg(pageList(picked))
                               .arg(offered->pageCount())
                               .arg(pageList(wanted))
                               .arg(pages.pageCount());
    }
  } else {
    for (int i = 1; i <= pages.pageCount(); ++i) {
      if (pageWanted(printer, i)) {
        wanted.push_back(i);
      }
    }
  }
  if (wanted.empty()) {
    error = QObject::tr("The page range has none of the %n page(s).", nullptr, pages.pageCount());
    return -1;
  }
  QPainter painter;
  if (!painter.begin(&printer)) {
    error = QObject::tr("Could not print to %1.").arg(printerTarget(printer));
    return -1;
  }
  const double pixelsPerMm = printer.resolution() / 25.4;
  int printed = 0;
  for (const int number : wanted) {
    if (printed > 0 && !printer.newPage()) {
      painter.end();
      error = QObject::tr("Could not start page %1 on %2.").arg(number).arg(printerTarget(printer));
      return -1;
    }
    paintPage(painter, pixelsPerMm, pages, pages.pages[static_cast<std::size_t>(number - 1)], picture);
    ++printed;
  }
  if (!painter.end()) {
    error = QObject::tr("Could not finish printing to %1.").arg(printerTarget(printer));
    return -1;
  }
  return printed;
}

QPageLayout sheetLayout(const QSizeF& sheet) {
  const double shorter = std::min(sheet.width(), sheet.height());
  const double longer = std::max(sheet.width(), sheet.height());
  const QPageSize size(QSizeF(shorter, longer), QPageSize::Millimeter, QString(), QPageSize::FuzzyMatch);
  const QPageLayout::Orientation orientation =
      sheet.width() > sheet.height() ? QPageLayout::Landscape : QPageLayout::Portrait;
  return QPageLayout(size, orientation, QMarginsF(0.0, 0.0, 0.0, 0.0), QPageLayout::Millimeter);
}

int printSheets(QPrinter& printer, const QVector<PrintPicture>& sheets, QString& error, int* fitted) {
  std::vector<int> wanted;
  for (int i = 0; i < sheets.size(); ++i) {
    if (pageWanted(printer, i + 1) && sheets[i].valid()) {
      wanted.push_back(i);
    }
  }
  if (wanted.empty()) {
    error = sheets.isEmpty() ? QObject::tr("The drawing has no sheet.")
                             : QObject::tr("The page range has none of the %n sheet(s).", nullptr,
                                           static_cast<int>(sheets.size()));
    return -1;
  }
  if (fitted != nullptr) {
    *fitted = 0;
  }
  // Only a PDF file changes its paper while it prints; a printer keeps the
  // paper of its first page (Qt refuses the change and warns).
  const bool perSheet = printer.outputFormat() == QPrinter::PdfFormat;
  printer.setFullPage(true);
  if (perSheet) {
    printer.setPageLayout(sheetLayout(sheets[wanted.front()].size));
  }
  QPainter painter;
  if (!painter.begin(&printer)) {
    error = QObject::tr("Could not print to %1.").arg(printerTarget(printer));
    return -1;
  }
  const double pixelsPerMm = printer.resolution() / 25.4;
  int printed = 0;
  for (const int index : wanted) {
    const PrintPicture& sheet = sheets[index];
    if (printed > 0) {
      if (perSheet) {
        printer.setPageLayout(sheetLayout(sheet.size));
      }
      if (!printer.newPage()) {
        painter.end();
        error = QObject::tr("Could not start sheet %1 on %2.").arg(index + 1).arg(printerTarget(printer));
        return -1;
      }
    }
    const QRectF paper = printer.pageLayout().fullRect(QPageLayout::Millimeter);
    double scale = 1.0;
    QPointF offset(0.0, 0.0);
    if (std::abs(paper.width() - sheet.size.width()) > 0.5 || std::abs(paper.height() - sheet.size.height()) > 0.5) {
      // Another paper: the sheet fitted to it, in its middle.
      scale = std::min(paper.width() / sheet.size.width(), paper.height() / sheet.size.height());
      offset = QPointF((paper.width() - sheet.size.width() * scale) / 2.0,
                       (paper.height() - sheet.size.height() * scale) / 2.0);
      if (fitted != nullptr) {
        ++*fitted;
      }
    }
    painter.save();
    painter.scale(pixelsPerMm, pixelsPerMm);
    painter.translate(offset);
    painter.scale(scale, scale);
    painter.setClipRect(QRectF(QPointF(0.0, 0.0), sheet.size));
    sheet.paint(painter, scale);
    painter.restore();
    ++printed;
  }
  if (!painter.end()) {
    error = QObject::tr("Could not finish printing to %1.").arg(printerTarget(printer));
    return -1;
  }
  return printed;
}

PrintPicture svgPicture(const QString& path, QString* error) {
  PrintPicture picture;
  auto renderer = std::make_shared<QSvgRenderer>(path);
  if (!renderer->isValid()) {
    if (error != nullptr) {
      *error = QObject::tr("Could not read %1.").arg(QDir::toNativeSeparators(path));
    }
    return picture;
  }
  picture.name = QFileInfo(path).completeBaseName();
  // The model writes its sheets with a view box in millimetres.
  const QRectF box = renderer->viewBoxF();
  picture.size = box.size();
  const QSizeF size = picture.size;
  picture.paint = [renderer, size](QPainter& painter, double) {
    renderer->render(&painter, QRectF(QPointF(0.0, 0.0), size));
  };
  return picture;
}

bool pageWanted(const QPrinter& printer, int number) {
  if (printer.printRange() != QPrinter::PageRange) {
    return true;
  }
  const QPageRanges ranges = printer.pageRanges();
  return ranges.isEmpty() || ranges.contains(number);
}

void waitForWork(QWidget* parent, const QString& title, const QString& note, const std::function<void()>& work,
                 int noteAfterMs) {
  QEventLoop loop;
  const std::unique_ptr<QThread> thread(QThread::create(work));
  QObject::connect(thread.get(), &QThread::finished, &loop, &QEventLoop::quit, Qt::QueuedConnection);
  // A busy note without a button: the work cannot be stopped, and the
  // window is not frozen meanwhile.
  QProgressDialog shown(note, QString(), 0, 0, parent);
  shown.setWindowTitle(title);
  shown.setWindowModality(parent != nullptr ? Qt::WindowModal : Qt::ApplicationModal);
  shown.setCancelButton(nullptr);
  shown.setMinimumDuration(noteAfterMs);
  QTimer later;
  later.setSingleShot(true);
  QObject::connect(&later, &QTimer::timeout, &shown, [&shown] { shown.show(); });
  thread->start();
  later.start(noteAfterMs);
  // Input waits until the work is done: the command that started it is
  // still on the stack, and nothing may close its window or start another.
  loop.exec(QEventLoop::ExcludeUserInputEvents);
  thread->wait();
  later.stop();
}

std::unique_ptr<QPrinter> newPrinter(QWidget* parent) {
#if defined(Q_OS_UNIX) && !defined(Q_OS_MACOS)
  QElapsedTimer took;
  took.start();
  std::unique_ptr<QPrinter> printer;
  waitForWork(parent, QObject::tr("Print"), QObject::tr("Looking for printers…"),
              [&printer] { printer = std::make_unique<QPrinter>(QPrinter::HighResolution); });
  if (took.elapsed() >= 500) {
    qInfo().noquote() << QStringLiteral("Printer found in %1 ms: %2")
                             .arg(took.elapsed())
                             .arg(printer->outputFormat() == QPrinter::PdfFormat ? QStringLiteral("PDF")
                                                                                 : printer->printerName());
  }
  return printer;
#else
  Q_UNUSED(parent);
  return std::make_unique<QPrinter>(QPrinter::HighResolution);
#endif
}

bool askPrinter(QPrinter& printer, QWidget* parent, int pages) {
  const QString testFile = qEnvironmentVariable("MITCAD_TEST_PRINT_PDF");
  if (!testFile.isEmpty()) {
    printer.setOutputFormat(QPrinter::PdfFormat);
    printer.setOutputFileName(testFile);
    qInfo().noquote() << QStringLiteral("Print dialog skipped (test): printing to %1").arg(testFile);
    return true;
  }
  QPrintDialog dialog(&printer, parent);
  dialog.setWindowTitle(QObject::tr("Print"));
  QAbstractPrintDialog::PrintDialogOptions options = QAbstractPrintDialog::PrintToFile |
                                                     QAbstractPrintDialog::PrintShowPageSize |
                                                     QAbstractPrintDialog::PrintCollateCopies;
  if (pages > 0) {
    options |= QAbstractPrintDialog::PrintPageRange;
    dialog.setMinMax(1, pages);
  }
  dialog.setOptions(options);
  qInfo().noquote() << QStringLiteral("Print dialog opened: %1 page(s)").arg(pages);
  if (dialog.exec() != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Print cancelled");
    return false;
  }
  return true;
}

QString printerTarget(const QPrinter& printer) {
  if (printer.outputFormat() == QPrinter::PdfFormat) {
    return QDir::toNativeSeparators(printer.outputFileName());
  }
  return printer.printerName();
}

} // namespace mitcad::printing
