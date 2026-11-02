// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <functional>
#include <optional>

#include <QFlags>
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
  // "Face F2:end(...) of F2.b0", for logs and tooltips.
  QString describe() const;
  // The feature that created the entity (F3 of F3:side(c1)), or empty.
  QString creatingFeature() const;

  static QString kindName(SelectKind kind, int count = 1);
};

using Selection = QVector<SelectionItem>;

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

// Text such as "2 edges, 1 face".
QString summarize(const Selection& items);

// Compact rich text for one item returned by the model's measure query.
// Missing properties are omitted; circular edges include radius and diameter.
QString selectionMeasurementText(const QJsonObject& measured);

// Rich-text owner lines for a body or one of its faces, edges and vertices.
// Occurrence names include their full assembly path; root bodies omit it.
QString selectionOwnerText(const SelectionItem& item, const DocumentSnapshot& snapshot);

} // namespace mitcad
