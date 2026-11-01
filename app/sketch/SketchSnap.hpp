// SPDX-License-Identifier: MIT
#pragma once

// Snapping of the cursor in sketch mode: to points (line
// ends, centres, the origin), midpoints, quadrants, intersections, onto
// curves and optionally to a grid; with a tool's switch on, also to body
// vertices and the midpoints of body edges in the sketch plane (mitcad#258),
// which the sketch's own geometry wins ties against; onto the sketch's x
// and y axes (mitcad#948). Tolerances are in screen pixels, so snapping
// feels the same at any zoom.

#include <functional>
#include <optional>

#include <QJsonObject>

#include <QPointF>
#include <QSet>
#include <QString>

#include "BodyGeometry.hpp"
#include "SketchModel.hpp"

namespace mitcad::sketch {

struct Snap {
  // BodyVertex, BodyMidpoint: a body vertex or the middle of a body edge in
  // the sketch plane (`bodyItem`), projected only when a point is placed
  // there (mitcad#258).
  enum class Kind {
    None,
    Grid,
    Point,
    Center,
    Origin,
    Midpoint,
    Quadrant,
    Intersection,
    OnCurve,
    BodyVertex,
    BodyMidpoint,
  };

  Kind kind = Kind::None;
  V2 at;          // the snapped position
  QString point;  // Point, Center: the point (Origin: the origin's point, if any)
  QString curve;  // Center: its circle; Midpoint, Quadrant, OnCurve, Intersection: the curve
  QString curve2; // Intersection: the other curve
  // OnCurve onto the sketch's x or y axis (mitcad#948): "x" or "y"; `curve`
  // is the line the model made for it, or its pick id (axisPick()) while
  // the sketch lacks it, which the action that places a point there makes
  // (SketchOp::constrain).
  QString axis;
  QPointF screen; // where the cursor was
  BodyItem bodyItem; // BodyVertex, BodyMidpoint: the vertex or the edge

  // On existing geometry (a new point there gets a constraint).
  bool onGeometry() const { return kind != Kind::None && kind != Kind::Grid; }
  // At an existing point, which a new curve can share.
  bool atPoint() const { return kind == Kind::Point || kind == Kind::Center; }
  QString describe() const;
};

struct SnapSettings {
  const SketchModel* model = nullptr;
  std::function<QPointF(const V2&)> toScreen;
  bool grid = true;
  double gridStep = 1.0; // mm
  QSet<QString> skipPoints;
  QSet<QString> skipCurves;
  // Body vertices and edge midpoints to snap to (mitcad#258): a tool's
  // switch (SketchTool::bodySnaps), on for the Point tool only; none when
  // null.
  const BodyGeometry* body = nullptr;
  // The sketch's x and y axes as lines across the view to snap onto
  // (mitcad#948), with the curves' tolerance; the sketch's own geometry and
  // the origin win ties. On for every tool that snaps.
  bool axes = false;
};

// The snapped position for the cursor at `screen`, where `raw` is the
// cursor's point on the sketch plane.
Snap snap(const SnapSettings& settings, const V2& raw, const QPointF& screen);

// The sketch axes a point placed where `snap` went lies on (mitcad#948):
// "x" or "y" onto that axis; "xy" at the origin (its snap, or the point the
// sketch has there); at an existing point, those the sketch holds it on
// (SketchModel::pointAxes). Empty elsewhere.
QString axesAt(const SketchModel& model, const Snap& snap);

// The constraint that holds `point` on the axis `snap` went onto
// (mitcad#948): coincident with the axis's entity, which `entity` gives for
// the snap's curve (SketchOp::axisInput: the line the sketch has, or one
// made now in the action's undo step). None when the entity was refused,
// and without asking for it when the snap is not onto an axis or there is
// no point to hold: hovering or a refused action makes no axis.
std::optional<QJsonObject> axisHold(const QString& point, const Snap& snap,
                                    const std::function<QString(const QString&)>& entity);

// --- What a drawing tool adds automatically (mitcad#1024). The tools add
// these through SketchOp::addAuto, which the model accepts only when the
// geometry already satisfies them, the others do not imply them and the
// solve moves nothing (`sketch.add_constraint` with `auto`).

// "horizontal" or "vertical" when the line from `a` to `b` lies exactly
// along that axis (within 1e-9 mm), else empty.
QString axisAlignment(const V2& a, const V2& b);

// The constraints that keep `point` at the quadrant of a circle, arc or
// ellipse where `snap` put it: coincident with the curve, and level with
// its centre point (`horizontal_points`) at the left and right quadrants
// or plumb with it (`vertical_points`) at the top and bottom ones, as the
// snap said (an ellipse turned off the axes: the coincident only). Empty
// for any other snap.
std::vector<QJsonObject> quadrantHold(const SketchModel& model, const QString& point, const Snap& snap);

// Where a line from `from` touches a circle or arc of the sketch, the
// nearest such point within `tolerance` mm of `cursor`: an
// OnCurve snap onto that curve at the touching point, for the line tool's
// tangent inference. None when no touching point is that near; a circle
// `from` lies on or inside has none. `skip`: curves left out.
std::optional<Snap> tangentTouch(const SketchModel& model, const V2& from, const V2& cursor, double tolerance,
                                 const QSet<QString>& skip = QSet<QString>());

// The tangents the line or arc `curve` already satisfies at its ends:
// where an end lies on a circle or arc of the sketch (or on a line, for an
// arc) and the two run the same way there (`{"type": "tangent", "a":
// other, "b": curve}`). Curves already tangent to it, the sketch's axes
// and the curve's own centre are left out.
std::vector<QJsonObject> tangentCandidates(const SketchModel& model, const QString& curve);

// A constraint for the log: its type and entities ("tangent c3 c5",
// "coincident p7 on c2").
QString constraintText(const QJsonObject& constraint);

// Why the model refused an automatic constraint, from its message: what
// follows "auto constraint k7: " (`redundant`, `conflict: …`, `moves p3
// by 1.2e-02 mm`), else the whole message.
QString autoConstraintReason(const QString& error);

} // namespace mitcad::sketch
