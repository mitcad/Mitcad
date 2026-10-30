// SPDX-License-Identifier: MIT
// Feature inputs where they were (mitcad#957; app/COMMANDS.md, "Timeline
// feature inspection"): a Fillet or a Chamfer selected or hovered on the
// timeline shows the edges it picked as lines where they were before it,
// from the polylines its build kept (the `feature_inputs` query, which
// computes nothing). The lines are read once per refresh of the scene and
// kept until the next one.
#include "MainWindow.hpp"

#include <cstddef>
#include <exception>
#include <vector>

#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRep_Builder.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include <QJsonArray>
#include <QJsonObject>

namespace mitcad {

namespace {

// A polyline's point; false when the value is not three numbers.
bool pointOf(const QJsonValue& value, gp_Pnt& point) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() != 3) {
    return false;
  }
  point.SetCoord(xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble());
  return true;
}

// The polylines of the `feature_inputs` answer as wires in one compound
// (null when none makes a line).
TopoDS_Shape inputWires(const QJsonArray& edges) {
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  bool any = false;
  for (const QJsonValue& edge : edges) {
    const QJsonArray points = edge.toArray();
    std::vector<gp_Pnt> line;
    line.reserve(static_cast<std::size_t>(points.size()));
    for (const QJsonValue& value : points) {
      gp_Pnt point;
      if (pointOf(value, point)) {
        line.push_back(point);
      }
    }
    if (line.size() < 2) {
      continue;
    }
    // A closed curve (a circle's edge) ends where it starts: closed, not
    // ended by a second vertex at its start.
    const bool closed = line.size() > 2 && line.front().Distance(line.back()) < 1e-7;
    if (closed) {
      line.pop_back();
    }
    BRepBuilderAPI_MakePolygon polygon;
    for (const gp_Pnt& point : line) {
      polygon.Add(point);
    }
    if (closed) {
      polygon.Close();
    }
    if (polygon.IsDone()) {
      builder.Add(compound, polygon.Wire());
      any = true;
    }
  }
  return any ? TopoDS_Shape(compound) : TopoDS_Shape();
}

} // namespace

TopoDS_Shape MainWindow::featureInputLines(const SelectionItem& item, int* edges) const {
  if (edges) {
    *edges = 0;
  }
  if (item.kind != SelectKind::Feature) {
    return TopoDS_Shape();
  }
  const DocumentSnapshot::Feature* feature = m_snapshot.feature(item.owner);
  if (!feature || feature->index >= m_snapshot.marker || !feature->succeeded() || feature->suppressed) {
    return TopoDS_Shape();
  }
  auto known = m_featureInputLines.constFind(item.owner);
  if (known == m_featureInputLines.cend()) {
    // Asked only while the document is idle: a job's document is the
    // worker's (docs/architecture.md, "Background computation"). Nothing
    // is remembered then, so the next idle refresh asks.
    if (modelBusy()) {
      return TopoDS_Shape();
    }
    FeatureInputLines found;
    try {
      const QJsonObject answer = queryObject(
          {{QStringLiteral("query"), QStringLiteral("feature_inputs")}, {QStringLiteral("uid"), item.owner}});
      const QJsonArray polylines = answer.value(QStringLiteral("edges")).toArray();
      found.lines = inputWires(polylines);
      found.edges = found.lines.IsNull() ? 0 : static_cast<int>(polylines.size());
    } catch (const std::exception&) {
      // Gone from the model: no lines.
    }
    known = m_featureInputLines.insert(item.owner, found);
  }
  if (known->lines.IsNull()) {
    return TopoDS_Shape();
  }
  if (edges) {
    *edges = known->edges;
  }
  // In the component's coordinates, placed by the occurrence as the
  // feature's faces are (itemShape).
  if (!item.occurrence.isEmpty()) {
    if (!m_placements.contains(item.occurrence)) {
      return TopoDS_Shape();
    }
    return known->lines.Moved(placementOf(item.occurrence));
  }
  return known->lines;
}

} // namespace mitcad
