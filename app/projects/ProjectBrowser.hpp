// SPDX-License-Identifier: MIT
#pragma once

// File > Open from Project (mitcad#530): a window like a file dialog on top
// of the project system, built on the project picker's data (readProjects
// on a thread of its own, `project_tree`; ProjectBrowserData.hpp), never on
// the file system's raw folders:
//
//   [Shop › parts › sub]                 Search: [______]  In: [All Projects]
//   +------------------+--------------------------------------------------+
//   | Shop (local)     | Name    Version  Edit lock          Last change  By |
//   |   parts          | sub                                                |
//   |     sub          | gear    v2*      Editing elsewhere  2026-10-22   Ada |
//   | Team (cloud)     |                                                    |
//   +------------------+--------------------------------------------------+
//   [Another File...]                       [ ] Read-only   [Open] [Cancel]
//
// Left, the projects (Local and Cloud, with the Projects panel's names and
// icons) and their folders; right, the place's sub-folders, then its
// designs with their version, edit lock, last change and who made it, then
// its other files (mitcad#2204) with the system's icon, "new" or "*" and
// their size in the tooltip. A double-click (or Enter) on a folder goes
// into it, on a design opens it, on another file hands it to the system's
// app (the browser closes, "chose <path> (a file for the system's app)");
// Backspace goes up. The path bar's parts are buttons. Typing in Search
// lists the designs whose name holds the text, in every project or in the
// place's (In), with their project and folder. Another File... leaves the
// choice to the system's file dialog. The place shown last (a design's
// folder when one is opened) is remembered (projects/browseRoot,
// projects/browseFolder) and shown the next time.
//
// For the UI tests (app/COMMANDS.md, "Logs that UI tests read"): "Project
// browser <title>: reading the projects", "Project browser <title>: 2
// projects: Shop (local) | Team (cloud)", "Project browser starts at <key>
// (remembered)" ("(current project)", "(first project)"), "Project browser
// shows <place key>: sub/ | gear.mitcad v2, changed" ("empty"), "Project
// browser search "<text>" in all projects: 1 found: Team/bolts/hex.mitcad"
// ("in <project key>", "0 found"), "Project browser search scope: all
// projects" ("this project"), "Project browser read-only: on" ("off"),
// "Project browser selected <key>", the
// places of what can be clicked (debug, when they moved): "Project browser
// folder <key> at x,y" and "Project browser item <key> at x,y" (rows in
// sight), "Project browser path <place key> at x,y", "Project browser
// search at x,y", "Project browser scope at x,y", "Project browser button
// <Open|Cancel|Another File|Read-only> at x,y" (main window coordinates);
// and at the end "Project browser <title>: chose <path>[ (read-only)]"
// ("another file[ (read-only)]", "cancelled").

#include <optional>

#include <QString>

class QWidget;

namespace mitcad {

struct ProjectBrowserOptions {
  QString title;        // the window's title
  QString acceptText;   // the accept button's text ("&Open")
  QString otherText;    // the system dialog's button ("A&nother File..."; "" none)
  QString readOnlyText; // the read-only check ("&Read-only"; "" none)
  // Where it starts when no place is remembered, or the remembered one is
  // gone: this project and folder ("" the first project).
  QString root;
  QString folder;
  // The edit locks' session of this run (whose locks are this window's).
  QString session;
};

struct ProjectBrowserResult {
  bool other = false;    // Another File...: the caller asks with the system's dialog
  bool readOnly = false; // the read-only check was on
  bool file = false;     // another file of the project (mitcad#2204): for the system's app
  QString root;          // the chosen design's project
  QString path;          // the chosen design's file (absolute, '/')
};

// The window, modal to `parent`; none when cancelled.
std::optional<ProjectBrowserResult> browseProjects(QWidget* parent, const ProjectBrowserOptions& options);

} // namespace mitcad
