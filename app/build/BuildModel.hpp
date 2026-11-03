// SPDX-License-Identifier: MIT
#pragma once

// What the BUILD commands (mitcad#375) read of the model, through its
// queries only (commands.md, "BUILD: profile assemblies"): occurrence
// paths and placements, the BUILD parts and junctions, picks as `at`
// fields, and the frames and lengths of `build_preview`'s answers.

#include <limits>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include <TopoDS_Shape.hxx>
#include <gp_Ax3.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../framework/Command.hpp"

namespace mitcad::build {

// --- Components and occurrence paths (uids from the root, "O1/O4")

// Where a path places its component in the design; identity for "".
std::optional<gp_Trsf> worldOf(const QString& path, const CommandContext& context);
// The component a path ends in ("" the root's).
QString componentAt(const QString& path, const CommandContext& context);

// The component BUILD places parts in (the `build_host` query, mitcad#394:
// the active one when it takes components, else the nearest one up that
// does; in a new design its active Component1, mitcad#421): its uid, name
// and path from the root ("" for the root). The commands' paths are from
// it and their points in its coordinates.
struct Host {
  QString component = QStringLiteral("C0");
  QString name;
  QString path;
};
Host buildHost(const CommandContext& context);
// An item's path from the host ("" for its own geometry); none when the
// item is not inside it.
std::optional<QString> pathFromHost(const SelectionItem& item, const CommandContext& context);
// A path from the host as a path from the root.
QString rootPath(const QString& fromHost, const CommandContext& context);
// A point of the design in the host's coordinates.
gp_Pnt inHost(const gp_Pnt& world, const CommandContext& context);

// --- Picks

// The least snap distance of a click on the screen (mitcad#417): the
// placements' `at.snap` is this many pixels of the view at the click.
constexpr double kSnapPixels = 12.0;
// Where a click (or the hover) hit an item, as `recordPick` keeps it:
// {item (its describe()), point (in the design), snap (mm, when the view
// tells its pixel size there)}; empty without the item's `at`.
QJsonObject pickRecord(const SelectionItem& item, const CommandContext& context);
// Keeps where a click hit the item of a selection input (in the design),
// under "<key>_pick", for `pickPoint` and `pickSnap`.
void recordPick(const QString& key, const SelectionItem& item, CommandState& state, const CommandContext& context);
// The snap distance kept with the input's first item's click, in mm.
std::optional<double> pickSnap(const QString& key, const CommandState& state);
// Where the input's first item was picked, in the design: the click kept,
// else the middle of the item (an item given otherwise, as live MCP does).
std::optional<gp_Pnt> pickPoint(const QString& key, const CommandState& state, const CommandContext& context);
// `at` of build_place and build_panel for the input's first item:
// `occurrence` (from the BUILD host), `geometry`, `point` (in the
// host's coordinates). None, with the reason, when it cannot
// be given. A plane whose pick has no point (mitcad#1196: chosen, the
// click that says where still to come) gives no `point`: the model takes
// the plane's origin.
std::optional<QJsonObject> atOf(const QString& key, const CommandState& state, const CommandContext& context,
                                QString& problem);
// An origin or construction plane's frame in the design (mitcad#1196: the
// first part on a chosen plane): the `datum` query's, moved by the item's
// occurrence; none for another item or when the model cannot say.
std::optional<gp_Ax3> planeOf(const SelectionItem& item, const CommandContext& context);
// The item a reference of the model's answers stands for in the view
// (`{"occurrence", "geometry"}`, the occurrence from the BUILD
// host): faces, edges and bodies.
std::optional<SelectionItem> itemOfTarget(const QJsonObject& target, const CommandContext& context);

// --- BUILD parts (the build_parts query)

struct BuildPart {
  QString component;
  QString name;
  QString role;
  QString source;
  QString designation;
  QString supplier;
  QString partNumber;
  QString config;
  QString addedBy;
  double length = std::numeric_limits<double>::quiet_NaN();
  QJsonObject ends;
  QJsonObject bound; // {parameter, to, mode}, empty when the length is a number
  QJsonArray adds;
  // The holes machined into its ends: [{end, hole: {diameter, depth, thread}}].
  QJsonArray machining;
  // As build_parts gives them, when it does (else false and 0).
  bool flip = false;
  int rotate = 0;
  // Timber (mitcad#478): `timber` for a member, its grade, the lead of the
  // spacing set it is in (empty: none) and, of a lead, the set's
  // {definition, count}.
  QString kind;
  QString grade;
  QString set;
  QJsonObject spacing;
  QJsonObject raw;

  bool isTimber() const { return kind == QStringLiteral("timber"); }

  QString shownName() const { return name.isEmpty() ? designation : name; }
};

QVector<BuildPart> buildParts(const CommandContext& context);
// Whether the design has BUILD parts (false when the model cannot say).
bool hasBuildParts(const CommandContext& context);
// The BUILD part an item of the view belongs to: the component its
// occurrence places.
std::optional<BuildPart> partOf(const SelectionItem& item, const CommandContext& context);
// The BUILD parts of items, each once.
QVector<BuildPart> partsOf(const Selection& items, const CommandContext& context);
// The junctions (the build_junctions query), empty when there are none.
QJsonArray junctions(const CommandContext& context);
// Where a junction is in the design: its part A's frame there.
std::optional<gp_Pnt> junctionPoint(const QJsonObject& junction);
// "L junction of Part1 and Part2", with ", connected" when it has connectors.
QString junctionText(const QJsonObject& junction, const CommandContext& context);

// --- Answers of build_preview

struct Frame {
  gp_Pnt origin;
  gp_Dir x = gp_Dir(1, 0, 0);
  gp_Dir z = gp_Dir(0, 0, 1);
  bool valid = false;
};
// A frame of the replies: {"origin", "x_axis", "z_axis"} in the design (a
// part's z from end A to end B); invalid without origin and z.
Frame frameOf(const QJsonValue& value);
// The millimetres of `length` (a number, or an object with `value`).
double lengthOf(const QJsonValue& value);
// A preview when the model gives no shape: the section along the frame's
// z, from its origin.
TopoDS_Shape boxAlong(const Frame& frame, double width, double height, double length);
QJsonArray pointJson(const gp_Pnt& point);

} // namespace mitcad::build
