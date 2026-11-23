// SPDX-License-Identifier: MIT
#include "FavoriteParameterRows.hpp"

#include <algorithm>
#include <cmath>
#include <iterator>
#include <numeric>
#include <vector>

#include <QJsonValue>
#include <QStringList>

#include "../framework/Numbers.hpp"
#include "ParameterSort.hpp"

namespace mitcad {
namespace {

QString key(const char* text) { return QString::fromLatin1(text); }

bool isTextUnit(const QString& unit) {
  return unit.trimmed().compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0;
}

// The Parameters window's columns as its log and its sort setting name
// them (ParametersDialog's columnLogName), in ParameterColumn's order.
constexpr const char* kColumnNames[] = {"name", "unit", "expression", "value",  "comment", "favorite",
                                        "min",  "max",  "step",       "strict", "slider"};

} // namespace

std::optional<FavoriteSlider> favoriteSlider(const QJsonObject& parameter) {
  const QString unit = parameter.value(key("unit")).toString();
  if (isTextUnit(unit) || parameter.value(key("kind")).toString() == key("reference")) {
    return std::nullopt;
  }
  const QJsonObject limits = parameter.value(key("limits")).toObject();
  const QJsonValue minValue = limits.value(key("min_value"));
  const QJsonValue maxValue = limits.value(key("max_value"));
  if (!minValue.isDouble() || !maxValue.isDouble() || !(maxValue.toDouble() > minValue.toDouble())) {
    return std::nullopt;
  }
  FavoriteSlider slider;
  slider.minNumber = limits.value(key("min_number")).toDouble();
  slider.maxNumber = limits.value(key("max_number")).toDouble();
  if (!(slider.maxNumber > slider.minNumber)) {
    return std::nullopt;
  }
  const double step = limits.value(key("step_number")).toDouble(0.0);
  slider.positions = 1000;
  slider.increment = (slider.maxNumber - slider.minNumber) / slider.positions;
  if (step > 0.0) {
    const double steps = std::floor((slider.maxNumber - slider.minNumber) / step + 1e-9);
    if (steps >= 1.0 && steps <= 10000.0) {
      slider.positions = static_cast<int>(steps);
      slider.increment = step;
    }
  }
  // Where the value is, from the canonical values (the same proportion).
  const double fraction = (parameter.value(key("value")).toDouble() - minValue.toDouble()) /
                          (maxValue.toDouble() - minValue.toDouble());
  const double number = slider.minNumber + fraction * (slider.maxNumber - slider.minNumber);
  slider.position =
      std::clamp(static_cast<int>(std::lround((number - slider.minNumber) / slider.increment)), 0, slider.positions);
  slider.minText = limits.value(key("min_text")).toString();
  slider.maxText = limits.value(key("max_text")).toString();
  if (step > 0.0) {
    slider.stepText = limits.value(key("step_text")).toString();
  }
  return slider;
}

FavoriteRow favoriteRow(const QJsonObject& parameter) {
  FavoriteRow row;
  row.name = parameter.value(key("name")).toString();
  row.unit = parameter.value(key("unit")).toString();
  row.expression = parameter.value(key("expression")).toString();
  row.valueText = parameter.value(key("text")).toString();
  row.reference = parameter.value(key("kind")).toString() == key("reference");
  row.text = isTextUnit(row.unit);
  row.formula = !parameter.value(key("dependencies")).toArray().isEmpty();
  row.slider = favoriteSlider(parameter);
  return row;
}

QVector<FavoriteRow> favoriteRows(const QJsonArray& parameters, const QString& sort) {
  std::vector<int> order(static_cast<size_t>(parameters.size()));
  std::iota(order.begin(), order.end(), 0);
  const QStringList words = sort.split(QLatin1Char(' '), Qt::SkipEmptyParts);
  if (words.size() == 2) {
    for (int column = 0; column < static_cast<int>(std::size(kColumnNames)); ++column) {
      if (QString::fromLatin1(kColumnNames[column]) == words.at(0)) {
        order = sortedParameterOrder(parameters, static_cast<ParameterColumn>(column),
                                     words.at(1) == QStringLiteral("descending") ? Qt::DescendingOrder
                                                                                 : Qt::AscendingOrder);
        break;
      }
    }
  }
  QVector<FavoriteRow> rows;
  for (const int index : order) {
    const QJsonObject parameter = parameters.at(index).toObject();
    if (parameter.value(key("favorite")).toBool()) {
      rows << favoriteRow(parameter);
    }
  }
  return rows;
}

double sliderNumber(const FavoriteSlider& slider, int position) {
  if (position >= slider.positions && slider.increment * slider.positions > slider.maxNumber - slider.minNumber) {
    return slider.maxNumber;
  }
  return slider.minNumber + slider.increment * std::clamp(position, 0, slider.positions);
}

QString favoriteExpression(const QString& typed, const QString& unit) {
  const QString expression = typed.trimmed();
  QString number;
  // A text parameter's expression is a text (mitcad#664), never a number
  // with a unit.
  if (isTextUnit(unit) || !parsePlainNumber(expression, &number)) {
    return expression;
  }
  return unit.isEmpty() ? number : number + QLatin1Char(' ') + unit;
}

QString sliderExpression(const FavoriteSlider& slider, int position, const QString& unit) {
  return favoriteExpression(QString::number(sliderNumber(slider, position), 'g', 12), unit);
}

QString favoriteRowLog(const FavoriteRow& row) {
  QString line = QStringLiteral("%1 = %2").arg(row.name, row.expression);
  if (row.formula) {
    line += QStringLiteral(" (= %1)").arg(row.valueText);
  }
  if (row.slider) {
    QString range = QStringLiteral("%1 … %2").arg(row.slider->minText, row.slider->maxText);
    if (!row.slider->stepText.isEmpty()) {
      range += QStringLiteral(", step %1").arg(row.slider->stepText);
    }
    line += QStringLiteral(" (%1)").arg(range);
  }
  return line;
}

} // namespace mitcad
