// SPDX-License-Identifier: MIT
#pragma once

#include <array>
#include <optional>

#include <QFlags>
#include <QJsonValue>
#include <QString>
#include <QVector>

namespace mitcad {

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

// A picked entity, named as the model names it (core/model/src/api/commands.md).
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

// Order in which a window selection prefers kinds: it keeps one kind.
SelectKind preferredKind(const Selection& items);

// Text such as "2 edges, 1 face".
QString summarize(const Selection& items);

} // namespace mitcad
