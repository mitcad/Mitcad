// SPDX-License-Identifier: MIT
#pragma once

#include <optional>
#include <vector>

#include <QPointF>
#include <QString>
#include <QWidget>

#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

#include "framework/EventFilterGuard.hpp"

namespace mitcad {

class OcctViewer;

// The resize handles of a selected construction plane or axis (mitcad#1748,
// GitHub mitcad/Mitcad#279; app/COMMANDS.md "Origin planes and the size of
// construction geometry"): a small square at each corner of the plane's
// square, a dot at each end of the axis's line, drawn over the 3D view in
// the handles' colour (the selection's while hovered or dragged). A left
// drag on one sets how large the plane or axis is drawn: the square stays
// centred on the plane's origin with the dragged corner under the pointer
// (where the pointer's ray meets the plane), the line stays centred with
// the dragged end nearest the pointer's ray; the new outline is drawn while
// the drag lasts and the size, rounded to a step that fits the zoom, goes
// to the model on the release (resized: the window's set_datum_display_size,
// one undo step). Only the drawing changes, never the datum. The mouse goes
// through to the view everywhere else; a press on a handle stays with it.
//
// For UI tests: "Datum handle <uid> corner <n> at x,y" (an axis's "end
// <n>"), window coordinates, whenever they move; "Datum handle <uid>
// resized to <mm> mm" on a release.
class DatumHandles : public QWidget {
  Q_OBJECT

public:
  struct Target {
    QString uid;
    bool plane = true;
    // In the design (placed by the occurrence that places it): a plane's
    // origin and its in-plane axes, an axis's origin and direction (xAxis).
    gp_Pnt centre;
    gp_Dir xAxis = gp_Dir(1, 0, 0);
    gp_Dir yAxis = gp_Dir(0, 1, 0);
    double size = 100.0; // as drawn now, mm: the square's side, the line's length
    bool operator==(const Target& other) const;
  };

  explicit DatumHandles(OcctViewer& viewer);
  ~DatumHandles() override;

  // The handles of a target, or none (hidden). A drag under way keeps its
  // target until it ends.
  void setTarget(const std::optional<Target>& target);
  bool isDragging() const { return m_drag >= 0; }

signals:
  void resized(const QString& uid, double size);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Where the handles are: the corners (or ends) in the design for a size.
  std::vector<gp_Pnt> handlePoints(double size) const;
  int handleAt(const QPointF& position) const;
  // The size a drag of handle `index` gives with the pointer there; none
  // while the view looks along the plane.
  std::optional<double> sizeAt(int index, const QPointF& position) const;
  bool viewerEvent(QEvent* event);
  void publish(); // the handles' areas to the view and their places to the log

  OcctViewer& m_viewer;
  EventFilterGuard m_filter{this};
  std::optional<Target> m_target;
  int m_hover = -1;
  int m_pressed = -1;
  int m_drag = -1;
  QPointF m_pressAt;
  double m_dragSize = 0.0;
  QString m_loggedPlaces;
};

} // namespace mitcad
