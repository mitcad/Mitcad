// SPDX-License-Identifier: MIT
// What a 3D sketch's drawing tools snap to (mitcad#1525): the sketch's own
// points (a shared point; an arc's center), the origin, the middles of its
// lines and arcs and the places on its curves; under the pointer, as the
// view picks them (visible, with its depth and ambiguity rules), body
// vertices, edges (their ends, middles and places on them), points and
// curves of other sketches and Intersection Curves, also of other
// components through their occurrence; the axes through the anchor on the
// active plane as guidance; and, lowest, the face in front under the
// pointer (On Face, mitcad#1549). Sketch3dDraw.hpp's chooseSnap() picks the
// winner; the sources are written as commands.md's "Linked references".

#include "Sketch3dController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <optional>

#include <QLineF>
#include <QSet>

#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_CurveType.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/datum.hpp"

#include "../OcctViewer.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr int kSamples = 32; // points per curved edge of a polyline

Xyz xyzOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return a.size() >= 3 ? Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()} : Xyz{};
}

// The place of a polyline nearest to the pointer on the screen, and how
// far: along each segment's picture (close enough to the true place in a
// perspective view for a preview; the relation the snap stores puts the
// point on the curve exactly).
struct Nearest {
  Xyz at{};
  double pixels = 1e300;
};

template <typename Project>
Nearest nearestOn(const std::vector<Xyz>& points, const QPointF& pointer, Project project) {
  Nearest best;
  if (points.empty()) {
    return best;
  }
  QPointF previous = project(points[0]);
  if (points.size() == 1) {
    best.at = points[0];
    best.pixels = QLineF(previous, pointer).length();
    return best;
  }
  for (std::size_t i = 1; i < points.size(); ++i) {
    const QPointF next = project(points[i]);
    const QPointF along = next - previous;
    const double squared = QPointF::dotProduct(along, along);
    double f = squared > 1e-12 ? QPointF::dotProduct(pointer - previous, along) / squared : 0.0;
    f = std::clamp(f, 0.0, 1.0);
    const double pixels = QLineF(previous + along * f, pointer).length();
    if (pixels < best.pixels) {
      best.pixels = pixels;
      best.at = add(points[i - 1], scaled(sub(points[i], points[i - 1]), f));
    }
    previous = next;
  }
  return best;
}

// The point halfway along a polyline by length.
Xyz middleOf(const std::vector<Xyz>& points) {
  if (points.empty()) {
    return Xyz{};
  }
  double total = 0.0;
  for (std::size_t i = 1; i < points.size(); ++i) {
    total += distance(points[i - 1], points[i]);
  }
  double left = total / 2.0;
  for (std::size_t i = 1; i < points.size(); ++i) {
    const double piece = distance(points[i - 1], points[i]);
    if (piece >= left && piece > 0.0) {
      return add(points[i - 1], scaled(sub(points[i], points[i - 1]), left / piece));
    }
    left -= piece;
  }
  return points.back();
}

QString directionText(const Xyz& at) {
  const auto n = [](double v) { return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 3); };
  return QStringLiteral("(%1, %2, %3)").arg(n(at[0]), n(at[1]), n(at[2]));
}

QString itemKey(const SelectionItem& item) {
  return QStringLiteral("%1|%2|%3|%4").arg(static_cast<unsigned>(item.kind)).arg(item.owner, item.name, item.occurrence);
}

// Where the pointer's ray first meets a face (mitcad#1549), in the design;
// none when it misses it.
std::optional<gp_Pnt> rayHit(const TopoDS_Shape& face, const gp_Lin& ray) {
  try {
    IntCurvesFace_ShapeIntersector hits;
    hits.Load(face, Precision::Confusion());
    hits.PerformNearest(ray, 0.0, Precision::Infinite());
    if (hits.IsDone() && hits.NbPnt() > 0) {
      return hits.Pnt(1);
    }
  } catch (const Standard_Failure&) {
    return std::nullopt; // no snap to it then
  }
  return std::nullopt;
}

} // namespace

const std::vector<gp_Pnt>& Sketch3dController::polylineOf(const SelectionItem& item) {
  const QString key = itemKey(item);
  if (const auto known = m_polylines.constFind(key); known != m_polylines.cend()) {
    return known.value();
  }
  std::vector<gp_Pnt> points;
  try {
    const TopoDS_Shape shape = m_host.itemShape(item);
    if (!shape.IsNull()) {
      for (TopExp_Explorer edges(shape, TopAbs_EDGE); edges.More(); edges.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
        if (BRep_Tool::Degenerated(edge)) {
          continue;
        }
        const BRepAdaptor_Curve curve(edge);
        const int count = curve.GetType() == GeomAbs_Line ? 1 : kSamples;
        std::vector<gp_Pnt> piece;
        for (int i = 0; i <= count; ++i) {
          const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / count;
          piece.push_back(curve.Value(t));
        }
        if (edge.Orientation() == TopAbs_REVERSED) {
          std::reverse(piece.begin(), piece.end());
        }
        if (!points.empty() && !piece.empty() && points.back().Distance(piece.front()) < 1e-9) {
          piece.erase(piece.begin());
        }
        points.insert(points.end(), piece.begin(), piece.end());
      }
      if (points.empty()) {
        for (TopExp_Explorer vertices(shape, TopAbs_VERTEX); vertices.More(); vertices.Next()) {
          points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices.Current())));
          break;
        }
      }
    }
  } catch (const Standard_Failure&) {
    points.clear(); // no snap to it then
  }
  return m_polylines.insert(key, points).value();
}

QJsonObject Sketch3dController::withOccurrence(QJsonObject source, const SelectionItem& item) const {
  // Geometry of another component is linked through where it was picked
  // and where the sketch's own component is seen (commands.md "Geometry of
  // other components").
  const QString own = m_host.sketchOccurrence(m_uid);
  if (item.occurrence != own) {
    source.insert(QStringLiteral("occurrence"), item.occurrence);
    if (!own.isEmpty()) {
      source.insert(QStringLiteral("context"), own);
    }
  }
  return source;
}

QString Sketch3dController::sourceOwnerText(const SelectionItem& item) const {
  QString owner = item.owner;
  if (item.kind == SelectKind::SketchPoint || item.kind == SelectKind::SketchCurve ||
      item.kind == SelectKind::Feature) {
    const QString name = m_hooks.featureName ? m_hooks.featureName(item.owner) : QString();
    if (!name.isEmpty() && name != item.owner) {
      owner = QStringLiteral("%1 (%2)").arg(name, item.owner);
    }
  }
  if (item.occurrence != m_host.sketchOccurrence(m_uid)) {
    const QString component = m_hooks.componentName ? m_hooks.componentName(item.occurrence) : item.occurrence;
    owner += QStringLiteral(" in %1").arg(component.isEmpty() ? item.occurrence : component);
  }
  return owner;
}

std::vector<SnapTarget> Sketch3dController::snapCandidates(const QPointF& pointer, const Xyz& from,
                                                           const std::optional<Xyz>& placed) {
  std::vector<SnapTarget> out;
  if (!m_model.hasFrame) {
    return out;
  }
  const auto pixels = [this, &pointer](const Xyz& at) { return QLineF(onScreen(at), pointer).length(); };
  // The centers of the sketch's arcs and circles.
  QSet<QString> centers;
  for (const Sketch3dCurve& curve : m_model.curves) {
    if (curve.type == QStringLiteral("arc") || curve.type == QStringLiteral("circle")) {
      centers.insert(curve.json.value(QStringLiteral("center")).toString());
    }
  }
  // The sketch's points: shared when a click takes one.
  for (const Sketch3dPoint& point : m_model.points) {
    SnapTarget target;
    target.kind = centers.contains(point.id) ? SnapKind::Center : SnapKind::Point;
    target.at = point.at;
    target.point = point.id;
    target.own = true;
    target.what = point.id;
    target.words = entityName(m_model, point.id); // "Point 3" (mitcad#1653)
    target.pixels = pixels(point.at);
    out.push_back(target);
  }
  // The origin: an origin attachment.
  {
    SnapTarget origin;
    origin.kind = SnapKind::Origin;
    origin.source = QStringLiteral("origin");
    origin.own = true;
    origin.pixels = pixels(origin.at);
    out.push_back(origin);
  }
  // The middles of its lines and arcs, and places on its curves.
  const QString ownOccurrence = m_host.sketchOccurrence(m_uid);
  for (const Sketch3dCurve& curve : m_model.curves) {
    const QJsonObject& json = curve.json;
    const auto pointAt = [this](const QJsonValue& id) -> std::optional<Xyz> {
      const Sketch3dPoint* p = m_model.point(id.toString());
      return p != nullptr ? std::optional<Xyz>(p->at) : std::nullopt;
    };
    std::optional<Xyz> middle;
    if (curve.type == QStringLiteral("line")) {
      const auto a = pointAt(json.value(QStringLiteral("start")));
      const auto b = pointAt(json.value(QStringLiteral("end")));
      if (a && b) {
        middle = scaled(add(*a, *b), 0.5);
      }
    } else if (curve.type == QStringLiteral("arc")) {
      const auto c = pointAt(json.value(QStringLiteral("center")));
      const auto s = pointAt(json.value(QStringLiteral("start")));
      const QJsonObject geometry = json.value(QStringLiteral("geometry")).toObject();
      if (c && s && geometry.contains(QStringLiteral("normal"))) {
        const auto n = sketch::unit(xyzOf(geometry.value(QStringLiteral("normal"))));
        if (n) {
          middle = add(*c, rotated(sub(*s, *c), *n, geometry.value(QStringLiteral("sweep")).toDouble() / 2.0));
        }
      }
    }
    if (middle) {
      SnapTarget target;
      target.kind = SnapKind::Midpoint;
      target.at = *middle;
      target.curve = curve.id;
      target.own = true;
      target.what = curve.id;
      target.words = entityName(m_model, curve.id);
      target.pixels = pixels(*middle);
      out.push_back(target);
    }
    const std::vector<gp_Pnt>& design =
        polylineOf({SelectKind::SketchCurve, m_uid, curve.id, curve.type, ownOccurrence});
    std::vector<Xyz> local;
    local.reserve(design.size());
    for (const gp_Pnt& p : design) {
      local.push_back(localPoint(p));
    }
    const Nearest closest = nearestOn(local, pointer, [this](const Xyz& at) { return onScreen(at); });
    if (!local.empty()) {
      SnapTarget target;
      target.kind = SnapKind::OnCurve;
      target.at = closest.at;
      target.curve = curve.id;
      target.own = true;
      target.what = curve.id;
      target.words = entityName(m_model, curve.id);
      target.pixels = closest.pixels;
      out.push_back(target);
    }
  }
  // What the view picks under the pointer: visible geometry outside the
  // sketch; of the faces only the first, the one in front (mitcad#1549).
  if (m_viewer.pickPointsReady()) {
    bool face = false;
    for (const SelectionItem& item : m_viewer.itemsAt(pointer)) {
      if (item.kind == SelectKind::Face) {
        if (face) {
          continue;
        }
        face = true;
      }
      addPickedCandidates(item, pointer, out);
    }
  }
  // Guidance: the axes through the anchor on the active plane (rotated
  // with it, mitcad#1661).
  const Placement placing = toolPlacement();
  if (placed && placing.mode == Placement::Mode::Plane) {
    for (const Axis3 axis : planeAxes(placing.plane)) {
      const Xyz along = placementAxis(placing, axis);
      SnapTarget guide;
      guide.kind = SnapKind::Guide;
      guide.at = add(from, scaled(along, dot(sub(*placed, from), along)));
      guide.what = axisLabel(axis);
      guide.pixels = pixels(guide.at);
      out.push_back(guide);
    }
  }
  return out;
}

void Sketch3dController::addPickedCandidates(const SelectionItem& item, const QPointF& pointer,
                                             std::vector<SnapTarget>& out) {
  if ((item.kind == SelectKind::SketchPoint || item.kind == SelectKind::SketchCurve) && item.owner == m_uid) {
    return; // the sketch's own, from the model
  }
  const auto pixels = [this, &pointer](const Xyz& at) { return QLineF(onScreen(at), pointer).length(); };
  const auto project = [this](const Xyz& at) { return onScreen(at); };
  const QString owner = sourceOwnerText(item);
  // `what` with its ids for the log; the badge says the source in words
  // (mitcad#1653).
  const auto offer = [this, &out](SnapKind kind, const Xyz& at, double screen, const QJsonObject& source,
                                  const QString& what) {
    SnapTarget target;
    target.kind = kind;
    target.at = at;
    target.pixels = screen;
    target.source = source;
    target.what = what;
    target.words = snapSourceWords(source, m_hooks.names);
    out.push_back(target);
  };
  if (item.kind == SelectKind::Face) {
    // A face of a body (mitcad#1549): where the pointer's ray meets it, in
    // sight; the point made there lies on it (On Face) and slides on it.
    const auto ray = m_viewer.pointerRay(pointer);
    if (!ray) {
      return;
    }
    TopoDS_Shape shape;
    try {
      shape = m_host.itemShape(item);
    } catch (const Standard_Failure&) {
      return;
    }
    if (shape.IsNull()) {
      return;
    }
    const std::optional<gp_Pnt> hit = rayHit(shape, gp_Lin(ray->first, ray->second));
    if (!hit || !m_viewer.inSight(*hit)) {
      return;
    }
    const Xyz at = localPoint(*hit);
    offer(SnapKind::OnFace, at, pixels(at),
        withOccurrence({{QStringLiteral("body"), item.owner}, {QStringLiteral("face"), item.name}}, item),
        QStringLiteral("face %1 of %2").arg(item.name, owner));
    return;
  }
  const std::vector<gp_Pnt>& design = polylineOf(item);
  if (design.empty()) {
    return;
  }
  std::vector<Xyz> local;
  local.reserve(design.size());
  for (const gp_Pnt& p : design) {
    local.push_back(localPoint(p));
  }
  switch (item.kind) {
  case SelectKind::Vertex: {
    // A body vertex at its own elevation, where it is in sight.
    if (!m_viewer.inSight(design.front())) {
      return;
    }
    offer(SnapKind::LinkedPoint, local.front(), pixels(local.front()),
        withOccurrence({{QStringLiteral("body"), item.owner}, {QStringLiteral("vertex"), item.name}}, item),
        QStringLiteral("vertex %1 of %2").arg(item.name, owner));
    break;
  }
  case SelectKind::SketchPoint:
    offer(SnapKind::LinkedPoint, local.front(), pixels(local.front()),
        withOccurrence({{QStringLiteral("sketch"), item.owner}, {QStringLiteral("point"), item.name}}, item),
        QStringLiteral("point %1 of %2").arg(item.name, owner));
    break;
  case SelectKind::SketchCurve: {
    // Its ends are its points, which snap as points of that sketch.
    const QJsonObject curve{{QStringLiteral("sketch"), item.owner}, {QStringLiteral("curve"), item.name}};
    QJsonObject middle = curve;
    middle.insert(QStringLiteral("at"), QStringLiteral("middle"));
    const Xyz mid = middleOf(local);
    offer(SnapKind::Midpoint, mid, pixels(mid), withOccurrence(middle, item),
          QStringLiteral("curve %1 of %2").arg(item.name, owner));
    const Nearest closest = nearestOn(local, pointer, project);
    offer(SnapKind::OnCurve, closest.at, closest.pixels, withOccurrence(curve, item),
        QStringLiteral("curve %1 of %2").arg(item.name, owner));
    break;
  }
  case SelectKind::Edge: {
    const QJsonObject edge{{QStringLiteral("body"), item.owner}, {QStringLiteral("edge"), item.name}};
    // Its start and end as the model names them (the `edges` query), in
    // the design where the occurrence shows them.
    if (!m_bodyEdges.contains(item.owner)) {
      QJsonArray edges;
      try {
        edges = m_host.query({{QStringLiteral("query"), QStringLiteral("edges")},
                              {QStringLiteral("body"), item.owner},
                              {QStringLiteral("geometry"), true}})
                    .toArray();
      } catch (const std::exception&) {
        edges = QJsonArray();
      }
      m_bodyEdges.insert(item.owner, edges);
    }
    const gp_Trsf placed = m_hooks.placementOf ? m_hooks.placementOf(item.occurrence) : gp_Trsf();
    for (const QJsonValue& value : m_bodyEdges.value(item.owner)) {
      const QJsonObject found = value.toObject();
      if (found.value(QStringLiteral("name")).toString() != item.name) {
        continue;
      }
      for (const char* end : {"start", "end"}) {
        const QJsonArray xyz = found.value(QString::fromLatin1(end)).toArray();
        if (xyz.size() < 3) {
          continue;
        }
        gp_Pnt at(xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble());
        at.Transform(placed);
        if (!m_viewer.inSight(at)) {
          continue;
        }
        QJsonObject source = edge;
        source.insert(QStringLiteral("at"), QString::fromLatin1(end));
        const Xyz local3 = localPoint(at);
        offer(SnapKind::LinkedPoint, local3, pixels(local3), withOccurrence(source, item),
            QStringLiteral("%1 of edge %2 of %3").arg(QString::fromLatin1(end), item.name, owner));
      }
      break;
    }
    const Xyz mid = middleOf(local);
    if (m_viewer.inSight(designPoint(mid))) {
      QJsonObject middle = edge;
      middle.insert(QStringLiteral("at"), QStringLiteral("middle"));
      offer(SnapKind::Midpoint, mid, pixels(mid), withOccurrence(middle, item),
            QStringLiteral("edge %1 of %2").arg(item.name, owner));
    }
    const Nearest closest = nearestOn(local, pointer, project);
    offer(SnapKind::OnEdge, closest.at, closest.pixels, withOccurrence(edge, item),
          QStringLiteral("edge %1 of %2").arg(item.name, owner));
    break;
  }
  case SelectKind::Feature: {
    // An Intersection Curve: its ends, middle and places on it.
    const QJsonObject curve{{QStringLiteral("feature"), item.owner}};
    const auto withAt = [&curve](const char* at) {
      QJsonObject source = curve;
      source.insert(QStringLiteral("at"), QString::fromLatin1(at));
      return source;
    };
    offer(SnapKind::LinkedPoint, local.front(), pixels(local.front()), withOccurrence(withAt("start"), item),
        QStringLiteral("start of %1").arg(owner));
    offer(SnapKind::LinkedPoint, local.back(), pixels(local.back()), withOccurrence(withAt("end"), item),
        QStringLiteral("end of %1").arg(owner));
    const Xyz mid = middleOf(local);
    offer(SnapKind::Midpoint, mid, pixels(mid), withOccurrence(withAt("middle"), item), owner);
    const Nearest closest = nearestOn(local, pointer, project);
    offer(SnapKind::OnCurve, closest.at, closest.pixels, withOccurrence(curve, item), owner);
    break;
  }
  default:
    break;
  }
}

void Sketch3dController::pickNormal() {
  std::optional<Xyz> direction;
  QString what;  // for the log, with the ids
  QString words; // as the palette shows it (mitcad#1653)
  const auto straight = [](const std::vector<Xyz>& points) -> std::optional<Xyz> {
    if (points.size() < 2) {
      return std::nullopt;
    }
    const Xyz along = sub(points.back(), points.front());
    const auto axis = sketch::unit(along);
    if (!axis) {
      return std::nullopt;
    }
    const double tolerance = std::max(1e-6, 1e-6 * norm(along));
    for (const Xyz& p : points) {
      const Xyz off = sub(p, points.front());
      if (norm(sub(off, scaled(*axis, dot(off, *axis)))) > tolerance) {
        return std::nullopt; // not straight
      }
    }
    return axis;
  };
  // A line of this sketch within the snaps' reach on the screen, from the
  // model as the snaps take its points (the view's pick is not needed:
  // its tolerance is narrower).
  double nearest = 10.0;
  for (const Sketch3dCurve& curve : m_model.curves) {
    const Sketch3dPoint* a = m_model.point(curve.json.value(QStringLiteral("start")).toString());
    const Sketch3dPoint* b = m_model.point(curve.json.value(QStringLiteral("end")).toString());
    if (curve.type != QStringLiteral("line") || a == nullptr || b == nullptr) {
      continue;
    }
    const Nearest closest = nearestOn({a->at, b->at}, m_pointer, [this](const Xyz& at) { return onScreen(at); });
    if (closest.pixels <= nearest) {
      if (const auto along = sketch::unit(sub(b->at, a->at))) {
        nearest = closest.pixels;
        direction = along;
        what = QStringLiteral("line %1").arg(curve.id);
        words = entityName(m_model, curve.id);
      }
    }
  }
  QStringList under; // what the view had there, for the log of a miss
  const std::vector<SelectionItem> picked =
      direction ? std::vector<SelectionItem>() : m_viewer.itemsAt(m_pointer);
  for (const SelectionItem& item : picked) {
    under << QStringLiteral("%1 %2 of %3").arg(item.geometry, item.name, item.owner);
    if (item.kind == SelectKind::SketchCurve && item.owner == m_uid) {
      // A line of this sketch: from its start to its end.
      const Sketch3dCurve* curve = m_model.curve(item.name);
      if (curve != nullptr && curve->type == QStringLiteral("line")) {
        const Sketch3dPoint* a = m_model.point(curve->json.value(QStringLiteral("start")).toString());
        const Sketch3dPoint* b = m_model.point(curve->json.value(QStringLiteral("end")).toString());
        if (a != nullptr && b != nullptr) {
          direction = sketch::unit(sub(b->at, a->at));
          what = QStringLiteral("line %1").arg(item.name);
          words = entityName(m_model, item.name);
        }
      }
    } else if (item.kind == SelectKind::Face) {
      // A planar face: its normal out of the material. A flat spline face
      // (a sheet body's wall) is one too, as Look At reads it (mitcad#1554).
      try {
        const TopoDS_Shape shape = m_host.itemShape(item);
        TopExp_Explorer faces(shape, TopAbs_FACE);
        if (faces.More()) {
          const TopoDS_Face& face = TopoDS::Face(faces.Current());
          if (const std::optional<gp_Pln> plane = geometry::face_plane(face)) {
            direction = localDirection(gp_Vec(plane->Axis().Direction()));
            what = QStringLiteral("face %1 of %2").arg(item.name, sourceOwnerText(item));
            words = snapSourceWords(
                withOccurrence({{QStringLiteral("body"), item.owner}, {QStringLiteral("face"), item.name}}, item),
                m_hooks.names);
          }
        }
      } catch (const Standard_Failure&) {
        direction.reset();
      }
    } else if (item.kind == SelectKind::Edge || item.kind == SelectKind::SketchCurve ||
               item.kind == SelectKind::Axis) {
      // A straight edge, another sketch's line, an axis: along it.
      std::vector<Xyz> local;
      for (const gp_Pnt& p : polylineOf(item)) {
        local.push_back(localPoint(p));
      }
      direction = straight(local);
      if (direction) {
        const bool edge = item.kind == SelectKind::Edge;
        what = item.kind == SelectKind::Axis
                   ? QStringLiteral("axis %1").arg(item.owner)
                   : QStringLiteral("%1 %2 of %3")
                         .arg(edge ? QStringLiteral("edge") : QStringLiteral("curve"), item.name,
                              sourceOwnerText(item));
        if (item.kind == SelectKind::Axis) {
          const QString axis = m_hooks.names.feature ? m_hooks.names.feature(item.owner) : QString();
          words = axis.isEmpty() ? tr("an axis") : axis;
        } else {
          words = snapSourceWords(withOccurrence(edge ? QJsonObject{{QStringLiteral("body"), item.owner},
                                                                    {QStringLiteral("edge"), item.name}}
                                                      : QJsonObject{{QStringLiteral("sketch"), item.owner},
                                                                    {QStringLiteral("curve"), item.name}},
                                                 item),
                                  m_hooks.names);
        }
      }
    }
    if (direction) {
      break;
    }
  }
  if (!direction) {
    m_host.showHint(tr("Pick a line, a straight edge, an axis or a planar face for the arc's normal."));
    toolLog(QStringLiteral("3D sketch Center Point Arc: no line, edge, axis or planar face under the pointer at "
                           "%1,%2 (%3; the view picks %4)")
                .arg(qRound(m_pointer.x()))
                .arg(qRound(m_pointer.y()))
                .arg(m_viewer.isCovered(m_pointer) ? QStringLiteral("under a panel") : QStringLiteral("in the view"))
                .arg(under.isEmpty() ? QStringLiteral("nothing") : under.join(QStringLiteral(", "))));
    return;
  }
  m_arcNormal = direction;
  m_arcNormalWhat = what;
  m_arcNormalWords = words;
  m_pickingNormal = false;
  toolLog(QStringLiteral("3D sketch Center Point Arc: normal from %1 %2").arg(what, directionText(*direction)));
  emit toolChanged(); // the pick filter goes back
  updateHover();
  emit toolStateChanged();
}

} // namespace mitcad::sketch
