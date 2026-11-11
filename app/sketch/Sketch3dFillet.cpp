// SPDX-License-Identifier: MIT
#include "Sketch3dFillet.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QCoreApplication>
#include <QJsonArray>

namespace mitcad::sketch {
namespace {

// The file's texts in the context Sketch3d, where lupdate records them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(Sketch3d)
};

constexpr double kMinimum = 1e-6;  // mm: the model's shortest segment and radius
constexpr double kStraight = 1e-7; // the sine below which two lines are in line
constexpr double kFits = 0.999;    // a dragged radius stays this far inside the largest

// A line of the sketch that a fillet rounds: an ordinary or a construction
// line with both ends known.
struct LineEnds {
  QString start;
  QString end;
  bool construction = false;
};

std::optional<LineEnds> lineOf(const Sketch3dModel& model, const QString& id) {
  const Sketch3dCurve* curve = model.curve(id);
  if (curve == nullptr || curve->type != QStringLiteral("line")) {
    return std::nullopt;
  }
  LineEnds ends;
  ends.start = curve->json.value(QStringLiteral("start")).toString();
  ends.end = curve->json.value(QStringLiteral("end")).toString();
  ends.construction = curve->construction;
  if (model.point(ends.start) == nullptr || model.point(ends.end) == nullptr || ends.start == ends.end) {
    return std::nullopt;
  }
  return ends;
}

// The corner of two lines at their shared end `point`, when they are not
// in line there.
std::optional<FilletCorner3> cornerOf(const Sketch3dModel& model, const QString& point, const QString& first,
                                      const QString& second, const LineEnds& a, const LineEnds& b) {
  const Sketch3dPoint* at = model.point(point);
  const Sketch3dPoint* farA = model.point(a.start == point ? a.end : a.start);
  const Sketch3dPoint* farB = model.point(b.start == point ? b.end : b.start);
  if (at == nullptr || farA == nullptr || farB == nullptr || farA == farB) {
    return std::nullopt;
  }
  const auto ua = sketch::unit(sub(farA->at, at->at));
  const auto ub = sketch::unit(sub(farB->at, at->at));
  if (!ua || !ub || norm(cross(*ua, *ub)) <= kStraight) {
    return std::nullopt;
  }
  FilletCorner3 corner;
  corner.point = point;
  corner.first = first;
  corner.second = second;
  corner.at = at->at;
  corner.firstEnd = farA->at;
  corner.secondEnd = farB->at;
  return corner;
}

// The shared end of two lines; empty when they share none.
QString sharedEnd(const LineEnds& a, const LineEnds& b) {
  for (const QString& p : {a.start, a.end}) {
    if (p == b.start || p == b.end) {
      return p;
    }
  }
  return QString();
}

// Half the angle between the lines at the corner (radians).
double halfAngle(const FilletCorner3& corner) {
  const auto ua = sketch::unit(sub(corner.firstEnd, corner.at));
  const auto ub = sketch::unit(sub(corner.secondEnd, corner.at));
  if (!ua || !ub) {
    return 0.0;
  }
  return 0.5 * std::acos(std::clamp(dot(*ua, *ub), -1.0, 1.0));
}

QString mm(double value) { return QString::number(value, 'f', 3); }

// A line as the palette names it, "Line 2" (mitcad#1653); "a line" for one
// the sketch lacks. Its id is the log's.
QString lineName(const Sketch3dModel& model, const QString& id) {
  const QString name = entityName(model, id);
  return name.isEmpty() ? Text::tr("a line") : name;
}

} // namespace

std::optional<FilletCorner3> filletCornerAt(const Sketch3dModel& model, const QString& point) {
  if (model.point(point) == nullptr) {
    return std::nullopt;
  }
  QStringList lines;
  for (const Sketch3dCurve& curve : model.curves) {
    if (curve.construction || curve.type != QStringLiteral("line")) {
      continue;
    }
    if (const auto ends = lineOf(model, curve.id); ends && (ends->start == point || ends->end == point)) {
      lines << curve.id;
    }
  }
  if (lines.size() != 2) {
    return std::nullopt;
  }
  return cornerOf(model, point, lines[0], lines[1], *lineOf(model, lines[0]), *lineOf(model, lines[1]));
}

std::optional<FilletCorner3> filletCornerOfLines(const Sketch3dModel& model, const QString& a, const QString& b) {
  const auto first = lineOf(model, a);
  const auto second = lineOf(model, b);
  if (a == b || !first || !second) {
    return std::nullopt;
  }
  const QString point = sharedEnd(*first, *second);
  if (point.isEmpty()) {
    return std::nullopt;
  }
  auto corner = cornerOf(model, point, a, b, *first, *second);
  if (corner) {
    corner->byLines = true;
  }
  return corner;
}

QStringList filletCornerPoints(const Sketch3dModel& model) {
  QStringList points;
  for (const Sketch3dPoint& point : model.points) {
    if (filletCornerAt(model, point.id)) {
      points << point.id;
    }
  }
  return points;
}

QString filletLinesProblem(const Sketch3dModel& model, const QString& a, const QString& b) {
  const auto first = lineOf(model, a);
  const auto second = lineOf(model, b);
  if (!first || !second) {
    return Text::tr("A fillet rounds the corner of two lines.");
  }
  if (a == b) {
    return Text::tr("Pick a second line.");
  }
  const QString point = sharedEnd(*first, *second);
  if (point.isEmpty()) {
    // In words; the ids are the log's (mitcad#1653).
    return Text::tr("%1 and %2 do not share an end: pick two lines that meet.")
        .arg(lineName(model, a), lineName(model, b));
  }
  if (!cornerOf(model, point, a, b, *first, *second)) {
    return Text::tr("%1 and %2 are in line at %3: there is no corner to round.")
        .arg(lineName(model, a), lineName(model, b), entityName(model, point));
  }
  return QString();
}

double filletMaxRadius(const FilletCorner3& corner) {
  const double half = halfAngle(corner);
  const double shorter = std::min(distance(corner.at, corner.firstEnd), distance(corner.at, corner.secondEnd));
  return std::max(0.0, (shorter - kMinimum) * std::tan(half));
}

std::optional<FilletArc> filletArc(const Sketch3dModel& model, const FilletCorner3& corner, double radius,
                                   QString* problem) {
  const auto fail = [problem](const QString& why) -> std::optional<FilletArc> {
    if (problem != nullptr) {
      *problem = why;
    }
    return std::nullopt;
  };
  if (!std::isfinite(radius) || radius <= kMinimum) {
    return fail(Text::tr("The radius must be greater than zero."));
  }
  const auto ua = sketch::unit(sub(corner.firstEnd, corner.at));
  const auto ub = sketch::unit(sub(corner.secondEnd, corner.at));
  const double half = halfAngle(corner);
  if (!ua || !ub || half <= 0.0) {
    return fail(Text::tr("The lines are in line: there is no corner to round."));
  }
  const auto bisector = sketch::unit(add(*ua, *ub));
  if (!bisector) {
    return fail(Text::tr("The lines are in line: there is no corner to round."));
  }
  FilletArc arc;
  arc.radius = radius;
  arc.setback = radius / std::tan(half);
  for (const auto& [line, end] : {std::pair<QString, Xyz>(corner.first, corner.firstEnd),
                                  std::pair<QString, Xyz>(corner.second, corner.secondEnd)}) {
    const double length = distance(corner.at, end);
    if (arc.setback >= length - kMinimum) {
      return fail(Text::tr("A radius of %1 mm does not fit %2: the arc would touch it %3 mm from the corner, "
                           "and it is %4 mm long.")
                      .arg(mm(radius), lineName(model, line), mm(arc.setback), mm(length)));
    }
  }
  arc.center = add(corner.at, scaled(*bisector, radius / std::sin(half)));
  arc.start = add(corner.at, scaled(*ua, arc.setback));
  arc.end = add(corner.at, scaled(*ub, arc.setback));
  const auto normal = sketch::unit(cross(sub(arc.start, arc.center), sub(arc.end, arc.center)));
  if (!normal) {
    return fail(Text::tr("The lines are in line: there is no corner to round."));
  }
  arc.normal = *normal;
  arc.sweep = angleAbout(sub(arc.start, arc.center), sub(arc.end, arc.center), arc.normal);
  if (problem != nullptr) {
    problem->clear();
  }
  return arc;
}

std::optional<double> filletRadiusOnRay(const FilletCorner3& corner, const Ray3& ray) {
  const auto ua = sketch::unit(sub(corner.firstEnd, corner.at));
  const auto ub = sketch::unit(sub(corner.secondEnd, corner.at));
  if (!ua || !ub) {
    return std::nullopt;
  }
  const auto bisector = sketch::unit(add(*ua, *ub));
  const auto normal = sketch::unit(cross(*ua, *ub));
  if (!bisector || !normal) {
    return std::nullopt;
  }
  // Where the ray passes the bisector, else where it meets the corner's
  // plane, taken onto the bisector.
  std::optional<Xyz> place = onLine(ray, corner.at, *bisector);
  if (!place) {
    place = onPlane(ray, corner.at, *normal);
  }
  if (!place) {
    return std::nullopt;
  }
  const double along = dot(sub(*place, corner.at), *bisector);
  const double s = std::sin(halfAngle(corner));
  if (!(along > 0.0) || !(s > 0.0) || s >= 1.0) {
    return std::nullopt;
  }
  // The arc's middle lies r / sin(half) - r from the corner.
  const double radius = along * s / (1.0 - s);
  const double largest = filletMaxRadius(corner) * kFits;
  if (!(largest > kMinimum)) {
    return std::nullopt;
  }
  return std::min(radius, largest);
}

std::vector<Xyz> filletArcPoints(const FilletArc& arc, int segments) {
  ArcShape shape;
  shape.center = arc.center;
  shape.normal = arc.normal;
  shape.radius = arc.radius;
  shape.sweep = arc.sweep;
  return arcPoints(shape, arc.start, segments);
}

QJsonObject filletCommandFor(const QString& sketch, const FilletCorner3& corner, const QJsonValue& radius) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.fillet")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("radius"), radius}};
  if (corner.byLines) {
    cmd.insert(QStringLiteral("a"), corner.first);
    cmd.insert(QStringLiteral("b"), corner.second);
  } else {
    cmd.insert(QStringLiteral("points"), QJsonArray{corner.point});
  }
  return cmd;
}

} // namespace mitcad::sketch
