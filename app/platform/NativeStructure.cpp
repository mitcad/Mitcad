// SPDX-License-Identifier: MIT
#include "platform/NativeStructure.hpp"

#include <algorithm>
#include <functional>

#include <QAbstractScrollArea>
#include <QHash>
#include <QWidget>

namespace mitcad {

QList<QWidget*> nativeStructureOrder(const QWidget* root, const QList<QWidget*>& widgets) {
  // Each parent's children in the given (child) order.
  QHash<const QWidget*, QList<QWidget*>> children;
  for (QWidget* widget : widgets) { children[widget->parentWidget()].append(widget); }
  QList<QWidget*> ordered;
  ordered.reserve(widgets.size());
  const auto visit = [&](auto&& self, const QWidget* parent) -> void {
    const auto found = children.constFind(parent);
    if (found == children.constEnd()) { return; }
    QList<QWidget*> group = found.value();
    children.remove(parent);
    // A set: the same widgets always come in the same order.
    if (qobject_cast<const QAbstractScrollArea*>(parent) != nullptr) {
      std::sort(group.begin(), group.end(), std::less<QWidget*>());
    }
    for (QWidget* child : group) {
      ordered.append(child);
      self(self, child);
    }
  };
  visit(visit, root);
  // A widget whose parent is outside the list keeps its given place.
  for (QWidget* widget : widgets) { visit(visit, widget->parentWidget()); }
  return ordered;
}

} // namespace mitcad
