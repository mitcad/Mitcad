// SPDX-License-Identifier: MIT
// Forms (mitcad#1757): the cage read from the model's answer, the picks,
// the edits' commands and the lines drawn (FormCage.hpp).
#include "FormCage.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <limits>
#include <map>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QObject>
#include <QStringList>

namespace mitcad::forms {
namespace {

Point pointOf(const QJsonValue& value) {
  const QJsonArray xyz = value.toArray();
  return {xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble()};
}

qint64 idOf(const QJsonValue& value) { return static_cast<qint64>(std::llround(value.toDouble())); }

QJsonArray idsJson(const QVector<qint64>& ids) {
  QJsonArray array;
  for (const qint64 id : ids) {
    array.append(static_cast<double>(id));
  }
  return array;
}

QJsonArray edgesJson(const QVector<EdgeIds>& edges) {
  QJsonArray array;
  for (const EdgeIds& edge : edges) {
    array.append(QJsonArray{static_cast<double>(edge[0]), static_cast<double>(edge[1])});
  }
  return array;
}

QJsonArray pointJson(const Point& p) { return QJsonArray{p[0], p[1], p[2]}; }

// The picks of one kind as a command's selection fields.
void addSelection(QJsonObject& command, const Picked& picked) {
  if (!picked.vertices.isEmpty()) {
    command.insert(QStringLiteral("vertices"), idsJson(picked.vertices));
  }
  if (!picked.edges.isEmpty()) {
    command.insert(QStringLiteral("edges"), edgesJson(picked.edges));
  }
  if (!picked.faces.isEmpty()) {
    command.insert(QStringLiteral("faces"), idsJson(picked.faces));
  }
}

void addVertex(QVector<qint64>& out, qint64 id) {
  if (!out.contains(id)) {
    out.append(id);
  }
}

} // namespace

const Vertex* Cage::vertex(qint64 id) const {
  for (const Vertex& v : vertices) {
    if (v.id == id) {
      return &v;
    }
  }
  return nullptr;
}

const Face* Cage::face(qint64 id) const {
  for (const Face& f : faces) {
    if (f.id == id) {
      return &f;
    }
  }
  return nullptr;
}

const Edge* Cage::edge(const EdgeIds& ids) const {
  for (const Edge& e : edges) {
    if (sameEdge(e.vertices, ids)) {
      return &e;
    }
  }
  return nullptr;
}

Cage cageOf(const QJsonObject& answer) {
  Cage cage;
  cage.uid = answer.value(QStringLiteral("uid")).toString();
  cage.name = answer.value(QStringLiteral("name")).toString();
  for (const QJsonValue& value : answer.value(QStringLiteral("vertices")).toArray()) {
    const QJsonObject v = value.toObject();
    Vertex vertex;
    vertex.id = idOf(v.value(QStringLiteral("id")));
    vertex.at = pointOf(v.value(QStringLiteral("at")));
    vertex.mirror = v.value(QStringLiteral("mirror")).toBool();
    vertex.corner = v.value(QStringLiteral("corner")).toBool();
    cage.vertices.push_back(vertex);
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("edges")).toArray()) {
    const QJsonObject e = value.toObject();
    const QJsonArray ends = e.value(QStringLiteral("vertices")).toArray();
    Edge edge;
    edge.vertices = {idOf(ends.at(0)), idOf(ends.at(1))};
    edge.boundary = e.value(QStringLiteral("boundary")).toBool();
    edge.creased = e.value(QStringLiteral("creased")).toBool();
    if (edge.creased && e.value(QStringLiteral("sharpness")).isDouble()) {
      edge.sharpness = e.value(QStringLiteral("sharpness")).toDouble();
    }
    cage.edges.push_back(edge);
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("faces")).toArray()) {
    const QJsonObject f = value.toObject();
    Face face;
    face.id = idOf(f.value(QStringLiteral("id")));
    for (const QJsonValue& id : f.value(QStringLiteral("vertices")).toArray()) {
      face.vertices.append(idOf(id));
    }
    face.center = pointOf(f.value(QStringLiteral("center")));
    face.normal = pointOf(f.value(QStringLiteral("normal")));
    face.mirror = f.value(QStringLiteral("mirror")).toBool();
    cage.faces.push_back(face);
  }
  cage.closed = answer.value(QStringLiteral("closed")).toBool();
  const QJsonObject symmetry = answer.value(QStringLiteral("symmetry")).toObject();
  if (!symmetry.isEmpty()) {
    cage.symmetric = true;
    cage.symmetryOrigin = pointOf(symmetry.value(QStringLiteral("origin")));
    cage.symmetryNormal = pointOf(symmetry.value(QStringLiteral("normal")));
  }
  return cage;
}

QString kindKey(Kind kind) {
  switch (kind) {
  case Kind::Faces:
    return QStringLiteral("faces");
  case Kind::Edges:
    return QStringLiteral("edges");
  case Kind::Vertices:
    return QStringLiteral("vertices");
  }
  return QStringLiteral("faces");
}

Kind kindOf(const QString& key) {
  if (key == QStringLiteral("edges")) {
    return Kind::Edges;
  }
  if (key == QStringLiteral("vertices")) {
    return Kind::Vertices;
  }
  return Kind::Faces;
}

bool sameEdge(const EdgeIds& a, const EdgeIds& b) {
  return (a[0] == b[0] && a[1] == b[1]) || (a[0] == b[1] && a[1] == b[0]);
}

bool Picked::contains(Kind kind, qint64 id, const EdgeIds& edge) const {
  switch (kind) {
  case Kind::Faces:
    return faces.contains(id);
  case Kind::Vertices:
    return vertices.contains(id);
  case Kind::Edges:
    return std::any_of(edges.begin(), edges.end(), [&edge](const EdgeIds& e) { return sameEdge(e, edge); });
  }
  return false;
}

bool Picked::operator==(const Picked& other) const {
  if (faces != other.faces || vertices != other.vertices || edges.size() != other.edges.size()) {
    return false;
  }
  for (int i = 0; i < edges.size(); ++i) {
    if (!sameEdge(edges.at(i), other.edges.at(i))) {
      return false;
    }
  }
  return true;
}

QString pickedText(const Picked& picked) {
  const QJsonObject object{{QStringLiteral("faces"), idsJson(picked.faces)},
                           {QStringLiteral("edges"), edgesJson(picked.edges)},
                           {QStringLiteral("vertices"), idsJson(picked.vertices)}};
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

Picked pickedOf(const QString& text) {
  Picked picked;
  const QJsonDocument document = QJsonDocument::fromJson(text.toUtf8());
  if (!document.isObject()) {
    return picked;
  }
  const QJsonObject object = document.object();
  for (const QJsonValue& id : object.value(QStringLiteral("faces")).toArray()) {
    picked.faces.append(idOf(id));
  }
  for (const QJsonValue& edge : object.value(QStringLiteral("edges")).toArray()) {
    const QJsonArray ends = edge.toArray();
    if (ends.size() == 2) {
      picked.edges.append({idOf(ends.at(0)), idOf(ends.at(1))});
    }
  }
  for (const QJsonValue& id : object.value(QStringLiteral("vertices")).toArray()) {
    picked.vertices.append(idOf(id));
  }
  return picked;
}

Picked clicked(const Picked& now, Kind kind, qint64 id, const EdgeIds& edge, bool add) {
  Picked next;
  if (add) {
    // Only picks of the clicked kind stay.
    switch (kind) {
    case Kind::Faces:
      next.faces = now.faces;
      break;
    case Kind::Edges:
      next.edges = now.edges;
      break;
    case Kind::Vertices:
      next.vertices = now.vertices;
      break;
    }
    if (next.contains(kind, id, edge)) {
      switch (kind) {
      case Kind::Faces:
        next.faces.removeAll(id);
        break;
      case Kind::Vertices:
        next.vertices.removeAll(id);
        break;
      case Kind::Edges:
        next.edges.erase(std::remove_if(next.edges.begin(), next.edges.end(),
                                        [&edge](const EdgeIds& e) { return sameEdge(e, edge); }),
                         next.edges.end());
        break;
      }
      return next;
    }
  }
  switch (kind) {
  case Kind::Faces:
    next.faces.append(id);
    break;
  case Kind::Edges:
    next.edges.append(edge);
    break;
  case Kind::Vertices:
    next.vertices.append(id);
    break;
  }
  return next;
}

Picked keptIn(const Picked& picked, const Cage& cage) {
  Picked kept;
  for (const qint64 id : picked.faces) {
    if (cage.face(id) != nullptr) {
      kept.faces.append(id);
    }
  }
  for (const EdgeIds& edge : picked.edges) {
    if (cage.edge(edge) != nullptr) {
      kept.edges.append(edge);
    }
  }
  for (const qint64 id : picked.vertices) {
    if (cage.vertex(id) != nullptr) {
      kept.vertices.append(id);
    }
  }
  return kept;
}

QVector<qint64> movedVertices(const Cage& cage, const Picked& picked) {
  QVector<qint64> moved;
  for (const qint64 id : picked.faces) {
    if (const Face* face = cage.face(id)) {
      for (const qint64 v : face->vertices) {
        addVertex(moved, v);
      }
    }
  }
  for (const EdgeIds& edge : picked.edges) {
    addVertex(moved, edge[0]);
    addVertex(moved, edge[1]);
  }
  for (const qint64 id : picked.vertices) {
    addVertex(moved, id);
  }
  return moved;
}

std::optional<Point> centerOf(const Cage& cage, const Picked& picked) {
  Point sum{};
  int count = 0;
  for (const qint64 id : movedVertices(cage, picked)) {
    if (const Vertex* v = cage.vertex(id)) {
      for (std::size_t k = 0; k < 3; ++k) {
        sum[k] += v->at[k];
      }
      ++count;
    }
  }
  if (count == 0) {
    return std::nullopt;
  }
  return Point{sum[0] / count, sum[1] / count, sum[2] / count};
}

std::optional<Point> normalOf(const Cage& cage, const Picked& picked) {
  Point sum{};
  for (const qint64 id : picked.faces) {
    if (const Face* face = cage.face(id)) {
      for (std::size_t k = 0; k < 3; ++k) {
        sum[k] += face->normal[k];
      }
    }
  }
  const double length = std::sqrt(sum[0] * sum[0] + sum[1] * sum[1] + sum[2] * sum[2]);
  if (!(length > 1e-9)) {
    return std::nullopt;
  }
  return Point{sum[0] / length, sum[1] / length, sum[2] / length};
}

double sizeOf(const Cage& cage, const Picked& picked) {
  QVector<qint64> ids = movedVertices(cage, picked);
  if (ids.isEmpty()) {
    for (const Vertex& v : cage.vertices) {
      ids.append(v.id);
    }
  }
  Point low{std::numeric_limits<double>::infinity(), std::numeric_limits<double>::infinity(),
            std::numeric_limits<double>::infinity()};
  Point high{-low[0], -low[1], -low[2]};
  for (const qint64 id : ids) {
    if (const Vertex* v = cage.vertex(id)) {
      for (std::size_t k = 0; k < 3; ++k) {
        low[k] = std::min(low[k], v->at[k]);
        high[k] = std::max(high[k], v->at[k]);
      }
    }
  }
  if (!(high[0] >= low[0])) {
    return 1.0;
  }
  const double dx = high[0] - low[0];
  const double dy = high[1] - low[1];
  const double dz = high[2] - low[2];
  return std::max(1.0, std::sqrt(dx * dx + dy * dy + dz * dz) / 2.0);
}

QString pickedWords(const Picked& picked) {
  QStringList words;
  if (!picked.faces.isEmpty()) {
    words << QObject::tr("%n face(s)", nullptr, static_cast<int>(picked.faces.size()));
  }
  if (!picked.edges.isEmpty()) {
    words << QObject::tr("%n edge(s)", nullptr, static_cast<int>(picked.edges.size()));
  }
  if (!picked.vertices.isEmpty()) {
    words << QObject::tr("%n vertex(es)", nullptr, static_cast<int>(picked.vertices.size()));
  }
  return words.join(QStringLiteral(", "));
}

QString actionKey(Action action) {
  switch (action) {
  case Action::Move:
    return QStringLiteral("move");
  case Action::Rotate:
    return QStringLiteral("rotate");
  case Action::Scale:
    return QStringLiteral("scale");
  case Action::Extrude:
    return QStringLiteral("extrude");
  case Action::InsertLoop:
    return QStringLiteral("insert_loop");
  case Action::DeleteLoop:
    return QStringLiteral("delete_loop");
  case Action::Crease:
    return QStringLiteral("crease");
  case Action::Uncrease:
    return QStringLiteral("uncrease");
  case Action::DeleteFaces:
    return QStringLiteral("delete_faces");
  case Action::Symmetry:
    return QStringLiteral("symmetry");
  }
  return QStringLiteral("move");
}

std::optional<Action> actionOf(const QString& key) {
  for (const Action action : {Action::Move, Action::Rotate, Action::Scale, Action::Extrude, Action::InsertLoop,
                              Action::DeleteLoop, Action::Crease, Action::Uncrease, Action::DeleteFaces,
                              Action::Symmetry}) {
    if (actionKey(action) == key) {
      return action;
    }
  }
  return std::nullopt;
}

std::optional<Kind> onlyKind(Action action) {
  switch (action) {
  case Action::Extrude:
  case Action::DeleteFaces:
    return Kind::Faces;
  case Action::InsertLoop:
  case Action::DeleteLoop:
    return Kind::Edges;
  default:
    return std::nullopt;
  }
}

bool takesKind(Action action, Kind kind) {
  if (const std::optional<Kind> only = onlyKind(action)) {
    return *only == kind;
  }
  switch (action) {
  case Action::Move:
  case Action::Rotate:
  case Action::Scale:
    return true;
  case Action::Crease:
  case Action::Uncrease:
    return kind == Kind::Edges || kind == Kind::Vertices;
  default:
    return false; // Symmetry takes no picks
  }
}

QString stepName(Action action) {
  switch (action) {
  case Action::Move:
    return QStringLiteral("Move");
  case Action::Rotate:
    return QStringLiteral("Rotate");
  case Action::Scale:
    return QStringLiteral("Scale");
  case Action::Extrude:
    return QStringLiteral("Extrude");
  case Action::InsertLoop:
    return QStringLiteral("Insert Edge Loop");
  case Action::DeleteLoop:
    return QStringLiteral("Delete Edge Loop");
  case Action::Crease:
  case Action::Uncrease:
    return QStringLiteral("Crease");
  case Action::DeleteFaces:
    return QStringLiteral("Delete Faces");
  case Action::Symmetry:
    return QStringLiteral("Symmetry");
  }
  return QString();
}

QJsonObject editCommand(const QString& form, const Edit& edit, const Picked& picked, bool preview) {
  QJsonObject command{{QStringLiteral("form"), form}};
  if (preview) {
    command.insert(QStringLiteral("preview"), true);
  }
  const bool none = picked.isEmpty();
  switch (edit.action) {
  case Action::Move: {
    if (none || (edit.move[0] == 0.0 && edit.move[1] == 0.0 && edit.move[2] == 0.0) ||
        !std::isfinite(edit.move[0] + edit.move[1] + edit.move[2])) {
      return {};
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.move"));
    addSelection(command, picked);
    command.insert(QStringLiteral("by"), pointJson(edit.move));
    return command;
  }
  case Action::Rotate: {
    if (none || edit.angle == 0.0 || !std::isfinite(edit.angle)) {
      return {};
    }
    Point axis{};
    axis[static_cast<std::size_t>(std::clamp(edit.axis, 0, 2))] = 1.0;
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.rotate"));
    addSelection(command, picked);
    command.insert(QStringLiteral("axis"), pointJson(axis));
    command.insert(QStringLiteral("angle"), edit.angle);
    return command;
  }
  case Action::Scale: {
    if (none || edit.factor == 1.0 || !(edit.factor > 0.0) || !std::isfinite(edit.factor)) {
      return {};
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.scale"));
    addSelection(command, picked);
    command.insert(QStringLiteral("factor"), edit.factor);
    return command;
  }
  case Action::Extrude: {
    if (picked.faces.isEmpty() || edit.distance == 0.0 || !std::isfinite(edit.distance)) {
      return {};
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.extrude"));
    command.insert(QStringLiteral("faces"), idsJson(picked.faces));
    command.insert(QStringLiteral("distance"), edit.distance);
    return command;
  }
  case Action::InsertLoop:
  case Action::DeleteLoop: {
    if (picked.edges.size() != 1) {
      return {};
    }
    const EdgeIds& edge = picked.edges.first();
    command.insert(QStringLiteral("cmd"), edit.action == Action::InsertLoop ? QStringLiteral("form.insert_loop")
                                                                            : QStringLiteral("form.delete_loop"));
    command.insert(QStringLiteral("edge"),
                   QJsonArray{static_cast<double>(edge[0]), static_cast<double>(edge[1])});
    if (edit.action == Action::InsertLoop) {
      if (!(edit.at > 0.0 && edit.at < 1.0)) {
        return {};
      }
      command.insert(QStringLiteral("at"), edit.at);
    }
    return command;
  }
  case Action::Crease:
  case Action::Uncrease: {
    if (picked.edges.isEmpty() && picked.vertices.isEmpty()) {
      return {};
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.crease"));
    if (!picked.edges.isEmpty()) {
      command.insert(QStringLiteral("edges"), edgesJson(picked.edges));
    }
    if (!picked.vertices.isEmpty()) {
      command.insert(QStringLiteral("vertices"), idsJson(picked.vertices));
    }
    if (edit.action == Action::Uncrease) {
      command.insert(QStringLiteral("sharpness"), 0.0);
    } else if (edit.sharpness) {
      // Semi-sharp (mitcad#1810): sharp for that many subdivision steps.
      if (!std::isfinite(*edit.sharpness) || !(*edit.sharpness > 0.0)) {
        return {};
      }
      command.insert(QStringLiteral("sharpness"), *edit.sharpness);
    }
    return command;
  }
  case Action::DeleteFaces: {
    if (picked.faces.isEmpty()) {
      return {};
    }
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.delete_faces"));
    command.insert(QStringLiteral("faces"), idsJson(picked.faces));
    return command;
  }
  case Action::Symmetry: {
    command.insert(QStringLiteral("cmd"), QStringLiteral("form.symmetry"));
    const QString plane = edit.plane;
    if (plane == QStringLiteral("x") || plane == QStringLiteral("y") || plane == QStringLiteral("z")) {
      command.insert(QStringLiteral("plane"), plane);
    } else if (plane == QStringLiteral("picked")) {
      // A picked plane or planar face (mitcad#1810): the half on the side
      // its normal points to is kept.
      const Point& n = edit.planeNormal;
      const double length = std::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
      if (!edit.planePicked || !(length > 1e-9) ||
          !std::isfinite(edit.planeOrigin[0] + edit.planeOrigin[1] + edit.planeOrigin[2] + length)) {
        return {};
      }
      command.insert(QStringLiteral("plane"),
                     QJsonObject{{QStringLiteral("origin"), pointJson(edit.planeOrigin)},
                                 {QStringLiteral("normal"), pointJson(Point{n[0] / length, n[1] / length, n[2] / length})}});
    } else {
      command.insert(QStringLiteral("plane"), QJsonValue::Null);
    }
    return command;
  }
  }
  return {};
}

QString editRefusal(const Edit& edit, const Picked& picked) {
  switch (edit.action) {
  case Action::Move:
  case Action::Rotate:
  case Action::Scale:
    return picked.isEmpty() ? QObject::tr("Pick faces, edges or vertices of the cage to move them.") : QString();
  case Action::Extrude:
    return picked.faces.isEmpty() ? QObject::tr("Pick faces of the cage to extrude.") : QString();
  case Action::InsertLoop:
    return picked.edges.size() != 1 ? QObject::tr("Pick one edge of the cage: the new loop runs across it.")
                                    : QString();
  case Action::DeleteLoop:
    return picked.edges.size() != 1 ? QObject::tr("Pick one edge of the loop to delete.") : QString();
  case Action::Crease:
  case Action::Uncrease:
    if (picked.edges.isEmpty() && picked.vertices.isEmpty()) {
      return QObject::tr("Pick edges or vertices of the cage.");
    }
    if (edit.action == Action::Crease && edit.sharpness &&
        !(std::isfinite(*edit.sharpness) && *edit.sharpness > 0.0)) {
      return QObject::tr("Sharpness must be more than zero; Uncrease makes edges smooth again.");
    }
    return QString();
  case Action::DeleteFaces:
    return picked.faces.isEmpty() ? QObject::tr("Pick faces of the cage to delete.") : QString();
  case Action::Symmetry:
    return edit.plane == QStringLiteral("picked") && !edit.planePicked
               ? QObject::tr("Pick a plane or a planar face for the mirror plane.")
               : QString();
  }
  return QString();
}

QString dragText(const Drag& drag) {
  const QJsonObject object{{QStringLiteral("vertex"), static_cast<double>(drag.vertex)},
                           {QStringLiteral("at"), pointJson(drag.at)}};
  return QString::fromUtf8(QJsonDocument(object).toJson(QJsonDocument::Compact));
}

std::optional<Drag> dragOf(const QString& text) {
  const QJsonDocument document = QJsonDocument::fromJson(text.toUtf8());
  if (!document.isObject()) {
    return std::nullopt;
  }
  const QJsonObject object = document.object();
  const QJsonArray at = object.value(QStringLiteral("at")).toArray();
  if (!object.value(QStringLiteral("vertex")).isDouble() || at.size() != 3) {
    return std::nullopt;
  }
  Drag drag;
  drag.vertex = idOf(object.value(QStringLiteral("vertex")));
  drag.at = pointOf(at);
  if (drag.vertex == 0 || !std::isfinite(drag.at[0] + drag.at[1] + drag.at[2])) {
    return std::nullopt;
  }
  return drag;
}

QJsonObject dragCommand(const QString& form, const Drag& drag, bool preview) {
  QJsonObject command{
      {QStringLiteral("cmd"), QStringLiteral("form.set_positions")},
      {QStringLiteral("form"), form},
      {QStringLiteral("positions"),
       QJsonArray{QJsonObject{{QStringLiteral("vertex"), static_cast<double>(drag.vertex)},
                              {QStringLiteral("at"), pointJson(drag.at)}}}}};
  if (preview) {
    command.insert(QStringLiteral("preview"), true);
  }
  return command;
}

Lines cageLines(const Cage& cage) {
  Lines lines;
  for (const Edge& edge : cage.edges) {
    const Vertex* a = cage.vertex(edge.vertices[0]);
    const Vertex* b = cage.vertex(edge.vertices[1]);
    if (a != nullptr && b != nullptr) {
      lines.push_back({a->at, b->at});
    }
  }
  return lines;
}

Lines limitOutline(const QJsonObject& limit) {
  const QJsonArray positions = limit.value(QStringLiteral("positions")).toArray();
  const QJsonArray triangles = limit.value(QStringLiteral("triangles")).toArray();
  const QJsonArray faces = limit.value(QStringLiteral("faces")).toArray();
  const int nodes = static_cast<int>(positions.size() / 3);
  const auto node = [&positions](int i) {
    return Point{positions.at(3 * i).toDouble(), positions.at(3 * i + 1).toDouble(),
                 positions.at(3 * i + 2).toDouble()};
  };
  // Each edge of the mesh with the cage faces of the triangles along it.
  std::map<std::pair<int, int>, std::vector<qint64>> along;
  const int count = static_cast<int>(triangles.size() / 3);
  for (int t = 0; t < count; ++t) {
    const qint64 face = t < faces.size() ? idOf(faces.at(t)) : 0;
    for (int k = 0; k < 3; ++k) {
      const int a = triangles.at(3 * t + k).toInt();
      const int b = triangles.at(3 * t + (k + 1) % 3).toInt();
      if (a < 0 || b < 0 || a >= nodes || b >= nodes) {
        continue;
      }
      along[{std::min(a, b), std::max(a, b)}].push_back(face);
    }
  }
  Lines lines;
  for (const auto& [ends, of] : along) {
    if (of.size() == 1 || (of.size() == 2 && of[0] != of[1])) {
      lines.push_back({node(ends.first), node(ends.second)});
    }
  }
  return lines;
}

} // namespace mitcad::forms
