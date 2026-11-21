// SPDX-License-Identifier: MIT
#include "ListPages.hpp"

#include <algorithm>
#include <cmath>

#include <QObject>
#include <QDir>
#include <QFont>
#include <QFontMetricsF>
#include <QImage>
#include <QMarginsF>
#include <QPageSize>
#include <QPagedPaintDevice>
#include <QPainter>
#include <QPdfWriter>
#include <QPen>
#include <QLineF>
#include <QRectF>
#include <QSizeF>
#include <QStringList>

namespace mitcad::build {
namespace {

// The resolution a printed list's line heights are measured at, whatever
// the device's (paintListPages).
constexpr int kReferenceDpi = 1200;
// The space under a rule, the conventions' 4 (docs/ui-conventions.md,
// Spacing), in points on paper.
constexpr double kRuleGap = 4.0;
// The reference's pixels in a point.
constexpr double kReferencePerPoint = kReferenceDpi / 72.0;
// The rule's width in points: as the PDF's one device pixel was before
// mitcad#2116, not a 1200 dpi printer's 0.06 pt.
constexpr double kRuleWidth = 0.25;

// Fits columns as wide as `texts` (each column's widest text) and `gap`
// after each into `width` (docs/ui-conventions.md, "Columns: the name
// keeps its room", on paper): when they are too wide together, the other
// columns first close up to `smallGap` and the name column takes the room
// left, its names then wrapped onto two lines; the name column keeps at
// least its header and a third of the page (all its names, when they need
// less), and only when the others do not fit beside that are the widest of
// them narrowed to one width, their texts cut. Without a name column the
// widest columns are narrowed so.
QVector<double> fitColumns(const QVector<double>& texts, int name, double nameHeader, double width, double gap,
                           double smallGap) {
  QVector<double> widths(texts.size(), 0.0);
  double total = 0.0;
  for (int c = 0; c < texts.size(); ++c) {
    widths[c] = texts[c] + gap;
    total += widths[c];
  }
  if (total <= width || widths.isEmpty()) {
    return widths;
  }
  // The columns of `which` narrowed into `room`: the narrowest keep their
  // widths, the widest share what is left.
  const auto narrow = [&widths](const QVector<int>& which, double room) {
    QVector<double> sorted;
    for (const int c : which) {
      sorted.append(widths[c]);
    }
    std::sort(sorted.begin(), sorted.end());
    double rest = room;
    int left = static_cast<int>(sorted.size());
    for (const double w : sorted) {
      if (w > rest / left) {
        break;
      }
      rest -= w;
      --left;
    }
    const double widest = left > 0 ? std::max(0.0, rest / left) : room;
    for (const int c : which) {
      widths[c] = std::min(widths[c], widest);
    }
  };
  QVector<int> others;
  for (int c = 0; c < widths.size(); ++c) {
    if (c != name) {
      others.append(c);
    }
  }
  if (name < 0) {
    narrow(others, width);
    return widths;
  }
  double rest = width;
  for (const int c : others) {
    widths[c] = texts[c] + smallGap;
    rest -= widths[c];
  }
  const double least = std::min(texts[name] + smallGap, std::max(nameHeader + smallGap, width / 3.0));
  if (rest >= least) {
    widths[name] = std::min(rest, texts[name] + gap);
    return widths;
  }
  widths[name] = least;
  narrow(others, width - least);
  return widths;
}

// `text` in at most two lines of `width`: whole when it fits, else broken
// after the last word that fits (or the last character, for a word longer
// than the line), the second line cut with an ellipsis when it is still too
// long.
QStringList twoLines(const QFontMetricsF& metrics, const QString& name, double width) {
  // Spaces at the ends are no part of the name: they never make a second
  // line of their own.
  const QString text = name.trimmed();
  if (metrics.horizontalAdvance(text) <= width) {
    return {text};
  }
  int end = 0;
  for (int i = 0; i <= text.size(); ++i) {
    if ((i == text.size() || text.at(i).isSpace()) && i > 0 && metrics.horizontalAdvance(text.left(i)) <= width) {
      end = i;
    }
  }
  if (end == 0) {
    end = 1;
    while (end < text.size() && metrics.horizontalAdvance(text.left(end + 1)) <= width) {
      ++end;
    }
  }
  const QString second = text.mid(end).trimmed();
  return {text.left(end).trimmed(), metrics.elidedText(second, Qt::ElideRight, width)};
}

} // namespace

QPageLayout listPageLayout() {
  return QPageLayout(QPageSize(QPageSize::A4), QPageLayout::Landscape, QMarginsF(12, 12, 12, 12),
                     QPageLayout::Millimeter);
}

bool writeListPdf(const QString& path, const QString& title, const QString& subtitle,
                  const QVector<ListTable>& tables, QString& error, const QPageLayout& layout,
                  const ListPageOptions& options, int* pages) {
  QPdfWriter writer(path);
  writer.setPageLayout(layout);
  writer.setResolution(300);
  writer.setTitle(title);
  writer.setCreator(QStringLiteral("MitCad"));
  const int painted = paintListPages(writer, title, subtitle, tables, options);
  if (pages != nullptr) {
    *pages = painted;
  }
  if (painted == -1) {
    error = QObject::tr("%1 cannot be written").arg(QDir::toNativeSeparators(path));
    return false;
  }
  if (painted < 0) {
    error = QObject::tr("%1 was not written").arg(QDir::toNativeSeparators(path));
    return false;
  }
  return true;
}

int paintListPages(QPagedPaintDevice& device, const QString& title, const QString& subtitle,
                   const QVector<ListTable>& tables, const ListPageOptions& options) {
  QPainter painter;
  if (!painter.begin(&device)) {
    return -1;
  }
  // The pages are laid out and painted in the pixels of a fixed 1200 dpi
  // reference, whatever the device's resolution: the painter scales them to
  // the device. The fonts have their size in those pixels and every text is
  // measured on the reference, so that the PDF (300 dpi) and a printer (its
  // own resolution) break the pages at the same rows (mitcad#2096) and give
  // the columns the same widths and the cells the same texts (mitcad#2116):
  // a point size alone becomes whole device pixels, 8.4 pt in the PDF and
  // 8.52 pt on a 1200 dpi printer.
  const double perPoint = kReferencePerPoint;
  const double deviceDpiX = device.logicalDpiX();
  const double deviceDpiY = device.logicalDpiY();
  // The page's size in reference pixels: its layout's paint rectangle, free
  // of the device pixels' rounding, when the device paints that layout (a
  // PDF writer, a printer).
  double width = device.width() * kReferenceDpi / deviceDpiX;
  double height = device.height() * kReferenceDpi / deviceDpiY;
  const QSizeF laidOut = device.pageLayout().paintRect(QPageLayout::Point).size() * perPoint;
  if (std::abs(laidOut.width() - width) * deviceDpiX / kReferenceDpi <= 1.0) {
    width = laidOut.width();
  }
  if (std::abs(laidOut.height() - height) * deviceDpiY / kReferenceDpi <= 1.0) {
    height = laidOut.height();
  }
  painter.scale(deviceDpiX / kReferenceDpi, deviceDpiY / kReferenceDpi);
  QImage reference(1, 1, QImage::Format_Mono);
  reference.setDotsPerMeterX(qRound(kReferenceDpi / 0.0254));
  reference.setDotsPerMeterY(qRound(kReferenceDpi / 0.0254));
  int pages = 1;
  const auto fontOf = [](double points, bool isBold) {
    QFont font(QStringLiteral("Helvetica"));
    font.setPixelSize(qRound(points * kReferencePerPoint));
    font.setBold(isBold);
    return font;
  };
  const QFont heading = fontOf(14.0, true);
  const QFont body = fontOf(8.5, false);
  const QFont bold = fontOf(8.5, true);
  const QFontMetricsF bodyMetrics(body, &reference);
  const QFontMetricsF boldMetrics(bold, &reference);
  const auto metricsOf = [&](const QFont& font) {
    return QFontMetricsF(font, &reference);
  };
  double y = 0.0;
  const auto lineOf = [&](const QFont& font) {
    painter.setFont(font);
    return metricsOf(font).height();
  };
  const auto drawLine = [&](const QString& text, const QFont& font) {
    const double h = lineOf(font);
    painter.drawText(QRectF(0.0, y, width, h), Qt::AlignLeft | Qt::AlignVCenter, text);
    y += h;
  };
  // The columns' widths (the table's, or every table's with shared
  // columns) in the page's width, and the column whose name keeps its room
  // (ListTable::nameColumn; -1 for none).
  QVector<double> widths;
  int nameColumn = -1;
  const double gap = bodyMetrics.averageCharWidth() * 2.0;
  const double smallGap = bodyMetrics.averageCharWidth();
  const auto measure = [&](const QVector<const ListTable*>& over) {
    int columns = 0;
    for (const ListTable* table : over) {
      columns = std::max(columns, static_cast<int>(table->columns.size()));
    }
    nameColumn = over.isEmpty() ? -1 : over.front()->nameColumn;
    if (nameColumn >= columns) {
      nameColumn = -1;
    }
    QVector<double> texts(columns, 0.0);
    double nameHeader = 0.0;
    for (const ListTable* table : over) {
      for (int c = 0; c < table->columns.size(); ++c) {
        const double w = boldMetrics.horizontalAdvance(table->columns[c]);
        texts[c] = std::max(texts[c], w);
        if (c == nameColumn) {
          nameHeader = std::max(nameHeader, w);
        }
      }
      for (const QStringList& row : table->rows) {
        for (int c = 0; c < row.size() && c < texts.size(); ++c) {
          texts[c] = std::max(texts[c], bodyMetrics.horizontalAdvance(row[c]));
        }
      }
    }
    widths = fitColumns(texts, nameColumn, nameHeader, width, gap, smallGap);
  };
  // A row's lines: one, or a name too long for its column wrapped onto a
  // second (cut only when two lines cannot hold it).
  const auto nameLinesOf = [&](const QStringList& cells) {
    if (nameColumn < 0 || nameColumn >= cells.size()) {
      return QStringList();
    }
    return twoLines(bodyMetrics, cells[nameColumn], widths[nameColumn] - smallGap);
  };
  // A row's cells at y, without a page break; a cell too long for its
  // column ends in an ellipsis a point short of the next. A wrapped name's
  // second line is under its first, the other cells beside the first.
  const auto drawCells = [&](const QStringList& cells, const QFont& font, const QStringList& nameLines) {
    const double h = lineOf(font);
    const QFontMetricsF metrics = metricsOf(font);
    double x = 0.0;
    for (int c = 0; c < cells.size() && c < widths.size(); ++c) {
      const double w = widths[c];
      if (c == nameColumn && !nameLines.isEmpty()) {
        for (int l = 0; l < nameLines.size(); ++l) {
          painter.drawText(QRectF(x, y + h * l, w, h), Qt::AlignLeft | Qt::AlignVCenter, nameLines[l]);
        }
      } else {
        const QString shown = metrics.elidedText(cells[c], Qt::ElideRight, w - perPoint);
        painter.drawText(QRectF(x, y, w, h), Qt::AlignLeft | Qt::AlignVCenter, shown);
      }
      x += w;
    }
    y += h * std::max<qsizetype>(1, nameLines.size());
  };
  // A rule at y and the space under it, which keeps it off the caption or
  // the row that follows.
  const QPen rulePen(Qt::black, kRuleWidth * perPoint);
  const auto drawRule = [&] {
    painter.save();
    painter.setPen(rulePen);
    painter.drawLine(QLineF(0.0, y, width, y));
    painter.restore();
    y += kRuleGap * perPoint;
  };
  // The column header and the rule under it: the shared one, or the table's
  // whose rows are being painted.
  QStringList header;
  const auto drawHeader = [&] {
    drawCells(header, bold, {});
    drawRule();
  };
  // The caption of the table whose rows are being painted, for a page that
  // continues it.
  QString continuing;
  const auto room = [&](double needed) {
    if (y + needed > height) {
      device.newPage();
      ++pages;
      y = 0.0;
      if (options.sharedColumns) {
        drawHeader();
      }
      if (options.continuedCaptions && !continuing.isEmpty()) {
        drawLine(QObject::tr("%1 (continued)").arg(continuing), bold);
      }
      // A table with columns of its own goes on under its header
      // (mitcad#2116).
      if (!options.sharedColumns && !continuing.isEmpty()) {
        drawHeader();
      }
    }
  };
  const auto drawRow = [&](const QStringList& cells) {
    const QStringList nameLines = nameLinesOf(cells);
    room(lineOf(body) * std::max<qsizetype>(1, nameLines.size()));
    drawCells(cells, body, nameLines);
  };
  // The title and the line under it.
  drawLine(title, heading);
  drawLine(subtitle, body);
  y += lineOf(body);
  QVector<const ListTable*> shown;
  for (const ListTable& table : tables) {
    if (!table.rows.isEmpty()) {
      shown.append(&table);
    }
  }
  if (options.sharedColumns && !shown.isEmpty()) {
    measure(shown);
    header = shown.front()->columns;
    drawHeader();
  }
  const double rule = kRuleGap * perPoint;
  for (const ListTable* table : shown) {
    if (!options.sharedColumns) {
      measure({table});
      header = table->columns;
    }
    const double line = lineOf(bold);
    // The caption with its first row (and its header) on one page.
    const double first = lineOf(body) * std::max<qsizetype>(1, nameLinesOf(table->rows.front()).size());
    room(options.sharedColumns ? line + first : line * 2 + rule + first);
    drawLine(table->heading, bold);
    if (!options.sharedColumns) {
      drawHeader();
    }
    continuing = table->heading;
    for (const QStringList& row : table->rows) {
      drawRow(row);
    }
    continuing.clear();
    y += line;
  }
  if (!painter.end()) {
    return -2;
  }
  return pages;
}

} // namespace mitcad::build
