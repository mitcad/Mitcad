// SPDX-License-Identifier: MIT
#include "Numbers.hpp"

#include <cmath>

#include <QLocale>

namespace mitcad {
namespace {

// The text with its commas as points.
QString withPoints(QString text) { return text.replace(QLatin1Char(','), QLatin1Char('.')); }

} // namespace

std::optional<double> parsePlainNumber(const QString& text, QString* normalized) {
  QString number = text.trimmed();
  // A number has one decimal separator, so one comma and no point is the
  // decimal comma; "1,000" is 1.0, as the model reads it.
  if (number.count(QLatin1Char(',')) == 1 && !number.contains(QLatin1Char('.'))) {
    number = withPoints(number);
  }
  bool ok = false;
  const double value = number.toDouble(&ok); // the C locale: points only
  if (!ok || !std::isfinite(value)) {
    return std::nullopt;
  }
  if (normalized != nullptr) {
    *normalized = number;
  }
  return value;
}

DecimalSpinBox::DecimalSpinBox(QWidget* parent) : QDoubleSpinBox(parent) {
  // Points in every locale (and no group separators).
  setLocale(QLocale::c());
}

QValidator::State DecimalSpinBox::validate(QString& text, int& pos) const {
  // A comma typed becomes a point in the field.
  text = withPoints(text);
  return QDoubleSpinBox::validate(text, pos);
}

void DecimalSpinBox::fixup(QString& text) const {
  text = withPoints(text);
  QDoubleSpinBox::fixup(text);
}

double DecimalSpinBox::valueFromText(const QString& text) const {
  return QDoubleSpinBox::valueFromText(withPoints(text));
}

} // namespace mitcad
