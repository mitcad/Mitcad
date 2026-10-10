// SPDX-License-Identifier: MIT
#pragma once

// The dialogs of Local and Cloud projects (mitcad#89): New Project, Open
// from Cloud, the design chooser of Open Project, Move to a Project's
// question, and Project Settings' Change... of the address. Each runs its
// network work on a thread of its own (RemoteTask) with its progress and
// Cancel in the dialog; a cancel or a failure leaves the folder as it was.

#include <functional>
#include <optional>

#include <QJsonObject>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

// File > New Project (also Open Project of a folder that is no project yet,
// and Move to a Project's new project).
struct NewProjectOptions {
  QString title;          // empty: "New Project"
  QString folder;         // suggested; empty: Documents/Mitcad/Project<n>
  bool folderFixed = false; // Open Project: the folder chosen there
  bool cloud = false;     // Storage preset to Cloud
  QString url;            // the address preset
  bool firstDesign = true; // a design named after the folder when it has none
};

struct NewProjectResult {
  enum class Kind {
    Created,       // the project is made (answer: create_project's)
    OpenProject,   // Open It: the folder is a project (or inside one)
    OpenFromCloud, // Open It Instead: the remote holds a project
  };
  Kind kind = Kind::Created;
  QString folder;      // the project's folder (absolute)
  QString url;         // Cloud: the address
  QJsonObject answer;  // Created: create_project's answer
  QString design;      // Created: the first design (relative), or empty
  QString broker;      // Created: live updates through this broker, or empty
};

std::optional<NewProjectResult> askNewProject(QWidget* parent, const NewProjectOptions& options);

// File > Open from Cloud: the address and a new or empty folder; clones a
// project, makes one in an empty remote (Create Project Here) or of a
// remote's files (Make It a Project), or opens a clone that is there (Open
// It).
struct OpenFromCloudOptions {
  QString url;
  QString folder;
};

struct OpenFromCloudResult {
  QString root;       // the project's folder
  QString url;
  QString action;     // "clone", "create", "adopt" or "open"
  QJsonObject answer; // the command's answer (none for "open")
  QString design;     // "create": the first design (relative)
};

std::optional<OpenFromCloudResult> askOpenFromCloud(QWidget* parent, const OpenFromCloudOptions& options);

// Open Project's design chooser for a project with several designs: each
// with its latest preview (`preview` gives a design's image path, or ""),
// `last` (relative) selected. The design chosen (relative), "" for New
// Design, nothing when cancelled.
std::optional<QString> askChooseDesign(QWidget* parent, const QString& project, const QStringList& designs,
                                       const QString& last, const std::function<QString(const QString&)>& preview);

// Move to a Project: a new project (through New Project) or an existing one.
enum class MoveTarget { NewProject, ExistingProject };
std::optional<MoveTarget> askMoveToProject(QWidget* parent, const QString& design, const QString& reason);

// Project Settings' Change...: a new address of the project's repository
// (moved or renamed), accepted only when it shares the project's history
// (remote_check's `related`).
std::optional<QString> askChangeAddress(QWidget* parent, const QString& root, const QString& current);

} // namespace mitcad
