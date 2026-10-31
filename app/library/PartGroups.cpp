// SPDX-License-Identifier: MIT
#include "PartGroups.hpp"

#include <algorithm>
#include <utility>

#include <QRegularExpression>

namespace mitcad {

QString groupKey(const QStringList& names) {
  QStringList parts;
  for (const QString& name : names) {
    const QString part = name.simplified().toLower();
    if (!part.isEmpty()) {
      parts << part;
    }
  }
  return parts.join(QLatin1Char('/'));
}

QString groupLabel(const QString& name) {
  const QString trimmed = name.trimmed();
  const bool idLike = !trimmed.isEmpty() && std::all_of(trimmed.begin(), trimmed.end(), [](QChar c) {
    const char16_t u = c.unicode();
    return (u >= u'a' && u <= u'z') || (u >= u'0' && u <= u'9') || u == u'-' || u == u'_';
  });
  if (!idLike) {
    return trimmed;
  }
  QString shown = trimmed;
  shown.replace(QLatin1Char('-'), QLatin1Char(' ')).replace(QLatin1Char('_'), QLatin1Char(' '));
  shown[0] = shown[0].toUpper();
  return shown;
}

bool inGroup(const QList<PartGroupPath>& groups, const QString& key) {
  if (key.isEmpty()) {
    return false;
  }
  const QString below = key + QLatin1Char('/');
  for (const PartGroupPath& group : groups) {
    const QString own = groupKey(group.names);
    if (own == key || own.startsWith(below)) {
      return true;
    }
  }
  return false;
}

void PartGroupTree::clear() {
  m_nodes.clear();
  m_top.clear();
}

void PartGroupTree::add(const PartGroupPath& group) {
  QString parent;
  QStringList names;
  QStringList labels;
  PartGroupPath above;
  for (int i = 0; i < group.names.size(); ++i) {
    const QString name = group.names.at(i).simplified();
    if (name.isEmpty()) {
      continue;
    }
    const int order = i < group.order.size() ? group.order.at(i) : 0;
    names << name;
    labels << groupLabel(name);
    above.names << name;
    above.order << order;
    const QString key = groupKey(names);
    auto found = m_nodes.find(key);
    if (found != m_nodes.end()) {
      // The first spelling stays; the earliest place orders it.
      found->order = std::min(found->order, order);
    } else {
      Node node;
      node.key = key;
      node.name = labels.last();
      node.path = labels.join(QLatin1Char('/'));
      node.parent = parent;
      node.group = above;
      node.order = order;
      m_nodes.insert(key, node);
      if (parent.isEmpty()) {
        m_top << key;
      } else {
        m_nodes[parent].children << key;
      }
    }
    parent = key;
  }
  if (!parent.isEmpty() && !group.category.isEmpty() && !m_nodes[parent].categories.contains(group.category)) {
    m_nodes[parent].categories << group.category;
  }
}

void PartGroupTree::sortKeys(QStringList& keys) const {
  std::stable_sort(keys.begin(), keys.end(), [this](const QString& a, const QString& b) {
    const Node* first = node(a);
    const Node* second = node(b);
    if (first == nullptr || second == nullptr) {
      return first != nullptr;
    }
    if (first->order != second->order) {
      return first->order < second->order;
    }
    return QString::localeAwareCompare(first->name, second->name) < 0;
  });
}

void PartGroupTree::finish() {
  sortKeys(m_top);
  for (auto it = m_nodes.begin(); it != m_nodes.end(); ++it) {
    QStringList children = it->children;
    sortKeys(children);
    it->children = children;
  }
}

const PartGroupTree::Node* PartGroupTree::node(const QString& key) const {
  const auto found = m_nodes.constFind(key);
  return found != m_nodes.cend() ? &found.value() : nullptr;
}

QStringList PartGroupTree::subtree(const QString& key) const {
  QStringList keys;
  const Node* at = node(key);
  if (at == nullptr) {
    return keys;
  }
  keys << key;
  for (const QString& child : at->children) {
    keys << subtree(child);
  }
  return keys;
}

QHash<QString, int> groupCounts(const QList<QList<PartGroupPath>>& parts) {
  QHash<QString, int> counts;
  for (const QList<PartGroupPath>& groups : parts) {
    // The part's groups and every group above them, each once.
    QSet<QString> keys;
    for (const PartGroupPath& group : groups) {
      const QString own = groupKey(group.names);
      if (own.isEmpty()) {
        continue;
      }
      keys.insert(own);
      for (qsizetype at = own.indexOf(QLatin1Char('/')); at >= 0; at = own.indexOf(QLatin1Char('/'), at + 1)) {
        keys.insert(own.left(at));
      }
    }
    for (const QString& key : std::as_const(keys)) {
      ++counts[key];
    }
  }
  return counts;
}

QSet<QString> hiddenGroups(const PartGroupTree& tree, const QHash<QString, int>& counts, const QString& chosen,
                           bool filtering, const QStringList& special) {
  // The chosen group and the groups above it stay.
  QSet<QString> kept{chosen};
  for (const PartGroupTree::Node* at = tree.node(chosen); at != nullptr; at = tree.node(at->parent)) {
    kept.insert(at->key);
  }
  QSet<QString> hidden;
  for (const QString& top : tree.top()) {
    const QStringList keys = tree.subtree(top);
    for (const QString& key : keys) {
      if (counts.value(key) <= 0 && !kept.contains(key)) {
        hidden.insert(key);
      }
    }
  }
  if (filtering) {
    for (const QString& key : special) {
      if (counts.value(key) <= 0 && !kept.contains(key)) {
        hidden.insert(key);
      }
    }
  }
  return hidden;
}

bool searchMatches(const QString& fields, const QString& search) {
  const QString haystack = fields.toLower();
  static const QRegularExpression spaces(QStringLiteral("\\s+"));
  const QStringList words = search.toLower().split(spaces, Qt::SkipEmptyParts);
  return std::all_of(words.begin(), words.end(), [&haystack](const QString& word) { return haystack.contains(word); });
}

} // namespace mitcad
