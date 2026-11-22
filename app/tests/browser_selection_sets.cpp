// SPDX-License-Identifier: MIT
// The Browser's Selection Sets folder (mitcad#2192): a BrowserPanel built
// from a snapshot. The folder is there only while the design has a set,
// beside Named Views; the sets by name, each with its members' rows: the
// item's own row type with the set's name, its live name, grayed while its
// item is suppressed, rolled back or gone. A set's row selects its members;
// the item's own rows are what Find in Browser finds, never the links. The
// members a set takes of a selection and of rows (browser/SelectionSets).
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"
#include "browser/SelectionSets.hpp"

#include <cstdio>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QStringList>
#include <QTreeWidget>
#include <QList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_selection_sets.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject member(const char* kind, const char* key, const char* reference, const char* name, bool active,
                   const char* type = "", const char* occurrence = "", const char* component = "C0") {
  QJsonObject made{{QStringLiteral("kind"), QString::fromLatin1(kind)},
                   {QStringLiteral("name"), QString::fromLatin1(name)},
                   {QStringLiteral("occurrence"), QString::fromLatin1(occurrence)},
                   {QStringLiteral("component"), QString::fromLatin1(component)},
                   {QStringLiteral("exists"), true},
                   {QStringLiteral("active"), active}};
  if (key[0] != '\0') {
    made.insert(QString::fromLatin1(key), QString::fromLatin1(reference));
  }
  if (type[0] != '\0') {
    made.insert(QStringLiteral("type"), QString::fromLatin1(type));
  }
  return made;
}

// Root (C0) with Lid (F2.b0, made by Extrude1, F2) on Sketch1 (F1); Rail:1
// (O1, of C1). Favourites links Lid, Sketch1, Extrude2 (F3, suppressed)
// and Rail:1; Zeta is empty.
DocumentSnapshot design(const QString& lidName) {
  DocumentSnapshot snapshot;
  DocumentSnapshot::Component root;
  root.uid = QStringLiteral("C0");
  root.name = QStringLiteral("Root");
  root.bodies << QStringLiteral("F2.b0");
  snapshot.components.append(root);
  DocumentSnapshot::Component rail;
  rail.uid = QStringLiteral("C1");
  rail.name = QStringLiteral("Rail");
  snapshot.components.append(rail);
  DocumentSnapshot::Body lid;
  lid.uid = QStringLiteral("F2.b0");
  lid.name = lidName;
  lid.component = QStringLiteral("C0");
  lid.visible = true;
  snapshot.bodies.insert(lid.uid, lid);
  DocumentSnapshot::Feature sketch;
  sketch.uid = QStringLiteral("F1");
  sketch.name = QStringLiteral("Sketch1");
  sketch.type = QStringLiteral("sketch");
  sketch.component = QStringLiteral("C0");
  sketch.status = QStringLiteral("ok");
  snapshot.features.append(sketch);
  DocumentSnapshot::Occurrence placed;
  placed.uid = QStringLiteral("O1");
  placed.path = QStringLiteral("O1");
  placed.name = QStringLiteral("Rail:1");
  placed.component = QStringLiteral("C1");
  snapshot.occurrences.append(placed);
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

QJsonArray sets(const QString& lidName) {
  const QJsonArray favourites{
      member("body", "body", "F2.b0", lidName.toLatin1().constData(), true),
      member("feature", "feature", "F1", "Sketch1", true, "sketch"),
      member("feature", "feature", "F3", "Extrude2", false, "extrude"),
      member("component", "", "", "Rail:1", true, "", "O1", "C1")};
  // Zeta first: the Browser sorts the sets by name.
  return QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("Zeta")},
                                {QStringLiteral("members"), QJsonArray()}},
                    QJsonObject{{QStringLiteral("name"), QStringLiteral("Favourites")},
                                {QStringLiteral("members"), favourites}}};
}

// The only row with this name, anywhere in the tree.
QTreeWidgetItem* rowNamed(QTreeWidget* tree, const QString& name) {
  const QList<QTreeWidgetItem*> found = tree->findItems(name, Qt::MatchExactly | Qt::MatchRecursive, 0);
  return found.size() == 1 ? found.first() : nullptr;
}

void folderTests() {
  // No set: no folder.
  {
    BrowserPanel panel;
    panel.rebuild(design(QStringLiteral("Lid")), false);
    for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
      CHECK(row.node.type != BrowserNode::Type::SelectionSets && row.node.type != BrowserNode::Type::SelectionSet);
    }
  }
  DocumentSnapshot snapshot = design(QStringLiteral("Lid"));
  snapshot.selectionSets = sets(QStringLiteral("Lid"));
  BrowserPanel panel;
  panel.rebuild(snapshot, false);
  QStringList rootRows;
  QStringList setRows;
  QHash<QString, BrowserPanel::PresentationRow> rows;
  QString rootKey;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    rows.insert(row.node.path, row);
    if (row.node.path == QStringLiteral("Root")) {
      rootKey = row.key;
    } else if (row.parent == rootKey) {
      rootRows << row.node.name;
    }
    if (row.node.path.startsWith(QStringLiteral("Root/Selection Sets/"))) {
      setRows << row.node.path;
    }
  }
  // Beside Named Views, before Origin.
  const qsizetype views = rootRows.indexOf(QStringLiteral("Named Views"));
  const qsizetype folder = rootRows.indexOf(QStringLiteral("Selection Sets"));
  CHECK(views >= 0 && folder == views + 1);
  CHECK(rootRows.indexOf(QStringLiteral("Origin")) > folder);
  // The sets by name, each member under its set.
  CHECK(setRows == QStringList({QStringLiteral("Root/Selection Sets/Favourites"),
                                QStringLiteral("Root/Selection Sets/Favourites/Lid"),
                                QStringLiteral("Root/Selection Sets/Favourites/Sketch1"),
                                QStringLiteral("Root/Selection Sets/Favourites/Extrude2"),
                                QStringLiteral("Root/Selection Sets/Favourites/Rail:1"),
                                QStringLiteral("Root/Selection Sets/Zeta")}));
  const BrowserPanel::PresentationRow setRow = rows.value(QStringLiteral("Root/Selection Sets/Favourites"));
  CHECK(setRow.node.type == BrowserNode::Type::SelectionSet && setRow.node.uid == QStringLiteral("Favourites"));
  CHECK(!setRow.node.isSetMember());
  CHECK(!setRow.node.item().isValid());
  // Members: the item's own row type, linked to the set.
  const BrowserNode lid = rows.value(QStringLiteral("Root/Selection Sets/Favourites/Lid")).node;
  CHECK(lid.type == BrowserNode::Type::Body && lid.uid == QStringLiteral("F2.b0"));
  CHECK(lid.selectionSet == QStringLiteral("Favourites") && lid.isSetMember());
  CHECK(lid.item().kind == SelectKind::Body && lid.item().owner == QStringLiteral("F2.b0"));
  CHECK(!lid.hasEye);
  const BrowserNode sketch = rows.value(QStringLiteral("Root/Selection Sets/Favourites/Sketch1")).node;
  CHECK(sketch.type == BrowserNode::Type::Sketch && sketch.uid == QStringLiteral("F1"));
  const BrowserNode extrude = rows.value(QStringLiteral("Root/Selection Sets/Favourites/Extrude2")).node;
  CHECK(extrude.type == BrowserNode::Type::Feature && extrude.uid == QStringLiteral("F3"));
  const BrowserNode railRow = rows.value(QStringLiteral("Root/Selection Sets/Favourites/Rail:1")).node;
  CHECK(railRow.type == BrowserNode::Type::Component && railRow.occurrence == QStringLiteral("O1"));
  CHECK(railRow.item().kind == SelectKind::Component);
  // Gray while its item is suppressed, rolled back or gone; the others plain.
  CHECK(rows.value(QStringLiteral("Root/Selection Sets/Favourites/Extrude2")).tone == QStringLiteral("dimmed"));
  CHECK(rows.value(QStringLiteral("Root/Selection Sets/Favourites/Lid")).tone.isEmpty());
  // Apart from the item's own rows: keys of their own, never what the
  // feature's rows are.
  CHECK(rows.value(QStringLiteral("Root/Selection Sets/Favourites/Lid")).key !=
        rows.value(QStringLiteral("Root/Bodies/Lid")).key);
  const QVector<BrowserNode> own = panel.nodesOfFeature(QStringLiteral("F1"));
  CHECK(own.size() == 1 && !own.first().isSetMember());
  CHECK(panel.setMemberNodes(QStringLiteral("Favourites")).size() == 4);
  CHECK(panel.setMemberNodes(QStringLiteral("Zeta")).isEmpty());

  // A click on the set's row selects every member: the body, the sketch
  // and the occurrence as items, the feature by its uid.
  Selection picked;
  QStringList pickedFeatures;
  QObject::connect(&panel, &BrowserPanel::rowsPicked, &panel,
                   [&picked, &pickedFeatures](const Selection& items, const QStringList& features) {
                     picked = items;
                     pickedFeatures = features;
                   });
  auto* tree = panel.findChild<QTreeWidget*>();
  CHECK(tree != nullptr);
  if (tree != nullptr) {
    QTreeWidgetItem* favourites = rowNamed(tree, QStringLiteral("Favourites"));
    CHECK(favourites != nullptr);
    if (favourites != nullptr) {
      favourites->setSelected(true);
      CHECK(picked.size() == 3);
      CHECK(pickedFeatures == QStringList{QStringLiteral("F3")});
      CHECK(picked.contains(lid.item()) && picked.contains(sketch.item()) && picked.contains(railRow.item()));
    }
  }

  // Renamed: the member row shows the item's new name.
  snapshot = design(QStringLiteral("Cover"));
  snapshot.selectionSets = sets(QStringLiteral("Cover"));
  panel.rebuild(snapshot, false);
  bool renamed = false;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    renamed = renamed || row.node.path == QStringLiteral("Root/Selection Sets/Favourites/Cover");
  }
  CHECK(renamed);
}

void memberTests() {
  // What a set takes of a selection: bodies, components, sketches,
  // construction features, joints and features, with their occurrence.
  const SelectionItem body{SelectKind::Body, QStringLiteral("F2.b0"), QString(), QString(), QStringLiteral("O1")};
  CHECK(setMemberOf(body) ==
        QJsonObject({{QStringLiteral("body"), QStringLiteral("F2.b0")}, {QStringLiteral("occurrence"), QStringLiteral("O1")}}));
  const SelectionItem rail{SelectKind::Component, QStringLiteral("C1"), QString(), QString(), QStringLiteral("O1")};
  CHECK(setMemberOf(rail) == QJsonObject({{QStringLiteral("occurrence"), QStringLiteral("O1")}}));
  const SelectionItem sketch{SelectKind::Sketch, QStringLiteral("F1"), QString(), QString()};
  CHECK(setMemberOf(sketch) == QJsonObject({{QStringLiteral("feature"), QStringLiteral("F1")}}));
  const SelectionItem plane{SelectKind::Plane, QStringLiteral("F4"), QString(), QStringLiteral("plane")};
  CHECK(setMemberOf(plane) == QJsonObject({{QStringLiteral("feature"), QStringLiteral("F4")}}));
  // Not taken: faces, edges, the origin's planes, the root.
  const SelectionItem face{SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:end"), QStringLiteral("plane")};
  CHECK(setMemberOf(face).isEmpty());
  const SelectionItem origin{SelectKind::Plane, QStringLiteral("xy"), QString(), QStringLiteral("plane")};
  CHECK(setMemberOf(origin).isEmpty());
  CHECK(setMemberOf(SelectionItem{SelectKind::Component, QStringLiteral("C0"), QString(), QString()}).isEmpty());
  // A selection with a face offers no set; one without takes each once.
  CHECK(setMembersOf(Selection{body, face}).isEmpty());
  CHECK(setMembersOf(Selection{body, sketch, body}).size() == 2);
  // Rows: a feature's row as the feature, a member's row as its item.
  BrowserNode feature;
  feature.type = BrowserNode::Type::Feature;
  feature.uid = QStringLiteral("F3");
  CHECK(setMemberOf(feature) == QJsonObject({{QStringLiteral("feature"), QStringLiteral("F3")}}));
  BrowserNode linked = feature;
  linked.selectionSet = QStringLiteral("Favourites");
  CHECK(setMemberOf(linked) == setMemberOf(feature));
  BrowserNode views;
  views.type = BrowserNode::Type::NamedViews;
  CHECK(setMemberOf(views).isEmpty());
  CHECK(setMembersOf(QVector<BrowserNode>{feature, views, linked}).size() == 1);
  // The feature's row selected selects its body's row too: the feature
  // alone; another body stays.
  BrowserNode made;
  made.type = BrowserNode::Type::Body;
  made.uid = QStringLiteral("F3.b0");
  BrowserNode other = made;
  other.uid = QStringLiteral("F2.b0");
  CHECK(setMembersOf(QVector<BrowserNode>{other, feature, made}) ==
        QJsonArray({QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")}},
                    QJsonObject{{QStringLiteral("feature"), QStringLiteral("F3")}}}));
  CHECK(setMemberText(setMemberOf(body)) == QStringLiteral("body F2.b0 in O1"));
}

} // namespace

int browserSelectionSetsTests() {
  failures = 0;
  folderTests();
  memberTests();
  return failures;
}
MITCAD_UNIT_TEST(browserSelectionSetsTests);
