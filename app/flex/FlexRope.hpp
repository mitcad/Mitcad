// SPDX-License-Identifier: MIT
#pragma once

// A Flex item drawn over the 3D view as itself (mitcad#1713, app/COMMANDS.md
// "Flex editing"): while its route follows the pointer (a node dragged, an
// item being laid) no solid is built, so the item is painted from the
// route's samples with its section's real size on the screen and its own
// colour: a round section as a tube, any other as a strip that shows its
// twist. One drawer for the in-view editing (FlexEditController) and for the
// command that lays an item (commands/FlexCommands.cpp), whose overlay,
// FlexRopeOverlay, is here too.
//
//   const FlexItem item = FlexItem::read(flexQueryAnswer);   // with "frames": true
//   paintFlexRope(painter, ropeSamples(item, placement), ropeLook(item, colour), view);
//
// Nothing here reads the model.

#include <functional>
#include <optional>
#include <utility>
#include <vector>

#include <QColor>
#include <QPointF>
#include <QString>
#include <QWidget>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "FlexEdit.hpp"

class QPainter;

namespace mitcad {

class OcctViewer;

namespace flex {

// A sample of the route in the design's coordinates: where the section is,
// the route's direction and the section's y (its "up").
struct RopeSample {
  gp_Pnt point;
  gp_Dir tangent = gp_Dir(0, 0, 1);
  gp_Dir normal = gp_Dir(0, 1, 0);
};

// The section and the colour.
struct RopeLook {
  bool round = true;
  double width = 0.0;  // a round section's diameter, else its width (mm)
  double height = 0.0; // mm
  QColor color;        // the body's own
};

// How the view shows the design.
struct RopeView {
  std::function<QPointF(const gp_Pnt&)> project;   // a point in widget pixels
  std::function<double(const gp_Pnt&)> pixelSize;  // millimetres per pixel at a point
  gp_Dir towardViewer = gp_Dir(0, 0, 1);           // from the design towards the eye
};

// The samples of an item as the `flex` query gave it: its `frames` when it
// has them, else its `polyline` with the centre line's directions (enough
// for a round section); placed by `placement` (where the view shows the
// item's component).
std::vector<RopeSample> ropeSamples(const FlexItem& item, const gp_Trsf& placement);
RopeLook ropeLook(const FlexItem& item, const QColor& color);

// Paints the item along the samples. Fewer than two samples paint nothing.
void paintFlexRope(QPainter& painter, const std::vector<RopeSample>& samples, const RopeLook& look,
                   const RopeView& view);

// Half the section's reach on the screen at a sample, in pixels (at least
// 1.5): how far from the centre line the pointer is still over the item.
double ropeHalfPixels(const RopeSample& sample, const RopeLook& look, const RopeView& view);

// The Flex command's route under the pointer: a child of the view that
// takes no input and draws one previewed item with the same drawer
// (paintFlexRope, in the view's colour for bodies: it has no body yet),
// with a short text (the routed length) beside its last node. It follows
// the camera.
class FlexRopeOverlay : public QWidget {
public:
  // `pixelSize`: the size of a pixel of the view at a point of the design (mm).
  FlexRopeOverlay(OcctViewer& viewer, std::function<double(const gp_Pnt&)> pixelSize);

  // The route to draw, in the coordinates `placement` puts into the
  // design, and the text beside its end; none takes the drawing away.
  void show(const std::optional<FlexItem>& item, const gp_Trsf& placement, const QString& text);
  // A thin dashed straight line between two points of the design (owner
  // 14.11.2026: from the last node to the pointer while it moves, before
  // the route is worked out); none takes it away. Drawn beside the item.
  void showLine(const std::optional<std::pair<gp_Pnt, gp_Pnt>>& line);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  OcctViewer& m_viewer;
  std::function<double(const gp_Pnt&)> m_pixelSize;
  std::optional<FlexItem> m_item;
  gp_Trsf m_placement;
  QString m_text;
  std::optional<std::pair<gp_Pnt, gp_Pnt>> m_line;
};

// The dashed line of FlexRopeOverlay::showLine, painted in widget pixels:
// thin, in the given colour, over a halo so that it shows on any body.
void paintDashedLine(QPainter& painter, const QPointF& from, const QPointF& to, const QColor& color,
                     const QColor& halo);

} // namespace flex
} // namespace mitcad
