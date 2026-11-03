// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include <QPointF>
#include <QString>
#include <QWidget>

#include <gp_Pnt.hxx>

#include "Comments.hpp"

namespace mitcad {

class OcctViewer;

// The comments' pins in the 3D view (mitcad#1119, app/COMMANDS.md
// "Comments"): a numbered pin at each comment's place (view/Comments.hpp
// draws one), kept where its point is as the camera moves, drawn over the
// bodies. A child of the view as large as it, under the view's own buttons
// and cards, as the annotations' overlay is (view/MeasurementOverlay). It
// lets the mouse through itself and watches the view's mouse instead: a
// click on a pin is taken (`clicked` with its id) and nothing else; the
// pointer resting on one shows its tooltip.
//
// For UI tests it logs "Comment pins drawn: C1 open | C2 dimmed | C3
// warning" (or "none"; "Comment pins drawn: hidden (n)" while they are
// turned off) when that changes, each pin's head as "Comment pin C1 at
// x,y" in the window's coordinates when the pins or the camera moved them,
// and "Comment pin clicked: C1".
class CommentPinOverlay : public QWidget {
public:
  struct Pin {
    QString id;
    int number = 0;
    gp_Pnt at; // the design's coordinates
    CommentPinLook look = CommentPinLook::Open;
    QString tip;
  };

  explicit CommentPinOverlay(OcctViewer* viewer);

  // The pins of the comments that have a place and are not resolved.
  void setPins(const std::vector<Pin>& pins);
  // View › Comment Pins: all of them shown or hidden.
  void setPinsShown(bool shown);
  bool pinsShown() const { return m_shown; }
  // The pin drawn with a ring (its row selected in the Comments panel).
  void setHighlighted(const QString& id);
  // Whether the pins take the mouse now (not while a command takes picks
  // or a sketch is edited); always when unset.
  std::function<bool()> active;
  std::function<void(const QString& id)> clicked;

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;

private:
  void refresh();
  void follow();
  // Where the pins' tips are on the view, rounded: drawn again and logged
  // when that changes.
  QString placesKey() const;
  void logPlaces();
  // The pin under a point of the view (the last drawn is on top); empty
  // when there is none.
  QString hitAt(const QPointF& at) const;
  bool mouse(QEvent* event);

  OcctViewer* m_viewer;
  std::vector<Pin> m_pins;
  bool m_shown = true;
  QString m_highlighted;
  QString m_drawnKey;
  QString m_pressed; // a press on a pin, whose release is the click
  bool m_hovering = false;
  QString m_loggedDrawn = QStringLiteral("Comment pins drawn: none");
  QString m_loggedPlaces;
};

} // namespace mitcad
