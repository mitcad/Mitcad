// SPDX-License-Identifier: MIT
// Sketch tools that pick geometry: Trim and Extend (a click on a curve),
// the sketch dimension (the kind follows what is picked and where the value
// is placed) and the geometric constraints.
#include <algorithm>
#include <cmath>
#include <vector>

#include <QObject>
#include <QtLogging>

#include "SketchController.hpp"
#include "SketchOverlay.hpp"
#include "SketchTool.hpp"
#include "ToolSupport.hpp"

namespace mitcad::sketch {
namespace {

using Style = ToolPreview::Style;

bool isPoint(const SketchModel& model, const QString& id) { return model.point(id) != nullptr; }

bool isLine(const SketchModel& model, const QString& id) {
  const CurveData* c = model.curve(id);
  return c != nullptr && c->isLine();
}

// ---------------------------------------------------------------------------
// Trim and Extend

// The piece of a curve between the curves crossing it around `at`.
std::vector<V2> trimPiece(const SketchModel& model, const CurveData& c, const V2& at, double step) {
  std::vector<double> cuts;
  for (const CurveData& other : model.curves) {
    if (other.id == c.id) {
      continue;
    }
    for (const V2& hit : intersections(c.curve, other.curve)) {
      cuts.push_back(c.curve.parameterOf(hit));
    }
    for (const QString& id : {other.start, other.end}) {
      const auto p = model.pointAt(id);
      if (p && c.curve.distanceTo(*p) < 1e-6) {
        cuts.push_back(c.curve.parameterOf(*p));
      }
    }
  }
  for (const PointData& p : model.points) {
    if (p.id != c.start && p.id != c.end && p.id != c.center && c.curve.distanceTo(p.at) < 1e-6) {
      cuts.push_back(c.curve.parameterOf(p.at));
    }
  }
  const double t = c.curve.parameterOf(at);
  double lo = c.curve.t0();
  double hi = c.curve.t1();
  if (c.curve.isClosed()) {
    std::sort(cuts.begin(), cuts.end());
    cuts.erase(std::unique(cuts.begin(), cuts.end(), [](double a, double b) { return std::abs(a - b) < 1e-9; }),
               cuts.end());
    if (cuts.size() >= 2) {
      const auto above = std::upper_bound(cuts.begin(), cuts.end(), t);
      hi = above == cuts.end() ? cuts.front() + kTwoPi : *above;
      lo = above == cuts.begin() ? cuts.back() - kTwoPi : *(above - 1);
    }
  } else {
    for (const double cut : cuts) {
      if (cut <= c.curve.t0() + 1e-9 || cut >= c.curve.t1() - 1e-9) {
        continue;
      }
      if (cut <= t) {
        lo = std::max(lo, cut);
      } else {
        hi = std::min(hi, cut);
      }
    }
  }
  std::vector<V2> piece;
  const double length = (hi - lo) * (c.curve.type == Curve2::Type::Line ? norm(c.curve.b - c.curve.a)
                                                                        : std::max(c.curve.radius, 1.0));
  const int count = c.curve.type == Curve2::Type::Line
                        ? 1
                        : std::clamp(static_cast<int>(std::abs(length) / std::max(step, 1e-6)), 8, 400);
  for (int i = 0; i <= count; ++i) {
    piece.push_back(c.curve.at(lo + (hi - lo) * i / count));
  }
  return piece;
}

class TrimTool : public SketchTool {
public:
  TrimTool(SketchController& c, bool extend) : SketchTool(c), m_extend(extend) {}

  QString name() const override { return m_extend ? QObject::tr("Extend") : QObject::tr("Trim"); }
  bool snaps() const override { return false; }

  void start() override {
    m_c.hint(m_extend ? QObject::tr("Extend: click near the end of a line or an arc to extend it.")
                      : QObject::tr("Trim: click the part of a curve to remove."));
  }

  void move(const Snap& snap) override {
    m_at = snap.at;
    m_curve = m_c.entityAt(snap.screen, PickCurves);
    if (m_extend) {
      m_c.setHover(m_curve);
    }
  }

  void press(const Snap& snap) override {
    const QString curve = m_c.entityAt(snap.screen, PickCurves);
    const CurveData* c = m_c.model().curve(curve);
    if (c == nullptr) {
      return;
    }
    const V2 on = c->curve.nearest(snap.at);
    SketchOp op(m_c);
    QJsonObject cmd = op.command(m_extend ? QStringLiteral("sketch.extend") : QStringLiteral("sketch.trim"));
    cmd.insert(QStringLiteral("curve"), curve);
    cmd.insert(QStringLiteral("at"), xy(on));
    if (!op.run(cmd)) {
      const QString reason = op.error();
      op.rollback();
      m_c.error(QObject::tr("%1: %2").arg(name(), reason));
      return;
    }
    op.commit();
    m_curve.clear();
    m_c.setHover(QString());
    qDebug().noquote() << QStringLiteral("%1 %2 at %3")
                              .arg(m_extend ? QStringLiteral("Extended") : QStringLiteral("Trimmed"), curve,
                                   text(on));
  }

  void paint(ToolPreview& preview) const override {
    const CurveData* c = m_c.model().curve(m_curve);
    if (c == nullptr || m_extend) {
      return;
    }
    preview.add(trimPiece(m_c.model(), *c, m_at, 2.0 * m_c.pixel()), Style::Remove);
  }

private:
  bool m_extend;
  QString m_curve;
  V2 m_at;
};

// ---------------------------------------------------------------------------
// Sketch dimension

class DimensionTool : public SketchTool {
public:
  using SketchTool::SketchTool;

  QString name() const override { return QObject::tr("Sketch Dimension"); }
  bool snaps() const override { return false; }

  void start() override { reset(); }

  void move(const Snap& snap) override {
    m_cursor = snap.at;
    if (m_state == State::Edit) {
      return;
    }
    QString hover = m_c.entityAt(snap.screen);
    if (m_picks.contains(hover) || (m_state == State::Place && !secondPick(hover))) {
      hover.clear();
    }
    m_c.setHover(hover);
  }

  void press(const Snap& snap) override {
    m_cursor = snap.at;
    const QString entity = m_c.entityAt(snap.screen);
    switch (m_state) {
    case State::Pick:
      if (entity.isEmpty()) {
        return;
      }
      m_picks = {entity};
      m_state = isPoint(m_c.model(), entity) ? State::Pick2 : State::Place;
      if (m_state == State::Place && !decide()) {
        // A spline: nothing to measure alone.
        m_state = State::Pick2;
      }
      break;
    case State::Pick2:
      if (entity.isEmpty() || m_picks.contains(entity)) {
        return;
      }
      m_picks << entity;
      if (!decide()) {
        m_c.hint(QObject::tr("Sketch Dimension: these cannot be dimensioned together."));
        reset();
        return;
      }
      m_state = State::Place;
      break;
    case State::Place:
      if (secondPick(entity)) {
        m_picks << entity;
        if (!decide()) {
          m_picks.removeLast();
        }
        break;
      }
      place();
      return;
    case State::Edit:
      return;
    }
    m_c.setPicked(m_picks);
    m_c.setHover(QString());
    prompt();
  }

  bool cancel() override {
    if (m_state == State::Pick) {
      return false;
    }
    reset();
    return true;
  }

  void paint(ToolPreview& preview) const override {
    if (m_state == State::Place || m_state == State::Edit) {
      if (auto d = decide()) {
        d->text = m_state == State::Edit ? m_placed : m_cursor;
        preview.dimension = *d;
      }
    }
  }

private:
  enum class State { Pick, Pick2, Place, Edit };

  void reset() {
    m_state = State::Pick;
    m_picks.clear();
    m_c.setPicked({});
    m_c.setHover(QString());
    prompt();
  }

  void prompt() const {
    switch (m_state) {
    case State::Pick:
      m_c.hint(QObject::tr("Sketch Dimension: pick a line, circle, arc or point."));
      break;
    case State::Pick2:
      m_c.hint(QObject::tr("Sketch Dimension: pick another point or curve."));
      break;
    case State::Place:
      m_c.hint(QObject::tr("Sketch Dimension: click where the value goes (or pick a second entity)."));
      break;
    case State::Edit:
      m_c.hint(QObject::tr("Sketch Dimension: type the value or an expression and press Enter."));
      break;
    }
  }

  // A second entity that makes another kind of dimension with the first.
  bool secondPick(const QString& entity) const {
    if (entity.isEmpty() || m_picks.size() != 1 || m_picks.contains(entity)) {
      return false;
    }
    const SketchModel& model = m_c.model();
    const CurveData* first = model.curve(m_picks.first());
    const CurveData* second = model.curve(entity);
    if (first == nullptr) {
      return true;
    }
    if (first->isLine()) {
      return second == nullptr || second->isLine() || second->isCircular();
    }
    if (first->isCircular()) {
      return second == nullptr || second->isCircular() || second->isLine();
    }
    return false;
  }

  // The dimension the picks and the cursor make (without its value).
  std::optional<DimensionData> decide() const {
    const SketchModel& model = m_c.model();
    DimensionData d;
    const auto set = [&d](const QString& type, std::initializer_list<std::pair<const char*, QString>> fields) {
      d.type = type;
      d.json = QJsonObject{{QStringLiteral("type"), type}};
      for (const auto& [key, value] : fields) {
        d.json.insert(QLatin1String(key), value);
        d.refs << value;
      }
    };
    // Distances between two points: aligned, or along an axis when the
    // value is placed above, below or beside them.
    const auto pointDistance = [&](const QString& a, const QString& b) {
      const V2 pa = *model.pointAt(a);
      const V2 pb = *model.pointAt(b);
      const V2 c = m_cursor;
      const bool insideX = c.x > std::min(pa.x, pb.x) && c.x < std::max(pa.x, pb.x);
      const bool insideY = c.y > std::min(pa.y, pb.y) && c.y < std::max(pa.y, pb.y);
      const bool alongX = std::abs(pa.y - pb.y) < 1e-9;
      const bool alongY = std::abs(pa.x - pb.x) < 1e-9;
      if (insideX && !insideY && !alongX && !alongY) {
        set(QStringLiteral("horizontal_distance"), {{"a", a}, {"b", b}});
      } else if (insideY && !insideX && !alongX && !alongY) {
        set(QStringLiteral("vertical_distance"), {{"a", a}, {"b", b}});
      } else {
        set(QStringLiteral("distance"), {{"a", a}, {"b", b}});
      }
    };
    if (m_picks.size() == 1) {
      const CurveData* c = model.curve(m_picks.first());
      if (c == nullptr) {
        return std::nullopt;
      }
      if (c->isLine()) {
        const V2 a = c->curve.a;
        const V2 b = c->curve.b;
        const V2 cur = m_cursor;
        const bool insideX = cur.x > std::min(a.x, b.x) && cur.x < std::max(a.x, b.x);
        const bool insideY = cur.y > std::min(a.y, b.y) && cur.y < std::max(a.y, b.y);
        const bool slanted = std::abs(a.x - b.x) > 1e-9 && std::abs(a.y - b.y) > 1e-9;
        if (slanted && insideX && !insideY) {
          set(QStringLiteral("horizontal_distance"), {{"a", c->start}, {"b", c->end}});
        } else if (slanted && insideY && !insideX) {
          set(QStringLiteral("vertical_distance"), {{"a", c->start}, {"b", c->end}});
        } else {
          set(QStringLiteral("length"), {{"line", c->id}});
        }
      } else if (c->isCircle()) {
        set(QStringLiteral("diameter"), {{"curve", c->id}});
      } else if (c->isArc()) {
        set(QStringLiteral("radius"), {{"curve", c->id}});
      } else if (c->isEllipse()) {
        const V2 major = polar(c->curve.rotation);
        const bool minor = std::abs(dot(unit(m_cursor - c->curve.center), major)) < std::sqrt(0.5);
        set(minor ? QStringLiteral("minor_radius") : QStringLiteral("major_radius"), {{"ellipse", c->id}});
      } else {
        return std::nullopt;
      }
    } else if (m_picks.size() == 2) {
      QString a = m_picks[0];
      QString b = m_picks[1];
      const CurveData* ca = model.curve(a);
      const CurveData* cb = model.curve(b);
      // Circles count by their centres.
      if (ca != nullptr && ca->isCircular()) {
        a = ca->center;
        ca = nullptr;
      }
      if (cb != nullptr && cb->isCircular()) {
        b = cb->center;
        cb = nullptr;
      }
      if (ca == nullptr && cb == nullptr) {
        if (!model.pointAt(a) || !model.pointAt(b) || a == b) {
          return std::nullopt;
        }
        pointDistance(a, b);
      } else if (ca != nullptr && cb != nullptr) {
        if (!ca->isLine() || !cb->isLine()) {
          return std::nullopt;
        }
        const V2 da = unit(ca->curve.b - ca->curve.a);
        const V2 db = unit(cb->curve.b - cb->curve.a);
        if (std::abs(cross(da, db)) < 1e-9) {
          if (ca->centerline || cb->centerline) {
            const CurveData* axis = ca->centerline ? ca : cb;
            const CurveData* other = ca->centerline ? cb : ca;
            set(QStringLiteral("linear_diameter"), {{"axis", axis->id}, {"entity", other->id}});
          } else {
            set(QStringLiteral("line_distance"), {{"a", ca->id}, {"b", cb->id}});
          }
        } else {
          set(QStringLiteral("angle"), {{"a", ca->id}, {"b", cb->id}});
        }
      } else {
        const CurveData* line = ca != nullptr ? ca : cb;
        const QString point = ca != nullptr ? b : a;
        if (!line->isLine() || !model.pointAt(point)) {
          return std::nullopt;
        }
        if (line->centerline) {
          set(QStringLiteral("linear_diameter"), {{"axis", line->id}, {"entity", point}});
        } else {
          set(QStringLiteral("point_line_distance"), {{"point", point}, {"line", line->id}});
        }
      }
    } else {
      return std::nullopt;
    }
    if (const auto value = measure(d, model)) {
      d.measured = *value;
    } else {
      return std::nullopt;
    }
    return d;
  }

  void place() {
    const auto d = decide();
    if (!d) {
      reset();
      return;
    }
    m_state = State::Edit;
    m_placed = m_cursor;
    m_c.setHover(QString());
    prompt();
    const bool angle = d->type == QStringLiteral("angle");
    const QString value = angle ? QString::number(d->measured * 180.0 / kPi, 'f', 2)
                                : QString::number(d->measured / m_c.lengthScale(), 'f', 2);
    const QJsonObject definition = d->json;
    const V2 at = m_placed;
    m_c.openValueEditor(m_c.toScreen(at), value, angle ? FieldSpec::Kind::Angle : FieldSpec::Kind::Length,
                        [this, definition, at](const QString& expression) { add(definition, at, expression); });
  }

  void add(const QJsonObject& definition, const V2& at, const QString& expression) {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.add_dimension"));
    cmd.insert(QStringLiteral("dimension"), definition);
    cmd.insert(QStringLiteral("value"), expression);
    cmd.insert(QStringLiteral("text"), xy(at));
    QJsonObject result;
    bool driven = false;
    if (!op.run(cmd, &result)) {
      if (!op.error().contains(QStringLiteral("over-constrain"))) {
        const QString reason = op.error();
        op.rollback();
        m_c.error(QObject::tr("Dimension not added: %1").arg(reason));
        reset();
        return;
      }
      // A driven dimension instead of over-constraining.
      cmd.remove(QStringLiteral("value"));
      cmd.insert(QStringLiteral("driven"), true);
      driven = true;
      if (!op.run(cmd, &result)) {
        const QString reason = op.error();
        op.rollback();
        m_c.error(QObject::tr("Dimension not added: %1").arg(reason));
        reset();
        return;
      }
    }
    op.commit();
    const QString id = result.value(QStringLiteral("dimensions")).toArray().first().toString();
    const DimensionData* d = m_c.model().dimension(id);
    if (driven) {
      m_c.hint(QObject::tr("The dimension would over-constrain the sketch: added as a driven dimension."));
      qDebug().noquote() << QStringLiteral("Added driven dimension %1 %2").arg(id, definition.value(QStringLiteral("type")).toString());
    } else {
      qDebug().noquote() << QStringLiteral("Added dimension %1 %2 %3 = %4")
                                .arg(id, definition.value(QStringLiteral("type")).toString(),
                                     d != nullptr ? d->parameter : QString(), expression);
    }
    reset();
  }

  State m_state = State::Pick;
  QStringList m_picks;
  V2 m_cursor;
  V2 m_placed;
};

// ---------------------------------------------------------------------------
// Constraints

int needed(const QString& type, const QStringList& picks, const SketchModel& model) {
  if (type == QStringLiteral("fix")) {
    return 1;
  }
  if (type == QStringLiteral("horizontal_vertical")) {
    return !picks.isEmpty() && isLine(model, picks.first()) ? 1 : 2;
  }
  if (type == QStringLiteral("symmetric")) {
    return 3;
  }
  return 2;
}

// Whether `entity` can be the next pick.
bool acceptable(const QString& type, const QStringList& picks, const QString& entity,
                const SketchModel& model) {
  if (entity.isEmpty() || picks.contains(entity)) {
    return false;
  }
  const bool point = isPoint(model, entity);
  const CurveData* curve = model.curve(entity);
  const int index = static_cast<int>(picks.size());
  const auto pickedPoints = [&] {
    return std::count_if(picks.begin(), picks.end(), [&](const QString& p) { return isPoint(model, p); });
  };
  if (type == QStringLiteral("fix")) {
    return true;
  }
  if (type == QStringLiteral("coincident")) {
    // A point and a point or curve, in either order.
    return index == 0 || point || pickedPoints() > 0;
  }
  if (type == QStringLiteral("horizontal_vertical")) {
    return index == 0 ? point || (curve != nullptr && curve->isLine()) : point;
  }
  if (type == QStringLiteral("parallel") || type == QStringLiteral("perpendicular") ||
      type == QStringLiteral("collinear")) {
    return curve != nullptr && curve->isLine();
  }
  if (type == QStringLiteral("tangent") || type == QStringLiteral("smooth")) {
    return curve != nullptr;
  }
  if (type == QStringLiteral("equal")) {
    if (curve == nullptr || !(curve->isLine() || curve->isCircular())) {
      return false;
    }
    const CurveData* first = index == 0 ? nullptr : model.curve(picks.first());
    return first == nullptr || first->isLine() == curve->isLine();
  }
  if (type == QStringLiteral("concentric")) {
    return curve != nullptr && (curve->isCircular() || curve->isEllipse());
  }
  if (type == QStringLiteral("midpoint")) {
    if (index == 0) {
      return point || (curve != nullptr && (curve->isLine() || curve->isArc()));
    }
    return pickedPoints() > 0 ? curve != nullptr && (curve->isLine() || curve->isArc()) : point;
  }
  if (type == QStringLiteral("symmetric")) {
    if (index == 2) {
      return curve != nullptr && curve->isLine();
    }
    if (index == 1) {
      const CurveData* first = model.curve(picks.first());
      return first == nullptr ? point : curve != nullptr && curve->type == first->type;
    }
    return true;
  }
  return false;
}

// The constraint of complete picks.
QJsonObject build(const QString& type, const QStringList& picks, const SketchModel& model) {
  const auto obj = [](const QString& t, std::initializer_list<std::pair<const char*, QString>> fields) {
    QJsonObject k{{QStringLiteral("type"), t}};
    for (const auto& [key, value] : fields) {
      k.insert(QLatin1String(key), value);
    }
    return k;
  };
  if (type == QStringLiteral("coincident") || type == QStringLiteral("midpoint")) {
    const bool firstPoint = isPoint(model, picks[0]);
    const QString point = firstPoint ? picks[0] : picks[1];
    const QString other = firstPoint ? picks[1] : picks[0];
    return type == QStringLiteral("coincident") ? obj(type, {{"point", point}, {"entity", other}})
                                                : obj(type, {{"point", point}, {"curve", other}});
  }
  if (type == QStringLiteral("horizontal_vertical")) {
    if (picks.size() == 1) {
      const CurveData* c = model.curve(picks[0]);
      const V2 d = c->curve.b - c->curve.a;
      return obj(std::abs(d.x) >= std::abs(d.y) ? QStringLiteral("horizontal") : QStringLiteral("vertical"),
                 {{"line", picks[0]}});
    }
    const V2 d = *model.pointAt(picks[1]) - *model.pointAt(picks[0]);
    return obj(std::abs(d.x) >= std::abs(d.y) ? QStringLiteral("horizontal_points")
                                              : QStringLiteral("vertical_points"),
               {{"a", picks[0]}, {"b", picks[1]}});
  }
  if (type == QStringLiteral("symmetric")) {
    return obj(type, {{"a", picks[0]}, {"b", picks[1]}, {"axis", picks[2]}});
  }
  return obj(type, {{"a", picks[0]}, {"b", picks[1]}});
}

// Adds a constraint for each set of complete picks, as one undo step. A
// refused one is left out, so entities already constrained do not stop
// the others; when all are refused nothing changes.
bool applyEach(SketchController& c, const QString& type, const QVector<QStringList>& sets) {
  SketchOp op(c);
  int added = 0;
  QString reason;
  for (const QStringList& picks : sets) {
    const QJsonObject constraint = build(type, picks, op.model());
    QJsonObject cmd = op.command(QStringLiteral("sketch.add_constraint"));
    cmd.insert(QStringLiteral("constraint"), constraint);
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      reason = op.error();
      qDebug().noquote() << QStringLiteral("Constraint %1 refused: %2")
                                .arg(constraint.value(QStringLiteral("type")).toString(), reason);
      continue;
    }
    ++added;
    qDebug().noquote() << QStringLiteral("Added constraint %1 %2 on %3")
                              .arg(constraint.value(QStringLiteral("type")).toString(),
                                   result.value(QStringLiteral("constraints")).toArray().first().toString(),
                                   picks.join(QStringLiteral(", ")));
  }
  if (added == 0) {
    op.rollback();
    c.error(QObject::tr("%1 not added: %2").arg(constraintName(type), reason));
    return false;
  }
  op.commit();
  if (added < static_cast<int>(sets.size())) {
    c.hint(QObject::tr("%1: %2 of %3 added; the others were refused: %4")
               .arg(constraintName(type))
               .arg(added)
               .arg(static_cast<int>(sets.size()))
               .arg(reason));
  }
  return true;
}

bool apply(SketchController& c, const QString& type, const QStringList& picks) {
  if (type == QStringLiteral("fix")) {
    SketchOp op(c);
    bool fixed = true;
    for (const QString& id : picks) {
      const PointData* p = c.model().point(id);
      const CurveData* curve = c.model().curve(id);
      fixed = fixed && ((p != nullptr && p->fixed) || (curve != nullptr && curve->fixed));
    }
    QJsonObject cmd = op.command(QStringLiteral("sketch.set_fixed"));
    cmd.insert(QStringLiteral("entities"), QJsonArray::fromStringList(picks));
    cmd.insert(QStringLiteral("fixed"), !fixed);
    if (!op.run(cmd)) {
      const QString reason = op.error();
      op.rollback();
      c.error(QObject::tr("Fix/Unfix: %1").arg(reason));
      return false;
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("%1 %2").arg(fixed ? QStringLiteral("Unfixed") : QStringLiteral("Fixed"),
                                                      picks.join(QStringLiteral(", ")));
    return true;
  }
  return applyEach(c, type, {picks});
}

// Constraints that a selection of more than they take applies to the
// first entity with each of the others (a line's horizontal/vertical: to
// each line).
bool pairwise(const QString& type) {
  return type == QStringLiteral("equal") || type == QStringLiteral("parallel") ||
         type == QStringLiteral("perpendicular") || type == QStringLiteral("collinear") ||
         type == QStringLiteral("concentric") || type == QStringLiteral("tangent") ||
         type == QStringLiteral("coincident") || type == QStringLiteral("horizontal_vertical");
}

// The picks of each constraint a preselection makes: the selection itself
// when it is what the constraint takes; for a pairwise constraint, the
// first entity with each of the others, in the order they were selected.
// Empty when the selection does not fit.
QVector<QStringList> selectionSets(const QString& type, const QStringList& picks, const SketchModel& model) {
  QStringList ordered;
  for (const QString& pick : picks) {
    if (!acceptable(type, ordered, pick, model)) {
      break;
    }
    ordered << pick;
  }
  if (ordered.size() == picks.size() &&
      (type == QStringLiteral("fix") || static_cast<int>(ordered.size()) == needed(type, ordered, model))) {
    return {ordered};
  }
  if (!pairwise(type) || picks.size() < 2) {
    return {};
  }
  const QString& first = picks.first();
  QVector<QStringList> sets;
  if (needed(type, {first}, model) == 1) {
    for (const QString& pick : picks) {
      if (!acceptable(type, {}, pick, model) || needed(type, {pick}, model) != 1) {
        return {};
      }
      sets << QStringList{pick};
    }
    return sets;
  }
  for (const QString& pick : picks.mid(1)) {
    if (!acceptable(type, {first}, pick, model)) {
      return {};
    }
    sets << QStringList{first, pick};
  }
  return sets;
}

class ConstraintTool : public SketchTool {
public:
  ConstraintTool(SketchController& c, const QString& type) : SketchTool(c), m_type(type) {}

  QString name() const override { return constraintName(m_type); }
  bool snaps() const override { return false; }
  void start() override { prompt(); }

  void move(const Snap& snap) override {
    const QString entity = m_c.entityAt(snap.screen);
    m_c.setHover(acceptable(m_type, m_picks, entity, m_c.model()) ? entity : QString());
  }

  void press(const Snap& snap) override {
    const QString entity = m_c.entityAt(snap.screen);
    if (!acceptable(m_type, m_picks, entity, m_c.model())) {
      return;
    }
    m_picks << entity;
    if (static_cast<int>(m_picks.size()) >= needed(m_type, m_picks, m_c.model())) {
      apply(m_c, m_type, m_picks);
      m_picks.clear();
    }
    m_c.setPicked(m_picks);
    m_c.setHover(QString());
    prompt();
  }

  bool cancel() override {
    if (m_picks.isEmpty()) {
      return false;
    }
    m_picks.clear();
    m_c.setPicked({});
    prompt();
    return true;
  }

private:
  void prompt() const {
    const QString what = m_type == QStringLiteral("coincident")     ? QObject::tr("a point and a point or curve")
                         : m_type == QStringLiteral("horizontal_vertical") ? QObject::tr("a line or two points")
                         : m_type == QStringLiteral("midpoint")     ? QObject::tr("a point and a line or arc")
                         : m_type == QStringLiteral("symmetric")    ? QObject::tr("two entities and a line")
                         : m_type == QStringLiteral("fix")          ? QObject::tr("an entity")
                         : m_type == QStringLiteral("equal")        ? QObject::tr("two lines, or two circles or arcs")
                         : m_type == QStringLiteral("concentric")   ? QObject::tr("two circles or arcs")
                         : m_type == QStringLiteral("tangent") || m_type == QStringLiteral("smooth")
                             ? QObject::tr("two curves")
                             : QObject::tr("two lines");
    m_c.hint(QObject::tr("%1: pick %2 (%3 picked).").arg(name(), what).arg(m_picks.size()));
  }

  QString m_type;
  QStringList m_picks;
};

} // namespace

QString constraintName(const QString& type) {
  if (type == QStringLiteral("coincident")) {
    return QObject::tr("Coincident");
  }
  if (type == QStringLiteral("horizontal_vertical")) {
    return QObject::tr("Horizontal/Vertical");
  }
  if (type == QStringLiteral("parallel")) {
    return QObject::tr("Parallel");
  }
  if (type == QStringLiteral("perpendicular")) {
    return QObject::tr("Perpendicular");
  }
  if (type == QStringLiteral("tangent")) {
    return QObject::tr("Tangent");
  }
  if (type == QStringLiteral("equal")) {
    return QObject::tr("Equal");
  }
  if (type == QStringLiteral("concentric")) {
    return QObject::tr("Concentric");
  }
  if (type == QStringLiteral("collinear")) {
    return QObject::tr("Collinear");
  }
  if (type == QStringLiteral("midpoint")) {
    return QObject::tr("Midpoint");
  }
  if (type == QStringLiteral("symmetric")) {
    return QObject::tr("Symmetry");
  }
  if (type == QStringLiteral("smooth")) {
    return QObject::tr("Curvature");
  }
  if (type == QStringLiteral("fix")) {
    return QObject::tr("Fix/Unfix");
  }
  return type;
}

bool constrainSelection(SketchController& c, const QString& type) {
  QStringList picks;
  for (const SelectionItem& item : c.host().selection()) {
    if (item.owner == c.uid() &&
        (item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint)) {
      picks << item.name;
    }
  }
  if (picks.isEmpty()) {
    return false;
  }
  const QVector<QStringList> sets = selectionSets(type, picks, c.model());
  if (sets.isEmpty()) {
    return false;
  }
  const bool done = sets.size() == 1 ? apply(c, type, sets.first()) : applyEach(c, type, sets);
  if (done) {
    c.host().setSelection({});
  }
  return true;
}

std::unique_ptr<SketchTool> trimTool(SketchController& c) { return std::make_unique<TrimTool>(c, false); }

std::unique_ptr<SketchTool> extendTool(SketchController& c) { return std::make_unique<TrimTool>(c, true); }

std::unique_ptr<SketchTool> dimensionTool(SketchController& c) { return std::make_unique<DimensionTool>(c); }

std::unique_ptr<SketchTool> constraintTool(SketchController& c, const QString& type) {
  return std::make_unique<ConstraintTool>(c, type);
}

} // namespace mitcad::sketch
