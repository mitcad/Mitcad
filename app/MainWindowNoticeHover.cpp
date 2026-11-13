// SPDX-License-Identifier: MIT
// The places a notice names, painted in the view while the pointer is over
// the notice (mitcad#1327; docs/ui-conventions.md "Messages and severity"):
// the command panel's message (its preview's notices; the faces of the
// preview's bodies first, where a warning's made faces are), a Timeline
// item with its tooltip, a feature's Browser row, the message band and the
// Docked status line (the notices of the feature the message is about).
// Faces, edges and vertices are drawn as the selection preview draws them,
// in the hover's colour and seen through what hides them; a point as a
// ball; a sketch region as its profile. Nothing is selected, picked or
// framed; moving away clears them. While a job computes the model they are
// kept and drawn once it has ended (mitcad#1600). A message shown in place
// of the hovered one in the message band or the status line gives its own
// feature's places, or none (mitcad#1609).
// Logged for the UI tests, when what is painted changes, with where it is
// drawn in the design's coordinates:
//   Notice hover highlighted: panel: face F3:fillet(E{…}) of F1.b0, point 1.000,2.000,3.000 (2 drawn)
//   Notice hover bounds: x 0.0 to 60.0, y 0.0 to 2.0, z 18.0 to 20.0
//   Notice hover highlighted: timeline: face F3:fillet(E{…}) of F1.b0 (drawn when the job ends)
//   Notice hover highlighted: none
#include "MainWindow.hpp"

#include <array>
#include <exception>
#include <functional>
#include <string>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <Bnd_Box.hxx>
#include <TopoDS_Compound.hxx>
#include <BRep_Builder.hxx>
#include <gp_Pnt.hxx>

#include <QEvent>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QtLogging>

#include "framework/NoticePlaces.hpp"
#include "framework/Theme.hpp"

namespace mitcad {

namespace {

// A point of a place's three numbers; false when they are not.
bool pointOf(const QJsonValue& value, gp_Pnt& point) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() != 3) {
    return false;
  }
  point.SetCoord(xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble());
  return true;
}

// The faces, edges or vertices of a named shape by a name, in one shape
// (null for none).
TopoDS_Shape namedPart(const geometry::Shape& shape, SelectKind kind, const std::string& name) {
  std::vector<int> found;
  try {
    found = kind == SelectKind::Face   ? shape.find_faces(name)
            : kind == SelectKind::Edge ? shape.find_edges(name)
                                       : shape.find_vertices(name);
  } catch (const std::exception&) {
    return TopoDS_Shape();
  }
  if (found.empty()) {
    return TopoDS_Shape();
  }
  if (found.size() == 1) {
    const int i = found.front();
    return kind == SelectKind::Face ? TopoDS_Shape(shape.face(i))
           : kind == SelectKind::Edge ? TopoDS_Shape(shape.edge(i))
                                      : TopoDS_Shape(shape.vertex(i));
  }
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  for (const int i : found) {
    builder.Add(compound, kind == SelectKind::Face   ? TopoDS_Shape(shape.face(i))
                          : kind == SelectKind::Edge ? TopoDS_Shape(shape.edge(i))
                                                     : TopoDS_Shape(shape.vertex(i)));
  }
  return compound;
}

// Calls `hovered` with true while the pointer moves over a widget and with
// false when it leaves it (or the widget hides).
class HoverWatch : public QObject {
public:
  HoverWatch(QWidget* widget, std::function<void(bool)> hovered)
      : QObject(widget), m_hovered(std::move(hovered)) {
    widget->setMouseTracking(true);
    widget->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    switch (event->type()) {
    case QEvent::Enter:
    case QEvent::MouseMove:
    case QEvent::HoverMove:
      if (!m_on) {
        m_on = true;
        m_hovered(true);
      }
      break;
    case QEvent::Leave:
    case QEvent::Hide:
      if (m_on) {
        m_on = false;
        m_hovered(false);
      }
      break;
    default:
      break;
    }
    return QObject::eventFilter(watched, event);
  }

private:
  std::function<void(bool)> m_hovered;
  bool m_on = false;
};

} // namespace

void MainWindow::hoverNoticePlaces(const QString& source, const QJsonArray& places,
                                   const std::vector<BodyDisplay>& preview) {
  // Another notice's pointer leaving does not take these away.
  if (places.isEmpty() && !m_noticeHoverSource.isEmpty() && source != m_noticeHoverSource) {
    return;
  }
  m_noticeHoverSource = places.isEmpty() ? QString() : source;
  m_noticeHover = places;
  m_noticeHoverBodies = places.isEmpty() ? std::vector<BodyDisplay>() : preview;
  if (modelBusy()) {
    // The worker's now (mitcad#1600: a command's preview or an asynchronous
    // command computes, the window stays live): the places are kept and
    // neither counted nor drawn, since their shapes read the document;
    // both once the job has ended, unless the pointer left them by then.
    const QString logged = noticeHoverText(source, places, -1);
    if (logged != m_loggedNoticeHover) {
      m_loggedNoticeHover = logged;
      qDebug().noquote() << QStringLiteral("Notice hover highlighted: %1").arg(logged);
    }
    if (!m_noticeHoverWaits) {
      m_noticeHoverWaits = true;
      whenIdle(this, [this] {
        m_noticeHoverWaits = false;
        const QString waited = m_noticeHoverSource;
        const QJsonArray shown = m_noticeHover;
        const std::vector<BodyDisplay> bodies = m_noticeHoverBodies;
        hoverNoticePlaces(waited, shown, bodies);
      });
    }
    return;
  }
  const std::vector<HighlightDisplay> highlights = noticeHighlights();
  const QString logged = noticeHoverText(source, places, static_cast<int>(highlights.size()));
  if (logged != m_loggedNoticeHover) {
    m_loggedNoticeHover = logged;
    qDebug().noquote() << QStringLiteral("Notice hover highlighted: %1").arg(logged);
    // Where they are drawn, in the design's coordinates (mitcad#1609: a
    // component's sketch region where its occurrence places it).
    Bnd_Box box;
    for (const HighlightDisplay& shown : highlights) {
      if (!shown.shape.IsNull()) {
        BRepBndLib::Add(shown.shape, box);
      }
    }
    if (!box.IsVoid()) {
      double x0 = 0;
      double y0 = 0;
      double z0 = 0;
      double x1 = 0;
      double y1 = 0;
      double z1 = 0;
      box.Get(x0, y0, z0, x1, y1, z1);
      qDebug().noquote() << QStringLiteral("Notice hover bounds: x %1 to %2, y %3 to %4, z %5 to %6")
                                .arg(x0, 0, 'f', 1)
                                .arg(x1, 0, 'f', 1)
                                .arg(y0, 0, 'f', 1)
                                .arg(y1, 0, 'f', 1)
                                .arg(z0, 0, 'f', 1)
                                .arg(z1, 0, 'f', 1);
    }
  }
  if (m_mode == Mode::Command && m_session) {
    m_session->refreshHighlights();
  } else {
    updateIdleHighlights();
  }
}

void MainWindow::sketchConflict(const QStringList& items, bool hovered) {
  // The command panel's refusal (mitcad#1566): in the sketch being edited.
  if (m_sketch == nullptr) {
    return;
  }
  m_sketch->setConflict(QStringLiteral("panel"), items);
  m_sketch->setConflictHovered(QStringLiteral("panel"), hovered);
}

void MainWindow::previewNoticePlaces(const QString& source, const QString& feature) {
  const DocumentSnapshot::Feature* found = feature.isEmpty() ? nullptr : m_snapshot.feature(feature);
  hoverNoticePlaces(source, found != nullptr ? found->places : QJsonArray(), {});
}

std::vector<HighlightDisplay> MainWindow::noticeHighlights() const {
  std::vector<HighlightDisplay> drawn;
  if (m_noticeHover.isEmpty()) {
    return drawn;
  }
  const QColor color = viewColor(ColorRole::Hover, false);
  const auto add = [&drawn, &color](const SelectionItem& item, const TopoDS_Shape& shape) {
    if (shape.IsNull()) {
      return;
    }
    HighlightDisplay shown{item, shape, color};
    // Seen where it is hidden too, never picked (as the Timeline's hover).
    shown.pickable = false;
    shown.hoverLook =
        item.kind == SelectKind::Face || item.kind == SelectKind::Body || item.kind == SelectKind::Profile;
    shown.throughSolid = true;
    drawn.push_back(std::move(shown));
  };
  // Points are in the coordinates of the body the notice is about, placed
  // as it is placed (the first place that names one).
  QString noticeBody;
  for (const QJsonValue& value : m_noticeHover) {
    noticeBody = value.toObject().value(QStringLiteral("body")).toString();
    if (!noticeBody.isEmpty()) {
      break;
    }
  }
  const QString placing = noticeBody.isEmpty() ? QString() : bodyOccurrence(noticeBody);
  const TopLoc_Location placed = placing.isEmpty() ? TopLoc_Location() : placementOf(placing);
  for (const QJsonValue& value : m_noticeHover) {
    const QJsonObject place = value.toObject();
    const QString kind = place.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("point")) {
      gp_Pnt at;
      if (pointOf(place.value(QStringLiteral("at")), at)) {
        add(SelectionItem{SelectKind::Point, noticeBody, QString(), QString(), QString()},
            BRepBuilderAPI_MakeVertex(at).Vertex().Moved(placed));
      }
      continue;
    }
    if (kind == QStringLiteral("box")) {
      gp_Pnt low;
      gp_Pnt high;
      if (pointOf(place.value(QStringLiteral("min")), low) && pointOf(place.value(QStringLiteral("max")), high) &&
          high.X() > low.X() && high.Y() > low.Y() && high.Z() > low.Z()) {
        add(SelectionItem{SelectKind::Body, noticeBody, QString(), QString(), QString()},
            BRepPrimAPI_MakeBox(low, high).Shape().Moved(placed));
      }
      continue;
    }
    if (kind == QStringLiteral("occurrence")) {
      // An occurrence with every body it places (mitcad#1745: what holds a
      // joint's side), where the design places it.
      const QString path = place.value(QStringLiteral("path")).toString();
      if (!path.isEmpty()) {
        const SelectionItem item{SelectKind::Component, QString(), QString(), QString(), path};
        add(item, itemShape(item));
      }
      continue;
    }
    if (kind == QStringLiteral("region")) {
      // A region of a sketch (mitcad#1334), drawn as its profile where the
      // sketch is shown: a component's where its occurrence places it
      // (mitcad#1609); one of no sketch, or one the sketch no longer has,
      // draws nothing.
      const QString sketch = place.value(QStringLiteral("sketch")).toString();
      const QString name = place.value(QStringLiteral("name")).toString();
      if (!sketch.isEmpty() && !name.isEmpty()) {
        const SelectionItem item{SelectKind::Profile, sketch, name, QString(), placingOccurrence(sketch)};
        add(item, itemShape(item));
      }
      continue;
    }
    // The 3D Sketch Palette's row under the pointer (mitcad#1652): a point
    // or curve of a sketch (the edited one, or another sketch a reference
    // follows), or an Intersection Curve, where the view shows it, or where
    // the place's occurrence puts it.
    const QString occurrence = place.value(QStringLiteral("occurrence")).toString();
    if (kind == QStringLiteral("sketch_point") || kind == QStringLiteral("sketch_curve")) {
      const QString sketch = place.value(QStringLiteral("sketch")).toString();
      const QString entity = place.value(QStringLiteral("name")).toString();
      if (!sketch.isEmpty() && !entity.isEmpty()) {
        const SelectionItem item{kind == QStringLiteral("sketch_point") ? SelectKind::SketchPoint
                                                                         : SelectKind::SketchCurve,
                                 sketch, entity, QString(),
                                 occurrence.isEmpty() ? placingOccurrence(sketch) : occurrence};
        add(item, itemShape(item));
      }
      continue;
    }
    if (kind == QStringLiteral("feature")) {
      const QString feature = place.value(QStringLiteral("name")).toString();
      if (!feature.isEmpty()) {
        const SelectionItem item{SelectKind::Feature, feature, QString(), QString(),
                                 occurrence.isEmpty() ? placingOccurrence(feature) : occurrence};
        add(item, itemShape(item));
      }
      continue;
    }
    const SelectKind selectKind = kind == QStringLiteral("face")   ? SelectKind::Face
                                  : kind == QStringLiteral("edge") ? SelectKind::Edge
                                  : kind == QStringLiteral("vertex") ? SelectKind::Vertex
                                                                     : SelectKind::None;
    const QString name = place.value(QStringLiteral("name")).toString();
    if (selectKind == SelectKind::None || name.isEmpty()) {
      continue;
    }
    const QString body = place.value(QStringLiteral("body")).toString();
    // Another component's, where that occurrence puts it (mitcad#1652: a
    // linked reference's source).
    if (!occurrence.isEmpty() && !body.isEmpty()) {
      const SelectionItem item{selectKind, body, name, QString(), occurrence};
      add(item, itemShape(item));
      continue;
    }
    // A command preview's bodies first: a warning's made faces are there.
    bool found = false;
    for (const BodyDisplay& shown : m_noticeHoverBodies) {
      if (!shown.shape || (!body.isEmpty() && QString::fromStdString(shown.uid) != body)) {
        continue;
      }
      const TopoDS_Shape part = namedPart(*shown.shape, selectKind, name.toStdString());
      if (!part.IsNull()) {
        add(SelectionItem{selectKind, QString::fromStdString(shown.uid), name, QString(),
                          QString::fromStdString(shown.occurrence)},
            part.Moved(shown.placement));
        found = true;
      }
    }
    if (found) {
      continue;
    }
    // Else the design's bodies: the one named, or each that has the name.
    const QStringList bodies = body.isEmpty() ? bodyUids() : QStringList{body};
    for (const QString& uid : bodies) {
      const SelectionItem item{selectKind, uid, name, QString(), bodyOccurrence(uid)};
      const TopoDS_Shape shape = itemShape(item);
      if (!shape.IsNull()) {
        add(item, shape);
        if (!body.isEmpty()) {
          break;
        }
      }
    }
  }
  return drawn;
}

void MainWindow::installNoticeHover() {
  if (m_statusLabel == nullptr) {
    return;
  }
  new HoverWatch(m_statusLabel, [this](bool hovered) {
    m_statusHovered = hovered;
    updateStatusNoticeHover();
  });
}

void MainWindow::updateStatusNoticeHover() {
  // Also when another message takes the hovered one's place (mitcad#1609):
  // the new one's feature, or none.
  if (m_statusNoticeHover.update(m_statusHovered, m_statusFeature)) {
    previewNoticePlaces(QStringLiteral("status line"), m_statusNoticeHover.feature());
  }
}

} // namespace mitcad
