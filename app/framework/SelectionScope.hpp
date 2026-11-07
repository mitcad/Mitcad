// SPDX-License-Identifier: MIT
#pragma once

// The three-state selection filter of sketch mode (mitcad#217): per
// category of geometry, whether a click or a window can select it. Plain Qt
// Core, so that the rules are tested without a view (app/tests).
//
// Each row cycles empty -> square -> cross -> empty:
//   empty  (Off)    cannot be selected;
//   square (Click)  a direct click selects it;
//   cross  (Window) a click and a window select it.
// Shift held over a click selects what empty rows hold as well (scope
// expansion); a window never goes beyond the filter.
//
// A scope is one context of rows: the sketch's (`SelectionScope::sketch`,
// active while a sketch is edited), Project's (`project`, while the
// Project command runs in the sketch) and Intersect's (`intersect`).
// Other sketches' curves and points have a row only in Project's and
// Intersect's (mitcad#904, mitcad#1031). Body edges have two rows, visible and
// occluded: which one governs an edge depends on whether the part of it the
// click or the window hit is in sight (`EdgeSight`, from the view).

#include <array>
#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QString>
#include <QVector>

#include "Selection.hpp"

namespace mitcad {

enum class FilterState { Off, Click, Window };

// empty -> square -> cross -> empty.
FilterState nextFilterState(FilterState state);
// "empty", "square", "cross" (logs and tests).
QString filterStateName(FilterState state);
// What the state means, for tooltips and accessible descriptions.
QString filterStateMeaning(FilterState state);

enum class FilterRow {
  SketchGeometry,       // the active sketch's ordinary curves and points
  ConstructionGeometry, // ... its construction curves and points
  ProjectedGeometry,    // ... its projected (reference) curves and points
  Annotations,          // ... its dimensions and constraints
  Profiles,             // ... its profile regions
  VisibleEdges,         // body edges, the part hit in sight
  OccludedEdges,        // body edges, the part hit behind faces
  Faces,
  Vertices,
  Bodies,
  Components,
  ReferenceGeometry, // planes, axes and points (origin and construction)
  // Other sketches' curves and points (mitcad#904), and the curves of
  // Intersection Curves (mitcad#1417, isFeatureCurve): only Project's and
  // Intersect's scopes have the row (mitcad#1031); everywhere else they
  // are never selectable.
  OtherSketchGeometry,
};
constexpr int kFilterRowCount = 13;

// "sketch-geometry", "visible-edges", ... (logs, tests, action names).
QString filterRowId(FilterRow row);
std::optional<FilterRow> filterRowFromId(const QString& id);
// "Sketch geometry", "Visible body edges", ... (the control's rows).
QString filterRowLabel(FilterRow row);
// The kinds the view picks for a row (none for annotations, which sketch
// mode picks from its overlay).
SelectFilter filterRowKinds(FilterRow row);

// Which part of a body edge a pick hit: in sight, behind faces, or (a
// window over an edge partly hidden) both.
enum class EdgeSight { Visible, Occluded, Both };

// How an item is being picked, for the scope's decision.
struct PickCase {
  bool window = false;   // a window selection, else a click (or its hover)
  bool expanded = false; // Shift held over a click: empty rows count too
  // For body edges: the part hit. Asked only when the visible and the
  // occluded rows decide differently (it costs a look along the view).
  std::function<EdgeSight()> sight = {};
};

class SelectionScope {
public:
  SelectionScope() = default;
  SelectionScope(const QString& name, const std::vector<std::pair<FilterRow, FilterState>>& defaults);

  // Sketch mode's rows with their defaults (the ticket's table): sketch
  // geometry, construction and projected geometry cross; dimensions and
  // constraints, profiles and visible body edges square; the rest empty.
  static SelectionScope sketch();
  // Project's source rows: visible edges cross, occluded edges empty, faces
  // and vertices square, other sketches' curves and points cross
  // (mitcad#904).
  static SelectionScope project();
  // Intersect's (mitcad#1031): as Project's without vertices, with bodies
  // square (a body is cut whole), other sketches' curves and points cross.
  static SelectionScope intersect();

  const QString& name() const { return m_name; }
  // The rows in the control's order.
  const QVector<FilterRow>& rows() const { return m_rows; }
  bool has(FilterRow row) const;
  // Off for a row the scope does not have.
  FilterState state(FilterRow row) const;
  FilterState defaultState(FilterRow row) const;
  // Ignored for a row the scope does not have.
  void setState(FilterRow row, FilterState state);
  // The row's next state, which it then has.
  FilterState cycle(FilterRow row);
  void reset();
  bool isDefault() const;

  // Whether a pick may take an item of the row: a window only on a cross;
  // a click unless the row is empty, or with Shift (expanded) also then.
  // Shift does not expand whole bodies where faces are offered: a click on
  // a body picks its face, as outside sketch mode.
  bool allows(FilterRow row, bool window, bool expanded) const;
  // A body edge: by the part of it hit (how.sight, asked only when the
  // visible and occluded rows decide differently).
  bool allowsEdge(const PickCase& how) const;
  // The kinds the view must make pickable for clicks: those of rows that a
  // click may take (expanded: with Shift held).
  SelectFilter kinds(bool expanded) const;
  // "sketch-geometry=cross, ..." in the rows' order (logs and tests).
  QString describe() const;

private:
  QString m_name;
  QVector<FilterRow> m_rows;
  std::array<FilterState, kFilterRowCount> m_states{};
  std::array<FilterState, kFilterRowCount> m_defaults{};
};

// The row of the active sketch's own curves and points (ordinary,
// construction or projected), from the sketch model.
using SketchRowOf = std::function<FilterRow(const SelectionItem&)>;

// The row that governs an item; none when no row may let it be picked:
// another sketch's profiles, dimensions and constraints, kinds the view
// does not offer. Another sketch's curves and points give
// OtherSketchGeometry, which only Project's and Intersect's scopes have
// (mitcad#904, mitcad#1031). Body
// edges give VisibleEdges (the scope decides with allowsEdge).
std::optional<FilterRow> filterRowOf(const SelectionItem& item, const QString& activeSketch,
                                     const SketchRowOf& sketchRow);

// The scope's decision for an item picked so.
bool scopeAccepts(const SelectionScope& scope, const SelectionItem& item, const PickCase& how,
                  const QString& activeSketch, const SketchRowOf& sketchRow);

// The selection filter context a command's selections pick through in
// sketch mode, by its id: Project's ("sketch.project") and Intersect's
// ("sketch.intersect"); none for others,
// whose inputs' own filters decide.
std::optional<SelectionScope> commandSelectionScope(const QString& commandId);

} // namespace mitcad
