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
// framed; moving away clears them.
// Logged for the UI tests, when what is painted changes:
//   Notice hover highlighted: panel: face F3:fillet(E{…}) of F1.b0, point 1.000,2.000,3.000 (2 drawn)
//   Notice hover highlighted: none
#include "MainWindow.hpp"

#include <array>
#include <exception>
#include <functional>
#include <string>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <TopoDS_Compound.hxx>
#include <BRep_Builder.hxx>
#include <gp_Pnt.hxx>

#include <QEvent>
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
  const std::size_t drawn = noticeHighlights().size();
  const QString logged = places.isEmpty() ? QStringLiteral("none")
                                          : QStringLiteral("%1: %2 (%3 drawn)")
                                                .arg(source, placesText(places))
                                                .arg(static_cast<qulonglong>(drawn));
  if (logged != m_loggedNoticeHover) {
    m_loggedNoticeHover = logged;
    qDebug().noquote() << QStringLiteral("Notice hover highlighted: %1").arg(logged);
  }
  if (modelBusy()) {
    return; // (Drawn with the next idle refresh.)
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
    if (kind == QStringLiteral("region")) {
      // A region of a sketch (mitcad#1334), drawn as its profile; one of
      // no sketch, or one the sketch no longer has, draws nothing.
      const QString sketch = place.value(QStringLiteral("sketch")).toString();
      const QString name = place.value(QStringLiteral("name")).toString();
      if (!sketch.isEmpty() && !name.isEmpty()) {
        const SelectionItem item{SelectKind::Profile, sketch, name, QString(), QString()};
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
    previewNoticePlaces(QStringLiteral("status line"), hovered ? m_statusFeature : QString());
  });
}

} // namespace mitcad
