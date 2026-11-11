// SPDX-License-Identifier: MIT
#include "ListTables.hpp"

#include <algorithm>
#include <utility>

#include <QChar>
#include <QList>
#include <QRegularExpression>

namespace mitcad::build {
namespace {

// Every line of the lists' text ends so (mitcad#1641).
const QString& lineEnd() {
  static const QString end = QStringLiteral("\r\n");
  return end;
}

QString csvField(const QString& cell) {
  if (!cell.contains(QLatin1Char(',')) && !cell.contains(QLatin1Char('"')) && !cell.contains(QLatin1Char('\n')) &&
      !cell.contains(QLatin1Char('\r'))) {
    return cell;
  }
  return QStringLiteral("\"%1\"").arg(QString(cell).replace(QLatin1Char('"'), QStringLiteral("\"\"")));
}

// A cell on one line: its line breaks and tabs as spaces.
QString oneLine(const QString& cell) {
  QString line = cell;
  line.replace(QStringLiteral("\r\n"), QStringLiteral(" "));
  line.replace(QLatin1Char('\r'), QLatin1Char(' '));
  line.replace(QLatin1Char('\n'), QLatin1Char(' '));
  line.replace(QLatin1Char('\t'), QLatin1Char(' '));
  return line;
}

// The room a text takes in a fixed-width font: its characters, without the
// combining marks and invisible format characters.
qsizetype widthOf(const QString& text) {
  qsizetype width = 0;
  const QList<uint> points = text.toUcs4();
  for (const uint point : points) {
    const QChar::Category category = QChar::category(static_cast<char32_t>(point));
    if (category != QChar::Mark_NonSpacing && category != QChar::Mark_Enclosing && category != QChar::Other_Format) {
      ++width;
    }
  }
  return width;
}

// A cell that is a number as the lists write them: "3", "1200.0", "-0.125".
bool isNumber(const QString& cell) {
  static const QRegularExpression number(QStringLiteral("^[+-]?[0-9]+(?:[.][0-9]+)?$"));
  return number.match(cell).hasMatch();
}

QString padded(const QString& cell, qsizetype width, bool right) {
  const qsizetype room = width - widthOf(cell);
  if (room <= 0) {
    return cell;
  }
  const QString spaces(room, QLatin1Char(' '));
  return right ? spaces + cell : cell + spaces;
}

QString withoutTrailingSpaces(QString line) {
  qsizetype end = line.size();
  while (end > 0 && line.at(end - 1) == QLatin1Char(' ')) {
    --end;
  }
  line.truncate(end);
  return line;
}

} // namespace

QString csvOf(const QVector<ListTable>& tables) {
  QStringList lines;
  for (const ListTable& table : tables) {
    if (!lines.isEmpty()) {
      lines << QString();
    }
    if (tables.size() > 1 && !table.heading.isEmpty()) {
      lines << csvField(table.heading);
    }
    QStringList header;
    for (const QString& column : table.columns) {
      header << csvField(column);
    }
    lines << header.join(QLatin1Char(','));
    for (const QStringList& row : table.rows) {
      QStringList cells;
      for (const QString& cell : row) {
        cells << csvField(cell);
      }
      lines << cells.join(QLatin1Char(','));
    }
  }
  return lines.join(lineEnd()) + lineEnd();
}

QString textTableOf(const QVector<ListTable>& tables) {
  QString text;
  bool first = true;
  for (const ListTable& table : tables) {
    if (!first) {
      text += lineEnd();
    }
    first = false;
    if (tables.size() > 1 && !table.heading.isEmpty()) {
      text += withoutTrailingSpaces(oneLine(table.heading)) + lineEnd();
    }
    // The cells on one line, as many as the table has columns.
    const qsizetype count = table.columns.size();
    QVector<QStringList> rows;
    rows.reserve(table.rows.size());
    for (const QStringList& row : table.rows) {
      QStringList cells;
      for (qsizetype c = 0; c < count; ++c) {
        cells << oneLine(row.value(c));
      }
      rows.append(cells);
    }
    QStringList names;
    for (const QString& column : table.columns) {
      names << oneLine(column);
    }
    // Each column's width, and whether all its filled cells are numbers.
    QVector<qsizetype> widths(count, 0);
    QVector<bool> numeric(count, false);
    QVector<bool> words(count, false);
    for (qsizetype c = 0; c < count; ++c) {
      widths[c] = widthOf(names[c]);
    }
    for (const QStringList& cells : std::as_const(rows)) {
      for (qsizetype c = 0; c < count; ++c) {
        const QString& cell = cells[c];
        widths[c] = std::max(widths[c], widthOf(cell));
        if (cell.isEmpty()) {
          continue;
        }
        if (isNumber(cell)) {
          numeric[c] = true;
        } else {
          words[c] = true;
        }
      }
    }
    const auto line = [&](const QStringList& cells, bool header) {
      QStringList shown;
      for (qsizetype c = 0; c < count; ++c) {
        const bool right = header ? numeric[c] && !words[c] : isNumber(cells[c]);
        shown << padded(cells[c], widths[c], right);
      }
      return withoutTrailingSpaces(shown.join(QStringLiteral("  "))) + lineEnd();
    };
    text += line(names, true);
    for (const QStringList& cells : std::as_const(rows)) {
      text += line(cells, false);
    }
  }
  return text;
}

} // namespace mitcad::build
