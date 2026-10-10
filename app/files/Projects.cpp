// SPDX-License-Identifier: MIT
#include "Projects.hpp"

#include <exception>

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>

#include "../framework/Json.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

QString ProjectState::name() const { return QFileInfo(root).fileName(); }

QString ProjectState::describe() const {
  switch (kind) {
  case Kind::None:
    return QStringLiteral("none");
  case Kind::Local:
    return unfollowedRemotes.isEmpty()
               ? QStringLiteral("local %1 at %2").arg(name(), root)
               : QStringLiteral("local %1 at %2 (remotes %3, none followed)")
                     .arg(name(), root, unfollowedRemotes.join(QStringLiteral(", ")));
  case Kind::Cloud:
    return QStringLiteral("cloud %1 at %2 (%3 %4)").arg(name(), root, remoteName, remoteUrl);
  case Kind::NoHistory:
    return outer.isEmpty() ? QStringLiteral("no versions %1 at %2").arg(name(), root)
                           : QStringLiteral("no versions %1 at %2 (inside %3)").arg(name(), root, outer);
  }
  return {};
}

bool ProjectState::operator==(const ProjectState& other) const {
  return kind == other.kind && root == other.root && remoteName == other.remoteName &&
         remoteUrl == other.remoteUrl && outer == other.outer && unfollowedRemotes == other.unfollowedRemotes;
}

QJsonObject projectsCommand(const QJsonObject& command) {
  try {
    const rust::Box<SyncControl> control = new_sync_control();
    return parseObject(projects_command(rustStr(compactJson(command)), *control));
  } catch (const std::exception& e) {
    return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                  {QStringLiteral("message"), errorText(e)},
                                                  {QStringLiteral("detail"), QString()}}}};
  }
}

QJsonObject inspectFolder(const QString& dir) {
  return projectsCommand({{QStringLiteral("cmd"), QStringLiteral("inspect_folder")},
                          {QStringLiteral("dir"), QDir::cleanPath(QDir::fromNativeSeparators(dir))}});
}

ProjectState projectOfInspection(const QJsonObject& inspection) {
  ProjectState state;
  const QString kind = inspection.value(QStringLiteral("kind")).toString();
  if (kind != QLatin1String("project") && kind != QLatin1String("inside_project")) {
    return state;
  }
  state.root = QDir::cleanPath(inspection.value(QStringLiteral("project_root")).toString());
  if (state.root.isEmpty()) {
    return state;
  }
  if (!inspection.value(QStringLiteral("has_history")).toBool()) {
    state.kind = ProjectState::Kind::NoHistory;
    state.outer = QDir::cleanPath(inspection.value(QStringLiteral("outer_repository")).toString());
    if (inspection.value(QStringLiteral("outer_repository")).isNull()) {
      state.outer.clear();
    }
    return state;
  }
  const QJsonObject remote = inspection.value(QStringLiteral("remote")).toObject();
  if (inspection.value(QStringLiteral("cloud")).toBool() && !remote.isEmpty()) {
    state.kind = ProjectState::Kind::Cloud;
    state.remoteName = remote.value(QStringLiteral("name")).toString();
    state.remoteUrl = remote.value(QStringLiteral("url")).toString();
  } else {
    state.kind = ProjectState::Kind::Local;
    if (inspection.value(QStringLiteral("cloud")).toBool()) {
      // Remotes, but none followed (mitcad#89).
      for (const QJsonValue& name : inspection.value(QStringLiteral("remotes")).toArray()) {
        state.unfollowedRemotes << name.toString();
      }
    }
  }
  return state;
}

ProjectState projectOfFile(const QString& path) {
  if (path.isEmpty()) {
    return {};
  }
  const rust::String found = find_project(rustStr(path.toUtf8()));
  const QString root = QString::fromUtf8(found.data(), static_cast<qsizetype>(found.size()));
  if (root.isEmpty()) {
    return {};
  }
  return projectOfInspection(inspectFolder(root));
}

GitIdentity gitIdentity() {
  // git_info's author: git's system and user configuration, not a
  // repository's that the working folder may be in.
  const QJsonObject author =
      projectsCommand({{QStringLiteral("cmd"), QStringLiteral("git_info")}}).value(QStringLiteral("author")).toObject();
  GitIdentity identity;
  identity.name = author.value(QStringLiteral("name")).toString();
  identity.email = author.value(QStringLiteral("email")).toString();
  return identity;
}

QString errorClassOf(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("class")).toString();
}

QString errorMessageOf(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
}

QString errorDetailOf(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("detail")).toString();
}

bool sameFolderPath(const QString& a, const QString& b) {
  const auto normal = [](const QString& path) {
    const QFileInfo info(path);
    const QString canonical = info.canonicalFilePath();
    return canonical.isEmpty() ? QDir::cleanPath(info.absoluteFilePath()) : canonical;
  };
#ifdef _WIN32
  return normal(a).compare(normal(b), Qt::CaseInsensitive) == 0;
#else
  return normal(a) == normal(b);
#endif
}

} // namespace mitcad
