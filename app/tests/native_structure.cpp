// SPDX-License-Identifier: MIT
// The widget tree whose change rebuilds a presenter's native controls
// (mitcad#986): an item view's or a scroll area's own children (header,
// viewport, scroll bar containers) changing places is no change; a widget
// added or removed anywhere, or children reordered in any other container
// (also inside a scroll area's contents), is one.
#include "platform/NativeStructure.hpp"

#include <cstdio>

#include <QLabel>
#include <QList>
#include <QScrollArea>
#include <QTreeWidget>
#include <QWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "native_structure.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::nativeStructureOrder;

QList<QWidget*> order(const QWidget& root) { return nativeStructureOrder(&root, root.findChildren<QWidget*>()); }

// The first of `parent`'s own widget children, which raise() moves last.
QWidget* firstWidgetChild(const QWidget* parent) {
  for (QObject* child : parent->children()) {
    if (auto* widget = qobject_cast<QWidget*>(child)) { return widget; }
  }
  return nullptr;
}

} // namespace

int nativeStructureTests() {
  failures = 0;
  QWidget root;
  auto* tree = new QTreeWidget(&root);
  tree->setHeaderLabels({QStringLiteral("Name"), QStringLiteral("Value")});
  // A container without a layout presents its children in child order.
  auto* plain = new QWidget(&root);
  auto* first = new QLabel(QStringLiteral("first"), plain);
  auto* second = new QLabel(QStringLiteral("second"), plain);
  auto* area = new QScrollArea(&root);
  auto* contents = new QWidget;
  auto* third = new QLabel(QStringLiteral("third"), contents);
  auto* fourth = new QLabel(QStringLiteral("fourth"), contents);
  area->setWidget(contents);

  const QList<QWidget*> listed = root.findChildren<QWidget*>();
  const QList<QWidget*> baseline = order(root);
  CHECK(baseline.size() == listed.size());
  for (QWidget* widget : listed) { CHECK(baseline.count(widget) == 1); }
  CHECK(baseline.indexOf(first) < baseline.indexOf(second));
  CHECK(baseline.indexOf(third) < baseline.indexOf(fourth));
  CHECK(baseline.indexOf(contents) < baseline.indexOf(third));

  // The item view's header, viewport and scroll bar containers change
  // places, as Qt's layout of a QTreeWidget does: the same tree.
  QWidget* internal = firstWidgetChild(tree);
  CHECK(internal != nullptr);
  if (internal != nullptr) { internal->raise(); }
  CHECK(root.findChildren<QWidget*>() != listed);
  CHECK(order(root) == baseline);
  // Likewise a scroll area's viewport, which holds its contents.
  const QList<QWidget*> beforeArea = root.findChildren<QWidget*>();
  QWidget* areaInternal = firstWidgetChild(area);
  CHECK(areaInternal != nullptr);
  if (areaInternal != nullptr) { areaInternal->raise(); }
  CHECK(root.findChildren<QWidget*>() != beforeArea);
  CHECK(order(root) == baseline);

  // Children reordered elsewhere are a change, also in the contents.
  first->raise();
  CHECK(order(root) != baseline);
  second->raise();
  CHECK(order(root) == baseline);
  third->raise();
  CHECK(order(root) != baseline);
  fourth->raise();
  CHECK(order(root) == baseline);

  // A widget added or removed is a change, also within an item view.
  auto* added = new QLabel(QStringLiteral("added"), plain);
  CHECK(order(root) != baseline);
  delete added;
  CHECK(order(root) == baseline);
  auto* indexWidget = new QLabel(QStringLiteral("cell"), tree->viewport());
  CHECK(order(root) != baseline);
  delete indexWidget;
  CHECK(order(root) == baseline);
  delete second;
  CHECK(order(root).size() == baseline.size() - 1);
  CHECK(order(root) != baseline);

  return failures;
}
