// SPDX-License-Identifier: MIT
#include "DocumentSnapshot.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QtLogging>

#include "../framework/Diagnostics.hpp"

namespace mitcad {
namespace {

const QString kRoot = QStringLiteral("C0");

QString text(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

QVector<DocumentSnapshot::Occurrence> occurrencesOf(const QJsonArray& values) {
  QVector<DocumentSnapshot::Occurrence> occurrences;
  for (const QJsonValue& value : values) {
    const QJsonObject o = value.toObject();
    DocumentSnapshot::Occurrence occurrence;
    occurrence.uid = text(o, "uid");
    occurrence.component = text(o, "component");
    occurrence.grounded = o.value(QStringLiteral("grounded")).toBool();
    occurrence.visible = o.value(QStringLiteral("visible")).toBool(true);
    occurrence.world = o.value(QStringLiteral("world")).toArray();
    occurrence.children = occurrencesOf(o.value(QStringLiteral("children")).toArray());
    occurrence.name = text(o, "name");
    occurrences.append(occurrence);
  }
  return occurrences;
}

// The paths of uids ("O1/O4"); the query's own paths are of names.
void setPaths(QVector<DocumentSnapshot::Occurrence>& occurrences, const QString& parent) {
  for (DocumentSnapshot::Occurrence& occurrence : occurrences) {
    occurrence.path = parent.isEmpty() ? occurrence.uid : parent + QLatin1Char('/') + occurrence.uid;
    setPaths(occurrence.children, occurrence.path);
  }
}

QString findOccurrence(const QVector<DocumentSnapshot::Occurrence>& occurrences,
                       const QString& component) {
  for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
    if (occurrence.component == component) {
      return occurrence.path;
    }
    const QString inside = findOccurrence(occurrence.children, component);
    if (!inside.isEmpty()) {
      return inside;
    }
  }
  return QString();
}

} // namespace

DocumentSnapshot DocumentSnapshot::read(const CommandContext& model) {
  DocumentSnapshot snapshot;
  const QJsonObject timeline = model.queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
  const QJsonObject components =
      model.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
  const QJsonArray bodies = model.queryArray(QStringLiteral("bodies"));
  const QJsonArray profiles = model.queryArray(QStringLiteral("profiles"));
  const QJsonObject document = model.queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
  const QJsonObject units = document.value(QStringLiteral("units")).toObject();
  snapshot.lengthUnit = units.value(QStringLiteral("length")).toString(QStringLiteral("mm"));
  snapshot.namedViews = model.queryArray(QStringLiteral("named_views"));
  const QJsonObject display = document.value(QStringLiteral("display")).toObject();
  snapshot.originShown = display.value(QStringLiteral("origin")).toBool();
  for (const QJsonValue& value : display.value(QStringLiteral("isolated")).toArray()) {
    const QJsonObject isolated = value.toObject();
    SelectionItem item;
    item.occurrence = isolated.value(QStringLiteral("occurrence")).toString();
    if (isolated.contains(QStringLiteral("body"))) {
      item.kind = SelectKind::Body;
      item.owner = isolated.value(QStringLiteral("body")).toString();
    } else {
      item.kind = SelectKind::Component;
    }
    snapshot.isolation.append(item);
  }
  snapshot.key = QJsonDocument(QJsonArray{timeline, components, bodies, profiles, snapshot.lengthUnit,
                                          snapshot.namedViews, display})
                     .toJson(QJsonDocument::Compact);

  snapshot.marker = timeline.value(QStringLiteral("marker")).toInt();
  for (const QJsonValue& value : timeline.value(QStringLiteral("groups")).toArray()) {
    const QJsonObject group = value.toObject();
    snapshot.groups.append({group.value(QStringLiteral("name")).toString(),
                            group.value(QStringLiteral("features")).toVariant().toStringList()});
  }
  const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
  for (int i = 0; i < features.size(); ++i) {
    const QJsonObject f = features[i].toObject();
    Feature feature;
    feature.uid = text(f, "uid");
    feature.name = text(f, "name");
    feature.type = text(f, "type");
    feature.status = text(f, "status");
    feature.error = text(f, "error");
    feature.component = f.value(QStringLiteral("component")).toString(kRoot);
    feature.suppressed = f.value(QStringLiteral("suppressed")).toBool();
    feature.dof = f.value(QStringLiteral("dof")).toInt(-1);
    feature.index = i;
    feature.visible = f.value(QStringLiteral("visible")).toBool(true);
    feature.visibleSet = f.value(QStringLiteral("visible_set")).toBool();
    if (feature.isSketch() || feature.isConstruction()) {
      snapshot.featureShown.insert(feature.uid, feature.visible);
    }
    snapshot.features.append(feature);
  }

  snapshot.activeComponent = components.value(QStringLiteral("active")).toString(kRoot);
  for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
    const QJsonObject c = value.toObject();
    Component component;
    component.uid = text(c, "uid");
    component.name = text(c, "name");
    component.linked = c.value(QStringLiteral("link")).isString();
    for (const QJsonValue& body : c.value(QStringLiteral("bodies")).toArray()) {
      component.bodies << body.toString();
    }
    snapshot.components.append(component);
  }
  snapshot.occurrences = occurrencesOf(components.value(QStringLiteral("occurrences")).toArray());
  setPaths(snapshot.occurrences, QString());

  for (const QJsonValue& value : bodies) {
    const QJsonObject b = value.toObject();
    Body body;
    body.uid = text(b, "uid");
    body.name = text(b, "name");
    body.component = b.value(QStringLiteral("component")).toString(kRoot);
    body.visible = b.value(QStringLiteral("visible")).toBool(true);
    snapshot.bodies.insert(body.uid, body);
  }
  return snapshot;
}

const DocumentSnapshot::Feature* DocumentSnapshot::feature(const QString& uid) const {
  for (const Feature& feature : features) {
    if (feature.uid == uid) {
      return &feature;
    }
  }
  return nullptr;
}

const DocumentSnapshot::Component* DocumentSnapshot::component(const QString& uid) const {
  for (const Component& component : components) {
    if (component.uid == uid) {
      return &component;
    }
  }
  return nullptr;
}

QString DocumentSnapshot::componentName(const QString& uid) const {
  const Component* found = component(uid);
  return found != nullptr ? found->name : uid;
}

QString DocumentSnapshot::firstOccurrence(const QString& component) const {
  return component == kRoot ? QString() : findOccurrence(occurrences, component);
}

bool DocumentSnapshot::isOccurrenceShown(const QString& path) const {
  const QVector<Occurrence>* level = &occurrences;
  for (const QString& uid : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    const Occurrence* found = nullptr;
    for (const Occurrence& occurrence : *level) {
      if (occurrence.uid == uid) {
        found = &occurrence;
      }
    }
    if (found == nullptr || !found->visible) {
      return false;
    }
    level = &found->children;
  }
  return true;
}

QVector<const DocumentSnapshot::Feature*> DocumentSnapshot::failures() const {
  QVector<const Feature*> failed;
  for (const Feature& feature : features) {
    if (feature.status == QStringLiteral("error")) {
      failed.append(&feature);
    }
  }
  return failed;
}

QString featureIcon(const QString& type) {
  static const QHash<QString, QString> icons = {
      {QStringLiteral("sketch"), QStringLiteral("sketch")},
      {QStringLiteral("extrude"), QStringLiteral("extrude")},
      {QStringLiteral("revolve"), QStringLiteral("revolve")},
      {QStringLiteral("fillet"), QStringLiteral("fillet")},
      {QStringLiteral("chamfer"), QStringLiteral("chamfer")},
      {QStringLiteral("hole"), QStringLiteral("hole")},
      {QStringLiteral("thread"), QStringLiteral("thread")},
      {QStringLiteral("shell"), QStringLiteral("shell")},
      {QStringLiteral("draft"), QStringLiteral("draft")},
      {QStringLiteral("offset_face"), QStringLiteral("press-pull")},
      {QStringLiteral("delete_face"), QStringLiteral("delete")},
      {QStringLiteral("replace_face"), QStringLiteral("replace-face")},
      {QStringLiteral("split_body"), QStringLiteral("split")},
      {QStringLiteral("split_face"), QStringLiteral("split-face")},
      {QStringLiteral("construction_plane"), QStringLiteral("offset-plane")},
      {QStringLiteral("construction_axis"), QStringLiteral("construction-axis")},
      {QStringLiteral("construction_point"), QStringLiteral("construction-point")},
      {QStringLiteral("base"), QStringLiteral("base-feature")},
      {QStringLiteral("rectangular_pattern"), QStringLiteral("rectangular-pattern")},
      {QStringLiteral("circular_pattern"), QStringLiteral("circular-pattern")},
      {QStringLiteral("path_pattern"), QStringLiteral("path-pattern")},
      {QStringLiteral("mirror"), QStringLiteral("mirror")},
      {QStringLiteral("combine"), QStringLiteral("combine")},
      {QStringLiteral("move"), QStringLiteral("move")},
      {QStringLiteral("align"), QStringLiteral("align")},
      {QStringLiteral("scale"), QStringLiteral("scale")},
      {QStringLiteral("box"), QStringLiteral("box")},
      {QStringLiteral("cylinder"), QStringLiteral("cylinder")},
      {QStringLiteral("sphere"), QStringLiteral("sphere")},
      {QStringLiteral("torus"), QStringLiteral("torus")},
      {QStringLiteral("sweep"), QStringLiteral("sweep")},
      {QStringLiteral("loft"), QStringLiteral("loft")},
      {QStringLiteral("pipe"), QStringLiteral("pipe")},
      {QStringLiteral("coil"), QStringLiteral("coil")},
      {QStringLiteral("rib"), QStringLiteral("rib")},
      {QStringLiteral("web"), QStringLiteral("web")},
      {QStringLiteral("component_from_bodies"), QStringLiteral("component")},
      {QStringLiteral("move_occurrence"), QStringLiteral("move")},
      {QStringLiteral("capture_position"), QStringLiteral("position")},
      {QStringLiteral("helix"), QStringLiteral("helix")},
  };
  return icons.value(type, QStringLiteral("feature"));
}

QJsonObject bodyVolumes(const CommandContext& model) {
  QJsonObject volumes;
  if (!volumesLogged()) {
    return volumes;
  }
  ScopedTiming timing("volumes");
  try {
    for (const QJsonValue& value : model.query({{QStringLiteral("query"), QStringLiteral("bodies")},
                                                {QStringLiteral("volumes"), true}})
                                       .toArray()) {
      const QJsonObject body = value.toObject();
      volumes.insert(body.value(QStringLiteral("uid")).toString(), body);
    }
  } catch (const std::exception&) {
    // Volumes are only logged.
  }
  return volumes;
}

void logVolumeChanges(const CommandContext& model, const QJsonObject& before) {
  if (!volumesLogged()) {
    return;
  }
  const QJsonObject after = bodyVolumes(model);
  const auto volume = [](const QJsonValue& body) {
    return body.toObject().value(QStringLiteral("volume")).toDouble();
  };
  const auto name = [](const QJsonValue& body) {
    return body.toObject().value(QStringLiteral("name")).toString();
  };
  for (auto it = after.begin(); it != after.end(); ++it) {
    const double now = volume(it.value());
    if (!before.contains(it.key())) {
      qDebug().noquote() << QStringLiteral("New body %1 (%2): volume %3 mm3")
                                .arg(name(it.value()), it.key())
                                .arg(now, 0, 'f', 3);
      continue;
    }
    const double was = volume(before.value(it.key()));
    if (std::abs(now - was) > 1e-9 * std::max(1.0, std::abs(was))) {
      qDebug().noquote() << QStringLiteral("Body %1 (%2): volume %3 -> %4 mm3")
                                .arg(name(it.value()), it.key())
                                .arg(was, 0, 'f', 3)
                                .arg(now, 0, 'f', 3);
    }
  }
  for (auto it = before.begin(); it != before.end(); ++it) {
    if (!after.contains(it.key())) {
      qDebug().noquote() << QStringLiteral("Removed body %1 (%2)").arg(name(it.value()), it.key());
    }
  }
}

} // namespace mitcad
