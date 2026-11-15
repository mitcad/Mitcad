// SPDX-License-Identifier: MIT
#pragma once

#include <map>
#include <utility>
#include <vector>

#include <QPointF>
#include <QString>
#include <QWidget>

#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "../commands/FormCage.hpp"
#include "../framework/EventFilterGuard.hpp"

namespace mitcad {

class OcctViewer;

// A Form's control cage over the 3D view while Edit Form is open
// (mitcad#1757, app/COMMANDS.md "Form"): its edges (creased ones thicker,
// those behind the cage dashed, the mirrored half lighter), its vertices
// (corners as squares) and, in the kind a click picks, the picked faces,
// edges or vertices in the selection's blue and the one under the pointer
// in the hover color. Drawn with QPainter in view pixels; a left click on
// an element of the kind picked (a face facing the viewer under the
// pointer, an edge within 7 px, a vertex within 9 px) picks it, Ctrl or
// Shift adds it or takes it out (pickRequested). While vertices are picked,
// a press on a vertex that moves more than 5 px drags it instead, in the
// plane through it that faces the viewer, and its release asks for it
// there (dragRequested, mitcad#1810). Elsewhere, and on the command's
// handles and the panels over the view, the mouse goes on to the view, so
// that it still turns, pans and zooms.
class FormCageOverlay : public QWidget {
  Q_OBJECT

public:
  explicit FormCageOverlay(OcctViewer& viewer);
  ~FormCageOverlay() override;

  // The cage (the `form_cage` answer read) in the Form's component's
  // coordinates, and where that component is seen in the design.
  void setCage(const forms::Cage& cage, const gp_Trsf& placement);
  // The picks drawn, the kind a click picks, and whether clicks pick at
  // all (not while Symmetry is chosen).
  void setPicked(const forms::Picked& picked, forms::Kind kind, bool picking);

signals:
  // A click asks for these picks (forms::pickedText).
  void pickRequested(const QString& picked);
  // A drag of a vertex asks for it where it was let go (forms::dragText, in
  // the Form's component's coordinates).
  void dragRequested(const QString& drag);

protected:
  void paintEvent(QPaintEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // What is under a point of the view.
  struct Hit {
    bool valid = false;
    forms::Kind kind = forms::Kind::Faces;
    qint64 id = 0;
    forms::EdgeIds edge{};
    bool operator==(const Hit& other) const {
      return valid == other.valid && kind == other.kind && id == other.id && forms::sameEdge(edge, other.edge);
    }
  };

  bool viewerEvent(QEvent* event);
  Hit hitAt(const QPointF& position) const;
  gp_Pnt designPoint(const forms::Point& point) const;
  QPointF viewPoint(qint64 vertex) const;
  // Whether a face looks towards the viewer (every face of an open cage
  // does: both sides are seen).
  bool facesViewer(const forms::Face& face) const;
  // Whether a vertex or an edge is on a face that looks towards the viewer.
  bool vertexSeen(qint64 vertex) const;
  bool edgeSeen(const forms::EdgeIds& edge) const;
  // For UI tests: where each element of the kind picked is seen, one line
  // each ("Form cage face 3 (+z) at x,y"), when they change.
  void logPlaces();
  static std::pair<qint64, qint64> edgeKey(const forms::EdgeIds& edge);

  OcctViewer& m_viewer;
  EventFilterGuard m_viewerFilter{this};
  forms::Cage m_cage;
  gp_Trsf m_placement;
  forms::Picked m_picked;
  forms::Kind m_kind = forms::Kind::Faces;
  bool m_picking = true;
  std::map<qint64, std::vector<std::size_t>> m_vertexFaces;              // the faces round each vertex
  std::map<std::pair<qint64, qint64>, std::vector<std::size_t>> m_edgeFaces; // the faces along each edge
  Hit m_hover;
  Hit m_pressed;
  QPointF m_pressAt;
  // A vertex dragged: where it was pressed (in the design) and where it is
  // now (in the Form's component), drawn there until the cage is read again.
  bool m_dragging = false;
  bool m_dragMoved = false;
  gp_Pnt m_dragFrom;
  forms::Point m_dragAt{};
  QString m_logged;
};

} // namespace mitcad
