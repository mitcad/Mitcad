// SPDX-License-Identifier: MIT
// Measure's history (mitcad#648): a row per measurement, newest first; a
// pick's own row becomes the pair's when the second pick follows in the
// same panel, never an earlier panel's row; the same picks again update
// their row in place; gone rows; numbers after Clear; tab-separated text.
#include "UnitTests.hpp"
#include "commands/MeasureHistory.hpp"

#include <cstdio>

#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "measure_history.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::MeasureHistory::Row measured(const QStringList& picks, const QString& picked, const QStringList& values) {
  mitcad::MeasureHistory::Row row;
  row.key = picks.join(QLatin1Char('\n'));
  row.picks = static_cast<int>(picks.size());
  row.picked = picked;
  row.values = values;
  return row;
}

} // namespace

int measureHistoryTests() {
  using mitcad::MeasureHistory;
  const QString a = QStringLiteral("Face A");
  const QString b = QStringLiteral("Edge B");
  const QString c = QStringLiteral("Vertex C");
  MeasureHistory history;
  CHECK(history.isEmpty());

  // A panel: the first pick, then the second: one row.
  history.beginRun();
  CHECK(history.record(measured({a}, QStringLiteral("Face of Body1"), {QStringLiteral("Area 400 mm²")})));
  CHECK(history.record(measured({a, b}, QStringLiteral("Face of Body1 ↔ Edge of Body2"),
                                {QStringLiteral("Distance 20 mm")})));
  CHECK(history.rows().size() == 1);
  CHECK(history.rows().front().number == 1);
  CHECK(history.rows().front().picks == 2);
  // Another second pick in the same panel: a row of its own.
  CHECK(history.record(measured({a, c}, QStringLiteral("Face of Body1 ↔ Vertex of Body2"),
                                {QStringLiteral("Distance 30 mm")})));
  CHECK(history.rows().size() == 2);
  CHECK(history.rows().front().number == 2);

  // The next panel: its first pick does not take the place of a row.
  history.beginRun();
  CHECK(history.record(measured({c}, QStringLiteral("Vertex of Body2"), {QStringLiteral("Position (1, 2, 3)")})));
  CHECK(history.rows().size() == 3);
  CHECK(history.rows().front().number == 3);
  CHECK(history.rows().back().number == 1);

  // The same picks again (the row shown again): no change, or new values
  // in place.
  CHECK(!history.record(measured({a, b}, QStringLiteral("Face of Body1 ↔ Edge of Body2"),
                                 {QStringLiteral("Distance 20 mm")})));
  CHECK(history.record(measured({a, b}, QStringLiteral("Face of Body1 ↔ Edge of Body2"),
                                {QStringLiteral("Distance 25 mm")})));
  CHECK(history.rows().size() == 3);
  CHECK(history.rows().back().values == QStringList{QStringLiteral("Distance 25 mm")});
  CHECK(history.row(1) != nullptr && history.row(1)->number == 1);
  CHECK(history.row(7) == nullptr);

  // Gone, and back when measured again.
  CHECK(history.setGone(2, true));
  CHECK(!history.setGone(2, true));
  CHECK(history.row(2)->gone);
  CHECK(history.record(measured({a, c}, QStringLiteral("Face of Body1 ↔ Vertex of Body2"),
                                {QStringLiteral("Distance 30 mm")})));
  CHECK(!history.row(2)->gone);

  // Tab-separated text, newest first.
  CHECK(MeasureHistory::tabSeparated(*history.row(1)) ==
        QStringLiteral("1\tFace of Body1 ↔ Edge of Body2\tDistance 25 mm"));
  const QStringList lines = history.tabSeparated().split(QLatin1Char('\n'));
  CHECK(lines.size() == 3);
  CHECK(lines.value(0).startsWith(QStringLiteral("3\tVertex of Body2\t")));
  CHECK(lines.value(2).startsWith(QStringLiteral("1\t")));

  // Clear: numbers start again.
  history.clear();
  CHECK(history.isEmpty());
  history.beginRun();
  CHECK(history.record(measured({b}, QStringLiteral("Edge of Body2"), {QStringLiteral("Length 20 mm")})));
  CHECK(history.rows().front().number == 1);
  // Nothing picked: no row.
  CHECK(!history.record(measured({}, QString(), {})));
  CHECK(history.rows().size() == 1);
  return failures;
}
MITCAD_UNIT_TEST(measureHistoryTests);
