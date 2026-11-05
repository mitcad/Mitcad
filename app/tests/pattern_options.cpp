// SPDX-License-Identifier: MIT
// The patterns' Direction Type and Distance Type (mitcad#545): the panels'
// choices as the definitions' `symmetric` and `distance_type` and back, the
// length row's label, Flip while symmetric, the options in words; the
// Pattern Type a selection made before the command picks (mitcad#1290).
#include "commands/PatternOptions.hpp"
#include "framework/Command.hpp"

#include <cstdio>

#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "pattern_options.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

void directionTypeTests() {
  // A direction without `symmetric` (the default, and older designs) is one
  // direction; an imported symmetric one shows Symmetric.
  CHECK(directionTypeOf(QJsonObject{}) == oneDirection());
  CHECK(directionTypeOf(QJsonObject{{QStringLiteral("symmetric"), false}}) == oneDirection());
  CHECK(directionTypeOf(QJsonObject{{QStringLiteral("symmetric"), true}}) == symmetricDirection());

  // Symmetric writes `symmetric: true`; One Direction leaves it out, also
  // where an edited definition had it.
  QJsonObject direction{{QStringLiteral("axis"), QStringLiteral("x")}, {QStringLiteral("quantity"), 3}};
  setDirectionType(direction, symmetricDirection());
  CHECK(direction.value(QStringLiteral("symmetric")).toBool());
  CHECK(directionTypeOf(direction) == symmetricDirection());
  setDirectionType(direction, oneDirection());
  CHECK(!direction.contains(QStringLiteral("symmetric")));
  CHECK(direction.size() == 2);

  // Flip negates only a direction of one way.
  CHECK(flipApplies(oneDirection(), true));
  CHECK(!flipApplies(oneDirection(), false));
  CHECK(!flipApplies(symmetricDirection(), true));
  CHECK(!flipApplies(symmetricDirection(), false));
}

void lengthLabelTests() {
  CHECK(lengthLabel(QStringLiteral("spacing")) == QStringLiteral("Spacing"));
  CHECK(lengthLabel(QStringLiteral("extent")) == QStringLiteral("Extent"));
  CHECK(lengthLabel(QStringLiteral("extent"), QStringLiteral("2")) == QStringLiteral("Extent 2"));
  CHECK(lengthLabel(QStringLiteral("spacing"), QStringLiteral("2")) == QStringLiteral("Spacing 2"));
  // No choice yet: the default's.
  CHECK(lengthLabel(QString()) == QStringLiteral("Spacing"));
}

void optionsTextTests() {
  // The ticket's pattern: symmetric along X (3 over 40 mm), one direction
  // along Y (3 over 100 mm), both by extent.
  const QJsonObject rectangular{
      {QStringLiteral("type"), QStringLiteral("rectangular_pattern")},
      {QStringLiteral("direction1"),
       QJsonObject{{QStringLiteral("axis"), QStringLiteral("x")},
                   {QStringLiteral("quantity"), 3},
                   {QStringLiteral("distance"), QStringLiteral("40 mm")},
                   {QStringLiteral("symmetric"), true}}},
      {QStringLiteral("direction2"),
       QJsonObject{{QStringLiteral("axis"), QStringLiteral("y")},
                   {QStringLiteral("quantity"), 3},
                   {QStringLiteral("distance"), QStringLiteral("100 mm")}}},
      {QStringLiteral("distance_type"), QStringLiteral("extent")}};
  CHECK(patternOptionsText(rectangular) ==
        QStringLiteral("direction 1 symmetric, quantity 3, extent 40 mm; "
                       "direction 2 one direction, quantity 3, extent 100 mm"));
  // One direction, spacing by default, a parameter's name as it is.
  const QJsonObject row{{QStringLiteral("type"), QStringLiteral("rectangular_pattern")},
                        {QStringLiteral("direction1"),
                         QJsonObject{{QStringLiteral("axis"), QStringLiteral("x")},
                                     {QStringLiteral("quantity"), QStringLiteral("d4")},
                                     {QStringLiteral("distance"), -20.5}}}};
  CHECK(patternOptionsText(row) == QStringLiteral("direction 1 one direction, quantity d4, spacing -20.5"));

  const QJsonObject circular{{QStringLiteral("type"), QStringLiteral("circular_pattern")},
                             {QStringLiteral("quantity"), 3},
                             {QStringLiteral("angle"), QStringLiteral("90 deg")},
                             {QStringLiteral("symmetric"), true}};
  CHECK(patternOptionsText(circular) == QStringLiteral("symmetric, quantity 3, angle 90 deg"));

  const QJsonObject path{{QStringLiteral("type"), QStringLiteral("path_pattern")},
                         {QStringLiteral("quantity"), 4},
                         {QStringLiteral("distance"), QStringLiteral("15 mm")},
                         {QStringLiteral("distance_type"), QStringLiteral("spacing")},
                         {QStringLiteral("flip"), true}};
  CHECK(patternOptionsText(path) == QStringLiteral("one direction, quantity 4, spacing 15 mm, flipped"));

  CHECK(patternOptionsText(QJsonObject{{QStringLiteral("type"), QStringLiteral("mirror")}}).isEmpty());
}

// The Pattern Type a preselection picks (mitcad#1290), on the state the
// preselection leaves: the inputs' defaults (Pattern Type Bodies) and each
// item in the first input that took it.
void preselectedTypeTests() {
  const QString type = QStringLiteral("object_type");
  const auto started = [&type] {
    mitcad::CommandState state;
    state.setChoice(type, QStringLiteral("bodies"));
    return state;
  };
  // A hole's face: the Features input took it as its feature.
  const mitcad::SelectionItem hole{mitcad::SelectKind::Feature, QStringLiteral("F4"), QString(), QString()};
  mitcad::CommandState face = started();
  face.setItems(QStringLiteral("features"), {hole});
  choosePreselectedPatternType(face);
  CHECK(face.choice(type) == QStringLiteral("features"));
  CHECK(face.items(QStringLiteral("features")).size() == 1);

  // A body: Bodies stays.
  const mitcad::SelectionItem body{mitcad::SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString()};
  mitcad::CommandState bodies = started();
  bodies.setItems(QStringLiteral("bodies"), {body});
  choosePreselectedPatternType(bodies);
  CHECK(bodies.choice(type) == QStringLiteral("bodies"));

  // A body and a feature: the body decides, Bodies stays.
  mitcad::CommandState both = started();
  both.setItems(QStringLiteral("bodies"), {body});
  both.setItems(QStringLiteral("features"), {hole});
  choosePreselectedPatternType(both);
  CHECK(both.choice(type) == QStringLiteral("bodies"));

  // Nothing taken, or only faces of no feature: Bodies stays, never Faces.
  mitcad::CommandState nothing = started();
  choosePreselectedPatternType(nothing);
  CHECK(nothing.choice(type) == QStringLiteral("bodies"));
  mitcad::CommandState faces = started();
  const mitcad::SelectionItem plain{mitcad::SelectKind::Face, QStringLiteral("B1"), QStringLiteral("f3"),
                                    QStringLiteral("plane")};
  faces.setItems(QStringLiteral("faces"), {plain});
  choosePreselectedPatternType(faces);
  CHECK(faces.choice(type) == QStringLiteral("bodies"));
}

} // namespace

int patternOptionsTests() {
  failures = 0;
  directionTypeTests();
  lengthLabelTests();
  optionsTextTests();
  preselectedTypeTests();
  return failures;
}
