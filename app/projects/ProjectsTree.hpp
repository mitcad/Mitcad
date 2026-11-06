// SPDX-License-Identifier: MIT
#pragma once

// What the Projects panel lists (mitcad#145): the projects known on this
// computer and those in the projects folder, each with its folders and
// designs as the core's `project_tree` gives them (commands.md,
// "Projects"). A project is its repository's root: nothing groups the
// projects (mitcad#173). The trees are read on a thread of their own
// (readProjects); the panel turns the answer into rows (projectRows) on
// the UI thread.

#include <QDateTime>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

// A design of a project as the panel shows it.
struct ProjectDesignRow {
  QString path;           // relative to the project, '/'-separated
  int version = 0;        // the latest version's number, 0: none recorded
  QString versionId;      // its short id
  qint64 versionTime = 0; // when it was saved (Unix seconds)
  QString versionAuthor;  // by whom (name)
  bool recorded = false;  // in the history at all
  bool changed = false;   // the file differs from its latest version
  // The edit lock (Cloud projects with edit locks): held by `lockOwner`;
  // `lockMine` by this session, `lockSameOwner` by the same author in
  // another session.
  bool locked = false;
  QString lockOwner;
  bool lockMine = false;
  bool lockSameOwner = false;
  // When the file last changed (`project_tree`'s `modified`; invalid: not
  // given), the Start page's cards (mitcad#1301).
  QDateTime modified;
};

// A project as the panel shows it.
struct ProjectRow {
  enum class Kind {
    Local,      // versions on this computer
    Cloud,      // versions also in a remote repository
    NoHistory,  // a project folder without versions (inside another repository)
    Missing,    // a known project whose folder is gone
    Unreadable, // its tree could not be read (`problem` says why)
  };
  Kind kind = Kind::Local;
  QString root; // the project's folder (absolute, '/')
  // The name shown for the project (mitcad#152, #173): the display name set
  // in Project Settings, else the default: a Cloud project's repository
  // address without the scheme (github.com/owner/repository), else the
  // folder's name.
  QString name;
  QString folderName;  // the folder's name (the rows' keys)
  QString displayName; // set in Project Settings ("" none)
  QString remoteUrl;
  // Remotes, but none followed (the indicator's "No remote chosen").
  bool noRemoteChosen = false;
  int ahead = -1; // versions not sent (-1: not known)
  int behind = -1;
  QStringList folders; // relative, sorted
  QVector<ProjectDesignRow> designs;
  bool truncated = false;
  bool known = false; // in the list of known projects (else from the projects folder)
  QString problem;
};

// What the panel lists, read with the core (no network, no writes): the
// known projects (newest first), then the projects folder's projects that
// are neither known nor hidden. Each entry is {"root", "known", and
// "tree" (project_tree's answer), or "missing": true, or "problem" with
// "inspection" (inspect_folder's answer)}. `session` is the edit locks'
// session of this run (whose locks are "mine"). For a thread of its own.
QJsonObject readProjects(const QStringList& known, const QString& projectsFolder, const QStringList& hidden,
                         const QString& session);

// The rows of readProjects' answer.
QVector<ProjectRow> projectRows(const QJsonObject& answer);

// A project's default name in the panel: a Cloud project's repository
// address without the scheme (repositoryAddressOf), else the folder's name.
QString defaultProjectName(const QString& root, const QString& remoteUrl);

// Whether any project is configured (mitcad#151): a known project whose
// folder is there, or a project in the projects folder that Remove from
// List did not take out. Reads only folder entries (no project opened).
bool anyProjectConfigured(const QStringList& known, const QString& projectsFolder, const QStringList& hidden);

// Whether two folder paths are the same (case-insensitive on Windows).
bool sameRoot(const QString& a, const QString& b);

// Paths inside a project: relative to its folder, '/'-separated.
// The folder part of a path ("" for a top-level name).
inline QString dirOf(const QString& path) {
  const qsizetype slash = path.lastIndexOf(QLatin1Char('/'));
  return slash < 0 ? QString() : path.left(slash);
}
// The name after the last '/'.
inline QString lastPart(const QString& path) { return path.section(QLatin1Char('/'), -1); }
// `name` inside `folder` ("" is the project's top level).
inline QString joinPath(const QString& folder, const QString& name) {
  return folder.isEmpty() ? name : QStringLiteral("%1/%2").arg(folder, name);
}

} // namespace mitcad
