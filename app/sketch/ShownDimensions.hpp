// SPDX-License-Identifier: MIT
#pragma once

// Show Dimensions of a sketch's row (mitcad#1772, GitHub mitcad/Mitcad#306,
// app/COMMANDS.md "Show Dimensions outside sketch mode"): the dimensions of
// shown planar sketches drawn over the 3D view while they are not edited,
// as sketch mode's overlay draws them (SketchOverlay: dimensionShape, the
// halo, the lines, arrows and value boxes, driven and inactive ones in
// their own styles), in view pixels in each sketch's placed frame, so that
// they face the camera and keep their size at any zoom. The view draws
// them itself, in its overlay layer, painted with the frame that moves the
// camera (a ViewPaintLayer, mitcad#2296): no widget over the view, which Qt
// would paint and compose again after every frame.
//
// Display only: the mouse is the view's, so they are neither picked by the
// model's tools nor snapped to. It watches the view's
// mouse: a value under the pointer shows its tooltip (its name and
// expression, as in sketch mode) and is drawn in the hover's colour; a
// double-click on one asks to edit it (editRequested), which opens its
// sketch and the dimension's value editor.

#include <cstddef>
#include <functional>
#include <vector>

#include <QFont>
#include <QObject>
#include <QPointF>
#include <QRectF>
#include <QString>
#include <QTimer>

#include "SketchModel.hpp"
#include "SketchOverlay.hpp"

class QPainter;

namespace mitcad {
class OcctViewer;
class ViewPaintLayer;
} // namespace mitcad

namespace mitcad::sketch {

// A sketch whose dimensions are drawn: its `sketch` query's answer, its
// frame placed in the design where the view shows its component.
struct ShownSketch {
  QString uid;  // F3
  QString name; // Sketch1
  SketchModel model;
};

class ShownDimensions : public QObject {
  Q_OBJECT

public:
  explicit ShownDimensions(OcctViewer& viewer);

  // The sketches whose dimensions are drawn now (none: the view no longer
  // draws them);
  // lengths in the document's unit (`lengthScale` mm per unit, `lengthUnit`
  // its symbol), `textMode` the palette's Dimension text
  // (SketchController::DimensionText). Logs "Sketch dimensions shown
  // outside sketch mode: Sketch1 (2), Sketch3 (1)" ("none") when the
  // sketches or their numbers of dimensions change.
  void setSketches(std::vector<ShownSketch> sketches, double lengthScale, const QString& lengthUnit, int textMode);
  // Whether a double-click on a value asks to edit it now (the main window
  // idle); without it, never.
  void setEditAllowed(std::function<bool()> allowed) { m_editAllowed = std::move(allowed); }
  // The value under a view position, as "<sketch uid> <dimension id>"
  // (empty: none).
  QString valueAt(const QPointF& position) const;

signals:
  // A double-click on dimension `dimension` (k5) of sketch `sketch` (F3).
  void editRequested(const QString& sketch, const QString& dimension);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Paints them, in view pixels (the view's frame, ViewPaintLayer).
  void paint(QPainter& painter);
  struct Layout {
    std::size_t sketch = 0;    // in m_sketches
    std::size_t dimension = 0; // in its model's dimensions
    DimensionShape shape;
    QRectF textRect;
  };
  // Where each dimension of every sketch is drawn now, in view pixels.
  std::vector<Layout> layouts() const;
  Projection projection(const SketchModel& model) const;
  QString key(const Layout& layout) const;
  void hover(const QPointF& position, const QPoint& global);
  // Logs "Shown dimension Sketch1 k5 at x,y" (window coordinates) for each
  // value drawn, and "Sketch dimensions drawn outside sketch mode:
  // Sketch1 (2)", when they change, for the UI tests: after the view has
  // painted and come to rest (m_logTimer).
  void logPlaces();

  OcctViewer& m_viewer;
  ViewPaintLayer* m_layer = nullptr; // what the view draws them in
  QFont m_font;                      // sketch mode's dimensions' font
  std::vector<ShownSketch> m_sketches;
  double m_lengthScale = 1.0;
  QString m_lengthUnit;
  int m_textMode = 0;
  std::function<bool()> m_editAllowed;
  QString m_hovered;
  QTimer m_logTimer;
  QString m_loggedShown;
  QString m_loggedDrawn;
  QString m_loggedPlaces;
};

} // namespace mitcad::sketch
