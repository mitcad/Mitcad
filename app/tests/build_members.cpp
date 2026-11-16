// SPDX-License-Identifier: MIT
// BUILD parts deleted as members (browser/BuildMembers, mitcad#1843): the
// adders read from the model's build_parts answer, and the build_remove
// commands of the Browser's Delete and Delete Component. A connector and a
// fastener it added, selected in either order, give the connector's
// build_remove only, which takes the fastener with it; a fastener alone, or
// one whose connector stays, gets its own; an adder's adder takes nothing
// more; parts are named once.
#include "UnitTests.hpp"
#include "browser/BuildMembers.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "build_members.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject part(const QString& component, const QJsonValue& addedBy) {
  return QJsonObject{{QStringLiteral("component"), component},
                     {QStringLiteral("name"), component + QStringLiteral(" part")},
                     {QStringLiteral("added_by"), addedBy}};
}

// A connector (C3) that added two fasteners (C4, C5), a timber member (C1)
// it joins, a second connector (C6) with a fastener of its own (C7), and a
// clamp (C8) added by the fastener C7 (an adder's adder).
QJsonArray parts() {
  return QJsonArray{part(QStringLiteral("C1"), QJsonValue::Null),
                    part(QStringLiteral("C3"), QJsonValue::Null),
                    part(QStringLiteral("C4"), QStringLiteral("C3")),
                    part(QStringLiteral("C5"), QStringLiteral("C3")),
                    part(QStringLiteral("C6"), QJsonValue::Null),
                    part(QStringLiteral("C7"), QStringLiteral("C6")),
                    part(QStringLiteral("C8"), QStringLiteral("C7"))};
}

// The components the commands remove, in their order; empty when any
// command is not a build_remove.
QStringList removed(const QJsonArray& commands) {
  QStringList components;
  for (const QJsonValue& value : commands) {
    const QJsonObject command = value.toObject();
    if (command.value(QStringLiteral("cmd")).toString() != QStringLiteral("build_remove") || command.size() != 2) {
      return {};
    }
    components.append(command.value(QStringLiteral("component")).toString());
  }
  return components;
}

void adderTests() {
  const QHash<QString, QString> adders = buildPartAdders(parts());
  CHECK(adders.size() == 7);
  CHECK(adders.contains(QStringLiteral("C1")));
  CHECK(adders.value(QStringLiteral("C1")).isEmpty()); // null: nothing added it
  CHECK(adders.value(QStringLiteral("C4")) == QStringLiteral("C3"));
  CHECK(adders.value(QStringLiteral("C8")) == QStringLiteral("C7"));
  // A row without a component is not a part.
  CHECK(buildPartAdders(QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("x")}}}).isEmpty());
  CHECK(buildPartAdders(QJsonArray{}).isEmpty());
}

void commandTests() {
  const QHash<QString, QString> adders = buildPartAdders(parts());
  const auto list = [&adders](const QStringList& components) {
    return removed(buildRemoveCommands(components, adders));
  };
  // The ticket's case: the connector and its fastener, in either order, go
  // by the connector's build_remove alone, one command array (one undo).
  CHECK(list({QStringLiteral("C3"), QStringLiteral("C4")}) == QStringList{QStringLiteral("C3")});
  CHECK(list({QStringLiteral("C4"), QStringLiteral("C3")}) == QStringList{QStringLiteral("C3")});
  // Both of its fasteners, around it.
  CHECK(list({QStringLiteral("C4"), QStringLiteral("C3"), QStringLiteral("C5")}) == QStringList{QStringLiteral("C3")});
  // A fastener alone, or with a part that did not add it, goes on its own.
  CHECK(list({QStringLiteral("C4")}) == QStringList{QStringLiteral("C4")});
  CHECK(list({QStringLiteral("C4"), QStringLiteral("C1")}) ==
        (QStringList{QStringLiteral("C4"), QStringLiteral("C1")}));
  CHECK(list({QStringLiteral("C4"), QStringLiteral("C6")}) ==
        (QStringList{QStringLiteral("C4"), QStringLiteral("C6")}));
  // Two connectors with their fasteners: each connector once, in order.
  CHECK(list({QStringLiteral("C7"), QStringLiteral("C4"), QStringLiteral("C6"), QStringLiteral("C3")}) ==
        (QStringList{QStringLiteral("C6"), QStringLiteral("C3")}));
  // Only the adder itself takes a part (build_remove removes what that part
  // added, not what those added): the clamp C8 is left to its fastener C7
  // when C7 has a command, but not to C6 alone; with all three, C6 takes
  // C7, which then takes nothing, so C8 keeps its own.
  CHECK(list({QStringLiteral("C7"), QStringLiteral("C8")}) == QStringList{QStringLiteral("C7")});
  CHECK(list({QStringLiteral("C8"), QStringLiteral("C7")}) == QStringList{QStringLiteral("C7")});
  CHECK(list({QStringLiteral("C6"), QStringLiteral("C8")}) ==
        (QStringList{QStringLiteral("C6"), QStringLiteral("C8")}));
  CHECK(list({QStringLiteral("C6"), QStringLiteral("C7"), QStringLiteral("C8")}) ==
        (QStringList{QStringLiteral("C6"), QStringLiteral("C8")}));
  CHECK(list({QStringLiteral("C8"), QStringLiteral("C7"), QStringLiteral("C6")}) ==
        (QStringList{QStringLiteral("C8"), QStringLiteral("C6")}));
  // Adders that name each other (not a model state): no endless walk, and
  // neither is left out.
  const QHash<QString, QString> loop{{QStringLiteral("A"), QStringLiteral("B")},
                                     {QStringLiteral("B"), QStringLiteral("A")}};
  CHECK(removed(buildRemoveCommands({QStringLiteral("A"), QStringLiteral("B")}, loop)) ==
        (QStringList{QStringLiteral("A"), QStringLiteral("B")}));
  // A part named twice is removed once.
  CHECK(list({QStringLiteral("C1"), QStringLiteral("C1")}) == QStringList{QStringLiteral("C1")});
  // A part the model does not tell keeps its command (no adder known).
  CHECK(removed(buildRemoveCommands({QStringLiteral("C9")}, {})) == QStringList{QStringLiteral("C9")});
  CHECK(buildRemoveCommands({}, adders).isEmpty());
}

} // namespace

int buildMembersTests() {
  failures = 0;
  adderTests();
  commandTests();
  return failures;
}
MITCAD_UNIT_TEST(buildMembersTests);
