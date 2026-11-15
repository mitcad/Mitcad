// SPDX-License-Identifier: MIT
// Edit Parameters sorted by a column (browser/ParameterSort, mitcad#1866,
// GitHub mitcad/Mitcad#338): names in natural order, case-insensitive (d2
// before d10); values by their numbers in the common unit of their kind,
// so 1 in (25.4 mm) sorts between 2 cm and 30 mm, lengths before angles,
// numbers without a unit and texts; limits by their own numbers or a
// feature's fixed ones; boxes on before off; blank cells last both ways;
// equal keys in the creation order.
#include "browser/ParameterSort.hpp"

#include <cstdio>
#include <initializer_list>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "parameter_sort.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// A parameter as the `parameters` query gives it: its value canonical (mm
// or rad), its text in its own unit.
QJsonObject parameter(const QString& name, const QString& unit, const QString& expression, const QString& text,
                      const QJsonValue& value) {
  return QJsonObject{{QStringLiteral("name"), name},
                     {QStringLiteral("kind"), QStringLiteral("user")},
                     {QStringLiteral("unit"), unit},
                     {QStringLiteral("expression"), expression},
                     {QStringLiteral("text"), text},
                     {QStringLiteral("value"), value},
                     {QStringLiteral("comment"), QString()},
                     {QStringLiteral("favorite"), false}};
}

// The names in the order sorted by a column.
QStringList sortedNames(const QJsonArray& parameters, ParameterColumn column, Qt::SortOrder order) {
  QStringList names;
  for (const int index : sortedParameterOrder(parameters, column, order)) {
    names << parameters.at(index).toObject().value(QStringLiteral("name")).toString();
  }
  return names;
}

QStringList list(std::initializer_list<const char*> names) {
  QStringList out;
  for (const char* name : names) {
    out << QString::fromUtf8(name);
  }
  return out;
}

void naturalOrderTests() {
  CHECK(naturalCompare(QStringLiteral("d2"), QStringLiteral("d10")) < 0);
  CHECK(naturalCompare(QStringLiteral("d10"), QStringLiteral("d2")) > 0);
  CHECK(naturalCompare(QStringLiteral("D2"), QStringLiteral("d10")) < 0);   // case-insensitive
  CHECK(naturalCompare(QStringLiteral("width"), QStringLiteral("Width")) == 0);
  CHECK(naturalCompare(QStringLiteral("apple"), QStringLiteral("Banana")) < 0);
  CHECK(naturalCompare(QStringLiteral("len"), QStringLiteral("len2")) < 0);
  CHECK(naturalCompare(QStringLiteral("x007"), QStringLiteral("x7")) == 0); // leading zeros
  CHECK(naturalCompare(QStringLiteral("x9y"), QStringLiteral("x10a")) < 0);
  CHECK(naturalCompare(QStringLiteral("rib12 b"), QStringLiteral("rib12 a")) > 0);
  CHECK(naturalCompare(QString(), QStringLiteral("a")) < 0);
}

// The test design's user parameters, in their creation order.
QJsonArray mixed() {
  return QJsonArray{
      parameter(QStringLiteral("len10"), QStringLiteral("in"), QStringLiteral("1 in"), QStringLiteral("1 in"), 25.4),
      parameter(QStringLiteral("Width"), QStringLiteral("cm"), QStringLiteral("2 cm"), QStringLiteral("2 cm"), 20.0),
      parameter(QStringLiteral("len2"), QStringLiteral("mm"), QStringLiteral("30 mm"), QStringLiteral("30 mm"), 30.0),
      parameter(QStringLiteral("count"), QString(), QStringLiteral("3"), QStringLiteral("3"), 3.0),
      parameter(QStringLiteral("angle"), QStringLiteral("deg"), QStringLiteral("90 deg"), QStringLiteral("90 deg"),
                1.5707963267948966),
      parameter(QStringLiteral("label"), QStringLiteral("Text"), QStringLiteral("'abc'"), QStringLiteral("abc"),
                QStringLiteral("abc")),
  };
}

void nameTests() {
  const QJsonArray parameters = mixed();
  CHECK(sortedNames(parameters, ParameterColumn::Name, Qt::AscendingOrder) ==
        list({"angle", "count", "label", "len2", "len10", "Width"}));
  CHECK(sortedNames(parameters, ParameterColumn::Name, Qt::DescendingOrder) ==
        list({"Width", "len10", "len2", "label", "count", "angle"}));
}

void valueTests() {
  const QJsonArray parameters = mixed();
  // 2 cm = 20 mm < 1 in = 25.4 mm < 30 mm; then the angle, the number
  // without a unit and the text. Their texts would sort 1 in, 2 cm, 3,
  // 30 mm.
  CHECK(sortedNames(parameters, ParameterColumn::Value, Qt::AscendingOrder) ==
        list({"Width", "len10", "len2", "angle", "count", "label"}));
  CHECK(sortedNames(parameters, ParameterColumn::Value, Qt::DescendingOrder) ==
        list({"label", "count", "angle", "len2", "len10", "Width"}));
  // A length in inches against one in millimetres, key by key.
  const ParameterSortKey inch = parameterSortKey(parameters.at(0).toObject(), ParameterColumn::Value);
  const ParameterSortKey cm = parameterSortKey(parameters.at(1).toObject(), ParameterColumn::Value);
  CHECK(!inch.blank && !cm.blank);
  CHECK(sortsBefore(cm, inch, Qt::AscendingOrder));
  CHECK(!sortsBefore(inch, cm, Qt::AscendingOrder));
  CHECK(sortsBefore(inch, cm, Qt::DescendingOrder));
  // A value the model could not evaluate is blank: last both ways.
  QJsonArray broken = parameters;
  broken.append(parameter(QStringLiteral("broken"), QStringLiteral("mm"), QStringLiteral("x +"), QString(),
                          QJsonValue()));
  CHECK(sortedNames(broken, ParameterColumn::Value, Qt::AscendingOrder).last() == QStringLiteral("broken"));
  CHECK(sortedNames(broken, ParameterColumn::Value, Qt::DescendingOrder).last() == QStringLiteral("broken"));
}

void textColumnTests() {
  QJsonArray parameters = mixed();
  QJsonObject width = parameters.at(1).toObject();
  width.insert(QStringLiteral("comment"), QStringLiteral("Plate 10"));
  parameters.replace(1, width);
  QJsonObject count = parameters.at(3).toObject();
  count.insert(QStringLiteral("comment"), QStringLiteral("plate 9"));
  parameters.replace(3, count);
  // Comments in natural order; the rows without one last both ways, in
  // their creation order.
  CHECK(sortedNames(parameters, ParameterColumn::Comment, Qt::AscendingOrder) ==
        list({"count", "Width", "len10", "len2", "angle", "label"}));
  CHECK(sortedNames(parameters, ParameterColumn::Comment, Qt::DescendingOrder) ==
        list({"Width", "count", "len10", "len2", "angle", "label"}));
  // Units as texts; the number without a unit has none.
  CHECK(sortedNames(parameters, ParameterColumn::Unit, Qt::AscendingOrder) ==
        list({"Width", "angle", "len10", "len2", "label", "count"}));
}

void limitAndBoxTests() {
  QJsonArray parameters = mixed();
  // len2: its own minimum 1 in (25.4 mm) and maximum 40 mm, a slider, strict.
  QJsonObject len2 = parameters.at(2).toObject();
  len2.insert(QStringLiteral("limits"),
              QJsonObject{{QStringLiteral("min"), QStringLiteral("1 in")},
                          {QStringLiteral("max"), QStringLiteral("40 mm")},
                          {QStringLiteral("min_value"), 25.4},
                          {QStringLiteral("max_value"), 40.0},
                          {QStringLiteral("step_value"), QJsonValue()},
                          {QStringLiteral("strict"), true}});
  len2.insert(QStringLiteral("favorite"), true);
  parameters.replace(2, len2);
  // len10: a feature's fixed minimum of 5 mm only.
  QJsonObject len10 = parameters.at(0).toObject();
  len10.insert(QStringLiteral("intrinsic"),
               QJsonArray{QJsonObject{{QStringLiteral("min_value"), 5.0}, {QStringLiteral("max_value"), QJsonValue()}}});
  parameters.replace(0, len10);
  // Width: its own minimum of 1 cm (10 mm).
  QJsonObject width = parameters.at(1).toObject();
  width.insert(QStringLiteral("limits"), QJsonObject{{QStringLiteral("min_value"), 10.0},
                                                     {QStringLiteral("strict"), false}});
  width.insert(QStringLiteral("favorite"), true);
  parameters.replace(1, width);
  CHECK(sortedNames(parameters, ParameterColumn::Min, Qt::AscendingOrder).mid(0, 3) ==
        list({"len10", "Width", "len2"}));
  CHECK(sortedNames(parameters, ParameterColumn::Min, Qt::DescendingOrder).mid(0, 3) ==
        list({"len2", "Width", "len10"}));
  CHECK(sortedNames(parameters, ParameterColumn::Max, Qt::AscendingOrder).first() == QStringLiteral("len2"));
  // Boxes: on first ascending, in their creation order among themselves.
  CHECK(sortedNames(parameters, ParameterColumn::Favorite, Qt::AscendingOrder) ==
        list({"Width", "len2", "len10", "count", "angle", "label"}));
  CHECK(sortedNames(parameters, ParameterColumn::Favorite, Qt::DescendingOrder) ==
        list({"len10", "count", "angle", "label", "Width", "len2"}));
  CHECK(sortedNames(parameters, ParameterColumn::Slider, Qt::AscendingOrder).first() == QStringLiteral("len2"));
  // Strict: a text has no box, last both ways.
  CHECK(sortedNames(parameters, ParameterColumn::Strict, Qt::AscendingOrder).first() == QStringLiteral("len2"));
  CHECK(sortedNames(parameters, ParameterColumn::Strict, Qt::AscendingOrder).last() == QStringLiteral("label"));
  CHECK(sortedNames(parameters, ParameterColumn::Strict, Qt::DescendingOrder).last() == QStringLiteral("label"));
  // A driven dimension's reference has no Strict box either.
  QJsonObject reference = parameter(QStringLiteral("d7"), QStringLiteral("mm"), QStringLiteral("12 mm"),
                                    QStringLiteral("12 mm"), 12.0);
  reference.insert(QStringLiteral("kind"), QStringLiteral("reference"));
  CHECK(parameterSortKey(reference, ParameterColumn::Strict).blank);
  CHECK(!parameterSortKey(reference, ParameterColumn::Value).blank);
}

} // namespace

int parameterSortTests() {
  naturalOrderTests();
  nameTests();
  valueTests();
  textColumnTests();
  limitAndBoxTests();
  return failures;
}
