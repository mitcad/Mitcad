// SPDX-License-Identifier: MIT
// Chains of sketch curves (SketchChains.hpp): Offset's Chain Selection and
// a double-click on a curve (mitcad#1541). Only the sketch model's data.
#include "SketchChains.hpp"

#include <algorithm>
#include <array>
#include <initializer_list>

#include <QSet>

#include "SketchGeometry.hpp"
#include "SketchModel.hpp"

namespace mitcad::sketch {
namespace {

// Lines, arcs, elliptical arcs and open splines chain; a closed curve is
// offset on its own.
bool joinable(const CurveData& c) {
  return c.isLine() || c.isArc() || c.type == QStringLiteral("elliptical_arc") ||
         (c.isSpline() && c.start != c.end);
}

// Whether two open curves lie on each other (an edge projected twice,
// mitcad#741): of one kind, through the same places, a line or a spline
// either way round.
bool sameCurve(const CurveData& a, const CurveData& b) {
  if (a.id == b.id || a.type != b.type || a.curve.isClosed() || b.curve.isClosed()) {
    return false;
  }
  const Curve2& p = a.curve;
  const Curve2& q = b.curve;
  const double size = std::max({1.0, norm(p.startPoint()), norm(p.endPoint())});
  const double tolerance = 1e-7 * size;
  const auto along = [](const Curve2& c, double f) { return c.at(c.t0() + f * (c.t1() - c.t0())); };
  const std::array<double, 5> fractions = {0.0, 0.25, 0.5, 0.75, 1.0};
  bool ahead = true;
  bool back = a.isLine() || a.isSpline();
  for (const double f : fractions) {
    ahead = ahead && dist(along(p, f), along(q, f)) <= tolerance;
    back = back && dist(along(p, f), along(q, 1.0 - f)) <= tolerance;
  }
  return ahead || back;
}

// Whether a curve shares an end point with another curve a chain selection
// would join to it.
bool joinsOthers(const SketchModel& model, const CurveData& c) {
  for (const CurveData& other : model.curves) {
    if (other.id != c.id && joinable(other) && other.construction == c.construction &&
        (other.start == c.start || other.end == c.start || other.start == c.end || other.end == c.end)) {
      return true;
    }
  }
  return false;
}

// A picked curve, or the curve it lies on when it joins nothing and that
// one does (mitcad#741: a face projected and one of its edges again, the
// edge with points of its own): the chain grows through the face's curve.
QString chainStart(const SketchModel& model, const QString& picked) {
  const CurveData* c = model.curve(picked);
  if (c == nullptr || !joinable(*c) || joinsOthers(model, *c)) {
    return picked;
  }
  for (const CurveData& other : model.curves) {
    if (joinable(other) && other.construction == c->construction && sameCurve(*c, other) &&
        joinsOthers(model, other)) {
      return other.id;
    }
  }
  return picked;
}

} // namespace

QStringList chainOf(const SketchModel& model, const QStringList& picked) {
  QStringList chain;
  for (const QString& id : picked) {
    const QString start = chainStart(model, id);
    if (!chain.contains(start)) {
      chain << start;
    }
  }
  QSet<QString> in(chain.begin(), chain.end());
  bool grew = true;
  while (grew) {
    grew = false;
    for (const QString& id : QStringList(chain)) {
      const CurveData* c = model.curve(id);
      if (c == nullptr || !joinable(*c)) {
        continue;
      }
      for (const QString& end : {c->start, c->end}) {
        QStringList at;
        for (const CurveData& other : model.curves) {
          if (joinable(other) && other.construction == c->construction &&
              (other.start == end || other.end == end)) {
            at << other.id;
          }
        }
        if (at.size() != 2) {
          continue;
        }
        for (const QString& next : at) {
          if (!in.contains(next)) {
            in.insert(next);
            chain << next;
            grew = true;
          }
        }
      }
    }
  }
  return chain;
}

QStringList doubleClickChain(const SketchModel& model, const QString& curve, const QStringList& loop) {
  if (!loop.isEmpty()) {
    return loop;
  }
  return chainOf(model, {curve});
}

} // namespace mitcad::sketch
