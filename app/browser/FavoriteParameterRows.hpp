// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QVector>

namespace mitcad {

// The rows of the Favorite Parameters card (mitcad#2203, GitHub
// mitcad/Mitcad#547; browser/FavoriteParametersCard): what it shows of
// each starred parameter, read from the `parameters` query, without any
// widget (the unit test favorite_parameter_rows reads them).

// A parameter's slider (mitcad#665, as the Parameters window's Slider
// column has it): a minimum and a maximum of its own, in its steps when
// it has a step (a thousand positions otherwise, and when it has more
// than ten thousand steps). The numbers are in the parameter's own unit.
struct FavoriteSlider {
  int positions = 0;
  double minNumber = 0.0;
  double maxNumber = 0.0;
  double increment = 0.0;
  int position = 0; // where the parameter's value is now
  QString minText;  // "600 mm"
  QString maxText;
  QString stepText; // empty without a step
};

struct FavoriteRow {
  QString name;
  QString unit;       // the model's unit: empty for a number without one
  QString expression; // as the Parameters window's Expression shows it
  QString valueText;  // the evaluated value with its unit ("60 mm")
  // The expression uses other parameters: its value is shown under it.
  bool formula = false;
  // A driven dimension's reference (mitcad#231): shown, never set.
  bool reference = false;
  bool text = false; // a text parameter (mitcad#664)
  std::optional<FavoriteSlider> slider;
};

// The starred parameters of a `parameters` answer in the Parameters
// window's Favorites order: its sort (the settings' `parameters/sort`,
// "<column> ascending|descending" as its log names the column, or
// "none"/empty for the creation order; mitcad#1866).
QVector<FavoriteRow> favoriteRows(const QJsonArray& parameters, const QString& sort);

// One row read from a `parameters` entry.
FavoriteRow favoriteRow(const QJsonObject& parameter);

// The slider of a parameter, if it has one (none for a text or a driven
// dimension's reference, nor without a minimum and a maximum of its own).
std::optional<FavoriteSlider> favoriteSlider(const QJsonObject& parameter);

// The number at a position of the slider (in the parameter's unit), never
// beyond the maximum.
double sliderNumber(const FavoriteSlider& slider, int position);

// What a typed value sends as the expression (as the Parameters window
// does): a bare number gets the parameter's unit ("30" is "30 mm") and a
// decimal point ("2,5" is "2.5 mm"); anything else, and a text
// parameter's text, as typed.
QString favoriteExpression(const QString& typed, const QString& unit);

// The slider's number as an expression in the parameter's unit.
QString sliderExpression(const FavoriteSlider& slider, int position, const QString& unit);

// The row as the card's log line names it: "width = 1200 mm (600 mm …
// 2400 mm, step 10 mm)", "depth = width / 20 (= 60 mm)", "count = 4".
QString favoriteRowLog(const FavoriteRow& row);

} // namespace mitcad
