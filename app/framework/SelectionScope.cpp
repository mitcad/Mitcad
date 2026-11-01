// SPDX-License-Identifier: MIT
#include "SelectionScope.hpp"

#include <cstddef>

#include <QStringList>

namespace mitcad {
namespace {

constexpr std::array<FilterRow, kFilterRowCount> kAllRows = {
    FilterRow::SketchGeometry, FilterRow::ConstructionGeometry, FilterRow::ProjectedGeometry,
    FilterRow::Annotations,    FilterRow::Profiles,             FilterRow::VisibleEdges,
    FilterRow::OccludedEdges,  FilterRow::Faces,                FilterRow::Vertices,
    FilterRow::Bodies,         FilterRow::Components,           FilterRow::ReferenceGeometry,
    FilterRow::OtherSketchGeometry};

std::size_t indexOf(FilterRow row) { return static_cast<std::size_t>(row); }

bool isSketchKind(SelectKind kind) {
  switch (kind) {
  case SelectKind::Profile:
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
  case SelectKind::SketchConstraint:
  case SelectKind::SketchDimension:
    return true;
  default:
    return false;
  }
}

} // namespace

FilterState nextFilterState(FilterState state) {
  switch (state) {
  case FilterState::Off:
    return FilterState::Click;
  case FilterState::Click:
    return FilterState::Window;
  case FilterState::Window:
    break;
  }
  return FilterState::Off;
}

QString filterStateName(FilterState state) {
  switch (state) {
  case FilterState::Off:
    return QStringLiteral("empty");
  case FilterState::Click:
    return QStringLiteral("square");
  case FilterState::Window:
    break;
  }
  return QStringLiteral("cross");
}

QString filterStateMeaning(FilterState state) {
  switch (state) {
  case FilterState::Off:
    return QStringLiteral("cannot be selected (Shift+click still selects it)");
  case FilterState::Click:
    return QStringLiteral("a click selects it");
  case FilterState::Window:
    break;
  }
  return QStringLiteral("a click and a window select it");
}

QString filterRowId(FilterRow row) {
  switch (row) {
  case FilterRow::SketchGeometry:
    return QStringLiteral("sketch-geometry");
  case FilterRow::ConstructionGeometry:
    return QStringLiteral("construction-geometry");
  case FilterRow::ProjectedGeometry:
    return QStringLiteral("projected-geometry");
  case FilterRow::Annotations:
    return QStringLiteral("annotations");
  case FilterRow::Profiles:
    return QStringLiteral("profiles");
  case FilterRow::VisibleEdges:
    return QStringLiteral("visible-edges");
  case FilterRow::OccludedEdges:
    return QStringLiteral("occluded-edges");
  case FilterRow::Faces:
    return QStringLiteral("faces");
  case FilterRow::Vertices:
    return QStringLiteral("vertices");
  case FilterRow::Bodies:
    return QStringLiteral("bodies");
  case FilterRow::Components:
    return QStringLiteral("components");
  case FilterRow::OtherSketchGeometry:
    return QStringLiteral("other-sketch-geometry");
  case FilterRow::ReferenceGeometry:
    break;
  }
  return QStringLiteral("reference-geometry");
}

std::optional<FilterRow> filterRowFromId(const QString& id) {
  for (const FilterRow row : kAllRows) {
    if (filterRowId(row) == id) {
      return row;
    }
  }
  return std::nullopt;
}

QString filterRowLabel(FilterRow row) {
  switch (row) {
  case FilterRow::SketchGeometry:
    return QStringLiteral("Sketch geometry");
  case FilterRow::ConstructionGeometry:
    return QStringLiteral("Construction geometry");
  case FilterRow::ProjectedGeometry:
    return QStringLiteral("Projected geometry");
  case FilterRow::Annotations:
    return QStringLiteral("Dimensions and constraints");
  case FilterRow::Profiles:
    return QStringLiteral("Profiles");
  case FilterRow::VisibleEdges:
    return QStringLiteral("Visible body edges");
  case FilterRow::OccludedEdges:
    return QStringLiteral("Occluded body edges");
  case FilterRow::Faces:
    return QStringLiteral("Body faces");
  case FilterRow::Vertices:
    return QStringLiteral("Body vertices");
  case FilterRow::Bodies:
    return QStringLiteral("Bodies");
  case FilterRow::Components:
    return QStringLiteral("Components");
  case FilterRow::OtherSketchGeometry:
    return QStringLiteral("Other sketches' curves and points");
  case FilterRow::ReferenceGeometry:
    break;
  }
  return QStringLiteral("Planes, axes and points");
}

SelectFilter filterRowKinds(FilterRow row) {
  switch (row) {
  case FilterRow::SketchGeometry:
  case FilterRow::ConstructionGeometry:
  case FilterRow::ProjectedGeometry:
  case FilterRow::OtherSketchGeometry:
    return SelectKind::SketchCurve | SelectKind::SketchPoint;
  case FilterRow::Annotations:
    return SelectFilter();
  case FilterRow::Profiles:
    return SelectKind::Profile;
  case FilterRow::VisibleEdges:
  case FilterRow::OccludedEdges:
    return SelectKind::Edge;
  case FilterRow::Faces:
    return SelectKind::Face;
  case FilterRow::Vertices:
    return SelectKind::Vertex;
  case FilterRow::Bodies:
    return SelectKind::Body;
  case FilterRow::Components:
    return SelectKind::Component;
  case FilterRow::ReferenceGeometry:
    break;
  }
  return kConstructionGeometry;
}

SelectionScope::SelectionScope(const QString& name,
                               const std::vector<std::pair<FilterRow, FilterState>>& defaults)
    : m_name(name) {
  for (const auto& [row, state] : defaults) {
    if (!m_rows.contains(row)) {
      m_rows.append(row);
    }
    m_defaults[indexOf(row)] = state;
  }
  m_states = m_defaults;
}

SelectionScope SelectionScope::sketch() {
  return SelectionScope(QStringLiteral("sketch"),
                        {{FilterRow::SketchGeometry, FilterState::Window},
                         {FilterRow::ConstructionGeometry, FilterState::Window},
                         {FilterRow::ProjectedGeometry, FilterState::Window},
                         {FilterRow::Annotations, FilterState::Click},
                         {FilterRow::Profiles, FilterState::Click},
                         {FilterRow::VisibleEdges, FilterState::Click},
                         {FilterRow::OccludedEdges, FilterState::Off},
                         {FilterRow::Faces, FilterState::Off},
                         {FilterRow::Vertices, FilterState::Off},
                         {FilterRow::Bodies, FilterState::Off},
                         {FilterRow::Components, FilterState::Off},
                         {FilterRow::ReferenceGeometry, FilterState::Off}});
}

SelectionScope SelectionScope::project() {
  return SelectionScope(QStringLiteral("project"), {{FilterRow::VisibleEdges, FilterState::Window},
                                                    {FilterRow::OccludedEdges, FilterState::Off},
                                                    {FilterRow::Faces, FilterState::Click},
                                                    {FilterRow::Vertices, FilterState::Click},
                                                    {FilterRow::OtherSketchGeometry, FilterState::Window}});
}

SelectionScope SelectionScope::intersect() {
  return SelectionScope(QStringLiteral("intersect"), {{FilterRow::VisibleEdges, FilterState::Window},
                                                      {FilterRow::OccludedEdges, FilterState::Off},
                                                      {FilterRow::Faces, FilterState::Click},
                                                      {FilterRow::Bodies, FilterState::Click},
                                                      {FilterRow::OtherSketchGeometry, FilterState::Window}});
}

bool SelectionScope::has(FilterRow row) const { return m_rows.contains(row); }

FilterState SelectionScope::state(FilterRow row) const {
  return has(row) ? m_states[indexOf(row)] : FilterState::Off;
}

FilterState SelectionScope::defaultState(FilterRow row) const {
  return has(row) ? m_defaults[indexOf(row)] : FilterState::Off;
}

void SelectionScope::setState(FilterRow row, FilterState state) {
  if (has(row)) {
    m_states[indexOf(row)] = state;
  }
}

FilterState SelectionScope::cycle(FilterRow row) {
  setState(row, nextFilterState(state(row)));
  return state(row);
}

void SelectionScope::reset() { m_states = m_defaults; }

bool SelectionScope::isDefault() const { return m_states == m_defaults; }

bool SelectionScope::allows(FilterRow row, bool window, bool expanded) const {
  if (!has(row)) {
    return false;
  }
  const FilterState current = state(row);
  if (window) {
    return current == FilterState::Window;
  }
  if (current != FilterState::Off) {
    return true;
  }
  if (!expanded) {
    return false;
  }
  // Faces win over whole bodies where both could be picked: Shift widens
  // to bodies only in a scope without a faces row. Faces are not widened
  // either while bodies are offered explicitly.
  if (row == FilterRow::Bodies) {
    return !has(FilterRow::Faces);
  }
  if (row == FilterRow::Faces) {
    return state(FilterRow::Bodies) == FilterState::Off;
  }
  return true;
}

bool SelectionScope::allowsEdge(const PickCase& how) const {
  const bool visible = allows(FilterRow::VisibleEdges, how.window, how.expanded);
  const bool occluded = allows(FilterRow::OccludedEdges, how.window, how.expanded);
  if (visible == occluded) {
    return visible;
  }
  const EdgeSight sight = how.sight ? how.sight() : EdgeSight::Visible;
  return (sight != EdgeSight::Occluded && visible) || (sight != EdgeSight::Visible && occluded);
}

SelectFilter SelectionScope::kinds(bool expanded) const {
  SelectFilter result;
  for (const FilterRow row : m_rows) {
    if (allows(row, false, expanded)) {
      result |= filterRowKinds(row);
    }
  }
  return result;
}

QString SelectionScope::describe() const {
  QStringList parts;
  for (const FilterRow row : m_rows) {
    parts << QStringLiteral("%1=%2").arg(filterRowId(row), filterStateName(state(row)));
  }
  return parts.join(QStringLiteral(", "));
}

std::optional<FilterRow> filterRowOf(const SelectionItem& item, const QString& activeSketch,
                                     const SketchRowOf& sketchRow) {
  if (isSketchKind(item.kind) && item.owner != activeSketch) {
    // Another sketch's curves and points: a row of their own, which only
    // Project's and Intersect's scopes have (mitcad#904, mitcad#1031); its
    // profiles, dimensions and constraints never.
    if (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint) {
      return FilterRow::OtherSketchGeometry;
    }
    return std::nullopt;
  }
  switch (item.kind) {
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
    return sketchRow ? sketchRow(item) : FilterRow::SketchGeometry;
  case SelectKind::SketchConstraint:
  case SelectKind::SketchDimension:
    return FilterRow::Annotations;
  case SelectKind::Profile:
    return FilterRow::Profiles;
  case SelectKind::Edge:
    return FilterRow::VisibleEdges;
  case SelectKind::Face:
    return FilterRow::Faces;
  case SelectKind::Vertex:
    return FilterRow::Vertices;
  case SelectKind::Body:
    return FilterRow::Bodies;
  case SelectKind::Component:
    return FilterRow::Components;
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
    return FilterRow::ReferenceGeometry;
  case SelectKind::Sketch:
  case SelectKind::Feature:
  case SelectKind::None:
    break;
  }
  return std::nullopt;
}

bool scopeAccepts(const SelectionScope& scope, const SelectionItem& item, const PickCase& how,
                  const QString& activeSketch, const SketchRowOf& sketchRow) {
  const std::optional<FilterRow> row = filterRowOf(item, activeSketch, sketchRow);
  if (!row) {
    return false;
  }
  if (item.kind == SelectKind::Edge) {
    return scope.allowsEdge(how);
  }
  return scope.allows(*row, how.window, how.expanded);
}

std::optional<SelectionScope> commandSelectionScope(const QString& commandId) {
  if (commandId == QStringLiteral("sketch.project")) {
    return SelectionScope::project();
  }
  if (commandId == QStringLiteral("sketch.intersect")) {
    return SelectionScope::intersect();
  }
  return std::nullopt;
}

} // namespace mitcad
