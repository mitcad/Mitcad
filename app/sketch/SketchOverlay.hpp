// SPDX-License-Identifier: MIT
#pragma once

// What sketch mode draws over the 3D view: constraint glyphs and
// dimensions (both selectable), the active tool's preview, the snap marker
// and the degrees of freedom. Drawn with QPainter in view pixels, so text
// and glyphs keep their size at any zoom; the widget lets the mouse
// through to the view, and the controller asks it what lies under the
// cursor.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QLineF>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QString>
#include <QWidget>

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
  // Where a dimension's value is drawn.
  QRectF dimensionTextRect(const QString& id) const;
  // The text a dimension shows: its value, "fx:" for an expression,
  // parentheses when driven.
  QString dimensionText(const DimensionData& dimension) const;

protected:
  void paintEvent(QPaintEvent* event) override;
  void hideEvent(QHideEvent* event) override;

private:
  struct Glyph {
    QString constraint; // k3, or empty for a fixed entity
    QString entity;     // what it is drawn at
    QString type;       // constraint type, or "fix"
    QRectF rect;
  };
  struct DimensionLayout {
    QString id;
    DimensionShape shape;
    QRectF textRect;
  };

  Projection projection() const;
  std::vector<Glyph> glyphs() const;
  std::vector<DimensionLayout> dimensions() const;
  QRectF textRect(const QPointF& at, const QString& text) const;
  void paintGlyph(QPainter& painter, const Glyph& glyph, bool selected, bool hovered) const;
  void paintDimension(QPainter& painter, const DimensionShape& shape, const QRectF& textRect,
                      const QColor& color, bool driven) const;
  void paintPreview(QPainter& painter, const ToolPreview& preview) const;
  void paintSnap(QPainter& painter) const;
  void paintEntityHighlight(QPainter& painter, const QString& entity, const QColor& color) const;
  bool isSelected(const QString& kind, const QString& id) const;

  SketchController& m_c;
};

} // namespace mitcad::sketch
