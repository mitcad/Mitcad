// SPDX-License-Identifier: MIT
#include "ProfilePickerRows.hpp"

#include <algorithm>

#include <QChar>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>

namespace mitcad {
namespace {

QStringList wordsOf(const QString& text) { return text.simplified().split(QLatin1Char(' '), Qt::SkipEmptyParts); }

bool sameWord(const QString& a, const QString& b) { return a.compare(b, Qt::CaseInsensitive) == 0; }

bool hasDigit(const QString& word) {
  return std::any_of(word.begin(), word.end(), [](QChar c) { return c.isDigit(); });
}

bool allDigits(const QStringList& words) {
  return std::all_of(words.begin(), words.end(), [](const QString& word) {
    return std::all_of(word.begin(), word.end(), [](QChar c) { return c.isDigit(); });
  });
}

QString textOf(const QJsonObject& row, const char* key) { return row.value(QLatin1String(key)).toString(); }

// A positive number of the row, else 0.
double numberOf(const QJsonObject& row, const char* key) {
  const QJsonValue value = row.value(QLatin1String(key));
  return value.isDouble() && value.toDouble() > 0.0 ? value.toDouble() : 0.0;
}

// The name without its nozzle (the model's `bare_name`), else the name.
QString bareName(const LibraryItem& item) {
  const QString bare = textOf(item.row, "bare_name").trimmed();
  return bare.isEmpty() ? item.name : bare;
}

} // namespace

QString printerVendor(const QString& bareName, const QStringList& bareNames) {
  const QStringList words = wordsOf(bareName);
  if (words.size() < 2) {
    return QString();
  }
  // The other printers of the same first word (one of the same words is
  // this one).
  QVector<QStringList> others;
  bool self = false;
  for (const QString& name : bareNames) {
    const QStringList other = wordsOf(name);
    if (!self && other.size() == words.size() &&
        std::equal(other.begin(), other.end(), words.begin(), sameWord)) {
      self = true;
      continue;
    }
    if (!other.isEmpty() && sameWord(other.first(), words.first())) {
      others.append(other);
    }
  }
  // The words all of them share after it, up to one with a digit (a
  // model's), always leaving a word of the name.
  int count = 1;
  while (count < words.size() - 1 && !others.isEmpty() && !hasDigit(words.at(count))) {
    const QString& next = words.at(count);
    const bool shared = std::all_of(others.begin(), others.end(), [count, &next](const QStringList& other) {
      return other.size() > count && sameWord(other.at(count), next);
    });
    if (!shared) {
      break;
    }
    ++count;
  }
  // "Printer 1" is a name and a number, no vendor.
  if (allDigits(words.mid(count))) {
    return QString();
  }
  return words.mid(0, count).join(QLatin1Char(' '));
}

QString profileSource(const LibraryItem& item) {
  if (item.place == QStringLiteral("mitcad")) {
    return QObject::tr("Built-in");
  }
  if (item.place == QStringLiteral("user")) {
    return QObject::tr("My library");
  }
  if (item.place == QStringLiteral("project")) {
    // The managers' group name (mitcad#1135).
    return QObject::tr("This project");
  }
  // The design's own: imported from a 3MF (its origin names the file), or
  // made in the design.
  const QJsonObject origin =
      item.row.value(QStringLiteral("item")).toObject().value(QStringLiteral("origin")).toObject();
  const QString file = textOf(origin, "id");
  if (textOf(origin, "library") == QStringLiteral("3mf") && !file.isEmpty()) {
    return file;
  }
  return QObject::tr("This design");
}

QVector<ProfilePickerRow> profilePickerRows(const QVector<LibraryItem>& items, const QString& kind) {
  const bool printers = kind == QStringLiteral("printer");
  QStringList bareNames;
  for (const LibraryItem& item : items) {
    bareNames << bareName(item);
  }
  QVector<ProfilePickerRow> rows;
  bool grouped = false;
  for (const LibraryItem& item : items) {
    ProfilePickerRow row;
    row.key = item.key();
    row.name = item.name;
    row.source = profileSource(item);
    const QString bare = bareName(item);
    if (printers) {
      // The vendor's heading over the model and the nozzle: the profile's
      // nozzle, else the one its name says.
      row.group = printerVendor(bare, bareNames);
      const QStringList words = wordsOf(bare);
      row.shortName = row.group.isEmpty() ? bare
                                          : words.mid(static_cast<int>(wordsOf(row.group).size())).join(QLatin1Char(' '));
      double nozzle = numberOf(item.row, "nozzle");
      if (nozzle <= 0.0) {
        nozzle = numberOf(item.row, "name_nozzle");
      }
      if (nozzle > 0.0) {
        row.shortName += QStringLiteral(" · ") + QObject::tr("%1 mm nozzle").arg(QString::number(nozzle));
      }
    } else {
      // A process's printer, named after its `@`.
      row.group = textOf(item.row, "for_printer").trimmed();
      row.shortName = row.group.isEmpty() ? item.name : bare;
    }
    grouped = grouped || !row.group.isEmpty();
    rows.append(row);
  }
  // By group, the rows without one last; then by name, the same names by
  // key.
  std::stable_sort(rows.begin(), rows.end(), [](const ProfilePickerRow& a, const ProfilePickerRow& b) {
    if (a.group.isEmpty() != b.group.isEmpty()) {
      return b.group.isEmpty();
    }
    const int byGroup = QString::localeAwareCompare(a.group.toLower(), b.group.toLower());
    if (byGroup != 0) {
      return byGroup < 0;
    }
    const int byName = QString::localeAwareCompare(a.name.toLower(), b.name.toLower());
    return byName != 0 ? byName < 0 : a.key < b.key;
  });
  if (grouped) {
    for (ProfilePickerRow& row : rows) {
      if (row.group.isEmpty()) {
        row.group = QObject::tr("Other");
      }
    }
  }
  return rows;
}

} // namespace mitcad
