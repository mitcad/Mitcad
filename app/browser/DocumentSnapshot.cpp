// SPDX-License-Identifier: MIT
#include "DocumentSnapshot.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QRegularExpression>
#include <QSet>
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
    // The condition it is shown under (mitcad#800, mitcad#809).
    occurrence.visibleWhen = text(o, "visible_when");
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

// The parameters the Show When conditions of occurrences use (their
// `dependencies`, mitcad#863), added to `names`.
void conditionParameters(const QJsonArray& occurrences, QSet<QString>& names) {
  for (const QJsonValue& value : occurrences) {
    const QJsonObject o = value.toObject();
    if (o.contains(QStringLiteral("visible_when"))) {
      for (const QJsonValue& name : o.value(QStringLiteral("dependencies")).toArray()) {
        names.insert(name.toString());
      }
    }
    conditionParameters(o.value(QStringLiteral("children")).toArray(), names);
  }
}

// The values of the parameters that Show When conditions of bodies and
// occurrences use (mitcad#863), as an expression writes them: a text
// parameter's text quoted ('No'), any other's value with its unit
// (12.5 mm). Empty, without a query, while nothing has a condition.
QHash<QString, QString> conditionValuesOf(const CommandContext& model, const QJsonArray& bodies,
                                          const QJsonObject& components) {
  QSet<QString> names;
  for (const QJsonValue& value : bodies) {
    const QJsonObject b = value.toObject();
    if (b.contains(QStringLiteral("visible_when"))) {
      for (const QJsonValue& name : b.value(QStringLiteral("dependencies")).toArray()) {
        names.insert(name.toString());
      }
    }
  }
  conditionParameters(components.value(QStringLiteral("occurrences")).toArray(), names);
  QHash<QString, QString> values;
  if (names.isEmpty()) {
    return values;
  }
  try {
    for (const QJsonValue& value : model.queryArray(QStringLiteral("parameters"))) {
      const QJsonObject p = value.toObject();
      const QString name = text(p, "name");
      if (!names.contains(name)) {
        continue;
      }
      QString shown = text(p, "text");
      if (text(p, "unit") == QStringLiteral("Text")) {
        shown = QLatin1Char('\'') + shown.replace(QLatin1Char('\''), QStringLiteral("''")) + QLatin1Char('\'');
      }
      values.insert(name, shown);
    }
  } catch (const std::exception&) {
    // No parameters to show: the tooltips name the condition only.
  }
  return values;
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
  snapshot.analyses = model.queryArray(QStringLiteral("analyses"));
  snapshot.joints = model.queryObject({{QStringLiteral("query"), QStringLiteral("joints")}});
  // Sheet-metal bodies with their rules and Flat Patterns (mitcad#210).
  try {
    snapshot.sheets = model.queryArray(QStringLiteral("sheet_bodies"));
  } catch (const std::exception&) {
  }
  // Motion studies and contact sets (mitcad#454).
  snapshot.motionStudies = model.queryArray(QStringLiteral("motion_studies"));
  snapshot.contactSets = model.queryArray(QStringLiteral("contact_sets"));
  // Measurement objects (mitcad#649), measured at the marker.
  snapshot.measurements = model.queryArray(QStringLiteral("measurements"));
  // Drawings of the root and the components (mitcad#707).
  snapshot.drawings = model.queryArray(QStringLiteral("drawings"));
  // Image references (mitcad#782).
  snapshot.imageReferences = model.queryArray(QStringLiteral("image_references"));
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
                                          snapshot.namedViews, display, snapshot.analyses, snapshot.joints,
                                          snapshot.sheets, snapshot.motionStudies, snapshot.contactSets,
                                          snapshot.measurements, snapshot.drawings, snapshot.imageReferences})
                     .toJson(QJsonDocument::Compact);
  // The values Show When conditions use (mitcad#863): a parameter changed
  // without changing what is shown still changes the light bulbs' tooltips.
  snapshot.conditionValues = conditionValuesOf(model, bodies, components);
  if (!snapshot.conditionValues.isEmpty()) {
    QJsonObject values;
    for (auto it = snapshot.conditionValues.cbegin(); it != snapshot.conditionValues.cend(); ++it) {
      values.insert(it.key(), it.value());
    }
    snapshot.key += QJsonDocument(values).toJson(QJsonDocument::Compact);
  }

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
    feature.importNote = text(f, "import");
    feature.component = f.value(QStringLiteral("component")).toString(kRoot);
    feature.suppressed = f.value(QStringLiteral("suppressed")).toBool();
    feature.dof = f.value(QStringLiteral("dof")).toInt(-1);
    feature.index = i;
    feature.visible = f.value(QStringLiteral("visible")).toBool(true);
    feature.visibleSet = f.value(QStringLiteral("visible_set")).toBool();
    feature.sketches = f.value(QStringLiteral("sketches")).toVariant().toStringList();
    // The condition it is suppressed under (mitcad#800, mitcad#809).
    feature.suppressedWhen = text(f, "suppressed_when");
    // A joint origin's light bulb is construction geometry's (mitcad#55).
    if (feature.isSketch() || feature.isConstruction() || feature.type == QStringLiteral("joint_origin")) {
      snapshot.featureShown.insert(feature.uid, feature.visible);
    }
    snapshot.featureIndex.insert(feature.uid, snapshot.features.size());
    snapshot.features.append(feature);
  }

  snapshot.activeComponent = components.value(QStringLiteral("active")).toString(kRoot);
  for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
    const QJsonObject c = value.toObject();
    Component component;
    component.uid = text(c, "uid");
    component.name = text(c, "name");
    component.linked = c.value(QStringLiteral("link")).isString();
    component.library = c.value(QStringLiteral("library")).isObject();
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
    body.mesh = b.value(QStringLiteral("mesh")).toBool();
    // The print bed face (mitcad#731).
    const QJsonObject bed = b.value(QStringLiteral("print_bed_face")).toObject();
    body.printBedFace = bed.value(QStringLiteral("face")).toString();
    for (const QJsonValue& face : bed.value(QStringLiteral("faces")).toArray()) {
      body.printBedFaces << face.toString();
    }
    body.printBedError = bed.value(QStringLiteral("error")).toString();
    // Bindings (mitcad#800, mitcad#809).
    body.appearanceExpr = text(b, "appearance_expr");
    body.materialExpr = text(b, "material_expr");
    body.visibleWhen = text(b, "visible_when");
    body.faceAppearanceExprs = b.value(QStringLiteral("face_appearance_exprs")).toObject();
    for (const QJsonValue& warning : b.value(QStringLiteral("warnings")).toArray()) {
      body.warnings << warning.toString();
    }
    snapshot.bodies.insert(body.uid, body);
  }
  return snapshot;
}

const DocumentSnapshot::Feature* DocumentSnapshot::feature(const QString& uid) const {
  const auto found = featureIndex.constFind(uid);
  if (found != featureIndex.cend() && *found < features.size() && features.at(*found).uid == uid) {
    return &features.at(*found);
  }
  // Every feature indexed (as read() makes the snapshot): not there.
  if (featureIndex.size() == features.size()) {
    return nullptr;
  }
  // Features added otherwise (a test's snapshot), looked for one by one.
  for (const Feature& feature : features) {
    if (feature.uid == uid) {
      return &feature;
    }
  }
  return nullptr;
}

QStringList DocumentSnapshot::conditionParts(const QString& condition) const {
  QStringList parts;
  if (conditionValues.isEmpty()) {
    return parts;
  }
  // The names outside the condition's text literals ('Yes', 'It''s').
  static const QRegularExpression literal(QStringLiteral("'(?:[^']|'')*'"));
  static const QRegularExpression name(QStringLiteral("[\\p{L}_][\\p{L}\\p{N}_]*"));
  QString names = condition;
  names.replace(literal, QStringLiteral(" "));
  QStringList seen;
  for (auto it = name.globalMatch(names); it.hasNext();) {
    const QString found = it.next().captured(0);
    if (!seen.contains(found) && conditionValues.contains(found)) {
      seen << found;
      parts << QStringLiteral("%1 is %2").arg(found, conditionValues.value(found));
    }
  }
  return parts;
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

QString DocumentSnapshot::occurrenceComponent(const QString& path) const {
  QString reached = kRoot;
  const QVector<Occurrence>* level = &occurrences;
  for (const QString& uid : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    const auto found = std::find_if(level->begin(), level->end(),
                                    [&uid](const Occurrence& occurrence) { return occurrence.uid == uid; });
    if (found == level->end()) {
      return QString();
    }
    reached = found->component;
    level = &found->children;
  }
  return reached;
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

QVector<const DocumentSnapshot::Feature*> DocumentSnapshot::warnings() const {
  QVector<const Feature*> warned;
  for (const Feature& feature : features) {
    if (feature.status != QStringLiteral("error") &&
        (feature.status == QStringLiteral("warning") || !feature.importNote.isEmpty())) {
      warned.append(&feature);
    }
  }
  return warned;
}

QJsonObject DocumentSnapshot::shownAnalysis() const {
  for (const QJsonValue& value : analyses) {
    if (value.toObject().value(QStringLiteral("visible")).toBool()) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::motionStudy(const QString& name) const {
  for (const QJsonValue& value : motionStudies) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::contactSet(const QString& name) const {
  for (const QJsonValue& value : contactSets) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::analysis(const QString& name) const {
  for (const QJsonValue& value : analyses) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::imageReference(const QString& name) const {
  for (const QJsonValue& value : imageReferences) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::measurement(const QString& name) const {
  for (const QJsonValue& value : measurements) {
    if (value.toObject().value(QStringLiteral("name")).toString() == name) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

QJsonArray DocumentSnapshot::drawingsOf(const QString& component) const {
  QJsonArray out;
  for (const QJsonValue& value : drawings) {
    if (value.toObject().value(QStringLiteral("component")).toString() == component) {
      out.append(value);
    }
  }
  return out;
}

QJsonObject DocumentSnapshot::joint(const QString& uid) const {
  for (const QJsonValue& value : joints.value(QStringLiteral("joints")).toArray()) {
    if (value.toObject().value(QStringLiteral("uid")).toString() == uid) {
      return value.toObject();
    }
  }
  // A constraint set (mitcad#148) as a joint of the kind "constraint", with
  // its first row's sides.
  for (const QJsonValue& value : joints.value(QStringLiteral("constraints")).toArray()) {
    QJsonObject set = value.toObject();
    if (set.value(QStringLiteral("uid")).toString() == uid) {
      const QJsonObject row = set.value(QStringLiteral("rows")).toArray().first().toObject();
      set.insert(QStringLiteral("kind"), QStringLiteral("constraint"));
      set.insert(QStringLiteral("a"), row.value(QStringLiteral("a")));
      set.insert(QStringLiteral("b"), row.value(QStringLiteral("b")));
      return set;
    }
  }
  return QJsonObject();
}

QJsonObject DocumentSnapshot::jointDof(const QString& component) const {
  for (const QJsonValue& value : joints.value(QStringLiteral("dof")).toArray()) {
    if (value.toObject().value(QStringLiteral("component")).toString() == component) {
      return value.toObject();
    }
  }
  return QJsonObject();
}

int DocumentSnapshot::occurrenceDof(const QString& uid) const {
  for (const QJsonValue& value : joints.value(QStringLiteral("dof")).toArray()) {
    for (const QJsonValue& unit : value.toObject().value(QStringLiteral("units")).toArray()) {
      if (unit.toObject().value(QStringLiteral("occurrences")).toArray().contains(uid)) {
        return unit.toObject().value(QStringLiteral("dof")).toInt();
      }
    }
  }
  return -1;
}

QJsonObject DocumentSnapshot::sheet(const QString& body) const {
  for (const QJsonValue& value : sheets) {
    if (value.toObject().value(QStringLiteral("body")).toString() == body) {
      return value.toObject();
    }
  }
  return {};
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
      {QStringLiteral("remove_body"), QStringLiteral("delete")},
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
      {QStringLiteral("pattern_at_points"), QStringLiteral("pattern-at-points")},
      {QStringLiteral("mirror"), QStringLiteral("mirror")},
      {QStringLiteral("combine"), QStringLiteral("combine")},
      {QStringLiteral("move"), QStringLiteral("move")},
      {QStringLiteral("align"), QStringLiteral("align")},
      {QStringLiteral("scale"), QStringLiteral("scale")},
      {QStringLiteral("scale_pattern"), QStringLiteral("scale")},
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
      // Joints (mitcad#55).
      {QStringLiteral("joint"), QStringLiteral("joint")},
      {QStringLiteral("as_built_joint"), QStringLiteral("as-built-joint")},
      {QStringLiteral("joint_origin"), QStringLiteral("joint-origin")},
      {QStringLiteral("rigid_group"), QStringLiteral("rigid-group")},
      // Assembly constraints (mitcad#148).
      {QStringLiteral("constraint_set"), QStringLiteral("constraint")},
      // Placeholders of imported items that did not come in (mitcad#170).
      {QStringLiteral("placeholder"), QStringLiteral("feature")},
      // Sheet metal (mitcad#206-mitcad#210).
      {QStringLiteral("sheet_rules"), QStringLiteral("sheet-rules")},
      {QStringLiteral("sheet_face"), QStringLiteral("sheet-face")},
      {QStringLiteral("sheet_flange"), QStringLiteral("sheet-flange")},
      {QStringLiteral("sheet_bend"), QStringLiteral("sheet-bend")},
      {QStringLiteral("sheet_fold"), QStringLiteral("sheet-fold")},
      {QStringLiteral("sheet_corner_round"), QStringLiteral("sheet-corner-round")},
      {QStringLiteral("sheet_corner_chamfer"), QStringLiteral("sheet-corner-chamfer")},
      {QStringLiteral("sheet_unfold"), QStringLiteral("sheet-unfold")},
      {QStringLiteral("sheet_refold"), QStringLiteral("sheet-refold")},
      {QStringLiteral("sheet_cut"), QStringLiteral("sheet-cut")},
      {QStringLiteral("sheet_corner_relief"), QStringLiteral("sheet-corner-relief")},
      // Motion links (mitcad#466).
      {QStringLiteral("motion_link"), QStringLiteral("motion-link")},
      // Emboss (mitcad#458).
      {QStringLiteral("emboss"), QStringLiteral("emboss")},
      // Surface modelling (mitcad#456, mitcad#455).
      {QStringLiteral("patch"), QStringLiteral("patch")},
      {QStringLiteral("stitch"), QStringLiteral("stitch")},
      {QStringLiteral("thicken"), QStringLiteral("thicken")},
      {QStringLiteral("offset_surface"), QStringLiteral("offset-surface")},
      {QStringLiteral("ruled_surface"), QStringLiteral("ruled-surface")},
      {QStringLiteral("extend_surface"), QStringLiteral("surface-extend")},
      {QStringLiteral("trim_surface"), QStringLiteral("surface-trim")},
      {QStringLiteral("untrim_surface"), QStringLiteral("surface-untrim")},
      {QStringLiteral("unstitch"), QStringLiteral("unstitch")},
      {QStringLiteral("reverse_normal"), QStringLiteral("reverse-normal")},
      {QStringLiteral("blend_surface"), QStringLiteral("blend-surface")},
      {QStringLiteral("boundary_surface"), QStringLiteral("boundary-surface")},
      {QStringLiteral("match_surface"), QStringLiteral("match-surface")},
      {QStringLiteral("fair_surface"), QStringLiteral("fair-surface")},
      // Plastic part features (mitcad#459).
      {QStringLiteral("plastic_rule"), QStringLiteral("plastic")},
      {QStringLiteral("boss"), QStringLiteral("plastic")},
      {QStringLiteral("lip"), QStringLiteral("plastic")},
      {QStringLiteral("groove"), QStringLiteral("plastic")},
      {QStringLiteral("snap_fit"), QStringLiteral("plastic")},
      {QStringLiteral("rest"), QStringLiteral("plastic")},
      // Body transfers (mitcad#465).
      {QStringLiteral("body_transfer"), QStringLiteral("component")},
      // Dogbones (mitcad#1011).
      {QStringLiteral("dogbone"), QStringLiteral("dogbone")},
      // Mortise and Tenon (mitcad#1012).
      {QStringLiteral("mortise_tenon"), QStringLiteral("mortise-tenon")},
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
