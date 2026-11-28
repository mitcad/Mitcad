// SPDX-License-Identifier: MIT
#pragma once

// Send Feedback on a Region (mitcad#2616): the overlay on which the user
// drags the region to report. One translucent, frameless window per screen
// over MitCad's windows (the main window, Floating chrome's panels and
// cards, a dialog that is not modal): a crosshair, the rest dimmed, the
// dragged rectangle clear with its size beside it. While it is up it holds
// the mouse and the keyboard (grabMouse, grabKeyboard, every shortcut
// taken), so nothing under it sees the drag.
//
// Both ways of the command's key: pressed once (the key, then a drag: the
// mouse's release takes the region) and held (the key down, a drag: the
// release of the mouse or of the key takes it). Esc, a click without a
// drag (FeedbackRegion.hpp, kRegionMinSide) or another button cancels.
// Done, it hides its windows at once and emits picked or canceled once;
// its owner deletes it (deleteLater).

#include <QList>
#include <QObject>
#include <QPoint>
#include <QPointer>
#include <QRect>
#include <QString>

class QWidget;

namespace mitcad::report {

class RegionPicker : public QObject {
  Q_OBJECT

public:
  // `area`: what it covers (global logical coordinates), on `screens`
  // (their geometries; one window for each part of the area on them).
  // `heldKey`: the command's key, whose release takes the region during a
  // drag (Qt::Key_unknown: none). `parent`: the main window's window, the
  // owner of the overlay's windows.
  RegionPicker(const QRect& area, const QList<QRect>& screens, int heldKey, QWidget* parent);
  ~RegionPicker() override;

  // Shows the windows and takes the mouse and the keys; false when the
  // area is on no screen (nothing shown).
  bool start();

  // The input, from the windows (global logical coordinates).
  void press(const QPoint& global, Qt::MouseButton button);
  void move(const QPoint& global);
  void release(const QPoint& global, Qt::MouseButton button);
  void keyPress(int key);
  void keyRelease(int key, bool autoRepeat);

  // The rectangle dragged so far, clipped to the area (null: no drag).
  QRect dragged() const;
  // Whether the left button is down on it.
  bool dragging() const { return m_pressed; }
  bool done() const { return m_done; }
  // The parts of the area it covers, one per window.
  const QList<QRect>& parts() const { return m_parts; }
  // Its windows (tests).
  QList<QWidget*> windows() const;
  // The line it shows until the drag starts.
  static QString hint();

signals:
  // The region (global logical coordinates, kRegionMinSide or more).
  void picked(const QRect& region);
  void canceled();

private:
  void finish(const QRect& region);
  void cancel(const QString& why);
  void repaint();

  QList<QRect> m_parts;
  QList<QPointer<QWidget>> m_windows;
  QPointer<QWidget> m_grabber;
  int m_heldKey;
  QPoint m_from;
  QPoint m_to;
  bool m_pressed = false;
  bool m_done = false;
};

} // namespace mitcad::report
