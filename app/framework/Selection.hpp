// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <functional>
#include <optional>

#include <QFlags>
#include <QHashFunctions>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

struct DocumentSnapshot;

// What can be picked in the 3D view; the selection filters.
enum class SelectKind : unsigned {
  None = 0,
  Face = 1u << 0,
  Edge = 1u << 1,
  Vertex = 1u << 2,
  Body = 1u << 3,
  Profile = 1u << 4,
  SketchCurve = 1u << 5,
  SketchPoint = 1u << 6,
  Plane = 1u << 7, // origin and construction planes
  Axis = 1u << 8,  // origin and construction axes
  Point = 1u << 9, // the origin and construction points
  Component = 1u << 10,
  // Sketch mode (U2): picked from the glyphs and values drawn over the
  // view, not by the view itself. A fixed entity's lock is "fix:<entity>".
  SketchConstraint = 1u << 11,
  SketchDimension = 1u << 12,
  // The browser and the timeline (U3): a whole sketch, picked there rather
  // than in the view.
  Sketch = 1u << 13,
  // A timeline feature (U4: patterns and mirrors of features), picked in
  // the timeline or through a face it made.
  Feature = 1u << 14,
};
Q_DECLARE_FLAGS(SelectFilter, SelectKind)
Q_DECLARE_OPERATORS_FOR_FLAGS(SelectFilter)

constexpr SelectFilter kConstructionGeometry =
    SelectKind::Plane | SelectKind::Axis | SelectKind::Point;

// A picked entity, named as the model names it (core/model-api/src/commands.md).
struct SelectionItem {
  SelectKind kind = SelectKind::None;
  // Body uid (F2.b0) of faces, edges, vertices and bodies; sketch uid of
  // profiles, sketch curves, points, constraints, dimensions and sketches;
  // datum uid (xy, x, origin, F5); component uid (C1) of components.
  QString owner;
  // Face, edge or vertex name, region key, curve, point, constraint or
  // dimension id; empty for bodies and datums.
  QString name;
  // Surface or curve type for finer filters: plane, cylinder, cone, sphere,
  // torus, line, circle, arc, ellipse, other.
  QString geometry;
  // The occurrence that places the item in the design (components, F6):
  // the uids of the occurrences from the root, "O1/O4"; empty in the root
  // component. A body of a component placed twice is two items.
  QString occurrence = QString();
  // Where a click hit it, in its component's coordinates (U4: hole
  // positions, primitives placed by a click); not part of its identity.
  std::optional<std::array<double, 3>> at = std::nullopt;

  bool isValid() const { return kind != SelectKind::None; }
  bool operator==(const SelectionItem& other) const {
    return kind == other.kind && owner == other.owner && name == other.name &&
           occurrence == other.occurrence;
  }
  bool operator!=(const SelectionItem& other) const { return !(*this == other); }

  // How a feature definition refers to it ("References to geometry").
  QJsonValue reference() const;
  // "Face F2:end(...) of F2.b0": the kind, the internal name and the owner's
  // id, for log lines, keys and agents only (mitcad#1741). Never in text a
  // person sees: pickListText or sketchItemCard say it in words
  // (docs/ui-conventions.md, "What a text says"); tools/check-all.sh's
  // grep keeps it out of tr() texts.
  QString logDescription() const;
  // The feature that created the entity (F3 of F3:side(c1)), or empty.
  QString creatingFeature() const;

  // The kind's English id ("face", with a count other than 1 "faces"), for
  // log lines, keys and the API: the same in every interface language.
  static QString kindName(SelectKind kind, int count = 1);
  // The kind as user-facing text, in the interface language (mitcad#1471):
  // "face", with a count other than 1 "faces" (the plural without a count,
  // as in "pick the body's own faces").
  static QString kindText(SelectKind kind, int count = 1);
  // A count and the kind, in the interface language: "1 face", "2 faces".
  static QString countText(SelectKind kind, int count);
};

using Selection = QVector<SelectionItem>;

// For sets of items (mitcad#1631: a window over a large design picks tens
// of thousands of edges; QVector::contains for each made that quadratic).
// Hashes what operator== compares.
inline size_t qHash(const SelectionItem& item, size_t seed = 0) noexcept {
  return qHashMulti(seed, static_cast<unsigned>(item.kind), item.owner, item.name, item.occurrence);
}

// The curve of an Intersection Curve (mitcad#1405, mitcad#1417): a feature
// item whose geometry is "curve" (featureCurveGeometry), as the view's drawn
// curve, its Browser row and its timeline entry give it. Inputs that take
// it rather than whole features (the sketch's Project and Intersect, Rib,
// Web) tell it from other features by this.
QString featureCurveGeometry();
SelectionItem featureCurveItem(const QString& feature, const QString& occurrence = QString());
bool isFeatureCurve(const SelectionItem& item);

// A fixed plane of a definition (`{"origin", "normal"[, "x_axis"]}`,
// core/model-api/src/commands.md "References to geometry") as an item
// (mitcad#1137): a plane owned by "fixed" whose name is its reference as
// compact JSON, so that `reference()` gives it back unchanged. Nothing in
// the view picks one; a placement row shows a definition's fixed plane so.
SelectionItem fixedPlaneItem(const QJsonObject& plane, const QString& occurrence = QString());
bool isFixedPlane(const SelectionItem& item);
// Whether a definition's reference is a fixed plane.
bool isFixedPlaneRef(const QJsonValue& ref);

// Order in which a window selection prefers kinds: it keeps one kind.
SelectKind preferredKind(const Selection& items);

// Selection inputs keep each model item once (mitcad#199). A profile is a
// region of a sketch: the occurrence that places the sketch in the view is
// not part of the region, and a region keeps the key a definition stored
// for it after the sketch changed while the view names it by its current
// key. A resolver names the regions an item means now (the model's
// `profile` query's `regions`, the main one first: several when a sketch
// edit split the region, mitcad#407), empty when it cannot tell.
using ItemResolver = std::function<QStringList(const SelectionItem&)>;

// Whether two items are one entry of a selection input: equal, or profiles
// of the same sketch with the same key or keys the resolver resolves to
// the same regions.
bool sameEntry(const SelectionItem& a, const SelectionItem& b, const ItemResolver& resolve = {});
// Whether `item` is in the selection: one of its entries, or a piece of a
// profile entry whose region a sketch edit split (every region the item
// means is one the entry means, mitcad#410).
bool containsEntry(const Selection& items, const SelectionItem& item,
                   const ItemResolver& resolve = {});
// Takes `item` out: the entries that are it go, and an entry it is a piece
// of keeps its other pieces, under their own current keys in its place
// (mitcad#410). How many entries went or changed.
int removeEntry(Selection& items, const SelectionItem& item, const ItemResolver& resolve = {});
// A click on `item`: takes out the entries that are it, else appends it.
// True when it was appended. Clicking the same region twice gives back the
// selection before, whatever the order of the clicks.
bool toggleEntry(Selection& items, const SelectionItem& item, const ItemResolver& resolve = {});
// A list given as a whole (a definition loaded for editing, the selection
// made before a command): each profile once, the first entry kept in its
// place (a piece of a split region after its entry goes); other items as
// they are.
Selection uniqueProfiles(const Selection& items, const ItemResolver& resolve = {});

// Text such as "2 edges, 1 face", in English for log lines.
QString summarize(const Selection& items);
// The same in the interface language, for user-facing text (mitcad#1471).
QString summarizeText(const Selection& items);

// Compact rich text for one item returned by the model's measure query.
// Missing properties are omitted; circular edges include radius and diameter.
QString selectionMeasurementText(const QJsonObject& measured);

// Rich-text owner lines for a body or one of its faces, edges and vertices.
// Occurrence names include their full assembly path; root bodies omit it.
QString selectionOwnerText(const SelectionItem& item, const DocumentSnapshot& snapshot);

// An item of the view's pick list in words (mitcad#1703), in the interface
// language: its occurrence's names, its owner's name and its kind,
// "Bracket:1: Body2: Edge", "Sketch2: Profile", "Origin: XY"; never a
// topological name or an id (a name not in the snapshot is left out).
QString pickListText(const SelectionItem& item, const DocumentSnapshot& snapshot);

// A selected profile, sketch curve, point, constraint or dimension on the
// idle card (mitcad#1741), in the interface language: its kind and where it
// is (`title`: "Profile of Sketch3", "Line of Sketch3", "Horizontal
// constraint in Sketch1"), and what can be measured (`measurements`, rich
// text as selectionMeasurementText's: a profile's area and perimeter, a
// line's length, a circle's or an arc's radius and diameter, a dimension's
// value). `sketch` is the `sketch` query's answer for the item's sketch
// (empty when it could not be asked), `sketchName` the sketch's name
// (empty: unknown) and `constraintName` a constraint type's name
// (sketch::constraintName). Never the region key, an item's id or a
// coordinate.
struct SketchItemCard {
  QString title;
  QString measurements;
};
SketchItemCard sketchItemCard(const SelectionItem& item, const QJsonObject& sketch, const QString& sketchName,
                              const std::function<QString(const QString&)>& constraintName);

} // namespace mitcad
