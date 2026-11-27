// SPDX-License-Identifier: MIT
// The Browser's construction sets (mitcad#2514, GitHub mitcad/Mitcad#656):
// a BrowserPanel built from a snapshot. A component's sets come before its
// Bodies folder, in the order they were made, the active one in bold; each
// holds its items' own rows (a 3D sketch, a construction plane, a sketch, a
// surface body) in the order they joined it, and those items leave their
// usual folders, which keep the rest. A design without sets shows the
// Browser as before; a folder whose items are all in sets stays while its
// eye is off. The snapshot's lookups: the set of an item, the active set.
#include "UnitTests.hpp"
#include "browser/BrowserPanel.hpp"

#include <cstdio>

#include <QFont>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QStringList>
#include <QTreeWidget>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_construction_sets.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

DocumentSnapshot::Feature feature(const char* uid, const char* name, const char* type) {
  DocumentSnapshot::Feature made;
  made.uid = QString::fromLatin1(uid);
  made.name = QString::fromLatin1(name);
  made.type = QString::fromLatin1(type);
  made.component = QStringLiteral("C0");
  made.status = QStringLiteral("ok");
  return made;
}

DocumentSnapshot::Body body(const char* uid, const char* name) {
  DocumentSnapshot::Body made;
  made.uid = QString::fromLatin1(uid);
  made.name = QString::fromLatin1(name);
  made.component = QStringLiteral("C0");
  made.visible = true;
  return made;
}

QJsonObject member(const char* key, const char* uid, const char* name, const char* type = "") {
  QJsonObject made{{QStringLiteral("kind"), QString::fromLatin1(key)},
                   {QString::fromLatin1(key), QString::fromLatin1(uid)},
                   {QStringLiteral("name"), QString::fromLatin1(name)},
                   {QStringLiteral("exists"), true},
                   {QStringLiteral("active"), true}};
  if (type[0] != '\0') {
    made.insert(QStringLiteral("type"), QString::fromLatin1(type));
  }
  return made;
}

// Root (C0): 3DSketch1 (F1), Plane1 (F2), Sketch1 (F3), Extrude1 (F4,
// the surface Skin, F4.b0), Sketch2 (F5) and Extrude2 (F6, the solid
// Block, F6.b0).
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  DocumentSnapshot::Component root;
  root.uid = QStringLiteral("C0");
  root.name = QStringLiteral("Root");
  root.bodies << QStringLiteral("F4.b0") << QStringLiteral("F6.b0");
  snapshot.components.append(root);
  for (const DocumentSnapshot::Body& b : {body("F4.b0", "Skin"), body("F6.b0", "Block")}) {
    snapshot.bodies.insert(b.uid, b);
  }
  snapshot.features << feature("F1", "3DSketch1", "spatial_sketch") << feature("F2", "Plane1", "construction_plane")
                    << feature("F3", "Sketch1", "sketch") << feature("F4", "Extrude1", "extrude")
                    << feature("F5", "Sketch2", "sketch") << feature("F6", "Extrude2", "extrude");
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

// Guides (active) with 3DSketch1, Plane1, Sketch1 and Skin; Skins empty.
QJsonArray sets() {
  const QJsonArray guides{member("feature", "F1", "3DSketch1", "spatial_sketch"),
                          member("feature", "F2", "Plane1", "construction_plane"),
                          member("feature", "F3", "Sketch1", "sketch"), member("body", "F4.b0", "Skin")};
  return QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("Guides")},
                                {QStringLiteral("component"), QStringLiteral("C0")},
                                {QStringLiteral("component_name"), QStringLiteral("Root")},
                                {QStringLiteral("active"), true},
                                {QStringLiteral("members"), guides}},
                    QJsonObject{{QStringLiteral("name"), QStringLiteral("Skins")},
                                {QStringLiteral("component"), QStringLiteral("C0")},
                                {QStringLiteral("component_name"), QStringLiteral("Root")},
                                {QStringLiteral("active"), false},
                                {QStringLiteral("members"), QJsonArray()}}};
}

struct Rows {
  QHash<QString, BrowserPanel::PresentationRow> byPath;
  QStringList rootRows; // the names of the root's rows, in order
  QHash<QString, QStringList> children; // by path: the names of its rows, in order
};

Rows rowsOf(const BrowserPanel& panel) {
  Rows rows;
  QHash<QString, QString> pathOfKey;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    rows.byPath.insert(row.node.path, row);
    pathOfKey.insert(row.key, row.node.path);
    const QString parent = pathOfKey.value(row.parent);
    if (parent == QStringLiteral("Root")) {
      rows.rootRows << row.node.name;
    }
    if (!parent.isEmpty()) {
      rows.children[parent] << row.node.name;
    }
  }
  return rows;
}

void withoutSets() {
  // A design without sets: the Browser as before.
  BrowserPanel panel;
  panel.rebuild(design(), false);
  const Rows rows = rowsOf(panel);
  CHECK(rows.children.value(QStringLiteral("Root/Bodies")) ==
        QStringList({QStringLiteral("Skin"), QStringLiteral("Block")}));
  CHECK(rows.children.value(QStringLiteral("Root/Sketches")) ==
        QStringList({QStringLiteral("3DSketch1"), QStringLiteral("Sketch1"), QStringLiteral("Sketch2")}));
  CHECK(rows.children.value(QStringLiteral("Root/Construction")) == QStringList({QStringLiteral("Plane1")}));
  for (auto it = rows.byPath.cbegin(); it != rows.byPath.cend(); ++it) {
    CHECK(it.value().node.type != BrowserNode::Type::ConstructionSet);
    CHECK(it.value().node.constructionSet.isEmpty());
  }
}

void withSets() {
  DocumentSnapshot snapshot = design();
  snapshot.constructionSets = sets();
  // The snapshot's lookups.
  CHECK(snapshot.activeConstructionSet() == QStringLiteral("Guides"));
  CHECK(snapshot.constructionSetOf(QStringLiteral("C0"), QStringLiteral("F4.b0")) == QStringLiteral("Guides"));
  CHECK(snapshot.constructionSetOf(QStringLiteral("C0"), QStringLiteral("F5")).isEmpty());
  CHECK(snapshot.constructionSetOf(QStringLiteral("C1"), QStringLiteral("F1")).isEmpty());
  CHECK(snapshot.constructionSetsOf(QStringLiteral("C0")).size() == 2);
  CHECK(snapshot.constructionSetsOf(QStringLiteral("C1")).isEmpty());

  BrowserPanel panel;
  panel.rebuild(snapshot, false);
  const Rows rows = rowsOf(panel);
  // The sets before the Bodies folder, in the order they were made.
  const qsizetype guides = rows.rootRows.indexOf(QStringLiteral("Guides"));
  const qsizetype skins = rows.rootRows.indexOf(QStringLiteral("Skins"));
  const qsizetype bodies = rows.rootRows.indexOf(QStringLiteral("Bodies"));
  CHECK(guides >= 0 && skins == guides + 1 && bodies == skins + 1);
  // Each with its items' own rows in the order they joined it.
  CHECK(rows.children.value(QStringLiteral("Root/Guides")) ==
        QStringList({QStringLiteral("3DSketch1"), QStringLiteral("Plane1"), QStringLiteral("Sketch1"),
                     QStringLiteral("Skin")}));
  CHECK(rows.children.value(QStringLiteral("Root/Skins")).isEmpty());
  // They left their usual folders, which keep the rest; the Construction
  // folder, empty, is gone.
  CHECK(rows.children.value(QStringLiteral("Root/Bodies")) == QStringList({QStringLiteral("Block")}));
  CHECK(rows.children.value(QStringLiteral("Root/Sketches")) == QStringList({QStringLiteral("Sketch2")}));
  CHECK(!rows.rootRows.contains(QStringLiteral("Construction")));
  // The set's row: its name, active; the other one is not.
  const BrowserNode set = rows.byPath.value(QStringLiteral("Root/Guides")).node;
  CHECK(set.type == BrowserNode::Type::ConstructionSet && set.uid == QStringLiteral("Guides"));
  CHECK(set.active && set.component == QStringLiteral("C0"));
  CHECK(set.isFolder() && !set.item().isValid());
  CHECK(!rows.byPath.value(QStringLiteral("Root/Skins")).node.active);
  // The items' rows are their own rows: types, eyes and the set's name.
  const BrowserNode guide = rows.byPath.value(QStringLiteral("Root/Guides/3DSketch1")).node;
  CHECK(guide.type == BrowserNode::Type::Sketch && guide.uid == QStringLiteral("F1") && guide.hasEye);
  CHECK(guide.constructionSet == QStringLiteral("Guides") && !guide.isSetMember());
  const BrowserNode plane = rows.byPath.value(QStringLiteral("Root/Guides/Plane1")).node;
  CHECK(plane.type == BrowserNode::Type::Datum && plane.featureType == QStringLiteral("construction_plane"));
  CHECK(plane.constructionSet == QStringLiteral("Guides"));
  const BrowserNode skin = rows.byPath.value(QStringLiteral("Root/Guides/Skin")).node;
  CHECK(skin.type == BrowserNode::Type::Body && skin.uid == QStringLiteral("F4.b0"));
  CHECK(skin.constructionSet == QStringLiteral("Guides") && skin.item().kind == SelectKind::Body);
  CHECK(rows.byPath.value(QStringLiteral("Root/Bodies/Block")).node.constructionSet.isEmpty());
  // The active set's name in bold.
  auto* tree = panel.findChild<QTreeWidget*>();
  CHECK(tree != nullptr);
  if (tree != nullptr) {
    const Qt::MatchFlags anywhere = Qt::MatchExactly | Qt::MatchRecursive;
    const QList<QTreeWidgetItem*> active = tree->findItems(QStringLiteral("Guides"), anywhere, 0);
    const QList<QTreeWidgetItem*> other = tree->findItems(QStringLiteral("Skins"), anywhere, 0);
    CHECK(active.size() == 1 && active.first()->font(0).bold());
    CHECK(other.size() == 1 && !other.first()->font(0).bold());
  }

  // Deactivated: no set in bold, the rows stay in their sets.
  QJsonArray inactive = sets();
  QJsonObject first = inactive.at(0).toObject();
  first.insert(QStringLiteral("active"), false);
  inactive.replace(0, first);
  snapshot.constructionSets = inactive;
  panel.rebuild(snapshot, false);
  const Rows after = rowsOf(panel);
  CHECK(!after.byPath.value(QStringLiteral("Root/Guides")).node.active);
  CHECK(after.children.value(QStringLiteral("Root/Guides")).size() == 4);
}

void hiddenFolderStays() {
  // Every construction feature in a set and the Construction folder's eye
  // off: the folder stays, empty, so that its eye can be turned on again.
  DocumentSnapshot snapshot = design();
  snapshot.constructionSets = sets();
  snapshot.hiddenFolders.insert(QStringLiteral("C0"), {DocumentSnapshot::Folder::Construction});
  BrowserPanel panel;
  panel.rebuild(snapshot, false);
  const Rows rows = rowsOf(panel);
  CHECK(rows.rootRows.contains(QStringLiteral("Construction")));
  CHECK(rows.children.value(QStringLiteral("Root/Construction")).isEmpty());
  CHECK(!rows.byPath.value(QStringLiteral("Root/Construction")).node.visible);
}

} // namespace

int browserConstructionSetsTests() {
  failures = 0;
  withoutSets();
  withSets();
  hiddenFolderStays();
  return failures;
}
MITCAD_UNIT_TEST(browserConstructionSetsTests);
