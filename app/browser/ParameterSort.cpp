// SPDX-License-Identifier: MIT
#include "ParameterSort.hpp"

#include <algorithm>
#include <numeric>

#include <QJsonValue>

#include "../framework/ParameterSuggest.hpp"

namespace mitcad {
namespace {

// A text column's key: blank when the cell is empty.
ParameterSortKey textKey(const QString& text) {
  ParameterSortKey key;
  key.blank = text.trimmed().isEmpty();
  key.text = text;
  return key;
}

// A box's key: on before off.
ParameterSortKey flagKey(bool on) {
  ParameterSortKey key;
  key.blank = false;
  key.rank = on ? 0 : 1;
  return key;
}

// A canonical number (mm, rad or a plain number) of a parameter's kind.
ParameterSortKey numberKey(const QJsonValue& value, ParameterKind kind) {
  ParameterSortKey key;
  if (value.isDouble()) {
    key.blank = false;
    key.rank = static_cast<int>(kind);
    key.number = value.toDouble();
  }
  return key;
}

int compareKeys(const ParameterSortKey& a, const ParameterSortKey& b) {
  if (a.rank != b.rank) {
    return a.rank < b.rank ? -1 : 1;
  }
  if (a.number != b.number) {
    return a.number < b.number ? -1 : 1;
  }
  return naturalCompare(a.text, b.text);
}

} // namespace

int naturalCompare(const QString& a, const QString& b) {
  qsizetype i = 0;
  qsizetype j = 0;
  while (i < a.size() && j < b.size()) {
    if (a.at(i).isDigit() && b.at(j).isDigit()) {
      // A run of digits each, compared as numbers: without leading zeros,
      // the longer is the larger, else digit by digit.
      qsizetype endA = i;
      qsizetype endB = j;
      while (endA < a.size() && a.at(endA).isDigit()) {
        ++endA;
      }
      while (endB < b.size() && b.at(endB).isDigit()) {
        ++endB;
      }
      qsizetype startA = i;
      qsizetype startB = j;
      while (startA + 1 < endA && a.at(startA) == QLatin1Char('0')) {
        ++startA;
      }
      while (startB + 1 < endB && b.at(startB) == QLatin1Char('0')) {
        ++startB;
      }
      const qsizetype lengthA = endA - startA;
      const qsizetype lengthB = endB - startB;
      if (lengthA != lengthB) {
        return lengthA < lengthB ? -1 : 1;
      }
      const int digits = QStringView(a).mid(startA, lengthA).compare(QStringView(b).mid(startB, lengthB));
      if (digits != 0) {
        return digits < 0 ? -1 : 1;
      }
      i = endA;
      j = endB;
      continue;
    }
    const QChar ca = a.at(i).toCaseFolded();
    const QChar cb = b.at(j).toCaseFolded();
    if (ca != cb) {
      return ca < cb ? -1 : 1;
    }
    ++i;
    ++j;
  }
  if (i < a.size()) {
    return 1;
  }
  if (j < b.size()) {
    return -1;
  }
  return 0;
}

ParameterSortKey parameterSortKey(const QJsonObject& parameter, ParameterColumn column) {
  const QString unit = parameter.value(QStringLiteral("unit")).toString();
  const ParameterKind kind = kindOfUnit(unit);
  const bool text = kind == ParameterKind::Text;
  const bool reference = parameter.value(QStringLiteral("kind")).toString() == QStringLiteral("reference");
  const QJsonObject limits = parameter.value(QStringLiteral("limits")).toObject();
  switch (column) {
  case ParameterColumn::Name:
    return textKey(parameter.value(QStringLiteral("name")).toString());
  case ParameterColumn::Unit:
    return textKey(unit);
  case ParameterColumn::Expression:
    return textKey(parameter.value(QStringLiteral("expression")).toString());
  case ParameterColumn::Comment:
    return textKey(parameter.value(QStringLiteral("comment")).toString());
  case ParameterColumn::Value: {
    if (text) {
      // A text parameter's value is its text, after the numbers.
      ParameterSortKey key = textKey(parameter.value(QStringLiteral("text")).toString());
      key.rank = static_cast<int>(ParameterKind::Text);
      return key;
    }
    return numberKey(parameter.value(QStringLiteral("value")), kind);
  }
  case ParameterColumn::Min:
  case ParameterColumn::Max:
  case ParameterColumn::Step: {
    // Its own limit, else a feature's fixed one shown in its place.
    const QString field = column == ParameterColumn::Min   ? QStringLiteral("min_value")
                          : column == ParameterColumn::Max ? QStringLiteral("max_value")
                                                           : QStringLiteral("step_value");
    const QJsonValue own = limits.value(field);
    if (own.isDouble()) {
      return numberKey(own, kind);
    }
    const QJsonArray intrinsic = parameter.value(QStringLiteral("intrinsic")).toArray();
    return numberKey(intrinsic.isEmpty() ? QJsonValue() : intrinsic.first().toObject().value(field), kind);
  }
  case ParameterColumn::Favorite:
    return flagKey(parameter.value(QStringLiteral("favorite")).toBool());
  case ParameterColumn::Strict:
    // A driven dimension's and a text have no box.
    return reference || text ? ParameterSortKey() : flagKey(limits.value(QStringLiteral("strict")).toBool());
  case ParameterColumn::Slider: {
    if (reference || text) {
      return ParameterSortKey();
    }
    const QJsonValue min = limits.value(QStringLiteral("min_value"));
    const QJsonValue max = limits.value(QStringLiteral("max_value"));
    return flagKey(min.isDouble() && max.isDouble() && max.toDouble() > min.toDouble());
  }
  }
  return ParameterSortKey();
}

bool sortsBefore(const ParameterSortKey& a, const ParameterSortKey& b, Qt::SortOrder order) {
  if (a.blank || b.blank) {
    return !a.blank && b.blank;
  }
  const int compared = compareKeys(a, b);
  return order == Qt::AscendingOrder ? compared < 0 : compared > 0;
}

std::vector<int> sortedParameterOrder(const QJsonArray& parameters, ParameterColumn column, Qt::SortOrder order) {
  std::vector<ParameterSortKey> keys;
  keys.reserve(static_cast<size_t>(parameters.size()));
  for (const QJsonValue& parameter : parameters) {
    keys.push_back(parameterSortKey(parameter.toObject(), column));
  }
  std::vector<int> indexes(keys.size());
  std::iota(indexes.begin(), indexes.end(), 0);
  std::stable_sort(indexes.begin(), indexes.end(), [&keys, order](int a, int b) {
    return sortsBefore(keys[static_cast<size_t>(a)], keys[static_cast<size_t>(b)], order);
  });
  return indexes;
}

} // namespace mitcad
