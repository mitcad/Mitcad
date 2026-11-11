// SPDX-License-Identifier: MIT
#include "Sketch3dDraw.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QCoreApplication>

namespace mitcad::sketch {
namespace {

// The file's texts in the context Sketch3d, where lupdate records them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(Sketch3d)
};

constexpr double kPi = 3.14159265358979323846;
constexpr double kTolerance = 1e-6; // mm: the geometric comparisons' baseline
constexpr double kTiePixels = 0.5;  // screen distances this close are equal

QJsonArray xyzJson(const Xyz& a) { return QJsonArray{a[0], a[1], a[2]}; }

int kindOrder(SnapKind kind) {
  switch (kind) {
  case SnapKind::Point:
    return 0;
  case SnapKind::Origin:
    return 1;
  case SnapKind::LinkedPoint:
    return 2;
  case SnapKind::Center:
    return 3;
  case SnapKind::Midpoint:
    return 4;
  case SnapKind::OnCurve:
  case SnapKind::OnEdge:
    return 5;
  case SnapKind::Guide:
    return 6;
  case SnapKind::OnFace:
    return 7;
  case SnapKind::Free:
    break;
  }
  return 8;
}

// Discrete points 0, points on curves 1, guidance 2, a face 3.
int snapClass(const SnapTarget& target) {
  if (target.discrete()) {
    return 0;
  }
  if (target.continuous()) {
    return 1;
  }
  return target.kind == SnapKind::OnFace ? 3 : 2;
}

QString offsetLabel(int axis) {
  switch (axis) {
  case 0:
    return Text::tr("Delta X");
  case 1:
    return Text::tr("Delta Y");
  default:
    return Text::tr("Delta Z");
  }
}

QString shown(double value) { return QString::number(std::abs(value) < 5e-7 ? 0.0 : value, 'f', 2); }

// A clamped uniform B-spline's point at `t` in [0, 1] (de Boor).
Xyz deBoor(const std::vector<Xyz>& control, int degree, const std::vector<double>& knots, double t) {
  const int n = static_cast<int>(control.size());
  int span = degree;
  while (span < n - 1 && t >= knots[static_cast<std::size_t>(span) + 1]) {
    ++span;
  }
  std::vector<Xyz> d;
  for (int j = 0; j <= degree; ++j) {
    d.push_back(control[static_cast<std::size_t>(span - degree + j)]);
  }
  for (int r = 1; r <= degree; ++r) {
    for (int j = degree; j >= r; --j) {
      const std::size_t i = static_cast<std::size_t>(span - degree + j);
      const double left = knots[i];
      const double right = knots[i + static_cast<std::size_t>(degree) + 1 - static_cast<std::size_t>(r)];
      const double alpha = right - left > 0.0 ? (t - left) / (right - left) : 0.0;
      const std::size_t k = static_cast<std::size_t>(j);
      d[k] = add(scaled(d[k - 1], 1.0 - alpha), scaled(d[k], alpha));
    }
  }
  return d[static_cast<std::size_t>(degree)];
}

} // namespace

// ---------------------------------------------------------------------------
// Vectors

Xyz add(const Xyz& a, const Xyz& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
Xyz sub(const Xyz& a, const Xyz& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
Xyz scaled(const Xyz& a, double factor) { return {a[0] * factor, a[1] * factor, a[2] * factor}; }
double dot(const Xyz& a, const Xyz& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
Xyz cross(const Xyz& a, const Xyz& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
double norm(const Xyz& a) { return std::sqrt(dot(a, a)); }
double distance(const Xyz& a, const Xyz& b) { return norm(sub(a, b)); }

std::optional<Xyz> unit(const Xyz& a) {
  const double length = norm(a);
  if (!(length > 1e-12) || !std::isfinite(length)) {
    return std::nullopt;
  }
  return scaled(a, 1.0 / length);
}

bool finite(const Xyz& a) { return std::isfinite(a[0]) && std::isfinite(a[1]) && std::isfinite(a[2]); }

Xyz rotated(const Xyz& a, const Xyz& axis, double angle) {
  // Rodrigues' rotation.
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  return add(add(scaled(a, c), scaled(cross(axis, a), s)), scaled(axis, dot(axis, a) * (1.0 - c)));
}

double angleAbout(const Xyz& a, const Xyz& b, const Xyz& axis) {
  double angle = std::atan2(dot(cross(a, b), axis), dot(a, b));
  if (angle < 0.0) {
    angle += 2.0 * kPi;
  }
  return angle;
}

Xyz axisVector(Axis3 axis) {
  switch (axis) {
  case Axis3::X:
    return {1.0, 0.0, 0.0};
  case Axis3::Y:
    return {0.0, 1.0, 0.0};
  case Axis3::Z:
    break;
  }
  return {0.0, 0.0, 1.0};
}

QString axisKey(Axis3 axis) {
  return axis == Axis3::X ? QStringLiteral("x") : axis == Axis3::Y ? QStringLiteral("y") : QStringLiteral("z");
}

QString axisLabel(Axis3 axis) { return axisKey(axis).toUpper(); }

Axis3 planeNormalAxis(Plane3 plane) {
  return plane == Plane3::XY ? Axis3::Z : plane == Plane3::YZ ? Axis3::X : Axis3::Y;
}

Xyz planeNormal(Plane3 plane) { return axisVector(planeNormalAxis(plane)); }

std::array<Axis3, 2> planeAxes(Plane3 plane) {
  switch (plane) {
  case Plane3::XY:
    return {Axis3::X, Axis3::Y};
  case Plane3::YZ:
    return {Axis3::Y, Axis3::Z};
  case Plane3::ZX:
    break;
  }
  return {Axis3::Z, Axis3::X};
}

QString planeLabel(Plane3 plane) {
  return plane == Plane3::XY ? QStringLiteral("XY") : plane == Plane3::YZ ? QStringLiteral("YZ") : QStringLiteral("ZX");
}

// ---------------------------------------------------------------------------
// The manipulator's rotation (mitcad#1661)

Frame3 sketchFrame() { return {axisVector(Axis3::X), axisVector(Axis3::Y), axisVector(Axis3::Z)}; }

bool isSketchFrame(const Frame3& frame) {
  const Frame3 own = sketchFrame();
  for (std::size_t k = 0; k < 3; ++k) {
    if (distance(frame[k], own[k]) > 1e-9) {
      return false;
    }
  }
  return true;
}

namespace {

// A component within 1e-12 of 0 or ±1 made exact.
Xyz cleaned(const Xyz& a) {
  Xyz out = a;
  for (double& c : out) {
    if (std::abs(c) < 1e-12) {
      c = 0.0;
    } else if (std::abs(c - 1.0) < 1e-12) {
      c = 1.0;
    } else if (std::abs(c + 1.0) < 1e-12) {
      c = -1.0;
    }
  }
  return out;
}

} // namespace

Frame3 turnedFrame(const Frame3& frame, Axis3 about, double angle) {
  const auto k = static_cast<std::size_t>(about);
  const std::size_t next = (k + 1) % 3;
  const std::size_t last = (k + 2) % 3;
  const Xyz axis = unit(frame[k]).value_or(axisVector(about));
  Frame3 out{};
  out[k] = axis;
  out[next] = rotated(frame[next], axis, angle);
  // Square again (Gram-Schmidt), so that many small turns do not drift.
  const Xyz across = sub(out[next], scaled(axis, dot(out[next], axis)));
  out[next] = unit(across).value_or(frame[next]);
  out[last] = cross(out[k], out[next]);
  for (Xyz& a : out) {
    a = cleaned(a);
  }
  return out;
}

Xyz inFrame(const Frame3& frame, const Xyz& v) { return {dot(v, frame[0]), dot(v, frame[1]), dot(v, frame[2])}; }

Xyz outOfFrame(const Frame3& frame, const Xyz& v) {
  return add(add(scaled(frame[0], v[0]), scaled(frame[1], v[1])), scaled(frame[2], v[2]));
}

QJsonObject frameJson(const Frame3& frame) {
  const auto json = [](const Xyz& v) { return QJsonArray{v[0], v[1], v[2]}; };
  return QJsonObject{{QStringLiteral("x"), json(frame[0])}, {QStringLiteral("normal"), json(frame[2])}};
}

Xyz placementAxis(const Placement& placement, Axis3 axis) { return placement.frame[static_cast<std::size_t>(axis)]; }

Xyz placementNormal(const Placement& placement) { return placementAxis(placement, planeNormalAxis(placement.plane)); }

std::array<Xyz, 2> placementPlaneAxes(const Placement& placement) {
  const std::array<Axis3, 2> axes = planeAxes(placement.plane);
  return {placementAxis(placement, axes[0]), placementAxis(placement, axes[1])};
}

std::vector<Xyz> handleRing(const Xyz& center, const Frame3& frame, Axis3 about, double radius, int segments) {
  const auto k = static_cast<std::size_t>(about);
  const Xyz& u = frame[(k + 1) % 3];
  const Xyz& v = frame[(k + 2) % 3];
  std::vector<Xyz> points;
  const int count = std::max(segments, 3);
  points.reserve(static_cast<std::size_t>(count) + 1);
  for (int i = 0; i <= count; ++i) {
    const double t = 2.0 * kPi * i / count;
    points.push_back(add(center, add(scaled(u, radius * std::cos(t)), scaled(v, radius * std::sin(t)))));
  }
  return points;
}

std::optional<double> handleTurn(const Ray3& ray, const Xyz& center, const Xyz& axis, const Xyz& grabbed) {
  const std::optional<Xyz> at = onPlane(ray, center, axis);
  if (!at) {
    return std::nullopt;
  }
  // Both in the ring's plane, from its center.
  const Xyz from = sub(sub(grabbed, center), scaled(axis, dot(sub(grabbed, center), axis)));
  const Xyz to = sub(*at, center);
  if (norm(from) <= 1e-9 || norm(to) <= 1e-9) {
    return std::nullopt;
  }
  double angle = std::atan2(dot(cross(from, to), axis), dot(from, to));
  if (angle <= -kPi) {
    angle += 2.0 * kPi;
  }
  return angle;
}

double wholeDegrees(double radians) { return std::round(radians * 180.0 / kPi) * kPi / 180.0; }

QString degreesText(double radians) {
  double degrees = radians * 180.0 / kPi;
  degrees = std::round(degrees * 1000.0) / 1000.0;
  if (degrees == 0.0) {
    degrees = 0.0; // no "-0°"
  }
  return QString::number(degrees, 'g', 10) + QChar(0x00B0);
}

// ---------------------------------------------------------------------------
// Placement

std::optional<Xyz> onPlane(const Ray3& ray, const Xyz& through, const Xyz& normal) {
  const double along = dot(ray.direction, normal);
  if (std::abs(along) < kEdgeOnSine) {
    return std::nullopt; // seen edge-on: a small move would jump far
  }
  const double reach = dot(sub(through, ray.from), normal) / along;
  if (reach < 0.0) {
    return std::nullopt; // behind the eye
  }
  return add(ray.from, scaled(ray.direction, reach));
}

std::optional<Xyz> onLine(const Ray3& ray, const Xyz& through, const Xyz& direction) {
  // The nearest points of the line through + s·u and the ray from + t·v.
  const Xyz& u = direction;
  const Xyz& v = ray.direction;
  const Xyz w = sub(through, ray.from);
  const double b = dot(u, v);
  const double denominator = 1.0 - b * b;
  if (denominator < kEdgeOnSine * kEdgeOnSine) {
    return std::nullopt; // seen end-on
  }
  const double d = dot(u, w);
  const double e = dot(v, w);
  const double s = (b * e - d) / denominator;
  const double t = (e - b * d) / denominator;
  if (t < 0.0) {
    return std::nullopt; // behind the eye
  }
  return add(through, scaled(u, s));
}

QString placementText(const Placement& placement) {
  const bool turned = placement.rotated();
  switch (placement.mode) {
  case Placement::Mode::Plane:
    return turned ? Text::tr("on the rotated %1 plane").arg(planeLabel(placement.plane))
                  : Text::tr("on the %1 plane").arg(planeLabel(placement.plane));
  case Placement::Mode::Axis:
    return turned ? Text::tr("along the rotated %1").arg(axisLabel(placement.axis))
                  : Text::tr("along %1").arg(axisLabel(placement.axis));
  case Placement::Mode::Normal:
    break;
  }
  return turned ? Text::tr("along the normal of the rotated %1").arg(planeLabel(placement.plane))
                : Text::tr("along the normal of %1").arg(planeLabel(placement.plane));
}

std::optional<Xyz> placePoint(const Ray3& ray, const Xyz& anchor, const Placement& placement) {
  switch (placement.mode) {
  case Placement::Mode::Plane:
    return onPlane(ray, anchor, placementNormal(placement));
  case Placement::Mode::Axis:
    return onLine(ray, anchor, placementAxis(placement, placement.axis));
  case Placement::Mode::Normal:
    break;
  }
  return onLine(ray, placement.normalBase.value_or(anchor), placementNormal(placement));
}

// ---------------------------------------------------------------------------
// Snaps

bool SnapTarget::discrete() const {
  return kind == SnapKind::Point || kind == SnapKind::Origin || kind == SnapKind::LinkedPoint ||
         kind == SnapKind::Center || kind == SnapKind::Midpoint;
}

bool SnapTarget::continuous() const { return kind == SnapKind::OnCurve || kind == SnapKind::OnEdge; }

namespace {

// A snap's kind in English, its text to translate; null for a free place.
const char* snapKindSource(SnapKind kind) {
  switch (kind) {
  case SnapKind::Point:
    return QT_TRANSLATE_NOOP("Sketch3d", "Point");
  case SnapKind::Origin:
    return QT_TRANSLATE_NOOP("Sketch3d", "Origin");
  case SnapKind::LinkedPoint:
    return QT_TRANSLATE_NOOP("Sketch3d", "Linked Point");
  case SnapKind::Center:
    return QT_TRANSLATE_NOOP("Sketch3d", "Center");
  case SnapKind::Midpoint:
    return QT_TRANSLATE_NOOP("Sketch3d", "Midpoint");
  case SnapKind::OnCurve:
    return QT_TRANSLATE_NOOP("Sketch3d", "On Curve");
  case SnapKind::OnEdge:
    return QT_TRANSLATE_NOOP("Sketch3d", "On Edge");
  case SnapKind::Guide:
    return QT_TRANSLATE_NOOP("Sketch3d", "Along %1");
  case SnapKind::OnFace:
    return QT_TRANSLATE_NOOP("Sketch3d", "On Face");
  case SnapKind::Free:
    break;
  }
  return nullptr;
}

// A tool's source as a linked reference record has it: where it was
// picked (`occurrence`) as its link's source.
QJsonValue linkedSource(const QJsonValue& source) {
  if (!source.isObject()) {
    return source;
  }
  QJsonObject s = source.toObject();
  const QString occurrence = s.value(QStringLiteral("occurrence")).toString();
  if (!occurrence.isEmpty() && !s.contains(QStringLiteral("link"))) {
    s.insert(QStringLiteral("link"), QJsonObject{{QStringLiteral("source"), occurrence}});
  }
  return s;
}

} // namespace

QString snapKindText(const SnapTarget& target) {
  const char* kind = snapKindSource(target.kind);
  if (kind == nullptr) {
    return QString();
  }
  return target.kind == SnapKind::Guide ? Text::tr(kind).arg(target.what) : Text::tr(kind);
}

QString snapBadge(const SnapTarget& target) {
  // What the snap is on, in words; the ids are snapDetail's (mitcad#1653).
  const QString kind = snapKindText(target);
  if (kind.isEmpty() || target.kind == SnapKind::Guide || target.kind == SnapKind::Origin || target.words.isEmpty()) {
    return kind;
  }
  return Text::tr("%1: %2").arg(kind, target.words);
}

QString snapDetail(const SnapTarget& target) {
  const char* source = snapKindSource(target.kind);
  if (source == nullptr) {
    return QString();
  }
  const QString kind = QString::fromLatin1(source);
  if (target.kind == SnapKind::Guide) {
    return kind.arg(target.what);
  }
  if (target.kind == SnapKind::Origin || target.what.isEmpty()) {
    return kind;
  }
  return QStringLiteral("%1: %2").arg(kind, target.what);
}

QString snapSourceWords(const QJsonValue& source, const SourceNames& names) {
  QJsonValue linked = linkedSource(source);
  if (linked.isObject()) {
    // A middle is the badge's kind (Midpoint); a start or an end is said.
    QJsonObject s = linked.toObject();
    if (s.value(QStringLiteral("at")).toString() == QStringLiteral("middle")) {
      s.remove(QStringLiteral("at"));
    }
    linked = s;
  }
  return sourceText(linked, names);
}

QJsonArray snapPlaces(const SnapTarget& target) {
  if (target.kind == SnapKind::Free || target.kind == SnapKind::Guide || target.kind == SnapKind::Origin ||
      !target.source.isObject()) {
    return QJsonArray();
  }
  return sourcePlaces(linkedSource(target.source));
}

bool snapsOntoStart(const SnapTarget& snap, const Xyz& start) {
  return snap.fixesPlace() && distance(snap.at, start) <= kTolerance;
}

std::optional<SnapTarget> chooseSnap(const std::vector<SnapTarget>& candidates, double tolerance) {
  const SnapTarget* best = nullptr;
  for (const SnapTarget& candidate : candidates) {
    if (candidate.kind == SnapKind::Free || !(candidate.pixels <= tolerance)) {
      continue;
    }
    if (best == nullptr) {
      best = &candidate;
      continue;
    }
    const int a = snapClass(candidate);
    const int b = snapClass(*best);
    if (a != b) {
      if (a < b) {
        best = &candidate;
      }
      continue;
    }
    if (candidate.pixels < best->pixels - kTiePixels) {
      best = &candidate;
      continue;
    }
    if (candidate.pixels > best->pixels + kTiePixels) {
      continue;
    }
    // A tie: by kind, then this sketch's curves before other sources.
    const int ka = kindOrder(candidate.kind);
    const int kb = kindOrder(best->kind);
    if (ka < kb || (ka == kb && candidate.own && !best->own)) {
      best = &candidate;
    }
  }
  if (best == nullptr) {
    return std::nullopt;
  }
  return *best;
}

QJsonValue pointInput(const PlacedPoint& point) {
  if (!point.snap.point.isEmpty() && (point.snap.kind == SnapKind::Point || point.snap.kind == SnapKind::Center)) {
    return point.snap.point;
  }
  return xyzJson(point.at);
}

QJsonValue pointSource(const PlacedPoint& point) {
  if (point.snap.kind == SnapKind::Point || point.snap.kind == SnapKind::Center || !point.snap.curve.isEmpty()) {
    return QJsonValue();
  }
  return point.snap.source;
}

QJsonObject ownCurveRelation(const PlacedPoint& point, const QString& pointId) {
  if (point.snap.curve.isEmpty() || pointId.isEmpty()) {
    return QJsonObject();
  }
  if (point.snap.kind == SnapKind::Midpoint) {
    return {{QStringLiteral("type"), QStringLiteral("midpoint")},
            {QStringLiteral("point"), pointId},
            {QStringLiteral("curve"), point.snap.curve}};
  }
  if (point.snap.kind == SnapKind::OnCurve) {
    return {{QStringLiteral("type"), QStringLiteral("point_on")},
            {QStringLiteral("point"), pointId},
            {QStringLiteral("curve"), point.snap.curve}};
  }
  return QJsonObject();
}

// ---------------------------------------------------------------------------
// Typed input

bool SegmentInput::anyEdited() const {
  if (mode == Mode::AxisLength) {
    return length.edited;
  }
  return std::any_of(offsets.begin(), offsets.end(), [](const TypedValue& v) { return v.edited; });
}

void SegmentInput::clearOtherMode() {
  if (mode == Mode::AxisLength) {
    for (TypedValue& value : offsets) {
      value.clear();
    }
  } else {
    length.clear();
  }
}

void SegmentInput::clearAll() {
  for (TypedValue& value : offsets) {
    value.clear();
  }
  length.clear();
}

Xyz typedEnd(const Xyz& start, const Xyz& pointer, const SegmentInput& input, const Frame3& frame) {
  // In the frame's coordinates (the sketch's, or a rotated plane's:
  // mitcad#1661), and back.
  const Xyz from = inFrame(frame, start);
  const Xyz to = inFrame(frame, pointer);
  if (input.mode == SegmentInput::Mode::AxisLength) {
    const Xyz along = scaled(axisVector(input.axis), input.negative ? -1.0 : 1.0);
    const double reach =
        input.length.edited && input.length.valid ? input.length.value : std::abs(dot(sub(to, from), along));
    return outOfFrame(frame, add(from, scaled(along, reach)));
  }
  Xyz end = to;
  for (int k = 0; k < 3; ++k) {
    const TypedValue& typed = input.offsets[static_cast<std::size_t>(k)];
    if (typed.edited && typed.valid) {
      end[static_cast<std::size_t>(k)] = from[static_cast<std::size_t>(k)] + typed.value;
    }
  }
  return outOfFrame(frame, end);
}

QString inputProblem(const Xyz& start, const Xyz& end, const SegmentInput& input, std::optional<Axis3> keepAxis) {
  const auto invalid = [](const TypedValue& v, const QString& label) {
    return v.edited && !v.valid ? QStringLiteral("%1: %2").arg(label, v.problem) : QString();
  };
  if (input.mode == SegmentInput::Mode::AxisLength) {
    if (const QString problem = invalid(input.length, Text::tr("Length")); !problem.isEmpty()) {
      return problem;
    }
    if (input.length.edited && !(input.length.value > kTolerance)) {
      return Text::tr("The length must be greater than zero.");
    }
    if (keepAxis && *keepAxis != input.axis) {
      return Text::tr("Keep Axis along %1 contradicts the axis %2 chosen.")
          .arg(axisLabel(*keepAxis), axisLabel(input.axis));
    }
  } else {
    for (int k = 0; k < 3; ++k) {
      if (const QString problem = invalid(input.offsets[static_cast<std::size_t>(k)], offsetLabel(k));
          !problem.isEmpty()) {
        return problem;
      }
    }
    if (keepAxis) {
      for (int k = 0; k < 3; ++k) {
        const TypedValue& typed = input.offsets[static_cast<std::size_t>(k)];
        if (k != static_cast<int>(*keepAxis) && typed.edited && std::abs(typed.value) > kTolerance) {
          return Text::tr("Keep Axis along %1 contradicts the typed %2.").arg(axisLabel(*keepAxis), offsetLabel(k));
        }
      }
    }
  }
  if (!finite(end)) {
    return Text::tr("The values give no finite point.");
  }
  if (distance(start, end) <= kTolerance) {
    return Text::tr("The segment would have no length.");
  }
  return QString();
}

QString snapConflict(const Xyz& start, const SnapTarget& snap, const SegmentInput& input,
                     std::optional<Axis3> keepAxis, const Frame3& frame) {
  if (!snap.fixesPlace()) {
    return QString();
  }
  // In the frame's coordinates (mitcad#1661).
  const Xyz d = inFrame(frame, sub(snap.at, start));
  const QString badge = snapBadge(snap);
  const auto refused = [&badge](const QString& what) {
    return Text::tr("The snap to %1 contradicts %2: move off the snap or clear the field.").arg(badge, what);
  };
  if (input.mode == SegmentInput::Mode::AxisLength && input.anyEdited()) {
    const Xyz along = scaled(axisVector(input.axis), input.negative ? -1.0 : 1.0);
    const double reach = dot(d, along);
    if (norm(sub(d, scaled(along, reach))) > kTolerance || reach <= 0.0) {
      return refused(Text::tr("the axis %1%2").arg(input.negative ? QStringLiteral("-") : QStringLiteral("+"),
                                                    axisLabel(input.axis)));
    }
    if (input.length.edited && input.length.valid && std::abs(reach - input.length.value) > kTolerance) {
      return refused(Text::tr("the typed Length %1").arg(shown(input.length.value)));
    }
  }
  if (input.mode == SegmentInput::Mode::Offsets) {
    for (int k = 0; k < 3; ++k) {
      const TypedValue& typed = input.offsets[static_cast<std::size_t>(k)];
      if (typed.edited && typed.valid && std::abs(d[static_cast<std::size_t>(k)] - typed.value) > kTolerance) {
        return refused(Text::tr("the typed %1 %2").arg(offsetLabel(k), shown(typed.value)));
      }
    }
  }
  if (keepAxis) {
    const Xyz along = axisVector(*keepAxis);
    if (norm(sub(d, scaled(along, dot(d, along)))) > kTolerance) {
      return refused(Text::tr("Keep Axis along %1").arg(axisLabel(*keepAxis)));
    }
  }
  return QString();
}

QString formatLength(double millimetres, double unitMillimetres) {
  return shown(millimetres / (unitMillimetres > 0.0 ? unitMillimetres : 1.0));
}

// ---------------------------------------------------------------------------
// Commands

QJsonObject lineCommand(const QString& sketch, const PlacedPoint& start, const PlacedPoint& end,
                        const SegmentInput& input, std::optional<Axis3> keepAxis, bool construction,
                        const Frame3& frame) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_line")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("start"), pointInput(start)},
                  {QStringLiteral("end"), pointInput(end)}};
  if (!isSketchFrame(frame)) {
    // The axis and the offsets along a rotated plane's axes (mitcad#1661).
    cmd.insert(QStringLiteral("frame"), frameJson(frame));
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  if (input.mode == SegmentInput::Mode::AxisLength) {
    cmd.insert(QStringLiteral("axis"), axisKey(input.axis));
    cmd.insert(QStringLiteral("sense"), input.negative ? QStringLiteral("negative") : QStringLiteral("positive"));
    if (input.length.edited) {
      cmd.insert(QStringLiteral("length"), input.length.expression);
    }
  } else {
    if (input.anyEdited()) {
      QJsonArray offsets;
      for (const TypedValue& typed : input.offsets) {
        offsets.append(typed.edited ? QJsonValue(typed.expression) : QJsonValue());
      }
      cmd.insert(QStringLiteral("offsets"), offsets);
    }
    if (keepAxis) {
      const double along = dot(sub(end.at, start.at), frame[static_cast<std::size_t>(*keepAxis)]);
      cmd.insert(QStringLiteral("axis"), axisKey(*keepAxis));
      cmd.insert(QStringLiteral("sense"), along < 0.0 ? QStringLiteral("negative") : QStringLiteral("positive"));
    }
  }
  if (const QJsonValue source = pointSource(start); !source.isNull() && !source.isUndefined()) {
    cmd.insert(QStringLiteral("start_source"), source);
  }
  if (const QJsonValue source = pointSource(end); !source.isNull() && !source.isUndefined()) {
    cmd.insert(QStringLiteral("end_source"), source);
  }
  return cmd;
}

QJsonObject pointCommand(const QString& sketch, const PlacedPoint& point) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_point")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("at"), xyzJson(point.at)}};
  if (const QJsonValue source = pointSource(point); !source.isNull() && !source.isUndefined()) {
    cmd.insert(QStringLiteral("source"), source);
  }
  return cmd;
}

// ---------------------------------------------------------------------------
// Arcs

std::optional<ArcShape> threePointArc(const Xyz& start, const Xyz& end, const Xyz& through) {
  const Xyz ab = sub(through, start);
  const Xyz ac = sub(end, start);
  const Xyz n = cross(ab, ac);
  const double nn = dot(n, n);
  const double scale = norm(ab) * norm(ac);
  if (!(scale > kTolerance * kTolerance) || std::sqrt(nn) <= 1e-9 * scale || distance(start, end) <= kTolerance ||
      distance(through, end) <= kTolerance) {
    return std::nullopt; // on one line, or two points meet
  }
  // The circumcentre of the triangle.
  const Xyz toCenter =
      scaled(add(scaled(cross(n, ab), dot(ac, ac)), scaled(cross(ac, n), dot(ab, ab))), 1.0 / (2.0 * nn));
  ArcShape arc;
  arc.center = add(start, toCenter);
  arc.radius = norm(toCenter);
  // Start, through and end turn counter-clockwise about the normal.
  const auto normal = unit(cross(sub(through, start), sub(end, through)));
  if (!normal) {
    return std::nullopt;
  }
  arc.normal = *normal;
  arc.sweep = angleAbout(sub(start, arc.center), sub(end, arc.center), arc.normal);
  return arc;
}

std::optional<ArcShape> centerArc(const Xyz& center, const Xyz& start, const Xyz& endHint, const Xyz& normal,
                                  Xyz* end) {
  const auto n = unit(normal);
  if (!n) {
    return std::nullopt;
  }
  const Xyz s = sub(start, center);
  const Xyz inPlane = sub(s, scaled(*n, dot(s, *n)));
  const double radius = norm(inPlane);
  if (radius <= kTolerance) {
    return std::nullopt;
  }
  const Xyz e = sub(endHint, center);
  const auto toward = unit(sub(e, scaled(*n, dot(e, *n))));
  if (!toward) {
    return std::nullopt;
  }
  ArcShape arc;
  arc.center = center;
  arc.normal = *n;
  arc.radius = radius;
  const Xyz onCircle = add(center, scaled(*toward, radius));
  arc.sweep = angleAbout(inPlane, scaled(*toward, radius), *n);
  if (end != nullptr) {
    *end = onCircle;
  }
  return arc;
}

std::optional<ArcShape> tangentArc(const Xyz& start, const Xyz& tangent, const Xyz& end) {
  const auto t = unit(tangent);
  if (!t) {
    return std::nullopt;
  }
  const Xyz d = sub(end, start);
  const auto normal = unit(cross(*t, d));
  if (!normal || norm(cross(*t, d)) <= kTolerance * std::max(1.0, norm(d))) {
    return std::nullopt; // the end lies on the tangent's line
  }
  const Xyz inward = cross(*normal, *t);
  const double across = dot(d, inward);
  if (!(across > kTolerance)) {
    return std::nullopt;
  }
  ArcShape arc;
  arc.normal = *normal;
  arc.radius = dot(d, d) / (2.0 * across);
  arc.center = add(start, scaled(inward, arc.radius));
  arc.sweep = angleAbout(sub(start, arc.center), sub(end, arc.center), arc.normal);
  return arc;
}

std::optional<ArcShape> tangentArcSized(const Xyz& start, const Xyz& tangent, const Xyz& endHint,
                                        std::optional<double> radius, std::optional<double> arcLength, Xyz* end) {
  std::optional<ArcShape> arc = tangentArc(start, tangent, endHint);
  if (!arc) {
    return std::nullopt;
  }
  if (!radius && !arcLength) {
    if (end != nullptr) {
      *end = endHint;
    }
    return arc;
  }
  const Xyz t = *unit(tangent);
  const Xyz inward = cross(arc->normal, t);
  if (radius) {
    if (!(*radius > kTolerance)) {
      return std::nullopt;
    }
    arc->radius = *radius;
    arc->center = add(start, scaled(inward, *radius));
    // Toward the hint, on the new circle.
    const Xyz h = sub(endHint, arc->center);
    const auto toward = unit(sub(h, scaled(arc->normal, dot(h, arc->normal))));
    if (toward) {
      arc->sweep = angleAbout(sub(start, arc->center), *toward, arc->normal);
    }
  }
  if (arcLength) {
    if (!(*arcLength > kTolerance)) {
      return std::nullopt;
    }
    arc->sweep = *arcLength / arc->radius;
  }
  if (!(arc->sweep > 1e-9) || arc->sweep >= 2.0 * kPi - 1e-9) {
    return std::nullopt;
  }
  if (end != nullptr) {
    *end = add(arc->center, rotated(sub(start, arc->center), arc->normal, arc->sweep));
  }
  return arc;
}

std::vector<Xyz> arcPoints(const ArcShape& arc, const Xyz& start, int segments) {
  std::vector<Xyz> points;
  const int count = std::max(segments, 1);
  const Xyz from = sub(start, arc.center);
  for (int i = 0; i <= count; ++i) {
    points.push_back(add(arc.center, rotated(from, arc.normal, arc.sweep * i / count)));
  }
  return points;
}

std::optional<std::pair<Axis3, bool>> keptNormal(const Xyz& normal, Plane3 plane) {
  const Axis3 axis = planeNormalAxis(plane);
  const double along = dot(normal, axisVector(axis));
  if (std::abs(std::abs(along) - 1.0) > 1e-6) {
    return std::nullopt;
  }
  return std::make_pair(axis, along < 0.0);
}

QJsonObject arcCommand(const QString& sketch, const ArcRequest& arc) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_arc")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("mode"), arc.mode},
                  {QStringLiteral("start"), pointInput(arc.start)},
                  {QStringLiteral("end"), pointInput(arc.end)}};
  if (arc.mode == QStringLiteral("three_point")) {
    cmd.insert(QStringLiteral("through"), xyzJson(arc.through));
  } else if (arc.mode == QStringLiteral("center")) {
    cmd.insert(QStringLiteral("center"), pointInput(arc.center));
    if (!arc.keepPlane) {
      cmd.insert(QStringLiteral("normal"), xyzJson(arc.normal));
    }
  } else {
    cmd.insert(QStringLiteral("from"), arc.from);
  }
  if (arc.keepPlane) {
    cmd.insert(QStringLiteral("normal_axis"), axisKey(arc.keepPlane->first));
    cmd.insert(QStringLiteral("normal_sense"),
               arc.keepPlane->second ? QStringLiteral("negative") : QStringLiteral("positive"));
  }
  if (!arc.radius.isEmpty()) {
    cmd.insert(QStringLiteral("radius"), arc.radius);
  }
  if (!arc.arcLength.isEmpty()) {
    cmd.insert(QStringLiteral("arc_length"), arc.arcLength);
  }
  if (arc.construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  if (const QJsonValue source = pointSource(arc.start); !source.isNull() && !source.isUndefined()) {
    cmd.insert(QStringLiteral("start_source"), source);
  }
  if (const QJsonValue source = pointSource(arc.end); !source.isNull() && !source.isUndefined()) {
    cmd.insert(QStringLiteral("end_source"), source);
  }
  return cmd;
}

QJsonObject splineCommand(const QString& sketch, const std::vector<PlacedPoint>& points, bool controlPoints,
                          bool closed, bool construction) {
  QJsonArray inputs;
  for (const PlacedPoint& point : points) {
    inputs.append(pointInput(point));
  }
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_spline")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("points"), inputs}};
  if (controlPoints) {
    cmd.insert(QStringLiteral("degree"), std::min(3, static_cast<int>(points.size()) - 1));
  } else if (closed) {
    cmd.insert(QStringLiteral("closed"), true);
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  return cmd;
}

QJsonArray followUps(const QString& sketch, const std::vector<PlacedPoint>& points, const QStringList& ids,
                     const QString& previous, bool attachSources) {
  QJsonArray out;
  const auto command = [&sketch](const char* name) {
    return QJsonObject{{QStringLiteral("cmd"), QString::fromLatin1(name)}, {QStringLiteral("sketch"), sketch}};
  };
  QString before = previous;
  for (std::size_t i = 0; i < points.size(); ++i) {
    const PlacedPoint& point = points[i];
    const QString id = i < static_cast<std::size_t>(ids.size()) ? ids.at(static_cast<qsizetype>(i)) : QString();
    if (id.isEmpty()) {
      continue;
    }
    if (attachSources) {
      const QJsonValue source = pointSource(point);
      if (!source.isNull() && !source.isUndefined()) {
        QJsonObject attach = command("sketch3d.attach");
        attach.insert(QStringLiteral("point"), id);
        attach.insert(QStringLiteral("source"), source);
        out.append(attach);
      }
    }
    if (const QJsonObject relation = ownCurveRelation(point, id); !relation.isEmpty()) {
      QJsonObject add = command("sketch3d.add_constraint");
      add.insert(QStringLiteral("constraint"), relation);
      out.append(add);
    }
    if (!before.isEmpty() && before != id) {
      for (int k = 0; k < 3; ++k) {
        const QString& expression = point.offsets[static_cast<std::size_t>(k)];
        if (expression.isEmpty()) {
          continue;
        }
        QJsonObject dimension = command("sketch3d.add_dimension");
        if (isSketchFrame(point.offsetFrame)) {
          dimension.insert(QStringLiteral("dimension"), QJsonObject{{QStringLiteral("type"), QStringLiteral("offset")},
                                                                    {QStringLiteral("a"), before},
                                                                    {QStringLiteral("b"), id},
                                                                    {QStringLiteral("axis"), axisKey(Axis3(k))}});
        } else {
          // Along a rotated plane's axis (mitcad#1661).
          const Xyz& along = point.offsetFrame[static_cast<std::size_t>(k)];
          dimension.insert(QStringLiteral("dimension"),
                           QJsonObject{{QStringLiteral("type"), QStringLiteral("offset_along")},
                                       {QStringLiteral("a"), before},
                                       {QStringLiteral("b"), id},
                                       {QStringLiteral("direction"), QJsonArray{along[0], along[1], along[2]}}});
        }
        dimension.insert(QStringLiteral("value"), expression);
        out.append(dimension);
      }
    }
    before = id;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Spline previews

std::vector<Xyz> fitSplinePreview(const std::vector<Xyz>& points, bool closed) {
  std::vector<Xyz> out;
  const std::size_t n = points.size();
  if (n < 2) {
    return points;
  }
  // A Catmull-Rom curve through the points: the model's cubic is close
  // enough for a preview.
  const auto at = [&points, n, closed](long long i) {
    if (closed) {
      const long long m = static_cast<long long>(n);
      return points[static_cast<std::size_t>(((i % m) + m) % m)];
    }
    if (i < 0) {
      return sub(scaled(points[0], 2.0), points[1]);
    }
    if (i >= static_cast<long long>(n)) {
      return sub(scaled(points[n - 1], 2.0), points[n - 2]);
    }
    return points[static_cast<std::size_t>(i)];
  };
  const long long spans = static_cast<long long>(closed ? n : n - 1);
  constexpr int kSteps = 16;
  for (long long i = 0; i < spans; ++i) {
    const Xyz p0 = at(i - 1);
    const Xyz p1 = at(i);
    const Xyz p2 = at(i + 1);
    const Xyz p3 = at(i + 2);
    for (int s = 0; s < kSteps; ++s) {
      const double t = static_cast<double>(s) / kSteps;
      const double t2 = t * t;
      const double t3 = t2 * t;
      Xyz p = scaled(p1, 2.0);
      p = add(p, scaled(sub(p2, p0), t));
      p = add(p, scaled(add(sub(scaled(p0, 2.0), scaled(p1, 5.0)), sub(scaled(p2, 4.0), p3)), t2));
      p = add(p, scaled(add(sub(scaled(p1, 3.0), p0), sub(p3, scaled(p2, 3.0))), t3));
      out.push_back(scaled(p, 0.5));
    }
  }
  out.push_back(closed ? points[0] : points[n - 1]);
  return out;
}

std::vector<Xyz> controlSplinePreview(const std::vector<Xyz>& control) {
  const int n = static_cast<int>(control.size());
  if (n < 2) {
    return control;
  }
  const int degree = std::min(3, n - 1);
  std::vector<double> knots;
  for (int i = 0; i <= degree; ++i) {
    knots.push_back(0.0);
  }
  for (int i = 1; i < n - degree; ++i) {
    knots.push_back(static_cast<double>(i) / (n - degree));
  }
  for (int i = 0; i <= degree; ++i) {
    knots.push_back(1.0);
  }
  std::vector<Xyz> out;
  constexpr int kSteps = 64;
  for (int s = 0; s <= kSteps; ++s) {
    const double t = std::min(static_cast<double>(s) / kSteps, 1.0 - 1e-12);
    out.push_back(deBoor(control, degree, knots, t));
  }
  out.back() = control.back();
  return out;
}

// ---------------------------------------------------------------------------
// Curves of the sketch

std::optional<Xyz> endTangent(const Sketch3dModel& model, const QString& curveId, const QString& pointId) {
  const Sketch3dCurve* curve = model.curve(curveId);
  const Sketch3dPoint* point = model.point(pointId);
  if (curve == nullptr || point == nullptr) {
    return std::nullopt;
  }
  const QJsonObject& json = curve->json;
  const auto pointAt = [&model](const QJsonValue& id) -> std::optional<Xyz> {
    const Sketch3dPoint* p = model.point(id.toString());
    return p != nullptr ? std::optional<Xyz>(p->at) : std::nullopt;
  };
  if (curve->type == QStringLiteral("line")) {
    const auto a = pointAt(json.value(QStringLiteral("start")));
    const auto b = pointAt(json.value(QStringLiteral("end")));
    if (!a || !b) {
      return std::nullopt;
    }
    if (json.value(QStringLiteral("end")).toString() == pointId) {
      return unit(sub(*b, *a));
    }
    if (json.value(QStringLiteral("start")).toString() == pointId) {
      return unit(sub(*a, *b));
    }
    return std::nullopt;
  }
  if (curve->type == QStringLiteral("arc")) {
    const auto c = pointAt(json.value(QStringLiteral("center")));
    const QJsonArray n = json.value(QStringLiteral("geometry")).toObject().value(QStringLiteral("normal")).toArray();
    if (!c || n.size() < 3) {
      return std::nullopt;
    }
    const Xyz normal{n.at(0).toDouble(), n.at(1).toDouble(), n.at(2).toDouble()};
    const Xyz radial = sub(point->at, *c);
    if (json.value(QStringLiteral("end")).toString() == pointId) {
      return unit(cross(normal, radial));
    }
    if (json.value(QStringLiteral("start")).toString() == pointId) {
      return unit(cross(radial, normal));
    }
    return std::nullopt;
  }
  if (curve->type == QStringLiteral("spline") || curve->type == QStringLiteral("fitted_spline")) {
    if (json.value(QStringLiteral("closed")).toBool()) {
      return std::nullopt;
    }
    const QJsonArray control =
        json.value(QStringLiteral("geometry")).toObject().value(QStringLiteral("control_points")).toArray();
    const QStringList& ids = curve->points;
    if (control.size() < 2 || ids.isEmpty()) {
      return std::nullopt;
    }
    const auto xyz = [](const QJsonValue& v) {
      const QJsonArray a = v.toArray();
      return Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()};
    };
    const QJsonArray order = json.value(curve->type == QStringLiteral("spline") ? QStringLiteral("control")
                                                                                : QStringLiteral("points"))
                                 .toArray();
    if (order.isEmpty()) {
      return std::nullopt;
    }
    if (order.last().toString() == pointId) {
      return unit(sub(xyz(control.at(control.size() - 1)), xyz(control.at(control.size() - 2))));
    }
    if (order.first().toString() == pointId) {
      return unit(sub(xyz(control.at(0)), xyz(control.at(1))));
    }
  }
  return std::nullopt;
}

QStringList continuableCurves(const Sketch3dModel& model, const QString& point) {
  QStringList free;
  QStringList joined;
  for (const Sketch3dCurve& curve : model.curves) {
    if (curve.type != QStringLiteral("line") && curve.type != QStringLiteral("arc") &&
        curve.type != QStringLiteral("spline") && curve.type != QStringLiteral("fitted_spline")) {
      continue;
    }
    if (!endTangent(model, curve.id, point)) {
      continue;
    }
    // A free end: no other curve ends there.
    const bool alone = std::none_of(model.curves.begin(), model.curves.end(), [&](const Sketch3dCurve& other) {
      return other.id != curve.id && other.type != QStringLiteral("reference") && other.points.contains(point);
    });
    (alone ? free : joined) << curve.id;
  }
  return free + joined;
}

// ---------------------------------------------------------------------------
// Plane-bound shapes (mitcad#1550)

QString planeKey(Plane3 plane) { return planeLabel(plane).toLower(); }

Xyz ShapePlane::u() const { return frame[static_cast<std::size_t>(planeAxes(plane)[0])]; }

Xyz ShapePlane::v() const { return frame[static_cast<std::size_t>(planeAxes(plane)[1])]; }

Xyz ShapePlane::normal() const { return frame[static_cast<std::size_t>(planeNormalAxis(plane))]; }

QJsonValue planeValue(const ShapePlane& plane) {
  if (!plane.rotated()) {
    return planeKey(plane.plane);
  }
  const auto json = [](const Xyz& v) { return QJsonArray{v[0], v[1], v[2]}; };
  return QJsonObject{{QStringLiteral("x"), json(plane.u())}, {QStringLiteral("normal"), json(plane.normal())}};
}

std::array<double, 2> planarOf(const Xyz& p, const ShapePlane& plane) {
  return {dot(p, plane.u()), dot(p, plane.v())};
}

Xyz fromPlanar(const std::array<double, 2>& uv, double elevation, const ShapePlane& plane) {
  return add(add(scaled(plane.u(), uv[0]), scaled(plane.v(), uv[1])), scaled(plane.normal(), elevation));
}

Xyz ontoPlane(const Xyz& p, const Xyz& through, const ShapePlane& plane) {
  const Xyz n = plane.normal();
  return sub(p, scaled(n, dot(sub(p, through), n)));
}

std::optional<CircleShape> twoPointCircle(const Xyz& a, const Xyz& b, const ShapePlane& plane) {
  const Xyz q = ontoPlane(b, a, plane);
  if (distance(a, q) <= kTolerance) {
    return std::nullopt;
  }
  return CircleShape{scaled(add(a, q), 0.5), distance(a, q) / 2.0};
}

std::optional<CircleShape> threePointCircle(const Xyz& a, const Xyz& b, const Xyz& c, const ShapePlane& plane) {
  const auto arc = threePointArc(a, ontoPlane(c, a, plane), ontoPlane(b, a, plane));
  if (!arc) {
    return std::nullopt;
  }
  return CircleShape{arc->center, arc->radius};
}

std::vector<Xyz> circlePoints(const Xyz& center, double radius, const ShapePlane& plane, int segments) {
  std::vector<Xyz> points;
  const int count = std::max(segments, 3);
  const Xyz u = plane.u();
  const Xyz v = plane.v();
  for (int i = 0; i <= count; ++i) {
    const double t = 2.0 * kPi * i / count;
    points.push_back(add(center, add(scaled(u, radius * std::cos(t)), scaled(v, radius * std::sin(t)))));
  }
  return points;
}

double ellipseMinor(const Xyz& center, const Xyz& major, const Xyz& at, const ShapePlane& plane) {
  const auto u = unit(sub(ontoPlane(major, center, plane), center));
  if (!u) {
    return 0.0;
  }
  const Xyz w = cross(plane.normal(), *u);
  return std::abs(dot(sub(at, center), w));
}

std::vector<Xyz> ellipsePoints(const Xyz& center, const Xyz& major, double minor, const ShapePlane& plane, int segments) {
  std::vector<Xyz> points;
  const Xyz toMajor = sub(ontoPlane(major, center, plane), center);
  const auto u = unit(toMajor);
  if (!u) {
    return points;
  }
  const Xyz w = cross(plane.normal(), *u);
  const double a = norm(toMajor);
  const int count = std::max(segments, 3);
  for (int i = 0; i <= count; ++i) {
    const double t = 2.0 * kPi * i / count;
    points.push_back(add(center, add(scaled(*u, a * std::cos(t)), scaled(w, minor * std::sin(t)))));
  }
  return points;
}

QString rectangleKey(RectangleMode mode) {
  return mode == RectangleMode::TwoPoint ? QStringLiteral("two_point")
         : mode == RectangleMode::Center ? QStringLiteral("center")
                                         : QStringLiteral("three_point");
}

namespace {

// The corner (0 lower left, then counter-clockwise) of a rectangle along
// the plane's axes that `p` is, `other` across it, as the model numbers
// them.
int cornerIndex(const std::array<double, 2>& p, const std::array<double, 2>& other) {
  const bool right = p[0] > other[0];
  const bool up = p[1] > other[1];
  return !right && !up ? 0 : right && !up ? 1 : right && up ? 2 : 3;
}

// A 3-point rectangle's planar corners and whether it runs the other way
// (its second click is then the fourth corner).
std::optional<std::pair<std::array<std::array<double, 2>, 4>, bool>>
turnedCorners(const std::array<double, 2>& a, const std::array<double, 2>& b, const std::array<double, 2>& c) {
  const std::array<double, 2> side{b[0] - a[0], b[1] - a[1]};
  const double length = std::hypot(side[0], side[1]);
  if (length <= kTolerance) {
    return std::nullopt;
  }
  const std::array<double, 2> n{-side[1] / length, side[0] / length};
  const double along = (c[0] - b[0]) * n[0] + (c[1] - b[1]) * n[1];
  const std::array<double, 2> h{n[0] * along, n[1] * along};
  std::array<std::array<double, 2>, 4> corners{
      a, b, std::array<double, 2>{b[0] + h[0], b[1] + h[1]}, std::array<double, 2>{a[0] + h[0], a[1] + h[1]}};
  if (side[0] * h[1] - side[1] * h[0] < 0.0) {
    return std::make_pair(std::array<std::array<double, 2>, 4>{corners[0], corners[3], corners[2], corners[1]}, true);
  }
  return std::make_pair(corners, false);
}

} // namespace

std::optional<std::array<Xyz, 4>> rectangleCorners(RectangleMode mode, const std::vector<Xyz>& clicks, const ShapePlane& plane) {
  const std::size_t needed = mode == RectangleMode::ThreePoint ? 3u : 2u;
  if (clicks.size() < needed) {
    return std::nullopt;
  }
  const double elevation = dot(clicks[0], plane.normal());
  const std::array<double, 2> a = planarOf(clicks[0], plane);
  const std::array<double, 2> b = planarOf(clicks[1], plane);
  std::array<std::array<double, 2>, 4> uv{};
  switch (mode) {
  case RectangleMode::TwoPoint: {
    const double x0 = std::min(a[0], b[0]), x1 = std::max(a[0], b[0]);
    const double y0 = std::min(a[1], b[1]), y1 = std::max(a[1], b[1]);
    uv = {std::array<double, 2>{x0, y0}, std::array<double, 2>{x1, y0}, std::array<double, 2>{x1, y1},
          std::array<double, 2>{x0, y1}};
    break;
  }
  case RectangleMode::Center: {
    const double dx = std::abs(b[0] - a[0]), dy = std::abs(b[1] - a[1]);
    uv = {std::array<double, 2>{a[0] - dx, a[1] - dy}, std::array<double, 2>{a[0] + dx, a[1] - dy},
          std::array<double, 2>{a[0] + dx, a[1] + dy}, std::array<double, 2>{a[0] - dx, a[1] + dy}};
    break;
  }
  case RectangleMode::ThreePoint: {
    const auto turned = turnedCorners(a, b, planarOf(clicks[2], plane));
    if (!turned) {
      return std::nullopt;
    }
    uv = turned->first;
    break;
  }
  }
  const auto apart = [](const std::array<double, 2>& p, const std::array<double, 2>& q) {
    return std::hypot(p[0] - q[0], p[1] - q[1]);
  };
  if (apart(uv[0], uv[1]) <= kTolerance || apart(uv[1], uv[2]) <= kTolerance) {
    return std::nullopt;
  }
  std::array<Xyz, 4> out{};
  for (std::size_t i = 0; i < 4; ++i) {
    out[i] = fromPlanar(uv[i], elevation, plane);
  }
  return out;
}

std::vector<int> rectangleClickCorners(RectangleMode mode, const std::vector<Xyz>& clicks, const ShapePlane& plane) {
  std::vector<int> out(clicks.size(), -1);
  if (clicks.size() < 2) {
    return out;
  }
  const std::array<double, 2> a = planarOf(clicks[0], plane);
  const std::array<double, 2> b = planarOf(clicks[1], plane);
  switch (mode) {
  case RectangleMode::TwoPoint:
    out[0] = cornerIndex(a, b);
    out[1] = cornerIndex(b, a);
    break;
  case RectangleMode::Center:
    out[1] = cornerIndex(b, a);
    break;
  case RectangleMode::ThreePoint:
    if (clicks.size() >= 3) {
      const auto turned = turnedCorners(a, b, planarOf(clicks[2], plane));
      out[0] = 0;
      out[1] = turned && turned->second ? 3 : 1;
    }
    break;
  }
  return out;
}

std::vector<Xyz> polygonCorners(const Xyz& center, const Xyz& vertex, int sides, bool inscribed, const ShapePlane& plane) {
  std::vector<Xyz> out;
  const std::array<double, 2> c = planarOf(center, plane);
  const std::array<double, 2> v = planarOf(vertex, plane);
  const double r = std::hypot(v[0] - c[0], v[1] - c[1]);
  if (sides < 3 || r <= kTolerance) {
    return out;
  }
  const double n = sides;
  const double a0 = std::atan2(v[1] - c[1], v[0] - c[0]);
  const double big = inscribed ? r : r / std::cos(kPi / n);
  const double first = inscribed ? a0 : a0 - kPi / n;
  const double elevation = dot(center, plane.normal());
  for (int i = 0; i < sides; ++i) {
    const double a = first + 2.0 * kPi * i / n;
    out.push_back(fromPlanar({c[0] + big * std::cos(a), c[1] + big * std::sin(a)}, elevation, plane));
  }
  return out;
}

std::optional<Xyz> edgePolygonCenter(const Xyz& a, const Xyz& b, const Xyz& side, int sides, const ShapePlane& plane) {
  const Xyz q = ontoPlane(b, a, plane);
  const auto along = unit(sub(q, a));
  if (!along || sides < 3) {
    return std::nullopt;
  }
  Xyz across = cross(plane.normal(), *along);
  if (dot(sub(side, a), across) < 0.0) {
    across = scaled(across, -1.0);
  }
  const double half = distance(a, q) / 2.0;
  const double apothem = half / std::tan(kPi / sides);
  return add(scaled(add(a, q), 0.5), scaled(across, apothem));
}

QJsonObject circleCommand(const QString& sketch, const PlacedPoint& center, double radius, const ShapePlane& plane,
                          const QString& typed, bool diameter, const QStringList& through, bool construction) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_circle")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("center"), pointInput(center)},
                  {QStringLiteral("plane"), planeValue(plane)}};
  if (!typed.isEmpty()) {
    cmd.insert(QStringLiteral("radius"), typed);
    if (diameter) {
      cmd.insert(QStringLiteral("diameter"), true);
    }
  } else {
    // Drawn, not typed: no dimension, as the planar circle tools.
    cmd.insert(QStringLiteral("radius"), radius);
    cmd.insert(QStringLiteral("dimension"), false);
  }
  if (!through.isEmpty()) {
    cmd.insert(QStringLiteral("through"), QJsonArray::fromStringList(through));
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  return cmd;
}

QJsonObject ellipseCommand(const QString& sketch, const PlacedPoint& center, const PlacedPoint& major, double minor,
                           const ShapePlane& plane, const QString& majorTyped, const QString& minorTyped, bool construction) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_ellipse")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("center"), pointInput(center)},
                  {QStringLiteral("major"), pointInput(major)},
                  {QStringLiteral("minor_radius"), minor},
                  {QStringLiteral("plane"), planeValue(plane)}};
  if (!majorTyped.isEmpty()) {
    cmd.insert(QStringLiteral("major_value"), majorTyped);
  }
  if (!minorTyped.isEmpty()) {
    cmd.insert(QStringLiteral("minor_value"), minorTyped);
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  return cmd;
}

QJsonObject rectangleCommand(const QString& sketch, RectangleMode mode, const std::vector<PlacedPoint>& clicks,
                             const ShapePlane& plane, const QString& width, const QString& height, bool construction) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_rectangle")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("mode"), rectangleKey(mode)},
                  {QStringLiteral("plane"), planeValue(plane)}};
  if (clicks.size() >= 2) {
    if (mode == RectangleMode::Center) {
      cmd.insert(QStringLiteral("center"), pointInput(clicks[0]));
      cmd.insert(QStringLiteral("corner"), pointInput(clicks[1]));
    } else {
      cmd.insert(QStringLiteral("a"), pointInput(clicks[0]));
      cmd.insert(QStringLiteral("b"), pointInput(clicks[1]));
    }
  }
  if (mode == RectangleMode::ThreePoint && clicks.size() >= 3) {
    cmd.insert(QStringLiteral("c"), xyzJson(clicks[2].at));
  }
  if (!width.isEmpty()) {
    cmd.insert(QStringLiteral("width"), width);
  }
  if (!height.isEmpty()) {
    cmd.insert(QStringLiteral("height"), height);
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  return cmd;
}

QJsonObject polygonCommand(const QString& sketch, const PlacedPoint& center, const PlacedPoint& vertex, int sides,
                           bool inscribed, const ShapePlane& plane, const QString& radius, bool construction) {
  // A circumscribed polygon's vertex is the middle of a side: a place.
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_polygon")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("center"), pointInput(center)},
                  {QStringLiteral("vertex"), inscribed ? pointInput(vertex) : QJsonValue(xyzJson(vertex.at))},
                  {QStringLiteral("sides"), sides},
                  {QStringLiteral("inscribed"), inscribed},
                  {QStringLiteral("plane"), planeValue(plane)}};
  if (!radius.isEmpty()) {
    cmd.insert(QStringLiteral("radius"), radius);
  }
  if (construction) {
    cmd.insert(QStringLiteral("construction"), true);
  }
  return cmd;
}

QJsonObject textCommand(const QString& sketch, const QString& text, const Xyz& at, const std::optional<Xyz>& frame,
                        const ShapePlane& plane, double height) {
  QJsonObject cmd{{QStringLiteral("cmd"), QStringLiteral("sketch3d.add_text")},
                  {QStringLiteral("sketch"), sketch},
                  {QStringLiteral("text"), text},
                  {QStringLiteral("at"), xyzJson(at)},
                  {QStringLiteral("plane"), planeValue(plane)},
                  {QStringLiteral("height"), height}};
  if (frame) {
    cmd.insert(QStringLiteral("frame"), xyzJson(*frame));
  }
  return cmd;
}

} // namespace mitcad::sketch
