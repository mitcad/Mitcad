// SPDX-License-Identifier: MIT
#pragma once

// Numbers typed in the application's own fields. A decimal comma counts as
// a decimal point, as in the model's expressions (core/model/src/expr,
// mitcad#2); numbers are always shown with a point.

#include <optional>

#include <QDoubleSpinBox>
#include <QString>

namespace mitcad {

// The value of a plain number with a decimal point or comma: "12.5",
// "12,5", ",5", "5,", "-2,5e3", white space around it allowed. Nothing for
// anything else, such as an expression ("d1 * 2", "20 mm"). `normalized`
// gets the number's text with a point ("12,5" -> "12.5"). Value fields
// that take expressions use it to give a bare number the field's unit.
std::optional<double> parsePlainNumber(const QString& text, QString* normalized = nullptr);

// A spin box that takes a decimal comma as a point and shows a point in
// every locale.
class DecimalSpinBox : public QDoubleSpinBox {
public:
  explicit DecimalSpinBox(QWidget* parent = nullptr);

  QValidator::State validate(QString& text, int& pos) const override;
  void fixup(QString& text) const override;
  double valueFromText(const QString& text) const override;
};

} // namespace mitcad
