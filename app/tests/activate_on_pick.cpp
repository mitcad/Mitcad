// SPDX-License-Identifier: MIT
// A feature that changes the picked body on another component's body
// activates that component (mitcad#1634, browser/ActivateOnPick): the
// command table (Fillet, Chamfer, Shell and the rest; not Create Sketch,
// the Construct commands or Extrude), when a pick activates, the panel's
// Activate button for picks of several components and the notice.
#include "browser/ActivateOnPick.hpp"

#include <cstdio>

#include <QSet>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "activate_on_pick.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// Root (C0) with a body of its own (F9.b0); Base:1 (O1, C1) with a block
// (F2.b0); Lid placed twice, Lid:1 (O2) and Lid:2 (O3) (C2), with a block
// (F4.b0) and a construction plane (F7). Base is active.
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  const auto component = [](const QString& uid, const QString& name) {
    DocumentSnapshot::Component made;
    made.uid = uid;
    made.name = name;
    return made;
  };
  const auto occurrence = [](const QString& uid, const QString& name, const QString& of) {
    DocumentSnapshot::Occurrence made;
    made.uid = uid;
    made.name = name;
    made.path = uid;
    made.component = of;
    return made;
  };
  const auto body = [](const QString& uid, const QString& of) {
    DocumentSnapshot::Body made;
    made.uid = uid;
    made.name = uid;
    made.component = of;
    return made;
  };
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root")));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Base")));
  snapshot.components.append(component(QStringLiteral("C2"), QStringLiteral("Lid")));
  snapshot.occurrences.append(occurrence(QStringLiteral("O1"), QStringLiteral("Base:1"), QStringLiteral("C1")));
  snapshot.occurrences.append(occurrence(QStringLiteral("O2"), QStringLiteral("Lid:1"), QStringLiteral("C2")));
  snapshot.occurrences.append(occurrence(QStringLiteral("O3"), QStringLiteral("Lid:2"), QStringLiteral("C2")));
  snapshot.bodies.insert(QStringLiteral("F2.b0"), body(QStringLiteral("F2.b0"), QStringLiteral("C1")));
  snapshot.bodies.insert(QStringLiteral("F4.b0"), body(QStringLiteral("F4.b0"), QStringLiteral("C2")));
  snapshot.bodies.insert(QStringLiteral("F9.b0"), body(QStringLiteral("F9.b0"), QStringLiteral("C0")));
  DocumentSnapshot::Feature plane;
  plane.uid = QStringLiteral("F7");
  plane.name = QStringLiteral("Plane1");
  plane.type = QStringLiteral("construction_plane");
  plane.component = QStringLiteral("C2");
  snapshot.features.append(plane);
  snapshot.activeComponent = QStringLiteral("C1");
  return snapshot;
}

SelectionItem pick(SelectKind kind, const QString& owner, const QString& occurrence) {
  SelectionItem item;
  item.kind = kind;
  item.owner = owner;
  item.name = kind == SelectKind::Edge ? QStringLiteral("E{F4:end(r1)|F4:side(c1)}") : QStringLiteral("F4:end(r1)");
  item.occurrence = occurrence;
  return item;
}

SelectionItem edge(const QString& body, const QString& occurrence) { return pick(SelectKind::Edge, body, occurrence); }

void tableTests() {
  // Fillet and Chamfer by their edges (and a whole body's), Shell by its
  // faces or body: the Done-when's three, by the inputs that hold the
  // picked bodies.
  CHECK(activatingInputs(QStringLiteral("solid.fillet")).contains(QStringLiteral("edges")));
  CHECK(activatingInputs(QStringLiteral("solid.fillet")).contains(QStringLiteral("body")));
  CHECK(activatingInputs(QStringLiteral("solid.chamfer")).contains(QStringLiteral("edges")));
  CHECK(activatingInputs(QStringLiteral("solid.chamfer")).contains(QStringLiteral("body")));
  CHECK(activatingInputs(QStringLiteral("solid.shell")) == QStringList{QStringLiteral("faces")});
  // The rest of the features that change a body.
  for (const char* id : {"solid.draft", "solid.offset_face", "solid.replace_face", "solid.split_face",
                         "solid.delete_face", "solid.move_face", "solid.thread", "solid.split_body",
                         "solid.press_pull", "solid.hole", "solid.dogbone", "solid.combine"}) {
    CHECK(!activatingInputs(QString::fromLatin1(id)).isEmpty());
  }
  // Only the inputs of the changed bodies: a Replace Face's new surface,
  // a Draft's plane, a Combine's tools and a Hole's sketch points may be
  // another component's, linked.
  CHECK(!activatingInputs(QStringLiteral("solid.replace_face")).contains(QStringLiteral("target")));
  CHECK(!activatingInputs(QStringLiteral("solid.draft")).contains(QStringLiteral("plane")));
  CHECK(!activatingInputs(QStringLiteral("solid.combine")).contains(QStringLiteral("tools")));
  CHECK(activatingInputs(QStringLiteral("solid.hole")) == QStringList{QStringLiteral("at")});
  // Not here: Create Sketch and the Construct commands keep their question
  // (mitcad#960), Extrude its linked objects; an unknown command nothing.
  for (const char* id : {"sketch.create", "construct.offset_plane", "construct.axis_through_cylinder",
                         "construct.point_at_vertex", "solid.extrude", "solid.revolve", "solid.move", ""}) {
    CHECK(activatingInputs(QString::fromLatin1(id)).isEmpty());
  }
  // One row per command, each with inputs.
  QSet<QString> seen;
  for (const ActivatingCommand& row : activatingCommands()) {
    CHECK(!seen.contains(row.command));
    seen.insert(row.command);
    CHECK(row.command.startsWith(QStringLiteral("solid.")));
    CHECK(!row.inputs.isEmpty());
  }
}

void pickTests() {
  const DocumentSnapshot snapshot = design();

  // The component of a body's face, edge, vertex or body; none for a datum
  // or a preview's body.
  CHECK(bodyComponent(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O2"))) == QStringLiteral("C2"));
  CHECK(bodyComponent(snapshot, pick(SelectKind::Body, QStringLiteral("F2.b0"), QStringLiteral("O1"))) ==
        QStringLiteral("C1"));
  CHECK(bodyComponent(snapshot, pick(SelectKind::Plane, QStringLiteral("F7"), QStringLiteral("O2"))).isEmpty());
  CHECK(bodyComponent(snapshot, edge(QStringLiteral("F99.b0"), QStringLiteral("O9"))).isEmpty());

  // The first pick, of Lid's edge: Lid activated, by its name.
  const PickActivation lid = activationForPick(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O2")), {});
  CHECK(!lid.isEmpty());
  CHECK(lid.component == QStringLiteral("C2"));
  CHECK(lid.name == QStringLiteral("Lid"));
  CHECK(lid.picked.owner == QStringLiteral("F4.b0"));
  CHECK(lid.dropped == 0);
  // At its other occurrence: the same component's definition.
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O3")), {}).component ==
        QStringLiteral("C2"));
  // A face and the body itself too.
  CHECK(activationForPick(snapshot, pick(SelectKind::Face, QStringLiteral("F4.b0"), QStringLiteral("O2")), {})
            .component == QStringLiteral("C2"));
  CHECK(activationForPick(snapshot, pick(SelectKind::Body, QStringLiteral("F4.b0"), QStringLiteral("O2")), {})
            .component == QStringLiteral("C2"));
  // The root's own body while Base is active: the root.
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F9.b0"), QString()), {}).component == QStringLiteral("C0"));

  // The active component's own edge: nothing.
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F2.b0"), QStringLiteral("O1")), {}).isEmpty());
  // A datum of another component (linked, not a changed body): nothing.
  CHECK(activationForPick(snapshot, pick(SelectKind::Plane, QStringLiteral("F7"), QStringLiteral("O2")), {})
            .isEmpty());
  // After a pick of the active component's: nothing, the refusal.
  const Selection baseFirst{edge(QStringLiteral("F2.b0"), QStringLiteral("O1"))};
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O2")), baseFirst).isEmpty());
  // After a pick of a third component's (the root's): nothing.
  const Selection rootFirst{edge(QStringLiteral("F9.b0"), QString())};
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O2")), rootFirst).isEmpty());
  // After picks of the same component only (an agent's, not activated):
  // activated now; a datum picked before does not count.
  const Selection lidFirst{edge(QStringLiteral("F4.b0"), QStringLiteral("O3")),
                           pick(SelectKind::Plane, QStringLiteral("xy"), QString())};
  CHECK(activationForPick(snapshot, edge(QStringLiteral("F4.b0"), QStringLiteral("O2")), lidFirst).component ==
        QStringLiteral("C2"));
}

void offerTests() {
  const DocumentSnapshot snapshot = design();

  // Nothing picked, or only the active component's: no button.
  CHECK(activationForPicks(snapshot, {}).isEmpty());
  CHECK(activationForPicks(snapshot, {edge(QStringLiteral("F2.b0"), QStringLiteral("O1"))}).isEmpty());

  // An edge of Base, then one of Lid: Activate Lid, dropping Base's.
  const PickActivation mixed = activationForPicks(snapshot, {edge(QStringLiteral("F2.b0"), QStringLiteral("O1")),
                                                             edge(QStringLiteral("F4.b0"), QStringLiteral("O2"))});
  CHECK(mixed.component == QStringLiteral("C2"));
  CHECK(mixed.name == QStringLiteral("Lid"));
  CHECK(mixed.dropped == 1);

  // Lid's, then the root's: the last one that is not the active one's.
  const PickActivation third = activationForPicks(snapshot, {edge(QStringLiteral("F4.b0"), QStringLiteral("O2")),
                                                             edge(QStringLiteral("F4.b0"), QStringLiteral("O3")),
                                                             edge(QStringLiteral("F9.b0"), QString())});
  CHECK(third.component == QStringLiteral("C0"));
  CHECK(third.name == QStringLiteral("Root"));
  CHECK(third.dropped == 2);

  // All of Lid's (an agent's picks, never activated by themselves): the
  // button, dropping nothing; a datum among them is no component's.
  const PickActivation agent = activationForPicks(
      snapshot, {edge(QStringLiteral("F4.b0"), QStringLiteral("O2")), pick(SelectKind::Plane, QStringLiteral("F7"),
                                                                            QStringLiteral("O2"))});
  CHECK(agent.component == QStringLiteral("C2"));
  CHECK(agent.dropped == 0);
}

void noticeTests() {
  CHECK(activatedNotice(QStringLiteral("Fillet"), QStringLiteral("Lid"), 0) ==
        QStringLiteral("Fillet goes into Lid (now active)."));
  CHECK(activatedNotice(QStringLiteral("Chamfer"), QStringLiteral("Lid"), 1) ==
        QStringLiteral("Chamfer goes into Lid (now active).\n1 pick of another component dropped."));
  CHECK(activatedNotice(QStringLiteral("Shell"), QStringLiteral("Base"), 3) ==
        QStringLiteral("Shell goes into Base (now active).\n3 picks of other components dropped."));
}

} // namespace

int activateOnPickTests() {
  failures = 0;
  tableTests();
  pickTests();
  offerTests();
  noticeTests();
  return failures;
}
