// SPDX-License-Identifier: MIT
// A Form's control cage drawn and picked over the view while Edit Form is
// open (mitcad#1757, FormCageOverlay.hpp).
#include "FormCageOverlay.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <limits>
#include <optional>
#include <utility>

#include <QMouseEvent>
#include <QPainter>
#include <QPen>
#include <QPolygonF>
#include <QRectF>
#include <QStringList>
#include <QtLogging>

#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

constexpr double kVertexReach = 9.0; // px
constexpr double kEdgeReach = 7.0;   // px
constexpr double kClickSlop = 5.0;   // px a click may move and stay a click

double distanceToSegment(const QPointF& p, const QPointF& a, const QPointF& b) {
  const QPointF ab = b - a;
  const double length2 = QPointF::dotProduct(ab, ab);
  const double t = length2 > 1e-12 ? std::clamp(QPointF::dotProduct(p - a, ab) / length2, 0.0, 1.0) : 0.0;
  const QPointF d = a + ab * t - p;
  return std::hypot(d.x(), d.y());
}

bool finite(const QPointF& p) { return std::isfinite(p.x()) && std::isfinite(p.y()); }

QColor withAlpha(QColor color, int alpha) {
  color.setAlpha(alpha);
  return color;
}

// A face's direction for the log: the axis it mostly faces ("+z"), or
// "slanted".
QString facing(const gp_Dir& normal) {
  const double c[3] = {normal.X(), normal.Y(), normal.Z()};
  const char* names[3] = {"x", "y", "z"};
  for (int k = 0; k < 3; ++k) {
    if (std::abs(c[k]) > 0.9) {
      return (c[k] > 0 ? QStringLiteral("+") : QStringLiteral("-")) + QString::fromLatin1(names[k]);
    }
  }
  return QStringLiteral("slanted");
}

} // namespace

FormCageOverlay::FormCageOverlay(OcctViewer& viewer) : QWidget(&viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("formCageOverlay"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  resize(viewer.size());
  show();
  m_viewerFilter.install(&viewer);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    update();
    logPlaces();
  });
}

FormCageOverlay::~FormCageOverlay() = default;

std::pair<qint64, qint64> FormCageOverlay::edgeKey(const forms::EdgeIds& edge) {
  return {std::min(edge[0], edge[1]), std::max(edge[0], edge[1])};
}

void FormCageOverlay::setCage(const forms::Cage& cage, const gp_Trsf& placement) {
  m_cage = cage;
  m_placement = placement;
  m_vertexFaces.clear();
  m_edgeFaces.clear();
  for (std::size_t f = 0; f < m_cage.faces.size(); ++f) {
    const QVector<qint64>& corners = m_cage.faces[f].vertices;
    for (int i = 0; i < corners.size(); ++i) {
      m_vertexFaces[corners.at(i)].push_back(f);
      m_edgeFaces[edgeKey({corners.at(i), corners.at((i + 1) % corners.size())})].push_back(f);
    }
  }
  m_hover = Hit();
  m_pressed = Hit();
  m_dragging = false;
  m_dragMoved = false;
  update();
  logPlaces();
}

void FormCageOverlay::setPicked(const forms::Picked& picked, forms::Kind kind, bool picking) {
  const bool kindChanged = kind != m_kind || picking != m_picking;
  m_picked = picked;
  m_kind = kind;
  m_picking = picking;
  if (kindChanged) {
    m_hover = Hit();
    m_logged.clear();
  }
  update();
  logPlaces();
}

gp_Pnt FormCageOverlay::designPoint(const forms::Point& point) const {
  return gp_Pnt(point[0], point[1], point[2]).Transformed(m_placement);
}

QPointF FormCageOverlay::viewPoint(qint64 vertex) const {
  const forms::Vertex* v = m_cage.vertex(vertex);
  if (v == nullptr) {
    return QPointF(std::numeric_limits<double>::quiet_NaN(), std::numeric_limits<double>::quiet_NaN());
  }
  return m_viewer.toWidgetF(designPoint(v->at));
}

bool FormCageOverlay::facesViewer(const forms::Face& face) const {
  if (!m_cage.closed) {
    return true;
  }
  const gp_Vec normal(face.normal[0], face.normal[1], face.normal[2]);
  if (normal.Magnitude() < 1e-12) {
    return true;
  }
  const gp_Dir outwards = gp_Dir(normal).Transformed(m_placement);
  return outwards.Dot(m_viewer.viewDirection()) < 0.0;
}

bool FormCageOverlay::vertexSeen(qint64 vertex) const {
  const auto found = m_vertexFaces.find(vertex);
  if (found == m_vertexFaces.end()) {
    return true;
  }
  return std::any_of(found->second.begin(), found->second.end(),
                     [this](std::size_t f) { return facesViewer(m_cage.faces[f]); });
}

bool FormCageOverlay::edgeSeen(const forms::EdgeIds& edge) const {
  const auto found = m_edgeFaces.find(edgeKey(edge));
  if (found == m_edgeFaces.end()) {
    return true;
  }
  return std::any_of(found->second.begin(), found->second.end(),
                     [this](std::size_t f) { return facesViewer(m_cage.faces[f]); });
}

FormCageOverlay::Hit FormCageOverlay::hitAt(const QPointF& position) const {
  Hit best;
  best.kind = m_kind;
  // The orientation cube's area is the cube's: its clicks and its context
  // menu (mitcad#1941: a right click there opened the cube's menu, not
  // Edit Form's).
  if (m_viewer.overlayCubeArea().contains(position.toPoint())) {
    return best;
  }
  switch (m_kind) {
  case forms::Kind::Vertices: {
    double nearest = kVertexReach;
    for (const forms::Vertex& v : m_cage.vertices) {
      if (!vertexSeen(v.id)) {
        continue;
      }
      const QPointF at = viewPoint(v.id);
      if (!finite(at)) {
        continue;
      }
      const double distance = std::hypot(at.x() - position.x(), at.y() - position.y());
      if (distance <= nearest) {
        nearest = distance;
        best.valid = true;
        best.id = v.id;
      }
    }
    break;
  }
  case forms::Kind::Edges: {
    double nearest = kEdgeReach;
    for (const forms::Edge& e : m_cage.edges) {
      if (!edgeSeen(e.vertices)) {
        continue;
      }
      const QPointF a = viewPoint(e.vertices[0]);
      const QPointF b = viewPoint(e.vertices[1]);
      if (!finite(a) || !finite(b)) {
        continue;
      }
      const double distance = distanceToSegment(position, a, b);
      if (distance <= nearest) {
        nearest = distance;
        best.valid = true;
        best.edge = e.vertices;
      }
    }
    break;
  }
  case forms::Kind::Faces: {
    // The face under the pointer nearest the viewer.
    double nearest = std::numeric_limits<double>::infinity();
    const gp_Dir towards = m_viewer.viewDirection();
    for (const forms::Face& face : m_cage.faces) {
      if (!facesViewer(face)) {
        continue;
      }
      QPolygonF polygon;
      bool whole = true;
      for (const qint64 v : face.vertices) {
        const QPointF at = viewPoint(v);
        whole = whole && finite(at);
        polygon << at;
      }
      if (!whole || !polygon.containsPoint(position, Qt::OddEvenFill)) {
        continue;
      }
      const gp_Pnt middle = designPoint(face.center);
      const double depth = gp_Vec(middle.XYZ()).Dot(gp_Vec(towards));
      if (depth < nearest) {
        nearest = depth;
        best.valid = true;
        best.id = face.id;
      }
    }
    break;
  }
  }
  return best;
}

bool FormCageOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer) {
    return QWidget::eventFilter(watched, event);
  }
  if (isHidden() && event->type() != QEvent::Resize) {
    return false;
  }
  // No exception leaves a Qt event handler: a cage that cannot be worked
  // out takes no events.
  try {
    return viewerEvent(event);
  } catch (const std::exception& failure) {
    qWarning().noquote() << "Form cage:" << failure.what();
    m_pressed = Hit();
    m_dragging = false;
    return false;
  }
}

bool FormCageOverlay::viewerEvent(QEvent* event) {
  switch (event->type()) {
  case QEvent::Resize:
    resize(m_viewer.size());
    return false;
  case QEvent::Leave:
    if (m_hover.valid) {
      m_hover = Hit();
      update();
    }
    return false;
  case QEvent::MouseMove: {
    const auto* e = static_cast<QMouseEvent*>(event);
    if (m_pressed.valid && m_pressed.kind == forms::Kind::Vertices) {
      // A vertex dragged once the press has moved more than a click may
      // (mitcad#1810): in the plane through it that faces the viewer.
      const QPointF moved = e->position() - m_pressAt;
      if (!m_dragging && std::hypot(moved.x(), moved.y()) > kClickSlop) {
        m_dragging = true;
        qDebug().noquote() << QStringLiteral("Form cage drag: vertex %1").arg(m_pressed.id);
      }
      if (m_dragging) {
        if (const std::optional<gp_Pnt> at = m_viewer.viewPlanePoint(e->position(), m_dragFrom)) {
          const gp_Pnt local = at->Transformed(m_placement.Inverted());
          m_dragAt = forms::Point{local.X(), local.Y(), local.Z()};
          m_dragMoved = true;
          for (forms::Vertex& v : m_cage.vertices) {
            if (v.id == m_pressed.id) {
              v.at = m_dragAt; // drawn there until the cage is read again
            }
          }
          update();
        }
      }
      return true;
    }
    if (m_pressed.valid) {
      return true; // a press on the cage: its click comes with the release
    }
    if (e->buttons() != Qt::NoButton) {
      return false; // the view's own drag (turn, pan)
    }
    const Hit hover = m_picking && !m_viewer.isCovered(e->position()) ? hitAt(e->position()) : Hit();
    if (!(hover == m_hover)) {
      m_hover = hover;
      update();
    }
    return false;
  }
  case QEvent::MouseButtonPress:
  case QEvent::MouseButtonDblClick: {
    const auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || !m_picking || m_viewer.isCovered(e->position())) {
      return false;
    }
    const Hit hit = hitAt(e->position());
    if (!hit.valid) {
      return false; // off the cage: the view's
    }
    if (event->type() == QEvent::MouseButtonDblClick) {
      return true; // the view does not take a double-click on the cage
    }
    m_pressed = hit;
    m_pressAt = e->position();
    m_dragging = false;
    m_dragMoved = false;
    if (hit.kind == forms::Kind::Vertices) {
      if (const forms::Vertex* v = m_cage.vertex(hit.id)) {
        m_dragFrom = designPoint(v->at);
      }
    }
    return true;
  }
  case QEvent::MouseButtonRelease: {
    const auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || !m_pressed.valid) {
      return false;
    }
    const Hit pressed = m_pressed;
    m_pressed = Hit();
    if (m_dragging) {
      // The vertex where it was let go, as the model is to put it.
      m_dragging = false;
      if (m_dragMoved) {
        m_dragMoved = false;
        forms::Drag drag;
        drag.vertex = pressed.id;
        drag.at = m_dragAt;
        qDebug().noquote() << QStringLiteral("Form cage drag: vertex %1 let go at %2, %3, %4")
                                  .arg(pressed.id)
                                  .arg(m_dragAt[0], 0, 'f', 3)
                                  .arg(m_dragAt[1], 0, 'f', 3)
                                  .arg(m_dragAt[2], 0, 'f', 3);
        emit dragRequested(forms::dragText(drag));
      }
      return true;
    }
    const QPointF moved = e->position() - m_pressAt;
    if (std::hypot(moved.x(), moved.y()) > kClickSlop) {
      return true; // dragged off: no click
    }
    const bool add = e->modifiers().testFlag(Qt::ControlModifier) || e->modifiers().testFlag(Qt::ShiftModifier);
    const forms::Picked next = forms::clicked(m_picked, pressed.kind, pressed.id, pressed.edge, add);
    qDebug().noquote() << QStringLiteral("Form cage click: %1 %2%3 -> %4")
                              .arg(forms::kindKey(pressed.kind),
                                   pressed.kind == forms::Kind::Edges
                                       ? QStringLiteral("%1,%2").arg(pressed.edge[0]).arg(pressed.edge[1])
                                       : QString::number(pressed.id),
                                   add ? QStringLiteral(" (added)") : QString(), forms::pickedText(next));
    emit pickRequested(forms::pickedText(next));
    return true;
  }
  default:
    return false;
  }
}

void FormCageOverlay::paintEvent(QPaintEvent*) {
  if (m_cage.isEmpty()) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor cage = viewColor(ColorRole::Handle, false);
  const QColor picked = viewColor(ColorRole::Select, false);
  const QColor hover = viewColor(ColorRole::Hover, false);

  // The faces picked or under the pointer, filled see-through.
  if (m_kind == forms::Kind::Faces) {
    for (const forms::Face& face : m_cage.faces) {
      const bool isPicked = m_picked.faces.contains(face.id);
      const bool isHovered = m_hover.valid && m_hover.id == face.id;
      if ((!isPicked && !isHovered) || !facesViewer(face)) {
        continue;
      }
      QPolygonF polygon;
      for (const qint64 v : face.vertices) {
        polygon << viewPoint(v);
      }
      painter.setPen(Qt::NoPen);
      painter.setBrush(withAlpha(isHovered ? hover : picked, isPicked && isHovered ? 140 : 90));
      painter.drawPolygon(polygon);
    }
  }

  // The edges: creased ones thicker (semi-sharp ones less so), those
  // behind the cage dashed, the mirrored half lighter.
  for (const forms::Edge& edge : m_cage.edges) {
    const QPointF a = viewPoint(edge.vertices[0]);
    const QPointF b = viewPoint(edge.vertices[1]);
    if (!finite(a) || !finite(b)) {
      continue;
    }
    const bool seen = edgeSeen(edge.vertices);
    const bool mirrored = edge.vertices[0] < 0 || edge.vertices[1] < 0;
    const double width = !edge.creased ? 1.4 : edge.sharpness ? 2.1 : 2.8;
    QPen pen(withAlpha(cage, !seen ? 90 : mirrored ? 150 : 235), width,
             seen ? Qt::SolidLine : Qt::DashLine, Qt::RoundCap);
    if (m_kind == forms::Kind::Edges) {
      const bool isPicked = m_picked.contains(forms::Kind::Edges, 0, edge.vertices);
      const bool isHovered = m_hover.valid && forms::sameEdge(m_hover.edge, edge.vertices);
      if (isPicked || isHovered) {
        pen = QPen(isHovered ? hover : picked, 4.0, Qt::SolidLine, Qt::RoundCap);
      }
    }
    painter.setPen(pen);
    painter.drawLine(a, b);
  }

  // The vertices: corners as squares.
  for (const forms::Vertex& v : m_cage.vertices) {
    const QPointF at = viewPoint(v.id);
    if (!finite(at)) {
      continue;
    }
    const bool seen = vertexSeen(v.id);
    double radius = m_kind == forms::Kind::Vertices ? 3.5 : 2.2;
    QColor color = withAlpha(cage, seen ? 235 : 90);
    if (m_kind == forms::Kind::Vertices) {
      const bool isPicked = m_picked.vertices.contains(v.id);
      const bool isHovered = m_hover.valid && m_hover.id == v.id;
      if (isPicked || isHovered) {
        radius = 5.5;
        color = isHovered ? hover : picked;
      }
    }
    painter.setPen(Qt::NoPen);
    painter.setBrush(color);
    if (v.corner) {
      painter.drawRect(QRectF(at.x() - radius, at.y() - radius, 2 * radius, 2 * radius));
    } else {
      painter.drawEllipse(at, radius, radius);
    }
  }
}

void FormCageOverlay::logPlaces() {
  if (!isVisible() || m_cage.isEmpty()) {
    return;
  }
  QStringList lines;
  const auto add = [this, &lines](const QString& what, const QPointF& place) {
    if (!finite(place) || !rect().contains(place.toPoint()) || m_viewer.isCovered(place) ||
        m_viewer.overlayCubeArea().contains(place.toPoint())) {
      return;
    }
    const QPoint at = mapTo(window(), place.toPoint());
    lines << QStringLiteral("Form cage %1 at %2,%3").arg(what).arg(at.x()).arg(at.y());
  };
  switch (m_kind) {
  case forms::Kind::Faces:
    for (const forms::Face& face : m_cage.faces) {
      if (!facesViewer(face)) {
        continue;
      }
      const gp_Vec normal(face.normal[0], face.normal[1], face.normal[2]);
      const QString direction = normal.Magnitude() > 1e-12 ? facing(gp_Dir(normal).Transformed(m_placement))
                                                          : QStringLiteral("slanted");
      add(QStringLiteral("face %1 (%2)").arg(face.id).arg(direction), m_viewer.toWidgetF(designPoint(face.center)));
    }
    break;
  case forms::Kind::Edges:
    for (const forms::Edge& edge : m_cage.edges) {
      if (!edgeSeen(edge.vertices)) {
        continue;
      }
      const QPointF a = viewPoint(edge.vertices[0]);
      const QPointF b = viewPoint(edge.vertices[1]);
      // An edge of an opening says so (mitcad#1941: Fill Hole, Bridge).
      add(QStringLiteral("edge %1,%2%3%4")
              .arg(edge.vertices[0])
              .arg(edge.vertices[1])
              .arg(!edge.creased     ? QString()
                   : edge.sharpness ? QStringLiteral(" (semi-sharp %1)").arg(*edge.sharpness)
                                    : QStringLiteral(" (creased)"))
              .arg(edge.boundary ? QStringLiteral(" (boundary)") : QString()),
          (a + b) / 2.0);
    }
    break;
  case forms::Kind::Vertices:
    for (const forms::Vertex& v : m_cage.vertices) {
      if (vertexSeen(v.id)) {
        add(QStringLiteral("vertex %1").arg(v.id), viewPoint(v.id));
      }
    }
    break;
  }
  const QString joined = lines.join(QLatin1Char('\n'));
  if (joined == m_logged) {
    return;
  }
  m_logged = joined;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

} // namespace mitcad
