// SPDX-License-Identifier: MIT
#pragma once

// Registration of the command definitions (app/COMMANDS.md), one function
// per family of the toolbar.

#include <functional>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

#include <gp_Trsf.hxx>

#include "../framework/Selection.hpp"

class QWidget;

namespace mitcad {

class CommandContext;
class CommandState;
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
// As-Built Joint, Joint Origin, Rigid Group, Constraint, Constraint Set; Drive
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

// Derived Component (mitcad#1812, DerivedPartCommands.cpp): the edit panel
// of a derived part (a BUILD part, a library part or a linked design
// inserted with options), opened only by editing its feature; `host` runs
// its build_edit or set_component_parameters and opens a linked source.
void registerDerivedPartCommands(CommandRegistry& registry, DocumentHost& host);

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

// Auto Constrain in sketches (mitcad#2019, AutoConstrainCommands.cpp): the
// relations the geometry has, then dimensions for what is left free.
void registerAutoConstrainCommands(CommandRegistry& registry, sketch::SketchController& controller);

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

// Forms (mitcad#1757, FormCommands.cpp; app/COMMANDS.md "Form"): Edit
// Form, the panel a Form feature is edited in (its cage's faces, edges and
// vertices picked in the view, moved, rotated and scaled with handles,
// faces extruded, edge loops inserted and deleted, edges creased, symmetry
// set; each edit one undo step), with what the main window gives it.
struct FormEditHooks {
  // The Form being edited (its uid, F3); empty while none is.
  std::function<QString()> form;
  // A `form.*` command with `preview` run on the model: its answer (the
  // edited cage and its limit mesh); throws when the model refuses it.
  std::function<QJsonObject(const QJsonObject&)> preview;
  // Where the Form's component is seen in the design; none: the root's.
  std::function<std::optional<gp_Trsf>()> placement;
  // The cage drawn and picked over the view (the `form_cage` answer), the
  // picks (FormCage.hpp's pickedText), the kind a click picks ("faces",
  // "edges", "vertices") and whether clicks pick at all.
  std::function<void(const QJsonObject& cage, const QString& picked, const QString& kind, bool picking)> show;
  // The panel ended: the cage is no longer drawn.
  std::function<void()> ended;
};
// Edit Form's id: the main window opens it for a Form feature
// (MainWindow::editForm) and passes the view's picks to its hidden input
// "picked" and a vertex's drag to "drag" (DocumentHost::chooseInCommand).
inline const char* const kFormEditCommand = "solid.form_edit";
// Fill Hole and Bridge (mitcad#1941): actions while Edit Form is open (the
// view's context menu, the command search) that choose the edit and apply
// it to the picks (MainWindowForm.cpp).
inline const char* const kFormFillHoleCommand = "solid.form_fill_hole";
inline const char* const kFormBridgeCommand = "solid.form_bridge";
void registerFormEditCommand(CommandRegistry& registry, const CommandContext& context, DocumentHost& host,
                             FormEditHooks hooks);

// Flex (mitcad#1713, FlexCommands.cpp): a wire, cable, hose or strip laid
// through the model by clicks; after Envelope in the CREATE group, in a
// section of its own.
void registerFlexCommands(CommandRegistry& registry, const CommandContext& context, DocumentHost& host);
// A Flex item's edit session (mitcad#1713, owner 14.11.2026): its panel
// with the item's handles in the view beside it (flex/FlexEditController).
// Whether the Flex panel's state edits an item; its nodes as the handles
// take them ({"type": "flex", "nodes": [...]}, each with its id, `at`,
// `on` or `port` and `twist`; empty when it edits none); and the nodes the
// handles changed put into the panel (a node's reference may say where it
// was picked with `occurrence` and `context`), false when it edits none or
// fewer than two nodes are given.
bool isFlexEditSession(const CommandState& state);
QJsonObject flexSessionDefinition(const CommandState& state);
bool setFlexSessionNodes(CommandState& state, const CommandContext& model, const QJsonArray& nodes);

// Let It Hang (mitcad#1713, GravityCommands.cpp; the ASSEMBLE group): the
// panel of a Flex item let hang, with the design's direction of gravity
// (Direction From, Reverse, Clear), the item's length, the damping and its
// end pins. The main window registers it (MainWindowGravity.cpp) with
// what drives the hanging item in the view (gravity/GravityController).
struct LetItHangHooks {
  // The Flex item (its uid) a pick is of; empty when it is of none.
  std::function<QString(const SelectionItem&)> flexOf;
  // The panel's definition made so (gravity::HangWish's JSON; an empty
  // object: the inputs are not complete, the item hangs on as it is);
  // empty when done, else the words of what was refused.
  std::function<QString(const QJsonObject&)> apply;
  // The Clear button: gravity.clear (the item put back first).
  std::function<void()> clear;
  // OK: the shape kept; false when the model refused it (the panel stays).
  std::function<bool()> lock;
  // The panel opened (its undo depth noted) and ended, with whether OK
  // ended it.
  std::function<void()> opened;
  std::function<void(bool committed)> ended;
};
CommandDef letItHangCommand(const CommandContext& context, LetItHangHooks hooks);
// The panel's Start pinned and End pinned set (a click on an end's handle
// in the view); false when they were so already.
bool setLetItHangEnds(CommandState& state, bool start, bool end);

// Sheet parts by material and process (mitcad#1939, SheetProcessCommands.cpp):
// Finger Joint, T-Nut Slot, Kerf Bend, Flat Pack, Glue Flap and Standard Box,
// each offered by the part's process profile.
void registerSheetProcessCommands(CommandRegistry& registry, const CommandContext& context);

} // namespace mitcad
