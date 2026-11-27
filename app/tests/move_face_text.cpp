// SPDX-License-Identifier: MIT
// Move Face's summary (mitcad#531): each kind of motion in words, values
// as numbers or through the parameters they name, a free move's matrix as
// its turn and translation.
#include "UnitTests.hpp"
#include "commands/MoveFaceText.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "move_face_text.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

void translateTests() {
  // The ticket's line, as an imported moved face has it.
  const QJsonObject translate{{QStringLiteral("type"), QStringLiteral("translate_xyz")},
                              {QStringLiteral("x"), -1.856},
                              {QStringLiteral("y"), 0.0},
                              {QStringLiteral("z"), -0.361}};
  CHECK(motionText(translate) == QStringLiteral("translate (-1.856, 0, -0.361) mm"));
  CHECK(moveFaceSummary(QStringLiteral("Move Face 1"), 2, QStringLiteral("Body1"), translate) ==
        QStringLiteral("Move Face 1: 2 faces of Body1, translate (-1.856, 0, -0.361) mm"));
  CHECK(moveFaceSummary(QStringLiteral("MoveFace1"), 1, QStringLiteral("Body1"), translate)
            .startsWith(QStringLiteral("MoveFace1: 1 face of Body1, ")));
  // Rounded to a millionth, never "-0".
  const QJsonObject tiny{{QStringLiteral("type"), QStringLiteral("translate_xyz")},
                         {QStringLiteral("x"), 5.0000000001},
                         {QStringLiteral("y"), -1e-12},
                         {QStringLiteral("z"), 0.0}};
  CHECK(motionText(tiny) == QStringLiteral("translate (5, 0, 0) mm"));
  // A parameter by its value, or by its name when it cannot be told.
  const QJsonObject named{{QStringLiteral("type"), QStringLiteral("translate_xyz")},
                          {QStringLiteral("x"), QStringLiteral("push")},
                          {QStringLiteral("y"), 0.0},
                          {QStringLiteral("z"), QStringLiteral("other")}};
  const SlotValue value = [](const QJsonValue& slot) -> std::optional<double> {
    if (slot.toString() == QStringLiteral("push")) {
      return 2.5;
    }
    return std::nullopt;
  };
  CHECK(motionText(named, value) == QStringLiteral("translate (2.5, 0, other) mm"));
  CHECK(motionText(named) == QStringLiteral("translate (push, 0, other) mm"));
}

void otherMotionTests() {
  const double pi = 3.14159265358979323846;
  const QJsonObject edge{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("edge"), QStringLiteral("E{a|b}")}};
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("rotate")},
                    {QStringLiteral("axis"), edge},
                    {QStringLiteral("angle"), 5.0 * pi / 180.0}}) == QStringLiteral("rotate 5 deg about an edge"));
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("translate_along")},
                    {QStringLiteral("axis"), QStringLiteral("x")},
                    {QStringLiteral("distance"), 3.0}}) == QStringLiteral("translate 3 mm along x"));
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("point_to_point")}}) == QStringLiteral("point to point"));
  // Along one axis only (mitcad#2339); All as without it.
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("point_to_point")}, {QStringLiteral("along"), QStringLiteral("z")}}) ==
        QStringLiteral("point to point along z"));
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("point_to_point")}, {QStringLiteral("along"), QStringLiteral("all")}}) ==
        QStringLiteral("point to point"));
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("point_to_position")},
                    {QStringLiteral("x"), 10.0},
                    {QStringLiteral("y"), 0.0},
                    {QStringLiteral("z"), 0.0}}) == QStringLiteral("point to (10, 0, 0) mm"));
  // A free move: a pure translation, then a quarter turn about Z.
  const QJsonArray shift{QJsonArray{1, 0, 0, 4}, QJsonArray{0, 1, 0, 0}, QJsonArray{0, 0, 1, -2}};
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("free")}, {QStringLiteral("matrix"), shift}}) ==
        QStringLiteral("translate (4, 0, -2) mm"));
  const QJsonArray turn{QJsonArray{0, -1, 0, 1}, QJsonArray{1, 0, 0, 2}, QJsonArray{0, 0, 1, 3}};
  CHECK(motionText({{QStringLiteral("type"), QStringLiteral("free")}, {QStringLiteral("matrix"), turn}}) ==
        QStringLiteral("turn 90 deg, translate (1, 2, 3) mm"));
}

// Up to an object (mitcad#2548): a definition's `to_object` read as its
// motion, said by the object's kind.
void toObjectTests() {
  const QJsonObject face{{QStringLiteral("type"), QStringLiteral("face")},
                         {QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("face"), QStringLiteral("F2:side(c4[c3,c1])")}};
  const QJsonObject def{{QStringLiteral("type"), QStringLiteral("move_face")},
                        {QStringLiteral("body"), QStringLiteral("F2.b0")},
                        {QStringLiteral("faces"), QJsonArray{QStringLiteral("F8:start(r{c1})")}},
                        {QStringLiteral("to_object"), face}};
  CHECK(motionText(moveFaceMotion(def)) == QStringLiteral("up to a face"));
  CHECK(moveFaceSummary(QStringLiteral("MoveFace1"), 1, QStringLiteral("Body1"), moveFaceMotion(def)) ==
        QStringLiteral("MoveFace1: 1 face of Body1, up to a face"));
  const auto upTo = [](const QJsonObject& object) {
    return motionText({{QStringLiteral("type"), QStringLiteral("to_object")}, {QStringLiteral("object"), object}});
  };
  CHECK(upTo({{QStringLiteral("type"), QStringLiteral("body")}, {QStringLiteral("body"), QStringLiteral("F2.b0")}}) ==
        QStringLiteral("up to a body"));
  CHECK(upTo({{QStringLiteral("type"), QStringLiteral("plane")}, {QStringLiteral("plane"), QStringLiteral("xy")}}) ==
        QStringLiteral("up to xy"));
  CHECK(upTo({{QStringLiteral("type"), QStringLiteral("point")},
              {QStringLiteral("point"), QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                                    {QStringLiteral("vertex"), QStringLiteral("V{a|b|c}")}}}}) ==
        QStringLiteral("up to a point"));
  // A definition with a motion of its own reads its `transform`.
  const QJsonObject moved{{QStringLiteral("type"), QStringLiteral("move_face")},
                          {QStringLiteral("transform"), QJsonObject{{QStringLiteral("type"), QStringLiteral("translate_xyz")},
                                                                    {QStringLiteral("x"), 1.0},
                                                                    {QStringLiteral("y"), 0.0},
                                                                    {QStringLiteral("z"), 0.0}}}};
  CHECK(motionText(moveFaceMotion(moved)) == QStringLiteral("translate (1, 0, 0) mm"));
}

} // namespace

int moveFaceTextTests() {
  failures = 0;
  translateTests();
  otherMotionTests();
  toObjectTests();
  return failures;
}
MITCAD_UNIT_TEST(moveFaceTextTests);
