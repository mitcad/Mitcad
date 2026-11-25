// SPDX-License-Identifier: MIT
#pragma once

// A guide step of the in-app tutorials (mitcad#2012, docs/tutorials.md
// "Guide steps"): one engine for a tutorial page's bridge (window.mitcad)
// and live MCP's ui_guide. A step names its target as the UI names it (a
// command id at its toolbar button or menu item, an input of the open
// command's panel by its key, a Browser row by its name, the view with a
// pick filter), shows a short callout text beside it, allows its own
// commands and holds back the rest, and is done by what the user does (a
// command finished or its panel opened, a value typed into an input, a pick
// of a kind, the view orbited, panned or zoomed, mitcad#2369). Never
// pixels: a step says what, the application finds where.
// The toolbar's own places (mitcad#2370): the workspace selector, a
// workspace of its list and a tab, each done when the selector opened, the
// workspace was chosen or the tab activated; the timeline, a place to point
// at only. A step without `done` is shown until the page or MCP ends it.
//
// What a step is and when it is done, in plain Qt Core
// (app/tests/guide_step.cpp); MainWindowGuide.cpp shows it and watches the
// window.

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

struct GuideStep {
  // mitcad#2370: WorkspaceSelector, Workspace (by its name), Tab (by its
  // name) and Timeline.
  enum class Target { Command, Input, Browser, View, WorkspaceSelector, Workspace, Tab, Timeline };
  // mitcad#2369: Navigate (the view orbited, panned or zoomed).
  // mitcad#2370: Opened (the workspace selector's list), Chosen (a
  // workspace), Activated (a tab), None (no `done`: the step is shown until
  // the page or MCP ends it, as for the timeline).
  enum class Done { Finished, Started, Value, Pick, Mode, Navigate, Opened, Chosen, Activated, None };

  QString id;   // the author's name of the step, echoed in its events
  QString text; // the callout's text
  Target target = Target::Command;
  // The target: a command id (Command), an input's key (Input), a Browser
  // row's name (Browser), a workspace's or a tab's name as the step gives it
  // (MainWindow resolves it to the toolbar's key: "DESIGN", "SOLID"); empty
  // for the view, the workspace selector and the timeline.
  QString targetName;
  // The kinds a pick in the view may be of ("face", "edge"; Selection's
  // kindName), for a View target or a Pick step; empty: any.
  QStringList pickKinds;
  // The commands it allows beyond its own (the target's and the done
  // command's): "allow".
  QStringList allowed;
  // View navigation (the wheel, the middle and right buttons, the camera
  // commands): allowed unless the step says "navigation": false.
  bool navigation = true;
  Done done = Done::Finished;
  // The command a Finished or Started step waits for, the input of a
  // Value step (of the open command), the mode a Mode step waits for
  // (guideModes()), the navigation a Navigate step waits for
  // (guideNavigationActions(): orbit, pan, zoom), what an Opened step
  // waits to open (kGuideWorkspaceSelector), the workspace a Chosen step
  // and the tab an Activated step wait for (by name, as targetName).
  QString doneName;
  // The value a Value step waits for, as typed ("20 mm").
  QString value;
  // "hold": true (mitcad#2368): once done, the window's model-changing input
  // stays held (no command but Leave Tutorial and the camera's, no picks in
  // the view, even with a tool open) until the next step starts, the page
  // releases it, or the tutorial is left or closed.
  bool hold = false;

  // The step from its JSON (docs/tutorials.md "Guide steps"); empty `id`
  // and the reason in `error` when it is not one. Its commands are not
  // checked here (MainWindow knows them).
  static GuideStep parse(const QJsonObject& json, QString* error);
  QJsonObject toJson() const;

  // The command ids it names (target, done), for MainWindow's check that
  // they exist.
  QStringList commands() const;
  // Whether `command` (an id) may run during the step: its own, the ones it
  // allows, Leave Tutorial, and the camera's while navigation is allowed.
  bool allowsCommand(const QString& command) const;
  // Whether `command` may run during the hold after the step is done
  // (mitcad#2368): Leave Tutorial, and the camera's while navigation is
  // allowed; none of the step's own.
  bool allowsCommandWhileHeld(const QString& command) const;
  // Whether the step takes picks in the view (a View target or a Pick step).
  bool takesPicks() const { return target == Target::View || done == Done::Pick; }
  // Whether a pick of `kind` (Selection::kindName) is one it waits for.
  bool pickMatches(const QString& kind) const;
};

// The window's modes a step can wait for: idle, pick_plane, sketch, command.
const QStringList& guideModes();
// The camera's commands, allowed while a step allows navigation (the
// arrow keys' turns among them).
const QStringList& guideNavigationCommands();
// The navigations of the view a step can wait for (mitcad#2369): orbit,
// pan, zoom, each as the active navigation scheme binds it.
const QStringList& guideNavigationActions();
// Whether a value typed matches the one a step waits for: the same text
// without spaces and case, a length's "mm" (the default unit) optional
// ("20" is "20 mm" and "20mm").
bool guideValueMatches(const QString& typed, const QString& wanted);
// The command that leaves a tutorial, always allowed.
inline constexpr char kLeaveTutorialCommand[] = "help.leave_tutorial";
// What an Opened step waits to open (mitcad#2370): the workspace
// selector's list.
inline constexpr char kGuideWorkspaceSelector[] = "workspace_selector";

} // namespace mitcad
