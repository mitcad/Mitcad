// SPDX-License-Identifier: MIT
#include "CommandSupport.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <functional>
#include <limits>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <Bnd_Box.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>

#include <QHash>
#include <QRegularExpression>

#include "../framework/ModelShapes.hpp"
#include "../framework/Numbers.hpp"
#include "SketchNormal.hpp"

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
  return {choiceInput(QStringLiteral("operation"), QObject::tr("Operation"), choices, kNewBody),
          selectionInput(QStringLiteral("objects"), QObject::tr("Objects"), SelectKind::Body, 0, 0)
              .withoutMeshes()
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
          .withoutMeshes()
          // A face of another component in front of an origin plane is
          // the pick while a component is active (mitcad#762).
          .withPlanesByDepth()
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

// Objects of other components (mitcad#762).

bool addObjectLink(QJsonObject& links, const SelectionItem& item, const QString& here) {
  if ((item.kind != SelectKind::Face && item.kind != SelectKind::Body) || item.occurrence == here) {
    return true;
  }
  // Empty paths (the root) are left out, as the model saves them.
  QJsonObject link;
  if (!item.occurrence.isEmpty()) {
    link.insert(QStringLiteral("source"), item.occurrence);
  }
  if (!here.isEmpty()) {
    link.insert(QStringLiteral("target"), here);
  }
  if (links.contains(item.owner) && links.value(item.owner).toObject() != link) {
    return false;
  }
  links.insert(item.owner, link);
  return true;
}

void linkObject(SelectionItem& item, const QJsonObject& links) {
  if ((item.kind == SelectKind::Face || item.kind == SelectKind::Body) && links.contains(item.owner)) {
    item.occurrence = str(links.value(item.owner).toObject(), "source");
  }
}

namespace {

// The component of the profiles' sketch (a feature of the model): "C0" for
// the root's; empty when the query fails.
QString sketchComponentOf(const Selection& profiles, const CommandContext& context) {
  if (profiles.isEmpty()) {
    return QString();
  }
  try {
    return context
        .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                      {QStringLiteral("uid"), profiles.first().owner}})
        .value(QStringLiteral("component"))
        .toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    return QString();
  }
}

// An edited feature's links' target, kept by keepLinksTarget().
const QString kLinksTarget = QStringLiteral("object_links_target");

} // namespace

QString profilesOccurrence(const Selection& profiles, const CommandContext& context) {
  if (profiles.isEmpty()) {
    return QString();
  }
  const SelectionItem& first = profiles.first();
  if (!first.occurrence.isEmpty()) {
    return first.occurrence;
  }
  // A profile the command took itself (the newest one) or loaded: where
  // its sketch's component is seen first.
  const QString component = sketchComponentOf(profiles, context);
  if (component.isEmpty() || component == QStringLiteral("C0")) {
    return QString();
  }
  QString path;
  bool found = false;
  eachOccurrence(context, [&component, &path, &found](const QJsonObject& node, const QString& at) {
    if (!found && str(node, "component") == component) {
      path = at;
      found = true;
    }
  });
  return path;
}

void keepLinksTarget(CommandState& state, const QJsonObject& links) {
  QJsonObject kept;
  if (!links.isEmpty()) {
    kept.insert(QStringLiteral("target"), str(links.constBegin().value().toObject(), "target"));
  }
  state.setObject(kLinksTarget, kept);
}

QString linksOccurrence(const Selection& profiles, const CommandState& state, const CommandContext& context) {
  // The session sets a profile's occurrence to the first one of its
  // sketch's component (canonical picks): an edit keeps the target its
  // links had, while that occurrence still places the sketch's component.
  const QJsonObject kept = state.object(kLinksTarget);
  if (kept.contains(QStringLiteral("target"))) {
    const QString target = str(kept, "target");
    const QString component = sketchComponentOf(profiles, context);
    if (!component.isEmpty() && component == occurrenceComponent(target, context)) {
      return target;
    }
  }
  return profilesOccurrence(profiles, context);
}

QString occurrenceComponent(const QString& path, const CommandContext& context) {
  if (path.isEmpty()) {
    return QStringLiteral("C0");
  }
  QString component;
  eachOccurrence(context, [&path, &component](const QJsonObject& node, const QString& at) {
    if (at == path) {
      component = str(node, "component");
    }
  });
  return component;
}

// Construction planes of other components (mitcad#772).

QString planeComponent(const SelectionItem& item, const CommandContext& context) {
  const QString& owner = item.owner;
  if (item.kind != SelectKind::Plane || owner == QStringLiteral("xy") || owner == QStringLiteral("xz") ||
      owner == QStringLiteral("yz")) {
    return QString(); // an origin plane is every component's own
  }
  try {
    return context
        .queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), owner}})
        .value(QStringLiteral("component"))
        .toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    return QString();
  }
}

QString foreignPlane(const SelectionItem& item, const Selection& profiles, const CommandContext& context) {
  const QString at = planeComponent(item, context);
  if (at.isEmpty()) {
    return QString();
  }
  // The feature is in its profiles' sketch's component (a face profile's
  // component is not known here: left to the model).
  const QString own = sketchComponentOf(profiles, context);
  if (own.isEmpty() || own == at) {
    return QString();
  }
  QString name = item.owner;
  try {
    name = context
               .queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), item.owner}})
               .value(QStringLiteral("name"))
               .toString(item.owner);
  } catch (const std::exception&) {
    // Its uid then.
  }
  return QObject::tr("%1 is in %2: pick a plane of %3, or a face of %2.")
      .arg(name, componentName(at, context), componentName(own, context));
}

// ---------------------------------------------------------------------------
// Values

void loadValue(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context) {
  if (slot.isNull() || slot.isUndefined()) {
    return;
  }
  const QString text = context.valueText(slot, def.value(QStringLiteral("uid")).toString());
  state.setText(key, text);
  // The feature's own parameter shows its expression: its name goes with
  // the label (mitcad#231).
  if (slot.isString() && text != slot.toString()) {
    state.setParameterName(key, slot.toString());
  }
}

QString negated(const QString& expression) {
  static const QRegularExpression number(QStringLiteral(R"(^\s*([-+]?)\s*(\d+\.?\d*|\.\d+)(.*)$)"));
  const auto match = number.match(expression);
  if (match.hasMatch() && !match.captured(3).contains(QRegularExpression(QStringLiteral("[-+*/^()]")))) {
    return (match.captured(1) == QStringLiteral("-") ? QString() : QStringLiteral("-")) + match.captured(2) +
           match.captured(3);
  }
  return QStringLiteral("-(%1)").arg(expression);
}

bool nonZero(const CommandState& state, const QString& key) {
  const double value = state.value(key);
  return std::isfinite(value) && value != 0.0;
}

bool zeroOrNone(const QJsonValue& slot) {
  return slot.isUndefined() || slot.isNull() || (slot.isDouble() && slot.toDouble() == 0.0);
}

void loadCount(CommandState& state, const QString& key, const QJsonValue& slot,
               const QJsonObject& def, const CommandContext& context) {
  // A count the definition does not have keeps the input's default (an
  // earlier gear without profile_shift or mate_teeth, mitcad#656).
  if (slot.isNull() || slot.isUndefined()) {
    return;
  }
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
    // The feature's own count shows its expression, with its name by the
    // label (its parameter is unitless, mitcad#662); a borrowed one its name.
    if (parameter.value(QStringLiteral("owner")).toString() == owner) {
      state.setText(key, parameter.value(QStringLiteral("expression")).toString());
      state.setParameterName(key, name);
    } else {
      state.setText(key, name);
    }
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
      // A region of a QR code brings the code's other dark regions: the
      // code extrudes whole (mitcad#473).
      const QString sketch = unused.last().owner;
      const QJsonObject model =
          context.queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), sketch}});
      const QJsonArray regions = model.value(QStringLiteral("regions")).toArray();
      QString text;
      for (const QJsonValue& value : regions) {
        const QJsonObject region = value.toObject();
        if (region.value(QStringLiteral("key")).toString() == unused.last().name) {
          text = region.value(QStringLiteral("text")).toString();
        }
      }
      bool code = false;
      for (const QJsonValue& value : model.value(QStringLiteral("texts")).toArray()) {
        const QJsonObject t = value.toObject();
        code = code || (!text.isEmpty() && t.value(QStringLiteral("id")).toString() == text &&
                        t.value(QStringLiteral("code")).isObject());
      }
      if (code) {
        profiles.clear();
        for (const QJsonValue& value : regions) {
          const QJsonObject region = value.toObject();
          const QString key = region.value(QStringLiteral("key")).toString();
          const bool free = std::any_of(unused.begin(), unused.end(), [&](const SelectionItem& item) {
            return item.owner == sketch && item.name == key;
          });
          if (free && region.value(QStringLiteral("text")).toString() == text &&
              region.value(QStringLiteral("letter")).toBool()) {
            profiles.append({SelectKind::Profile, sketch, key, QStringLiteral("plane")});
          }
        }
        if (profiles.isEmpty()) {
          profiles.append(unused.last());
        }
      }
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

QJsonObject componentsOf(const CommandContext& context) {
  return context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
}

QString componentName(const QString& uid, const CommandContext& context) {
  try {
    for (const QJsonValue& value : componentsOf(context).value(QStringLiteral("components")).toArray()) {
      if (str(value.toObject(), "uid") == uid) {
        return str(value.toObject(), "name");
      }
    }
  } catch (const std::exception&) {
    // The uid then.
  }
  return uid;
}

void eachOccurrence(const CommandContext& context,
                    const std::function<void(const QJsonObject&, const QString&)>& visit) {
  std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& level, const QString& parent) {
    for (const QJsonValue& value : level) {
      const QJsonObject node = value.toObject();
      const QString path = parent.isEmpty() ? str(node, "uid") : parent + QLatin1Char('/') + str(node, "uid");
      visit(node, path);
      walk(node.value(QStringLiteral("children")).toArray(), path);
    }
  };
  try {
    walk(componentsOf(context).value(QStringLiteral("occurrences")).toArray(), QString());
  } catch (const std::exception&) {
    // No occurrences then.
  }
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
    // The frame is in the sketch's component's coordinates: into the
    // design through every occurrence on the path the profiles are seen
    // at (mitcad#958: a sketch in a turned occurrence pointed its arrow
    // along the component's normal).
    const QString path = profilesOccurrence(profiles, context);
    const QJsonArray occurrences =
        path.isEmpty() ? QJsonArray() : componentsOf(context).value(QStringLiteral("occurrences")).toArray();
    const std::optional<gp_Dir> normal = sketchNormalInDesign(frame, occurrences, path);
    if (!normal) {
      return std::nullopt;
    }
    gp_Pnt at = *center;
    // A profile without its occurrence is shown unplaced by itemShape: its
    // middle in the component's coordinates too.
    if (profiles.first().occurrence.isEmpty()) {
      if (const std::optional<gp_Trsf> world = occurrenceWorldIn(occurrences, path)) {
        at.Transform(*world);
      }
    }
    return std::make_pair(at, *normal);
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

QString shortLength(double millimetres, const CommandContext& context) {
  return lengthText(std::round(millimetres * 10.0) / 10.0, context);
}

QString beltText(const QJsonObject& belt, const QString& profile, const CommandContext& context) {
  static const QHash<QString, QString> names{{QStringLiteral("gt2_2mm"), QStringLiteral("GT2")},
                                             {QStringLiteral("gt2_3mm"), QStringLiteral("GT2 3M")},
                                             {QStringLiteral("htd_3m"), QStringLiteral("HTD 3M")},
                                             {QStringLiteral("htd_5m"), QStringLiteral("HTD 5M")},
                                             {QStringLiteral("t5"), QStringLiteral("T5")},
                                             {QStringLiteral("xl"), QStringLiteral("XL")}};
  QString text = shortLength(belt.value(QStringLiteral("length")).toDouble(), context);
  const QString name = names.value(profile);
  if (!name.isEmpty()) {
    text = name + QLatin1Char(' ') + text;
  }
  if (belt.value(QStringLiteral("width")).isDouble()) {
    text += QObject::tr(", %1 wide").arg(shortLength(belt.value(QStringLiteral("width")).toDouble(), context));
  } else if (belt.value(QStringLiteral("teeth")).isDouble()) {
    text += QObject::tr(", %1 teeth").arg(number(belt.value(QStringLiteral("teeth")).toDouble()));
  }
  return text;
}

std::vector<gp_Pnt> beltPitchLine(const gp_Pnt& first, const gp_Dir& normal, double d1, const gp_Pnt& second,
                                  double d2) {
  if (!(d1 > 0.0) || !(d2 > 0.0)) {
    return {};
  }
  const gp_Vec axis(normal);
  gp_Vec apart(first, second);
  apart -= axis * apart.Dot(axis); // into the first pulley's plane
  const double distance = apart.Magnitude();
  const double r1 = d1 / 2.0;
  const double r2 = d2 / 2.0;
  if (distance <= std::abs(r1 - r2) + 1e-6) {
    return {};
  }
  const gp_Vec u = apart / distance;
  const gp_Vec v = axis.Crossed(u);
  const gp_Pnt c2 = first.Translated(apart);
  // Where the spans leave the circles: at +-t from the line of centres.
  const double t = std::acos(std::clamp((r1 - r2) / distance, -1.0, 1.0));
  const auto on = [&u, &v](const gp_Pnt& centre, double radius, double angle) {
    return centre.Translated(u * (radius * std::cos(angle)) + v * (radius * std::sin(angle)));
  };
  constexpr double kPi = 3.14159265358979323846;
  constexpr int kSteps = 48;
  std::vector<gp_Pnt> line;
  for (int i = 0; i <= kSteps; ++i) { // round the first, away from the second
    line.push_back(on(first, r1, t + (2.0 * kPi - 2.0 * t) * i / kSteps));
  }
  for (int i = 0; i <= kSteps; ++i) { // round the second, away from the first
    line.push_back(on(c2, r2, -t + 2.0 * t * i / kSteps));
  }
  line.push_back(line.front());
  return line;
}

QString beltFitText(const QString& fit) {
  if (fit == QStringLiteral("ok")) {
    return QObject::tr("fits");
  }
  if (fit == QStringLiteral("tensioner")) {
    return QObject::tr("needs a tensioner");
  }
  if (fit == QStringLiteral("short")) {
    return QObject::tr("belt too short");
  }
  return QString();
}

QString travelText(double millimetres, const CommandContext& context) {
  if (std::abs(millimetres) < 0.0005) {
    return lengthText(0.0, context);
  }
  const QString size = lengthText(std::abs(millimetres), context);
  return millimetres > 0.0 ? QObject::tr("%1 apart").arg(size) : QObject::tr("%1 closer").arg(size);
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

// ---------------------------------------------------------------------------
// Splitting and trimming tools (Split Face, Split Body, Trim)

// A splitting tool: one plane, face or body, or sketch curves of one sketch.
QJsonValue toolOf(const Selection& items, Built& error, const QString& input) {
  if (items.first().kind == SelectKind::SketchCurve) {
    const QString sketch = oneSketch(items, error, input);
    for (const SelectionItem& item : items) {
      if (item.kind != SelectKind::SketchCurve) {
        error = Built::failure(QObject::tr("Select sketch curves, or one plane, face or body."), input);
      }
    }
    return QJsonObject{{QStringLiteral("sketch"), sketch},
                       {QStringLiteral("curves"), names(items, SelectKind::SketchCurve)}};
  }
  if (items.size() != 1) {
    error = Built::failure(QObject::tr("Select one plane, face or body, or sketch curves of one sketch."), input);
    return QJsonValue();
  }
  return items.first().reference();
}

std::optional<Selection> toolItems(const QJsonValue& tool, const CommandContext& context) {
  const QJsonObject o = tool.toObject();
  if (o.contains(QStringLiteral("sketch"))) {
    if (o.contains(QStringLiteral("curve"))) {
      return Selection{itemOf(tool, context)};
    }
    return pathItems(tool, context);
  }
  const SelectionItem item = itemOf(tool, context);
  if (!item.isValid()) {
    return std::nullopt;
  }
  return Selection{item};
}

// A tool of sketch curves can be swept along a direction (an along
// vector); without one, along the sketch's normal.
QVector<InputDef> directionInputs() {
  const auto shown = [](const CommandState& s) {
    const Selection& tool = s.items(QStringLiteral("tool"));
    return !tool.isEmpty() && tool.first().kind == SelectKind::SketchCurve;
  };
  const auto vector = [shown](const CommandState& s) {
    return shown(s) && s.choice(QStringLiteral("split_type")) == QStringLiteral("vector");
  };
  return {
      choiceInput(QStringLiteral("split_type"), QObject::tr("Split Type"),
                  {{QStringLiteral("normal"), QStringLiteral("Along Sketch Normal")},
                   {QStringLiteral("vector"), QStringLiteral("Along Vector")}},
                  QStringLiteral("normal"))
          .withVisible(shown),
      valueInput(QStringLiteral("dx"), QObject::tr("Direction X"), ValueKind::Unitless, QStringLiteral("0"))
          .withVisible(vector),
      valueInput(QStringLiteral("dy"), QObject::tr("Direction Y"), ValueKind::Unitless, QStringLiteral("0"))
          .withVisible(vector),
      valueInput(QStringLiteral("dz"), QObject::tr("Direction Z"), ValueKind::Unitless, QStringLiteral("1"))
          .withVisible(vector),
  };
}

// The tool, its plane's offset, extension and direction into a definition.
bool addTool(QJsonObject& def, const CommandState& state, Built& error) {
  const Selection& tool = state.items(QStringLiteral("tool"));
  const QJsonValue ref = toolOf(tool, error, QStringLiteral("tool"));
  if (!error.error.isEmpty()) {
    return false;
  }
  def.insert(QStringLiteral("tool"), ref);
  if (tool.first().kind == SelectKind::Plane && nonZero(state, QStringLiteral("offset"))) {
    def.insert(QStringLiteral("offset"), state.expression(QStringLiteral("offset")));
  }
  if (tool.first().kind == SelectKind::SketchCurve && state.choice(QStringLiteral("split_type")) == QStringLiteral("vector")) {
    const double x = state.value(QStringLiteral("dx"));
    const double y = state.value(QStringLiteral("dy"));
    const double z = state.value(QStringLiteral("dz"));
    if (std::hypot(x, y, z) < 1e-12) {
      error = Built::failure(QObject::tr("The direction is zero."), QStringLiteral("dz"));
      return false;
    }
    def.insert(QStringLiteral("direction"), QJsonArray{x, y, z});
  }
  if (!state.checked(QStringLiteral("extend"))) {
    def.insert(QStringLiteral("extend"), false);
  }
  return true;
}

void loadTool(const QJsonObject& feature, CommandState& state, const CommandContext& context) {
  if (const auto items = toolItems(feature.value(QStringLiteral("tool")), context)) {
    state.setItems(QStringLiteral("tool"), *items);
  }
  loadValue(state, QStringLiteral("offset"), feature.value(QStringLiteral("offset")), feature, context);
  const QJsonArray direction = feature.value(QStringLiteral("direction")).toArray();
  if (direction.size() == 3) {
    state.setChoice(QStringLiteral("split_type"), QStringLiteral("vector"));
    state.setText(QStringLiteral("dx"), number(direction[0].toDouble()));
    state.setText(QStringLiteral("dy"), number(direction[1].toDouble()));
    state.setText(QStringLiteral("dz"), number(direction[2].toDouble()));
  }
  state.setChecked(QStringLiteral("extend"), feature.value(QStringLiteral("extend")).toBool(true));
}

bool toolEditable(const QJsonObject& feature, const CommandContext& context) {
  return toolItems(feature.value(QStringLiteral("tool")), context).has_value();
}

InputDef toolInput(const QString& label) {
  return selectionInput(QStringLiteral("tool"), label.isEmpty() ? QObject::tr("Splitting Tool") : label,
                        SelectKind::Plane | SelectKind::Face | SelectKind::Body | SelectKind::SketchCurve, 1, 0)
      .withoutMeshes()
      .withTooltip(QObject::tr("A plane, a face, a body, or sketch curves of one sketch"));
}

InputDef toolOffsetInput() {
  return valueInput(QStringLiteral("offset"), QObject::tr("Offset"), ValueKind::Length, QStringLiteral("0 mm"))
      .withTooltip(QObject::tr("Moves a plane tool along its normal"))
      .withVisible([](const CommandState& s) {
        const Selection& tool = s.items(QStringLiteral("tool"));
        return !tool.isEmpty() && tool.first().kind == SelectKind::Plane;
      });
}

} // namespace mitcad::cmd
