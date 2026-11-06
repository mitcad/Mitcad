// SPDX-License-Identifier: MIT
#include "ProjectsTree.hpp"

#include <exception>
#include <utility>

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonValue>

#include "../files/CloudAddress.hpp"
#include "../files/Projects.hpp"
#include "../framework/Json.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

// At most this many projects of the projects folder are listed.
constexpr int kMaxFolderProjects = 500;

QString clean(const QString& path) { return QDir::cleanPath(QDir::fromNativeSeparators(path)); }

bool isProjectFolder(const QString& dir) {
  return QFileInfo(QDir(dir).filePath(QStringLiteral(".mitcad/project.json"))).isFile();
}

QJsonObject readOne(const QString& root, bool known, const QByteArray& treeJson) {
  QJsonObject entry{{QStringLiteral("root"), root}, {QStringLiteral("known"), known}};
  if (!QFileInfo(root).isDir()) {
    entry.insert(QStringLiteral("missing"), true);
    return entry;
  }
  try {
    // A project of this thread's own.
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    entry.insert(QStringLiteral("tree"), parseObject(project->command(rustStr(treeJson))));
  } catch (const std::exception& e) {
    // No versions (inside another repository, or an older project), or no
    // project any more: what the folder is.
    entry.insert(QStringLiteral("problem"), errorText(e));
    entry.insert(QStringLiteral("inspection"), inspectFolder(root));
  }
  return entry;
}

} // namespace

bool sameRoot(const QString& a, const QString& b) {
#ifdef _WIN32
  return clean(a).compare(clean(b), Qt::CaseInsensitive) == 0;
#else
  return clean(a) == clean(b);
#endif
}

QString defaultProjectName(const QString& root, const QString& remoteUrl) {
  const QString cloud = remoteUrl.isEmpty() ? QString() : repositoryAddressOf(remoteUrl);
  return cloud.isEmpty() ? QFileInfo(clean(root)).fileName() : cloud;
}

bool anyProjectConfigured(const QStringList& known, const QString& projectsFolder, const QStringList& hidden) {
  for (const QString& root : known) {
    if (!root.isEmpty() && isProjectFolder(clean(root))) {
      return true;
    }
  }
  if (projectsFolder.isEmpty()) {
    return false;
  }
  const QDir folder(projectsFolder);
  for (const QString& name : folder.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
    const QString root = clean(folder.filePath(name));
    if (!isProjectFolder(root)) {
      continue;
    }
    bool isHidden = false;
    for (const QString& other : hidden) {
      isHidden = isHidden || sameRoot(other, root);
    }
    if (!isHidden) {
      return true;
    }
  }
  return false;
}

QJsonObject readProjects(const QStringList& known, const QString& projectsFolder, const QStringList& hidden,
                         const QString& session) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("project_tree")}};
  if (!session.isEmpty()) {
    command.insert(QStringLiteral("session"), session);
  }
  const QByteArray treeJson = compactJson(command);
  QJsonArray projects;
  QStringList listed;
  const auto isListed = [&listed](const QString& root) {
    for (const QString& other : std::as_const(listed)) {
      if (sameRoot(other, root)) {
        return true;
      }
    }
    return false;
  };
  for (const QString& given : known) {
    const QString root = clean(given);
    if (root.isEmpty() || isListed(root)) {
      continue;
    }
    listed << root;
    projects.append(readOne(root, true, treeJson));
  }
  if (!projectsFolder.isEmpty()) {
    const QDir folder(projectsFolder);
    const QStringList names = folder.entryList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name | QDir::IgnoreCase);
    int count = 0;
    for (const QString& name : names) {
      const QString root = clean(folder.filePath(name));
      if (!isProjectFolder(root) || isListed(root)) {
        continue;
      }
      bool isHidden = false;
      for (const QString& other : hidden) {
        isHidden = isHidden || sameRoot(other, root);
      }
      if (isHidden) {
        continue;
      }
      if (++count > kMaxFolderProjects) {
        break;
      }
      listed << root;
      projects.append(readOne(root, false, treeJson));
    }
  }
  return {{QStringLiteral("projects"), projects}};
}

QVector<ProjectRow> projectRows(const QJsonObject& answer) {
  QVector<ProjectRow> rows;
  for (const QJsonValue& value : answer.value(QStringLiteral("projects")).toArray()) {
    const QJsonObject entry = value.toObject();
    ProjectRow row;
    row.root = clean(entry.value(QStringLiteral("root")).toString());
    row.folderName = QFileInfo(row.root).fileName();
    row.name = row.folderName;
    row.known = entry.value(QStringLiteral("known")).toBool();
    if (entry.value(QStringLiteral("missing")).toBool()) {
      row.kind = ProjectRow::Kind::Missing;
      rows << row;
      continue;
    }
    if (!entry.contains(QStringLiteral("tree"))) {
      const QJsonObject inspection = entry.value(QStringLiteral("inspection")).toObject();
      const ProjectState state = projectOfInspection(inspection);
      if (state.kind == ProjectState::Kind::NoHistory && sameRoot(state.root, row.root)) {
        row.kind = ProjectRow::Kind::NoHistory;
        row.problem = state.outer.isEmpty() ? QString() : state.outer;
      } else {
        row.kind = ProjectRow::Kind::Unreadable;
        row.problem = entry.value(QStringLiteral("problem")).toString();
      }
      rows << row;
      continue;
    }
    const QJsonObject tree = entry.value(QStringLiteral("tree")).toObject();
    if (!tree.value(QStringLiteral("error")).isNull() && tree.value(QStringLiteral("error")).isObject()) {
      row.kind = ProjectRow::Kind::Unreadable;
      row.problem = errorMessageOf(tree);
      rows << row;
      continue;
    }
    const QJsonValue remote = tree.value(QStringLiteral("remote"));
    if (remote.isObject()) {
      row.kind = ProjectRow::Kind::Cloud;
      row.remoteUrl = remote.toObject().value(QStringLiteral("url")).toString();
    } else {
      row.kind = ProjectRow::Kind::Local;
      row.noRemoteChosen = !tree.value(QStringLiteral("remotes")).toArray().isEmpty();
    }
    row.displayName = tree.value(QStringLiteral("display_name")).toString().trimmed();
    row.name = row.displayName.isEmpty() ? defaultProjectName(row.root, row.remoteUrl) : row.displayName;
    row.ahead = tree.value(QStringLiteral("ahead")).isDouble() ? tree.value(QStringLiteral("ahead")).toInt() : -1;
    row.behind = tree.value(QStringLiteral("behind")).isDouble() ? tree.value(QStringLiteral("behind")).toInt() : -1;
    row.truncated = tree.value(QStringLiteral("truncated")).toBool();
    for (const QJsonValue& folder : tree.value(QStringLiteral("folders")).toArray()) {
      row.folders << folder.toString();
    }
    for (const QJsonValue& item : tree.value(QStringLiteral("designs")).toArray()) {
      const QJsonObject design = item.toObject();
      ProjectDesignRow d;
      d.path = design.value(QStringLiteral("path")).toString();
      d.recorded = design.value(QStringLiteral("recorded")).toBool();
      d.changed = design.value(QStringLiteral("changed")).toBool();
      d.modified = QDateTime::fromString(design.value(QStringLiteral("modified")).toString(), Qt::ISODate);
      const QJsonObject version = design.value(QStringLiteral("version")).toObject();
      if (!version.isEmpty()) {
        d.version = version.value(QStringLiteral("number")).toInt();
        d.versionId = version.value(QStringLiteral("short_id")).toString();
        d.versionTime = version.value(QStringLiteral("time")).toInteger();
        d.versionAuthor = version.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString();
      }
      const QJsonObject lock = design.value(QStringLiteral("lock")).toObject();
      if (!lock.isEmpty()) {
        d.locked = true;
        d.lockOwner = lock.value(QStringLiteral("owner")).toObject().value(QStringLiteral("name")).toString();
        d.lockMine = lock.value(QStringLiteral("mine")).toBool();
        d.lockSameOwner = lock.value(QStringLiteral("same_owner")).toBool();
      }
      row.designs << d;
    }
    rows << row;
  }
  return rows;
}

} // namespace mitcad
