// SPDX-License-Identifier: MIT
#pragma once

#include <optional>
#include <vector>

#include <QPoint>
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
// centred where it is drawn with the dragged corner under the pointer
// (where the pointer's ray meets the plane), the line stays centred with
// the dragged end nearest the pointer's ray; the new outline is drawn while
// the drag lasts and the size, rounded to a step that fits the zoom, goes
// to the model on the release (resized: the window's set_datum_display_size,
// one undo step). Only the drawing changes, never the datum. The mouse goes
// through to the view everywhere else; a press on a handle stays with it.
// The handles sit where the plane or axis is drawn (mitcad#1827): at the
// corners of its rectangle fitted to what it is made from, which a drag
// turns into a square of its own size about the same middle; a handle's
// context menu offers Reset Size while it has a size of its own
// (resetRequested: the window clears it, one undo step).
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
    double size = 100.0;   // as drawn now, mm: the rectangle's width, the line's length
    double height = 100.0; // a plane's rectangle's height, mm
    bool stored = false;   // drawn at a size of its own (display_size): Reset Size clears it
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
  // Reset Size in a handle's context menu (mitcad#1827).
  void resetRequested(const QString& uid);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // Where the handles are: the corners (or ends) in the design for a width
  // and a height (a plane's; an axis's line is `width` long).
  std::vector<gp_Pnt> handlePoints(double width, double height) const;
  // A handle's context menu (mitcad#1827): Reset Size, while a size of its
  // own is set.
  void showMenu(const QPoint& globalPosition);
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
  bool m_menuPress = false; // a right press on a handle: its release is the handles' too
  QPointF m_pressAt;
  double m_dragSize = 0.0;
  QString m_loggedPlaces;
};

} // namespace mitcad
