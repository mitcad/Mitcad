// SPDX-License-Identifier: MIT
// Toolpaths as segments in the design and the tool's places along them
// (mitcad#1013; CamToolpaths.hpp).
#include "CamToolpaths.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonValue>

namespace mitcad::cam {
namespace {

constexpr double kPi = 3.14159265358979323846;
// Arcs are drawn in steps of at most this angle.
constexpr double kArcStep = 5.0 * kPi / 180.0;

Point3 pointOf(const QJsonValue& value, const Point3& fallback) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() != 3) {
    return fallback;
  }
  return {xyz.at(0).toDouble(), xyz.at(1).toDouble(), xyz.at(2).toDouble()};
}

double length(const Point3& v) { return std::sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]); }

Point3 unit(const Point3& v, const Point3& fallback) {
  const double l = length(v);
  if (!(l > 1e-12)) {
    return fallback;
  }
  return {v[0] / l, v[1] / l, v[2] / l};
}

void addSegment(std::vector<float>& out, const Point3& a, const Point3& b) {
  for (const Point3* p : {&a, &b}) {
    for (int k = 0; k < 3; ++k) {
      out.push_back(static_cast<float>((*p)[static_cast<std::size_t>(k)]));
    }
  }
}

// The work points a move passes through after `from`: its end, and an
// arc's points on the way.
std::vector<Point3> movePoints(const QJsonObject& move, const Point3& from, const Point3& to) {
  if (move.value(QStringLiteral("kind")).toString() != QStringLiteral("arc")) {
    return {to};
  }
  const QJsonArray center = move.value(QStringLiteral("center")).toArray();
  if (center.size() != 2) {
    return {to};
  }
  return arcPoints(from, to, {center.at(0).toDouble(), center.at(1).toDouble()},
                   move.value(QStringLiteral("ccw")).toBool());
}

} // namespace

WorkFrame WorkFrame::of(const QJsonObject& work) {
  WorkFrame frame;
  if (work.isEmpty() || work.contains(QStringLiteral("error"))) {
    return frame;
  }
  frame.origin = pointOf(work.value(QStringLiteral("origin")), frame.origin);
  frame.x = unit(pointOf(work.value(QStringLiteral("x")), frame.x), {1.0, 0.0, 0.0});
  frame.y = unit(pointOf(work.value(QStringLiteral("y")), frame.y), {0.0, 1.0, 0.0});
  frame.z = unit(pointOf(work.value(QStringLiteral("z")), frame.z), {0.0, 0.0, 1.0});
  frame.valid = work.contains(QStringLiteral("origin"));
  return frame;
}

Point3 WorkFrame::toModel(const Point3& p) const {
  const Point3 d = directionToModel(p);
  return {origin[0] + d[0], origin[1] + d[1], origin[2] + d[2]};
}

Point3 WorkFrame::directionToModel(const Point3& d) const {
  Point3 out{};
  for (std::size_t k = 0; k < 3; ++k) {
    out[k] = x[k] * d[0] + y[k] * d[1] + z[k] * d[2];
  }
  return out;
}

MoveStyle styleOf(const QString& kind, const Point3& from, const Point3& to) {
  if (kind == QStringLiteral("rapid")) {
    return MoveStyle::Rapid;
  }
  return to[2] < from[2] - 1e-6 ? MoveStyle::Entry : MoveStyle::Feed;
}

std::vector<Point3> arcPoints(const Point3& from, const Point3& to, const std::array<double, 2>& center, bool ccw) {
  const double fx = from[0] - center[0];
  const double fy = from[1] - center[1];
  const double tx = to[0] - center[0];
  const double ty = to[1] - center[1];
  const double radius = std::hypot(fx, fy);
  if (!(radius > 1e-9)) {
    return {to};
  }
  const double start = std::atan2(fy, fx);
  const double end = std::atan2(ty, tx);
  const bool fullTurn = std::hypot(to[0] - from[0], to[1] - from[1]) < 1e-9;
  double sweep = end - start;
  if (ccw) {
    while (sweep <= 0.0) {
      sweep += 2.0 * kPi;
    }
    if (fullTurn) {
      sweep = 2.0 * kPi;
    }
  } else {
    while (sweep >= 0.0) {
      sweep -= 2.0 * kPi;
    }
    if (fullTurn) {
      sweep = -2.0 * kPi;
    }
  }
  // Whole steps of 5 degrees stay whole (a quarter is 18, not 19).
  const int steps = std::clamp(static_cast<int>(std::ceil(std::abs(sweep) / kArcStep - 1e-9)), 1, 720);
  std::vector<Point3> points;
  points.reserve(static_cast<std::size_t>(steps));
  for (int i = 1; i < steps; ++i) {
    const double t = static_cast<double>(i) / steps;
    const double angle = start + sweep * t;
    points.push_back({center[0] + radius * std::cos(angle), center[1] + radius * std::sin(angle),
                      from[2] + (to[2] - from[2]) * t});
  }
  points.push_back(to);
  return points;
}

PathLines pathLines(const QJsonArray& moves, const WorkFrame& frame, std::optional<Point3>& at) {
  PathLines lines;
  for (const QJsonValue& value : moves) {
    const QJsonObject move = value.toObject();
    const QString kind = move.value(QStringLiteral("kind")).toString();
    const Point3 to = pointOf(move.value(QStringLiteral("to")), at.value_or(Point3{0.0, 0.0, 0.0}));
    ++lines.moves;
    if (!at) {
      at = to;
      ++lines.rapidMoves;
      continue;
    }
    const Point3 from = *at;
    const MoveStyle style = styleOf(kind, from, to);
    std::vector<float>& out = style == MoveStyle::Rapid  ? lines.rapids
                              : style == MoveStyle::Entry ? lines.entries
                                                          : lines.feeds;
    switch (style) {
    case MoveStyle::Rapid:
      ++lines.rapidMoves;
      break;
    case MoveStyle::Entry:
      ++lines.entryMoves;
      break;
    case MoveStyle::Feed:
      ++lines.feedMoves;
      break;
    }
    Point3 previous = frame.toModel(from);
    for (const Point3& point : movePoints(move, from, to)) {
      const Point3 here = frame.toModel(point);
      addSegment(out, previous, here);
      previous = here;
    }
    at = to;
  }
  return lines;
}

void appendPlayback(const QJsonArray& moves, const WorkFrame& frame, int operation, std::optional<Point3>& at,
                    std::vector<PlaybackPoint>& points) {
  int index = 0;
  for (const QJsonValue& value : moves) {
    const QJsonObject move = value.toObject();
    const Point3 to = pointOf(move.value(QStringLiteral("to")), at.value_or(Point3{0.0, 0.0, 0.0}));
    const std::vector<Point3> way = at ? movePoints(move, *at, to) : std::vector<Point3>{to};
    for (const Point3& point : way) {
      points.push_back({frame.toModel(point), operation, index});
    }
    at = to;
    ++index;
  }
}

bool ToolpathScene::empty() const {
  return !tool && std::all_of(groups.begin(), groups.end(), [](const Group& group) { return group.segments.empty(); });
}

QString ToolpathScene::log() const {
  if (empty()) {
    return QStringLiteral("CAM toolpath overlay: none");
  }
  int total = 0;
  int rapid = 0;
  int feed = 0;
  int entry = 0;
  int stale = 0;
  int shown = 0;
  for (const Group& group : groups) {
    const int count = static_cast<int>(group.segments.size() / 6);
    if (count == 0) {
      continue;
    }
    ++shown;
    total += count;
    switch (group.look) {
    case Look::Rapid:
    case Look::CurrentRapid:
      rapid += count;
      break;
    case Look::Feed:
    case Look::Current:
      feed += count;
      break;
    case Look::Entry:
      entry += count;
      break;
    case Look::Stale:
      stale += count;
      break;
    }
  }
  QString line = QStringLiteral("CAM toolpath overlay: %1 groups, %2 segments (rapid %3, feed %4, entry %5, stale %6)")
                     .arg(shown)
                     .arg(total)
                     .arg(rapid)
                     .arg(feed)
                     .arg(entry)
                     .arg(stale);
  if (tool) {
    line += QStringLiteral("; tool at %1, %2, %3")
                .arg(shortNumber(tool->tip[0]), shortNumber(tool->tip[1]), shortNumber(tool->tip[2]));
  }
  return line;
}

QString pathLinesLog(const QString& name, const PathLines& lines) {
  return QStringLiteral("%1: %2 moves (rapid %3, feed %4, entry %5), %6 segments")
      .arg(name)
      .arg(lines.moves)
      .arg(lines.rapidMoves)
      .arg(lines.feedMoves)
      .arg(lines.entryMoves)
      .arg(lines.segments());
}

QString shortNumber(double value) {
  QString text = QString::number(value, 'f', 3);
  while (text.contains(QLatin1Char('.')) && (text.endsWith(QLatin1Char('0')) || text.endsWith(QLatin1Char('.')))) {
    text.chop(1);
  }
  return text == QStringLiteral("-0") ? QStringLiteral("0") : text;
}

} // namespace mitcad::cam
