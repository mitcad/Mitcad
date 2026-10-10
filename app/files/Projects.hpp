// SPDX-License-Identifier: MIT
#pragma once

// Local and Cloud projects in the application (mitcad#89): the current
// project as state of the window's own, and the core's project commands
// (core/model/src/api/commands.md, "Projects") that need no thread of
// their own.
//
// A project is a folder with .mitcad/project.json. It is Local when the
// folder is the root of a git repository without a remote, Cloud with one;
// the kind is read from the folder, never stored. A project inside another
// git repository (not at its root) has no versions.

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

// The window's current project: set by New Project, Open Project, Open from
// Cloud, Open..., Open Recent and Move to a Project, and kept while an
// untitled design is open. Remote work, the indicator and Project Settings
// use its root, never the design's path.
struct ProjectState {
  enum class Kind {
    None,      // no project: a loose file or an untitled design
    Local,     // versions on this computer
    Cloud,     // versions also in a remote repository
    NoHistory, // a project folder without versions (inside another repository)
  };

  Kind kind = Kind::None;
  QString root;        // the project's folder (absolute, '/')
  QString remoteName;  // Cloud: the remote the branch follows
  QString remoteUrl;   // Cloud: its address, credentials hidden
  QString outer;       // NoHistory: the git repository it is inside
  // Local, but its repository has remotes and follows none of them
  // (several, none of them origin): their names. Project Settings asks
  // which to follow; until then nothing is synced.
  QStringList unfollowedRemotes;

  bool isProject() const { return kind != Kind::None; }
  bool hasHistory() const { return kind == Kind::Local || kind == Kind::Cloud; }
  // The folder's name ("Robot arm").
  QString name() const;
  // For logs: "local Robot arm at /path" (cloud with the address, none).
  QString describe() const;
  bool operator==(const ProjectState& other) const;
  bool operator!=(const ProjectState& other) const { return !(*this == other); }
};

// A projects command without a project (inspect_folder, ssh_public_key,
// git_info, ...) on this thread; for those that use the network run a
// RemoteTask::projects instead. A command that fails as a command answers
// {"error": {"class": "other", "message"}}.
QJsonObject projectsCommand(const QJsonObject& command);

// inspect_folder of a folder (no network, no writes).
QJsonObject inspectFolder(const QString& dir);

// The project a folder's inspection describes (its project, or the project
// it is inside), as the window keeps it.
ProjectState projectOfInspection(const QJsonObject& inspection);

// The project a file (which need not exist) is in.
ProjectState projectOfFile(const QString& path);

// git's configured author outside a project (user.name, user.email of the
// git program remote work uses): empty when it has none.
struct GitIdentity {
  QString name;
  QString email;
  bool complete() const { return !name.isEmpty() && !email.isEmpty(); }
};
GitIdentity gitIdentity();

// The answer's error class and message ("" when it has none).
QString errorClassOf(const QJsonObject& answer);
QString errorMessageOf(const QJsonObject& answer);
QString errorDetailOf(const QJsonObject& answer);

// Whether two paths name the same folder.
bool sameFolderPath(const QString& a, const QString& b);

} // namespace mitcad
