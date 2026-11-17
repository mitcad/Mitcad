// SPDX-License-Identifier: MIT
#include "GravityLogic.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <utility>

namespace mitcad::gravity {
namespace {

bool finite(const Vec3& a) { return std::isfinite(a[0]) && std::isfinite(a[1]) && std::isfinite(a[2]); }

QString number(double value, int decimals) {
  // No "-0.000" in a log line.
  const QString text = QString::number(value, 'f', decimals);
  const QString zero = QString::number(0.0, 'f', decimals);
  return text == QStringLiteral("-") + zero ? zero : text;
}

} // namespace

Vec3 add(const Vec3& a, const Vec3& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
Vec3 sub(const Vec3& a, const Vec3& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
Vec3 scaled(const Vec3& a, double k) { return {a[0] * k, a[1] * k, a[2] * k}; }
double dot(const Vec3& a, const Vec3& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
Vec3 cross(const Vec3& a, const Vec3& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
double length(const Vec3& a) { return std::sqrt(dot(a, a)); }

std::optional<Vec3> unit(const Vec3& a) {
  const double size = length(a);
  if (!std::isfinite(size) || size < 1e-12) {
    return std::nullopt;
  }
  return scaled(a, 1.0 / size);
}

std::optional<Vec3> vecOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  if (array.size() != 3) {
    return std::nullopt;
  }
  Vec3 a{};
  for (int i = 0; i < 3; ++i) {
    if (!array.at(i).isDouble()) {
      return std::nullopt;
    }
    a[static_cast<std::size_t>(i)] = array.at(i).toDouble();
  }
  if (!finite(a)) {
    return std::nullopt;
  }
  return a;
}

QJsonArray toJson(const Vec3& a) { return QJsonArray{a[0], a[1], a[2]}; }

// ---------------------------------------------------------------------------
// The direction

QString sourceKind(const QJsonValue& geometry) {
  if (geometry.isString()) {
    const QString text = geometry.toString();
    if (text == QStringLiteral("xy") || text == QStringLiteral("xz") || text == QStringLiteral("yz")) {
      return QStringLiteral("plane");
    }
    if (text == QStringLiteral("x") || text == QStringLiteral("y") || text == QStringLiteral("z")) {
      return QStringLiteral("axis");
    }
    return QStringLiteral("construction");
  }
  const QJsonObject ref = geometry.toObject();
  if (ref.contains(QStringLiteral("face"))) {
    return QStringLiteral("face");
  }
  if (ref.contains(QStringLiteral("edge"))) {
    return QStringLiteral("edge");
  }
  if (ref.contains(QStringLiteral("curve"))) {
    return QStringLiteral("line");
  }
  if (ref.contains(QStringLiteral("normal"))) {
    return QStringLiteral("plane");
  }
  if (ref.contains(QStringLiteral("direction"))) {
    return QStringLiteral("axis");
  }
  return QString();
}

Direction Direction::read(const QJsonObject& query) {
  Direction out;
  out.hanging = query.value(QStringLiteral("hanging")).toString();
  out.direction = vecOf(query.value(QStringLiteral("direction")));
  if (!out.direction) {
    return out;
  }
  const QJsonObject source = query.value(QStringLiteral("source")).toObject();
  out.sourceJson = source;
  if (!source.isEmpty()) {
    out.source = sourceKind(source.value(QStringLiteral("geometry")));
    out.reversed = source.value(QStringLiteral("reverse")).toBool();
    if (const std::optional<Vec3> now = vecOf(query.value(QStringLiteral("source_direction")))) {
      out.sourceMoved = length(sub(*now, *out.direction)) > 1e-6;
    } else {
      // What it was taken from is gone: the stored direction stays.
      out.sourceMoved = true;
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// A Flex item let hang

std::optional<Hanging> Hanging::read(const QJsonObject& answer) {
  Hanging out;
  out.flex = answer.value(QStringLiteral("flex")).toString();
  if (out.flex.isEmpty()) {
    return std::nullopt;
  }
  // The points and the section's y at each; the lengths along the chain
  // and its directions from the points.
  const QJsonArray polyline = answer.value(QStringLiteral("polyline")).toArray();
  const QJsonArray normals = answer.value(QStringLiteral("normals")).toArray();
  for (qsizetype i = 0; i < polyline.size(); ++i) {
    const std::optional<Vec3> point = vecOf(polyline.at(i));
    if (!point) {
      continue;
    }
    Frame frame;
    frame.point = *point;
    if (i < normals.size()) {
      frame.normal = vecOf(normals.at(i)).value_or(frame.normal);
    }
    if (!out.frames.empty()) {
      frame.along = out.frames.back().along + gravity::length(sub(frame.point, out.frames.back().point));
    }
    out.frames.push_back(frame);
  }
  const std::size_t count = out.frames.size();
  if (count < 2) {
    return std::nullopt;
  }
  for (std::size_t i = 0; i < count; ++i) {
    const Vec3& before = out.frames[i == 0 ? 0 : i - 1].point;
    const Vec3& after = out.frames[std::min(i + 1, count - 1)].point;
    out.frames[i].tangent = unit(sub(after, before)).value_or(Vec3{1.0, 0.0, 0.0});
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("touching")).toArray()) {
    out.touching.push_back(value.toBool());
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("nodes")).toArray()) {
    const QJsonObject entry = value.toObject();
    HangNode node;
    node.id = entry.value(QStringLiteral("id")).toString();
    if (node.id.isEmpty()) {
      continue;
    }
    node.index = entry.value(QStringLiteral("index")).toInt();
    node.point = vecOf(entry.value(QStringLiteral("point"))).value_or(Vec3{});
    node.along = entry.value(QStringLiteral("along")).toDouble();
    node.pinned = entry.value(QStringLiteral("pinned")).toBool();
    node.support = entry.value(QStringLiteral("support")).toString();
    node.moved = entry.value(QStringLiteral("moved")).toBool();
    out.nodes.push_back(node);
  }
  out.length = answer.value(QStringLiteral("length")).toDouble();
  out.segment = answer.value(QStringLiteral("segment")).toDouble();
  const QJsonValue grabbed = answer.value(QStringLiteral("grabbed"));
  if (grabbed.isDouble() && std::isfinite(grabbed.toDouble())) {
    out.grabbed = grabbed.toDouble();
  }
  out.atRest = answer.value(QStringLiteral("at_rest")).toBool();
  out.time = answer.value(QStringLiteral("time")).toDouble();
  out.taut = answer.value(QStringLiteral("taut")).toBool();
  out.damping = answer.value(QStringLiteral("damping")).toDouble(out.damping);
  return out;
}

QStringList Hanging::pinsWithEnds(bool start, bool end) const {
  QStringList out;
  const std::size_t count = nodes.size();
  for (std::size_t i = 0; i < count; ++i) {
    const bool pinned = i == 0 ? start : i + 1 == count ? end : nodes[i].pinned;
    if (pinned) {
      out << nodes[i].id;
    }
  }
  return out;
}

bool Hanging::startPinned() const { return !nodes.empty() && nodes.front().pinned; }

bool Hanging::endPinned() const { return !nodes.empty() && nodes.back().pinned; }

QStringList Hanging::pins() const {
  QStringList out;
  for (const HangNode& node : nodes) {
    if (node.pinned) {
      out << node.id;
    }
  }
  return out;
}

QStringList Hanging::toggled(const QString& id) const {
  QStringList out;
  for (const HangNode& node : nodes) {
    if (node.pinned != (node.id == id)) {
      out << node.id;
    }
  }
  return out;
}

const HangNode* Hanging::node(const QString& id) const {
  const auto found = std::find_if(nodes.begin(), nodes.end(), [&id](const HangNode& n) { return n.id == id; });
  return found != nodes.end() ? &*found : nullptr;
}

int Hanging::touchingCount() const {
  return static_cast<int>(std::count(touching.begin(), touching.end(), true));
}

int Hanging::nearest(const Vec3& point) const {
  int best = -1;
  double nearest = std::numeric_limits<double>::infinity();
  for (std::size_t i = 0; i < frames.size(); ++i) {
    const double d = gravity::length(sub(frames[i].point, point));
    if (d < nearest) {
      nearest = d;
      best = static_cast<int>(i);
    }
  }
  return best;
}

QJsonObject hangCommand(const QString& flex, std::optional<double> damping) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("flex.hang")}, {QStringLiteral("flex"), flex}};
  if (damping) {
    command.insert(QStringLiteral("damping"), *damping);
  }
  return command;
}

QJsonObject hangStepCommand(double seconds, const std::optional<std::pair<Vec3, Vec3>>& grab) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("flex.hang_step")}, {QStringLiteral("seconds"), seconds}};
  if (grab) {
    command.insert(QStringLiteral("grab"), QJsonObject{{QStringLiteral("point"), toJson(grab->first)},
                                                       {QStringLiteral("target"), toJson(grab->second)}});
  }
  return command;
}

QJsonObject hangPinsCommand(const QStringList& pins) {
  return {{QStringLiteral("cmd"), QStringLiteral("flex.hang_set")},
          {QStringLiteral("pins"), QJsonArray::fromStringList(pins)}};
}

QJsonObject hangLengthCommand(double length) {
  return {{QStringLiteral("cmd"), QStringLiteral("flex.hang_set")}, {QStringLiteral("length"), length}};
}

QJsonObject hangDampingCommand(double damping) {
  return {{QStringLiteral("cmd"), QStringLiteral("flex.hang_set")}, {QStringLiteral("damping"), damping}};
}

QJsonObject hangLockCommand() { return {{QStringLiteral("cmd"), QStringLiteral("flex.hang_lock")}}; }

QJsonObject hangCancelCommand() { return {{QStringLiteral("cmd"), QStringLiteral("flex.hang_cancel")}}; }

// ---------------------------------------------------------------------------
// What the panel wants

QJsonObject HangWish::toJson() const {
  QJsonObject out{{QStringLiteral("flex"), flex},
                  {QStringLiteral("damping"), damping},
                  {QStringLiteral("start_pinned"), startPinned},
                  {QStringLiteral("end_pinned"), endPinned}};
  if (gravity) {
    out.insert(QStringLiteral("gravity"), *gravity);
  }
  if (length) {
    out.insert(QStringLiteral("length"), *length);
  }
  return out;
}

std::optional<HangWish> HangWish::read(const QJsonObject& definition) {
  HangWish wish;
  wish.flex = definition.value(QStringLiteral("flex")).toString();
  if (wish.flex.isEmpty()) {
    return std::nullopt;
  }
  const QJsonValue gravity = definition.value(QStringLiteral("gravity"));
  if (gravity.isObject()) {
    wish.gravity = gravity.toObject();
  }
  const QJsonValue length = definition.value(QStringLiteral("length"));
  if (length.isDouble() && std::isfinite(length.toDouble()) && length.toDouble() > 0.0) {
    wish.length = length.toDouble();
  }
  wish.damping = std::clamp(definition.value(QStringLiteral("damping")).toDouble(wish.damping), 0.0, 1.0);
  wish.startPinned = definition.value(QStringLiteral("start_pinned")).toBool(true);
  wish.endPinned = definition.value(QStringLiteral("end_pinned")).toBool(true);
  return wish;
}

bool sameDirection(const Direction& design, const QJsonObject& setCommand) {
  if (!design.direction || design.sourceMoved || design.sourceJson.isEmpty()) {
    return false;
  }
  const QJsonObject& source = design.sourceJson;
  return source.value(QStringLiteral("geometry")) == setCommand.value(QStringLiteral("geometry")) &&
         source.value(QStringLiteral("occurrence")).toString() ==
             setCommand.value(QStringLiteral("occurrence")).toString() &&
         source.value(QStringLiteral("reverse")).toBool() == setCommand.value(QStringLiteral("reverse")).toBool();
}

// ---------------------------------------------------------------------------
// The steps

double stepSeconds(double elapsed) {
  if (!std::isfinite(elapsed)) {
    return 1.0 / 60.0;
  }
  return std::clamp(elapsed, 1.0 / 240.0, kStepMostSeconds);
}

void StepTimes::add(double milliseconds) {
  ++m_count;
  m_total += milliseconds;
  m_most = std::max(m_most, milliseconds);
}

QString StepTimes::text() const {
  if (m_count == 0) {
    return QStringLiteral("no steps timed");
  }
  return QStringLiteral("steps of %1 ms on average, %2 ms at most")
      .arg(number(m_total / m_count, 1), number(m_most, 1));
}

bool StepGate::begin() {
  if (m_busy) {
    m_missed = true;
    ++m_skipped;
    return false;
  }
  m_busy = true;
  m_missed = false;
  ++m_started;
  return true;
}

void StepGate::end() { m_busy = false; }

void StepGate::reset() { *this = StepGate(); }

// ---------------------------------------------------------------------------
// Commands of the direction

QJsonObject setCommand(const QJsonValue& geometry, const QString& occurrence, bool reverse) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("gravity.set")}, {QStringLiteral("geometry"), geometry}};
  if (!occurrence.isEmpty()) {
    command.insert(QStringLiteral("occurrence"), occurrence);
  }
  if (reverse) {
    command.insert(QStringLiteral("reverse"), true);
  }
  return command;
}

QJsonObject clearCommand() { return {{QStringLiteral("cmd"), QStringLiteral("gravity.clear")}}; }

// ---------------------------------------------------------------------------
// The press

PressRoute routePress(const PressInput& press) {
  if (press.dragging) {
    return PressRoute::Swallow;
  }
  if (!press.left || !press.viewArea) {
    return PressRoute::View;
  }
  if (press.onNode) {
    return PressRoute::TogglePin;
  }
  if (press.onChain) {
    return PressRoute::Grab;
  }
  return PressRoute::View;
}

// ---------------------------------------------------------------------------
// Drawn and logged

ScreenArrow screenArrow(const Vec3& direction, const Vec3& view, const Vec3& up) {
  ScreenArrow arrow;
  const std::optional<Vec3> d = unit(direction);
  const std::optional<Vec3> v = unit(view);
  const std::optional<Vec3> u = unit(up);
  if (!d || !v || !u) {
    arrow.endOn = true;
    return arrow;
  }
  // The screen's right: the view's direction across its up.
  const Vec3 right = unit(cross(*v, *u)).value_or(Vec3{1.0, 0.0, 0.0});
  arrow.x = dot(*d, right);
  arrow.y = dot(*d, *u);
  arrow.away = dot(*d, *v) > 0.0;
  arrow.endOn = std::hypot(arrow.x, arrow.y) < 0.25;
  return arrow;
}

QString vectorText(const Vec3& a, int decimals) {
  return QStringLiteral("%1,%2,%3").arg(number(a[0], decimals), number(a[1], decimals), number(a[2], decimals));
}

QString hangText(const Hanging& hanging, const Vec3& down) {
  // The lowest point: the farthest along gravity.
  double lowest = -std::numeric_limits<double>::infinity();
  for (const Hanging::Frame& frame : hanging.frames) {
    lowest = std::max(lowest, dot(frame.point, down));
  }
  return QStringLiteral("points %1, touching %2, pins %3, length %4, lowest %5 along %6, at rest %7%8")
      .arg(static_cast<int>(hanging.frames.size()))
      .arg(hanging.touchingCount())
      .arg(hanging.pins().isEmpty() ? QStringLiteral("none") : hanging.pins().join(QLatin1Char(',')),
           number(hanging.length, 1), number(-lowest, 1), vectorText(down),
           hanging.atRest ? QStringLiteral("yes") : QStringLiteral("no"),
           hanging.taut ? QStringLiteral(", pulled tight") : QString());
}

} // namespace mitcad::gravity
