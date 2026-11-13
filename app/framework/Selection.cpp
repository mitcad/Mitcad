// SPDX-License-Identifier: MIT
#include "Selection.hpp"

#include <algorithm>
#include <cmath>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMap>
#include <QStringList>

#include "../browser/DocumentSnapshot.hpp"

namespace mitcad {

QString selectionOwnerText(const SelectionItem& item, const DocumentSnapshot& snapshot) {
  if (item.kind != SelectKind::Face && item.kind != SelectKind::Edge &&
      item.kind != SelectKind::Vertex && item.kind != SelectKind::Body) {
    return {};
  }
  QStringList lines;
  if (!item.occurrence.isEmpty()) {
    QStringList names;
    const QVector<DocumentSnapshot::Occurrence>* level = &snapshot.occurrences;
    for (const QString& uid : item.occurrence.split(QLatin1Char('/'))) {
      const auto found = std::find_if(level->cbegin(), level->cend(), [&](const auto& occurrence) {
        return occurrence.uid == uid;
      });
      // A level not in the snapshot ends the path: never its uid
      // (mitcad#1741).
      if (found == level->cend() || found->name.isEmpty()) {
        break;
      }
      names << found->name;
      level = &found->children;
    }
    if (!names.isEmpty()) {
      lines << QCoreApplication::translate("Selection", "Component: %1")
                   .arg(names.join(QStringLiteral(" › ")).toHtmlEscaped());
    }
  }
  // A body not in the snapshot (yet) is named by its kind, never by its uid.
  const auto body = snapshot.bodies.constFind(item.owner);
  if (body != snapshot.bodies.cend() && !body->name.isEmpty()) {
    lines << QCoreApplication::translate("Selection", "Body: %1").arg(body->name.toHtmlEscaped());
  } else {
    lines << QCoreApplication::translate("Selection", "Body");
  }
  return lines.join(QStringLiteral("<br>"));
}

QString pickListText(const SelectionItem& item, const DocumentSnapshot& snapshot) {
  QString kind = SelectionItem::kindText(item.kind);
  if (!kind.isEmpty()) {
    kind[0] = kind[0].toUpper();
  }
  QStringList parts;
  // The occurrence that places it, by its names from the root; a level
  // not in the snapshot ends the path (never its uid).
  if (!item.occurrence.isEmpty()) {
    QStringList names;
    const QVector<DocumentSnapshot::Occurrence>* level = &snapshot.occurrences;
    for (const QString& uid : item.occurrence.split(QLatin1Char('/'))) {
      const auto found = std::find_if(level->cbegin(), level->cend(), [&](const auto& occurrence) {
        return occurrence.uid == uid;
      });
      if (found == level->cend() || found->name.isEmpty()) {
        break;
      }
      names << found->name;
      level = &found->children;
    }
    if (!names.isEmpty()) {
      parts << names.join(QStringLiteral(" › "));
    }
  }
  const auto bodyName = [&]() -> QString {
    const auto body = snapshot.bodies.constFind(item.owner);
    return body == snapshot.bodies.cend() ? QString() : body->name;
  };
  const auto featureName = [&]() -> QString {
    const DocumentSnapshot::Feature* feature = snapshot.feature(item.owner);
    return feature == nullptr ? QString() : feature->name;
  };
  const auto add = [&parts](const QString& name) {
    if (!name.isEmpty()) {
      parts << name;
    }
  };
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
    add(bodyName());
    parts << kind;
    break;
  case SelectKind::Body: {
    const QString name = bodyName();
    parts << (name.isEmpty() ? kind : name);
    break;
  }
  case SelectKind::Profile:
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
  case SelectKind::SketchConstraint:
  case SelectKind::SketchDimension:
    add(featureName());
    parts << kind;
    break;
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point: {
    // The origin's planes, axes and point as the Browser names them.
    static const QStringList origin = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                       QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z")};
    if (item.owner == QStringLiteral("origin")) {
      parts << QCoreApplication::translate("Selection", "Origin");
    } else if (origin.contains(item.owner)) {
      parts << QCoreApplication::translate("Selection", "Origin") << item.owner.toUpper();
    } else {
      const QString name = featureName();
      parts << (name.isEmpty() ? kind : name);
    }
    break;
  }
  case SelectKind::Component:
    // An occurrence is named by its path above; else its component.
    if (parts.isEmpty()) {
      const auto component = std::find_if(snapshot.components.cbegin(), snapshot.components.cend(),
                                           [&](const auto& one) { return one.uid == item.owner; });
      parts << (component == snapshot.components.cend() || component->name.isEmpty() ? kind : component->name);
    }
    break;
  case SelectKind::Sketch:
  case SelectKind::Feature: {
    const QString name = featureName();
    parts << (name.isEmpty() ? kind : name);
    break;
  }
  case SelectKind::None:
    parts << kind;
    break;
  }
  return parts.join(QStringLiteral(": "));
}

QString selectionMeasurementText(const QJsonObject& measured) {
  QStringList lines;
  const auto add = [&](const QString& label, const QJsonValue& value, const QString& unit) {
    if (!value.isDouble() || !std::isfinite(value.toDouble())) {
      return;
    }
    lines << QStringLiteral("%1: <b>%2 %3</b>")
                 .arg(label.toHtmlEscaped(), QString::number(value.toDouble(), 'g', 6), unit);
  };
  add(QCoreApplication::translate("Selection", "Length"), measured.value(QStringLiteral("length")),
      QStringLiteral("mm"));
  add(QCoreApplication::translate("Selection", "Area"), measured.value(QStringLiteral("area")),
      QStringLiteral("mm²"));
  // A profile's (mitcad#1741).
  add(QCoreApplication::translate("Selection", "Perimeter"), measured.value(QStringLiteral("perimeter")),
      QStringLiteral("mm"));
  add(QCoreApplication::translate("Selection", "Volume"), measured.value(QStringLiteral("volume")),
      QStringLiteral("mm³"));
  const QJsonObject circle = measured.value(QStringLiteral("circle")).toObject();
  const QJsonValue radius = circle.value(QStringLiteral("radius"));
  if (radius.isDouble()) {
    add(QCoreApplication::translate("Selection", "Radius"), radius, QStringLiteral("mm"));
    add(QCoreApplication::translate("Selection", "Diameter"), radius.toDouble() * 2.0, QStringLiteral("mm"));
  }
  return lines.join(QStringLiteral("<br>"));
}

QJsonValue SelectionItem::reference() const {
  switch (kind) {
  case SelectKind::Face:
    return QJsonObject{{QStringLiteral("body"), owner}, {QStringLiteral("face"), name}};
  case SelectKind::Edge:
    return QJsonObject{{QStringLiteral("body"), owner}, {QStringLiteral("edge"), name}};
  case SelectKind::Vertex:
    return QJsonObject{{QStringLiteral("body"), owner}, {QStringLiteral("vertex"), name}};
  case SelectKind::Body:
    return QJsonObject{{QStringLiteral("body"), owner}};
  case SelectKind::Profile:
    return QJsonObject{{QStringLiteral("sketch"), owner}, {QStringLiteral("region"), name}};
  case SelectKind::SketchCurve:
    return QJsonObject{{QStringLiteral("sketch"), owner}, {QStringLiteral("curve"), name}};
  case SelectKind::SketchPoint:
    return QJsonObject{{QStringLiteral("sketch"), owner}, {QStringLiteral("point"), name}};
  case SelectKind::SketchConstraint:
    return QJsonObject{{QStringLiteral("sketch"), owner}, {QStringLiteral("constraint"), name}};
  case SelectKind::SketchDimension:
    return QJsonObject{{QStringLiteral("sketch"), owner}, {QStringLiteral("dimension"), name}};
  case SelectKind::Sketch:
    return QJsonObject{{QStringLiteral("sketch"), owner}};
  case SelectKind::Plane:
    if (isFixedPlane(*this)) {
      return QJsonDocument::fromJson(name.toUtf8()).object();
    }
    return owner;
  case SelectKind::Axis:
  case SelectKind::Point:
  case SelectKind::Component:
  case SelectKind::Feature:
    return owner;
  case SelectKind::None:
    break;
  }
  return QJsonValue();
}

SelectionItem fixedPlaneItem(const QJsonObject& plane, const QString& occurrence) {
  return {SelectKind::Plane, QStringLiteral("fixed"),
          QString::fromUtf8(QJsonDocument(plane).toJson(QJsonDocument::Compact)), QStringLiteral("plane"),
          occurrence};
}

bool isFixedPlane(const SelectionItem& item) {
  return item.kind == SelectKind::Plane && item.owner == QStringLiteral("fixed") && !item.name.isEmpty();
}

bool isFixedPlaneRef(const QJsonValue& ref) {
  const QJsonObject o = ref.toObject();
  return o.value(QStringLiteral("origin")).toArray().size() == 3 &&
         o.value(QStringLiteral("normal")).toArray().size() == 3;
}

QString SelectionItem::kindName(SelectKind kind, int count) {
  const bool one = count == 1;
  switch (kind) {
  case SelectKind::Face:
    return one ? QStringLiteral("face") : QStringLiteral("faces");
  case SelectKind::Edge:
    return one ? QStringLiteral("edge") : QStringLiteral("edges");
  case SelectKind::Vertex:
    return one ? QStringLiteral("vertex") : QStringLiteral("vertices");
  case SelectKind::Body:
    return one ? QStringLiteral("body") : QStringLiteral("bodies");
  case SelectKind::Profile:
    return one ? QStringLiteral("profile") : QStringLiteral("profiles");
  case SelectKind::SketchCurve:
    return one ? QStringLiteral("sketch curve") : QStringLiteral("sketch curves");
  case SelectKind::SketchPoint:
    return one ? QStringLiteral("sketch point") : QStringLiteral("sketch points");
  case SelectKind::Plane:
    return one ? QStringLiteral("plane") : QStringLiteral("planes");
  case SelectKind::Axis:
    return one ? QStringLiteral("axis") : QStringLiteral("axes");
  case SelectKind::Point:
    return one ? QStringLiteral("point") : QStringLiteral("points");
  case SelectKind::Component:
    return one ? QStringLiteral("component") : QStringLiteral("components");
  case SelectKind::SketchConstraint:
    return one ? QStringLiteral("constraint") : QStringLiteral("constraints");
  case SelectKind::SketchDimension:
    return one ? QStringLiteral("dimension") : QStringLiteral("dimensions");
  case SelectKind::Sketch:
    return one ? QStringLiteral("sketch") : QStringLiteral("sketches");
  case SelectKind::Feature:
    return one ? QStringLiteral("feature") : QStringLiteral("features");
  case SelectKind::None:
    break;
  }
  return QStringLiteral("nothing");
}

namespace {

// A kind's words in the interface language: alone and after a count, each
// in the singular and the plural (the context SelectionKind).
struct KindWords {
  const char* one;
  const char* many;
  const char* countOne;
  const char* countMany;
};

KindWords kindWords(SelectKind kind) {
  switch (kind) {
  case SelectKind::Face:
    return {QT_TRANSLATE_NOOP("SelectionKind", "face"), QT_TRANSLATE_NOOP("SelectionKind", "faces"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 face"), QT_TRANSLATE_NOOP("SelectionKind", "%1 faces")};
  case SelectKind::Edge:
    return {QT_TRANSLATE_NOOP("SelectionKind", "edge"), QT_TRANSLATE_NOOP("SelectionKind", "edges"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 edge"), QT_TRANSLATE_NOOP("SelectionKind", "%1 edges")};
  case SelectKind::Vertex:
    return {QT_TRANSLATE_NOOP("SelectionKind", "vertex"), QT_TRANSLATE_NOOP("SelectionKind", "vertices"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 vertex"), QT_TRANSLATE_NOOP("SelectionKind", "%1 vertices")};
  case SelectKind::Body:
    return {QT_TRANSLATE_NOOP("SelectionKind", "body"), QT_TRANSLATE_NOOP("SelectionKind", "bodies"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 body"), QT_TRANSLATE_NOOP("SelectionKind", "%1 bodies")};
  case SelectKind::Profile:
    return {QT_TRANSLATE_NOOP("SelectionKind", "profile"), QT_TRANSLATE_NOOP("SelectionKind", "profiles"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 profile"), QT_TRANSLATE_NOOP("SelectionKind", "%1 profiles")};
  case SelectKind::SketchCurve:
    return {QT_TRANSLATE_NOOP("SelectionKind", "sketch curve"), QT_TRANSLATE_NOOP("SelectionKind", "sketch curves"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 sketch curve"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 sketch curves")};
  case SelectKind::SketchPoint:
    return {QT_TRANSLATE_NOOP("SelectionKind", "sketch point"), QT_TRANSLATE_NOOP("SelectionKind", "sketch points"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 sketch point"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 sketch points")};
  case SelectKind::Plane:
    return {QT_TRANSLATE_NOOP("SelectionKind", "plane"), QT_TRANSLATE_NOOP("SelectionKind", "planes"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 plane"), QT_TRANSLATE_NOOP("SelectionKind", "%1 planes")};
  case SelectKind::Axis:
    return {QT_TRANSLATE_NOOP("SelectionKind", "axis"), QT_TRANSLATE_NOOP("SelectionKind", "axes"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 axis"), QT_TRANSLATE_NOOP("SelectionKind", "%1 axes")};
  case SelectKind::Point:
    return {QT_TRANSLATE_NOOP("SelectionKind", "point"), QT_TRANSLATE_NOOP("SelectionKind", "points"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 point"), QT_TRANSLATE_NOOP("SelectionKind", "%1 points")};
  case SelectKind::Component:
    return {QT_TRANSLATE_NOOP("SelectionKind", "component"), QT_TRANSLATE_NOOP("SelectionKind", "components"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 component"), QT_TRANSLATE_NOOP("SelectionKind", "%1 components")};
  case SelectKind::SketchConstraint:
    return {QT_TRANSLATE_NOOP("SelectionKind", "constraint"), QT_TRANSLATE_NOOP("SelectionKind", "constraints"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 constraint"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 constraints")};
  case SelectKind::SketchDimension:
    return {QT_TRANSLATE_NOOP("SelectionKind", "dimension"), QT_TRANSLATE_NOOP("SelectionKind", "dimensions"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 dimension"), QT_TRANSLATE_NOOP("SelectionKind", "%1 dimensions")};
  case SelectKind::Sketch:
    return {QT_TRANSLATE_NOOP("SelectionKind", "sketch"), QT_TRANSLATE_NOOP("SelectionKind", "sketches"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 sketch"), QT_TRANSLATE_NOOP("SelectionKind", "%1 sketches")};
  case SelectKind::Feature:
    return {QT_TRANSLATE_NOOP("SelectionKind", "feature"), QT_TRANSLATE_NOOP("SelectionKind", "features"),
            QT_TRANSLATE_NOOP("SelectionKind", "%1 feature"), QT_TRANSLATE_NOOP("SelectionKind", "%1 features")};
  case SelectKind::None:
    break;
  }
  return {nullptr, nullptr, nullptr, nullptr};
}

QString kindTranslated(const char* text) { return QCoreApplication::translate("SelectionKind", text); }

} // namespace

QString SelectionItem::kindText(SelectKind kind, int count) {
  const KindWords words = kindWords(kind);
  if (words.one == nullptr) {
    return QCoreApplication::translate("SelectionKind", "nothing");
  }
  return kindTranslated(count == 1 ? words.one : words.many);
}

QString SelectionItem::countText(SelectKind kind, int count) {
  const KindWords words = kindWords(kind);
  if (words.one == nullptr) {
    return QCoreApplication::translate("SelectionKind", "nothing");
  }
  return kindTranslated(count == 1 ? words.countOne : words.countMany).arg(count);
}

QString SelectionItem::logDescription() const {
  const QString kindText = kindName(kind);
  // Where an occurrence places it (components).
  const QString in = occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + occurrence;
  switch (kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
    return QStringLiteral("%1 %2 of %3").arg(kindText, name, owner) + in;
  case SelectKind::Profile:
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
  case SelectKind::SketchConstraint:
  case SelectKind::SketchDimension:
    return QStringLiteral("%1 %2 of %3").arg(kindText, name, owner) + in;
  default:
    return QStringLiteral("%1 %2").arg(kindText, owner) + in;
  }
}

QString SelectionItem::creatingFeature() const {
  switch (kind) {
  case SelectKind::Face: {
    // "F3:side(c1)" was made by F3.
    const int colon = name.indexOf(QLatin1Char(':'));
    return colon > 0 ? name.left(colon) : QString();
  }
  case SelectKind::Body: {
    const int dot = owner.indexOf(QLatin1Char('.'));
    return dot > 0 ? owner.left(dot) : QString();
  }
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
  case SelectKind::Sketch:
  case SelectKind::Feature:
    return owner.startsWith(QLatin1Char('F')) ? owner : QString();
  default:
    return QString();
  }
}

SelectKind preferredKind(const Selection& items) {
  static const SelectKind order[] = {
      SelectKind::Profile,     SelectKind::Body,  SelectKind::Edge, SelectKind::Face,
      SelectKind::Vertex,      SelectKind::SketchCurve, SelectKind::SketchPoint,
      SelectKind::Plane,       SelectKind::Axis,  SelectKind::Point, SelectKind::Component,
      SelectKind::SketchConstraint, SelectKind::SketchDimension, SelectKind::Sketch,
      SelectKind::Feature};
  for (const SelectKind kind : order) {
    for (const SelectionItem& item : items) {
      if (item.kind == kind) {
        return kind;
      }
    }
  }
  return SelectKind::None;
}

bool sameEntry(const SelectionItem& a, const SelectionItem& b, const ItemResolver& resolve) {
  if (a == b) {
    return true;
  }
  if (a.kind != SelectKind::Profile || b.kind != SelectKind::Profile || a.owner != b.owner) {
    return false;
  }
  if (a.name == b.name) {
    return true; // placed by another occurrence, or by none
  }
  if (!resolve) {
    return false;
  }
  QStringList regions = resolve(a);
  QStringList others = resolve(b);
  regions.sort();
  others.sort();
  return !regions.isEmpty() && regions == others;
}

namespace {

// The regions of `item` that `entry` also means, when `entry` means every
// region `item` does: `item` is `entry`'s region or pieces of it (a split
// region, mitcad#410). Empty otherwise.
QStringList piecesOf(const SelectionItem& entry, const SelectionItem& item, const ItemResolver& resolve) {
  if (!resolve || entry.kind != SelectKind::Profile || item.kind != SelectKind::Profile ||
      entry.owner != item.owner) {
    return {};
  }
  const QStringList pieces = resolve(item);
  if (pieces.isEmpty()) {
    return {};
  }
  const QStringList regions = resolve(entry);
  const bool inside = std::all_of(pieces.cbegin(), pieces.cend(),
                                  [&](const QString& piece) { return regions.contains(piece); });
  return inside ? pieces : QStringList();
}

} // namespace

bool containsEntry(const Selection& items, const SelectionItem& item, const ItemResolver& resolve) {
  return std::any_of(items.cbegin(), items.cend(), [&](const SelectionItem& entry) {
    return sameEntry(entry, item, resolve) || !piecesOf(entry, item, resolve).isEmpty();
  });
}

int removeEntry(Selection& items, const SelectionItem& item, const ItemResolver& resolve) {
  Selection kept;
  int changed = 0;
  for (const SelectionItem& entry : items) {
    if (sameEntry(entry, item, resolve)) {
      ++changed;
      continue;
    }
    const QStringList taken = piecesOf(entry, item, resolve);
    if (taken.isEmpty()) {
      kept.append(entry);
      continue;
    }
    // A piece of a split region clicked off: the other pieces stay
    // selected, each under its own key (the entry's key would mean all of
    // them again), the main one first.
    ++changed;
    for (const QString& region : resolve(entry)) {
      if (!taken.contains(region)) {
        SelectionItem piece = entry;
        piece.name = region;
        piece.at.reset();
        kept.append(piece);
      }
    }
  }
  // A piece kept may already be an entry of its own.
  items = changed > 0 ? uniqueProfiles(kept, resolve) : kept;
  return changed;
}

bool toggleEntry(Selection& items, const SelectionItem& item, const ItemResolver& resolve) {
  if (removeEntry(items, item, resolve) > 0) {
    return false;
  }
  items.append(item);
  return true;
}

Selection uniqueProfiles(const Selection& items, const ItemResolver& resolve) {
  Selection unique;
  for (const SelectionItem& item : items) {
    if (item.kind != SelectKind::Profile || !containsEntry(unique, item, resolve)) {
      unique.append(item);
    }
  }
  return unique;
}

QString summarize(const Selection& items) {
  if (items.isEmpty()) {
    return QStringLiteral("nothing");
  }
  QMap<unsigned, int> counts;
  for (const SelectionItem& item : items) {
    ++counts[static_cast<unsigned>(item.kind)];
  }
  QStringList parts;
  for (auto it = counts.cbegin(); it != counts.cend(); ++it) {
    parts << QStringLiteral("%1 %2").arg(it.value()).arg(
                 SelectionItem::kindName(static_cast<SelectKind>(it.key()), it.value()));
  }
  return parts.join(QStringLiteral(", "));
}

QString summarizeText(const Selection& items) {
  if (items.isEmpty()) {
    return QCoreApplication::translate("SelectionKind", "nothing");
  }
  QMap<unsigned, int> counts;
  for (const SelectionItem& item : items) {
    ++counts[static_cast<unsigned>(item.kind)];
  }
  QStringList parts;
  for (auto it = counts.cbegin(); it != counts.cend(); ++it) {
    parts << SelectionItem::countText(static_cast<SelectKind>(it.key()), it.value());
  }
  return parts.join(QStringLiteral(", "));
}

QString featureCurveGeometry() { return QStringLiteral("curve"); }

SelectionItem featureCurveItem(const QString& feature, const QString& occurrence) {
  SelectionItem item{SelectKind::Feature, feature, QString(), featureCurveGeometry()};
  item.occurrence = occurrence;
  return item;
}

bool isFeatureCurve(const SelectionItem& item) {
  return item.kind == SelectKind::Feature && item.geometry == featureCurveGeometry();
}

namespace {

// The entry of `list` (the sketch answer's entities, constraints,
// dimensions) whose `field` is `id`.
QJsonObject entryWith(const QJsonObject& sketch, const char* list, const char* field, const QString& id) {
  for (const QJsonValue& value : sketch.value(QLatin1String(list)).toArray()) {
    const QJsonObject entry = value.toObject();
    if (entry.value(QLatin1String(field)).toString() == id) {
      return entry;
    }
  }
  return {};
}

double pointDistance(const QJsonArray& a, const QJsonArray& b) {
  if (a.size() < 2 || b.size() < 2) {
    return std::nan("");
  }
  return std::hypot(a.at(0).toDouble() - b.at(0).toDouble(), a.at(1).toDouble() - b.at(1).toDouble());
}

QString selectionText(const char* text) { return QCoreApplication::translate("Selection", text); }

// A sketch curve's type in words (the sketch definition's entity types).
QString curveWord(const QString& type) {
  if (type == QStringLiteral("line")) return selectionText("Line");
  if (type == QStringLiteral("circle")) return selectionText("Circle");
  if (type == QStringLiteral("arc")) return selectionText("Arc");
  if (type == QStringLiteral("ellipse")) return selectionText("Ellipse");
  if (type == QStringLiteral("elliptical_arc")) return selectionText("Elliptical arc");
  if (type == QStringLiteral("spline") || type == QStringLiteral("fitted_spline")) return selectionText("Spline");
  return selectionText("Curve");
}

} // namespace

SketchItemCard sketchItemCard(const SelectionItem& item, const QJsonObject& sketch, const QString& sketchName,
                              const std::function<QString(const QString&)>& constraintName) {
  SketchItemCard card;
  // "<kind> of Sketch3"; without the sketch's name the kind alone.
  const auto where = [&sketchName](const QString& kind) {
    return sketchName.isEmpty() ? kind : selectionText("%1 of %2").arg(kind, sketchName);
  };
  QJsonObject measured;
  switch (item.kind) {
  case SelectKind::Profile: {
    card.title = where(selectionText("Profile"));
    const QJsonObject region = entryWith(sketch, "regions", "key", item.name);
    measured.insert(QStringLiteral("area"), region.value(QStringLiteral("area")));
    measured.insert(QStringLiteral("perimeter"), region.value(QStringLiteral("perimeter")));
    break;
  }
  case SelectKind::SketchCurve: {
    const QJsonObject curve = entryWith(sketch, "entities", "id", item.name);
    const QString type = curve.value(QStringLiteral("type")).toString();
    card.title = where(curve.isEmpty() ? SelectionItem::kindText(item.kind) : curveWord(type));
    if (card.title.at(0).isLower()) {
      card.title[0] = card.title.at(0).toUpper();
    }
    const QJsonObject geometry = curve.value(QStringLiteral("geometry")).toObject();
    if (type == QStringLiteral("line")) {
      measured.insert(QStringLiteral("length"), pointDistance(geometry.value(QStringLiteral("start")).toArray(),
                                                              geometry.value(QStringLiteral("end")).toArray()));
    } else if (type == QStringLiteral("circle") || type == QStringLiteral("arc")) {
      const double radius = geometry.value(QStringLiteral("radius")).toDouble(std::nan(""));
      if (type == QStringLiteral("arc")) {
        double sweep = geometry.value(QStringLiteral("end_angle")).toDouble() -
                       geometry.value(QStringLiteral("start_angle")).toDouble();
        const double turn = 2.0 * std::acos(-1.0);
        sweep = std::fmod(std::fmod(sweep, turn) + turn, turn); // counter-clockwise
        measured.insert(QStringLiteral("length"), radius * sweep);
      }
      measured.insert(QStringLiteral("circle"), QJsonObject{{QStringLiteral("radius"), radius}});
    }
    break;
  }
  case SelectKind::SketchPoint:
    card.title = where(selectionText("Point"));
    break;
  case SelectKind::SketchConstraint: {
    // A fixed entity's lock is "fix:<entity>".
    const bool fix = item.name.startsWith(QStringLiteral("fix:"));
    const QString type = fix ? QStringLiteral("fix")
                             : entryWith(sketch, "constraints", "id", item.name).value(QStringLiteral("type")).toString();
    const QString kind = fix ? selectionText("Fix")
                         : type.isEmpty() || !constraintName ? QString()
                                                             : constraintName(type);
    if (kind.isEmpty()) {
      card.title = sketchName.isEmpty() ? selectionText("Constraint")
                                        : selectionText("Constraint in %1").arg(sketchName);
    } else {
      card.title = sketchName.isEmpty() ? selectionText("%1 constraint").arg(kind)
                                        : selectionText("%1 constraint in %2").arg(kind, sketchName);
    }
    break;
  }
  case SelectKind::SketchDimension: {
    const QJsonObject dimension = entryWith(sketch, "dimensions", "id", item.name);
    // Its parameter's name is the user's (they rename it, formulas use it).
    QString parameter = dimension.value(QStringLiteral("value")).toString();
    if (parameter.isEmpty()) {
      parameter = dimension.value(QStringLiteral("reference")).toString();
    }
    const QString kind = parameter.isEmpty() ? selectionText("Dimension")
                                             : selectionText("Dimension %1").arg(parameter);
    card.title = sketchName.isEmpty() ? kind : selectionText("%1 in %2").arg(kind, sketchName);
    const QJsonValue value = dimension.value(QStringLiteral("measured"));
    if (value.isDouble() && std::isfinite(value.toDouble())) {
      const bool angle = dimension.value(QStringLiteral("type")).toString() == QStringLiteral("angle");
      const double shown = angle ? value.toDouble() * 180.0 / std::acos(-1.0) : value.toDouble();
      card.measurements = selectionText("Value: <b>%1 %2</b>")
                              .arg(QString::number(shown, 'g', 6), angle ? QStringLiteral("°") : QStringLiteral("mm"));
    }
    break;
  }
  default:
    card.title = SelectionItem::kindText(item.kind);
    break;
  }
  if (card.measurements.isEmpty()) {
    card.measurements = selectionMeasurementText(measured);
  }
  return card;
}

} // namespace mitcad
