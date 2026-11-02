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

} // namespace

int browserRolesTests() {
  rowLookTests();
  return failures;
}
