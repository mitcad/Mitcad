// SPDX-License-Identifier: MIT
// Move/Copy of a component picked inside a sub-assembly (mitcad#2302): the
// component the move works in (a component placed deeper than the active
// one moves in its own parent), picks that need different components,
// and the occurrence whose placement is the move's frame (the parent's on
// the picked path, not the parent component's first occurrence).
#include "UnitTests.hpp"
#include "commands/MoveTarget.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "move_target.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::cmd;

QJsonObject node(const char* uid, const char* component, double x, const QJsonArray& children = QJsonArray()) {
  const QJsonArray world{QJsonArray{1, 0, 0, x}, QJsonArray{0, 1, 0, 0}, QJsonArray{0, 0, 1, 0}};
  return {{QStringLiteral("uid"), QLatin1String(uid)},
          {QStringLiteral("component"), QLatin1String(component)},
          {QStringLiteral("world"), world},
          {QStringLiteral("children"), children}};
}

// The `components` query's tree: the root places Component1 (C1, O1) and
// Other (C6, O7, with Pin C7 at O8). Component1 places Downdraft (C2, O2)
// and Bracket (C5, O6); Downdraft places Common (C3) twice, O3 and O5,
// each with Logo (C4, its own occurrence O4, seen once per Common).
QJsonArray tree() {
  const QJsonArray logo{node("O4", "C4", 0)};
  const QJsonArray common{node("O3", "C3", 30, logo), node("O5", "C3", 50, logo)};
  const QJsonArray component1{node("O2", "C2", 20, common), node("O6", "C5", 60)};
  return {node("O1", "C1", 10, component1), node("O7", "C6", 70, QJsonArray{node("O8", "C7", 80)})};
}

void treeTests() {
  const QJsonArray occurrences = tree();
  CHECK(componentAtPath(occurrences, QString()) == QStringLiteral("C0"));
  CHECK(componentAtPath(occurrences, QStringLiteral("O1/O2/O5/O4")) == QStringLiteral("C4"));
  CHECK(componentAtPath(occurrences, QStringLiteral("O1/O9")).isEmpty());
  CHECK(occurrenceNodeAt(occurrences, QStringLiteral("O1/O2/O5")).value(QStringLiteral("component")).toString() ==
        QStringLiteral("C3"));
  CHECK(occurrenceNodeAt(occurrences, QString()).isEmpty());
  CHECK(parentPathOf(QStringLiteral("O1/O2/O3/O4")) == QStringLiteral("O1/O2/O3"));
  CHECK(parentPathOf(QStringLiteral("O7")).isEmpty());
  CHECK(occurrenceCountOf(occurrences, QStringLiteral("C3")) == 2);
  CHECK(occurrenceCountOf(occurrences, QStringLiteral("C4")) == 2);
  CHECK(occurrenceCountOf(occurrences, QStringLiteral("C1")) == 1);
}

void targetTests() {
  const QJsonArray occurrences = tree();
  const QString active = QStringLiteral("C1");
  // The reported case: Logo picked by its row three levels under the active
  // Component1 moves in Common, its parent, at the Common it was picked in.
  MoveTarget target = moveTargetOf({QStringLiteral("O1/O2/O3/O4")}, active, occurrences);
  CHECK(target.component == QStringLiteral("C3"));
  CHECK(target.parentPath == QStringLiteral("O1/O2/O3"));
  CHECK(!target.mixed);
  target = moveTargetOf({QStringLiteral("O1/O2/O5/O4")}, active, occurrences);
  CHECK(target.component == QStringLiteral("C3"));
  CHECK(target.parentPath == QStringLiteral("O1/O2/O5"));
  // Logo in both Commons: one component, one feature.
  target = moveTargetOf({QStringLiteral("O1/O2/O3/O4"), QStringLiteral("O1/O2/O5/O4")}, active, occurrences);
  CHECK(target.component == QStringLiteral("C3") && !target.mixed);
  // Placed in the active component: moves there, as before.
  target = moveTargetOf({QStringLiteral("O1/O6")}, active, occurrences);
  CHECK(target.component == active);
  CHECK(target.parentPath == QStringLiteral("O1"));
  CHECK(!target.mixed);
  // Bracket (in Component1) beside Logo (in Common): two components.
  target = moveTargetOf({QStringLiteral("O1/O6"), QStringLiteral("O1/O2/O3/O4")}, active, occurrences);
  CHECK(target.component == active);
  CHECK(target.mixed);
  target = moveTargetOf({QStringLiteral("O1/O2/O3/O4"), QStringLiteral("O1/O2/O3")}, active, occurrences);
  CHECK(target.component == QStringLiteral("C3"));
  CHECK(target.mixed);
  // Not under the active component at all: stays with it, the model says
  // why it cannot move it.
  target = moveTargetOf({QStringLiteral("O7/O8")}, active, occurrences);
  CHECK(target.component == active);
  CHECK(target.parentPath.isEmpty());
  CHECK(!target.mixed);
  // The root active: a component deep inside moves in its parent; one in
  // the root in the root.
  target = moveTargetOf({QStringLiteral("O1/O2/O3/O4")}, QStringLiteral("C0"), occurrences);
  CHECK(target.component == QStringLiteral("C3"));
  CHECK(target.parentPath == QStringLiteral("O1/O2/O3"));
  target = moveTargetOf({QStringLiteral("O7")}, QStringLiteral("C0"), occurrences);
  CHECK(target.component == QStringLiteral("C0"));
  CHECK(target.parentPath.isEmpty());
  // Nothing picked: the active component.
  target = moveTargetOf({}, active, occurrences);
  CHECK(target.component == active && !target.mixed);
}

void frameTests() {
  const QJsonArray occurrences = tree();
  // The parent's occurrence on the picked path, the second Common here, not
  // Common's first occurrence.
  CHECK(frameOccurrenceOf({QStringLiteral("O1/O2/O5/O4")}, QStringLiteral("C3"), occurrences) ==
        QStringLiteral("O1/O2/O5"));
  CHECK(frameOccurrenceOf({QStringLiteral("O1/O2/O3/O4")}, QStringLiteral("C3"), occurrences) ==
        QStringLiteral("O1/O2/O3"));
  // Nothing picked under it (Bodies, Move Face): its first occurrence.
  CHECK(frameOccurrenceOf({}, QStringLiteral("C3"), occurrences) == QStringLiteral("O1/O2/O3"));
  CHECK(frameOccurrenceOf({QStringLiteral("O7/O8")}, QStringLiteral("C1"), occurrences) == QStringLiteral("O1"));
  // The root has no occurrence: the identity.
  CHECK(frameOccurrenceOf({QStringLiteral("O7")}, QStringLiteral("C0"), occurrences).isEmpty());
  CHECK(frameOccurrenceOf({}, QString(), occurrences).isEmpty());
  // The frame's placement is that occurrence's `world`.
  const QJsonArray world =
      occurrenceNodeAt(occurrences, frameOccurrenceOf({QStringLiteral("O1/O2/O5/O4")}, QStringLiteral("C3"), occurrences))
          .value(QStringLiteral("world"))
          .toArray();
  CHECK(world.size() == 3 && world.at(0).toArray().at(3).toDouble() == 50.0);
}

int moveTargetTests() {
  failures = 0;
  treeTests();
  targetTests();
  frameTests();
  return failures;
}

} // namespace

MITCAD_UNIT_TEST(moveTargetTests);
