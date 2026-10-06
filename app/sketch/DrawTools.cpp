// SPDX-License-Identifier: MIT
// The drawing tools of sketch mode (the sketch CREATE menu): each
// click places a point of the shape, the shape follows the cursor until
// the last one, typed values fix its sizes and become dimensions, and the
// points that snapped onto geometry are constrained there.
#include <algorithm>
#include <cmath>
#include <vector>

#include <QObject>
#include <QtLogging>

#include "SketchController.hpp"
#include "SketchTool.hpp"
#include "ToolSupport.hpp"

namespace mitcad::sketch {

std::optional<Curve2> tangentArc(const V2& start, const V2& direction, const V2& end) {
  const V2 t = unit(direction);
  const V2 n = perp(t);
  const V2 d = end - start;
  const double side = dot(n, d);
  if (std::abs(side) < 1e-9 * std::max(1.0, norm(d))) {
    return std::nullopt;
  }
  const double k = dot(d, d) / (2.0 * side);
  const V2 center = start + n * k;
  const double radius = std::abs(k);
  const double a0 = angleOf(start - center);
  const double a1 = angleOf(end - center);
  // Leaving along t: counter-clockwise when the centre is on the left.
  return k > 0 ? Curve2::arc(center, radius, a0, a1) : Curve2::arc(center, radius, a1, a0);
}

std::optional<Curve2> arcThrough(const V2& a, const V2& m, const V2& b) {
  const auto center = circumcenter(a, m, b);
  if (!center) {
    return std::nullopt;
  }
  const double r = dist(*center, a);
  const double sa = angleOf(a - *center);
  const double sb = angleOf(b - *center);
  const double sm = angleOf(m - *center);
  // Counter-clockwise from a to b when m lies on that way.
  if (normalizeAngle(sm - sa) <= normalizeAngle(sb - sa)) {
    return Curve2::arc(*center, r, sa, sb);
  }
  return Curve2::arc(*center, r, sb, sa);
}

std::optional<V2> leavingDirection(const SketchModel& model, const QString& point, QString* curve) {
  for (const CurveData& c : model.curves) {
    if (!c.curve.isOpen()) {
      continue;
    }
    if (c.end == point) {
      if (curve != nullptr) {
        *curve = c.id;
      }
      return unit(c.curve.tangent(c.curve.t1()));
    }
    if (c.start == point) {
      if (curve != nullptr) {
        *curve = c.id;
      }
      return -unit(c.curve.tangent(c.curve.t0()));
    }
  }
  return std::nullopt;
}

namespace {

using Style = ToolPreview::Style;

double screenDistance(const QPointF& a, const QPointF& b) {
  return std::hypot(a.x() - b.x(), a.y() - b.y());
}

// Points that snapped onto existing points become those points; elsewhere
// a snap only places them.
bool shares(const Snap& snap) { return snap.atPoint() || snap.kind == Snap::Kind::Origin; }

// A snap at a position the tool moved the point to (typed values,
// inference): no longer on what it snapped to.
Snap at(const Snap& snap, const V2& position) {
  if (dist(snap.at, position) <= 1e-9) {
    return snap;
  }
  Snap free;
  free.at = position;
  free.screen = snap.screen;
  return free;
}

void refuse(SketchController& c, SketchOp& op, const QString& what) {
  const QString reason = op.error();
  op.rollback();
  c.error(QObject::tr("%1 not added: %2").arg(what, reason));
}

// ---------------------------------------------------------------------------
// Line

class LineTool : public SketchTool {
public:
  using SketchTool::SketchTool;

  QString name() const override { return QObject::tr("Line"); }

  void start() override { m_c.hint(QObject::tr("Line: click the start point.")); }

  void move(const Snap& snap) override {
    m_snap = snap;
    if (m_state == State::First) {
      return;
    }
    if (m_arcPress && m_state == State::Chain && screenDistance(snap.screen, m_arcPressAt) > 4.0) {
      m_state = State::Arc;
      m_c.clearFields();
      m_c.hint(QObject::tr("Line: release where the tangent arc ends."));
    }
    // The line the cursor passed last: parallel and perpendicular to it.
    if (m_state == State::Chain) {
      const QString passed = m_c.entityAt(snap.screen, PickCurves);
      const CurveData* c = m_c.model().curve(passed);
      if (c != nullptr && c->isLine() && passed != m_previous) {
        m_reference = passed;
      }
    }
    update();
  }

  void press(const Snap& snap) override {
    m_snap = snap;
    if (m_state == State::First) {
      m_startSnap = snap;
      m_start = snap.at;
      m_state = State::Chain;
      m_end = m_start;
      m_c.setFields(fields());
      m_c.hint(QObject::tr("Line: click the end point, or type the length (Tab: angle). "
                           "Drag from the end for a tangent arc."));
      return;
    }
    if (m_state != State::Chain) {
      return;
    }
    if (!m_startPoint.isEmpty() && screenDistance(snap.screen, m_c.toScreen(m_start)) <= 8.0) {
      // On the chain's end: a click ends the chain, a drag draws an arc.
      m_arcPress = true;
      m_arcPressAt = snap.screen;
      return;
    }
    commitLine(m_snap);
  }

  void release(const Snap& snap) override {
    if (!m_arcPress) {
      return;
    }
    m_arcPress = false;
    if (m_state == State::Arc) {
      m_state = State::Chain;
      commitArc(snap);
    } else {
      endChain();
    }
  }

  void doubleClick(const Snap& snap) override {
    Q_UNUSED(snap);
    endChain();
  }

  bool confirm() override {
    if (m_state != State::Chain) {
      return false;
    }
    if (m_c.typed(QStringLiteral("length")) || m_c.typed(QStringLiteral("angle"))) {
      update();
      Snap free;
      free.at = m_end;
      commitLine(free);
    } else {
      endChain();
    }
    return true;
  }

  bool cancel() override {
    if (m_state == State::First) {
      return false;
    }
    endChain();
    return true;
  }

  void fieldsEdited() override { update(); }

  void paint(ToolPreview& preview) const override {
    if (m_state == State::Chain && dist(m_start, m_end) > 0.0) {
      preview.line(m_start, m_end);
      preview.points.push_back(m_end);
      if (m_inference) {
        preview.glyph = m_inference->type;
        preview.glyphAt = (m_start + m_end) / 2.0;
      }
    } else if (m_state == State::Arc) {
      if (const auto arc = tangentArc(m_start, m_prevDir, m_snap.at)) {
        preview.curve(*arc, 2.0 * m_c.pixel());
        preview.glyph = QStringLiteral("tangent");
      }
    }
  }

private:
  enum class State { First, Chain, Arc };
  struct Inference {
    QString type;
    QString other;
    V2 at;
  };

  static QVector<FieldSpec> fields() {
    return {{QStringLiteral("length"), QObject::tr("Length"), FieldSpec::Kind::Length},
            {QStringLiteral("angle"), QObject::tr("Angle"), FieldSpec::Kind::Angle}};
  }

  // The direction of a typed angle: from the sketch x axis for the first
  // line, from the previous line (as an angle dimension measures it) for
  // the next, on the cursor's side.
  V2 typedDirection(double angle, const V2& cursor) const {
    if (m_previous.isEmpty()) {
      return polar(angle);
    }
    const double base = angleOf(m_prevDir);
    const V2 a = polar(base + angle);
    const V2 b = polar(base - angle);
    const V2 want = unit(cursor - m_start);
    return dot(a, want) >= dot(b, want) ? a : b;
  }

  double displayAngle() const {
    const V2 d = m_end - m_start;
    if (norm(d) <= 0.0) {
      return 0.0;
    }
    if (m_previous.isEmpty()) {
      return normalizeAngle(angleOf(d));
    }
    return std::acos(std::clamp(dot(unit(d), m_prevDir), -1.0, 1.0));
  }

  std::optional<Inference> infer(const V2& p) const {
    const V2 d = p - m_start;
    const double px = m_c.pixel();
    if (norm(d) < 4.0 * px) {
      return std::nullopt;
    }
    struct Option {
      QString type;
      QString other;
      V2 dir;
      bool forward;
    };
    std::vector<Option> options = {{QStringLiteral("horizontal"), QString(), {1, 0}, false},
                                   {QStringLiteral("vertical"), QString(), {0, 1}, false}};
    if (!m_previous.isEmpty()) {
      if (m_prevArc) {
        options.push_back({QStringLiteral("tangent"), m_previous, m_prevDir, true});
      } else {
        options.push_back({QStringLiteral("perpendicular"), m_previous, perp(m_prevDir), false});
      }
    }
    const CurveData* reference = m_c.model().curve(m_reference);
    if (reference != nullptr && reference->isLine() && m_reference != m_previous) {
      const V2 r = unit(reference->curve.b - reference->curve.a);
      options.push_back({QStringLiteral("parallel"), m_reference, r, false});
      options.push_back({QStringLiteral("perpendicular"), m_reference, perp(r), false});
    }
    std::optional<Inference> best;
    double bestOffset = 6.0;
    for (const Option& o : options) {
      const double along = dot(d, o.dir);
      if (std::abs(along) < 4.0 * px || (o.forward && along <= 0.0)) {
        continue;
      }
      const V2 projected = m_start + o.dir * along;
      const double offset = dist(projected, p) / px;
      if (offset <= bestOffset) {
        bestOffset = offset - 1e-9; // the first of equal options wins
        best = Inference{o.type, o.other, projected};
      }
    }
    return best;
  }

  void update() {
    if (m_state != State::Chain) {
      return;
    }
    m_inference.reset();
    V2 end = m_snap.at;
    const auto length = m_c.fieldValue(QStringLiteral("length"));
    const auto angle = m_c.fieldValue(QStringLiteral("angle"));
    const bool free = m_snap.kind == Snap::Kind::None || m_snap.kind == Snap::Kind::Grid;
    if (angle) {
      const V2 dir = typedDirection(*angle, end);
      end = m_start + dir * (length ? *length : std::max(0.0, dot(end - m_start, dir)));
    } else {
      if (free || length) {
        if (const auto inference = infer(end)) {
          m_inference = inference;
          end = inference->at;
        }
      } else if (const auto inference = infer(end)) {
        // Onto a point that is already level (or square) with the start:
        // the constraint without moving the point.
        if (dist(inference->at, end) <= 2.0 * m_c.pixel()) {
          m_inference = inference;
          m_inference->at = end;
        }
      }
      if (length) {
        const V2 d = end - m_start;
        end = m_start + (norm(d) > 0.0 ? unit(d) : V2{1.0, 0.0}) * *length;
      }
    }
    m_end = end;
    m_c.setLive(QStringLiteral("length"), dist(m_start, m_end));
    m_c.setLive(QStringLiteral("angle"), displayAngle());
  }

  void commitLine(const Snap& snap) {
    update();
    if (dist(m_start, m_end) <= 1e-9) {
      return;
    }
    const Snap endSnap = at(snap, m_end);
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.add_line"));
    const QJsonValue startInput = m_startPoint.isEmpty() ? op.pointInput(m_startSnap) : QJsonValue(m_startPoint);
    const QJsonValue endInput = op.pointInput(endSnap);
    if (startInput == endInput) {
      op.rollback();
      return;
    }
    cmd.insert(QStringLiteral("start"), startInput);
    cmd.insert(QStringLiteral("end"), endInput);
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const QString line = made(result).value(0);
    QString startPoint;
    QString endPoint;
    if (const CurveData* c = op.model().curve(line)) {
      startPoint = c->start;
      endPoint = c->end;
    }
    if (m_startPoint.isEmpty() && !shares(m_startSnap)) {
      op.constrain(startPoint, m_startSnap);
    }
    if (!shares(endSnap)) {
      op.constrain(endPoint, endSnap);
    }
    QString inferred;
    const auto angle = m_c.fieldValue(QStringLiteral("angle"));
    if (m_inference) {
      inferred = m_inference->type;
    } else if (angle && m_previous.isEmpty()) {
      const double a = normalizeAngle(*angle);
      for (int quarter = 0; quarter <= 4; ++quarter) {
        if (std::abs(a - quarter * kPi / 2.0) < 1e-9) {
          inferred = quarter % 2 == 0 ? QStringLiteral("horizontal") : QStringLiteral("vertical");
        }
      }
    }
    if (inferred == QStringLiteral("horizontal") || inferred == QStringLiteral("vertical")) {
      op.addConstraint({{QStringLiteral("type"), inferred}, {QStringLiteral("line"), line}});
    } else if (!inferred.isEmpty()) {
      op.addConstraint({{QStringLiteral("type"), inferred},
                        {QStringLiteral("a"), m_inference->other},
                        {QStringLiteral("b"), line}});
    }
    if (m_c.typed(QStringLiteral("length"))) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("length")}, {QStringLiteral("line"), line}},
                      m_c.fieldExpression(QStringLiteral("length")));
    }
    if (angle && !m_previous.isEmpty() && !m_prevArc) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("angle")},
                       {QStringLiteral("a"), m_previous},
                       {QStringLiteral("b"), line}},
                      m_c.fieldExpression(QStringLiteral("angle")));
    }
    op.commit();
    const V2 from = m_start;
    V2 to = m_end;
    if (const CurveData* c = m_c.model().curve(line)) {
      to = c->curve.b;
      m_prevDir = unit(c->curve.b - c->curve.a);
    } else {
      m_prevDir = unit(m_end - m_start);
    }
    qDebug().noquote() << QStringLiteral("Added line %1 from %2 to %3%4")
                              .arg(line, text(from), text(to),
                                   inferred.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(inferred));
    m_previous = line;
    m_prevArc = false;
    m_start = to;
    m_startPoint = endPoint;
    if (m_chainStart.isEmpty()) {
      m_chainStart = startPoint;
    }
    m_reference.clear();
    if (endPoint == m_chainStart) {
      qDebug() << "Line chain closed";
      endChain();
      return;
    }
    m_c.setFields(fields());
    m_end = m_start;
  }

  void commitArc(const Snap& snap) {
    const auto preview = tangentArc(m_start, m_prevDir, snap.at);
    if (!preview) {
      return;
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.arc"));
    cmd.insert(QStringLiteral("mode"), QStringLiteral("tangent"));
    cmd.insert(QStringLiteral("from"), m_startPoint);
    cmd.insert(QStringLiteral("end"), op.pointInput(snap));
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, QObject::tr("Tangent arc"));
      m_c.setFields(fields());
      return;
    }
    const QString arc = made(result).value(0);
    QString chainEnd;
    if (const CurveData* c = op.model().curve(arc)) {
      chainEnd = c->start == m_startPoint ? c->end : c->start;
    }
    if (!shares(snap)) {
      op.constrain(chainEnd, snap);
    }
    op.commit();
    if (const auto p = m_c.model().pointAt(chainEnd)) {
      m_start = *p;
    }
    QString curve;
    if (const auto dir = leavingDirection(m_c.model(), chainEnd, &curve)) {
      m_prevDir = *dir;
    }
    qDebug().noquote() << QStringLiteral("Added tangent arc %1 to %2").arg(arc, text(m_start));
    m_previous = arc;
    m_prevArc = true;
    m_startPoint = chainEnd;
    m_end = m_start;
    m_c.setFields(fields());
    m_c.hint(QObject::tr("Line: click the end point; double-click or Esc ends the chain."));
  }

  void endChain() {
    m_state = State::First;
    m_previous.clear();
    m_startPoint.clear();
    m_chainStart.clear();
    m_reference.clear();
    m_arcPress = false;
    m_inference.reset();
    m_c.clearFields();
    m_c.hint(QObject::tr("Line: click the start point."));
  }

  State m_state = State::First;
  Snap m_snap;
  Snap m_startSnap;
  V2 m_start;
  V2 m_end;
  QString m_startPoint; // the point the next line starts from
  QString m_chainStart;
  QString m_previous; // the chain's last curve
  V2 m_prevDir{1.0, 0.0};
  bool m_prevArc = false;
  QString m_reference;
  std::optional<Inference> m_inference;
  bool m_arcPress = false;
  QPointF m_arcPressAt;
};

// ---------------------------------------------------------------------------
// A tool of a few clicks; the subclasses say what each click means.

class StepTool : public SketchTool {
public:
  using SketchTool::SketchTool;

  void start() override {
    m_clicks.clear();
    m_c.hint(prompt(0));
  }

  void move(const Snap& snap) override {
    m_snap = snap;
    if (!m_clicks.empty()) {
      live();
    }
  }

  void press(const Snap& snap) override {
    m_snap = snap;
    if (!m_clicks.empty()) {
      live();
    }
    // Typed values fix where the click lands.
    Snap placed = snap;
    if (!m_clicks.empty()) {
      placed = at(snap, cursor());
    }
    m_clicks.push_back(placed);
    if (static_cast<int>(m_clicks.size()) >= clicks()) {
      finish();
      return;
    }
    m_c.setFields(fields(static_cast<int>(m_clicks.size())));
    m_c.hint(prompt(static_cast<int>(m_clicks.size())));
    live();
  }

  bool confirm() override {
    if (m_clicks.empty()) {
      return false;
    }
    Snap placed;
    placed.at = cursor();
    placed.screen = m_snap.screen;
    press(placed);
    return true;
  }

  bool cancel() override {
    if (m_clicks.empty()) {
      return false;
    }
    m_clicks.pop_back();
    m_c.clearFields();
    if (!m_clicks.empty()) {
      m_c.setFields(fields(static_cast<int>(m_clicks.size())));
    }
    m_c.hint(prompt(static_cast<int>(m_clicks.size())));
    return true;
  }

  void fieldsEdited() override { live(); }

protected:
  // The number of clicks.
  virtual int clicks() const = 0;
  virtual QString prompt(int clicked) const = 0;
  virtual QVector<FieldSpec> fields(int clicked) const {
    Q_UNUSED(clicked);
    return {};
  }
  // The position of the next click: the cursor, moved by typed values.
  virtual V2 cursor() const { return m_snap.at; }
  // Shows the cursor's values in the fields.
  virtual void live() {}
  // The last click: makes the shape.
  virtual void commit() = 0;

  void finish() {
    commit();
    m_clicks.clear();
    m_c.clearFields();
    m_c.hint(prompt(0));
  }

  V2 click(int i) const { return m_clicks[static_cast<std::size_t>(i)].at; }
  const Snap& snapOf(int i) const { return m_clicks[static_cast<std::size_t>(i)]; }
  int clicked() const { return static_cast<int>(m_clicks.size()); }
  double step() const { return 2.0 * m_c.pixel(); }

  std::vector<Snap> m_clicks;
  Snap m_snap;
};

// ---------------------------------------------------------------------------
// Rectangle

class RectangleTool : public StepTool {
public:
  RectangleTool(SketchController& c, const QString& mode) : StepTool(c), m_mode(mode) {}

  QString name() const override {
    if (m_mode == QStringLiteral("three_point")) {
      return QObject::tr("3-Point Rectangle");
    }
    return m_mode == QStringLiteral("center") ? QObject::tr("Center Rectangle")
                                              : QObject::tr("2-Point Rectangle");
  }

  void paint(ToolPreview& preview) const override {
    if (clicked() == 0) {
      return;
    }
    const auto corners = cornersFor(cursor());
    if (corners.empty()) {
      preview.line(click(0), cursor());
      return;
    }
    preview.add({corners.begin(), corners.end()}, Style::Draw, true);
    if (m_mode == QStringLiteral("center")) {
      preview.line(corners[0], corners[2], Style::Construction);
    }
  }

protected:
  int clicks() const override { return m_mode == QStringLiteral("three_point") ? 3 : 2; }

  QString prompt(int clicked) const override {
    if (m_mode == QStringLiteral("center")) {
      return clicked == 0 ? QObject::tr("Center Rectangle: click the centre.")
                          : QObject::tr("Center Rectangle: click a corner, or type the width (Tab: height).");
    }
    if (m_mode == QStringLiteral("three_point")) {
      return clicked == 0   ? QObject::tr("3-Point Rectangle: click the first corner.")
             : clicked == 1 ? QObject::tr("3-Point Rectangle: click the end of the first side.")
                            : QObject::tr("3-Point Rectangle: click the opposite side, or type the height.");
    }
    return clicked == 0 ? QObject::tr("Rectangle: click the first corner.")
                        : QObject::tr("Rectangle: click the opposite corner, or type the width (Tab: height).");
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (m_mode == QStringLiteral("three_point")) {
      if (clicked == 1) {
        return {{QStringLiteral("length"), QObject::tr("Length"), FieldSpec::Kind::Length}};
      }
      return {{QStringLiteral("height"), QObject::tr("Height"), FieldSpec::Kind::Length}};
    }
    return {{QStringLiteral("width"), QObject::tr("Width"), FieldSpec::Kind::Length},
            {QStringLiteral("height"), QObject::tr("Height"), FieldSpec::Kind::Length}};
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (clicked() == 0) {
      return p;
    }
    const V2 a = click(0);
    if (m_mode == QStringLiteral("three_point")) {
      if (clicked() == 1) {
        if (const auto length = m_c.fieldValue(QStringLiteral("length"))) {
          p = a + unit(p - a) * *length;
        }
        return p;
      }
      if (const auto height = m_c.fieldValue(QStringLiteral("height"))) {
        const V2 n = perp(unit(click(1) - a));
        const double side = dot(p - click(1), n) >= 0 ? 1.0 : -1.0;
        p = click(1) + n * (side * *height);
      }
      return p;
    }
    const double scale = m_mode == QStringLiteral("center") ? 0.5 : 1.0;
    if (const auto width = m_c.fieldValue(QStringLiteral("width"))) {
      p.x = a.x + (p.x >= a.x ? 1.0 : -1.0) * *width * scale;
    }
    if (const auto height = m_c.fieldValue(QStringLiteral("height"))) {
      p.y = a.y + (p.y >= a.y ? 1.0 : -1.0) * *height * scale;
    }
    return p;
  }

  void live() override {
    const V2 p = cursor();
    const V2 a = click(0);
    if (m_mode == QStringLiteral("three_point")) {
      if (clicked() == 1) {
        m_c.setLive(QStringLiteral("length"), dist(a, p));
      } else {
        m_c.setLive(QStringLiteral("height"), std::abs(dot(p - click(1), perp(unit(click(1) - a)))));
      }
      return;
    }
    const double scale = m_mode == QStringLiteral("center") ? 2.0 : 1.0;
    m_c.setLive(QStringLiteral("width"), std::abs(p.x - a.x) * scale);
    m_c.setLive(QStringLiteral("height"), std::abs(p.y - a.y) * scale);
  }

  std::vector<V2> cornersFor(const V2& p) const {
    const V2 a = click(0);
    if (m_mode == QStringLiteral("three_point")) {
      if (clicked() < 2) {
        return {};
      }
      const V2 b = click(1);
      const V2 n = perp(unit(b - a));
      const V2 h = n * dot(p - b, n);
      return {a, b, b + h, a + h};
    }
    if (m_mode == QStringLiteral("center")) {
      const V2 d{std::abs(p.x - a.x), std::abs(p.y - a.y)};
      return {{a.x - d.x, a.y - d.y}, {a.x + d.x, a.y - d.y}, {a.x + d.x, a.y + d.y}, {a.x - d.x, a.y + d.y}};
    }
    return {{std::min(a.x, p.x), std::min(a.y, p.y)},
            {std::max(a.x, p.x), std::min(a.y, p.y)},
            {std::max(a.x, p.x), std::max(a.y, p.y)},
            {std::min(a.x, p.x), std::max(a.y, p.y)}};
  }

  void commit() override {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.rectangle"));
    cmd.insert(QStringLiteral("mode"), m_mode);
    const int last = clicked() - 1;
    if (m_mode == QStringLiteral("center")) {
      cmd.insert(QStringLiteral("center"), xy(click(0)));
      cmd.insert(QStringLiteral("corner"), xy(click(1)));
    } else {
      cmd.insert(QStringLiteral("a"), xy(click(0)));
      cmd.insert(QStringLiteral("b"), xy(click(1)));
      if (m_mode == QStringLiteral("three_point")) {
        cmd.insert(QStringLiteral("c"), xy(click(2)));
      }
    }
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const QStringList lines = made(result);
    const SketchModel& model = op.model();
    // Each clicked corner (or centre) is constrained where it snapped.
    std::vector<std::pair<QString, Snap>> constrained;
    for (int i = 0; i <= last; ++i) {
      const Snap& s = snapOf(i);
      if (!s.onGeometry() || (m_mode == QStringLiteral("three_point") && i == 2)) {
        continue;
      }
      constrained.emplace_back(pointNear(model, result, s.at), s);
    }
    // Width and height on the first two sides (from the first corner).
    QString widthLine;
    QString heightLine;
    if (lines.size() == 4) {
      if (m_mode == QStringLiteral("three_point")) {
        for (const QString& id : lines) {
          const CurveData* c = model.curve(id);
          if (c == nullptr) {
            continue;
          }
          const bool first = (dist(c->curve.a, click(0)) < 1e-6 && dist(c->curve.b, click(1)) < 1e-6) ||
                             (dist(c->curve.b, click(0)) < 1e-6 && dist(c->curve.a, click(1)) < 1e-6);
          const bool fromB = !first && (dist(c->curve.a, click(1)) < 1e-6 || dist(c->curve.b, click(1)) < 1e-6);
          if (first) {
            widthLine = id;
          } else if (fromB) {
            heightLine = id;
          }
        }
      } else {
        widthLine = lines[0];
        heightLine = lines[1];
      }
    }
    for (const auto& [point, s] : constrained) {
      op.constrain(point, s);
    }
    const QString widthField =
        m_mode == QStringLiteral("three_point") ? QStringLiteral("length") : QStringLiteral("width");
    if (m_c.typed(widthField) && !widthLine.isEmpty()) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("length")}, {QStringLiteral("line"), widthLine}},
                      m_c.fieldExpression(widthField));
    }
    if (m_c.typed(QStringLiteral("height")) && !heightLine.isEmpty()) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("length")}, {QStringLiteral("line"), heightLine}},
                      m_c.fieldExpression(QStringLiteral("height")));
    }
    op.commit();
    // The size from the made lines.
    double width = 0.0;
    double height = 0.0;
    V2 corner;
    if (lines.size() == 4) {
      const CurveData* first = m_c.model().curve(lines[0]);
      const CurveData* second = m_c.model().curve(lines[1]);
      if (first != nullptr && second != nullptr) {
        width = dist(first->curve.a, first->curve.b);
        height = dist(second->curve.a, second->curve.b);
        corner = first->curve.a;
      }
    }
    qDebug().noquote() << QStringLiteral("Added rectangle %1 x %2 mm at (%3, %4) [%5]")
                              .arg(width)
                              .arg(height)
                              .arg(corner.x)
                              .arg(corner.y)
                              .arg(lines.join(QStringLiteral(", ")));
  }

private:
  QString m_mode;
};

// ---------------------------------------------------------------------------
// Circle

class CircleTool : public StepTool {
public:
  CircleTool(SketchController& c, const QString& mode) : StepTool(c), m_mode(mode) {}

  QString name() const override {
    if (m_mode == QStringLiteral("two_point")) {
      return QObject::tr("2-Point Circle");
    }
    if (m_mode == QStringLiteral("three_point")) {
      return QObject::tr("3-Point Circle");
    }
    if (m_mode == QStringLiteral("two_tangent")) {
      return QObject::tr("2-Tangent Circle");
    }
    if (m_mode == QStringLiteral("three_tangent")) {
      return QObject::tr("3-Tangent Circle");
    }
    return QObject::tr("Center Diameter Circle");
  }

  bool snaps() const override { return !tangent() || m_lines.size() >= 2; }

  void move(const Snap& snap) override {
    if (tangent() && pickingLines()) {
      m_snap = snap;
      const QString line = lineAt(snap);
      m_c.setHover(line);
      return;
    }
    StepTool::move(snap);
  }

  void press(const Snap& snap) override {
    if (tangent() && pickingLines()) {
      const QString line = lineAt(snap);
      if (line.isEmpty() || m_lines.contains(line)) {
        return;
      }
      m_lines << line;
      m_c.setPicked(m_lines);
      m_c.setHover(QString());
      if (m_mode == QStringLiteral("three_tangent") && m_lines.size() == 3) {
        commitTangent3();
        return;
      }
      m_c.hint(prompt(0));
      if (!pickingLines()) {
        m_c.setFields({{QStringLiteral("diameter"), QObject::tr("Diameter"), FieldSpec::Kind::Length}});
      }
      return;
    }
    StepTool::press(snap);
  }

  bool cancel() override {
    if (tangent() && !m_lines.isEmpty()) {
      m_lines.removeLast();
      m_c.setPicked(m_lines);
      m_c.clearFields();
      m_c.hint(prompt(0));
      return true;
    }
    return StepTool::cancel();
  }

  void paint(ToolPreview& preview) const override {
    if (tangent()) {
      if (!pickingLines()) {
        if (const auto c = tangentCircle(m_snap.at)) {
          preview.curve(Curve2::circle(c->first, c->second), step());
        }
      }
      return;
    }
    if (clicked() == 0) {
      return;
    }
    if (const auto c = circleFor(cursor())) {
      preview.curve(*c, step());
    } else {
      preview.line(click(clicked() - 1), cursor(), Style::Guide);
    }
    if (m_mode == QStringLiteral("center")) {
      preview.line(click(0), cursor(), Style::Guide);
    }
  }

protected:
  bool tangent() const {
    return m_mode == QStringLiteral("two_tangent") || m_mode == QStringLiteral("three_tangent");
  }
  bool pickingLines() const {
    return m_lines.size() < (m_mode == QStringLiteral("three_tangent") ? 3 : 2);
  }

  QString lineAt(const Snap& snap) const {
    const QString id = m_c.entityAt(snap.screen, PickCurves);
    const CurveData* c = m_c.model().curve(id);
    return c != nullptr && c->isLine() ? id : QString();
  }

  int clicks() const override {
    if (m_mode == QStringLiteral("three_point")) {
      return 3;
    }
    return tangent() ? 1 : 2;
  }

  QString prompt(int clicked) const override {
    if (tangent()) {
      return pickingLines() ? QObject::tr("%1: pick line %2.").arg(name()).arg(m_lines.size() + 1)
                            : QObject::tr("%1: click near where the circle goes, or type its diameter.").arg(name());
    }
    if (m_mode == QStringLiteral("center")) {
      return clicked == 0 ? QObject::tr("Circle: click the centre.")
                          : QObject::tr("Circle: click a point on the circle, or type the diameter.");
    }
    if (m_mode == QStringLiteral("two_point")) {
      return clicked == 0 ? QObject::tr("2-Point Circle: click one end of a diameter.")
                          : QObject::tr("2-Point Circle: click the other end, or type the diameter.");
    }
    return QObject::tr("3-Point Circle: click point %1.").arg(clicked + 1);
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (m_mode == QStringLiteral("three_point") || clicked == 0) {
      return {};
    }
    return {{QStringLiteral("diameter"), QObject::tr("Diameter"), FieldSpec::Kind::Length}};
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (clicked() == 1 && m_mode != QStringLiteral("three_point")) {
      if (const auto d = m_c.fieldValue(QStringLiteral("diameter"))) {
        const double r = m_mode == QStringLiteral("center") ? *d / 2.0 : *d;
        p = click(0) + unit(p - click(0)) * r;
      }
    }
    return p;
  }

  void live() override {
    if (clicked() == 1 && m_mode != QStringLiteral("three_point")) {
      const double d = dist(click(0), cursor());
      m_c.setLive(QStringLiteral("diameter"), m_mode == QStringLiteral("center") ? 2.0 * d : d);
    }
    if (tangent() && !pickingLines()) {
      if (const auto c = tangentCircle(m_snap.at)) {
        m_c.setLive(QStringLiteral("diameter"), 2.0 * c->second);
      }
    }
  }

  std::optional<Curve2> circleFor(const V2& p) const {
    if (m_mode == QStringLiteral("center")) {
      return dist(click(0), p) > 0.0 ? std::optional<Curve2>(Curve2::circle(click(0), dist(click(0), p)))
                                     : std::nullopt;
    }
    if (m_mode == QStringLiteral("two_point")) {
      return dist(click(0), p) > 0.0
                 ? std::optional<Curve2>(Curve2::circle((click(0) + p) / 2.0, dist(click(0), p) / 2.0))
                 : std::nullopt;
    }
    if (clicked() < 2) {
      return std::nullopt;
    }
    const auto center = circumcenter(click(0), click(1), p);
    return center ? std::optional<Curve2>(Curve2::circle(*center, dist(*center, p))) : std::nullopt;
  }

  // The circle tangent to the two lines whose centre is nearest the cursor.
  std::optional<std::pair<V2, double>> tangentCircle(const V2& p) const {
    if (m_lines.size() < 2) {
      return std::nullopt;
    }
    const CurveData* a = m_c.model().curve(m_lines[0]);
    const CurveData* b = m_c.model().curve(m_lines[1]);
    if (a == nullptr || b == nullptr) {
      return std::nullopt;
    }
    const V2 da = unit(a->curve.b - a->curve.a);
    const V2 db = unit(b->curve.b - b->curve.a);
    const double c = cross(da, db);
    const auto signedDistance = [](const Curve2& line, const V2& dir, const V2& q) {
      return cross(dir, q - line.a);
    };
    if (std::abs(c) < 1e-9) {
      // Parallel: the radius is half their distance, the centre midway.
      const double half = signedDistance(b->curve, db, a->curve.a) / 2.0;
      const V2 foot = a->curve.a + da * dot(p - a->curve.a, da);
      return std::make_pair(foot - perp(db) * half, std::abs(half));
    }
    if (const auto given = m_c.fieldValue(QStringLiteral("diameter"))) {
      // The centre on the bisector of the cursor's quadrant at that radius.
      const double r = *given / 2.0;
      const double sa = signedDistance(a->curve, da, p) >= 0 ? 1.0 : -1.0;
      const double sb = signedDistance(b->curve, db, p) >= 0 ? 1.0 : -1.0;
      // Solve cross(da, x - a0) = sa r, cross(db, x - b0) = sb r.
      const double ra = sa * r + cross(da, -a->curve.a) * -1.0;
      const double rb = sb * r + cross(db, -b->curve.a) * -1.0;
      // cross(d, x) = d.x * x.y - d.y * x.x
      const double det = da.x * (-db.y) - (-da.y) * db.x;
      if (std::abs(det) < 1e-12) {
        return std::nullopt;
      }
      // [ -da.y  da.x ] [x]   [ra]
      // [ -db.y  db.x ] [y] = [rb]
      const double x = (ra * db.x - da.x * rb) / (-da.y * db.x + db.y * da.x);
      const double y = (-da.y * rb + db.y * ra) / (-da.y * db.x + db.y * da.x);
      return std::make_pair(V2{x, y}, r);
    }
    const double t = cross(b->curve.a - a->curve.a, db) / c;
    const V2 corner = a->curve.a + da * t;
    // The bisector of the quadrant the cursor is in.
    const V2 ua = dot(p - corner, da) >= 0 ? da : -da;
    const V2 ub = dot(p - corner, db) >= 0 ? db : -db;
    const V2 bisector = unit(ua + ub);
    const double along = std::max(dot(p - corner, bisector), 1e-6);
    const V2 center = corner + bisector * along;
    return std::make_pair(center, std::abs(cross(da, center - a->curve.a)));
  }

  void commitTangent3() {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.circle"));
    cmd.insert(QStringLiteral("mode"), m_mode);
    cmd.insert(QStringLiteral("lines"), QJsonArray::fromStringList(m_lines));
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
    } else {
      op.commit();
      logCircle(made(result).value(0));
    }
    m_lines.clear();
    m_c.setPicked({});
    m_c.clearFields();
    m_c.hint(prompt(0));
  }

  void commit() override {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.circle"));
    cmd.insert(QStringLiteral("mode"), m_mode);
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    if (tangent()) {
      const auto c = tangentCircle(click(0));
      if (!c) {
        op.rollback();
        return;
      }
      cmd.insert(QStringLiteral("lines"), QJsonArray::fromStringList(m_lines));
      cmd.insert(QStringLiteral("radius"), c->second);
      cmd.insert(QStringLiteral("near"), xy(c->first));
    } else if (m_mode == QStringLiteral("center")) {
      cmd.insert(QStringLiteral("center"), op.pointInput(snapOf(0)));
      cmd.insert(QStringLiteral("radius"), dist(click(0), click(1)));
    } else {
      cmd.insert(QStringLiteral("a"), xy(click(0)));
      cmd.insert(QStringLiteral("b"), xy(click(1)));
      if (m_mode == QStringLiteral("three_point")) {
        cmd.insert(QStringLiteral("c"), xy(click(2)));
      }
    }
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      m_lines.clear();
      m_c.setPicked({});
      return;
    }
    const QString circle = made(result).value(0);
    QString center;
    if (const CurveData* c = op.model().curve(circle)) {
      center = c->center;
    }
    if (m_mode == QStringLiteral("center") && !shares(snapOf(0))) {
      op.constrain(center, snapOf(0));
    }
    // Clicks on the circle that snapped to points keep it through them.
    const int first = m_mode == QStringLiteral("center") ? 1 : 0;
    if (!tangent()) {
      for (int i = first; i < clicked(); ++i) {
        const Snap& s = snapOf(i);
        if (s.atPoint() || s.kind == Snap::Kind::Origin) {
          const QJsonValue point = op.pointInput(s);
          op.addConstraint({{QStringLiteral("type"), QStringLiteral("coincident")},
                            {QStringLiteral("point"), point.toString()},
                            {QStringLiteral("entity"), circle}});
        }
      }
    }
    if (m_c.typed(QStringLiteral("diameter"))) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("diameter")}, {QStringLiteral("curve"), circle}},
                      m_c.fieldExpression(QStringLiteral("diameter")));
    }
    op.commit();
    logCircle(circle);
    m_lines.clear();
    m_c.setPicked({});
  }

  void logCircle(const QString& id) const {
    if (const CurveData* c = m_c.model().curve(id)) {
      qDebug().noquote() << QStringLiteral("Added circle, diameter %1 mm at (%2, %3) [%4]")
                                .arg(2.0 * c->curve.radius)
                                .arg(c->curve.center.x)
                                .arg(c->curve.center.y)
                                .arg(id);
    }
  }

private:
  QString m_mode;
  QStringList m_lines;
};

// ---------------------------------------------------------------------------
// Arc

class ArcTool : public StepTool {
public:
  ArcTool(SketchController& c, const QString& mode) : StepTool(c), m_mode(mode) {}

  QString name() const override {
    if (m_mode == QStringLiteral("center")) {
      return QObject::tr("Center Point Arc");
    }
    return m_mode == QStringLiteral("tangent") ? QObject::tr("Tangent Arc") : QObject::tr("3-Point Arc");
  }

  void move(const Snap& snap) override {
    // The sweep of a centre point arc follows the cursor around.
    if (m_mode == QStringLiteral("center") && clicked() == 2) {
      const V2 c = click(0);
      const double before = angleOf(m_snap.at - c);
      const double now = angleOf(snap.at - c);
      m_sweep += std::remainder(now - before, kTwoPi);
    }
    StepTool::move(snap);
  }

  void press(const Snap& snap) override {
    if (m_mode == QStringLiteral("tangent") && clicked() == 0) {
      QString curve;
      if (!snap.atPoint() || !leavingDirection(m_c.model(), snap.point, &curve)) {
        m_c.hint(QObject::tr("Tangent Arc: click the end of a line or an arc."));
        return;
      }
    }
    if (m_mode == QStringLiteral("center") && clicked() == 1) {
      m_sweep = 0.0;
    }
    StepTool::press(snap);
  }

  void paint(ToolPreview& preview) const override {
    if (clicked() == 0) {
      return;
    }
    if (const auto arc = arcFor(cursor())) {
      preview.curve(*arc, step());
    } else {
      preview.line(click(clicked() - 1), cursor(), Style::Guide);
    }
    if (m_mode == QStringLiteral("center")) {
      preview.line(click(0), click(clicked() == 1 ? 0 : 1), Style::Guide);
      preview.line(click(0), cursor(), Style::Guide);
    }
  }

protected:
  int clicks() const override { return m_mode == QStringLiteral("tangent") ? 2 : 3; }

  QString prompt(int clicked) const override {
    if (m_mode == QStringLiteral("center")) {
      return clicked == 0   ? QObject::tr("Center Point Arc: click the centre.")
             : clicked == 1 ? QObject::tr("Center Point Arc: click the start, or type the radius.")
                            : QObject::tr("Center Point Arc: click the end, or type the angle.");
    }
    if (m_mode == QStringLiteral("tangent")) {
      return clicked == 0 ? QObject::tr("Tangent Arc: click the end of a line or an arc.")
                          : QObject::tr("Tangent Arc: click the end point.");
    }
    return clicked == 0   ? QObject::tr("3-Point Arc: click the start point.")
           : clicked == 1 ? QObject::tr("3-Point Arc: click the end point.")
                          : QObject::tr("3-Point Arc: click a point on the arc.");
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (m_mode == QStringLiteral("center")) {
      if (clicked == 1) {
        return {{QStringLiteral("radius"), QObject::tr("Radius"), FieldSpec::Kind::Length}};
      }
      return {{QStringLiteral("angle"), QObject::tr("Angle"), FieldSpec::Kind::Angle}};
    }
    return {};
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (m_mode == QStringLiteral("center")) {
      if (clicked() == 1) {
        if (const auto r = m_c.fieldValue(QStringLiteral("radius"))) {
          p = click(0) + unit(p - click(0)) * *r;
        }
      } else if (clicked() == 2) {
        if (const auto a = m_c.fieldValue(QStringLiteral("angle"))) {
          const double start = angleOf(click(1) - click(0));
          p = click(0) + polar(start + (m_sweep >= 0 ? *a : -*a), dist(click(0), click(1)));
        }
      }
    }
    return p;
  }

  void live() override {
    if (m_mode != QStringLiteral("center")) {
      return;
    }
    if (clicked() == 1) {
      m_c.setLive(QStringLiteral("radius"), dist(click(0), cursor()));
    } else if (clicked() == 2) {
      m_c.setLive(QStringLiteral("angle"), std::abs(m_sweep));
    }
  }

  std::optional<Curve2> arcFor(const V2& p) const {
    if (m_mode == QStringLiteral("tangent")) {
      const auto dir = leavingDirection(m_c.model(), snapOf(0).point);
      return dir ? tangentArc(click(0), *dir, p) : std::nullopt;
    }
    if (m_mode == QStringLiteral("center")) {
      if (clicked() < 2) {
        return std::nullopt;
      }
      const V2 c = click(0);
      const double r = dist(c, click(1));
      const double start = angleOf(click(1) - c);
      const double end = angleOf(p - c);
      return m_sweep >= 0 ? Curve2::arc(c, r, start, end) : Curve2::arc(c, r, end, start);
    }
    if (clicked() < 2) {
      return std::nullopt;
    }
    return arcThrough(click(0), p, click(1));
  }

  void commit() override {
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.arc"));
    cmd.insert(QStringLiteral("mode"), m_mode);
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    if (m_mode == QStringLiteral("three_point")) {
      cmd.insert(QStringLiteral("start"), op.pointInput(snapOf(0)));
      cmd.insert(QStringLiteral("end"), op.pointInput(snapOf(1)));
      cmd.insert(QStringLiteral("through"), xy(click(2)));
    } else if (m_mode == QStringLiteral("center")) {
      cmd.insert(QStringLiteral("center"), op.pointInput(snapOf(0)));
      cmd.insert(QStringLiteral("start"), op.pointInput(snapOf(1)));
      cmd.insert(QStringLiteral("end"), xy(click(2)));
      cmd.insert(QStringLiteral("clockwise"), m_sweep < 0);
    } else {
      cmd.insert(QStringLiteral("from"), snapOf(0).point);
      cmd.insert(QStringLiteral("end"), op.pointInput(snapOf(1)));
    }
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const QString arc = made(result).value(0);
    if (m_c.typed(QStringLiteral("radius"))) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("radius")}, {QStringLiteral("curve"), arc}},
                      m_c.fieldExpression(QStringLiteral("radius")));
    }
    op.commit();
    if (const CurveData* c = m_c.model().curve(arc)) {
      qDebug().noquote() << QStringLiteral("Added arc %1, radius %2 mm at %3")
                                .arg(arc)
                                .arg(c->curve.radius)
                                .arg(text(c->curve.center));
    }
  }

private:
  QString m_mode;
  double m_sweep = 0.0; // a centre point arc's sweep so far, signed
};

// ---------------------------------------------------------------------------
// Polygon

class PolygonTool : public StepTool {
public:
  PolygonTool(SketchController& c, const QString& mode) : StepTool(c), m_mode(mode) {}

  QString name() const override {
    if (m_mode == QStringLiteral("inscribed")) {
      return QObject::tr("Inscribed Polygon");
    }
    return m_mode == QStringLiteral("edge") ? QObject::tr("Edge Polygon")
                                            : QObject::tr("Circumscribed Polygon");
  }

  void paint(ToolPreview& preview) const override {
    if (clicked() == 0) {
      return;
    }
    const auto shape = corners(cursor());
    if (shape.empty()) {
      preview.line(click(0), cursor(), Style::Guide);
      return;
    }
    preview.add({shape.begin(), shape.end()}, Style::Draw, true);
  }

protected:
  int clicks() const override { return m_mode == QStringLiteral("edge") ? 3 : 2; }

  QString prompt(int clicked) const override {
    if (m_mode == QStringLiteral("edge")) {
      return clicked == 0   ? QObject::tr("Edge Polygon: click the start of an edge.")
             : clicked == 1 ? QObject::tr("Edge Polygon: click the end of the edge (Tab: sides).")
                            : QObject::tr("Edge Polygon: click the side the polygon is on.");
    }
    return clicked == 0 ? QObject::tr("%1: click the centre.").arg(name())
                        : QObject::tr("%1: click the size, or type the radius (Tab: sides).").arg(name());
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (clicked == 2) {
      return {{QStringLiteral("sides"), QObject::tr("Sides"), FieldSpec::Kind::Count}};
    }
    if (m_mode == QStringLiteral("edge")) {
      return {{QStringLiteral("length"), QObject::tr("Length"), FieldSpec::Kind::Length},
              {QStringLiteral("sides"), QObject::tr("Sides"), FieldSpec::Kind::Count}};
    }
    return {{QStringLiteral("radius"), QObject::tr("Radius"), FieldSpec::Kind::Length},
            {QStringLiteral("sides"), QObject::tr("Sides"), FieldSpec::Kind::Count}};
  }

  int sides() const {
    if (const auto n = m_c.fieldValue(QStringLiteral("sides"))) {
      m_sides = std::clamp(static_cast<int>(std::lround(*n)), 3, 256);
    }
    return m_sides;
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (clicked() == 1) {
      const char* field = m_mode == QStringLiteral("edge") ? "length" : "radius";
      if (const auto r = m_c.fieldValue(QLatin1String(field))) {
        p = click(0) + unit(p - click(0)) * *r;
      }
    }
    return p;
  }

  void live() override {
    if (clicked() == 1) {
      m_c.setLive(m_mode == QStringLiteral("edge") ? QStringLiteral("length") : QStringLiteral("radius"),
                  dist(click(0), cursor()));
    }
    m_c.setLive(QStringLiteral("sides"), sides());
  }

  // The centre and a vertex of the polygon, as the model takes them.
  std::optional<std::pair<V2, V2>> placement(const V2& p) const {
    const int n = sides();
    if (m_mode == QStringLiteral("edge")) {
      if (clicked() < 2) {
        return std::nullopt;
      }
      const V2 a = click(0);
      const V2 b = click(1);
      const double length = dist(a, b);
      if (length <= 0.0) {
        return std::nullopt;
      }
      const V2 normal = perp(unit(b - a));
      const double side = dot(p - a, normal) >= 0 ? 1.0 : -1.0;
      const V2 center = (a + b) / 2.0 + normal * (side * length / 2.0 / std::tan(kPi / n));
      return std::make_pair(center, a);
    }
    if (dist(click(0), p) <= 0.0) {
      return std::nullopt;
    }
    return std::make_pair(click(0), p);
  }

  std::vector<V2> corners(const V2& p) const {
    if (m_mode == QStringLiteral("edge") && clicked() == 1) {
      return {};
    }
    const auto place = placement(p);
    if (!place) {
      return {};
    }
    const int n = sides();
    const V2 center = place->first;
    V2 vertex = place->second;
    if (m_mode == QStringLiteral("circumscribed")) {
      // The vertex given is a side's middle.
      const double r = dist(center, vertex) / std::cos(kPi / n);
      vertex = center + polar(angleOf(vertex - center) + kPi / n, r);
    }
    std::vector<V2> result;
    const double r = dist(center, vertex);
    const double a0 = angleOf(vertex - center);
    for (int i = 0; i < n; ++i) {
      result.push_back(center + polar(a0 + kTwoPi * i / n, r));
    }
    return result;
  }

  void commit() override {
    const auto place = placement(click(clicked() - 1));
    if (!place) {
      return;
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.polygon"));
    cmd.insert(QStringLiteral("center"), xy(place->first));
    cmd.insert(QStringLiteral("vertex"), xy(place->second));
    cmd.insert(QStringLiteral("sides"), sides());
    cmd.insert(QStringLiteral("inscribed"), m_mode != QStringLiteral("circumscribed"));
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const SketchModel& model = op.model();
    if (m_mode != QStringLiteral("edge") && snapOf(0).onGeometry()) {
      // The polygon's construction circle has its centre.
      for (const QString& id : entities(result)) {
        const CurveData* c = model.curve(id);
        if (c != nullptr && c->isCircle()) {
          op.constrain(c->center, snapOf(0));
          break;
        }
      }
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Added polygon (%1 sides) at %2 [%3]")
                              .arg(sides())
                              .arg(text(place->first))
                              .arg(made(result).join(QStringLiteral(", ")));
  }

private:
  QString m_mode;
  mutable int m_sides = 6;
};

// ---------------------------------------------------------------------------
// Slot

class SlotTool : public StepTool {
public:
  SlotTool(SketchController& c, const QString& mode) : StepTool(c), m_mode(mode) {}

  QString name() const override {
    if (m_mode == QStringLiteral("overall")) {
      return QObject::tr("Overall Slot");
    }
    if (m_mode == QStringLiteral("center_point")) {
      return QObject::tr("Center Point Slot");
    }
    if (m_mode == QStringLiteral("three_point_arc")) {
      return QObject::tr("Three Point Arc Slot");
    }
    if (m_mode == QStringLiteral("center_point_arc")) {
      return QObject::tr("Center Point Arc Slot");
    }
    return QObject::tr("Center to Center Slot");
  }

  void paint(ToolPreview& preview) const override {
    if (clicked() == 0) {
      return;
    }
    if (clicked() < clicks() - 1) {
      preview.line(click(clicked() - 1), cursor(), Style::Guide);
      if (arcSlot() && clicked() == 2) {
        if (const auto axis = axisArc(cursor())) {
          preview.curve(*axis, step(), Style::Construction);
        }
      }
      return;
    }
    const double w = width(cursor());
    if (arcSlot()) {
      const auto axis = axisArc(click(clicks() - 2));
      if (!axis || w <= 0.0) {
        return;
      }
      preview.curve(*axis, step(), Style::Construction);
      for (const double r : {axis->radius - w / 2.0, axis->radius + w / 2.0}) {
        if (r > 0.0) {
          preview.curve(Curve2::arc(axis->center, r, axis->start, axis->end), step());
        }
      }
      for (const double a : {axis->start, axis->end}) {
        preview.curve(Curve2::circle(axis->center + polar(a, axis->radius), w / 2.0), step(), Style::Guide);
      }
      return;
    }
    const auto ends = centers();
    if (!ends || w <= 0.0) {
      return;
    }
    const V2 u = unit(ends->second - ends->first);
    const V2 n = perp(u) * (w / 2.0);
    preview.line(ends->first + n, ends->second + n);
    preview.line(ends->first - n, ends->second - n);
    const double a = angleOf(n);
    preview.curve(Curve2::arc(ends->second, w / 2.0, a - kPi, a), step());
    preview.curve(Curve2::arc(ends->first, w / 2.0, a, a + kPi), step());
    preview.line(ends->first, ends->second, Style::Construction);
  }

protected:
  bool arcSlot() const {
    return m_mode == QStringLiteral("three_point_arc") || m_mode == QStringLiteral("center_point_arc");
  }

  int clicks() const override { return arcSlot() ? 4 : 3; }

  QString prompt(int clicked) const override {
    if (clicked == clicks() - 1) {
      return QObject::tr("%1: click the width, or type it.").arg(name());
    }
    if (m_mode == QStringLiteral("center_point")) {
      return clicked == 0 ? QObject::tr("Center Point Slot: click the centre.")
                          : QObject::tr("Center Point Slot: click the centre of an end.");
    }
    if (m_mode == QStringLiteral("three_point_arc")) {
      return clicked == 0   ? QObject::tr("Three Point Arc Slot: click the start.")
             : clicked == 1 ? QObject::tr("Three Point Arc Slot: click the end.")
                            : QObject::tr("Three Point Arc Slot: click a point on the arc.");
    }
    if (m_mode == QStringLiteral("center_point_arc")) {
      return clicked == 0   ? QObject::tr("Center Point Arc Slot: click the centre.")
             : clicked == 1 ? QObject::tr("Center Point Arc Slot: click the start.")
                            : QObject::tr("Center Point Arc Slot: click the end.");
    }
    return clicked == 0 ? QObject::tr("%1: click the first end.").arg(name())
                        : QObject::tr("%1: click the other end, or type the length.").arg(name());
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (clicked == clicks() - 1) {
      return {{QStringLiteral("width"), QObject::tr("Width"), FieldSpec::Kind::Length}};
    }
    if (!arcSlot() && clicked == 1) {
      return {{QStringLiteral("length"), QObject::tr("Length"), FieldSpec::Kind::Length}};
    }
    return {};
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (!arcSlot() && clicked() == 1) {
      if (const auto length = m_c.fieldValue(QStringLiteral("length"))) {
        p = click(0) + unit(p - click(0)) * *length;
      }
    }
    return p;
  }

  void live() override {
    if (!arcSlot() && clicked() == 1) {
      m_c.setLive(QStringLiteral("length"), dist(click(0), cursor()));
    }
    if (clicked() == clicks() - 1) {
      m_c.setLive(QStringLiteral("width"), width(cursor()));
    }
  }

  double width(const V2& p) const {
    if (const auto w = m_c.fieldValue(QStringLiteral("width"))) {
      return *w;
    }
    if (arcSlot()) {
      const auto axis = axisArc(click(clicks() - 2));
      return axis ? 2.0 * std::abs(dist(p, axis->center) - axis->radius) : 0.0;
    }
    const V2 a = click(0);
    const V2 b = click(1);
    const V2 n = perp(unit(b - a));
    return 2.0 * std::abs(dot(p - a, n));
  }

  // The centres of the slot's ends (straight slots).
  std::optional<std::pair<V2, V2>> centers() const {
    if (clicked() < 2) {
      return std::nullopt;
    }
    V2 a = click(0);
    V2 b = click(1);
    if (m_mode == QStringLiteral("center_point")) {
      a = click(0) * 2.0 - click(1);
    } else if (m_mode == QStringLiteral("overall")) {
      const double w = width(cursor());
      const V2 u = unit(b - a);
      a = a + u * (w / 2.0);
      b = b - u * (w / 2.0);
      if (dot(b - a, u) <= 0.0) {
        return std::nullopt;
      }
    }
    return std::make_pair(a, b);
  }

  // The slot's centre arc, counter-clockwise.
  std::optional<Curve2> axisArc(const V2& third) const {
    if (m_mode == QStringLiteral("three_point_arc")) {
      if (clicked() < 2) {
        return std::nullopt;
      }
      return arcThrough(click(0), third, click(1));
    }
    if (clicked() < 2) {
      return std::nullopt;
    }
    const V2 c = click(0);
    const double r = dist(c, click(1));
    return Curve2::arc(c, r, angleOf(click(1) - c), angleOf(third - c));
  }

  void commit() override {
    const double w = width(click(clicks() - 1));
    if (w <= 0.0) {
      return;
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.slot"));
    cmd.insert(QStringLiteral("width"), w);
    if (arcSlot()) {
      const auto axis = axisArc(click(2));
      if (!axis) {
        op.rollback();
        return;
      }
      cmd.insert(QStringLiteral("mode"), QStringLiteral("arc"));
      cmd.insert(QStringLiteral("center"), xy(axis->center));
      cmd.insert(QStringLiteral("start"), xy(axis->startPoint()));
      cmd.insert(QStringLiteral("end"), xy(axis->endPoint()));
    } else if (m_mode == QStringLiteral("center_point")) {
      cmd.insert(QStringLiteral("mode"), QStringLiteral("center_point"));
      cmd.insert(QStringLiteral("center"), xy(click(0)));
      cmd.insert(QStringLiteral("end"), xy(click(1)));
    } else {
      const auto ends = centers();
      if (!ends) {
        op.rollback();
        return;
      }
      cmd.insert(QStringLiteral("mode"), QStringLiteral("center_to_center"));
      cmd.insert(QStringLiteral("a"), xy(ends->first));
      cmd.insert(QStringLiteral("b"), xy(ends->second));
    }
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Added slot, width %1 mm [%2]")
                              .arg(w)
                              .arg(made(result).join(QStringLiteral(", ")));
  }

private:
  QString m_mode;
};

// ---------------------------------------------------------------------------
// Ellipse

class EllipseTool : public StepTool {
public:
  using StepTool::StepTool;

  QString name() const override { return QObject::tr("Ellipse"); }

  void paint(ToolPreview& preview) const override {
    if (clicked() == 0) {
      return;
    }
    preview.line(click(0), clicked() == 1 ? cursor() : click(1), Style::Guide);
    if (clicked() == 2) {
      if (const auto e = ellipse(cursor())) {
        preview.curve(*e, step());
      }
    }
  }

protected:
  int clicks() const override { return 3; }

  QString prompt(int clicked) const override {
    return clicked == 0   ? QObject::tr("Ellipse: click the centre.")
           : clicked == 1 ? QObject::tr("Ellipse: click the end of the major axis.")
                          : QObject::tr("Ellipse: click a point on the ellipse, or type the minor radius.");
  }

  QVector<FieldSpec> fields(int clicked) const override {
    if (clicked == 1) {
      return {{QStringLiteral("major"), QObject::tr("Major radius"), FieldSpec::Kind::Length}};
    }
    return {{QStringLiteral("minor"), QObject::tr("Minor radius"), FieldSpec::Kind::Length}};
  }

  V2 cursor() const override {
    V2 p = m_snap.at;
    if (clicked() == 1) {
      if (const auto r = m_c.fieldValue(QStringLiteral("major"))) {
        p = click(0) + unit(p - click(0)) * *r;
      }
    }
    return p;
  }

  double minor(const V2& p) const {
    if (const auto r = m_c.fieldValue(QStringLiteral("minor"))) {
      return *r;
    }
    const V2 u = unit(click(1) - click(0));
    return std::abs(cross(u, p - click(0)));
  }

  void live() override {
    if (clicked() == 1) {
      m_c.setLive(QStringLiteral("major"), dist(click(0), cursor()));
    } else if (clicked() == 2) {
      m_c.setLive(QStringLiteral("minor"), minor(cursor()));
    }
  }

  std::optional<Curve2> ellipse(const V2& p) const {
    const double major = dist(click(0), click(1));
    const double r = minor(p);
    if (major <= 0.0 || r <= 0.0) {
      return std::nullopt;
    }
    Curve2 e;
    e.type = Curve2::Type::Ellipse;
    e.center = click(0);
    e.radius = std::max(major, r);
    e.minor = std::min(major, r);
    e.rotation = angleOf(click(1) - click(0)) + (r > major ? kPi / 2.0 : 0.0);
    return e;
  }

  void commit() override {
    const auto e = ellipse(click(2));
    if (!e) {
      return;
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.ellipse"));
    cmd.insert(QStringLiteral("center"), op.pointInput(snapOf(0)));
    cmd.insert(QStringLiteral("major"), xy(e->center + polar(e->rotation, e->radius)));
    cmd.insert(QStringLiteral("minor_radius"), e->minor);
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const QString id = made(result).value(0);
    if (m_c.typed(QStringLiteral("major"))) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("major_radius")}, {QStringLiteral("ellipse"), id}},
                      m_c.fieldExpression(QStringLiteral("major")));
    }
    if (m_c.typed(QStringLiteral("minor"))) {
      op.addDimension({{QStringLiteral("type"), QStringLiteral("minor_radius")}, {QStringLiteral("ellipse"), id}},
                      m_c.fieldExpression(QStringLiteral("minor")));
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Added ellipse %1, radii %2 and %3 mm").arg(id).arg(e->radius).arg(e->minor);
  }
};

// ---------------------------------------------------------------------------
// Spline

class SplineTool : public SketchTool {
public:
  SplineTool(SketchController& c, bool control) : SketchTool(c), m_control(control) {}

  QString name() const override {
    return m_control ? QObject::tr("Control Point Spline") : QObject::tr("Fit Point Spline");
  }

  void start() override {
    m_c.hint(QObject::tr("%1: click points; double-click or Enter finishes.").arg(name()));
  }

  void move(const Snap& snap) override { m_snap = snap; }

  void press(const Snap& snap) override {
    if (!m_points.empty() && dist(m_points.back().at, snap.at) <= 1e-9) {
      return;
    }
    m_points.push_back(snap);
  }

  void doubleClick(const Snap& snap) override {
    Q_UNUSED(snap);
    finish();
  }

  bool confirm() override {
    if (m_points.empty()) {
      return false;
    }
    finish();
    return true;
  }

  bool cancel() override {
    if (m_points.empty()) {
      return false;
    }
    m_points.pop_back();
    return true;
  }

  void paint(ToolPreview& preview) const override {
    std::vector<V2> points;
    for (const Snap& s : m_points) {
      points.push_back(s.at);
    }
    if (points.empty()) {
      return;
    }
    if (dist(points.back(), m_snap.at) > 0.0) {
      points.push_back(m_snap.at);
    }
    if (m_control) {
      preview.add(points, Style::Guide);
      preview.add(controlSplinePreview(points, 3));
    } else {
      preview.add(fitSplinePreview(points));
    }
    for (const Snap& s : m_points) {
      preview.points.push_back(s.at);
    }
  }

private:
  void finish() {
    if (m_points.size() < 2) {
      m_points.clear();
      return;
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.spline"));
    QJsonArray points;
    for (const Snap& s : m_points) {
      points.append(op.pointInput(s));
    }
    cmd.insert(QStringLiteral("points"), points);
    if (m_control) {
      cmd.insert(QStringLiteral("degree"), std::min(3, static_cast<int>(m_points.size()) - 1));
    }
    cmd.insert(QStringLiteral("construction"), m_c.construction());
    QJsonObject result;
    const std::size_t count = m_points.size();
    m_points.clear();
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Added spline %1 through %2 points").arg(made(result).value(0)).arg(count);
  }

  bool m_control;
  std::vector<Snap> m_points;
  Snap m_snap;
};

// ---------------------------------------------------------------------------
// Point and text

class PointTool : public SketchTool {
public:
  using SketchTool::SketchTool;

  QString name() const override { return QObject::tr("Point"); }
  void start() override { m_c.hint(QObject::tr("Point: click where it goes.")); }

  void press(const Snap& snap) override {
    if (snap.atPoint()) {
      return; // a point is there already
    }
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.add_point"));
    cmd.insert(QStringLiteral("at"), xy(snap.at));
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return;
    }
    const QString point = entities(result).value(0);
    op.constrain(point, snap);
    op.commit();
    qDebug().noquote() << QStringLiteral("Added point %1 at %2").arg(point, text(snap.at));
  }
};

// Text: a click places a text that starts there; a drag draws its
// frame (a construction rectangle the text fills, multi-line). Then the
// text and its height are typed; Enter adds it. Font, style, alignment,
// angle and a path are set in the Edit Text panel (double-click the text).
class TextTool : public SketchTool {
public:
  using SketchTool::SketchTool;

  QString name() const override { return QObject::tr("Text"); }
  void start() override {
    m_c.hint(QObject::tr("Text: click where the text starts, or drag its frame."));
  }

  void move(const Snap& snap) override { m_cursor = snap.at; }

  void press(const Snap& snap) override {
    if (m_placed) {
      return;
    }
    m_at = snap.at;
    m_cursor = snap.at;
    m_pressed = true;
  }

  void release(const Snap& snap) override {
    if (!m_pressed) {
      return;
    }
    m_pressed = false;
    // A drag of a few pixels or more is a frame.
    const V2 d = snap.at - m_at;
    m_framed = std::abs(d.x) > 6 * m_c.pixel() && std::abs(d.y) > 6 * m_c.pixel();
    m_diagonal = snap.at;
    m_placed = true;
    m_c.setFields({{QStringLiteral("text"), QObject::tr("Text"), FieldSpec::Kind::Text},
                   {QStringLiteral("height"), QObject::tr("Height"), FieldSpec::Kind::Length}});
    m_c.setLive(QStringLiteral("height"), kDefaultHeight);
    m_c.hint(QObject::tr("Text: type the text (Tab: height) and press Enter."));
  }

  bool confirm() override {
    if (!m_placed) {
      return false;
    }
    const QString content = m_c.fieldText(QStringLiteral("text"));
    if (content.trimmed().isEmpty()) {
      m_c.hint(QObject::tr("Text: type the text first."));
      return true;
    }
    const double height = m_c.fieldValue(QStringLiteral("height")).value_or(kDefaultHeight);
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.add_text"));
    cmd.insert(QStringLiteral("text"), content);
    if (m_framed) {
      // The frame from its lower left corner; the text at its top.
      const V2 lo{std::min(m_at.x, m_diagonal.x), std::min(m_at.y, m_diagonal.y)};
      const V2 hi{std::max(m_at.x, m_diagonal.x), std::max(m_at.y, m_diagonal.y)};
      cmd.insert(QStringLiteral("frame"),
                 QJsonObject{{QStringLiteral("corner"), xy(lo)}, {QStringLiteral("diagonal"), xy(hi)}});
      cmd.insert(QStringLiteral("valign"), QStringLiteral("top"));
    } else {
      cmd.insert(QStringLiteral("at"), xy(m_at));
    }
    cmd.insert(QStringLiteral("height"), height);
    QJsonObject result;
    m_placed = false;
    m_c.clearFields();
    if (!op.run(cmd, &result)) {
      refuse(m_c, op, name());
      return true;
    }
    op.commit();
    qDebug().noquote() << QStringLiteral("Added text %1 \"%2\" %3 %4")
                              .arg(made(result).value(0), content,
                                   m_framed ? QStringLiteral("in a frame from") : QStringLiteral("at"),
                                   m_framed ? text(m_at) + QStringLiteral(" to ") + text(m_diagonal) : text(m_at));
    m_c.hint(QObject::tr("Text: click where the next text starts, or drag its frame."));
    return true;
  }

  bool cancel() override {
    if (!m_placed && !m_pressed) {
      return false;
    }
    m_placed = false;
    m_pressed = false;
    m_c.clearFields();
    return true;
  }

  void paint(ToolPreview& preview) const override {
    if (!m_placed && !m_pressed) {
      return;
    }
    preview.points.push_back(m_at);
    const V2 corner = m_pressed ? m_cursor : m_diagonal;
    if (m_pressed || m_framed) {
      preview.add({m_at, V2{corner.x, m_at.y}, corner, V2{m_at.x, corner.y}}, ToolPreview::Style::Construction,
                  true);
    }
  }

private:
  static constexpr double kDefaultHeight = 5.0; // mm

  V2 m_at;
  V2 m_diagonal;
  V2 m_cursor;
  bool m_pressed = false;
  bool m_framed = false;
  bool m_placed = false;
};

} // namespace

std::unique_ptr<SketchTool> lineTool(SketchController& c) { return std::make_unique<LineTool>(c); }

std::unique_ptr<SketchTool> rectangleTool(SketchController& c, const QString& mode) {
  return std::make_unique<RectangleTool>(c, mode);
}

std::unique_ptr<SketchTool> circleTool(SketchController& c, const QString& mode) {
  return std::make_unique<CircleTool>(c, mode);
}

std::unique_ptr<SketchTool> arcTool(SketchController& c, const QString& mode) {
  return std::make_unique<ArcTool>(c, mode);
}

std::unique_ptr<SketchTool> polygonTool(SketchController& c, const QString& mode) {
  return std::make_unique<PolygonTool>(c, mode);
}

std::unique_ptr<SketchTool> slotTool(SketchController& c, const QString& mode) {
  return std::make_unique<SlotTool>(c, mode);
}

std::unique_ptr<SketchTool> ellipseTool(SketchController& c) { return std::make_unique<EllipseTool>(c); }

std::unique_ptr<SketchTool> splineTool(SketchController& c, bool controlPoints) {
  return std::make_unique<SplineTool>(c, controlPoints);
}

std::unique_ptr<SketchTool> pointTool(SketchController& c) { return std::make_unique<PointTool>(c); }

std::unique_ptr<SketchTool> textTool(SketchController& c) { return std::make_unique<TextTool>(c); }

} // namespace mitcad::sketch
