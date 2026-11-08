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
      if (found == level->cend()) {
        names << uid;
        break;
      }
      names << (found->name.isEmpty() ? found->uid : found->name);
      level = &found->children;
    }
    lines << QCoreApplication::translate("Selection", "Component: %1")
                 .arg(names.join(QStringLiteral(" › ")).toHtmlEscaped());
  }
  const auto body = snapshot.bodies.constFind(item.owner);
  const QString name = body == snapshot.bodies.cend() || body->name.isEmpty() ? item.owner : body->name;
  lines << QCoreApplication::translate("Selection", "Body: %1").arg(name.toHtmlEscaped());
  return lines.join(QStringLiteral("<br>"));
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

QString SelectionItem::describe() const {
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

} // namespace mitcad
