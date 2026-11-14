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
// MODIFY, in the menu's sections (mitcad#1778): Press Pull, Fillet,
// Chamfer, Shell, Draft; Offset Face, Move Face, Replace Face, Split Face,
// Delete Face; Combine, Split Body, Scale, Convert Mesh; Move/Copy, Align;
// Dogbone, Mortise and Tenon; Material, Appearance. The main window adds
// Edit Appearances to the last and Edit Parameters and Configurations as
// the "parameters" section.
void registerModifyCommands(CommandRegistry& registry, const CommandContext& context);
// CONSTRUCT: every construction plane, axis and point, in the sections
// "planes", "axes" and "points" (mitcad#1778).
void registerConstructCommands(CommandRegistry& registry, const CommandContext& context);
// ASSEMBLE (mitcad#55), in the menu's sections (mitcad#1778): Joint,
// As-Built Joint, Joint Origin, Rigid Group, Assembly Constraint; Drive
// Joint, Motion Link, Contact Set. New Component comes first
// (registerCreateCommands); Animate Joint and Motion Study end "motion",
// Insert Component and Insert from Library are the last section (the main
// window).
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

// SHEET METAL (mitcad#210), in the menu's sections (mitcad#1778): Sheet
// Rules and the main window's `editRules` (Edit Sheet Rules); Face,
// Flange; Bend, Fold, Corner Relief; Corner Round, Corner Chamfer, Sheet
// Cut; Unfold, Refold, Flat Pattern and the main window's `showFlat` (Show
// Flat Pattern). The main window adds Export Flat Pattern (the Make tab).
void registerSheetMetalCommands(CommandRegistry& registry, const CommandContext& context,
                                const CommandDef& editRules, const CommandDef& showFlat);

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

// SURFACE (mitcad#455, SurfaceCommands.cpp), in the menu's sections
// (mitcad#1778): Surface Extrude, Surface Revolve, Surface Sweep, Surface
// Loft; Patch, Ruled Surface, Boundary Surface, Blend Surface; Extend,
// Trim, Untrim, Offset Surface; Match Surface, Fair Surface, Reverse
// Normal; Stitch, Unstitch, Thicken.
void registerSurfaceCommands(CommandRegistry& registry, const CommandContext& context);

// Redefine Sketch Plane (mitcad#787, SketchPlaneCommands.cpp): a sketch
// moved to another plane or face and placed there, from the sketch's
// context menus; in the sketch `controller` edits it always takes that
// sketch.
void registerSketchPlaneCommands(CommandRegistry& registry, sketch::SketchController& controller);

} // namespace mitcad
