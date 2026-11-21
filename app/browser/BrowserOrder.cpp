// SPDX-License-Identifier: MIT
#include "BrowserOrder.hpp"

#include <algorithm>
#include <utility>

#include <QJsonObject>
#include <QJsonValue>

namespace mitcad::browser_order {

namespace {

// The occurrence at `path` in a tree of them; null when there is none.
const DocumentSnapshot::Occurrence* findOccurrence(const QVector<DocumentSnapshot::Occurrence>& level,
                                                   const QString& path) {
  for (const DocumentSnapshot::Occurrence& occurrence : level) {
    if (occurrence.path == path) {
      return &occurrence;
    }
    if (path.startsWith(occurrence.path + QLatin1Char('/'))) {
      return findOccurrence(occurrence.children, path);
    }
  }
  return nullptr;
}

// The rows as the Browser lists them: each folder's (in folderOrder), then
// those in no folder, each as "<folder>|<path>".
QStringList listed(const QVector<Sibling>& siblings) {
  QStringList rows;
  QStringList groups = folderOrder(siblings);
  groups << QString();
  for (const QString& group : std::as_const(groups)) {
    for (const QString& path : groupOf(siblings, group)) {
      rows << group + QLatin1Char('|') + path;
    }
  }
  return rows;
}

} // namespace

QString parentPath(const QString& path) {
  const qsizetype slash = path.lastIndexOf(QLatin1Char('/'));
  return slash < 0 ? QString() : path.left(slash);
}

QVector<Sibling> siblingsIn(const DocumentSnapshot& snapshot, const QString& parent) {
  const DocumentSnapshot::Occurrence* owner = parent.isEmpty() ? nullptr : findOccurrence(snapshot.occurrences, parent);
  if (!parent.isEmpty() && owner == nullptr) {
    return {};
  }
  QVector<Sibling> siblings;
  for (const DocumentSnapshot::Occurrence& occurrence : owner != nullptr ? owner->children : snapshot.occurrences) {
    siblings.append({occurrence.path, occurrence.name, occurrence.folder});
  }
  return siblings;
}

QStringList folderOrder(const QVector<Sibling>& siblings) {
  QStringList folders;
  for (const Sibling& sibling : siblings) {
    if (!sibling.folder.isEmpty() && !folders.contains(sibling.folder)) {
      folders << sibling.folder;
    }
  }
  return folders;
}

QStringList groupOf(const QVector<Sibling>& siblings, const QString& folder) {
  QStringList paths;
  for (const Sibling& sibling : siblings) {
    if (sibling.folder == folder) {
      paths << sibling.path;
    }
  }
  return paths;
}

QString newFolderName(const QStringList& taken) {
  for (int n = 1;; ++n) {
    const QString name = QStringLiteral("Folder%1").arg(n);
    if (!taken.contains(name)) {
      return name;
    }
  }
}

QJsonArray occurrenceFolderCommands(const QVector<Sibling>& siblings, const QStringList& paths,
                                    const QString& folder) {
  QJsonArray commands;
  for (const Sibling& sibling : siblings) {
    if (!paths.contains(sibling.path) || sibling.folder == folder) {
      continue;
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_occurrence_folder")},
                        {QStringLiteral("occurrence"), sibling.path}};
    if (!folder.isEmpty()) {
      command.insert(QStringLiteral("folder"), folder);
    }
    commands.append(command);
  }
  return commands;
}

QJsonArray bodyFolderCommands(const QHash<QString, QString>& folders, const QStringList& bodies,
                              const QString& folder) {
  QJsonArray commands;
  for (const QString& body : bodies) {
    if (!folders.contains(body) || folders.value(body) == folder) {
      continue;
    }
    QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("set_body_folder")}, {QStringLiteral("uid"), body}};
    if (!folder.isEmpty()) {
      command.insert(QStringLiteral("folder"), folder);
    }
    commands.append(command);
  }
  return commands;
}

QString neighbor(const QVector<Sibling>& siblings, const QString& path, int step) {
  QString folder;
  bool found = false;
  for (const Sibling& sibling : siblings) {
    if (sibling.path == path) {
      folder = sibling.folder;
      found = true;
    }
  }
  if (!found) {
    return QString();
  }
  const QStringList group = groupOf(siblings, folder);
  const qsizetype at = group.indexOf(path) + step;
  return at >= 0 && at < group.size() ? group.at(at) : QString();
}

QJsonArray moveCommands(const QVector<Sibling>& siblings, const QStringList& moved, const QString& folder,
                        const QString& anchor, bool after) {
  QStringList order; // the siblings' paths in the model's order
  for (const Sibling& sibling : siblings) {
    order << sibling.path;
  }
  QStringList moving;
  for (const QString& path : moved) {
    if (order.contains(path) && !moving.contains(path) && path != anchor) {
      moving << path;
    }
  }
  if (moving.isEmpty() || (!anchor.isEmpty() && !groupOf(siblings, folder).contains(anchor))) {
    return {};
  }
  QJsonArray commands = occurrenceFolderCommands(siblings, moving, folder);
  // The occurrences the reorder places together, and where: in a folder,
  // all of its occurrences in their new order, where its first one was
  // (its place among the folders kept); in no folder, those moved, next to
  // the anchor.
  QStringList together;
  qsizetype index = 0;
  if (!folder.isEmpty()) {
    QStringList group;
    for (const QString& path : groupOf(siblings, folder)) {
      if (!moving.contains(path)) {
        group << path;
      }
    }
    const qsizetype at = anchor.isEmpty() ? group.size() : group.indexOf(anchor) + (after ? 1 : 0);
    together = group.mid(0, at) + moving + group.mid(at);
    // Where the folder's first occurrence is now, among the siblings that
    // stay where they are.
    qsizetype first = order.size();
    for (const QString& path : groupOf(siblings, folder)) {
      first = std::min(first, order.indexOf(path));
    }
    if (first == order.size()) {
      for (const QString& path : moving) {
        first = std::min(first, order.indexOf(path));
      }
    }
    for (qsizetype i = 0; i < first; ++i) {
      index += together.contains(order.at(i)) ? 0 : 1;
    }
  } else {
    together = moving;
    QStringList rest = order;
    for (const QString& path : moving) {
      rest.removeAll(path);
    }
    index = anchor.isEmpty() ? rest.size() : rest.indexOf(anchor) + (after ? 1 : 0);
  }
  // Left out when the Browser would list the rows as it does now (a row
  // moved after the row in no folder before it, a folder's first row in
  // its place).
  QStringList result = order;
  for (const QString& path : std::as_const(together)) {
    result.removeAll(path);
  }
  for (qsizetype i = 0; i < together.size(); ++i) {
    result.insert(index + i, together.at(i));
  }
  QVector<Sibling> then;
  for (const QString& path : std::as_const(result)) {
    const auto found = std::find_if(siblings.begin(), siblings.end(),
                                    [&path](const Sibling& sibling) { return sibling.path == path; });
    Sibling row = *found;
    if (moving.contains(path)) {
      row.folder = folder;
    }
    then.append(row);
  }
  if (listed(then) != listed(siblings)) {
    QJsonArray with;
    for (qsizetype i = 1; i < together.size(); ++i) {
      with.append(together.at(i));
    }
    QJsonObject reorder{{QStringLiteral("cmd"), QStringLiteral("reorder_occurrence")},
                        {QStringLiteral("occurrence"), together.first()},
                        {QStringLiteral("index"), static_cast<int>(index)}};
    if (!with.isEmpty()) {
      reorder.insert(QStringLiteral("with"), with);
    }
    commands.append(reorder);
  }
  return commands;
}

} // namespace mitcad::browser_order
