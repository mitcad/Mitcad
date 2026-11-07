// SPDX-License-Identifier: MIT
#pragma once

#include <optional>
#include <vector>

#include <QPoint>
#include <QPointF>
#include <QPolygonF>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "Command.hpp"
#include "EventFilterGuard.hpp"

namespace mitcad {

class OcctViewer;

// The handles of a running command drawn over the 3D view (U4): an arrow
// that drags a distance along its direction, a ring that turns an angle
// about its axis. Drawn with QPainter in view
// pixels; the mouse goes through to the view except on a handle, which a
// left drag moves. A press that starts on a handle stays with it: without
// a drag it does nothing, and it never reaches the view or the
// orientation cube as a click (mitcad#1105). The values come from the
// command's value inputs and go
// back to them as numbers rounded to a step that fits the zoom, or the
// size of the geometry a handle works on when that is finer (mitcad#1431,
// HandleStep.hpp).
class ManipulatorOverlay : public QWidget {
  Q_OBJECT

public:
  struct Handle {
    QString key;   // the value input it drags
    QString label; // for the log
    Manipulator manipulator;
    double value = 0.0; // mm or radians
    // The value as the box shows it ("12.5 mm", Manipulator::valueBox).
    QString text = QString();
    // A length: its value at a drag's start may count as the arrow's scale
    // (Manipulator::valueScale, mitcad#1431).
    bool length = false;
    // The last decimal the value box shows, in the value's unit: an
    // arrow's step from its scale is not finer (mitcad#1431). 0: no floor.
    double finest = 0.0;
  };

  ManipulatorOverlay(OcctViewer& viewer, const QString& command);
  ~ManipulatorOverlay() override;

  void setHandles(std::vector<Handle> handles);
  // A line over the top of the view (Inspection::viewHint, mitcad#394);
  // none when empty. Logged as "View hint <command>: <text>".
  void setHint(const QString& text);
  // Lines of the command's own drawn over the view (mitcad#656: a belt's
  // pitch line), in design coordinates; none when empty.
  void setLines(std::vector<std::vector<gp_Pnt>> lines);
  // The inputs whose handles have value boxes, in the handles' order, and
  // where a box is (the view's coordinates; its handle's knob while it is
  // hidden at 0): Tab steps through them (mitcad#656).
  QStringList boxKeys() const;
  std::optional<QPoint> boxCentre(const QString& key) const;
  bool isDragging() const { return m_drag >= 0; }
  // Hidden with the command's preview while Ctrl is held (mitcad#1421):
  // neither drawn nor taking the mouse, and the view picks where its value
  // boxes were; shown again as they were.
  void setHandlesShown(bool shown);
  // The arrow of value input `key` as it is drawn, if it has one (a drag
  // proxy's direction, mitcad#636).
  std::optional<Manipulator> arrowOf(const QString& key) const {
    for (const Handle& handle : m_handles) {
      if (handle.key == key && handle.manipulator.kind == Manipulator::Kind::Arrow) {
        return handle.manipulator;
      }
    }
    return std::nullopt;
  }

signals:
  void dragged(const QString& key, double value);
  void dragFinished(const QString& key);
  // A toggle was clicked (P9: a pattern's instance).
  void toggled(const QString& key, int element);
  // A handle's value box was clicked (Manipulator::valueBox, mitcad#656):
  // `at` is the box's centre in the view's coordinates.
  void valueBoxClicked(const QString& key, const QPoint& at);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  friend class TestDriver;
  struct Drawn {
    QPointF origin; // the arrow's start or the ring's centre
    QPointF handle; // where it is grabbed
    QPointF tip;    // the arrow's head
    QPolygonF ring;
    QRectF box; // the value box (Manipulator::valueBox); empty without one
    QPolygonF arc; // a ring's angle as an arc from its zero (with a value box)
    QPointF zero;  // the end of a ring's zero line
    bool valid = false;
  };

  // Where each value box goes (m_boxes, by handle): off the others and
  // out of the panels floating over the view (mitcad#656).
  void layoutBoxes();
  // The value boxes' places to the view, which offers no picks there, and
  // where the handles take the mouse (mitcad#1105).
  void publishBoxes();
  bool viewerEvent(QEvent* event);
  Drawn draw(const Handle& handle) const;
  int handleAt(const QPointF& position) const;
  // The handle whose value box is at the position; -1 when none.
  int boxAt(const QPointF& position) const;
  struct DragValue {
    double snapped;
    double continuous;
  };
  // Only snapped goes to the command. Continuous is local presentation;
  // it must never require a model query while the document worker is busy.
  DragValue valueAt(const Handle& handle, const QPointF& position);
  // A ring: the angle of the position about its centre; NaN when edge-on.
  double ringAngle(const Handle& handle, const QPointF& position) const;
  // An arrow: the step its value rounds to, in the value's unit, from the
  // zoom at its origin, its scale or the value the drag started from, and
  // the zoom's step alone (HandleStep.hpp, mitcad#1431).
  struct ArrowStep {
    double step;
    double zoom;
  };
  ArrowStep arrowStep(const Handle& handle) const;
  double pixelSize(const gp_Pnt& at) const; // mm per pixel near a point
  void logPlaces();
  void logDragFeedback() const;

  OcctViewer& m_viewer;
  EventFilterGuard m_viewerFilter{this}; // its input; the view may go first (mitcad#226)
  QString m_command;
  std::vector<Handle> m_handles;
  int m_hover = -1;
  int m_drag = -1;
  int m_pending = -1; // pressed, not moved yet: a drag or a click
  QString m_boxKey; // a value box pressed: a click on it edits the value
  int m_boxHover = -1; // the value box under the mouse
  std::vector<QRectF> m_boxes; // layoutBoxes(), by handle; empty: none shown
  std::vector<std::vector<gp_Pnt>> m_lines; // setLines()
  QPointF m_pressAt;
  double m_dragStart = 0.0; // the value when the drag began
  double m_dragStep = 0.0;  // an arrow: the step it rounds to, fixed when the drag began
  QPointF m_grab;           // an arrow: the cursor from the handle then
  double m_angleLast = 0.0; // a ring: the cursor's last angle
  double m_turned = 0.0;    // and how far it turned since the drag began
  std::optional<double> m_visualValue;
  unsigned m_dragMoves = 0;
  unsigned m_semanticChanges = 0;
  unsigned m_visualUpdates = 0;
  unsigned m_dragPaints = 0;
  QString m_logged;
  QString m_hint;
};

} // namespace mitcad
