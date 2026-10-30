// SPDX-License-Identifier: MIT
#pragma once

// What the feature command families share (U4): references between the
// model's JSON and picked items, operations, values, and the geometry that
// places manipulators and primitives.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QVector>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "../framework/Command.hpp"
#include "../framework/Json.hpp"

namespace mitcad::cmd {

using Choices = QVector<QPair<QString, QString>>;

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

// --- Objects of other components (mitcad#762)

// An object picked in another occurrence than `here`, the one the
// feature's profiles are seen at: its body's link into `links` (the
// model's `object_links`, commands.md "extrude"), `{"source": <its
// occurrence>, "target": <here>}` with an empty path (the root) left out,
// as Combine's tools in occurrences. Planes, and faces and bodies picked
// where the profiles are, need none. False when the body is linked from
// another occurrence already (one link per body).
bool addObjectLink(QJsonObject& links, const SelectionItem& item, const QString& here);
// Where the profiles (or a feature's other picks of its own sketch) are
// seen: the first pick's occurrence, else the first occurrence path to
// its sketch's component, as the model's links take it; empty in the
// root.
QString profilesOccurrence(const Selection& profiles, const CommandContext& context);
// A loaded face or body object back in the occurrence its link names.
void linkObject(SelectionItem& item, const QJsonObject& links);
// An edited feature's links: their target (where the links saw its
// profiles) kept in the state for linksOccurrence(); nothing without links.
void keepLinksTarget(CommandState& state, const QJsonObject& links);
// Where the profiles are seen for a feature's links: an edited feature's
// kept target while that occurrence places the profiles' sketch's
// component (the session resets a profile's occurrence to the first one),
// else profilesOccurrence().
QString linksOccurrence(const Selection& profiles, const CommandState& state, const CommandContext& context);
// The component an occurrence path ("O1/O4") places, "C0" for the root's
// empty path; empty when the path leads nowhere.
QString occurrenceComponent(const QString& path, const CommandContext& context);

// --- Construction planes of other components (mitcad#772)

// The component of a picked construction plane (its feature's, "C0" for the
// root's); empty for an origin plane, anything but a plane, or a plane the
// model does not know.
QString planeComponent(const SelectionItem& item, const CommandContext& context);
// Why an extent's object or a start plane cannot be used by a feature of the
// profiles' sketch's component: a construction plane of another component,
// which the model reads only in its own component and no link carries (the
// links carry bodies, `addObjectLink`); empty when it can be used.
QString foreignPlane(const SelectionItem& item, const Selection& profiles, const CommandContext& context);

// --- Values

// Fills a value input from a definition's parameter slot (null: left as is).
void loadValue(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context);
// The expression negated (a Flip): "5 mm" gives "-5 mm", "-5 mm" gives
// "5 mm", an expression "-(d1 / 2)".
QString negated(const QString& expression);
// Whether the value is not zero (a taper of 0 makes no parameter).
bool nonZero(const CommandState& state, const QString& key);
// A definition's slot that is missing, null or a literal 0: a field the model
// works out, shown empty as Auto (mitcad#656).
bool zeroOrNone(const QJsonValue& slot);
// Fills a count, factor or fraction input (a unitless slot, mitcad#662;
// built with `state.expression`): a number as it is, the feature's own
// parameter as its expression, a borrowed one by name.
void loadCount(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context);

// --- The model

void newestProfile(CommandState& state, const CommandContext& context);
bool hasProfiles(const CommandContext& context);
bool hasBodies(const CommandContext& context);
bool hasSketches(const CommandContext& context);

// --- Components and occurrence paths (uids from the root, "O1/O4")

// The `components` query's answer; throws like the query.
QJsonObject componentsOf(const CommandContext& context);
// A component's name; its uid when it is not known.
QString componentName(const QString& uid, const CommandContext& context);
// Calls `visit` with each occurrence of the `components` query's tree and
// its path of uids from the root; no occurrences when the query fails.
void eachOccurrence(const CommandContext& context,
                    const std::function<void(const QJsonObject&, const QString&)>& visit);

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
// The normal of the profiles' sketch at the profiles' middle, both in the
// design: moved by the occurrences the profiles are seen at (mitcad#958).
std::optional<std::pair<gp_Pnt, gp_Dir>> profileNormal(const Selection& profiles,
                                                       const CommandContext& context);
// An arrow manipulator along a direction from a point.
Manipulator arrow(const gp_Pnt& origin, const gp_Dir& direction, double base = 0.0);
// A ring manipulator about an axis.
Manipulator ring(const gp_Pnt& origin, const gp_Dir& axis);
// A length in the document's unit, as a value input's text ("12.5 mm").
QString lengthText(double millimetres, const CommandContext& context);
// A belt's tension travel (mitcad#656): how far and which way the axes
// move to fit the belt ("1 mm apart", "6 mm closer", "0 mm").
QString travelText(double millimetres, const CommandContext& context);
// A length rounded to 0.1 mm for the belt's values (mitcad#656).
QString shortLength(double millimetres, const CommandContext& context);
// A belt as it is bought (mitcad#656): its profile, length and width,
// "GT2 200 mm, 6 mm wide", or with its teeth when the width is not known
// ("T5 400 mm, 80 teeth"); `profile` the model's (`gt2_2mm`).
QString beltText(const QJsonObject& belt, const QString& profile, const CommandContext& context);
// The model's belt verdict (`fit`) as a word: "fits", "needs a tensioner",
// "belt too short"; empty without one.
QString beltFitText(const QString& fit);
// A belt's pitch line round two pulleys (mitcad#656): round the first
// (centre `first`, axis `normal`, pitch diameter `d1`) and the second
// (`second`, brought into the first's plane, `d2`), both wraps and the
// two spans; empty when the circles overlap or a diameter is not known.
std::vector<gp_Pnt> beltPitchLine(const gp_Pnt& first, const gp_Dir& normal, double d1, const gp_Pnt& second,
                                  double d2);

// --- Splitting and trimming tools (Split Face, Split Body, Trim): the input
// `tool` (one plane, face or body, or sketch curves of one sketch), its
// plane's `offset`, `extend`, and the direction curves are swept along
// (`split_type`, `dx`, `dy`, `dz`).

// The tool's reference; an error (into `error`) for a mixed selection.
QJsonValue toolOf(const Selection& items, Built& error, const QString& input);
// The items of a tool reference; none when one is gone.
std::optional<Selection> toolItems(const QJsonValue& tool, const CommandContext& context);
// The tool input (labelled "Splitting Tool" without a label).
InputDef toolInput(const QString& label = QString());
InputDef toolOffsetInput();
QVector<InputDef> directionInputs();
// The tool, its plane's offset, extension and direction into a definition.
bool addTool(QJsonObject& def, const CommandState& state, Built& error);
void loadTool(const QJsonObject& feature, CommandState& state, const CommandContext& context);
bool toolEditable(const QJsonObject& feature, const CommandContext& context);

} // namespace mitcad::cmd
