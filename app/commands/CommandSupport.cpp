// SPDX-License-Identifier: MIT
#include "CommandSupport.hpp"

#include <algorithm>
#include <cmath>
#include <limits>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>

#include "../framework/ModelShapes.hpp"
#include "../framework/Numbers.hpp"

namespace mitcad::cmd {
namespace {

const QString kNewBody = QStringLiteral("new_body");
constexpr double kDegreesPerRadian = 57.29577951308232;

QString surfaceType(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face).GetType()) {
  case GeomAbs_Plane:
    return QStringLiteral("plane");
  case GeomAbs_Cylinder:
    return QStringLiteral("cylinder");
  case GeomAbs_Cone:
    return QStringLiteral("cone");
  case GeomAbs_Sphere:
    return QStringLiteral("sphere");
  case GeomAbs_Torus:
    return QStringLiteral("torus");
  default:
    return QStringLiteral("other");
  }
}

QString curveType(const TopoDS_Edge& edge) {
  switch (BRepAdaptor_Curve(edge).GetType()) {
  case GeomAbs_Line:
    return QStringLiteral("line");
  case GeomAbs_Circle:
    return QStringLiteral("circle");
  case GeomAbs_Ellipse:
    return QStringLiteral("ellipse");
  default:
    return QStringLiteral("other");
  }
}

QString sketchCurveType(const QString& sketch, const QString& curve, const CommandContext& context) {
  try {
    const QJsonObject answer = context.queryObject(
        {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), sketch}});
    for (const QJsonValue& value : answer.value(QStringLiteral("entities")).toArray()) {
      const QJsonObject entity = value.toObject();
      if (entity.value(QStringLiteral("id")).toString() == curve) {
        return entity.value(QStringLiteral("type")).toString();
      }
    }
  } catch (const std::exception&) {
    // An unknown sketch: no finer type.
  }
  return QString();
}

} // namespace

QString str(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

QString label(const Choices& choices, const QString& value) {
  for (const auto& [choice, text] : choices) {
    if (choice == value) {
      return text;
    }
  }
  return value;
}

QString number(double value) { return QString::number(value, 'g', 10); }

QString degrees(double radians) { return number(std::round(radians * kDegreesPerRadian * 1e6) / 1e6); }

// ---------------------------------------------------------------------------
// References and selections

QJsonArray refsOf(const Selection& items) {
  QJsonArray refs;
  for (const SelectionItem& item : items) {
    refs.append(item.reference());
  }
  return refs;
}

Selection profilesOf(const QJsonArray& refs) {
  Selection items;
  for (const QJsonValue& value : refs) {
    const QJsonObject ref = value.toObject();
    items.append({SelectKind::Profile, str(ref, "sketch"), str(ref, "region"), QStringLiteral("plane")});
  }
  return items;
}

QJsonArray bodyUids(const Selection& items) {
  QJsonArray uids;
  for (const SelectionItem& item : items) {
    uids.append(item.owner);
  }
  return uids;
}

Selection bodiesOf(const QJsonArray& uids) {
  Selection items;
  for (const QJsonValue& uid : uids) {
    items.append({SelectKind::Body, uid.toString(), QString(), QString()});
  }
  return items;
}

QJsonArray names(const Selection& items, SelectKind kind) {
  QJsonArray result;
  for (const SelectionItem& item : items) {
    if (item.kind == kind) {
      result.append(item.name);
    }
  }
  return result;
}

QString oneSketch(const Selection& items, Built& error, const QString& input) {
  QString sketch;
  for (const SelectionItem& item : items) {
    if (!sketch.isEmpty() && item.owner != sketch) {
      error = Built::failure(QObject::tr("Select items of one sketch."), input);
      return QString();
    }
    sketch = item.owner;
  }
  return sketch;
}

QString oneBody(const Selection& items, Built& error, const QString& input) {
  QString body;
  for (const SelectionItem& item : items) {
    if (!body.isEmpty() && item.owner != body) {
      error = Built::failure(QObject::tr("Select edges and faces of one body."), input);
      return QString();
    }
    body = item.owner;
  }
  return body;
}

SelectionItem itemOf(const QJsonValue& ref, const CommandContext& context) {
  if (ref.isString()) {
    const QString uid = ref.toString();
    if (uid == QStringLiteral("xy") || uid == QStringLiteral("xz") || uid == QStringLiteral("yz")) {
      return {SelectKind::Plane, uid, QString(), QStringLiteral("plane")};
    }
    if (uid == QStringLiteral("x") || uid == QStringLiteral("y") || uid == QStringLiteral("z")) {
      return {SelectKind::Axis, uid, QString(), QStringLiteral("axis")};
    }
    if (uid == QStringLiteral("origin")) {
      return {SelectKind::Point, uid, QString(), QStringLiteral("point")};
    }
    try {
      const QString type = context
                               .queryObject({{QStringLiteral("query"), QStringLiteral("datum")},
                                             {QStringLiteral("uid"), uid}})
                               .value(QStringLiteral("type"))
                               .toString();
      if (type == QStringLiteral("plane")) {
        return {SelectKind::Plane, uid, QString(), QStringLiteral("plane")};
      }
      if (type == QStringLiteral("axis")) {
        return {SelectKind::Axis, uid, QString(), QStringLiteral("axis")};
      }
      if (type == QStringLiteral("point")) {
        return {SelectKind::Point, uid, QString(), QStringLiteral("point")};
      }
    } catch (const std::exception&) {
      // Not a datum.
    }
    return SelectionItem();
  }
  const QJsonObject o = ref.toObject();
  if (o.contains(QStringLiteral("sketch"))) {
    const QString sketch = str(o, "sketch");
    if (o.contains(QStringLiteral("region"))) {
      return {SelectKind::Profile, sketch, str(o, "region"), QStringLiteral("plane")};
    }
    if (o.contains(QStringLiteral("curve"))) {
      const QString curve = str(o, "curve");
      return {SelectKind::SketchCurve, sketch, curve, sketchCurveType(sketch, curve, context)};
    }
    if (o.contains(QStringLiteral("point"))) {
      return {SelectKind::SketchPoint, sketch, str(o, "point"), QStringLiteral("point")};
    }
    return SelectionItem();
  }
  if (o.contains(QStringLiteral("body"))) {
    const QString body = str(o, "body");
    const auto shape = context.bodyShape(body);
    if (o.contains(QStringLiteral("face"))) {
      const QString face = str(o, "face");
      QString geometry;
      if (shape) {
        const auto found = shape->find_faces(face.toStdString());
        if (!found.empty()) {
          geometry = surfaceType(shape->face(found.front()));
        }
      }
      return {SelectKind::Face, body, face, geometry};
    }
    if (o.contains(QStringLiteral("edge"))) {
      const QString edge = str(o, "edge");
      QString geometry;
      if (shape) {
        const auto found = shape->find_edges(edge.toStdString());
        if (!found.empty()) {
          geometry = curveType(shape->edge(found.front()));
        }
      }
      return {SelectKind::Edge, body, edge, geometry};
    }
    if (o.contains(QStringLiteral("vertex"))) {
      return {SelectKind::Vertex, body, str(o, "vertex"), QStringLiteral("point")};
    }
    if (o.size() == 1) {
      return {SelectKind::Body, body, QString(), QString()};
    }
  }
  return SelectionItem();
}

std::optional<Selection> itemsOf(const QJsonArray& refs, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& ref : refs) {
    const SelectionItem item = itemOf(ref, context);
    if (!item.isValid()) {
      return std::nullopt;
    }
    items.append(item);
  }
  return items;
}

bool isShowable(const QJsonValue& ref, const CommandContext& context) {
  return itemOf(ref, context).isValid();
}

QJsonValue pathOf(const Selection& items, bool chain, Built& error, const QString& input) {
  if (items.isEmpty()) {
    return QJsonValue();
  }
  const SelectKind kind = items.first().kind;
  for (const SelectionItem& item : items) {
    if (item.kind != kind || item.owner != items.first().owner) {
      error = Built::failure(kind == SelectKind::Edge
                                 ? QObject::tr("Select edges of one body.")
                                 : QObject::tr("Select curves of one sketch."),
                             input);
      return QJsonValue();
    }
  }
  if (kind == SelectKind::Edge) {
    return QJsonObject{{QStringLiteral("body"), items.first().owner},
                       {QStringLiteral("edges"), names(items, SelectKind::Edge)}};
  }
  QJsonObject path{{QStringLiteral("sketch"), items.first().owner},
                   {QStringLiteral("curves"), names(items, SelectKind::SketchCurve)}};
  if (chain) {
    path.insert(QStringLiteral("chain"), true);
  }
  return path;
}

std::optional<Selection> pathItems(const QJsonValue& path, const CommandContext& context,
                                   bool* chain) {
  const QJsonObject o = path.toObject();
  Selection items;
  if (chain != nullptr) {
    *chain = o.value(QStringLiteral("chain")).toBool();
  }
  if (o.contains(QStringLiteral("sketch"))) {
    const QString sketch = str(o, "sketch");
    QJsonArray curves = o.value(QStringLiteral("curves")).toArray();
    if (o.contains(QStringLiteral("curve"))) {
      curves = {o.value(QStringLiteral("curve"))};
    }
    if (curves.isEmpty()) {
      return std::nullopt; // all curves of the sketch
    }
    for (const QJsonValue& curve : curves) {
      items.append({SelectKind::SketchCurve, sketch, curve.toString(),
                    sketchCurveType(sketch, curve.toString(), context)});
    }
    return items;
  }
  if (o.contains(QStringLiteral("body")) && o.contains(QStringLiteral("edges"))) {
    for (const QJsonValue& edge : o.value(QStringLiteral("edges")).toArray()) {
      const SelectionItem item = itemOf(
          QJsonObject{{QStringLiteral("body"), str(o, "body")}, {QStringLiteral("edge"), edge}}, context);
      items.append(item);
    }
    return items;
  }
  return std::nullopt;
}

// ---------------------------------------------------------------------------
// Kinds of geometry

bool isLine(const SelectionItem& item) { return item.geometry == QStringLiteral("line"); }

bool isPlanar(const SelectionItem& item) {
  return item.kind == SelectKind::Plane ||
         (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
}

bool isCircular(const SelectionItem& item) {
  return item.geometry == QStringLiteral("circle") || item.geometry == QStringLiteral("arc");
}

bool isAxial(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Axis:
    return true;
  case SelectKind::Edge:
    return isLine(item) || isCircular(item);
  case SelectKind::SketchCurve:
    return isLine(item);
  case SelectKind::Face:
    return item.geometry == QStringLiteral("cylinder") || item.geometry == QStringLiteral("cone") ||
           item.geometry == QStringLiteral("torus");
  default:
    return false;
  }
}

bool isPointLike(const SelectionItem& item) {
  return item.kind == SelectKind::Point || item.kind == SelectKind::Vertex ||
         item.kind == SelectKind::SketchPoint;
}

// ---------------------------------------------------------------------------
// Operations

const Choices kOperations = {{QStringLiteral("new_body"), QStringLiteral("New Body")},
                             {QStringLiteral("join"), QStringLiteral("Join")},
                             {QStringLiteral("cut"), QStringLiteral("Cut")},
                             {QStringLiteral("intersect"), QStringLiteral("Intersect")},
                             {QStringLiteral("new_component"), QStringLiteral("New Component")}};

QVector<InputDef> operationInputs(bool newComponent) {
  Choices choices = kOperations;
  if (!newComponent) {
    choices.removeLast();
  }
  return {choiceInput(QStringLiteral("operation"), QObject::tr("Operation"), choices, kNewBody)
              .withTooltip(QObject::tr("Make a new body or component, or join to, cut from or "
                                       "intersect with bodies")),
          selectionInput(QStringLiteral("objects"), QObject::tr("Objects"), SelectKind::Body, 0, 0)
              .withTooltip(QObject::tr("The bodies to join, cut or intersect; none: every body "
                                       "the feature reaches"))
              .withVisible([](const CommandState& state) {
                const QString operation = state.choice(QStringLiteral("operation"));
                return operation != kNewBody && operation != QStringLiteral("new_component");
              })};
}

void addOperation(QJsonObject& def, const CommandState& state) {
  const QString operation = state.choice(QStringLiteral("operation"));
  def.insert(QStringLiteral("operation"), operation);
  const Selection& objects = state.items(QStringLiteral("objects"));
  if (operation != kNewBody && operation != QStringLiteral("new_component") && !objects.isEmpty()) {
    def.insert(QStringLiteral("participants"), bodyUids(objects));
  }
}

void loadOperation(const QJsonObject& def, CommandState& state) {
  state.setChoice(QStringLiteral("operation"), def.value(QStringLiteral("operation")).toString(kNewBody));
  state.setItems(QStringLiteral("objects"),
                 bodiesOf(def.value(QStringLiteral("participants")).toArray()));
}

// ---------------------------------------------------------------------------
// Objects

QVector<InputDef> objectInputs(const QString& suffix, std::function<bool(const CommandState&)> shown) {
  const QString object = QStringLiteral("object") + suffix;
  const auto kindIs = [object](SelectKind kind) {
    return [object, kind](const CommandState& state) {
      const Selection& items = state.items(object);
      return !items.isEmpty() && items.first().kind == kind;
    };
  };
  const auto faceShown = kindIs(SelectKind::Face);
  const auto bodyShown = kindIs(SelectKind::Body);
  return {
      selectionInput(object, QObject::tr("Object"),
                     SelectKind::Plane | SelectKind::Face | SelectKind::Body, 1, 1)
          .withTooltip(QObject::tr("A plane, a face or a body the extent ends at"))
          .withVisible(shown),
      choiceInput(object + QStringLiteral("_extend"), QObject::tr("Extend"),
                  {{QStringLiteral("selected"), QStringLiteral("To Selected Face")},
                   {QStringLiteral("adjacent"), QStringLiteral("To Adjacent Faces")}},
                  QStringLiteral("selected"))
          .withTooltip(QObject::tr("The face's surface continued, or the face and the faces next to it"))
          .withVisible([shown, faceShown](const CommandState& s) { return shown(s) && faceShown(s); }),
      choiceInput(object + QStringLiteral("_body"), QObject::tr("Body Extent"),
                  {{QStringLiteral("to_body"), QStringLiteral("To Body")},
                   {QStringLiteral("through_body"), QStringLiteral("Through Body")}},
                  QStringLiteral("to_body"))
          .withVisible([shown, bodyShown](const CommandState& s) { return shown(s) && bodyShown(s); }),
  };
}

QJsonObject objectOf(const CommandState& state, const QString& suffix) {
  const QString key = QStringLiteral("object") + suffix;
  const SelectionItem& item = state.items(key).first();
  // The model's defaults (false) are left out, as it saves them.
  switch (item.kind) {
  case SelectKind::Face: {
    QJsonObject object{{QStringLiteral("type"), QStringLiteral("face")},
                       {QStringLiteral("body"), item.owner},
                       {QStringLiteral("face"), item.name}};
    if (state.choice(key + QStringLiteral("_extend")) == QStringLiteral("adjacent")) {
      object.insert(QStringLiteral("chained"), true);
    }
    return object;
  }
  case SelectKind::Body: {
    QJsonObject object{{QStringLiteral("type"), QStringLiteral("body")}, {QStringLiteral("body"), item.owner}};
    if (state.choice(key + QStringLiteral("_body")) == QStringLiteral("through_body")) {
      object.insert(QStringLiteral("through"), true);
    }
    return object;
  }
  default:
    return {{QStringLiteral("type"), QStringLiteral("plane")}, {QStringLiteral("plane"), item.reference()}};
  }
}

bool loadObject(const QJsonObject& object, CommandState& state, const QString& suffix,
                const CommandContext& context) {
  const QString key = QStringLiteral("object") + suffix;
  const QString type = str(object, "type");
  SelectionItem item;
  if (type == QStringLiteral("plane")) {
    item = itemOf(object.value(QStringLiteral("plane")), context);
  } else if (type == QStringLiteral("face")) {
    item = itemOf(QJsonObject{{QStringLiteral("body"), object.value(QStringLiteral("body"))},
                              {QStringLiteral("face"), object.value(QStringLiteral("face"))}},
                  context);
    state.setChoice(key + QStringLiteral("_extend"), object.value(QStringLiteral("chained")).toBool()
                                                         ? QStringLiteral("adjacent")
                                                         : QStringLiteral("selected"));
  } else if (type == QStringLiteral("body")) {
    item = {SelectKind::Body, str(object, "body"), QString(), QString()};
    state.setChoice(key + QStringLiteral("_body"), object.value(QStringLiteral("through")).toBool()
                                                       ? QStringLiteral("through_body")
                                                       : QStringLiteral("to_body"));
  }
  if (!item.isValid()) {
    return false;
  }
  state.setItems(key, {item});
  return true;
}

bool objectShowable(const QJsonObject& object, const CommandContext& context) {
  return str(object, "type") != QStringLiteral("plane") ||
         isShowable(object.value(QStringLiteral("plane")), context);
}

// ---------------------------------------------------------------------------
// Values

void loadValue(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context) {
  if (slot.isNull() || slot.isUndefined()) {
    return;
  }
  state.setText(key, context.valueText(slot, def.value(QStringLiteral("uid")).toString()));
}

bool nonZero(const CommandState& state, const QString& key) {
  const double value = state.value(key);
  return std::isfinite(value) && value != 0.0;
}

QJsonValue countValue(const CommandState& state, const QString& key) {
  if (parsePlainNumber(state.text(key))) {
    return state.value(key);
  }
  return state.expression(key);
}

void loadCount(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context) {
  if (slot.isDouble()) {
    state.setText(key, number(slot.toDouble()));
    return;
  }
  const QString name = slot.toString();
  const QString owner = def.value(QStringLiteral("uid")).toString();
  for (const QJsonValue& value : context.queryArray(QStringLiteral("parameters"))) {
    const QJsonObject parameter = value.toObject();
    if (parameter.value(QStringLiteral("name")).toString() != name) {
      continue;
    }
    // The feature's own count shows as its number (its parameter has the
    // slot's unit, which the count does not have).
    state.setText(key, parameter.value(QStringLiteral("owner")).toString() == owner
                           ? number(parameter.value(QStringLiteral("value")).toDouble())
                           : name);
    return;
  }
  state.setText(key, name);
}

// ---------------------------------------------------------------------------
// The model

void newestProfile(CommandState& state, const CommandContext& context) {
  Selection& profiles = state.items(QStringLiteral("profiles"));
  if (profiles.isEmpty()) {
    const Selection unused = context.unusedProfiles();
    if (!unused.isEmpty()) {
      profiles.append(unused.last());
    }
  }
}

bool hasProfiles(const CommandContext& context) {
  return !context.queryArray(QStringLiteral("profiles")).isEmpty();
}

bool hasBodies(const CommandContext& context) {
  return !context.queryArray(QStringLiteral("bodies")).isEmpty();
}

bool hasSketches(const CommandContext& context) {
  const QJsonArray features = context.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}})
                                  .value(QStringLiteral("features"))
                                  .toArray();
  return std::any_of(features.begin(), features.end(), [](const QJsonValue& feature) {
    return feature.toObject().value(QStringLiteral("type")).toString() == QStringLiteral("sketch");
  });
}

// ---------------------------------------------------------------------------
// Geometry

std::optional<gp_Pnt> centerOf(const Selection& items, const CommandContext& context) {
  Bnd_Box box;
  for (const SelectionItem& item : items) {
    const TopoDS_Shape shape = context.itemShape(item);
    if (!shape.IsNull()) {
      BRepBndLib::Add(shape, box);
    }
  }
  if (box.IsVoid()) {
    return std::nullopt;
  }
  return gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0);
}

std::optional<PlaneFrame> frameOf(const SelectionItem& item, const CommandContext& context) {
  PlaneFrame frame;
  if (item.kind == SelectKind::Plane) {
    try {
      const QJsonObject datum = context.queryObject(
          {{QStringLiteral("query"), QStringLiteral("datum")}, {QStringLiteral("uid"), item.owner}});
      frame.origin = pointOf(datum.value(QStringLiteral("origin")));
      frame.normal = gp_Dir(vectorOf(datum.value(QStringLiteral("normal"))));
      frame.x = gp_Dir(vectorOf(datum.value(QStringLiteral("x_axis"))));
      frame.y = gp_Dir(vectorOf(datum.value(QStringLiteral("y_axis"))));
      return frame;
    } catch (const std::exception&) {
      return std::nullopt;
    }
  }
  if (item.kind == SelectKind::Profile || item.kind == SelectKind::Sketch) {
    try {
      const QJsonObject sketch = context.queryObject(
          {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), item.owner}});
      const QJsonObject axes = sketch.value(QStringLiteral("frame")).toObject();
      if (axes.isEmpty()) {
        return std::nullopt;
      }
      frame.origin = pointOf(axes.value(QStringLiteral("origin")));
      frame.x = gp_Dir(vectorOf(axes.value(QStringLiteral("x_axis"))));
      frame.y = gp_Dir(vectorOf(axes.value(QStringLiteral("y_axis"))));
      frame.normal = gp_Dir(vectorOf(axes.value(QStringLiteral("normal"))));
      return frame;
    } catch (const std::exception&) {
      return std::nullopt;
    }
  }
  if (item.kind != SelectKind::Face) {
    return std::nullopt;
  }
  const TopoDS_Shape shape = context.itemShape(item);
  TopExp_Explorer faces(shape, TopAbs_FACE);
  if (!faces.More()) {
    return std::nullopt;
  }
  const TopoDS_Face face = TopoDS::Face(faces.Current());
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    return std::nullopt;
  }
  const gp_Pln plane = surface.Plane();
  gp_Dir normal = plane.Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  // As the model frames a face: the origin projected onto it, x along +X
  // projected (+Y when it faces X).
  const gp_Pnt on = plane.Location();
  frame.origin = gp_Pnt(gp_Pnt(0, 0, 0).XYZ() -
                        normal.XYZ() * gp_Vec(on, gp_Pnt(0, 0, 0)).Dot(gp_Vec(normal)));
  gp_Vec x = gp_Vec(1, 0, 0) - gp_Vec(normal) * normal.X();
  if (x.Magnitude() < 1e-9) {
    x = gp_Vec(0, 1, 0) - gp_Vec(normal) * normal.Y();
  }
  frame.normal = normal;
  frame.x = gp_Dir(x);
  frame.y = normal.Crossed(frame.x);
  return frame;
}

std::optional<std::pair<gp_Pnt, gp_Dir>> axisOf(const SelectionItem& item,
                                                 const CommandContext& context) {
  if (item.kind == SelectKind::Axis) {
    try {
      const QJsonObject datum = context.queryObject(
          {{QStringLiteral("query"), QStringLiteral("datum")}, {QStringLiteral("uid"), item.owner}});
      return std::make_pair(pointOf(datum.value(QStringLiteral("origin"))),
                            gp_Dir(vectorOf(datum.value(QStringLiteral("direction")))));
    } catch (const std::exception&) {
      return std::nullopt;
    }
  }
  const TopoDS_Shape shape = context.itemShape(item);
  if (shape.IsNull()) {
    return std::nullopt;
  }
  TopExp_Explorer edges(shape, TopAbs_EDGE);
  if (item.kind != SelectKind::Face && edges.More()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
    if (curve.GetType() == GeomAbs_Line) {
      const gp_Pnt a = curve.Value(curve.FirstParameter());
      const gp_Pnt b = curve.Value(curve.LastParameter());
      if (a.Distance(b) > 1e-9) {
        return std::make_pair(a, gp_Dir(gp_Vec(a, b)));
      }
    }
    if (curve.GetType() == GeomAbs_Circle) {
      const gp_Ax1 axis = curve.Circle().Axis();
      return std::make_pair(axis.Location(), axis.Direction());
    }
    return std::nullopt;
  }
  TopExp_Explorer faces(shape, TopAbs_FACE);
  if (!faces.More()) {
    return std::nullopt;
  }
  const BRepAdaptor_Surface surface(TopoDS::Face(faces.Current()));
  switch (surface.GetType()) {
  case GeomAbs_Cylinder:
    return std::make_pair(surface.Cylinder().Axis().Location(), surface.Cylinder().Axis().Direction());
  case GeomAbs_Cone:
    return std::make_pair(surface.Cone().Axis().Location(), surface.Cone().Axis().Direction());
  case GeomAbs_Torus:
    return std::make_pair(surface.Torus().Axis().Location(), surface.Torus().Axis().Direction());
  default:
    return std::nullopt;
  }
}

std::optional<std::pair<gp_Pnt, gp_Dir>> profileNormal(const Selection& profiles,
                                                       const CommandContext& context) {
  if (profiles.isEmpty()) {
    return std::nullopt;
  }
  const auto center = centerOf(profiles, context);
  if (!center) {
    return std::nullopt;
  }
  try {
    const QJsonObject sketch = context.queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                                    {QStringLiteral("uid"), profiles.first().owner}});
    const QJsonObject frame = sketch.value(QStringLiteral("frame")).toObject();
    if (frame.isEmpty()) {
      return std::nullopt;
    }
    return std::make_pair(*center, gp_Dir(vectorOf(frame.value(QStringLiteral("normal")))));
  } catch (const std::exception&) {
    return std::nullopt;
  }
}

Manipulator arrow(const gp_Pnt& origin, const gp_Dir& direction, double base) {
  Manipulator handle;
  handle.kind = Manipulator::Kind::Arrow;
  handle.origin = origin;
  handle.direction = direction;
  handle.base = base;
  return handle;
}

Manipulator ring(const gp_Pnt& origin, const gp_Dir& axis) {
  Manipulator handle;
  handle.kind = Manipulator::Kind::Ring;
  handle.origin = origin;
  handle.direction = axis;
  // Angle 0 along the model axis most across the ring's axis.
  gp_Vec reference = std::abs(axis.Z()) < 0.9 ? gp_Vec(0, 0, 1) : gp_Vec(1, 0, 0);
  reference = gp_Vec(axis).Crossed(reference).Crossed(gp_Vec(axis)).Reversed();
  handle.reference = gp_Dir(reference);
  return handle;
}

QString lengthText(double millimetres, const CommandContext& context) {
  const double value = millimetres / context.unitMillimetres();
  QString text = QString::number(value, 'f', 4);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  if (text == QStringLiteral("-0")) {
    text = QStringLiteral("0");
  }
  return text + QLatin1Char(' ') + context.lengthUnit();
}

void placeAt(const SelectionItem& item, CommandState& state, const CommandContext& context,
             const QString& x, const QString& y) {
  if (!item.at) {
    return;
  }
  const auto frame = frameOf(item, context);
  if (!frame) {
    return;
  }
  const gp_Vec offset(frame->origin, gp_Pnt((*item.at)[0], (*item.at)[1], (*item.at)[2]));
  // Rounded to a hundredth of a millimetre, as typed values would be.
  const auto round = [](double value) { return std::round(value * 100.0) / 100.0; };
  state.setText(x, lengthText(round(offset.Dot(gp_Vec(frame->x))), context));
  state.setText(y, lengthText(round(offset.Dot(gp_Vec(frame->y))), context));
  state.setValue(x, std::numeric_limits<double>::quiet_NaN());
  state.setValue(y, std::numeric_limits<double>::quiet_NaN());
}

std::optional<gp_Pnt> pointOnPlane(const CommandState& state, const CommandContext& context,
                                   const QString& plane, const QString& x, const QString& y) {
  const Selection& items = state.items(plane);
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const auto frame = frameOf(items.first(), context);
  const double u = state.value(x);
  const double v = state.value(y);
  if (!frame || !std::isfinite(u) || !std::isfinite(v)) {
    return std::nullopt;
  }
  return frame->origin.Translated(gp_Vec(frame->x) * u + gp_Vec(frame->y) * v);
}

} // namespace mitcad::cmd
