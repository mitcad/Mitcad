// SPDX-License-Identifier: MIT
// Folders of the Browser's component and body rows (browser/BrowserOrder,
// mitcad#2103): a component's occurrences by folder, New Folder's proposed
// name, the folder commands of the row menus (only the rows that change),
// and where dragged rows may go (BrowserPanel::dropFor): into a folder of
// their parent's, out of their folders onto their parent's row or the
// Bodies folder, never onto another component's folder. The order among
// siblings (mitcad#2104): Move Up and Move Down within a row's group and
// at its ends, the commands of a move (a folder's rows kept together where
// its first one is), and rows dropped before or after a sibling.
#include "UnitTests.hpp"
#include "browser/BrowserOrder.hpp"
#include "browser/BrowserPanel.hpp"

#include <cstdio>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_order.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

DocumentSnapshot::Occurrence occurrence(const QString& path, const QString& name, const QString& folder) {
  DocumentSnapshot::Occurrence made;
  made.uid = path.section(QLatin1Char('/'), -1);
  made.path = path;
  made.name = name;
  made.folder = folder;
  return made;
}

// Rail:1 (O1) and Panel:1 (O3) in Frame, Post:1 (O2) in none, Brace:1 (O4)
// in Braces; Pin:1 (O1/O5) placed in Rail:1, in Pins.
DocumentSnapshot design() {
  DocumentSnapshot snapshot;
  snapshot.occurrences = {occurrence(QStringLiteral("O1"), QStringLiteral("Rail:1"), QStringLiteral("Frame")),
                          occurrence(QStringLiteral("O2"), QStringLiteral("Post:1"), QString()),
                          occurrence(QStringLiteral("O3"), QStringLiteral("Panel:1"), QStringLiteral("Frame")),
                          occurrence(QStringLiteral("O4"), QStringLiteral("Brace:1"), QStringLiteral("Braces"))};
  snapshot.occurrences[0].children = {
      occurrence(QStringLiteral("O1/O5"), QStringLiteral("Pin:1"), QStringLiteral("Pins"))};
  return snapshot;
}

void siblingTests() {
  const DocumentSnapshot snapshot = design();
  CHECK(browser_order::parentPath(QStringLiteral("O1/O5")) == QStringLiteral("O1"));
  CHECK(browser_order::parentPath(QStringLiteral("O1")).isEmpty());
  const QVector<browser_order::Sibling> root = browser_order::siblingsIn(snapshot, QString());
  CHECK(root.size() == 4);
  CHECK(browser_order::folderOrder(root) == QStringList({QStringLiteral("Frame"), QStringLiteral("Braces")}));
  CHECK(browser_order::groupOf(root, QStringLiteral("Frame")) ==
        QStringList({QStringLiteral("O1"), QStringLiteral("O3")}));
  CHECK(browser_order::groupOf(root, QString()) == QStringList({QStringLiteral("O2")}));
  const QVector<browser_order::Sibling> inRail = browser_order::siblingsIn(snapshot, QStringLiteral("O1"));
  CHECK(inRail.size() == 1 && inRail.first().name == QStringLiteral("Pin:1"));
  CHECK(browser_order::siblingsIn(snapshot, QStringLiteral("O9")).isEmpty());

  // New Folder's name: the first FolderN not taken.
  CHECK(browser_order::newFolderName({}) == QStringLiteral("Folder1"));
  CHECK(browser_order::newFolderName({QStringLiteral("Folder1"), QStringLiteral("Folder3")}) ==
        QStringLiteral("Folder2"));
}

void folderCommandTests() {
  const QVector<browser_order::Sibling> root = browser_order::siblingsIn(design(), QString());
  // Into Frame: Post:1 only (Rail:1 is in it already).
  QJsonArray commands = browser_order::occurrenceFolderCommands(
      root, {QStringLiteral("O1"), QStringLiteral("O2")}, QStringLiteral("Frame"));
  CHECK(commands.size() == 1);
  CHECK(commands.at(0).toObject() ==
        QJsonObject({{QStringLiteral("cmd"), QStringLiteral("set_occurrence_folder")},
                     {QStringLiteral("occurrence"), QStringLiteral("O2")},
                     {QStringLiteral("folder"), QStringLiteral("Frame")}}));
  // Out of their folders: the two in one, without a folder field.
  commands = browser_order::occurrenceFolderCommands(root, {QStringLiteral("O2"), QStringLiteral("O3"),
                                                            QStringLiteral("O4")},
                                                     QString());
  CHECK(commands.size() == 2);
  CHECK(!commands.at(0).toObject().contains(QStringLiteral("folder")));
  CHECK(commands.at(1).toObject().value(QStringLiteral("occurrence")).toString() == QStringLiteral("O4"));

  // Bodies: one already in Glass, one unknown, one moved.
  const QHash<QString, QString> folders{{QStringLiteral("F1.b0"), QStringLiteral("Glass")},
                                        {QStringLiteral("F2.b0"), QString()}};
  commands = browser_order::bodyFolderCommands(
      folders, {QStringLiteral("F1.b0"), QStringLiteral("F2.b0"), QStringLiteral("F9.b0")}, QStringLiteral("Glass"));
  CHECK(commands.size() == 1);
  CHECK(commands.at(0).toObject().value(QStringLiteral("uid")).toString() == QStringLiteral("F2.b0"));
  CHECK(commands.at(0).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("set_body_folder"));
}

BrowserNode row(BrowserNode::Type type, const QString& uid, const QString& occurrence, const QString& component,
                const QString& folder = QString()) {
  BrowserNode node;
  node.type = type;
  node.uid = uid;
  node.name = uid;
  node.occurrence = occurrence;
  node.component = component;
  node.folder = folder;
  return node;
}

void dropTests() {
  using Type = BrowserNode::Type;
  const BrowserNode rail = row(Type::Component, QStringLiteral("C1"), QStringLiteral("O1"), QStringLiteral("C1"),
                               QStringLiteral("Frame"));
  const BrowserNode post = row(Type::Component, QStringLiteral("C2"), QStringLiteral("O2"), QStringLiteral("C2"));
  const BrowserNode pin = row(Type::Component, QStringLiteral("C5"), QStringLiteral("O1/O5"), QStringLiteral("C5"));
  const BrowserNode root = row(Type::Component, QStringLiteral("C0"), QString(), QStringLiteral("C0"));
  const BrowserNode frame = row(Type::ComponentFolder, QStringLiteral("Frame"), QString(), QStringLiteral("C0"));
  const BrowserNode braces = row(Type::ComponentFolder, QStringLiteral("Braces"), QString(), QStringLiteral("C0"));
  const BrowserNode pins =
      row(Type::ComponentFolder, QStringLiteral("Pins"), QStringLiteral("O1"), QStringLiteral("C1"));
  const BrowserNode bodiesOfRail = row(Type::Bodies, QString(), QStringLiteral("O1"), QStringLiteral("C1"));
  BrowserDrop place = BrowserDrop::After;

  // Into a folder of their parent's: allowed while one is not in it.
  CHECK(BrowserPanel::dropFor({post}, frame, false, &place) && place == BrowserDrop::Onto);
  CHECK(BrowserPanel::dropFor({rail, post}, braces, true, &place));
  CHECK(!BrowserPanel::dropFor({rail}, frame, false, &place));
  // Out of their folders onto their parent's row; nothing to take out.
  CHECK(BrowserPanel::dropFor({rail, post}, root, false, &place) && place == BrowserDrop::Onto);
  CHECK(!BrowserPanel::dropFor({post}, root, false, &place));
  // Another parent's folder, rows of two parents, another component's
  // folder of its own (Rail:1's Bodies), rows of two kinds: refused.
  CHECK(!BrowserPanel::dropFor({post}, pins, false, &place));
  CHECK(!BrowserPanel::dropFor({post, pin}, frame, false, &place));
  CHECK(!BrowserPanel::dropFor({post}, bodiesOfRail, false, &place));
  const BrowserNode pane = row(Type::Body, QStringLiteral("F1.b0"), QString(), QStringLiteral("C0"));
  CHECK(!BrowserPanel::dropFor({post, pane}, frame, false, &place));

  // Bodies: into a folder of their Bodies folder, out of it onto the
  // Bodies folder; not into another component's.
  const BrowserNode glass = row(Type::BodyFolder, QStringLiteral("Glass"), QString(), QStringLiteral("C0"));
  const BrowserNode bodies = row(Type::Bodies, QString(), QString(), QStringLiteral("C0"));
  const BrowserNode sash = row(Type::Body, QStringLiteral("F3.b0"), QString(), QStringLiteral("C0"),
                               QStringLiteral("Glass"));
  CHECK(BrowserPanel::dropFor({pane, sash}, glass, false, &place));
  CHECK(!BrowserPanel::dropFor({sash}, glass, false, &place));
  CHECK(BrowserPanel::dropFor({sash}, bodies, false, &place));
  CHECK(!BrowserPanel::dropFor({pane}, bodies, false, &place));
  CHECK(!BrowserPanel::dropFor({pane}, bodiesOfRail, false, &place));
  const BrowserNode railGlass =
      row(Type::BodyFolder, QStringLiteral("Glass"), QStringLiteral("O1"), QStringLiteral("C1"));
  CHECK(!BrowserPanel::dropFor({pane}, railGlass, false, &place));
  CHECK(!BrowserPanel::dropFor({}, glass, false, &place));
}

// The order among siblings (mitcad#2104): A (O1), C (O3) and H (O6) in no
// folder; B (O2) and D (O4) in F, E (O5) in G. The Browser lists F's B, D,
// G's E, then A, C, H.
QVector<browser_order::Sibling> orderSiblings() {
  return {{QStringLiteral("O1"), QStringLiteral("A:1"), QString()},
          {QStringLiteral("O2"), QStringLiteral("B:1"), QStringLiteral("F")},
          {QStringLiteral("O3"), QStringLiteral("C:1"), QString()},
          {QStringLiteral("O4"), QStringLiteral("D:1"), QStringLiteral("F")},
          {QStringLiteral("O5"), QStringLiteral("E:1"), QStringLiteral("G")},
          {QStringLiteral("O6"), QStringLiteral("H:1"), QString()}};
}

QJsonObject reorder(const QString& occurrence, int index, const QStringList& with = {}) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("reorder_occurrence")},
                      {QStringLiteral("occurrence"), occurrence},
                      {QStringLiteral("index"), index}};
  if (!with.isEmpty()) {
    command.insert(QStringLiteral("with"), QJsonArray::fromStringList(with));
  }
  return command;
}

void orderTests() {
  const QVector<browser_order::Sibling> siblings = orderSiblings();
  // Move Up and Move Down within the row's group; none at its ends (the
  // entries are disabled there).
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O1"), -1).isEmpty());
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O1"), 1) == QStringLiteral("O3"));
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O6"), 1).isEmpty());
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O4"), -1) == QStringLiteral("O2"));
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O4"), 1).isEmpty());
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O5"), -1).isEmpty());
  CHECK(browser_order::neighbor(siblings, QStringLiteral("O9"), 1).isEmpty());

  // H before A among the rows in no folder: H first, the rest in order.
  QJsonArray commands =
      browser_order::moveCommands(siblings, {QStringLiteral("O6")}, QString(), QStringLiteral("O1"), false);
  CHECK(commands == QJsonArray({reorder(QStringLiteral("O6"), 0)}));
  // A after C: after C in the model's order.
  commands = browser_order::moveCommands(siblings, {QStringLiteral("O1")}, QString(), QStringLiteral("O3"), true);
  CHECK(commands == QJsonArray({reorder(QStringLiteral("O1"), 2)}));
  // Move Down of B in F: F's rows together where B is (the folder keeps its
  // place before G), D first.
  commands = browser_order::moveCommands(siblings, {QStringLiteral("O2")}, QStringLiteral("F"),
                                         QStringLiteral("O4"), true);
  CHECK(commands == QJsonArray({reorder(QStringLiteral("O4"), 1, {QStringLiteral("O2")})}));
  // C dropped after B: into F, between B and D, one step with its folder.
  commands = browser_order::moveCommands(siblings, {QStringLiteral("O3")}, QStringLiteral("F"),
                                         QStringLiteral("O2"), true);
  CHECK(commands.size() == 2);
  CHECK(commands.at(0).toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("set_occurrence_folder"));
  CHECK(commands.at(1).toObject() ==
        reorder(QStringLiteral("O2"), 1, {QStringLiteral("O3"), QStringLiteral("O4")}));
  // C and H (in that order) before A: two moved at once.
  commands = browser_order::moveCommands(siblings, {QStringLiteral("O3"), QStringLiteral("O6")}, QString(),
                                         QStringLiteral("O1"), false);
  CHECK(commands == QJsonArray({reorder(QStringLiteral("O3"), 0, {QStringLiteral("O6")})}));
  // Nothing the Browser would show differently: C after A, which it is;
  // B before D, which it is; a row onto itself.
  CHECK(browser_order::moveCommands(siblings, {QStringLiteral("O3")}, QString(), QStringLiteral("O1"), true).isEmpty());
  CHECK(browser_order::moveCommands(siblings, {QStringLiteral("O2")}, QStringLiteral("F"), QStringLiteral("O4"), false)
            .isEmpty());
  CHECK(browser_order::moveCommands(siblings, {QStringLiteral("O3")}, QString(), QStringLiteral("O3"), false)
            .isEmpty());
  // An anchor of another group: nothing.
  CHECK(browser_order::moveCommands(siblings, {QStringLiteral("O1")}, QString(), QStringLiteral("O2"), false)
            .isEmpty());

  // Where component rows dropped on a sibling's row go: before it in the
  // upper half, after it in the lower; never onto one of themselves.
  using Type = BrowserNode::Type;
  const BrowserNode a = row(Type::Component, QStringLiteral("C1"), QStringLiteral("O1"), QStringLiteral("C1"));
  const BrowserNode c = row(Type::Component, QStringLiteral("C3"), QStringLiteral("O3"), QStringLiteral("C3"));
  const BrowserNode b = row(Type::Component, QStringLiteral("C2"), QStringLiteral("O2"), QStringLiteral("C2"),
                            QStringLiteral("F"));
  BrowserDrop place = BrowserDrop::Onto;
  CHECK(BrowserPanel::dropFor({c}, a, false, &place) && place == BrowserDrop::Before);
  CHECK(BrowserPanel::dropFor({c}, a, true, &place) && place == BrowserDrop::After);
  CHECK(BrowserPanel::dropFor({a, c}, b, true, &place) && place == BrowserDrop::After);
  CHECK(!BrowserPanel::dropFor({a, c}, c, false, &place));
  const BrowserNode pin = row(Type::Component, QStringLiteral("C5"), QStringLiteral("O1/O5"), QStringLiteral("C5"));
  CHECK(!BrowserPanel::dropFor({c}, pin, false, &place));
}

} // namespace

int browserOrderTests() {
  siblingTests();
  folderCommandTests();
  dropTests();
  orderTests();
  return failures;
}
MITCAD_UNIT_TEST(browserOrderTests);
