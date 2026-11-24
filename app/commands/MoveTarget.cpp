// SPDX-License-Identifier: MIT
// The component a Move/Copy of components works in (mitcad#2302): see
// MoveTarget.hpp.
#include "MoveTarget.hpp"

#include <functional>

#include <QJsonValue>

namespace mitcad::cmd {
namespace {

const QString kRootComponent = QStringLiteral("C0");

QString uidOf(const QJsonObject& node) { return node.value(QStringLiteral("uid")).toString(); }

QString joined(const QString& prefix, const QString& uid) {
  return prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
}

// The first path (depth first, in the tree's order) of an occurrence of
// the component; empty when there is none.
QString firstOccurrenceOf(const QJsonArray& tree, const QString& component) {
  std::function<QString(const QJsonArray&, const QString&)> find = [&](const QJsonArray& nodes,
                                                                        const QString& prefix) {
    for (const QJsonValue& value : nodes) {
      const QJsonObject node = value.toObject();
      const QString path = joined(prefix, uidOf(node));
      if (node.value(QStringLiteral("component")).toString() == component) {
        return path;
      }
      const QString inside = find(node.value(QStringLiteral("children")).toArray(), path);
      if (!inside.isEmpty()) {
        return inside;
      }
    }
    return QString();
  };
  return find(tree, QString());
}

} // namespace

QJsonObject occurrenceNodeAt(const QJsonArray& tree, const QString& path) {
  if (path.isEmpty()) {
    return QJsonObject();
  }
  QJsonArray nodes = tree;
  QJsonObject found;
  for (const QString& step : path.split(QLatin1Char('/'))) {
    found = QJsonObject();
    for (const QJsonValue& value : nodes) {
      const QJsonObject node = value.toObject();
      if (uidOf(node) == step) {
        found = node;
        break;
      }
    }
    if (found.isEmpty()) {
      return QJsonObject();
    }
    nodes = found.value(QStringLiteral("children")).toArray();
  }
  return found;
}

QString componentAtPath(const QJsonArray& tree, const QString& path) {
  if (path.isEmpty()) {
    return kRootComponent;
  }
  return occurrenceNodeAt(tree, path).value(QStringLiteral("component")).toString();
}

QString parentPathOf(const QString& path) {
  const qsizetype slash = path.lastIndexOf(QLatin1Char('/'));
  return slash < 0 ? QString() : path.left(slash);
}

int occurrenceCountOf(const QJsonArray& tree, const QString& component) {
  int count = 0;
  std::function<void(const QJsonArray&)> walk = [&](const QJsonArray& nodes) {
    for (const QJsonValue& value : nodes) {
      const QJsonObject node = value.toObject();
      if (node.value(QStringLiteral("component")).toString() == component) {
        ++count;
      }
      walk(node.value(QStringLiteral("children")).toArray());
    }
  };
  walk(tree);
  return count;
}

MoveTarget moveTargetOf(const QStringList& paths, const QString& home, const QJsonArray& tree) {
  MoveTarget target;
  target.component = home;
  bool first = true;
  for (const QString& path : paths) {
    if (path.isEmpty()) {
      continue;
    }
    const QStringList steps = path.split(QLatin1Char('/'));
    const QString parentPath = parentPathOf(path);
    const QString parent = componentAtPath(tree, parentPath);
    // Placed directly in `home`, or deeper on a path through it, or not
    // under it at all (the model's refusal).
    QString component = home;
    QString framePath;
    if (parent == home) {
      framePath = parentPath;
    } else {
      for (qsizetype i = 0; i + 1 < steps.size(); ++i) {
        const QString above = steps.mid(0, i).join(QLatin1Char('/'));
        if (componentAtPath(tree, above) == home) {
          component = parent;
          framePath = parentPath;
          break;
        }
      }
    }
    if (first) {
      target.component = component;
      target.parentPath = framePath;
      first = false;
    } else if (component != target.component) {
      target.mixed = true;
    }
  }
  return target;
}

QString frameOccurrenceOf(const QStringList& paths, const QString& component, const QJsonArray& tree) {
  if (component.isEmpty() || component == kRootComponent) {
    return QString();
  }
  for (const QString& path : paths) {
    const QString parentPath = parentPathOf(path);
    if (!parentPath.isEmpty() && componentAtPath(tree, parentPath) == component) {
      return parentPath;
    }
  }
  return firstOccurrenceOf(tree, component);
}

std::optional<QString> pickedOccurrenceToWrite(const QString& path, const QString& component,
                                               const QJsonArray& tree) {
  const QString own = component.isEmpty() ? kRootComponent : component;
  if (componentAtPath(tree, path) == own) {
    return std::nullopt;
  }
  return path;
}

QString parentOccurrenceToWrite(const QStringList& paths, const QString& component, const QJsonArray& tree) {
  if (component.isEmpty() || component == kRootComponent || occurrenceCountOf(tree, component) < 2) {
    return QString();
  }
  for (const QString& path : paths) {
    const QString parentPath = parentPathOf(path);
    if (!parentPath.isEmpty() && componentAtPath(tree, parentPath) == component) {
      return parentPath;
    }
  }
  return QString();
}

QString loadedOccurrencePath(const QString& uid, const QString& parentOccurrence, const QString& fallback,
                             const QJsonArray& tree) {
  if (parentOccurrence.isEmpty() || occurrenceNodeAt(tree, parentOccurrence).isEmpty()) {
    return fallback;
  }
  const QString path = joined(parentOccurrence, uid);
  return occurrenceNodeAt(tree, path).isEmpty() ? fallback : path;
}

} // namespace mitcad::cmd
