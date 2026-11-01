// SPDX-License-Identifier: MIT
#pragma once

// Registration of the command definitions (app/COMMANDS.md), one function
// per family of the toolbar.

#include <functional>

#include <QString>
#include <QStringList>

#include "../framework/Selection.hpp"

class QWidget;

namespace mitcad {

class CommandContext;
class CommandRegistry;
class DocumentHost;
struct CommandDef;

namespace sketch {
class SketchController;
}

// The SOLID tab's CREATE group (U4), in menu order: Create Sketch (the
// main window's, given here), Extrude, Revolve, Sweep, Loft, Rib, Web,
// Hole, Thread, the primitives, Coil, Pipe, the patterns and Mirror; also
// New Component, which leads the ASSEMBLE group. `context` answers the
// definitions' questions about the model while the application runs.
void registerCreateCommands(CommandRegistry& registry, const CommandContext& context,
                            const CommandDef& createSketch);
// MODIFY: Press Pull, Fillet, Chamfer, Shell, Draft, Scale, Combine,
// Offset Face, Replace Face, Split Face, Split Body, Move/Copy, Align,
// Delete, Appearance. Materials are the main window's Edit Materials
// (mitcad#655).
void registerModifyCommands(CommandRegistry& registry, const CommandContext& context);
// CONSTRUCT: every construction plane, axis and point.
void registerConstructCommands(CommandRegistry& registry, const CommandContext& context);
// ASSEMBLE (mitcad#55): Joint, As-Built Joint, Joint Origin, Rigid Group,
// Drive Joint. New Component comes first (registerCreateCommands); Animate
// Joint, Insert Component and Insert from Library last (the main window).
void registerAssembleCommands(CommandRegistry& registry, const CommandContext& context);
// INSPECT: Measure, Interference, Section Analysis, Print Analysis
// (mitcad#620) and Physical Properties (a dialog over `window` for the
// bodies `selection` gives).
void registerInspectCommands(CommandRegistry& registry, const CommandContext& context,
                             QWidget* window, std::function<Selection()> selection);

// The SKETCH tab (U2): drawing tools, the sketch dimension, constraints and
// the modify commands, run in the sketch `controller` edits.
void registerSketchCommands(CommandRegistry& registry, sketch::SketchController& controller);
// The constraint commands' ids, in the sketch palette's order.
QStringList sketchConstraintCommands();

// SHEET METAL (mitcad#210): Face, Flange, Bend, Fold, Corner Round, Corner
// Chamfer, Sheet Cut, Corner Relief, Unfold, Refold, Flat Pattern and Sheet Rules; the main
// window adds Edit Sheet Rules, Show Flat Pattern and Export Flat Pattern.
void registerSheetMetalCommands(CommandRegistry& registry, const CommandContext& context);

// The BUILD tab (mitcad#375, BuildCommands.cpp): PROFILE (Profile, Panel,
// Linear Component), TIMBER (Timber, Wall, Floor / Roof; mitcad#520), CONNECT
// (Connector, Fasteners, Swap Fastener), MODIFY (Length, Ends, Flip, Rotate
// 90 deg, Spacing Set, Timber Frame), OUTPUT (Cut List, Parts List). `host` runs their model
// commands; the lists' dialogs open over `window` with the design's name.
void registerBuildCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host,
                           QWidget* window, std::function<QString()> designName);

// Gear (mitcad#472, GearCommands.cpp), at the end of the SOLID tab's CREATE
// group: `host` links the joints of gears it meshes.
void registerGearCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host);

// Pulley (mitcad#562, PulleyCommands.cpp), after Gear at the end of the
// SOLID tab's CREATE group.
void registerPulleyCommands(CommandRegistry& registry, const CommandContext& context);

// Bearing (mitcad#577, BearingCommands.cpp): printable bearings, after
// Pulley in the CREATE group's Machine Elements.
void registerBearingCommands(CommandRegistry& registry, const CommandContext& context);

// Equal spacing in sketches (mitcad#221, EqualSpacingCommands.cpp): Equal
// Spacing, Equal Spacing Dimension and Edit Equal Spacing. Sketch Dimension
// with three or more points, circles, arcs or lines selected opens Equal
// Spacing Dimension instead of its tool (true then).
void registerEqualSpacingCommands(CommandRegistry& registry, sketch::SketchController& controller);
bool dimensionEqualSpacing(sketch::SketchController& controller);

// SURFACE (mitcad#455, SurfaceCommands.cpp): Surface Extrude, Surface
// Revolve, Surface Sweep, Surface Loft, Patch, Stitch, Thicken and the
// surface features that make and change surface bodies.
void registerSurfaceCommands(CommandRegistry& registry, const CommandContext& context);

// Redefine Sketch Plane (mitcad#787, SketchPlaneCommands.cpp): a sketch
// moved to another plane or face and placed there, from the sketch's
// context menus; in the sketch `controller` edits it always takes that
// sketch.
void registerSketchPlaneCommands(CommandRegistry& registry, sketch::SketchController& controller);

} // namespace mitcad
