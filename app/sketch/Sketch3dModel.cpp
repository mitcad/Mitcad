// SPDX-License-Identifier: MIT
#include "Sketch3dModel.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QCoreApplication>
#include <QJsonArray>
#include <QRegularExpression>

#include "framework/UserText.hpp"

namespace mitcad::sketch {
namespace {

// The file's texts in the context Sketch3d, where lupdate records them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(Sketch3d)
};

Xyz xyzOf(const QJsonValue& value, bool* ok = nullptr) {
  const QJsonArray a = value.toArray();
  if (ok != nullptr) {
    *ok = a.size() >= 3;
  }
  if (a.size() < 3) {
    return Xyz{};
  }
  return Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
}

QStringList strings(const QJsonValue& value) {
  QStringList out;
  for (const QJsonValue& item : value.toArray()) {
    out << item.toString();
  }
  return out;
}

// The point and curve ids a relation, a dimension or a curve names, in
// the order its fields give them; a source's own ids (another sketch's)
// are not the sketch's.
void collectIds(const QJsonValue& value, QStringList& ids) {
  static const QRegularExpression entity(QStringLiteral("^[pc][0-9]+$"));
  if (value.isString()) {
    const QString text = value.toString();
    if (entity.match(text).hasMatch() && !ids.contains(text)) {
      ids << text;
    }
  } else if (value.isArray()) {
    for (const QJsonValue& item : value.toArray()) {
      collectIds(item, ids);
    }
  } else if (value.isObject()) {
    const QJsonObject object = value.toObject();
    for (auto it = object.begin(); it != object.end(); ++it) {
      if (it.key() != QStringLiteral("id") && it.key() != QStringLiteral("source") &&
          it.key() != QStringLiteral("geometry") && it.key() != QStringLiteral("target")) {
        collectIds(it.value(), ids);
      }
    }
  }
}

QString number(double value) {
  return QString::number(std::abs(value) < 5e-7 ? 0.0 : value, 'f', 2);
}

QString coordinates(const Xyz& at, double millimetres) {
  const double scale = millimetres > 0.0 ? millimetres : 1.0;
  return QStringLiteral("%1, %2, %3").arg(number(at[0] / scale), number(at[1] / scale), number(at[2] / scale));
}

// A curve's kind in English, its text to translate.
const char* curveKindSource(const QString& type) {
  static const char* const kinds[][2] = {
      {"line", QT_TRANSLATE_NOOP("Sketch3d", "Line")},
      {"arc", QT_TRANSLATE_NOOP("Sketch3d", "Arc")},
      {"circle", QT_TRANSLATE_NOOP("Sketch3d", "Circle")},
      {"spline", QT_TRANSLATE_NOOP("Sketch3d", "Control point spline")},
      {"fitted_spline", QT_TRANSLATE_NOOP("Sketch3d", "Fit point spline")},
      {"ellipse", QT_TRANSLATE_NOOP("Sketch3d", "Ellipse")},
      {"elliptical_arc", QT_TRANSLATE_NOOP("Sketch3d", "Elliptical arc")},
      {"reference", QT_TRANSLATE_NOOP("Sketch3d", "Reference curve")},
  };
  for (const auto& entry : kinds) {
    if (type == QLatin1String(entry[0])) {
      return entry[1];
    }
  }
  return QT_TRANSLATE_NOOP("Sketch3d", "Curve");
}

QString curveKind(const QString& type) { return Text::tr(curveKindSource(type)); }
// The same for agents and the log, in English.
QString curveTypeName(const QString& type) { return QString::fromLatin1(curveKindSource(type)); }

// A relation's or dimension's kind in English, its text to translate.
const char* itemKindSource(const QString& type, bool dimension);

QString itemKind(const QString& type, bool dimension) { return Text::tr(itemKindSource(type, dimension)); }
QString itemTypeName(const QString& type, bool dimension) {
  return QString::fromLatin1(itemKindSource(type, dimension));
}

const char* itemKindSource(const QString& type, bool dimension) {
  static const char* const relations[][2] = {
      {"coincident", QT_TRANSLATE_NOOP("Sketch3d", "Coincident")},
      {"fix", QT_TRANSLATE_NOOP("Sketch3d", "Fix")},
      {"attach", QT_TRANSLATE_NOOP("Sketch3d", "Attachment")},
      {"align", QT_TRANSLATE_NOOP("Sketch3d", "Axis alignment")},
      {"point_on", QT_TRANSLATE_NOOP("Sketch3d", "Point on Curve")},
      {"midpoint", QT_TRANSLATE_NOOP("Sketch3d", "Midpoint")},
      {"tangent", QT_TRANSLATE_NOOP("Sketch3d", "Tangent")},
      {"equal", QT_TRANSLATE_NOOP("Sketch3d", "Equal")},
      {"normal_align", QT_TRANSLATE_NOOP("Sketch3d", "Normal alignment")},
      {"on_sketch_plane", QT_TRANSLATE_NOOP("Sketch3d", "On Sketch Plane")},
      {"in_plane", QT_TRANSLATE_NOOP("Sketch3d", "In-plane relation")},
      {"in_plane_fixed", QT_TRANSLATE_NOOP("Sketch3d", "In-plane Fix")},
      {"on_face", QT_TRANSLATE_NOOP("Sketch3d", "On Face")},
      {"plane_bound", QT_TRANSLATE_NOOP("Sketch3d", "Shape plane")},
      {"parallel", QT_TRANSLATE_NOOP("Sketch3d", "Parallel")},
      {"perpendicular", QT_TRANSLATE_NOOP("Sketch3d", "Perpendicular")},
      {"symmetric", QT_TRANSLATE_NOOP("Sketch3d", "Symmetric")},
      {"concentric", QT_TRANSLATE_NOOP("Sketch3d", "Concentric")},
      {"curvature", QT_TRANSLATE_NOOP("Sketch3d", "Curvature")},
      {"on_plane", QT_TRANSLATE_NOOP("Sketch3d", "On Plane")},
  };
  static const char* const dimensions[][2] = {
      {"length", QT_TRANSLATE_NOOP("Sketch3d", "Length")},
      {"offset", QT_TRANSLATE_NOOP("Sketch3d", "Offset")},
      {"radius", QT_TRANSLATE_NOOP("Sketch3d", "Radius")},
      {"diameter", QT_TRANSLATE_NOOP("Sketch3d", "Diameter")},
      {"arc_length", QT_TRANSLATE_NOOP("Sketch3d", "Arc Length")},
      {"spline_length", QT_TRANSLATE_NOOP("Sketch3d", "Spline Length")},
      {"in_plane", QT_TRANSLATE_NOOP("Sketch3d", "In-plane dimension")},
      {"angle", QT_TRANSLATE_NOOP("Sketch3d", "Angle")},
      {"tangent_magnitude", QT_TRANSLATE_NOOP("Sketch3d", "Tangent Magnitude")},
  };
  if (dimension) {
    for (const auto& entry : dimensions) {
      if (type == QLatin1String(entry[0])) {
        return entry[1];
      }
    }
    return QT_TRANSLATE_NOOP("Sketch3d", "Dimension");
  }
  for (const auto& entry : relations) {
    if (type == QLatin1String(entry[0])) {
      return entry[1];
    }
  }
  return QT_TRANSLATE_NOOP("Sketch3d", "Relation");
}

} // namespace

Sketch3dModel Sketch3dModel::read(const QJsonObject& sketch) {
  Sketch3dModel model;
  if (sketch.value(QStringLiteral("type")).toString() != QStringLiteral("spatial_sketch")) {
    return model;
  }
  model.uid = sketch.value(QStringLiteral("uid")).toString();
  model.name = sketch.value(QStringLiteral("name")).toString();
  model.support = sketch.value(QStringLiteral("support"));
  const QJsonObject frame = sketch.value(QStringLiteral("frame")).toObject();
  bool origin = false;
  bool x = false;
  bool y = false;
  bool n = false;
  model.origin = xyzOf(frame.value(QStringLiteral("origin")), &origin);
  model.xAxis = xyzOf(frame.value(QStringLiteral("x_axis")), &x);
  model.yAxis = xyzOf(frame.value(QStringLiteral("y_axis")), &y);
  model.normal = xyzOf(frame.value(QStringLiteral("normal")), &n);
  model.hasFrame = origin && x && y && n;
  for (const QJsonValue& value : sketch.value(QStringLiteral("points")).toArray()) {
    const QJsonObject p = value.toObject();
    Sketch3dPoint point;
    point.id = p.value(QStringLiteral("id")).toString();
    point.at = xyzOf(p.value(QStringLiteral("at")));
    point.position = xyzOf(p.value(QStringLiteral("position")), &point.hasPosition);
    point.determined = p.value(QStringLiteral("determined")).toBool();
    point.onSketchPlane = p.value(QStringLiteral("on_sketch_plane")).toBool();
    model.points.push_back(point);
  }
  for (const QJsonValue& value : sketch.value(QStringLiteral("curves")).toArray()) {
    const QJsonObject c = value.toObject();
    Sketch3dCurve curve;
    curve.id = c.value(QStringLiteral("id")).toString();
    curve.type = c.value(QStringLiteral("type")).toString();
    curve.construction = c.value(QStringLiteral("construction")).toBool();
    curve.planeBound = c.value(QStringLiteral("plane_bound")).toBool();
    curve.determined = c.value(QStringLiteral("determined")).toBool();
    collectIds(c, curve.points);
    curve.points.removeAll(curve.id);
    curve.json = c;
    model.curves.push_back(curve);
  }
  for (const bool dimension : {false, true}) {
    const QString list = dimension ? QStringLiteral("dimensions") : QStringLiteral("constraints");
    for (const QJsonValue& value : sketch.value(list).toArray()) {
      const QJsonObject k = value.toObject();
      Sketch3dItem item;
      item.id = k.value(QStringLiteral("id")).toString();
      item.type = k.value(QStringLiteral("type")).toString();
      item.dimension = dimension;
      item.inPlane = k.value(QStringLiteral("in_plane")).toBool();
      item.redundant = k.value(QStringLiteral("redundant")).toBool();
      item.driven = k.value(QStringLiteral("driven")).toBool();
      QJsonObject named = k;
      named.remove(QStringLiteral("value")); // a parameter's name
      collectIds(named, item.entities);
      item.json = k;
      model.items.push_back(item);
    }
  }
  for (const QJsonValue& value : sketch.value(QStringLiteral("references")).toArray()) {
    const QJsonObject r = value.toObject();
    Sketch3dReference reference;
    reference.id = r.value(QStringLiteral("id")).toString();
    reference.kind = r.value(QStringLiteral("kind")).toString();
    reference.mode = r.value(QStringLiteral("mode")).toString();
    reference.status = r.value(QStringLiteral("status")).toString();
    reference.message = r.value(QStringLiteral("message")).toString();
    const QString detail = r.value(QStringLiteral("detail")).toString();
    reference.detail = detail.isEmpty() ? reference.message : detail;
    reference.body = r.value(QStringLiteral("body")).toString();
    reference.occurrence = r.value(QStringLiteral("occurrence")).toString();
    reference.source = r.value(QStringLiteral("source"));
    reference.users = strings(r.value(QStringLiteral("users")));
    model.references.push_back(reference);
  }
  model.texts = static_cast<int>(sketch.value(QStringLiteral("texts")).toArray().size());
  model.evaluated = sketch.value(QStringLiteral("evaluated")).toBool();
  const QJsonObject status = sketch.value(QStringLiteral("status")).toObject();
  model.solved = !status.isEmpty();
  if (model.solved) {
    model.dof = status.value(QStringLiteral("dof")).toInt(-1);
    model.fullyConstrained = status.value(QStringLiteral("fully_constrained")).toBool();
    model.redundant = strings(status.value(QStringLiteral("redundant")));
  }
  const QJsonObject failure = sketch.value(QStringLiteral("failure")).toObject();
  if (!failure.isEmpty()) {
    model.failureKind = failure.value(QStringLiteral("kind")).toString();
    model.failureMessage = failure.value(QStringLiteral("message")).toString();
    // The ids are the agents' and the log's (mitcad#1644).
    const QString detail = failure.value(QStringLiteral("detail")).toString();
    model.failureDetail = detail.isEmpty() ? model.failureMessage : detail;
    model.failureItems = strings(failure.value(QStringLiteral("constraints"))) +
                         strings(failure.value(QStringLiteral("entities")));
  }
  // Whether it all lies in its sketch plane (it can become a 2D sketch),
  // what does not, and the profile regions it bounds there (mitcad#1550).
  model.flat = sketch.value(QStringLiteral("flat")).toBool();
  model.offPlane = strings(sketch.value(QStringLiteral("off_plane")));
  // What a person reads of it (mitcad#1644), the agents' texts without it.
  model.offPlaneShown = strings(sketch.value(QStringLiteral("off_plane_messages")));
  if (model.offPlaneShown.isEmpty()) {
    for (const QString& text : model.offPlane) {
      model.offPlaneShown << userText(text);
    }
  }
  model.regions = strings(sketch.value(QStringLiteral("regions")));
  for (const QJsonValue& warning : sketch.value(QStringLiteral("warnings")).toArray()) {
    // A warning is a string or an object with its message; shown without
    // its tail, the ids the agents' (mitcad#1644).
    model.warnings << userText(warning.isString() ? warning.toString()
                                                  : warning.toObject().value(QStringLiteral("message")).toString());
  }
  return model;
}

const Sketch3dPoint* Sketch3dModel::point(const QString& id) const {
  const auto found = std::find_if(points.begin(), points.end(), [&id](const Sketch3dPoint& p) { return p.id == id; });
  return found != points.end() ? &*found : nullptr;
}

const Sketch3dCurve* Sketch3dModel::curve(const QString& id) const {
  const auto found = std::find_if(curves.begin(), curves.end(), [&id](const Sketch3dCurve& c) { return c.id == id; });
  return found != curves.end() ? &*found : nullptr;
}

const Sketch3dItem* Sketch3dModel::item(const QString& id) const {
  const auto found = std::find_if(items.begin(), items.end(), [&id](const Sketch3dItem& k) { return k.id == id; });
  return found != items.end() ? &*found : nullptr;
}

const Sketch3dReference* Sketch3dModel::reference(const QString& id) const {
  const auto found =
      std::find_if(references.begin(), references.end(), [&id](const Sketch3dReference& r) { return r.id == id; });
  return found != references.end() ? &*found : nullptr;
}

const Sketch3dReference* Sketch3dModel::referenceOf(const QString& id) const {
  if (const Sketch3dReference* own = reference(id)) {
    return own;
  }
  for (const Sketch3dReference& record : references) {
    if (record.users.contains(id)) {
      return &record;
    }
  }
  // A point on a reference curve: through the relation that puts it there.
  for (const Sketch3dItem& relation : items) {
    if (relation.dimension || !relation.entities.contains(id)) {
      continue;
    }
    for (const QString& entity : relation.entities) {
      const Sketch3dReference* record = reference(entity);
      if (record != nullptr && entity != id) {
        return record;
      }
    }
  }
  return nullptr;
}

int Sketch3dModel::determinedPoints() const {
  return static_cast<int>(
      std::count_if(points.begin(), points.end(), [](const Sketch3dPoint& p) { return p.determined; }));
}

int Sketch3dModel::determinedCurves() const {
  return static_cast<int>(std::count_if(curves.begin(), curves.end(), [](const Sketch3dCurve& c) {
    return c.determined && c.type != QStringLiteral("reference");
  }));
}

QStringList Sketch3dModel::inPlaneItems() const {
  QStringList ids;
  for (const Sketch3dItem& item : items) {
    if (item.inPlane) {
      ids << item.id;
    }
  }
  return ids;
}

int Sketch3dModel::onSketchPlanePoints() const {
  return static_cast<int>(
      std::count_if(points.begin(), points.end(), [](const Sketch3dPoint& p) { return p.onSketchPlane; }));
}

QStringList Sketch3dModel::failedReferences() const {
  QStringList ids;
  for (const Sketch3dReference& record : references) {
    if (record.status == QStringLiteral("failed")) {
      ids << record.id;
    }
  }
  return ids;
}

QString itemKindName(const QString& type, bool dimension) { return itemKind(type, dimension); }

QString itemKindEnglish(const QString& type, bool dimension) { return itemTypeName(type, dimension); }

QString entityName(const Sketch3dModel& model, const QString& id) {
  // Numbered within its kind in the sketch's order, from 1.
  int number = 0;
  for (const Sketch3dPoint& point : model.points) {
    ++number;
    if (point.id == id) {
      return Text::tr("Point %1").arg(number);
    }
  }
  if (const Sketch3dCurve* curve = model.curve(id)) {
    number = 0;
    for (const Sketch3dCurve& other : model.curves) {
      number += other.type == curve->type ? 1 : 0;
      if (&other == curve) {
        break;
      }
    }
    return Text::tr("%1 %2").arg(curveKind(curve->type)).arg(number);
  }
  if (const Sketch3dItem* item = model.item(id)) {
    number = 0;
    for (const Sketch3dItem& other : model.items) {
      number += other.type == item->type && other.dimension == item->dimension ? 1 : 0;
      if (&other == item) {
        break;
      }
    }
    return Text::tr("%1 %2").arg(itemKind(item->type, item->dimension)).arg(number);
  }
  return QString();
}

QString entityNames(const Sketch3dModel& model, const QStringList& ids) {
  QStringList names;
  for (const QString& id : ids) {
    const QString name = entityName(model, id);
    if (!name.isEmpty()) {
      names << name;
    }
  }
  return names.join(QStringLiteral(", "));
}

QString sourceText(const QJsonValue& source, const SourceNames& names) {
  if (source.isString()) {
    return source.toString() == QStringLiteral("origin") ? Text::tr("the origin") : Text::tr("a source");
  }
  const QJsonObject s = source.toObject();
  const QString at = s.value(QStringLiteral("at")).toString();
  const auto named = [&names](const QString& feature) {
    return names.feature && !feature.isEmpty() ? names.feature(feature) : QString();
  };
  // What it is on its owner, then the owner: a body by the name of the
  // feature that made it, a sketch by its name.
  QString part;
  bool ofBody = true;
  QString text;
  if (s.contains(QStringLiteral("vertex"))) {
    part = Text::tr("a vertex");
  } else if (s.contains(QStringLiteral("edge"))) {
    part = at == QStringLiteral("start")    ? Text::tr("the start of an edge")
           : at == QStringLiteral("end")    ? Text::tr("the end of an edge")
           : at == QStringLiteral("middle") ? Text::tr("the middle of an edge")
                                            : Text::tr("an edge");
  } else if (s.contains(QStringLiteral("face"))) {
    part = Text::tr("a face"); // mitcad#1549
  } else if (s.contains(QStringLiteral("point"))) {
    part = Text::tr("a point");
    ofBody = false;
  } else if (s.contains(QStringLiteral("curve"))) {
    part = at == QStringLiteral("start")    ? Text::tr("the start of a curve")
           : at == QStringLiteral("end")    ? Text::tr("the end of a curve")
           : at == QStringLiteral("middle") ? Text::tr("the middle of a curve")
                                            : Text::tr("a curve");
    ofBody = false;
  } else if (s.contains(QStringLiteral("feature"))) {
    // An Intersection Curve by its own name.
    const QString name = named(s.value(QStringLiteral("feature")).toString());
    const QString curve = name.isEmpty() ? Text::tr("an Intersection Curve") : name;
    text = at == QStringLiteral("start")    ? Text::tr("the start of %1").arg(curve)
           : at == QStringLiteral("end")    ? Text::tr("the end of %1").arg(curve)
           : at == QStringLiteral("middle") ? Text::tr("the middle of %1").arg(curve)
                                            : curve;
  } else {
    return Text::tr("a source");
  }
  if (!part.isEmpty()) {
    const QString owner = ofBody ? named(s.value(QStringLiteral("body")).toString().section(QLatin1Char('.'), 0, 0))
                                 : named(s.value(QStringLiteral("sketch")).toString());
    text = !owner.isEmpty() ? Text::tr("%1 of %2").arg(part, owner)
           : ofBody         ? Text::tr("%1 of a body").arg(part)
                            : Text::tr("%1 of another sketch").arg(part);
  }
  // Another component's, through its occurrence (mitcad#1543).
  const QString occurrence = s.value(QStringLiteral("link")).toObject().value(QStringLiteral("source")).toString();
  if (occurrence.isEmpty()) {
    return text;
  }
  const QString component = names.component ? names.component(occurrence) : QString();
  return component.isEmpty() ? Text::tr("%1 in another component").arg(text) : Text::tr("%1 in %2").arg(text, component);
}

QString sourceDetail(const QJsonValue& source) {
  if (source.isString()) {
    return source.toString() == QStringLiteral("origin") ? QStringLiteral("the origin") : source.toString();
  }
  const QJsonObject s = source.toObject();
  QString text;
  const QString at = s.value(QStringLiteral("at")).toString();
  const QString where = at == QStringLiteral("start") || at == QStringLiteral("end") || at == QStringLiteral("middle")
                            ? QStringLiteral("the %1 of ").arg(at)
                            : QString();
  if (s.contains(QStringLiteral("vertex"))) {
    text = QStringLiteral("vertex %1 of %2")
               .arg(s.value(QStringLiteral("vertex")).toString(), s.value(QStringLiteral("body")).toString());
  } else if (s.contains(QStringLiteral("edge"))) {
    text = QStringLiteral("%1edge %2 of %3")
               .arg(where, s.value(QStringLiteral("edge")).toString(), s.value(QStringLiteral("body")).toString());
  } else if (s.contains(QStringLiteral("face"))) {
    text = QStringLiteral("face %1 of %2")
               .arg(s.value(QStringLiteral("face")).toString(), s.value(QStringLiteral("body")).toString());
  } else if (s.contains(QStringLiteral("point"))) {
    text = QStringLiteral("point %1 of %2")
               .arg(s.value(QStringLiteral("point")).toString(), s.value(QStringLiteral("sketch")).toString());
  } else if (s.contains(QStringLiteral("curve"))) {
    text = QStringLiteral("%1curve %2 of %3")
               .arg(where, s.value(QStringLiteral("curve")).toString(), s.value(QStringLiteral("sketch")).toString());
  } else if (s.contains(QStringLiteral("feature"))) {
    text = QStringLiteral("Intersection Curve %1").arg(s.value(QStringLiteral("feature")).toString());
  } else {
    return QStringLiteral("a source");
  }
  const QString occurrence = s.value(QStringLiteral("link")).toObject().value(QStringLiteral("source")).toString();
  return occurrence.isEmpty() ? text : QStringLiteral("%1 in %2").arg(text, occurrence);
}

QJsonArray sourcePlaces(const QJsonValue& source) {
  QJsonArray places;
  const QJsonObject s = source.toObject();
  QJsonObject place;
  for (const char* kind : {"vertex", "edge", "face"}) {
    const QString key = QString::fromLatin1(kind);
    if (s.contains(key)) {
      place = QJsonObject{{QStringLiteral("kind"), key},
                          {QStringLiteral("name"), s.value(key).toString()},
                          {QStringLiteral("body"), s.value(QStringLiteral("body")).toString()}};
    }
  }
  if (place.isEmpty() && (s.contains(QStringLiteral("point")) || s.contains(QStringLiteral("curve")))) {
    const bool point = s.contains(QStringLiteral("point"));
    place = QJsonObject{
        {QStringLiteral("kind"), point ? QStringLiteral("sketch_point") : QStringLiteral("sketch_curve")},
        {QStringLiteral("sketch"), s.value(QStringLiteral("sketch")).toString()},
        {QStringLiteral("name"), s.value(point ? QStringLiteral("point") : QStringLiteral("curve")).toString()}};
  } else if (place.isEmpty() && s.contains(QStringLiteral("feature"))) {
    place = QJsonObject{{QStringLiteral("kind"), QStringLiteral("feature")},
                        {QStringLiteral("name"), s.value(QStringLiteral("feature")).toString()}};
  }
  if (place.isEmpty()) {
    return places; // the origin
  }
  const QString occurrence = s.value(QStringLiteral("link")).toObject().value(QStringLiteral("source")).toString();
  if (!occurrence.isEmpty()) {
    place.insert(QStringLiteral("occurrence"), occurrence);
  }
  places.append(place);
  return places;
}

QJsonArray entityPlaces(const Sketch3dModel& model, const QString& id) {
  QJsonArray places;
  const auto add = [&places](const QJsonArray& more) {
    for (const QJsonValue& place : more) {
      if (!places.contains(place)) {
        places.append(place);
      }
    }
  };
  // A point or curve of the sketch itself, where the view shows it.
  const auto own = [&model, &places](const QString& entity) {
    const bool point = model.point(entity) != nullptr;
    if (!point && model.curve(entity) == nullptr) {
      return;
    }
    const QJsonObject place{
        {QStringLiteral("kind"), point ? QStringLiteral("sketch_point") : QStringLiteral("sketch_curve")},
        {QStringLiteral("sketch"), model.uid},
        {QStringLiteral("name"), entity}};
    if (!places.contains(place)) {
      places.append(place);
    }
  };
  const auto recordSource = [&model, &add](const QString& record) {
    if (const Sketch3dReference* reference = model.reference(record)) {
      add(sourcePlaces(reference->source));
    }
  };
  if (model.point(id) != nullptr) {
    own(id);
    if (const Sketch3dReference* record = model.referenceOf(id)) {
      add(sourcePlaces(record->source));
    }
    return places;
  }
  if (model.curve(id) != nullptr) {
    own(id);
    recordSource(id); // a reference curve's source
    return places;
  }
  const Sketch3dItem* item = model.item(id);
  const Sketch3dReference* record = model.reference(id);
  if (item == nullptr && record == nullptr) {
    return places;
  }
  // A relation or dimension, or an attachment's or On Face's record: its
  // points and curves, its own source, the sources of the reference curves
  // it uses.
  QStringList entities = item != nullptr ? item->entities : QStringList();
  if (record != nullptr) {
    for (const QString& user : record->users) {
      if (!entities.contains(user)) {
        entities << user;
      }
    }
  }
  for (const QString& entity : std::as_const(entities)) {
    own(entity);
  }
  recordSource(id);
  for (const QString& entity : std::as_const(entities)) {
    if (model.curve(entity) != nullptr) {
      recordSource(entity);
    }
  }
  return places;
}

QString statusText(const Sketch3dModel& model) {
  if (!model.failureKind.isEmpty()) {
    return Text::tr("Not solved: %1").arg(model.failureMessage);
  }
  if (!model.solved) {
    return model.evaluated ? Text::tr("Not solved") : Text::tr("Not computed: the timeline is rolled back before it");
  }
  if (model.fullyConstrained) {
    return Text::tr("Fully constrained");
  }
  return Text::tr("%n degree(s) of freedom", nullptr, model.dof);
}

QStringList detailLines(const Sketch3dModel& model) {
  QStringList lines;
  if (model.solved) {
    const int curves = static_cast<int>(std::count_if(model.curves.begin(), model.curves.end(),
                                                      [](const Sketch3dCurve& c) {
                                                        return c.type != QStringLiteral("reference");
                                                      }));
    lines << Text::tr("Determined: %1 of %2 points, %3 of %4 curves")
                 .arg(model.determinedPoints())
                 .arg(static_cast<int>(model.points.size()))
                 .arg(model.determinedCurves())
                 .arg(curves);
  }
  // Counts in words, as the planar Sketch Palette says them (mitcad#1171,
  // mitcad#1644): the ids are the agents' (the query, statusLog), and the
  // list marks the items themselves.
  if (!model.redundant.isEmpty()) {
    lines << Text::tr("Redundant: %n relation(s) or dimension(s) the others hold already", nullptr,
                      static_cast<int>(model.redundant.size()));
  }
  if (!model.failureItems.isEmpty()) {
    lines << Text::tr("Involved: %n item(s), marked in the list", nullptr,
                      static_cast<int>(model.failureItems.size()));
  }
  const QStringList inPlane = model.inPlaneItems();
  if (!inPlane.isEmpty()) {
    lines << Text::tr("In plane: %n relation(s) or dimension(s)", nullptr, static_cast<int>(inPlane.size()));
  }
  if (const int kept = model.onSketchPlanePoints(); kept > 0) {
    lines << Text::tr("On Sketch Plane: %n point(s)", nullptr, kept);
  }
  if (!model.references.empty()) {
    const QStringList failed = model.failedReferences();
    const int records = static_cast<int>(model.references.size());
    lines << (failed.isEmpty() ? Text::tr("%n linked reference(s)", nullptr, records)
                               : Text::tr("%n linked reference(s), failed: %1", nullptr, records)
                                     .arg(static_cast<int>(failed.size())));
  }
  for (const QString& warning : model.warnings) {
    if (!warning.isEmpty()) {
      lines << warning;
    }
  }
  // Its closed loops in the sketch plane bound profiles (mitcad#1550).
  const int regions = static_cast<int>(model.regions.size());
  lines << (regions == 0 ? Text::tr("No profiles: no closed loop lies in the sketch plane.")
                         : Text::tr("Profiles: %n region(s) in the sketch plane", nullptr, regions));
  return lines;
}

QString describeEntity(const Sketch3dModel& model, const QString& id, const SourceNames& names) {
  // What it is by its kind and number, then its state as a list of whole
  // phrases; where it is, the view paints (mitcad#1652).
  const QString name = entityName(model, id);
  if (name.isEmpty()) {
    return QString();
  }
  QString head = name;
  QStringList state;
  if (const Sketch3dPoint* point = model.point(id)) {
    state << (point->determined ? Text::tr("determined") : Text::tr("free"));
    if (point->onSketchPlane) {
      state << Text::tr("On Sketch Plane");
    }
    if (const Sketch3dReference* record = model.referenceOf(id)) {
      state << Text::tr("linked to %1").arg(sourceText(record->source, names));
    }
  } else if (const Sketch3dCurve* curve = model.curve(id)) {
    if (const Sketch3dReference* record = model.reference(id)) {
      head = Text::tr("%1 (%2)").arg(name, sourceText(record->source, names));
      state << (record->mode == QStringLiteral("project") ? Text::tr("projected onto the sketch plane")
                                                          : Text::tr("included in 3D"));
      if (record->status == QStringLiteral("failed")) {
        state << Text::tr("failed: %1").arg(userText(record->message));
      }
    } else {
      const QString points = entityNames(model, curve->points);
      if (!points.isEmpty()) {
        state << Text::tr("its points: %1").arg(points);
      }
      state << (curve->determined ? Text::tr("determined") : Text::tr("free"));
      if (curve->construction) {
        state << Text::tr("construction");
      }
      if (curve->planeBound) {
        state << Text::tr("in the sketch plane");
      }
    }
  } else if (const Sketch3dItem* item = model.item(id)) {
    // What it is on, in the head as the palette's list row says it:
    // "Coincident 1 on Point 2, Point 4".
    const QString on = entityNames(model, item->entities);
    if (!on.isEmpty()) {
      head = Text::tr("%1 on %2").arg(name, on);
    }
    // An attachment's or On Face's source.
    if (const Sketch3dReference* record = model.reference(id)) {
      state << Text::tr("linked to %1").arg(sourceText(record->source, names));
    }
    if (item->inPlane) {
      state << Text::tr("In plane");
    }
    if (item->redundant) {
      state << Text::tr("redundant");
    }
    if (item->dimension && item->driven) {
      state << Text::tr("driven");
    }
  }
  return state.isEmpty() ? head : Text::tr("%1; %2").arg(head, state.join(QStringLiteral("; ")));
}

QString entityDetail(const Sketch3dModel& model, const QString& id, const QString& unit, double millimetres) {
  // The agents' and the log's: English, with the ids and the coordinates.
  QString head;
  QStringList state;
  if (const Sketch3dPoint* point = model.point(id)) {
    head = QStringLiteral("Point %1 at %2 %3").arg(id, coordinates(point->at, millimetres), unit);
    // Where the frame puts it in its component.
    if (point->hasPosition) {
      state << QStringLiteral("%1 %2 in the component").arg(coordinates(point->position, millimetres), unit);
    }
    state << (point->determined ? QStringLiteral("determined") : QStringLiteral("free"));
    if (point->onSketchPlane) {
      state << QStringLiteral("On Sketch Plane");
    }
    if (const Sketch3dReference* record = model.referenceOf(id)) {
      state << QStringLiteral("linked to %1").arg(sourceDetail(record->source));
    }
  } else if (const Sketch3dCurve* curve = model.curve(id)) {
    if (const Sketch3dReference* record = model.reference(id)) {
      head = QStringLiteral("Reference curve %1: %2").arg(id, sourceDetail(record->source));
      state << (record->mode == QStringLiteral("project") ? QStringLiteral("projected onto the sketch plane")
                                                          : QStringLiteral("included in 3D"));
      if (record->status == QStringLiteral("failed")) {
        state << QStringLiteral("failed: %1").arg(record->detail);
      }
    } else {
      head = QStringLiteral("%1 %2").arg(curveTypeName(curve->type), id);
      if (!curve->points.isEmpty()) {
        state << QStringLiteral("points %1").arg(curve->points.join(QStringLiteral(", ")));
      }
      state << (curve->determined ? QStringLiteral("determined") : QStringLiteral("free"));
      if (curve->construction) {
        state << QStringLiteral("construction");
      }
      if (curve->planeBound) {
        state << QStringLiteral("in the sketch plane");
      }
    }
  } else if (const Sketch3dItem* item = model.item(id)) {
    head = QStringLiteral("%1 %2").arg(itemTypeName(item->type, item->dimension), id);
    if (!item->entities.isEmpty()) {
      state << QStringLiteral("on %1").arg(item->entities.join(QStringLiteral(", ")));
    }
    if (item->inPlane) {
      state << QStringLiteral("In plane");
    }
    if (item->redundant) {
      state << QStringLiteral("redundant");
    }
    if (item->dimension && item->driven) {
      state << QStringLiteral("driven");
    }
  } else {
    return QString();
  }
  return state.isEmpty() ? head : QStringLiteral("%1; %2").arg(head, state.join(QStringLiteral("; ")));
}

QString statusLog(const Sketch3dModel& model) {
  const auto listed = [](const QStringList& ids) {
    return ids.isEmpty() ? QStringLiteral("none") : ids.join(QStringLiteral(" "));
  };
  int curves = 0;
  for (const Sketch3dCurve& curve : model.curves) {
    curves += curve.type == QStringLiteral("reference") ? 0 : 1;
  }
  return QStringLiteral("3D sketch %1 (%2): %3 points, %4 curves, dof %5, determined %6 points %7 curves, "
                        "redundant %8, in plane %9, on sketch plane %10, references %11 (failed %12), failure %13")
      .arg(model.uid, model.name)
      .arg(static_cast<int>(model.points.size()))
      .arg(curves)
      .arg(model.dof)
      .arg(model.determinedPoints())
      .arg(model.determinedCurves())
      .arg(listed(model.redundant), listed(model.inPlaneItems()))
      .arg(model.onSketchPlanePoints())
      .arg(static_cast<int>(model.references.size()))
      .arg(listed(model.failedReferences()),
           model.failureKind.isEmpty() ? QStringLiteral("none")
                                       : QStringLiteral("%1: %2").arg(model.failureKind, model.failureDetail));
}

} // namespace mitcad::sketch
