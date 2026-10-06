// SPDX-License-Identifier: MIT
#pragma once

// The sketch being edited as the model's `sketch` query describes it
// (core/model/src/api/commands.md, "Sketch commands (S1)"): solved points
// and curves, constraints, dimensions and status, in sketch coordinates.

#include <optional>
#include <vector>

#include <QJsonObject>
#include <QString>
#include <QStringList>

#include "SketchGeometry.hpp"
#include "mitcad/geometry/profile.hpp"

namespace mitcad::sketch {

struct PointData {
  QString id; // p3
  V2 at;
  bool fixed = false;
  bool construction = false;
  bool reference = false;   // projected
  bool constrained = false; // fully constrained
  bool derived = false;     // computed by a derived offset (P4)
};

struct CurveData {
  QString id;   // c4
  QString type; // line, circle, arc, ellipse, elliptical_arc, spline, fitted_spline
  Curve2 curve;
  QString start, end, center; // point ids, where the curve has them
  QStringList points;         // every point it refers to
  bool construction = false;
  bool centerline = false;
  bool fixed = false;
  bool reference = false;
  bool constrained = false;

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
  QStringList refs; // the entities, in the definition's order
};

struct DimensionData {
  QString id; // k5
  QString type;
  QJsonObject json;
  QStringList refs;
  bool driven = false;
  QString parameter;  // a driving dimension's parameter (d3)
  QString expression; // its expression ("d1 / 2")
  double measured = 0.0; // mm or radians
  std::optional<V2> text; // where its value is shown, when placed
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

class SketchModel {
public:
  // Reads a `sketch` query's answer; false when the sketch has no frame
  // (it did not evaluate).
  bool load(const QJsonObject& sketch);

  QString uid;
  QString name;
  geometry::Frame frame;
  bool solved = false;
  QString error;
  int dof = 0;
  std::vector<PointData> points;
  std::vector<CurveData> curves;
  std::vector<ConstraintData> constraints;
  std::vector<DimensionData> dimensions;
  std::vector<TextData> texts;
  std::vector<PatternData> patterns;
  std::vector<OffsetData> offsets;
  std::vector<QStringList> conflicts;

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
  // Curves with a point as an end (or their centre, with `centers`).
  QStringList curvesAt(const QString& point, bool centers = false) const;
  // Whether everything is constrained (and there is something).
  bool fullyConstrained() const { return dof == 0 && !points.empty(); }
};

// The sketch point of a 3D frame point and back.
V2 toSketch(const geometry::Frame& frame, const gp_Pnt& point);
gp_Pnt toModel(const geometry::Frame& frame, const V2& point);

} // namespace mitcad::sketch
