// SPDX-License-Identifier: MIT
// Extend to Meet (mitcad#2178, app/COMMANDS.md "Extend to Meet"): two
// lines or arcs extended or shortened to where they meet, ending in one
// shared point. The tool takes a click on the first curve and one on the
// second, each on the part that stays; while the pointer is over the
// second curve the preview shows the result the click makes, and Tab
// (Shift+Tab back) steps to the next one where there are several.
#include "ExtendToMeet.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <memory>
#include <utility>
#include <vector>

#include <QJsonObject>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QtLogging>

#include "../framework/UserText.hpp"
#include "SketchController.hpp"
#include "SketchTool.hpp"
#include "ToolSupport.hpp"

namespace mitcad::sketch {
namespace {

// The endless carrier of a line or an arc: through `p` along the unit `d`,
// or the circle about `c` of radius `r`.
struct Carrier {
  bool line = true;
  V2 p, d;
  V2 c;
  double r = 0.0;
};

double length(const V2& v) { return std::hypot(v.x, v.y); }
double distance(const V2& a, const V2& b) { return length(a - b); }

// An angle in [0, 2π), as Rust's rem_euclid.
double turn(double angle) {
  const double r = std::fmod(angle, kTwoPi);
  return r < 0.0 ? r + kTwoPi : r;
}

bool carrierOf(const Curve2& curve, Carrier& out) {
  if (curve.type == Curve2::Type::Line) {
    const double l = distance(curve.a, curve.b);
    if (!(l > 0.0)) {
      return false;
    }
    out.line = true;
    out.p = curve.a;
    out.d = (curve.b - curve.a) * (1.0 / l);
    return true;
  }
  if (curve.type == Curve2::Type::Arc) {
    out.line = false;
    out.c = curve.center;
    out.r = curve.radius;
    return true;
  }
  return false;
}

// One plus the largest coordinate of the two curves.
double meetSize(const Curve2& a, const Curve2& b) {
  const auto extent = [](const Curve2& c) {
    if (c.type == Curve2::Type::Line) {
      return std::max(std::max(std::abs(c.a.x), std::abs(c.a.y)), std::max(std::abs(c.b.x), std::abs(c.b.y)));
    }
    if (c.type == Curve2::Type::Arc) {
      return std::max(std::abs(c.center.x), std::abs(c.center.y)) + c.radius;
    }
    return 0.0;
  };
  return 1.0 + std::max(extent(a), extent(b));
}

NoMeet lineCircle(const V2& p, const V2& d, const V2& c, double r, double size, std::vector<V2>& out) {
  const V2 foot = p + d * dot(c - p, d);
  const double h = distance(foot, c);
  const double touch = 1e-6 * size;
  if (h > r + touch) {
    return NoMeet::Apart;
  }
  if (h >= r - touch) {
    out.push_back(foot);
    return NoMeet::None;
  }
  const double k = std::sqrt(r * r - h * h);
  out.push_back(foot - d * k);
  out.push_back(foot + d * k);
  return NoMeet::None;
}

NoMeet carrierMeets(const Carrier& a, const Carrier& b, double size, std::vector<V2>& out) {
  if (a.line && b.line) {
    const double den = cross(a.d, b.d);
    if (std::abs(den) <= 1e-12) {
      return std::abs(cross(b.p - a.p, a.d)) <= 1e-9 * size ? NoMeet::Collinear : NoMeet::Parallel;
    }
    const double s = cross(b.p - a.p, b.d) / den;
    out.push_back(a.p + a.d * s);
    return NoMeet::None;
  }
  if (a.line) {
    return lineCircle(a.p, a.d, b.c, b.r, size, out);
  }
  if (b.line) {
    return lineCircle(b.p, b.d, a.c, a.r, size, out);
  }
  const V2 dv = b.c - a.c;
  const double d = length(dv);
  if (d <= 1e-9 * size) {
    return NoMeet::Concentric;
  }
  const double touch = 1e-6 * size;
  if (d > a.r + b.r + touch || d < std::abs(a.r - b.r) - touch) {
    return NoMeet::Apart;
  }
  const double along = (d * d + a.r * a.r - b.r * b.r) / (2.0 * d);
  const V2 m = a.c + dv * (along / d);
  const double h2 = a.r * a.r - along * along;
  if (std::abs(d - (a.r + b.r)) <= touch || std::abs(d - std::abs(a.r - b.r)) <= touch || h2 <= 0.0) {
    out.push_back(m);
    return NoMeet::None;
  }
  const double h = std::sqrt(h2);
  const V2 n{-dv.y / d, dv.x / d};
  out.push_back(m - n * h);
  out.push_back(m + n * h);
  return NoMeet::None;
}

// The ways a curve can end at `x` keeping the part around `pick`.
std::vector<MeetEnd> curveEnds(const Curve2& curve, const V2& pick, const V2& x, double size) {
  std::vector<MeetEnd> ends;
  if (curve.type == Curve2::Type::Line) {
    const double l = distance(curve.a, curve.b);
    const V2 u = (curve.b - curve.a) * (1.0 / l);
    const double tp = std::clamp(dot(pick - curve.a, u), 0.0, l);
    const double tx = dot(x - curve.a, u);
    const double least = 1e-9 * size;
    if (tx >= tp) {
      if (tx > least) {
        ends.push_back({true, Curve2::line(curve.a, x)});
      }
    } else if (l - tx > least) {
      ends.push_back({false, Curve2::line(x, curve.b)});
    }
    return ends;
  }
  if (curve.type != Curve2::Type::Arc) {
    return ends;
  }
  const double span = curve.end - curve.start;
  const double least = 1e-9;
  const auto angle = [&curve](const V2& p) {
    const V2 v = p - curve.center;
    return turn(std::atan2(v.y, v.x) - curve.start);
  };
  double fp = angle(pick);
  if (fp > span) {
    fp = fp - span < kTwoPi - fp ? span : 0.0;
  }
  double fx = angle(x);
  if (fx > kTwoPi - least) {
    fx = 0.0;
  }
  const auto arc = [&curve](double s, double e) { return Curve2::arc(curve.center, curve.radius, s, e); };
  if (fx <= span + least) {
    if (fx >= fp) {
      if (fx > least) {
        ends.push_back({true, arc(curve.start, curve.start + fx)});
      }
    } else if (span - fx > least) {
      ends.push_back({false, arc(curve.start + fx, curve.end)});
    }
    return ends;
  }
  const MeetEnd forward{true, arc(curve.start, curve.start + fx)};
  const MeetEnd back{false, arc(curve.start + fx - kTwoPi, curve.end)};
  if (span - fp <= fp) {
    ends = {forward, back};
  } else {
    ends = {back, forward};
  }
  return ends;
}

// The tool: a click on the first curve, then one on the second.
class ExtendToMeetTool : public SketchTool {
public:
  explicit ExtendToMeetTool(SketchController& c) : SketchTool(c) {}

  QString name() const override { return QObject::tr("Extend to Meet"); }
  bool snaps() const override { return false; }
  bool bodySnaps() const override { return false; }

  void start() override { hintFirst(); }

  void move(const Snap& snap) override {
    const QString curve = curveAt(snap);
    if (m_first.isEmpty()) {
      m_c.setHover(curve);
      return;
    }
    const CurveData* c = m_c.model().curve(curve);
    if (c == nullptr || curve == m_first) {
      clearSecond();
      return;
    }
    if (curve != m_second) {
      m_choice = 0;
    }
    m_second = curve;
    m_secondAt = c->curve.nearest(snap.at);
    m_c.setHover(curve);
    refresh();
  }

  void press(const Snap& snap) override {
    const QString curve = curveAt(snap);
    const CurveData* c = m_c.model().curve(curve);
    if (c == nullptr) {
      return;
    }
    const V2 at = c->curve.nearest(snap.at);
    if (m_first.isEmpty() || curve == m_first) {
      // The first curve, or picked again on another part.
      m_first = curve;
      m_firstAt = at;
      qDebug().noquote() << QStringLiteral("Extend to Meet first %1 at %2").arg(curve, text(at));
      m_c.setHover(QString());
      m_c.hint(QObject::tr("Extend to Meet: click the second line or arc on the part to keep; Tab shows the next "
                           "result."));
      return;
    }
    if (curve != m_second) {
      m_choice = 0;
    }
    m_second = curve;
    m_secondAt = at;
    refresh();
    SketchOp op(m_c);
    QJsonObject cmd = op.command(QStringLiteral("sketch.extend_to_meet"));
    cmd.insert(QStringLiteral("a"), m_first);
    cmd.insert(QStringLiteral("at_a"), xy(m_firstAt));
    cmd.insert(QStringLiteral("b"), curve);
    cmd.insert(QStringLiteral("at_b"), xy(at));
    cmd.insert(QStringLiteral("result"), m_choice);
    QJsonObject result;
    if (!op.run(cmd, &result)) {
      const QString reason = op.error();
      op.rollback();
      m_c.error(QObject::tr("%1: %2").arg(name(), reason));
      return;
    }
    op.commit();
    const QString joined = result.value(QStringLiteral("joined")).toString();
    const auto place = m_c.model().pointAt(joined);
    qDebug().noquote() << QStringLiteral("Extended %1 and %2 to meet at %3 %4, result %5 of %6")
                              .arg(m_first, curve, joined, place ? text(*place) : QStringLiteral("?"))
                              .arg(result.value(QStringLiteral("result")).toInt() + 1)
                              .arg(result.value(QStringLiteral("results")).toInt());
    // What the model says besides, in words: an offset's or a pattern's
    // curve freed first (mitcad#1755, mitcad#2007).
    const QStringList notes = shownTexts(result, QStringLiteral("notes"), QStringLiteral("note_messages"));
    if (!notes.isEmpty()) {
      m_c.notice(QObject::tr("%1: %2").arg(name(), notes.join(QStringLiteral("; "))), NoticeLevel::Information);
    }
    m_first.clear();
    clearSecond();
    hintFirst();
  }

  // Esc: the first pick goes; at the start the tool ends.
  bool cancel() override {
    if (m_first.isEmpty()) {
      return false;
    }
    m_first.clear();
    clearSecond();
    hintFirst();
    return true;
  }

  // Tab and Shift+Tab: the next or the previous result.
  bool viewKey(int key, Qt::KeyboardModifiers modifiers) override {
    const Qt::KeyboardModifiers held = modifiers & ~Qt::KeyboardModifiers(Qt::KeypadModifier | Qt::ShiftModifier);
    if ((key != Qt::Key_Tab && key != Qt::Key_Backtab) || held != Qt::NoModifier || m_results.size() < 2) {
      return false;
    }
    const int count = static_cast<int>(m_results.size());
    const bool back = key == Qt::Key_Backtab || modifiers.testFlag(Qt::ShiftModifier);
    m_choice = ((m_choice % count) + (back ? count - 1 : 1)) % count;
    qDebug().noquote() << QStringLiteral("Extend to Meet Tab steps to result %1 of %2").arg(m_choice + 1).arg(count);
    refresh();
    return true;
  }

  void paint(ToolPreview& preview) const override {
    const double step = 2.0 * m_c.pixel();
    if (const CurveData* first = m_c.model().curve(m_first)) {
      preview.curve(first->curve, step, ToolPreview::Style::Highlight);
    }
    if (m_results.empty()) {
      return;
    }
    const MeetResult& shown = m_results.at(static_cast<std::size_t>(m_choice) % m_results.size());
    for (const QString& id : {m_first, m_second}) {
      if (const CurveData* c = m_c.model().curve(id)) {
        preview.curve(c->curve, step, ToolPreview::Style::Remove);
      }
    }
    preview.curve(shown.a.curve, step);
    preview.curve(shown.b.curve, step);
    preview.points.push_back(shown.point);
  }

private:
  QString curveAt(const Snap& snap) const { return m_c.entityAt(snap.screen, PickCurves); }

  void hintFirst() {
    m_c.hint(QObject::tr("Extend to Meet: click the first line or arc on the part to keep."));
  }

  void clearSecond() {
    m_second.clear();
    m_results.clear();
    m_choice = 0;
    m_logged.clear();
    m_c.setHover(QString());
  }

  // The results for the picks, and the one shown in the log.
  void refresh() {
    const CurveData* a = m_c.model().curve(m_first);
    const CurveData* b = m_c.model().curve(m_second);
    m_results = a != nullptr && b != nullptr ? meetResults(a->curve, m_firstAt, b->curve, m_secondAt)
                                             : std::vector<MeetResult>();
    QString logged = QStringLiteral("%1 and %2: no result").arg(m_first, m_second);
    if (!m_results.empty()) {
      const int count = static_cast<int>(m_results.size());
      const int index = m_choice % count;
      logged = QStringLiteral("%1 and %2: result %3 of %4 at %5")
                   .arg(m_first, m_second)
                   .arg(index + 1)
                   .arg(count)
                   .arg(text(m_results.at(static_cast<std::size_t>(index)).point));
    }
    if (logged != m_logged) {
      m_logged = logged;
      qDebug().noquote() << QStringLiteral("Extend to Meet preview %1").arg(logged);
    }
  }

  QString m_first;
  V2 m_firstAt;
  QString m_second;
  V2 m_secondAt;
  std::vector<MeetResult> m_results;
  int m_choice = 0;
  QString m_logged;
};

} // namespace

std::vector<MeetResult> meetResults(const Curve2& a, const V2& atA, const Curve2& b, const V2& atB, NoMeet* why) {
  const auto refuse = [why](NoMeet reason) {
    if (why != nullptr) {
      *why = reason;
    }
    return std::vector<MeetResult>();
  };
  Carrier ca;
  Carrier cb;
  if (!carrierOf(a, ca) || !carrierOf(b, cb)) {
    return refuse(NoMeet::NotLineOrArc);
  }
  const double size = meetSize(a, b);
  std::vector<V2> points;
  const NoMeet none = carrierMeets(ca, cb, size, points);
  if (none != NoMeet::None) {
    return refuse(none);
  }
  std::vector<MeetResult> results;
  for (const V2& x : points) {
    for (const MeetEnd& ea : curveEnds(a, atA, x, size)) {
      for (const MeetEnd& eb : curveEnds(b, atB, x, size)) {
        results.push_back({x, ea, eb});
      }
    }
  }
  if (results.empty()) {
    return refuse(NoMeet::Collapses);
  }
  // A stable sort: results at one point keep their order.
  const auto key = [&atA, &atB](const MeetResult& r) { return distance(r.point, atA) + distance(r.point, atB); };
  std::stable_sort(results.begin(), results.end(),
                   [&key](const MeetResult& p, const MeetResult& q) { return key(p) < key(q); });
  if (why != nullptr) {
    *why = NoMeet::None;
  }
  return results;
}

std::unique_ptr<SketchTool> extendToMeetTool(SketchController& c) { return std::make_unique<ExtendToMeetTool>(c); }

} // namespace mitcad::sketch
