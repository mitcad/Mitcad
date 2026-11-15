// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <Qt>

namespace mitcad {

// The parameter table's columns (Edit Parameters), in its order.
enum class ParameterColumn { Name, Unit, Expression, Value, Comment, Favorite, Min, Max, Step, Strict, Slider };

// What a parameter sorts by in a column (mitcad#1866): a number, a text or
// a box on or off. `rank` comes first: for numbers the kind (lengths, then
// angles, then numbers without a unit, then texts), for boxes on before
// off. A blank cell sorts after the others in either order.
struct ParameterSortKey {
  bool blank = true;
  int rank = 0;
  double number = 0.0;
  QString text;
};

// Texts in natural order, case-insensitive: digits compare as numbers, so
// d2 comes before d10. Negative, zero or positive as a comes before, with
// or after b.
int naturalCompare(const QString& a, const QString& b);

// The key of a parameter (an entry of the `parameters` query) in a column.
// Value, Min, Max and Step sort by their evaluated numbers, in the common
// unit of their kind (mm, rad), so that 1 in comes after 20 mm; Name,
// Unit, Expression and Comment as texts; the favourite, Strict and Slider
// by on or off.
ParameterSortKey parameterSortKey(const QJsonObject& parameter, ParameterColumn column);

// Whether a comes before b in that order (blanks last both ways).
bool sortsBefore(const ParameterSortKey& a, const ParameterSortKey& b, Qt::SortOrder order);

// The parameters' indexes in their order sorted by a column; equal keys
// keep their creation order.
std::vector<int> sortedParameterOrder(const QJsonArray& parameters, ParameterColumn column, Qt::SortOrder order);

} // namespace mitcad
