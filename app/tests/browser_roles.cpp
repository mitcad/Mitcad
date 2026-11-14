// SPDX-License-Identifier: MIT
// The browser rows' item data roles (mitcad#1143): a BrowserPanel built
// from a snapshot, each row's look as its name delegate gives it
// (initStyleOption). A shown transparent body keeps the plain text colour
// (its transparency once read as a colour role, the name in error red) and
// has the half-open eye; a visible body inside a hidden occurrence has the
// open eye (its grey name role once read as transparent).
#include "browser/BrowserPanel.hpp"

#include <cstdio>

#include <QAbstractItemModel>
#include <QApplication>
#include <QColor>
#include <QIcon>
#include <QModelIndex>
#include <QPalette>
#include <QSize>
#include <QStyleOptionViewItem>
#include <QStyledItemDelegate>
#include <QTreeWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_roles.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// The delegates' protected initStyleOption, named through a class that
// makes it public; called on the browser's delegate, it is that delegate's
// own (virtual).
struct StyleOptionAccess : QStyledItemDelegate {
  using QStyledItemDelegate::initStyleOption;
};
using InitStyleOption = void (QStyledItemDelegate::*)(QStyleOptionViewItem*, const QModelIndex&) const;

// A row's look: its name's colour and its light bulb's icon name (eye,
// eye-off, eye-half; empty without a bulb).
struct Look {
  QColor text;
  QString bulb;
};

Look look(QTreeWidget* tree, const QModelIndex& index) {
  Look found;
  const auto* delegate = qobject_cast<const QStyledItemDelegate*>(tree->itemDelegate());
  CHECK(delegate != nullptr);
  if (delegate == nullptr || !index.isValid()) {
    return found;
  }
  QStyleOptionViewItem option;
  option.initFrom(tree);
  option.widget = tree;
  option.decorationSize = QSize(16, 16);
  const InitStyleOption initStyleOption = &StyleOptionAccess::initStyleOption;
  (delegate->*initStyleOption)(&option, index);
  found.text = option.palette.color(QPalette::Text);
  found.bulb = option.icon.name();
  return found;
}

// The first row with this name, anywhere in the tree.
QModelIndex rowNamed(const QTreeWidget* tree, const QString& name) {
  const QAbstractItemModel* model = tree->model();
  const QModelIndexList found =
      model->match(model->index(0, 0), Qt::DisplayRole, name, 1, Qt::MatchExactly | Qt::MatchRecursive);
  return found.isEmpty() ? QModelIndex() : found.first();
}

DocumentSnapshot::Component component(const QString& uid, const QString& name, const QString& bodyUid) {
  DocumentSnapshot::Component made;
  made.uid = uid;
  made.name = name;
  made.bodies << bodyUid;
  return made;
}

DocumentSnapshot::Body body(const QString& uid, const QString& name, const QString& owner, bool transparent) {
  DocumentSnapshot::Body made;
  made.uid = uid;
  made.name = name;
  made.component = owner;
  made.visible = true;
  made.transparent = transparent;
  return made;
}

// Root (C0) with Glass (F1.b0), shown and transparent; Bracket:1 (O1, of
// C1), hidden, with Block (F2.b0), visible and opaque.
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  snapshot.components.append(component(QStringLiteral("C0"), QStringLiteral("Root"), QStringLiteral("F1.b0")));
  snapshot.components.append(component(QStringLiteral("C1"), QStringLiteral("Bracket"), QStringLiteral("F2.b0")));
  snapshot.bodies.insert(QStringLiteral("F1.b0"),
                         body(QStringLiteral("F1.b0"), QStringLiteral("Glass"), QStringLiteral("C0"), true));
  snapshot.bodies.insert(QStringLiteral("F2.b0"),
                         body(QStringLiteral("F2.b0"), QStringLiteral("Block"), QStringLiteral("C1"), false));
  DocumentSnapshot::Occurrence bracket;
  bracket.uid = QStringLiteral("O1");
  bracket.name = QStringLiteral("Bracket:1");
  bracket.path = QStringLiteral("O1");
  bracket.component = QStringLiteral("C1");
  bracket.visible = false;
  snapshot.occurrences.append(bracket);
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

void rowLookTests() {
  BrowserPanel panel;
  panel.rebuild(design(), false);
  auto* tree = panel.findChild<QTreeWidget*>();
  CHECK(tree != nullptr);
  if (tree == nullptr) {
    return;
  }
  const QColor plain = tree->palette().color(QPalette::Text);

  // (a) A shown transparent body: the plain text colour, the half-open eye.
  const QModelIndex glass = rowNamed(tree, QStringLiteral("Glass"));
  CHECK(glass.isValid());
  const Look transparent = look(tree, glass);
  CHECK(transparent.text == plain);
  CHECK(transparent.bulb == QStringLiteral("eye-half"));

  // (b) A visible body inside a hidden occurrence: greyed, the open eye.
  const QModelIndex block = rowNamed(tree, QStringLiteral("Block"));
  CHECK(block.isValid());
  const Look greyed = look(tree, block);
  CHECK(greyed.bulb == QStringLiteral("eye"));
  CHECK(greyed.text != plain);

  // The hidden occurrence's own row: the closed eye.
  const QModelIndex occurrence = rowNamed(tree, QStringLiteral("Bracket:1"));
  CHECK(occurrence.isValid());
  CHECK(look(tree, occurrence).bulb == QStringLiteral("eye-off"));
}

DocumentSnapshot::Feature timelineFeature(const QString& uid, const QString& name, const QString& type, int index,
                                   const QStringList& sketches = {}) {
  DocumentSnapshot::Feature made;
  made.uid = uid;
  made.name = name;
  made.type = type;
  made.status = QStringLiteral("ok");
  made.component = QStringLiteral("C0");
  made.index = index;
  made.sketches = sketches;
  return made;
}

// Show used sketches under their features (mitcad#1699): Sketch1 (F1) used
// by Extrude1 (F2) and Extrude2 (F3), Sketch2 (F4) by none, Sketch3 (F5) by
// Extrude3 (F6) alone; the bodies F2.b0, F3.b0, F6.b0 in the root.
DocumentSnapshot usedSketchesDesign() {
  DocumentSnapshot snapshot;
  DocumentSnapshot::Component root = component(QStringLiteral("C0"), QStringLiteral("Root"), QStringLiteral("F2.b0"));
  root.bodies << QStringLiteral("F3.b0") << QStringLiteral("F6.b0");
  snapshot.components.append(root);
  for (const char* uid : {"F2.b0", "F3.b0", "F6.b0"}) {
    const QString made = QString::fromLatin1(uid);
    snapshot.bodies.insert(made, body(made, QStringLiteral("Body ") + made, QStringLiteral("C0"), false));
  }
  const QString f1 = QStringLiteral("F1");
  const QString f5 = QStringLiteral("F5");
  const QString sketch = QStringLiteral("sketch");
  const QString extrude = QStringLiteral("extrude");
  snapshot.features.append(timelineFeature(f1, QStringLiteral("Sketch1"), sketch, 0));
  snapshot.features.append(timelineFeature(QStringLiteral("F2"), QStringLiteral("Extrude1"), extrude, 1, {f1}));
  snapshot.features.append(timelineFeature(QStringLiteral("F3"), QStringLiteral("Extrude2"), extrude, 2, {f1}));
  snapshot.features.append(timelineFeature(QStringLiteral("F4"), QStringLiteral("Sketch2"), sketch, 3));
  snapshot.features.append(timelineFeature(f5, QStringLiteral("Sketch3"), sketch, 4));
  snapshot.features.append(timelineFeature(QStringLiteral("F6"), QStringLiteral("Extrude3"), extrude, 5, {f5}));
  snapshot.marker = 6;
  snapshot.activeComponent = QStringLiteral("C0");
  return snapshot;
}

// The rows' paths ("Root/Features/Extrude1/Sketch1"), from the panel's
// presentation rows, and the rows by path.
QHash<QString, BrowserPanel::PresentationRow> rowsByPath(const BrowserPanel& panel) {
  QHash<QString, BrowserPanel::PresentationRow> rows;
  for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
    rows.insert(row.node.path, row);
  }
  return rows;
}

void usedSketchesTests() {
  const DocumentSnapshot snapshot = usedSketchesDesign();
  BrowserPanel panel;

  // Off (the default): every sketch in Sketches, the features under them
  // (mitcad#177), no Features folder.
  CHECK(!panel.consumeSketches());
  panel.rebuild(snapshot, false);
  QHash<QString, BrowserPanel::PresentationRow> rows = rowsByPath(panel);
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch1")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch2")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch3")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch1/Extrude2")));
  CHECK(!rows.contains(QStringLiteral("Root/Features")));

  // The users of each sketch: Sketch1 two, Sketch3 one, Sketch2 none.
  const QHash<QString, int> users = BrowserPanel::sketchUsers(snapshot, QStringLiteral("C0"));
  CHECK(users.value(QStringLiteral("F1")) == 2);
  CHECK(users.value(QStringLiteral("F5")) == 1);
  CHECK(!users.contains(QStringLiteral("F4")));

  // On: the Features folder; Sketch1 under Extrude1 and Extrude2, Sketch3
  // under Extrude3; Sketch2 alone left in Sketches.
  panel.setConsumeSketches(true);
  CHECK(panel.consumeSketches());
  panel.rebuild(snapshot, false);
  rows = rowsByPath(panel);
  CHECK(rows.contains(QStringLiteral("Root/Features")));
  CHECK(rows.value(QStringLiteral("Root/Features")).node.type == BrowserNode::Type::Features);
  for (const char* path : {"Root/Features/Extrude1", "Root/Features/Extrude2", "Root/Features/Extrude3"}) {
    CHECK(rows.value(QString::fromLatin1(path)).node.type == BrowserNode::Type::Feature);
  }
  const BrowserPanel::PresentationRow first = rows.value(QStringLiteral("Root/Features/Extrude1/Sketch1"));
  const BrowserPanel::PresentationRow second = rows.value(QStringLiteral("Root/Features/Extrude2/Sketch1"));
  const BrowserPanel::PresentationRow only = rows.value(QStringLiteral("Root/Features/Extrude3/Sketch3"));
  CHECK(first.node.type == BrowserNode::Type::Sketch && first.node.uid == QStringLiteral("F1"));
  CHECK(second.node.type == BrowserNode::Type::Sketch && second.node.uid == QStringLiteral("F1"));
  CHECK(first.node.feature == QStringLiteral("F2"));
  CHECK(second.node.feature == QStringLiteral("F3"));
  // Two rows of one sketch, told apart by their keys; the same item.
  CHECK(!first.key.isEmpty() && first.key != second.key);
  CHECK(first.node.item() == second.node.item());
  CHECK(only.node.type == BrowserNode::Type::Sketch && only.node.feature == QStringLiteral("F6"));
  // The shared mark's tooltip on both of Sketch1's rows, none on Sketch3's.
  CHECK(first.tooltip.contains(QStringLiteral("Used by 2 features")));
  CHECK(second.tooltip.contains(QStringLiteral("Used by 2 features")));
  CHECK(!only.tooltip.contains(QStringLiteral("Used by")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch2")));
  CHECK(!rows.contains(QStringLiteral("Root/Sketches/Sketch1")));
  CHECK(!rows.contains(QStringLiteral("Root/Sketches/Sketch3")));
  // No feature rows under the sketches while on.
  CHECK(!rows.contains(QStringLiteral("Root/Sketches/Sketch2/Extrude1")));

  // Off again: as before.
  panel.setConsumeSketches(false);
  panel.rebuild(snapshot, false);
  rows = rowsByPath(panel);
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch1")));
  CHECK(rows.contains(QStringLiteral("Root/Sketches/Sketch3")));
  CHECK(!rows.contains(QStringLiteral("Root/Features")));
}

// Isolate (mitcad#1700): the eyes show what the view shows. Bracket:1
// shown and isolated, Glass in the root left out: Glass's eye and its
// Bodies folder's are the dimmed closed eye (only the isolation hides them),
// Bracket:1 and its Block keep the open eye. Glass hidden by its own light
// bulb as well: the plain closed eye. A body isolated inside Bracket:1:
// Bracket:1 holds it and keeps its eye.
void isolationEyeTests() {
  DocumentSnapshot snapshot = design();
  snapshot.occurrences[0].visible = true;
  snapshot.bodies[QStringLiteral("F1.b0")].transparent = false;
  SelectionItem isolated;
  isolated.kind = SelectKind::Component;
  isolated.occurrence = QStringLiteral("O1");
  snapshot.isolation << isolated;

  CHECK(!DocumentSnapshot::isolationShows(snapshot.isolation, QStringLiteral("F1.b0"), QString()));
  CHECK(DocumentSnapshot::isolationShows(snapshot.isolation, QStringLiteral("F2.b0"), QStringLiteral("O1")));
  CHECK(DocumentSnapshot::isolationShows(snapshot.isolation, QStringLiteral("F2.b0"), QStringLiteral("O1/O2")));
  CHECK(!DocumentSnapshot::isolationShows(snapshot.isolation, QStringLiteral("F2.b0"), QStringLiteral("O10")));
  CHECK(DocumentSnapshot::isolationReaches(snapshot.isolation, QStringLiteral("O1")));
  CHECK(!DocumentSnapshot::isolationReaches(snapshot.isolation, QStringLiteral("O2")));
  CHECK(DocumentSnapshot::isolationShows(Selection(), QStringLiteral("F1.b0"), QString()));

  {
    BrowserPanel panel;
    panel.rebuild(snapshot, false);
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree == nullptr) {
      return;
    }
    const QColor plain = tree->palette().color(QPalette::Text);
    const Look glass = look(tree, rowNamed(tree, QStringLiteral("Glass")));
    CHECK(glass.bulb == QStringLiteral("eye-isolated"));
    CHECK(glass.text != plain);
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Bracket:1"))).bulb == QStringLiteral("eye"));
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Block"))).bulb == QStringLiteral("eye"));
    // The root's Bodies folder is the first row named Bodies.
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Bodies"))).bulb == QStringLiteral("eye-isolated"));
    // Other presentations get the eye as shown (the AppKit rows).
    for (const BrowserPanel::PresentationRow& row : panel.presentationRows()) {
      if (row.node.name == QStringLiteral("Glass")) {
        CHECK(row.node.isolatedOut);
        CHECK(row.node.visible);
        CHECK(!row.eyeOn);
      } else if (row.node.name == QStringLiteral("Block")) {
        CHECK(!row.node.isolatedOut);
        CHECK(row.eyeOn);
      }
    }
  }

  {
    DocumentSnapshot hidden = snapshot;
    hidden.bodies[QStringLiteral("F1.b0")].visible = false;
    BrowserPanel panel;
    panel.rebuild(hidden, false);
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree != nullptr) {
      CHECK(look(tree, rowNamed(tree, QStringLiteral("Glass"))).bulb == QStringLiteral("eye-off"));
    }
  }

  {
    DocumentSnapshot inner = snapshot;
    SelectionItem block;
    block.kind = SelectKind::Body;
    block.owner = QStringLiteral("F2.b0");
    block.occurrence = QStringLiteral("O1");
    inner.isolation = {block};
    CHECK(DocumentSnapshot::isolationReaches(inner.isolation, QStringLiteral("O1")));
    CHECK(!DocumentSnapshot::isolationShows(inner.isolation, QStringLiteral("F1.b0"), QString()));
    BrowserPanel panel;
    panel.rebuild(inner, false);
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree != nullptr) {
      CHECK(look(tree, rowNamed(tree, QStringLiteral("Bracket:1"))).bulb == QStringLiteral("eye"));
      CHECK(look(tree, rowNamed(tree, QStringLiteral("Block"))).bulb == QStringLiteral("eye"));
      CHECK(look(tree, rowNamed(tree, QStringLiteral("Glass"))).bulb == QStringLiteral("eye-isolated"));
    }
  }
}

// The folders' own eyes (mitcad#1771): the root's Sketches and Bodies
// folders hidden. Their eyes are closed; the rows in them are greyed with
// their own eyes as they are: Sketch1 and Sketch3 open, Sketch2 closed,
// the bodies open. Shown again, the rows that are on are not greyed.
void folderEyeTests() {
  DocumentSnapshot snapshot = usedSketchesDesign();
  snapshot.featureShown.insert(QStringLiteral("F1"), true);
  snapshot.featureShown.insert(QStringLiteral("F4"), false);
  snapshot.featureShown.insert(QStringLiteral("F5"), true);
  snapshot.hiddenFolders.insert(QStringLiteral("C0"),
                                {DocumentSnapshot::Folder::Bodies, DocumentSnapshot::Folder::Sketches});
  CHECK(!snapshot.folderShown(QStringLiteral("C0"), DocumentSnapshot::Folder::Sketches));
  CHECK(snapshot.folderShown(QStringLiteral("C0"), DocumentSnapshot::Folder::Construction));
  CHECK(snapshot.folderShown(QStringLiteral("C1"), DocumentSnapshot::Folder::Sketches));
  CHECK(snapshot.kindHidden(QStringLiteral("F1")));
  CHECK(!snapshot.isDrawn(QStringLiteral("F1")));
  CHECK(!snapshot.kindHidden(QStringLiteral("F2"))); // an extrude has no folder eye
  CHECK(DocumentSnapshot::folderKey(DocumentSnapshot::Folder::Construction) == QStringLiteral("construction"));
  {
    BrowserPanel panel;
    panel.rebuild(snapshot, false);
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree == nullptr) {
      return;
    }
    const QColor plain = tree->palette().color(QPalette::Text);
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketches"))).bulb == QStringLiteral("eye-off"));
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Bodies"))).bulb == QStringLiteral("eye-off"));
    const Look sketch1 = look(tree, rowNamed(tree, QStringLiteral("Sketch1")));
    CHECK(sketch1.bulb == QStringLiteral("eye"));
    CHECK(sketch1.text != plain);
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketch2"))).bulb == QStringLiteral("eye-off"));
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketch3"))).bulb == QStringLiteral("eye"));
    const Look body = look(tree, rowNamed(tree, QStringLiteral("Body F2.b0")));
    CHECK(body.bulb == QStringLiteral("eye"));
    CHECK(body.text != plain);
  }
  {
    snapshot.hiddenFolders.clear();
    CHECK(snapshot.isDrawn(QStringLiteral("F1")));
    BrowserPanel panel;
    panel.rebuild(snapshot, false);
    auto* tree = panel.findChild<QTreeWidget*>();
    CHECK(tree != nullptr);
    if (tree == nullptr) {
      return;
    }
    const QColor plain = tree->palette().color(QPalette::Text);
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketches"))).bulb == QStringLiteral("eye"));
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketch1"))).text == plain);
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Sketch2"))).bulb == QStringLiteral("eye-off"));
    CHECK(look(tree, rowNamed(tree, QStringLiteral("Body F2.b0"))).text == plain);
  }
}

} // namespace

int browserRolesTests() {
  rowLookTests();
  usedSketchesTests();
  isolationEyeTests();
  folderEyeTests();
  return failures;
}
