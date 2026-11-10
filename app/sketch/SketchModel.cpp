// SPDX-License-Identifier: MIT
#include "SketchModel.hpp"

#include <algorithm>

#include <QJsonArray>
#include <QSet>

#include "../framework/ModelShapes.hpp"
#include "../framework/UserText.hpp"

namespace mitcad::sketch {
namespace {

// Fields that refer to entities, in the order constraints and dimensions
// list their entities.
const char* const kRefFields[] = {"point", "entity", "line", "curve", "arc",
                                  "ellipse", "a", "b", "axis", "circle"};

V2 v2(const QJsonValue& value) {
  const QJsonArray xy = value.toArray();
  return {xy.at(0).toDouble(), xy.at(1).toDouble()};
}

double number(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toDouble();
}

bool flag(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toBool();
}

QStringList refsOf(const QJsonObject& object) {
  QStringList refs;
  for (const char* field : kRefFields) {
    const QJsonValue value = object.value(QLatin1String(field));
    if (value.isString()) {
      const QString text = value.toString();
      if (text.size() > 1 && (text.startsWith(QLatin1Char('p')) || text.startsWith(QLatin1Char('c')))) {
        refs << text;
      }
    }
  }
  return refs;
}

Curve2 curveOf(const QString& type, const QJsonObject& g) {
  Curve2 c;
  if (type == QStringLiteral("line")) {
    return Curve2::line(v2(g.value(QStringLiteral("start"))), v2(g.value(QStringLiteral("end"))));
  }
  if (type == QStringLiteral("circle")) {
    return Curve2::circle(v2(g.value(QStringLiteral("center"))), number(g, "radius"));
  }
  if (type == QStringLiteral("arc")) {
    return Curve2::arc(v2(g.value(QStringLiteral("center"))), number(g, "radius"),
                       number(g, "start_angle"), number(g, "end_angle"));
  }
  if (type == QStringLiteral("ellipse") || type == QStringLiteral("elliptical_arc")) {
    c.type = type == QStringLiteral("ellipse") ? Curve2::Type::Ellipse : Curve2::Type::EllipticalArc;
    c.center = v2(g.value(QStringLiteral("center")));
    c.radius = number(g, "major_radius");
    c.minor = number(g, "minor_radius");
    c.rotation = number(g, "rotation");
    c.start = number(g, "start_angle");
    c.end = number(g, "end_angle");
    return c;
  }
  // Splines: a NURBS with the full knot vector (clamped uniform when left
  // out), as the model gives it.
  c.type = Curve2::Type::Spline;
  c.degree = g.value(QStringLiteral("degree")).toInt(3);
  for (const QJsonValue& p : g.value(QStringLiteral("control")).toArray()) {
    c.control.push_back(v2(p));
  }
  for (const QJsonValue& w : g.value(QStringLiteral("weights")).toArray()) {
    c.weights.push_back(w.toDouble());
  }
  for (const QJsonValue& k : g.value(QStringLiteral("knots")).toArray()) {
    c.knots.push_back(k.toDouble());
  }
  const int n = static_cast<int>(c.control.size());
  if (c.degree < 1 || n < c.degree + 1) {
    c.degree = std::max(1, n - 1);
  }
  if (static_cast<int>(c.knots.size()) != n + c.degree + 1) {
    c.knots.assign(static_cast<std::size_t>(c.degree + 1), 0.0);
    const int inner = n - c.degree - 1;
    for (int i = 1; i <= inner; ++i) {
      c.knots.push_back(static_cast<double>(i) / (inner + 1));
    }
    c.knots.insert(c.knots.end(), static_cast<std::size_t>(c.degree + 1), 1.0);
  }
  return c;
}

} // namespace

bool SketchModel::load(const QJsonObject& sketch) {
  uid = sketch.value(QStringLiteral("uid")).toString();
  name = sketch.value(QStringLiteral("name")).toString(uid);
  solved = flag(sketch, "solved");
  error = sketch.value(QStringLiteral("error")).toString();
  // In plain words (mitcad#1612): `warnings` keeps the agents' texts.
  warnings = shownTexts(sketch, QStringLiteral("warnings"), QStringLiteral("warning_messages"));
  // What each warning is about (mitcad#264), one list per warning.
  warningEntities.clear();
  const QJsonArray about = sketch.value(QStringLiteral("warning_entities")).toArray();
  for (int i = 0; i < warnings.size(); ++i) {
    QStringList ids;
    for (const QJsonValue& id : about.at(i).toArray()) {
      ids << id.toString();
    }
    warningEntities.push_back(ids);
  }
  staleProjections = false;
  projectAgain = false;
  projections.clear();
  for (const QJsonValue& value : sketch.value(QStringLiteral("projections")).toArray()) {
    const QJsonObject p = value.toObject();
    const bool stale = flag(p, "kept") || flag(p, "lost") ||
                       !p.value(QStringLiteral("unfollowed")).toArray().isEmpty() ||
                       !p.value(QStringLiteral("stale")).toArray().isEmpty();
    staleProjections = staleProjections || stale;
    projectAgain = projectAgain || (stale && !flag(p, "lost"));
    ProjectionData projection;
    const QJsonValue source = p.value(QStringLiteral("source"));
    if (source.isObject()) {
      // Another sketch's curve or point (mitcad#904): "F5/c3", as the
      // model's messages name it; no body item matches it.
      const QJsonObject entity = source.toObject();
      const QString id = entity.contains(QStringLiteral("point")) ? entity.value(QStringLiteral("point")).toString()
                                                                  : entity.value(QStringLiteral("curve")).toString();
      projection.source = entity.value(QStringLiteral("sketch")).toString() + QLatin1Char('/') + id;
    } else {
      projection.source = source.toString();
    }
    projection.body = p.value(QStringLiteral("body")).toString();
    projection.linked = p.value(QStringLiteral("link")).isObject();
    projection.occurrence =
        p.value(QStringLiteral("link")).toObject().value(QStringLiteral("source")).toString();
    for (const QJsonValue& id : p.value(QStringLiteral("entities")).toArray()) {
      projection.entities << id.toString();
    }
    projection.stale = stale;
    projection.section = flag(p, "section");
    projections.push_back(projection);
  }
  dof = sketch.value(QStringLiteral("dof")).toInt();
  points.clear();
  curves.clear();
  constraints.clear();
  dimensions.clear();
  texts.clear();
  conflicts.clear();
  unmetInactive.clear();
  notices = sketch.value(QStringLiteral("notices")).toArray();
  redundant.clear();
  for (const QJsonValue& id : sketch.value(QStringLiteral("redundant")).toArray()) {
    redundant << id.toString();
  }
  for (const QJsonValue& id : sketch.value(QStringLiteral("unmet_inactive")).toArray()) {
    unmetInactive << id.toString();
  }
  const bool framed = sketchFrame(sketch, frame);

  for (const QJsonValue& value : sketch.value(QStringLiteral("entities")).toArray()) {
    const QJsonObject e = value.toObject();
    const QString type = e.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("point")) {
      PointData p;
      p.id = e.value(QStringLiteral("id")).toString();
      p.at = v2(e.value(QStringLiteral("at")));
      p.fixed = flag(e, "fixed");
      p.construction = flag(e, "construction");
      p.reference = flag(e, "reference");
      p.constrained = flag(e, "fully_constrained");
      p.derived = flag(e, "derived");
      p.axis = e.value(QStringLiteral("axis")).toString();
      points.push_back(p);
      continue;
    }
    const QJsonObject geometry = e.value(QStringLiteral("geometry")).toObject();
    if (geometry.isEmpty()) {
      continue;
    }
    CurveData c;
    c.id = e.value(QStringLiteral("id")).toString();
    c.type = type;
    c.curve = curveOf(type, geometry);
    c.start = e.value(QStringLiteral("start")).toString();
    c.end = e.value(QStringLiteral("end")).toString();
    c.center = e.value(QStringLiteral("center")).toString();
    for (const char* field : {"start", "end", "center", "major"}) {
      const QString id = e.value(QLatin1String(field)).toString();
      if (!id.isEmpty()) {
        c.points << id;
      }
    }
    for (const char* field : {"control", "points"}) {
      for (const QJsonValue& id : e.value(QLatin1String(field)).toArray()) {
        c.points << id.toString();
      }
    }
    c.closed = flag(e, "closed");
    if (c.isSpline() && !c.closed && c.start.isEmpty() && !c.points.isEmpty()) {
      c.start = c.points.first();
      c.end = c.points.last();
    }
    c.construction = flag(e, "construction");
    c.centerline = flag(e, "centerline");
    c.fixed = flag(e, "fixed");
    c.reference = flag(e, "reference");
    c.constrained = flag(e, "fully_constrained");
    c.axis = e.value(QStringLiteral("axis")).toString();
    curves.push_back(c);
  }
  // A derived offset's spline (P4): only its ends are points to snap to and
  // pick, its other control points are the offset's.
  QSet<QString> ends;
  for (const CurveData& c : curves) {
    ends << c.start << c.end << c.center;
  }
  points.erase(std::remove_if(points.begin(), points.end(),
                              [&ends](const PointData& p) { return p.derived && !ends.contains(p.id); }),
               points.end());

  for (const QJsonValue& value : sketch.value(QStringLiteral("constraints")).toArray()) {
    const QJsonObject k = value.toObject();
    ConstraintData c;
    c.id = k.value(QStringLiteral("id")).toString();
    c.type = k.value(QStringLiteral("type")).toString();
    c.json = k;
    c.refs = refsOf(k);
    c.active = k.value(QStringLiteral("active")).toBool(true);
    c.inactiveReason = k.value(QStringLiteral("inactive_reason")).toString();
    if (c.type == QStringLiteral("equal_spacing")) {
      // Its anchors in their order; the direction line is not one.
      c.refs.clear();
      for (const QJsonValue& anchor : k.value(QStringLiteral("anchors")).toArray()) {
        c.refs << anchor.toString();
      }
    }
    constraints.push_back(c);
  }

  for (const QJsonValue& value : sketch.value(QStringLiteral("dimensions")).toArray()) {
    const QJsonObject k = value.toObject();
    DimensionData d;
    d.id = k.value(QStringLiteral("id")).toString();
    d.type = k.value(QStringLiteral("type")).toString();
    d.json = k;
    d.refs = refsOf(k);
    d.driven = flag(k, "driven");
    d.active = k.value(QStringLiteral("active")).toBool(true);
    d.inactiveReason = k.value(QStringLiteral("inactive_reason")).toString();
    // A driving dimension's parameter, a driven one's reference (mitcad#231).
    d.parameter = k.value(d.driven ? QStringLiteral("reference") : QStringLiteral("value")).toString();
    d.expression = k.value(QStringLiteral("expression")).toString();
    d.comment = k.value(QStringLiteral("comment")).toString();
    d.limited = k.value(QStringLiteral("limited")).toString();
    d.limitMessage = k.value(QStringLiteral("limit_message")).toString();
    d.measured = number(k, "measured");
    d.signedOffset = flag(k, "signed");
    if (k.value(QStringLiteral("text")).isArray()) {
      d.text = v2(k.value(QStringLiteral("text")));
    }
    dimensions.push_back(d);
  }

  for (const QJsonValue& value : sketch.value(QStringLiteral("texts")).toArray()) {
    const QJsonObject t = value.toObject();
    TextData text;
    text.id = t.value(QStringLiteral("id")).toString();
    text.text = t.value(QStringLiteral("text")).toString();
    text.at = v2(t.value(QStringLiteral("at")));
    text.height = number(t, "height");
    text.angle = number(t, "angle");
    text.font = t.value(QStringLiteral("font")).toString();
    text.bold = flag(t, "bold");
    text.italic = flag(t, "italic");
    text.align = t.value(QStringLiteral("align")).toString(QStringLiteral("left"));
    text.valign = t.value(QStringLiteral("valign")).toString(QStringLiteral("baseline"));
    text.spacing = number(t, "spacing");
    for (const QJsonValue& p : t.value(QStringLiteral("frame")).toArray()) {
      text.frame << p.toString();
    }
    const QJsonObject path = t.value(QStringLiteral("path")).toObject();
    text.path = path.value(QStringLiteral("curve")).toString();
    text.above = path.value(QStringLiteral("above")).toBool(true);
    text.fit = flag(path, "fit");
    text.flipX = flag(t, "flip_x");
    text.flipY = flag(t, "flip_y");
    text.outlined = !t.value(QStringLiteral("outline")).toArray().isEmpty();
    // The contours' curves carry no type: their fields tell it.
    for (const QJsonValue& contour : t.value(QStringLiteral("outline")).toArray()) {
      for (const QJsonValue& part : contour.toObject().value(QStringLiteral("curves")).toArray()) {
        const QJsonObject g = part.toObject();
        const QString kind = g.contains(QStringLiteral("control"))        ? QStringLiteral("spline")
                             : g.contains(QStringLiteral("major_radius")) ? (g.contains(QStringLiteral("start_angle"))
                                                                                 ? QStringLiteral("elliptical_arc")
                                                                                 : QStringLiteral("ellipse"))
                             : g.contains(QStringLiteral("start_angle")) ? QStringLiteral("arc")
                             : g.contains(QStringLiteral("radius"))      ? QStringLiteral("circle")
                                                                         : QStringLiteral("line");
        text.outline.push_back(curveOf(kind, g));
      }
    }
    text.fontFile = t.value(QStringLiteral("font_file")).toObject().value(QStringLiteral("path")).toString();
    const QJsonObject code = t.value(QStringLiteral("code")).toObject();
    text.code = !code.isEmpty();
    text.ecc = code.value(QStringLiteral("ecc")).toString(QStringLiteral("M"));
    const QJsonObject followed = t.value(QStringLiteral("value")).toObject();
    if (!followed.isEmpty()) {
      text.value = followed.value(QStringLiteral("own")).toBool()
                       ? followed.value(QStringLiteral("expression")).toString()
                       : followed.value(QStringLiteral("parameter")).toString();
    }
    texts.push_back(text);
  }
  // The regions a text's contours bound, letters and counters (mitcad#1369).
  regionTexts.clear();
  for (const QJsonValue& value : sketch.value(QStringLiteral("regions")).toArray()) {
    const QJsonObject r = value.toObject();
    const QString text = r.value(QStringLiteral("text")).toString();
    if (!text.isEmpty()) {
      regionTexts.insert(r.value(QStringLiteral("key")).toString(), text);
    }
  }

  patterns.clear();
  for (const QJsonValue& value : sketch.value(QStringLiteral("patterns")).toArray()) {
    const QJsonObject k = value.toObject();
    PatternData p;
    p.id = k.value(QStringLiteral("id")).toString();
    p.type = k.value(QStringLiteral("type")).toString();
    const QJsonValue count = k.value(QStringLiteral("count"));
    if (count.isArray()) {
      p.count = count.toArray().at(0).toInt(1);
      p.count2 = count.toArray().at(1).toInt(1);
    } else {
      p.count = count.toInt(2);
    }
    for (const QJsonValue& v : k.value(QStringLiteral("values")).toArray()) {
      p.values << v.toObject().value(QStringLiteral("expression")).toString();
    }
    for (const QJsonValue& e : k.value(QStringLiteral("entities")).toArray()) {
      p.entities << e.toString();
    }
    for (const QJsonValue& c : k.value(QStringLiteral("copies")).toArray()) {
      const QJsonObject map = c.toObject().value(QStringLiteral("entities")).toObject();
      for (auto it = map.begin(); it != map.end(); ++it) {
        p.copies << it.value().toString();
      }
    }
    patterns.push_back(p);
  }

  offsets.clear();
  for (const QJsonValue& value : sketch.value(QStringLiteral("offsets")).toArray()) {
    const QJsonObject k = value.toObject();
    OffsetData o;
    o.id = k.value(QStringLiteral("id")).toString();
    for (const QJsonValue& c : k.value(QStringLiteral("curves")).toArray()) {
      o.curves << c.toString();
    }
    for (const QJsonValue& c : k.value(QStringLiteral("results")).toArray()) {
      o.made << c.toString();
    }
    for (const QJsonValue& c : k.value(QStringLiteral("corners")).toArray()) {
      o.made << c.toObject().value(QStringLiteral("arc")).toString();
    }
    o.expression = k.value(QStringLiteral("value")).toObject().value(QStringLiteral("expression")).toString();
    o.left = flag(k, "left");
    o.derived = flag(k, "derived");
    offsets.push_back(o);
  }

  for (const QJsonValue& set : sketch.value(QStringLiteral("conflicts")).toArray()) {
    QStringList ids;
    for (const QJsonValue& id : set.toArray()) {
      ids << id.toString();
    }
    conflicts.push_back(ids);
  }
  return framed;
}

const PointData* SketchModel::point(const QString& id) const {
  for (const PointData& p : points) {
    if (p.id == id) {
      return &p;
    }
  }
  return nullptr;
}

const CurveData* SketchModel::curve(const QString& id) const {
  for (const CurveData& c : curves) {
    if (c.id == id) {
      return &c;
    }
  }
  return nullptr;
}

const TextData* SketchModel::text(const QString& id) const {
  for (const TextData& t : texts) {
    if (t.id == id) {
      return &t;
    }
  }
  return nullptr;
}

const PatternData* SketchModel::patternOf(const QString& entity) const {
  for (const PatternData& p : patterns) {
    if (p.entities.contains(entity) || p.copies.contains(entity)) {
      return &p;
    }
  }
  return nullptr;
}

const OffsetData* SketchModel::offsetOf(const QString& curve) const {
  for (const OffsetData& o : offsets) {
    if (o.made.contains(curve)) {
      return &o;
    }
  }
  return nullptr;
}

const ConstraintData* SketchModel::constraint(const QString& id) const {
  for (const ConstraintData& c : constraints) {
    if (c.id == id) {
      return &c;
    }
  }
  return nullptr;
}

const DimensionData* SketchModel::dimension(const QString& id) const {
  for (const DimensionData& d : dimensions) {
    if (d.id == id) {
      return &d;
    }
  }
  return nullptr;
}

std::optional<V2> SketchModel::pointAt(const QString& id) const {
  if (const PointData* p = point(id)) {
    return p->at;
  }
  return std::nullopt;
}

QStringList SketchModel::curvesAt(const QString& point, bool centers) const {
  QStringList found;
  for (const CurveData& c : curves) {
    if (c.start == point || c.end == point || (centers && c.center == point)) {
      found << c.id;
    }
  }
  return found;
}

std::optional<V2> SketchModel::anchorAt(const QString& id) const {
  if (const PointData* p = point(id)) {
    return p->at;
  }
  if (const CurveData* c = curve(id)) {
    if (c->isLine()) {
      return (c->curve.a + c->curve.b) / 2.0;
    }
    if (c->isCircular()) {
      return c->curve.center;
    }
  }
  return std::nullopt;
}

const ConstraintData* SketchModel::equalSpacingOf(const QString& entity) const {
  for (const ConstraintData& c : constraints) {
    if (c.type == QStringLiteral("equal_spacing") && c.refs.contains(entity)) {
      return &c;
    }
  }
  return nullptr;
}

const DimensionData* SketchModel::spacingDimension(const QString& constraint) const {
  for (const DimensionData& d : dimensions) {
    if (d.type == QStringLiteral("spacing") &&
        d.json.value(QStringLiteral("constraint")).toString() == constraint) {
      return &d;
    }
  }
  return nullptr;
}

V2 toSketch(const geometry::Frame& frame, const gp_Pnt& point) {
  const gp_XYZ d = point.XYZ() - frame.origin.XYZ();
  return {d.Dot(frame.x_axis.XYZ()), d.Dot(frame.y_axis.XYZ())};
}

gp_Pnt toModel(const geometry::Frame& frame, const V2& point) {
  return frame.point(gp_Pnt2d(point.x, point.y));
}

Curve2 curveFromJson(const QString& type, const QJsonObject& geometry) {
  return curveOf(type, geometry);
}

} // namespace mitcad::sketch
