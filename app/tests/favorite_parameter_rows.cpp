// SPDX-License-Identifier: MIT
// The Favorite Parameters card's rows (browser/FavoriteParameterRows,
// mitcad#2203, GitHub mitcad/Mitcad#547): only the starred parameters, in
// the Parameters window's Favorites order (its sort setting); a formula
// (an expression that uses other parameters) with its value; a slider for
// a minimum and a maximum of the parameter's own, in its steps, none for a
// text or a driven dimension's reference; a typed bare number with the
// parameter's unit; the slider's numbers and the card's log line.
#include "UnitTests.hpp"
#include "browser/FavoriteParameterRows.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "favorite_parameter_rows.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

bool near(double a, double b) { return std::abs(a - b) < 1e-9; }

// A parameter as the `parameters` query gives it: its value canonical (mm),
// its text in its own unit.
QJsonObject parameter(const QString& name, const QString& unit, const QString& expression, const QString& text,
                      double value, bool favorite) {
  return QJsonObject{{QStringLiteral("name"), name},
                     {QStringLiteral("kind"), QStringLiteral("user")},
                     {QStringLiteral("unit"), unit},
                     {QStringLiteral("expression"), expression},
                     {QStringLiteral("text"), text},
                     {QStringLiteral("value"), value},
                     {QStringLiteral("comment"), QString()},
                     {QStringLiteral("favorite"), favorite}};
}

// Limits as the query gives them (mm, the parameter's unit mm).
QJsonObject limits(double min, double max, double step) {
  QJsonObject out{{QStringLiteral("min_value"), min},
                  {QStringLiteral("max_value"), max},
                  {QStringLiteral("min_number"), min},
                  {QStringLiteral("max_number"), max},
                  {QStringLiteral("min_text"), QStringLiteral("%1 mm").arg(min)},
                  {QStringLiteral("max_text"), QStringLiteral("%1 mm").arg(max)},
                  {QStringLiteral("strict"), false}};
  if (step > 0.0) {
    out.insert(QStringLiteral("step_value"), step);
    out.insert(QStringLiteral("step_number"), step);
    out.insert(QStringLiteral("step_text"), QStringLiteral("%1 mm").arg(step));
  }
  return out;
}

QJsonArray shelf() {
  QJsonObject width = parameter(QStringLiteral("width"), QStringLiteral("mm"), QStringLiteral("1200 mm"),
                                QStringLiteral("1200 mm"), 1200.0, true);
  width.insert(QStringLiteral("limits"), limits(600.0, 2400.0, 10.0));
  QJsonObject depth = parameter(QStringLiteral("depth"), QStringLiteral("mm"), QStringLiteral("width / 20"),
                                QStringLiteral("60 mm"), 60.0, true);
  depth.insert(QStringLiteral("dependencies"), QJsonArray{QStringLiteral("width")});
  const QJsonObject height = parameter(QStringLiteral("height"), QStringLiteral("mm"), QStringLiteral("800 mm"),
                                       QStringLiteral("800 mm"), 800.0, false);
  const QJsonObject count =
      parameter(QStringLiteral("count"), QString(), QStringLiteral("4"), QStringLiteral("4"), 4.0, true);
  const QJsonObject board = parameter(QStringLiteral("board"), QStringLiteral("Text"),
                                      QStringLiteral("'Birch plywood'"), QStringLiteral("Birch plywood"), 0.0, true);
  return {width, depth, height, count, board};
}

QStringList names(const QVector<FavoriteRow>& rows) {
  QStringList out;
  for (const FavoriteRow& row : rows) {
    out << row.name;
  }
  return out;
}

void rowTests() {
  // The starred ones in the creation order, the others left out.
  const QVector<FavoriteRow> rows = favoriteRows(shelf(), QString());
  CHECK(names(rows) == (QStringList{QStringLiteral("width"), QStringLiteral("depth"), QStringLiteral("count"),
                                    QStringLiteral("board")}));
  CHECK(names(favoriteRows(shelf(), QStringLiteral("none"))) == names(rows));
  // Sorted as the Parameters window sorts its Favorites group.
  CHECK(names(favoriteRows(shelf(), QStringLiteral("name ascending"))) ==
        (QStringList{QStringLiteral("board"), QStringLiteral("count"), QStringLiteral("depth"),
                     QStringLiteral("width")}));
  CHECK(names(favoriteRows(shelf(), QStringLiteral("name descending"))).first() == QStringLiteral("width"));
  // An unknown column: the creation order.
  CHECK(names(favoriteRows(shelf(), QStringLiteral("colour ascending"))) == names(rows));

  const FavoriteRow& width = rows.at(0);
  CHECK(width.expression == QStringLiteral("1200 mm"));
  CHECK(!width.formula && !width.reference && !width.text);
  CHECK(width.slider.has_value());
  CHECK(width.slider->positions == 180);
  CHECK(width.slider->position == 60);
  CHECK(near(width.slider->increment, 10.0));
  CHECK(favoriteRowLog(width) == QStringLiteral("width = 1200 mm (600 mm … 2400 mm, step 10 mm)"));

  const FavoriteRow& depth = rows.at(1);
  CHECK(depth.formula);
  CHECK(!depth.slider.has_value());
  CHECK(depth.valueText == QStringLiteral("60 mm"));
  CHECK(favoriteRowLog(depth) == QStringLiteral("depth = width / 20 (= 60 mm)"));

  CHECK(favoriteRowLog(rows.at(2)) == QStringLiteral("count = 4"));
  CHECK(rows.at(3).text);
  CHECK(!rows.at(3).slider.has_value());
}

void sliderTests() {
  // Without a step: a thousand positions between the bounds.
  QJsonObject free = parameter(QStringLiteral("gap"), QStringLiteral("mm"), QStringLiteral("2.5 mm"),
                               QStringLiteral("2.5 mm"), 2.5, true);
  free.insert(QStringLiteral("limits"), limits(0.0, 10.0, 0.0));
  const std::optional<FavoriteSlider> slider = favoriteSlider(free);
  CHECK(slider.has_value());
  CHECK(slider->positions == 1000);
  CHECK(slider->position == 250);
  CHECK(slider->stepText.isEmpty());
  CHECK(near(sliderNumber(*slider, 0), 0.0));
  CHECK(near(sliderNumber(*slider, 1000), 10.0));
  CHECK(sliderExpression(*slider, 500, QStringLiteral("mm")) == QStringLiteral("5 mm"));
  CHECK(favoriteRowLog(favoriteRow(free)) == QStringLiteral("gap = 2.5 mm (0 mm … 10 mm)"));

  // Steps that do not reach the maximum: the last position is the last
  // whole step below it, as in the Parameters window.
  QJsonObject stepped = parameter(QStringLiteral("rail"), QStringLiteral("mm"), QStringLiteral("100 mm"),
                                  QStringLiteral("100 mm"), 100.0, true);
  stepped.insert(QStringLiteral("limits"), limits(100.0, 205.0, 25.0));
  const std::optional<FavoriteSlider> rail = favoriteSlider(stepped);
  CHECK(rail.has_value());
  CHECK(rail->positions == 4);
  CHECK(near(sliderNumber(*rail, 3), 175.0));
  CHECK(near(sliderNumber(*rail, 4), 200.0));
  CHECK(sliderExpression(*rail, 1, QStringLiteral("mm")) == QStringLiteral("125 mm"));

  // Only a minimum, a reference and a text: no slider.
  QJsonObject low = parameter(QStringLiteral("low"), QStringLiteral("mm"), QStringLiteral("5 mm"),
                              QStringLiteral("5 mm"), 5.0, true);
  QJsonObject minimum = limits(1.0, 0.0, 0.0);
  minimum.insert(QStringLiteral("max_value"), QJsonValue::Null);
  low.insert(QStringLiteral("limits"), minimum);
  CHECK(!favoriteSlider(low).has_value());
  QJsonObject reference = parameter(QStringLiteral("d7"), QStringLiteral("mm"), QStringLiteral("12 mm"),
                                    QStringLiteral("12 mm"), 12.0, true);
  reference.insert(QStringLiteral("kind"), QStringLiteral("reference"));
  reference.insert(QStringLiteral("limits"), limits(0.0, 20.0, 1.0));
  CHECK(!favoriteSlider(reference).has_value());
  CHECK(favoriteRow(reference).reference);
}

void expressionTests() {
  // A bare number gets the unit and a decimal point; anything else as typed.
  CHECK(favoriteExpression(QStringLiteral("900"), QStringLiteral("mm")) == QStringLiteral("900 mm"));
  CHECK(favoriteExpression(QStringLiteral(" 2,5 "), QStringLiteral("mm")) == QStringLiteral("2.5 mm"));
  CHECK(favoriteExpression(QStringLiteral("5"), QString()) == QStringLiteral("5"));
  CHECK(favoriteExpression(QStringLiteral("width / 10"), QStringLiteral("mm")) == QStringLiteral("width / 10"));
  CHECK(favoriteExpression(QStringLiteral("3 in"), QStringLiteral("mm")) == QStringLiteral("3 in"));
  CHECK(favoriteExpression(QStringLiteral("42"), QStringLiteral("Text")) == QStringLiteral("42"));
}

} // namespace

int favoriteParameterRowsTests() {
  rowTests();
  sliderTests();
  expressionTests();
  return failures;
}
MITCAD_UNIT_TEST(favoriteParameterRowsTests);
