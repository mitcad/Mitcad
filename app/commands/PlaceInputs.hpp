// SPDX-License-Identifier: MIT
#pragma once

// Placement by picked geometry (mitcad#656; commands.md, "Placement by
// picked geometry"): the panel inputs every part placed by a pick shares
// (Pulley, Gear, Bearing, Coil, Box, Cylinder, Sphere, Torus, Hole), and
// the definition's `at` they make. One pick says where the part goes:
//
//   at         Place On: a shaft's cylindrical face or circular edge, a
//              hole's wall or rim, a construction or origin axis, a sketch
//              line, circle or arc (`at.axis`); a sketch point, a vertex, a
//              construction point (`at.point`); a click on a planar face or
//              a plane (`at.face`), which snaps to the face's vertices,
//              centres and edge midpoints;
//   at_edge1, at_offset1, at_edge2, at_offset2
//              on a face: the two lines the point is measured from (the
//              nearest straight edges of a body's face, the origin axes on
//              an origin plane) and its distances from them, in-view
//              dimensions with value boxes; a click on another line (an
//              edge, a sketch line or an axis along the face) while Place On
//              takes picks makes it the reference it runs with most nearly;
//   at_offset, at_flip
//              along the axis or the normal, and turned round;
//   at_angle, at_align
//              where the part's turn matters (a gear's phase, a keyway, a
//              box): a ring with its value box, and a line its x axis runs
//              along;
//   bore_clearance
//              on a shaft or in a hole: added to its diameter for the bore.
//
// A shaft of another component than the feature's is a `seat` with its
// occurrence link, as the model takes it. Definitions written before
// (`plane` and `center`, a box's `corner`, `seat`) load into the same
// inputs: a plane with distances from the origin axes; saving keeps them
// as they were until the placement changes, then writes `at`. A fixed
// plane (`{"origin", "normal"[, "x_axis"]}`, written by importers and
// agents) is Place On's item of its own, "Plane at Z 36 mm (fixed)", drawn
// in the view (mitcad#1137): saved unchanged while the placement is, moved
// on it as `at` on the same plane, replaced by a face or a plane picked.

#include <functional>
#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QVector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../framework/Command.hpp"
#include "CommandSupport.hpp"

namespace mitcad::cmd {

enum class PlaceForm { None, Axis, Point, Face };

struct PlacePanel {
  // The command's name, for the log lines ("Place Pulley: ...").
  QString command;
  // Whether the inputs show in a state (a gear meshing with a rack is
  // placed by the rack); always when unset.
  std::function<bool(const CommandState&)> offered;
  // Whether a pick is needed; else the command places the part without
  // one (a gear by its mate).
  bool required = true;
  // Whether Angle and Align show in a state; never when unset.
  std::function<bool(const CommandState&)> turns;
  // Offset and Flip along the axis (a hole has its own Flip).
  bool along = true;
  // Bore Clearance on a shaft or in a hole, and its printed-fit feature.
  bool clearance = false;
  QString printedFit;
};

// The inputs, for a command's input list.
QVector<InputDef> placeInputs(const PlacePanel& panel);
// For a command's init: where the new feature's component is seen, and a
// pick made before the command placed as a click in its middle would
// place it; nothing is placed without a pick (mitcad#656).
void initPlace(CommandState& state, const CommandContext& context, const PlacePanel& panel);
// The form of the placement picked (None without one, or when the panel
// does not offer it).
PlaceForm placeForm(const CommandState& state, const PlacePanel& panel = {});
// On a body's round face or edge (a shaft or a hole): the placement has a
// diameter, the bore of a pulley or gear follows it.
bool onShaft(const CommandState& state, const PlacePanel& panel = {});
// Takes the placement away (a gear then placed by its mate).
void clearPlace(CommandState& state);
// The definition's `at` (or `seat` for another component's shaft) and
// `bore_clearance`; a failure (false) with the input to pick when the
// placement is not complete.
bool addPlace(QJsonObject& def, const CommandState& state, const CommandContext& context, const PlacePanel& panel,
              Built& error);
// The `at` of the inputs (empty without one, or for a linked seat); with
// `numbers` the values as numbers, for queries while they are typed.
QJsonObject atOf(const CommandState& state, const CommandContext& context, const PlacePanel& panel = {},
                 bool numbers = false);
// Keeps an earlier definition's own placement fields (`plane` and
// `center`, `belt_with`, `mesh_with` with its angle) beside the `at` the
// inputs now make of them; addPlace writes the fields unchanged while the
// inputs make that `at` (mitcad#656: old forms saved as they were).
void keepEarlierForm(CommandState& state, const CommandContext& context, const PlacePanel& panel,
                     const QJsonObject& fields);
// Those fields while the placement is unchanged; empty otherwise.
QJsonObject keptEarlierForm(const CommandState& state, const CommandContext& context, const PlacePanel& panel = {});
// Fills the inputs from a definition: its `at`, a `seat`, or `plane` and
// `center` (`corner` with `cornerKey`); false when it has none of them. A
// `seat` of the feature's own component and `plane` and `center` are kept
// as they were (keepEarlierForm).
bool loadPlace(const QJsonObject& feature, CommandState& state, const CommandContext& context,
               const PlacePanel& panel, const QString& cornerKey = QStringLiteral("center"));
// Places the inputs on a plane or planar face (`plane`, a reference) at a
// point of it, measured from the origin axes (an earlier definition's
// position, a hole's); false when it is no plane.
bool placeOnPlaneAt(CommandState& state, const CommandContext& context, const QJsonValue& plane,
                    const gp_Pnt& point);
// Logs where the inputs place the part ("Place <command>: ..."), as a pick
// and loadPlace do: for a placement set otherwise (placeAtFrame,
// placeOnPlaneAt).
void logPlacement(const CommandState& state, const CommandContext& context, const PlacePanel& panel);
// Whether the panel can show a definition's placement (none counts): its
// references, a fixed plane among them (mitcad#1137); not a fixed axis or
// point.
bool placeShowable(const QJsonObject& feature, const CommandContext& context);
// Where the placement puts the part: its frame at the timeline marker
// (the model's `place_frame`, with the inputs' values as numbers); none
// while it does not resolve.
std::optional<PlaneFrame> placeFrame(const CommandState& state, const CommandContext& context,
                                     const PlacePanel& panel = {});
// Where the placement's coordinates (the feature's component's: the
// point, the references, placeFrame) are seen in the design: the
// occurrence the face, edge or shaft was picked in, or where the
// feature's component is seen; none in the root (mitcad#1743). A command
// placed by a pick gives it as CommandDef::handlesSeenAt, so that its
// handles are drawn at the part, not where the component's coordinates
// would put them.
std::optional<gp_Trsf> placeSeenAt(const CommandState& state, const CommandContext& context);
// The plane and the centre in its coordinates for definitions that keep
// them (a rack, a bevel pair), from a placement on a face or a plane;
// none for another form.
std::optional<std::pair<QJsonValue, QJsonArray>> planeCenterOf(const CommandState& state,
                                                              const CommandContext& context);
// The direction of Reference 1 on the face, as an angle about the face's
// normal from its x axis (radians), its side toward the point on the
// left: a rack runs along it (mitcad#656). 0 on an origin plane and for
// another form.
double referenceAngle(const CommandState& state, const CommandContext& context);
// What a panel's note says of a part on a shaft from its values query
// (`bore_diameter`, `shaft_diameter`): "bore 8.2 mm (shaft 8 mm)"; empty
// when it has no shaft.
QString shaftNote(const QJsonObject& values, const CommandContext& context);
// The shaft's or hole's seat as the model gives it (`place_frame`'s
// `seat`, or `seat_frame` for a linked seat): `diameter`, `hole`, ...;
// empty when not on a shaft.
QJsonObject shaftSeat(const CommandState& state, const CommandContext& context, const PlacePanel& panel = {});
// The occurrence link of a body's face or edge picked in another
// component's occurrence than the active component's (`source`, and
// `target` when the active component is not the root), as a seat takes
// it; empty for the active component's own geometry and for axes, points
// and sketch curves (Gear Pair and Belt Drive, mitcad#754).
QJsonObject occurrenceLinkOf(const CommandContext& context, const SelectionItem& item);

// --- Where earlier definitions put their part (mitcad#656: belt_with and
// mesh_with.angle opened again, a placement for the Belt Drive query)

struct BuiltFrame {
  gp_Pnt origin;
  gp_Dir normal = gp_Dir(0, 0, 1);
  gp_Dir x = gp_Dir(1, 0, 0);
  // The plane the part lies on when its placement names one (`plane`, a
  // mate's `plane`); null otherwise.
  QJsonValue plane;
};
// The frame of a pulley's, gear's or other part's definition at the
// timeline marker: `at`, `seat`, `plane` and `center`, `belt_with` and
// `mesh_with` (`angle`) through their mates; `uid` is the feature's own
// (its values); none when it cannot be told.
std::optional<BuiltFrame> builtFrame(const QJsonObject& def, const QString& uid, const CommandContext& context,
                                     int depth = 0);
// Places the inputs at a frame: on its plane (or the origin plane square
// to its normal, offset along it), measured from the origin axes; false
// when no plane fits (a slanted normal without a plane).
bool placeAtFrame(CommandState& state, const CommandContext& context, const BuiltFrame& frame);
// An `at` that puts a part where a definition put it (for queries that
// take placements: Belt Drive's check of a pulley made before); empty when
// it cannot be told.
QJsonObject atOfFeature(const QJsonObject& def, const QString& uid, const CommandContext& context);
// Evaluates a definition's value slot (a number, or a parameter's name or
// an expression) as a length or an angle ("length", "angle"); NaN when it
// does not evaluate.
double evaluatedSlot(const QJsonValue& slot, const QString& owner, const QString& kind, const CommandContext& context);

} // namespace mitcad::cmd
