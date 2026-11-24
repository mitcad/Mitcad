// SPDX-License-Identifier: MIT
#include "AccessibleTree.hpp"

#include <algorithm>
#include <vector>

#include <QAccessible>
#include <QAccessibleInterface>
#include <QElapsedTimer>
#include <QMetaEnum>
#include <QObject>
#include <QRect>
#include <QWidget>

namespace mitcad {
namespace {

// A walk stops here: a tree this big is the fault being looked for.
constexpr int kNodeLimit = 2000000;
constexpr int kDepthLimit = 256;
// Below the window's children, lines only to this depth and this many.
constexpr int kDetailDepth = 14;
constexpr int kDetailLines = 200;

QString describe(QAccessibleInterface* node) {
  const char* role = QMetaEnum::fromType<QAccessible::Role>().valueToKey(node->role());
  QString text = role != nullptr ? QString::fromLatin1(role) : QString::number(node->role());
  if (const QObject* object = node->object()) {
    text += QLatin1Char(' ') + QString::fromLatin1(object->metaObject()->className());
    if (!object->objectName().isEmpty()) {
      text += QStringLiteral(" '%1'").arg(object->objectName());
    }
  }
  return text;
}

// The id Qt's UI Automation bridge gives a node without an identifier of
// its own: the object names (or class names) of its ancestors, built for
// every node as the bridge builds it.
qsizetype pathId(QAccessibleInterface* node) {
  QString id = node->text(QAccessible::Identifier);
  if (!id.isEmpty()) {
    return id.size();
  }
  for (QAccessibleInterface* at = node; at != nullptr; at = at->parent()) {
    if (const QObject* object = at->object()) {
      id.prepend(object->objectName().isEmpty() ? QString::fromLatin1(object->metaObject()->className())
                                                : object->objectName());
      id.prepend(QLatin1Char('.'));
    }
  }
  return id.size();
}

struct Entry {
  int depth = 0;
  int nodes = 0;
  QString text;
  bool hidden = false;
};

struct Walk {
  AccessibleTreeStats stats;
  std::vector<Entry> entries;
  // Read as a client reads each node; the sum only keeps the reads.
  qint64 readSum = 0;
  bool walkHidden = false;

  // As the bridges walk it: a child whose state is invisible is skipped
  // with what is below it (UI Automation's navigation skips it).
  // A hidden subtree (below a child whose state says invisible) is walked
  // too, counted apart: what it would add when shown.
  int visit(QAccessibleInterface* node, QAccessibleInterface* parent, int depth, bool hidden) {
    if (stats.nodes + stats.hiddenNodes >= kNodeLimit) {
      stats.truncated = true;
      return 0;
    }
    ++(hidden ? stats.hiddenNodes : stats.nodes);
    if (!hidden) {
      stats.maxDepth = std::max(stats.maxDepth, depth);
    }
    readSum += node->role() + node->text(QAccessible::Name).size() + node->text(QAccessible::Description).size() +
               node->rect().width() + pathId(node) + (parent != nullptr ? parent->indexOfChild(node) : 0);
    const std::size_t at = entries.size();
    const bool listed = depth <= kDetailDepth;
    if (listed) {
      entries.push_back(Entry{depth, 0, QString(), hidden});
    }
    int nodes = 1;
    const int count = depth < kDepthLimit ? node->childCount() : 0;
    if (!hidden && count > stats.widest) {
      stats.widest = count;
      stats.widestNode = describe(node);
    }
    for (int i = 0; i < count; ++i) {
      QAccessibleInterface* child = node->child(i);
      if (child == nullptr || !child->isValid()) {
        continue;
      }
      const bool invisible = child->state().invisible;
      if (invisible && !hidden) {
        ++stats.invisible;
      }
      if (invisible && !hidden && !walkHidden) {
        continue;
      }
      const int below = visit(child, node, depth + 1, hidden || invisible);
      if (hidden || !invisible) {
        nodes += below;
      }
    }
    if (listed) {
      entries[at].nodes = nodes;
      entries[at].text = describe(node) + QStringLiteral(", %1 children").arg(count);
    }
    return nodes;
  }
};

} // namespace

AccessibleTreeStats walkAccessibleTree(QWidget* window, bool walkHidden, double detailShare, int detailNodes) {
  Walk walk;
  walk.walkHidden = walkHidden;
  QAccessibleInterface* root = window != nullptr ? QAccessible::queryAccessibleInterface(window) : nullptr;
  if (root == nullptr) {
    return walk.stats;
  }
  QElapsedTimer clock;
  clock.start();
  walk.visit(root, nullptr, 0, false);
  walk.stats.ms = clock.elapsed();
  const int share = std::max(detailNodes, static_cast<int>(walk.stats.nodes * detailShare));
  for (const Entry& entry : walk.entries) {
    if (walk.stats.subtrees.size() >= kDetailLines) {
      break;
    }
    if (entry.depth == 1 || (entry.depth > 1 && entry.nodes >= share)) {
      walk.stats.subtrees << QStringLiteral("%1%2%3: %4 nodes")
                                 .arg(QString(2 * (entry.depth - 1), QLatin1Char(' ')),
                                      entry.hidden ? QStringLiteral("(hidden) ") : QString(), entry.text)
                                 .arg(entry.nodes);
    }
  }
  return walk.stats;
}

} // namespace mitcad
