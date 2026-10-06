// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QPointF>
#include <QPolygonF>
#include <QString>
#include <QWidget>

#include "Command.hpp"

namespace mitcad {

class OcctViewer;

// The handles of a running command drawn over the 3D view (U4): an arrow
// that drags a distance along its direction, a ring that turns an angle
// about its axis. Drawn with QPainter in view
// pixels; the mouse goes through to the view except on a handle, which a
// left drag moves. The values come from the command's value inputs and go
// back to them as numbers rounded to a step that fits the zoom.
class ManipulatorOverlay : public QWidget {
  Q_OBJECT

public:
  struct Handle {
    QString key;   // the value input it drags
    QString label; // for the log
    Manipulator manipulator;
    double value = 0.0; // mm or radians
  };

  ManipulatorOverlay(OcctViewer& viewer, const QString& command);
  ~ManipulatorOverlay() override;

  void setHandles(std::vector<Handle> handles);
  bool isDragging() const { return m_drag >= 0; }

signals:
  void dragged(const QString& key, double value);
  void dragFinished(const QString& key);
  // A toggle was clicked (P9: a pattern's instance).
  void toggled(const QString& key, int element);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Drawn {
    QPointF origin; // the arrow's start or the ring's centre
    QPointF handle; // where it is grabbed
    QPointF tip;    // the arrow's head
    QPolygonF ring;
    bool valid = false;
  };

  bool viewerEvent(QEvent* event);
  Drawn draw(const Handle& handle) const;
  int handleAt(const QPointF& position) const;
  // The value a drag to the position gives (a ring keeps count of turns).
  double valueAt(const Handle& handle, const QPointF& position);
  // A ring: the angle of the position about its centre; NaN when edge-on.
  double ringAngle(const Handle& handle, const QPointF& position) const;
  double pixelSize(const gp_Pnt& at) const; // mm per pixel near a point
  void logPlaces();

  OcctViewer& m_viewer;
  QString m_command;
  std::vector<Handle> m_handles;
  int m_hover = -1;
  int m_drag = -1;
  int m_pending = -1; // pressed, not moved yet: a drag or a click
  QPointF m_pressAt;
  Qt::KeyboardModifiers m_pressModifiers;
  bool m_forwarding = false; // a click passed on to the view
  double m_dragStart = 0.0; // the value when the drag began
  QPointF m_grab;           // an arrow: the cursor from the handle then
  double m_angleLast = 0.0; // a ring: the cursor's last angle
  double m_turned = 0.0;    // and how far it turned since the drag began
  QString m_logged;
};

} // namespace mitcad
