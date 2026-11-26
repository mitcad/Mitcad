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
// The learner's sketch input one interaction at a time (mitcad#2440): a
// sketch on a given plane (XY, XZ, YZ, a construction plane, a face), the
// next point of a 2D sketch's drawing tool placed where it snapped (the
// origin, an axis, a point, a curve, given coordinates), a value typed into
// a drawing tool's value box (a rectangle's width). Input that misses is
// held back and is a "mismatch" event.
//
// What a step is and when it is done, in plain Qt Core
// (app/tests/guide_step.cpp); MainWindowGuide.cpp shows it and watches the
// window.

#include <QJsonObject>
#include <QPointF>
#include <QString>
#include <QStringList>

namespace mitcad {

// Where a 2D sketch's drawing tool is about to place a point (mitcad#2440):
// what it snapped to, as guidePointPlaces() names it (sketch::snapPlaces:
// "origin", "x_axis", "point", "grid", ...), and where, in the sketch's
// coordinates (mm).
struct GuidePoint {
  QStringList places;
  QPointF at;
};

struct GuideStep {
  // mitcad#2370: WorkspaceSelector, Workspace (by its name), Tab (by its
  // name) and Timeline.
  // mitcad#2440: Plane (the view, where the plane to sketch on is picked),
  // Field (a drawing tool's value box by its key).
  enum class Target { Command, Input, Browser, View, WorkspaceSelector, Workspace, Tab, Timeline, Plane, Field };
  // mitcad#2369: Navigate (the view orbited, panned or zoomed).
  // mitcad#2370: Opened (the workspace selector's list), Chosen (a
  // workspace), Activated (a tab), None (no `done`: the step is shown until
  // the page or MCP ends it, as for the timeline).
  // mitcad#2440: Plane (a sketch made on the plane), Point (a drawing
  // tool's point placed where it snapped), Field (a value typed into a
  // drawing tool's value box).
  enum class Done { Finished, Started, Value, Pick, Mode, Navigate, Opened, Chosen, Activated, None, Plane, Point, Field };

  QString id;   // the author's name of the step, echoed in its events
  QString text; // the callout's text
  Target target = Target::Command;
  // The target: a command id (Command), an input's key (Input), a Browser
  // row's name (Browser), a workspace's or a tab's name as the step gives it
  // (MainWindow resolves it to the toolbar's key: "DESIGN", "SOLID"), a
  // plane's name as the step gives it (Plane), a drawing tool's value box's
  // key (Field); empty for the view, the workspace selector and the
  // timeline.
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
  // and the tab an Activated step wait for (by name, as targetName), the
  // plane a Plane step waits for, the value box of a Field step.
  QString doneName;
  // The value a Value or a Field step waits for, as typed ("20 mm").
  QString value;
  // A Point step (mitcad#2440): the places the point may snap to
  // (guidePointPlaces(), any of them), or, with pointAtGiven, the point's
  // coordinates in the sketch (mm, within kGuidePointTolerance).
  QStringList pointPlaces;
  bool pointAtGiven = false;
  QPointF pointAt;
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
  // Whether the step takes picks in the view (a View or a Plane target or a
  // Pick step).
  bool takesPicks() const { return target == Target::View || target == Target::Plane || done == Done::Pick; }
  // Whether a pick of `kind` (Selection::kindName) is one it waits for.
  bool pickMatches(const QString& kind) const;

  // The plane a sketch must be made on during the step (mitcad#2440): its
  // target's or its done's, as the step gives it ("XY", "Plane1", "face");
  // empty when it names none.
  QString plane() const;
  // Whether a sketch on the plane called `names` (guide names of one
  // plane, MainWindow's: "xy"; a construction plane's uid and name; "face"
  // and a face's reference) is one the step takes: any when it names no
  // plane. Case does not matter.
  bool planeMatches(const QStringList& names) const;
  // Whether a Point step waits for `point`: one of its places, or its
  // coordinates. False for any other step.
  bool pointMatches(const GuidePoint& point) const;
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
// The places a Point step's point may snap to (mitcad#2440): origin,
// x_axis, y_axis, point (a point of the sketch, the origin's too), center,
// midpoint, quadrant, intersection, curve (along a curve of the sketch),
// extension (on a line's extension), body_vertex, body_midpoint, body_edge,
// body_center, body_section, grid, free (nothing).
const QStringList& guidePointPlaces();
// How far a point may lie from a Point step's coordinates, in mm.
inline constexpr double kGuidePointTolerance = 1e-3;
// The plane names a Plane step may give beyond a construction plane's and
// a face's reference (mitcad#2440): xy, xz, yz (the origin's planes) and
// face (any face).
const QStringList& guideOriginPlanes();

} // namespace mitcad
