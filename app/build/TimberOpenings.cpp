// SPDX-License-Identifier: MIT
#include "build/TimberOpenings.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <utility>

#include <QJsonObject>
#include <QJsonValue>
#include <QObject>

namespace mitcad::build {
namespace {

constexpr double kEps = 1e-6;

bool known(double value) { return std::isfinite(value); }

QString number(double value) { return QString::number(value, 'g', 10); }

QString mm(double value) { return QString::number(value, 'f', 0); }

} // namespace

bool OpeningRow::operator==(const OpeningRow& other) const {
  return kind == other.kind && fromEnd == other.fromEnd && offset == other.offset && across == other.across &&
         width == other.width && height == other.height && sill == other.sill;
}

std::optional<double> openingNumber(QString text) {
  text = text.trimmed();
  if (text.endsWith(QStringLiteral("mm"))) {
    text.chop(2);
  }
  text = text.trimmed();
  text.replace(QLatin1Char(','), QLatin1Char('.'));
  bool ok = false;
  const double value = text.toDouble(&ok);
  return ok && std::isfinite(value) ? std::optional<double>(value) : std::nullopt;
}

std::vector<OpeningRow> openingRows(const QString& text, bool floor, QString* problem) {
  std::vector<OpeningRow> rows;
  const QString format = floor ? QObject::tr("a floor's opening is \"a b widthxlength\" (mm)")
                               : QObject::tr("a wall's opening is \"offset widthxheight [sill]\" (mm; no sill: a door)");
  const auto refuse = [&]() {
    if (problem != nullptr) {
      *problem = format;
    }
    return std::vector<OpeningRow>();
  };
  for (const QString& entry : text.split(QLatin1Char(';'), Qt::SkipEmptyParts)) {
    const QStringList words = entry.simplified().split(QLatin1Char(' '), Qt::SkipEmptyParts);
    if (words.isEmpty()) {
      continue;
    }
    const int sizeAt = floor ? 2 : 1;
    if (words.size() <= sizeAt || words.size() > sizeAt + (floor ? 1 : 2)) {
      return refuse();
    }
    const QStringList size = words[sizeAt].toLower().split(QLatin1Char('x'));
    std::vector<std::optional<double>> numbers;
    for (int i = 0; i < sizeAt; ++i) {
      numbers.push_back(openingNumber(words[i]));
    }
    numbers.push_back(size.size() == 2 ? openingNumber(size[0]) : std::nullopt);
    numbers.push_back(size.size() == 2 ? openingNumber(size[1]) : std::nullopt);
    if (!floor && words.size() == 3) {
      numbers.push_back(openingNumber(words[2]));
    }
    if (std::any_of(numbers.begin(), numbers.end(), [](const std::optional<double>& n) { return !n; })) {
      return refuse();
    }
    OpeningRow row;
    if (floor) {
      row.kind = OpeningRow::Kind::Floor;
      row.offset = *numbers[0];
      row.across = *numbers[1];
      row.width = *numbers[2];
      row.height = *numbers[3];
    } else {
      row.offset = *numbers[0];
      row.width = *numbers[1];
      row.height = *numbers[2];
      row.sill = numbers.size() > 3 ? *numbers[3] : 0.0;
      row.kind = row.sill > 0.0 ? OpeningRow::Kind::Window : OpeningRow::Kind::Door;
      if (row.kind == OpeningRow::Kind::Door) {
        row.sill = 0.0;
      }
    }
    rows.push_back(row);
  }
  if (problem != nullptr) {
    problem->clear();
  }
  return rows;
}

double offsetFromStart(const OpeningRow& row, double wallLength) {
  if (row.kind == OpeningRow::Kind::Floor || !row.fromEnd || !known(wallLength)) {
    return row.offset;
  }
  return wallLength - row.offset - row.width;
}

QString openingsText(const std::vector<OpeningRow>& rows, double wallLength) {
  QStringList entries;
  for (const OpeningRow& row : rows) {
    if (row.kind == OpeningRow::Kind::Floor) {
      entries << QStringLiteral("%1 %2 %3x%4")
                     .arg(number(row.offset), number(row.across), number(row.width), number(row.height));
      continue;
    }
    QString entry = QStringLiteral("%1 %2x%3")
                        .arg(number(offsetFromStart(row, wallLength)), number(row.width), number(row.height));
    if (row.kind == OpeningRow::Kind::Window && row.sill > 0.0) {
      entry += QLatin1Char(' ') + number(row.sill);
    }
    entries << entry;
  }
  return entries.join(QStringLiteral("; "));
}

QJsonArray openingsJson(const std::vector<OpeningRow>& rows, double wallLength) {
  QJsonArray openings;
  for (const OpeningRow& row : rows) {
    QJsonObject opening;
    if (row.kind == OpeningRow::Kind::Floor) {
      opening.insert(QStringLiteral("offset"), QJsonArray{row.offset, row.across});
      opening.insert(QStringLiteral("size"), QJsonArray{row.width, row.height});
    } else {
      opening.insert(QStringLiteral("offset"), offsetFromStart(row, wallLength));
      opening.insert(QStringLiteral("width"), row.width);
      opening.insert(QStringLiteral("height"), row.height);
      if (row.kind == OpeningRow::Kind::Window && row.sill > 0.0) {
        opening.insert(QStringLiteral("sill"), row.sill);
      }
    }
    openings.append(opening);
  }
  return openings;
}

std::vector<OpeningRow> openingRowsOf(const QJsonArray& openings) {
  std::vector<OpeningRow> rows;
  for (const QJsonValue& value : openings) {
    const QJsonObject opening = value.toObject();
    const QJsonValue offset = opening.value(QStringLiteral("offset"));
    OpeningRow row;
    if (offset.isArray()) {
      const QJsonArray at = offset.toArray();
      const QJsonArray size = opening.value(QStringLiteral("size")).toArray();
      row.kind = OpeningRow::Kind::Floor;
      row.offset = at.at(0).toDouble();
      row.across = at.at(1).toDouble();
      row.width = size.at(0).toDouble();
      row.height = size.at(1).toDouble();
    } else {
      row.offset = offset.toDouble();
      row.width = opening.value(QStringLiteral("width")).toDouble();
      row.height = opening.value(QStringLiteral("height")).toDouble();
      row.sill = std::max(0.0, opening.value(QStringLiteral("sill")).toDouble());
      row.kind = row.sill > 0.0 ? OpeningRow::Kind::Window : OpeningRow::Kind::Door;
    }
    rows.push_back(row);
  }
  return rows;
}

QString openingsSummary(const std::vector<OpeningRow>& rows) {
  if (rows.empty()) {
    return QObject::tr("No openings");
  }
  const auto count = [&rows](OpeningRow::Kind kind) {
    return static_cast<int>(std::count_if(rows.begin(), rows.end(), [kind](const OpeningRow& row) { return row.kind == kind; }));
  };
  const int windows = count(OpeningRow::Kind::Window);
  const int doors = count(OpeningRow::Kind::Door);
  const QString total =
      rows.size() == 1 ? QObject::tr("1 opening") : QObject::tr("%1 openings").arg(static_cast<int>(rows.size()));
  if (windows + doors == 0) {
    return total; // a floor's
  }
  QStringList parts;
  if (windows > 0) {
    parts << (windows == 1 ? QObject::tr("1 window") : QObject::tr("%1 windows").arg(windows));
  }
  if (doors > 0) {
    parts << (doors == 1 ? QObject::tr("1 door") : QObject::tr("%1 doors").arg(doors));
  }
  return QStringLiteral("%1: %2").arg(total, parts.join(QStringLiteral(", ")));
}

QStringList openingProblems(const std::vector<OpeningRow>& rows, const WallSize& wall) {
  QStringList problems;
  const double ts = wall.studThick;
  // The framing each side of the opening: king and trimmer studs, or the
  // king studs alone (a partition's, mitcad#519); the header's depth.
  const double side = known(ts) ? (wall.loadBearing ? 2.0 * ts : ts) : 0.0;
  const double headerDepth = wall.loadBearing ? wall.studDepth : ts;
  std::vector<std::pair<double, double>> zones;
  for (std::size_t i = 0; i < rows.size(); ++i) {
    const OpeningRow& row = rows[i];
    const double o = offsetFromStart(row, wall.length);
    QString problem;
    if (!(row.width > kEps)) {
      problem = QObject::tr("needs its width");
    } else if (!(row.height > kEps)) {
      problem = QObject::tr("needs its height");
    } else if (row.kind == OpeningRow::Kind::Window && !(row.sill > kEps)) {
      problem = QObject::tr("a window needs its sill (a door has none)");
    } else if (o < -kEps || (known(wall.length) && o + row.width > wall.length + kEps)) {
      problem = known(wall.length) ? QObject::tr("outside the wall (%1 mm long)").arg(mm(wall.length))
                                   : QObject::tr("outside the wall");
    } else if (known(ts) && known(wall.length) &&
               (o - side < ts - kEps || o + row.width + side > wall.length - ts + kEps)) {
      problem = QObject::tr("with its studs it does not fit between the wall's end studs");
    }
    const double lo = o - side;
    const double hi = o + row.width + side;
    if (problem.isEmpty()) {
      for (std::size_t j = 0; j < zones.size(); ++j) {
        if (lo < zones[j].second - kEps && zones[j].first < hi - kEps) {
          problem = QObject::tr("overlaps opening %1 with its studs").arg(static_cast<int>(j) + 1);
          break;
        }
      }
    }
    if (problem.isEmpty() && known(ts) && known(wall.height) && known(headerDepth)) {
      const double zb = wall.bottomPlates * ts;
      const double zt = wall.height - wall.topPlates * ts;
      const double sill = row.kind == OpeningRow::Kind::Window ? row.sill : 0.0;
      if (sill + row.height + headerDepth > zt + kEps) {
        problem = QObject::tr("its header reaches %1 mm, above the top plate at %2 mm")
                      .arg(mm(sill + row.height + headerDepth), mm(zt));
      } else if (row.kind == OpeningRow::Kind::Window && sill - ts < zb - kEps) {
        problem = QObject::tr("its sill is below the top of the bottom plate (%1 mm)").arg(mm(zb + ts));
      }
    }
    zones.emplace_back(lo, hi);
    problems << problem;
  }
  return problems;
}

QStringList floorOpeningProblems(const std::vector<OpeningRow>& rows, double length, double width) {
  QStringList problems;
  for (const OpeningRow& row : rows) {
    QString problem;
    if (!(row.width > kEps) || !(row.height > kEps)) {
      problem = QObject::tr("needs its width and length");
    } else if (row.offset < -kEps || row.across < -kEps) {
      problem = QObject::tr("its offsets are from the floor's corner: not negative");
    } else if ((known(length) && row.offset + row.width > length + kEps) ||
               (known(width) && row.across + row.height > width + kEps)) {
      problem = known(length) && known(width) ? QObject::tr("outside the floor (%1 × %2 mm)").arg(mm(length), mm(width))
                                              : QObject::tr("outside the floor");
    }
    problems << problem;
  }
  return problems;
}

} // namespace mitcad::build
