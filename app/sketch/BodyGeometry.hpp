// SPDX-License-Identifier: MIT
#pragma once

// Body edges and vertices in the sketch plane (mitcad#256, mitcad#258), as
// the model's `plane_geometry` query lists them in sketch coordinates
// (core/model-api/src/commands.md, "Body geometry in a sketch's plane"):
// what the Dimension and constraint tools pick (mitcad#1782) and the Point
// tool snaps to. With them come the edges and vertices out of the plane
// (mitcad#866), as the curves and points they project to along its normal,
// with where they are above or below it: the Dimension tool picks them where
// the view shows them; the others take the ones in the plane only. Nothing is
// projected while a tool picks or snaps; the dimension, constraint or point
// made from one projects it then (linked, SketchOp::projectBody), or takes the
// entities of an existing projection of the same source that still match it.
// Plain Qt Core and plane geometry, so that the snapper's unit test needs no
// model.

#include <cstddef>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include "SketchModel.hpp"

namespace mitcad::sketch {

// One edge or vertex of a body in the sketch plane, or out of it, as what
// it projects to.
struct BodyItem {
  // A point of an item out of the plane: where it projects to and how far
  // above the plane it is (negative below).
  struct Lifted {
    V2 at;
    double height = 0.0;
  };

  QString source;     // E{…} or V{…}: sketch.project's `source`
  QString body;       // F2.b0
  QString occurrence; // a body of another component: where it was found (O1/O4; empty: the root)
  bool linked = false; // `occurrence` is given (another component's body)
  // vertex, or what the edge projects to: line, circle, arc, ellipse,
  // elliptical_arc, spline, or point (an edge seen end on, out of the plane)
  QString type;
  Curve2 curve;       // an edge
  V2 at;              // a vertex, or the point an edge seen end on projects to
  // Out of the plane (mitcad#866): where it is, for picking it where the
  // view shows it; the height of a vertex or of a circle's or arc's centre,
  // and the edge's points from its start to its end (a vertex's one).
  bool inPlane = true;
  double height = 0.0;
  std::vector<Lifted> path;

  bool isVertex() const { return type == QStringLiteral("vertex"); }
  // What projects to a point: a vertex, or an edge seen end on.
  bool isPoint() const { return isVertex() || type == QStringLiteral("point"); }
  // Lines and arcs have a midpoint to snap to.
  bool hasMiddle() const { return type == QStringLiteral("line") || type == QStringLiteral("arc"); }
  // Circles and arcs have a centre to pick.
  bool hasCenter() const { return type == QStringLiteral("circle") || type == QStringLiteral("arc"); }
  // "the body vertex V{…}" / "the body edge E{…}" for hints and logs.
  QString describe() const;
};

// What a projection of a body item made: a vertex's point, an edge's curve.
struct ProjectedItem {
  QString point;
  QString curve;
  bool empty() const { return point.isEmpty() && curve.isEmpty(); }
};

class BodyGeometry {
public:
  // How a curve's `type` and `geometry` read (curveFromJson in
  // SketchModel.hpp).
  using CurveReader = Curve2 (*)(const QString& type, const QJsonObject& geometry);

  // Reads a `plane_geometry` answer.
  void load(const QJsonArray& answer, CurveReader read);
  void set(std::vector<BodyItem> list);
  void clear() { set({}); }
  const std::vector<BodyItem>& items() const { return m_items; }
  // Changes with every load, so that a tool knows when its view is stale.
  int revision() const { return m_revision; }
  // The edges and vertices in the plane.
  int edgeCount() const;
  int vertexCount() const;
  // The items out of the plane (mitcad#866).
  int outOfPlaneCount() const;

  // The ids of the items and of their points, which never clash with a
  // sketch's (p3, c4): "body:3" an item, "body:3:start", "body:3:end" and
  // "body:3:center" the points of a curve.
  static bool isBodyId(const QString& id) { return id.startsWith(QStringLiteral("body:")); }
  static QString itemId(std::size_t index) { return QStringLiteral("body:%1").arg(index); }
  // The item an id names (an item's or one of its points'), with the point
  // it names: empty for the item itself, else start, end or center.
  const BodyItem* item(const QString& id, QString* part = nullptr) const;
  std::optional<V2> pointAt(const QString& id) const;
  const Curve2* curve(const QString& id) const;
  // What can be picked as a point: the vertices, and the centres of
  // circles and arcs (ids and places).
  std::vector<std::pair<QString, V2>> points() const;
  // Adds the items to a copy of the sketch as fixed reference entities
  // with the ids above, the curves with their ends and centres as points:
  // a tool then treats a body item as the line, circle, arc or point it
  // would project to.
  void addTo(SketchModel& model) const;

private:
  std::vector<BodyItem> m_items;
  int m_revision = 0;
};

// Whether two curves are the same (either way round) within `tolerance`.
bool sameCurve(const Curve2& a, const Curve2& b, double tolerance);

// The entities of a linked projection the sketch already has of the item's
// source (the same name, body and occurrence) when they still match it:
// the projection follows its source and its point or curve is where the
// item is (mitcad#256 rule 5, mitcad#258 rule 3). None otherwise.
std::optional<ProjectedItem> existingProjection(const SketchModel& model, const BodyItem& item);

// The projected point or curve of any projection of the sketch (not stale)
// that is where the item projects to: what a projection of the item joined
// instead of adding the same curve again (an edge below one projected
// before, mitcad#866). None otherwise.
std::optional<ProjectedItem> projectedLike(const SketchModel& model, const BodyItem& item);

// A point of the sketch other than `point` that a coincident constraint
// holds on it (a point placed on a projected vertex before), or empty.
QString coincidentPoint(const SketchModel& model, const QString& point);

} // namespace mitcad::sketch
