// SPDX-License-Identifier: MIT
#pragma once

// What sketch mode draws over the 3D view: constraint glyphs and
// dimensions (both selectable), the active tool's preview, the snap marker,
// the sketch's axes and origin, the pointer's place and the degrees of
// freedom. Drawn with QPainter in view pixels, so text
// and glyphs keep their size at any zoom; the widget lets the mouse
// through to the view, and the controller asks it what lies under the
// cursor.

#include <functional>
#include <map>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QLineF>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QString>
#include <QWidget>

#include "PointMarkers.hpp"
#include "SketchModel.hpp"

class QPainter;

namespace mitcad::sketch {

class SketchController;
struct ToolPreview;

// A dimension as drawn, in view pixels.
struct DimensionShape {
  std::vector<QLineF> lines;              // extension and dimension lines
  std::vector<QPolygonF> arcs;            // angle and arc length arcs
  std::vector<std::pair<QPointF, QPointF>> arrows; // tip and the direction it points to
  QPointF textAt;
  QString text;
  bool valid = false;
};

// Maps sketch coordinates to the view.
struct Projection {
  std::function<QPointF(const V2&)> toScreen;
  double pixel = 0.1; // mm per pixel
};

// Lays out a dimension of a sketch (a placed one, or one being placed).
DimensionShape dimensionShape(const DimensionData& dimension, const SketchModel& model,
                              const Projection& projection, const QString& text);
// The default place of a dimension's value.
V2 defaultTextPosition(const DimensionData& dimension, const SketchModel& model, double pixel);
// What a dimension measures on the current geometry (mm or radians), when
// it can be measured.
std::optional<double> measure(const DimensionData& dimension, const SketchModel& model);

// An angle dimension's corner, where its lines meet, and the unit rays from
// it that bound the sector it measures (mitcad#296): the lines' directions
// (start to end), reversed where its `reverse_a` and `reverse_b` say. One
// stored without them measures the angle closer to its value: with
// `byValue` the sector of that angle nearer to its text, else the
// directions. None for parallel lines.
struct AngleRays {
  V2 corner, da, db;
};
std::optional<AngleRays> angleRays(const DimensionData& dimension, const SketchModel& model, bool byValue = true);

class SketchOverlay : public QWidget {
  Q_OBJECT

public:
  struct Hit {
    enum class Kind { None, Constraint, Fixed, Dimension };
    Kind kind = Kind::None;
    QString id; // k3; for Fixed the entity
    bool valid() const { return kind != Kind::None; }
  };

  SketchOverlay(SketchController& controller, QWidget* view);

  Hit hitTest(const QPointF& position) const;
  // The dimensions' values and constraint glyphs a window holds (view
  // pixels): wholly inside it, or with `crossing` also those it touches
  // (mitcad#217). Only those shown.
  std::vector<Hit> hitsIn(const QRectF& area, bool crossing) const;
  // Where a dimension's value is drawn.
  QRectF dimensionTextRect(const QString& id) const;
  // The text a dimension shows: its value, "fx:" for an expression,
  // parentheses when driven.
  QString dimensionText(const DimensionData& dimension) const;
  // A dimension's tooltip: its name, its expression, its value in the
  // document's unit and its comment ("d3 = d1 / 2 → 20 mm (Sketch1
  // distance)", mitcad#231).
  QString dimensionTooltip(const DimensionData& dimension) const;
  // An inactive constraint's or dimension's tooltip line (mitcad#721): "Not
  // applied: <why>"; empty for an active one.
  static QString inactiveTooltip(bool active, const QString& reason);
  // Where each shown constraint's first symbol is drawn (view pixels).
  std::map<QString, QRectF> constraintGlyphRects() const;
  // Every shown symbol's bounds, including repeated constraints and fixed
  // entities, as used by painting and picking.
  std::vector<QRectF> constraintGlyphBounds() const;
  // Logs "Sketch inactive drawn: k5 constraint, k7 dimension" when what is
  // drawn inactive changes, for the UI tests (mitcad#721).
  void logInactive();
  // Logs "Sketch dimension halos: <count> dark|light" when it changes, for
  // the UI tests (mitcad#633). Called when the overlay paints, when the
  // background's tone changes and when the sketch's edit starts: a session
  // without a desktop (Windows session 0) may never paint it.
  void logDimensionHalos();
  // Logs "Sketch axes drawn: origin at x,y in the view; x axis x,y to x,y;
  // y axis x,y to x,y" (window coordinates; "outside the view", "none")
  // when what paintAxes draws changes, for the UI tests (mitcad#739: the
  // sketch draws its origin and axes whatever the Origin folder's light
  // bulb says). Called when the overlay paints and with the "Sketch view"
  // line, as a session without a desktop may never paint it.
  void logAxes();
  // Logs "Sketch overlay: point markers over highlights; 1 curve, 0 points
  // highlighted, 3 markers over them" when it changes, for the UI tests
  // (mitcad#946): the highlights of the selection, picks and hover and the
  // point markers painted over them. Called when the overlay paints and
  // when the selection or the hover changes, as a session without a
  // desktop may never paint it.
  void logHighlights();

protected:
  void paintEvent(QPaintEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  struct Glyph {
    QString constraint; // k3, or empty for a fixed entity
    QString entity;     // what it is drawn at
    QString type;       // constraint type, or "fix"
    QRectF rect;
    bool inactive = false; // an inactive constraint's (mitcad#721), drawn pink
  };
  struct DimensionLayout {
    QString id;
    DimensionShape shape;
    QRectF textRect;
  };

  // The sketch's origin and its x and y axes as paintAxes draws them, in
  // view pixels: each axis a line across the view from its negative to its
  // positive end, none when the plane is seen edge on or the axis misses
  // the view.
  struct AxesShape {
    QPointF origin;
    std::optional<QLineF> x;
    std::optional<QLineF> y;
  };

  Projection projection() const;
  AxesShape axesShape() const;
  std::vector<Glyph> glyphs() const;
  // A sketch point's place, or a curve as a polyline, in view pixels.
  std::vector<QPointF> entityOutline(const QString& entity) const;
  std::vector<DimensionLayout> dimensions() const;
  QRectF textRect(const QPointF& at, const QString& text) const;
  // The sketch's x and y axes across the view, its origin, and the design
  // origin's place when the sketch's origin is elsewhere (mitcad#254).
  void paintAxes(QPainter& painter) const;
  void paintGlyph(QPainter& painter, const Glyph& glyph, bool selected, bool hovered) const;
  // The halo under a dimension's lines, arcs and arrowheads (mitcad#633):
  // wider strokes in `halo`, the opposite tone of the lines, so that they
  // show over a body's face of their own tone. Painted for all dimensions
  // before any `paintDimension`, so that a halo never cuts another
  // dimension's line.
  void paintDimensionHalo(QPainter& painter, const DimensionShape& shape, const QColor& halo) const;
  // `color`: the value's text; `lines`: the dimension and extension lines
  // and arrows, light on a dark background.
  void paintDimension(QPainter& painter, const DimensionShape& shape, const QRectF& textRect,
                      const QColor& color, const QColor& lines, bool driven,
                      bool inactive = false) const;
  void paintPreview(QPainter& painter, const ToolPreview& preview) const;
  void paintSnap(QPainter& painter) const;
  // What highlightLayers gives to paint: the entities drawn as strokes
  // (curves, lines, axes) and as balls (points), the plain point markers
  // over them, and whether they are painted after the strokes.
  struct HighlightSummary {
    int curves = 0;
    int points = 0;
    int markers = 0;
    bool markersOver = false;
  };
  // The highlights with no tool or a picking tool running: the selection's
  // curves and points, the axes a command's input holds, a tool's picks and
  // the hover, each in its colour, later ones over earlier ones.
  std::vector<std::pair<QString, QColor>> idleHighlights() const;
  // Sketch entities (and body geometry as sketch entities, mitcad#256)
  // highlighted, in view pixels: a curve's or an axis's 4 px line, a
  // point's ball, and the plain markers of the highlighted curves' points,
  // which paintHighlightLayers (sketch/PointMarkers.hpp) paints over the
  // lines (mitcad#946).
  HighlightLayers highlightLayers(const std::vector<std::pair<QString, QColor>>& entities,
                                  HighlightSummary* summary) const;
  void paintHighlights(QPainter& painter, const std::vector<std::pair<QString, QColor>>& entities) const;
  void logHighlightSummary(const HighlightSummary& summary);
  // An equal spacing's anchors joined in order and numbered (mitcad#221).
  void paintChain(QPainter& painter, const QStringList& anchors, const QColor& color) const;
  bool isSelected(const QString& kind, const QString& id) const;

  SketchController& m_c;
  // The last "Sketch dimension halos:" line logged (for UI tests).
  QString m_loggedDimensionHalos;
  // The last "Sketch inactive drawn:" line logged (mitcad#721).
  QString m_loggedInactive;
  // The last "Sketch axes drawn:" line logged (mitcad#739); cleared when
  // the overlay hides, so that every sketch edit logs it again.
  QString m_loggedAxes;
  // The last "Sketch overlay:" line logged (mitcad#946).
  QString m_loggedHighlights;
};

} // namespace mitcad::sketch
