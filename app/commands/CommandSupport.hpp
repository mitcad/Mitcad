// SPDX-License-Identifier: MIT
#pragma once

// What the feature command families share (U4): references between the
// model's JSON and picked items, operations, values, and the geometry that
// places manipulators and primitives.

#include <optional>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QVector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "../framework/Command.hpp"

namespace mitcad::cmd {

using Choices = QVector<QPair<QString, QString>>;

QString str(const QJsonObject& object, const char* key);
// The label of a choice's value.
QString label(const Choices& choices, const QString& value);
QString number(double value);
QString degrees(double radians);

// --- References and selections

QJsonArray refsOf(const Selection& items);
Selection profilesOf(const QJsonArray& refs);
QJsonArray bodyUids(const Selection& items);
Selection bodiesOf(const QJsonArray& uids);
// Names of the items of one kind (faces, edges, sketch curves).
QJsonArray names(const Selection& items, SelectKind kind);
// The one sketch of the items; empty (with an error) when there are several.
QString oneSketch(const Selection& items, Built& error, const QString& input);
// The body of faces, edges or vertices, which must all be of one body.
QString oneBody(const Selection& items, Built& error, const QString& input);
// An item for a reference of the model's JSON (commands.md, "References to
// geometry"): a datum, a face, edge or vertex, a body, a profile, a sketch
// curve or point; invalid for fixed geometry, which no input shows.
SelectionItem itemOf(const QJsonValue& ref, const CommandContext& context);
// Items for a list of references; empty when one cannot be shown.
std::optional<Selection> itemsOf(const QJsonArray& refs, const CommandContext& context);
// A path (sweeps, pipes, rails): sketch curves of one sketch or edges of
// one body; `chain` adds the curves joined to them.
QJsonValue pathOf(const Selection& items, bool chain, Built& error, const QString& input);
// The items of a path, and whether it chains; none for a fixed line.
std::optional<Selection> pathItems(const QJsonValue& path, const CommandContext& context,
                                   bool* chain = nullptr);
// Whether a value is a reference itemOf can show.
bool isShowable(const QJsonValue& ref, const CommandContext& context);

// --- Kinds of geometry

bool isLine(const SelectionItem& item);
// A datum plane or a planar face.
bool isPlanar(const SelectionItem& item);
// An axis: a datum axis, a straight edge or sketch line, a cylinder, cone
// or torus face, or a circular edge (through its centre).
bool isAxial(const SelectionItem& item);
// A point: the origin or a construction point, a vertex, a sketch point.
bool isPointLike(const SelectionItem& item);
bool isCircular(const SelectionItem& item); // circles and arcs (edges, sketch curves)
constexpr SelectFilter kPlaneKinds = SelectKind::Plane | SelectKind::Face;
constexpr SelectFilter kAxisKinds =
    SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve | SelectKind::Face;
constexpr SelectFilter kPointKinds =
    SelectKind::Point | SelectKind::Vertex | SelectKind::SketchPoint;
constexpr SelectFilter kPathKinds = SelectKind::SketchCurve | SelectKind::Edge;

// --- Operations of features that make or change bodies

extern const Choices kOperations;
// The operation choice and the participants ("objects") it works on.
QVector<InputDef> operationInputs(bool newComponent = true);
void addOperation(QJsonObject& def, const CommandState& state);
void loadOperation(const QJsonObject& def, CommandState& state);

// --- Objects that extents end at (extrudes, revolves, holes)

// The inputs "object<suffix>" (a plane, face or body), how a face is
// reached ("object<suffix>_extend": the selected face or the adjacent
// faces) and a body ("object<suffix>_body": to it or through it).
QVector<InputDef> objectInputs(const QString& suffix, std::function<bool(const CommandState&)> shown);
// The model's <object>: a plane, a face or a body (commands.md, "extrude").
QJsonObject objectOf(const CommandState& state, const QString& suffix);
bool loadObject(const QJsonObject& object, CommandState& state, const QString& suffix,
                const CommandContext& context);
bool objectShowable(const QJsonObject& object, const CommandContext& context);

// --- Values

// Fills a value input from a definition's parameter slot (null: left as is).
void loadValue(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context);
// Whether the value is not zero (a taper of 0 makes no parameter).
bool nonZero(const CommandState& state, const QString& key);
// A count, factor or fraction for a slot the model reads as a length
// (pattern quantities, scale factors, construction paths' fractions): the
// number itself, or the expression when it names parameters.
QJsonValue countValue(const CommandState& state, const QString& key);
// Fills such an input: the feature's own parameter as its number, a
// borrowed one by name.
void loadCount(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context);

// --- The model

void newestProfile(CommandState& state, const CommandContext& context);
bool hasProfiles(const CommandContext& context);
bool hasBodies(const CommandContext& context);
bool hasSketches(const CommandContext& context);

// --- Geometry (model coordinates, placed by occurrences)

// The middle of the items' bounding box.
std::optional<gp_Pnt> centerOf(const Selection& items, const CommandContext& context);
// A plane through an item: a datum plane, a planar face (outward normal)
// or a profile (its sketch's normal).
struct PlaneFrame {
  gp_Pnt origin;
  gp_Dir x = gp_Dir(1, 0, 0);
  gp_Dir y = gp_Dir(0, 1, 0);
  gp_Dir normal = gp_Dir(0, 0, 1);
};
std::optional<PlaneFrame> frameOf(const SelectionItem& item, const CommandContext& context);
// An axis through an item (isAxial).
std::optional<std::pair<gp_Pnt, gp_Dir>> axisOf(const SelectionItem& item,
                                                 const CommandContext& context);
// The normal of the profiles' sketch at the profiles' middle.
std::optional<std::pair<gp_Pnt, gp_Dir>> profileNormal(const Selection& profiles,
                                                       const CommandContext& context);
// An arrow manipulator along a direction from a point.
Manipulator arrow(const gp_Pnt& origin, const gp_Dir& direction, double base = 0.0);
// A ring manipulator about an axis.
Manipulator ring(const gp_Pnt& origin, const gp_Dir& axis);
// A length in the document's unit, as a value input's text ("12.5 mm").
QString lengthText(double millimetres, const CommandContext& context);
// Where a click on a plane or planar face hit, in the plane's coordinates
// (the frame a sketch on it has), into the inputs `x` and `y`: primitives
// and coils are placed by a click.
void placeAt(const SelectionItem& item, CommandState& state, const CommandContext& context,
             const QString& x, const QString& y);
// The plane's point at plane coordinates (x, y) of the state.
std::optional<gp_Pnt> pointOnPlane(const CommandState& state, const CommandContext& context,
                                   const QString& plane, const QString& x, const QString& y);

} // namespace mitcad::cmd
