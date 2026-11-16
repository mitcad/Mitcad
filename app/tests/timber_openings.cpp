// SPDX-License-Identifier: MIT
// A timber frame's openings as the Openings window edits them (mitcad#519,
// build/TimberOpenings): the text form to rows and back (a window, a door
// without a sill, a floor's opening), an offset from the wall's end B,
// the model's `openings` both ways, the summary, and the checks of the
// rows as they are typed (outside the wall, overlapping, a window without
// its sill, the header above the top plate; a partition's narrower
// framing). The model's side: core/model-tests/build-timber/src/wall_bearing_tests.rs.
#include "UnitTests.hpp"
#include "build/TimberOpenings.hpp"

#include <cmath>
#include <cstdio>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "timber_openings.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::build;

bool near(double a, double b) { return std::abs(a - b) < 1e-6; }

void textToRows() {
  QString problem;
  // A window and a door (no sill), as the panel's row has them.
  const std::vector<OpeningRow> rows = openingRows(QStringLiteral("600 900x1200 900; 2000 900x2100"), false, &problem);
  CHECK(problem.isEmpty());
  CHECK(rows.size() == 2);
  if (rows.size() == 2) {
    CHECK(rows[0].kind == OpeningRow::Kind::Window);
    CHECK(near(rows[0].offset, 600.0) && near(rows[0].width, 900.0) && near(rows[0].height, 1200.0));
    CHECK(near(rows[0].sill, 900.0));
    CHECK(!rows[0].fromEnd);
    CHECK(rows[1].kind == OpeningRow::Kind::Door);
    CHECK(near(rows[1].offset, 2000.0) && near(rows[1].height, 2100.0) && near(rows[1].sill, 0.0));
  }
  // A sill of 0 is a door; "mm" and a decimal comma are numbers too.
  const std::vector<OpeningRow> door = openingRows(QStringLiteral(" 600mm  900x2100 0 ;; "), false, &problem);
  CHECK(problem.isEmpty() && door.size() == 1 && door[0].kind == OpeningRow::Kind::Door);
  CHECK(openingNumber(QStringLiteral("12,5")).value_or(0.0) == 12.5);
  CHECK(!openingNumber(QStringLiteral("twelve")));
  // A floor's opening: from the outline's corner across and along the
  // joists, its width and length.
  const std::vector<OpeningRow> floor = openingRows(QStringLiteral("600 400 900x2400"), true, &problem);
  CHECK(problem.isEmpty() && floor.size() == 1);
  if (floor.size() == 1) {
    CHECK(floor[0].kind == OpeningRow::Kind::Floor);
    CHECK(near(floor[0].offset, 600.0) && near(floor[0].across, 400.0));
    CHECK(near(floor[0].width, 900.0) && near(floor[0].height, 2400.0));
  }
  // Not the format: no rows and the format said.
  CHECK(openingRows(QStringLiteral("600 900 1200"), false, &problem).empty());
  CHECK(problem.contains(QStringLiteral("offset widthxheight")));
  CHECK(openingRows(QStringLiteral("600 900x2400"), true, &problem).empty());
  CHECK(problem.contains(QStringLiteral("a b widthxlength")));
  CHECK(openingRows(QString(), false, &problem).empty() && problem.isEmpty());
}

void rowsToText() {
  OpeningRow window;
  window.offset = 600;
  window.width = 900;
  window.height = 1200;
  window.sill = 900;
  OpeningRow door;
  door.kind = OpeningRow::Kind::Door;
  door.offset = 2000;
  door.width = 900;
  door.height = 2100;
  door.sill = 750; // a door has none: not written
  const std::vector<OpeningRow> rows{window, door};
  const QString text = openingsText(rows);
  CHECK(text == QStringLiteral("600 900x1200 900; 2000 900x2100"));
  // Both ways: the text's rows are the rows (the door without a sill).
  std::vector<OpeningRow> back = openingRows(text, false);
  door.sill = 0;
  CHECK(back == (std::vector<OpeningRow>{window, door}));
  CHECK(openingsText(back) == text);
  // From End B: 300 from the end of a 3600 wall to the far side of a 900
  // opening is 2400 from its start.
  OpeningRow fromEnd = window;
  fromEnd.fromEnd = true;
  fromEnd.offset = 300;
  CHECK(near(offsetFromStart(fromEnd, 3600.0), 2400.0));
  CHECK(openingsText({fromEnd}, 3600.0) == QStringLiteral("2400 900x1200 900"));
  // A floor's, and numbers as short as they are exact.
  OpeningRow hole;
  hole.kind = OpeningRow::Kind::Floor;
  hole.offset = 600;
  hole.across = 400.5;
  hole.width = 900;
  hole.height = 2400;
  CHECK(openingsText({hole}) == QStringLiteral("600 400.5 900x2400"));
  CHECK(openingRows(openingsText({hole}), true) == std::vector<OpeningRow>{hole});
  CHECK(openingsText({}).isEmpty());
}

void rowsAsJson() {
  const std::vector<OpeningRow> rows = openingRows(QStringLiteral("600 900x1200 900; 2000 900x2100"), false);
  const QJsonArray json = openingsJson(rows);
  CHECK(json.size() == 2);
  CHECK(json.at(0).toObject().value(QStringLiteral("sill")).toDouble() == 900.0);
  CHECK(!json.at(1).toObject().contains(QStringLiteral("sill")));
  CHECK(json.at(1).toObject().value(QStringLiteral("offset")).toDouble() == 2000.0);
  CHECK(openingRowsOf(json) == rows);
  const std::vector<OpeningRow> floor = openingRows(QStringLiteral("600 400 900x2400"), true);
  const QJsonArray floorJson = openingsJson(floor);
  CHECK(floorJson.at(0).toObject().value(QStringLiteral("offset")).toArray().at(1).toDouble() == 400.0);
  CHECK(floorJson.at(0).toObject().value(QStringLiteral("size")).toArray().at(1).toDouble() == 2400.0);
  CHECK(openingRowsOf(floorJson) == floor);
  CHECK(openingsSummary(rows) == QStringLiteral("2 openings: 1 window, 1 door"));
  CHECK(openingsSummary(floor) == QStringLiteral("1 opening"));
  CHECK(openingsSummary({}) == QStringLiteral("No openings"));
}

void checks() {
  WallSize wall;
  wall.length = 3600;
  wall.height = 2400;
  wall.studThick = 48;
  wall.studDepth = 98;
  wall.loadBearing = true;
  std::vector<OpeningRow> rows = openingRows(QStringLiteral("600 900x1200 900; 2000 900x2100"), false);
  QStringList problems = openingProblems(rows, wall);
  CHECK(problems.size() == 2 && problems[0].isEmpty() && problems[1].isEmpty());
  // Outside the wall; too near its end with its studs; overlapping.
  rows = openingRows(QStringLiteral("4000 900x1200 900; 2604 900x2100; 1200 900x1200 900"), false);
  problems = openingProblems(rows, wall);
  CHECK(problems.size() == 3);
  if (problems.size() == 3) {
    CHECK(problems[0].contains(QStringLiteral("outside the wall")));
    CHECK(problems[1].contains(QStringLiteral("end studs")));
    CHECK(problems[2].isEmpty());
  }
  // A partition's framing is a stud narrower each side: the door at 2604
  // fits it.
  wall.loadBearing = false;
  problems = openingProblems(rows, wall);
  CHECK(problems.size() == 3 && problems[1].isEmpty());
  wall.loadBearing = true;
  rows = openingRows(QStringLiteral("600 900x1200 900; 1400 900x1200 900"), false);
  problems = openingProblems(rows, wall);
  CHECK(problems.size() == 2 && problems[0].isEmpty() && problems[1].contains(QStringLiteral("overlaps opening 1")));
  // A window without its sill; a header above the top plate.
  OpeningRow window;
  window.offset = 600;
  window.width = 900;
  window.height = 1200;
  problems = openingProblems({window}, wall);
  CHECK(problems.size() == 1 && problems[0].contains(QStringLiteral("sill")));
  window.sill = 1200;
  problems = openingProblems({window}, wall);
  CHECK(problems.size() == 1 && problems[0].contains(QStringLiteral("above the top plate")));
  // Unknown sizes are not checked.
  problems = openingProblems({window}, WallSize());
  CHECK(problems.size() == 1 && problems[0].isEmpty());
  // A floor's: within its size, no negative offsets.
  const std::vector<OpeningRow> holes = openingRows(QStringLiteral("600 400 900x2400; 3500 0 900x900; -1 0 900x900"), true);
  problems = floorOpeningProblems(holes, 4000, 3000);
  CHECK(problems.size() == 3 && problems[0].isEmpty());
  CHECK(problems.size() == 3 && problems[1].contains(QStringLiteral("outside the floor")));
  CHECK(problems.size() == 3 && problems[2].contains(QStringLiteral("negative")));
}

} // namespace

int timberOpeningsTests() {
  textToRows();
  rowsToText();
  rowsAsJson();
  checks();
  return failures;
}
MITCAD_UNIT_TEST(timberOpeningsTests);
