// SPDX-License-Identifier: MIT
#pragma once

// The Sketch page of Preferences (mitcad#860): "Scale the sketch to its
// first dimension", on by default. While it is on, a sketch action's
// `sketch.add_dimension` asks the model for the first-dimension rule
// (`scale_sketch`, commands.md "Scaling a sketch"): the first driving
// dimension of a sketch drawn out of proportion scales the whole sketch to
// it, in the same undo step.

#include <QJsonObject>

namespace mitcad::sketch {

// The preference (settings "sketch/scaleToFirstDimension"). Never saved:
// on, unless MITCAD_SKETCH_FIRST_DIMENSION_SCALE is 0 (the UI test
// libraries set it, so that their drawn sketches keep their sizes).
bool scaleToFirstDimension();
void saveScaleToFirstDimension(bool on);
// No saved value (Preferences › Restore Defaults, mitcad#1887): the
// default above applies again.
void resetScaleToFirstDimension();

// `command` with `scale_sketch` when it is a `sketch.add_dimension` and the
// preference is on; any other command as it is.
QJsonObject withFirstDimensionScale(const QJsonObject& command);

// "Click adds to the selection in sketches" (mitcad#1403, settings
// "sketch/clickAddsToSelection"), off by default: while it is on, a click
// in sketch mode with no tool or command running adds to the selection or
// removes from it, as Ctrl+click does; a click on empty space or Esc
// clears it, and a right click on what is not selected adds it. Picks of
// sketch tools and commands are unchanged.
bool clickAddsToSelection();
void saveClickAddsToSelection(bool on);

// "Esc leaves an unchanged sketch" (mitcad#1623, settings
// "sketch/escLeavesUnchangedSketch"), off by default: while it is on, Esc
// with nothing else to cancel in a sketch nothing was changed in leaves it
// as Finish Sketch does (MainWindow::escapeSketch); while it is off, Esc in
// a sketch only cancels and clears the selection.
bool escLeavesUnchangedSketch();
void saveEscLeavesUnchangedSketch(bool on);

// "Show profiles when a sketch opens" (mitcad#1670, GitHub
// mitcad/Mitcad#255, settings "sketch/showProfilesOnOpen"), on by default:
// each sketch opens with the palette's Show profiles as it says
// (SketchController::enter); the palette's checkbox still turns the shading
// on or off for that sketch only.
bool showProfilesOnOpen();
void saveShowProfilesOnOpen(bool on);

// "Project the face's edges into a sketch on a face" (mitcad#2051, GitHub
// mitcad/Mitcad#426, settings "sketch/projectFaceOnCreate"), on by default:
// Create Sketch on a planar face of a body asks `sketch.create` for
// `project_face`, so the sketch starts with the face's outline as a linked
// projection, in the same undo step. Unsaved, off while
// MITCAD_SKETCH_PROJECT_FACE is 0 (the UI test libraries set it, so that
// their sketches on faces start empty as their steps expect).
bool projectFaceOnCreate();
void saveProjectFaceOnCreate(bool on);
// No saved value (Restore Defaults): the default above applies again.
void resetProjectFaceOnCreate();

// "Hide a sketch when a feature uses it" (mitcad#2393, GitHub
// mitcad/Mitcad#652, settings "sketch/hideUsedSketches"), on by default
// (the model's rule of mitcad#7). Off: a shown planar sketch that a feature
// starts to use keeps its light bulb on, set by the model in that feature's
// undo step; sketches already hidden stay hidden. The model reads it for
// the whole process (set_hide_used_sketches in the bridge), which
// applyHideUsedSketches tells it: at start, and whenever the setting is
// saved, reset or imported.
bool hideUsedSketches();
void saveHideUsedSketches(bool on);
void applyHideUsedSketches();

} // namespace mitcad::sketch
