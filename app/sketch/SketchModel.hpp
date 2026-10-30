// SPDX-License-Identifier: MIT
#pragma once

// The sketch being edited as the model's `sketch` query describes it
// (core/model-api/src/commands.md, "Sketch commands (S1)"): solved points
// and curves, constraints, dimensions and status, in sketch coordinates.

#include <optional>
#include <vector>

#include <QJsonObject>
#include <QPointF>
#include <QString>
#include <QStringList>

#include "SketchGeometry.hpp"
#include "mitcad/geometry/profile.hpp"

namespace mitcad {
struct SelectionItem;
}

namespace mitcad::sketch {

struct PointData {
  QString id; // p3
  V2 at;
  bool fixed = false;
  bool construction = false;
  bool reference = false;   // projected
  bool constrained = false; // fully constrained
  bool derived = false;     // computed by a derived offset (P4)
  QString axis;             // "x" or "y": the far end of an axis line (mitcad#500)
};

struct CurveData {
  QString id;   // c4
  QString type; // line, circle, arc, ellipse, elliptical_arc, spline, fitted_spline
  Curve2 curve;
  QString start, end, center; // point ids, where the curve has them
  QStringList points;         // every point it refers to
  // A closed fitted spline (mitcad#783): no start or end, its points listed
  // once.
  bool closed = false;
  bool construction = false;
  bool centerline = false;
  bool fixed = false;
  bool reference = false;
  bool constrained = false;
  // "x" or "y": the line that stands for a sketch axis (mitcad#500), which
  // the overlay draws across the view instead.
  QString axis;

  bool isLine() const { return type == QStringLiteral("line"); }
  bool isCircle() const { return type == QStringLiteral("circle"); }
  bool isArc() const { return type == QStringLiteral("arc"); }
  bool isCircular() const { return isCircle() || isArc(); }
  bool isEllipse() const {
    return type == QStringLiteral("ellipse") || type == QStringLiteral("elliptical_arc");
  }
  bool isSpline() const {
    return type == QStringLiteral("spline") || type == QStringLiteral("fitted_spline");
  }
};

struct ConstraintData {
  QString id; // k2
  QString type;
  QJsonObject json; // the definition's fields
  QStringList refs; // the entities, in the definition's order (an equal spacing's anchors)
  // Whether the solve applies it (mitcad#721): an inactive one is kept and
  // drawn, but constrains nothing until it is activated; `inactiveReason`
  // says why when something did ("not met in the imported file").
  bool active = true;
  QString inactiveReason;
};

struct DimensionData {
  QString id; // k5
  QString type;
  QJsonObject json;
  QStringList refs;
  bool driven = false;
  // Inactive (mitcad#721): it keeps its value but drives nothing.
  bool active = true;
  QString inactiveReason;
  QString parameter;  // a driving dimension's parameter or a driven one's reference (d3)
  QString expression; // its expression ("d1 / 2")
  QString comment;    // its parameter's comment ("Sketch1 distance")
  double measured = 0.0; // mm or radians
  std::optional<V2> text; // where its value is shown, when placed
  // A signed horizontal or vertical distance (mitcad#404): the offset b - a,
  // which may be negative.
  bool signedOffset = false;
  // A drawing tool's angular sector, preserving signed/reflex input.
  // Transient preview data only; never read from or written to a sketch.
  std::optional<double> previewSweep;
  // Which limit of its parameter changed the value ("min", "max", "step";
  // empty when none did) and the model's words for it (mitcad#665).
  QString limited;
  QString limitMessage;

  bool isAxisDistance() const {
    return type == QStringLiteral("horizontal_distance") || type == QStringLiteral("vertical_distance");
  }
};

struct TextData {
  QString id; // t5
  QString text;
  V2 at;              // the anchor: the start of the first line's baseline
  double height = 5.0; // mm (the font size)
  double angle = 0.0;  // from the sketch x axis
  QString font;        // empty: the bundled font
  bool bold = false;
  bool italic = false;
  QString align = QStringLiteral("left");      // left, center, right
  QString valign = QStringLiteral("baseline"); // baseline, bottom, middle, top
  double spacing = 0.0;                         // percent
  QStringList frame;   // the frame's corner points (a text in a frame)
  QString path;        // the curve a text runs along
  bool above = true;
  bool fit = false;
  bool flipX = false;
  bool flipY = false;
  bool outlined = false; // the model gave its glyph outlines
  QString fontFile;      // the font file its font comes from (mitcad#473)
  // A QR code of `text` (mitcad#473): `height` is a module's size, `at` its
  // lower left corner; `ecc` its error correction (L, M, Q, H).
  bool code = false;
  QString ecc;
  // The text parameter its text follows (mitcad#664), as the Edit Text
  // panel shows it: the parameter's name, or the expression of the
  // sketch's own one; empty when it follows none.
  QString value;
};

// A pattern of the sketch (commands.md, `sketch.circular_pattern`).
struct PatternData {
  QString id;   // k7
  QString type; // circular, rectangular
  int count = 2;
  int count2 = 1;
  // The values' expressions (angle, or the two spacings).
  QStringList values;
  QStringList entities; // originals
  QStringList copies;   // every copy
};

// An offset of the sketch (commands.md, `sketch.offset`).
struct OffsetData {
  QString id; // k9
  QStringList curves; // its source chain
  QStringList made;   // the curves it made
  QString expression; // of its distance
  bool left = false;
  bool derived = false;
};

// A linked projection of the sketch (the `sketch` query's `projections`): its
// source and the entities made from it, which a tool reuses for another
// dimension or point on the same body edge or vertex (mitcad#256).
struct ProjectionData {
  QString source;      // E{…}, V{…} or a face
  QString body;        // empty: any body
  QString occurrence;  // its link's source: where another component's geometry was picked
  bool linked = false; // it has a link (geometry of another component)
  QStringList entities;
  bool stale = false; // it did not follow its source all the way (kept, lost, unfollowed, stale)
  bool section = false; // section curves: where the sketch's plane cuts the source face (mitcad#389)
};

class SketchModel {
public:
  // Reads a `sketch` query's answer; false when the sketch has no frame
  // (it did not evaluate).
  bool load(const QJsonObject& sketch);

  QString uid;
  QString name;
  // As the query gives it, in the sketch's component's coordinates; sketch
  // mode's model has it in the design's, placed where the view shows the
  // component (SketchHost::sketchPlacement, mitcad#505).
  geometry::Frame frame;
  bool solved = false;
  QString error;
  // What the evaluation warns about (linked projections that could not
  // follow their sources, mitcad#232).
  QStringList warnings;
  // The entities each warning is about, one list per warning (the query's
  // `warning_entities`, mitcad#264): empty for a warning about nothing in
  // particular.
  std::vector<QStringList> warningEntities;
  // Linked projections that do not follow their sources (kept, lost or with
  // entities that found nothing): Clean Up Stale Projections tidies them,
  // and the palette offers it (mitcad#252).
  bool staleProjections = false;
  int dof = 0;
  std::vector<PointData> points;
  std::vector<CurveData> curves;
  std::vector<ConstraintData> constraints;
  std::vector<DimensionData> dimensions;
  std::vector<TextData> texts;
  std::vector<PatternData> patterns;
  std::vector<OffsetData> offsets;
  std::vector<ProjectionData> projections;
  std::vector<QStringList> conflicts;
  // The inactive constraints and dimensions the geometry does not meet
  // (mitcad#721): activating one of them moves it.
  QStringList unmetInactive;

  const PointData* point(const QString& id) const;
  const CurveData* curve(const QString& id) const;
  const TextData* text(const QString& id) const;
  // The pattern an entity is an original or a copy of, and the offset that
  // made a curve.
  const PatternData* patternOf(const QString& entity) const;
  const OffsetData* offsetOf(const QString& curve) const;
  const ConstraintData* constraint(const QString& id) const;
  const DimensionData* dimension(const QString& id) const;
  std::optional<V2> pointAt(const QString& id) const;
  // A fixed point at the sketch origin, which stands for the origin.
  QString originPoint() const;
  // The sketch's axes and origin as picks (mitcad#500). The line that
  // stands for axis "x" or "y", when the sketch has it.
  QString axisLine(const QString& axis) const;
  // "x" or "y" for an axis line or an axis pick, else empty.
  QString axisOf(const QString& id) const;
  // An axis line, an axis pick, the origin point or the origin pick: what
  // the sketch fixes in place, so that a constraint or dimension takes at
  // most one of them.
  bool isAxisOrOrigin(const QString& id) const;
  // Adds the picks of the axes and origin the sketch lacks (axisPick ids)
  // as the fixed entities the model would make, so that the tools decide
  // with them as with any line and point; SketchOp::axisInput makes them.
  void addAxisPicks();
  // The axes and origin selected with no tool running (mitcad#899): the
  // selection keeps them by their pick ids. The entity that stands for a
  // pick in this sketch is the one the model made, or the pick id until a
  // step makes it (a tool's view adds it, addAxisPicks); any other id as
  // it is.
  QString axisEntity(const QString& id) const;
  // What a click with no tool running selects of what entityAt() found
  // there: an axis by its pick id (also where the model made its line),
  // the origin's pick; empty for anything else, which is the view's to
  // pick.
  QString axisSelectionPick(const QString& entity) const;
  // The picks a selection's curve and point ids of this sketch give a tool,
  // in their order: an axis or origin pick as axisEntity(); with `axes`
  // false (Fix/Unfix), the axes and origin left out.
  QStringList selectionPicks(const QStringList& names, bool axes) const;
  // The axes point `id` lies on by what the sketch holds it with
  // (mitcad#948): "xy" for the origin point, else "x", "y" or both for its
  // active coincident constraints on the axes' lines; empty otherwise.
  QString pointAxes(const QString& id) const;
  // Curves with a point as an end (or their centre, with `centers`).
  QStringList curvesAt(const QString& point, bool centers = false) const;
  // Whether everything is constrained (and there is something).
  bool fullyConstrained() const { return dof == 0 && !points.empty(); }

  // Equal spacing (mitcad#221): where an anchor is (a point, a circle's or
  // an arc's centre, a line's midpoint), the first equal spacing an entity
  // is an anchor of, and an equal spacing's common spacing dimension.
  std::optional<V2> anchorAt(const QString& id) const;
  const ConstraintData* equalSpacingOf(const QString& entity) const;
  const DimensionData* spacingDimension(const QString& constraint) const;
};

// The ids that stand for the sketch's origin and axes in the picking tools
// until a step makes them (mitcad#500): "origin", "x_axis", "y_axis".
QString originPick();
QString axisPick(const QString& axis);
bool isAxisPick(const QString& id);
// What `sketch.axis` takes for a pick ("origin", "x", "y"), or empty.
QString axisPickName(const QString& id);
// How long the model makes an axis line (core/sketch/src/axes.rs).
constexpr double kAxisLineLength = 100.0;
// The selection's item for an axis or origin pick of sketch `sketch`
// (mitcad#899): a sketch curve of geometry "line" for an axis (what a line
// input such as Mirror's Mirror Line takes, mitcad#876), a sketch point for
// the origin.
SelectionItem axisSelectionItem(const QString& sketch, const QString& pick, const QString& occurrence);
// Whether a selection item is such a pick: no entity of the sketch (yet),
// so only the tools and the inputs that take the axes take it; Delete,
// drags, copies, Fix/Unfix and other commands' inputs leave it out.
bool isAxisSelection(const SelectionItem& item);
// How far a view position is from an axis drawn across the view through
// `origin` (view pixels) along `along` (the view offset of a step along the
// axis); none when the sketch plane is seen edge on.
std::optional<double> axisScreenDistance(const QPointF& screen, const QPointF& origin, const QPointF& along);

// The sketch point of a 3D frame point and back.
V2 toSketch(const geometry::Frame& frame, const gp_Pnt& point);
gp_Pnt toModel(const geometry::Frame& frame, const V2& point);
// A curve from the model's JSON: an entity's `type` and its `geometry`
// (as the `sketch` query gives them).
Curve2 curveFromJson(const QString& type, const QJsonObject& geometry);

} // namespace mitcad::sketch
