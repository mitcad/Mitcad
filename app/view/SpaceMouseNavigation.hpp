// SPDX-License-Identifier: MIT
#pragma once

// Navigation with a 3D mouse (mitcad#951): its settings (Preferences >
// Navigation > 3D mouse, saved in the group "view") and the pure mapping
// from the device's six axes to the camera, which the unit tests check.
//
// The device's axes come in the view's frame (platform/SpaceMouseDevice.hpp).
// The model follows the cap ("object mode"): pushed to the right it moves
// to the right, lifted it moves up, pulled toward the user it comes closer
// (zooms in), and turned it turns the same way, about the same centre as
// the mouse's orbit and with the orbit's Z lock. A flat view (sketch mode)
// takes the pan and the zoom only.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>

#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QString>
#include <QStringList>

#include "../platform/SpaceMouseDevice.hpp"

namespace mitcad {

struct SpaceMouseSettings {
  bool enabled = true;
  double speed = 1.0;
  // Per axis (tx, ty, tz, rx, ry, rz): whether it moves the view, and
  // whether its direction is reversed.
  std::array<bool, 6> axisOn{{true, true, true, true, true, true}};
  std::array<bool, 6> reversed{{false, false, false, false, false, false}};
  bool lockRotation = false;
  bool lockTranslation = false;
  // Only the axis deflected most moves the view.
  bool dominantAxis = false;
  // The command each button runs (0 the first), empty for none.
  QStringList buttons{QStringLiteral("view.fit"), QStringLiteral("view.home")};
};

inline double sanitizeSpaceMouseSpeed(double speed) {
  return std::isfinite(speed) ? std::clamp(speed, 0.1, 4.0) : 1.0;
}

// The axes' names in the settings.
inline const char* spaceMouseAxisKey(std::size_t axis) {
  static const char* const keys[6] = {"tx", "ty", "tz", "rx", "ry", "rz"};
  return axis < 6 ? keys[axis] : "";
}

inline void loadSpaceMouseSettings(const QSettings& store, SpaceMouseSettings& settings) {
  const QString prefix = QStringLiteral("view/spaceMouse");
  settings = SpaceMouseSettings();
  settings.enabled = store.value(prefix, true).toBool();
  bool ok = false;
  const double speed = store.value(prefix + QStringLiteral("Speed"), 1.0).toDouble(&ok);
  settings.speed = ok ? sanitizeSpaceMouseSpeed(speed) : 1.0;
  const QStringList off = store.value(prefix + QStringLiteral("AxesOff")).toStringList();
  const QStringList reversed = store.value(prefix + QStringLiteral("AxesReversed")).toStringList();
  for (std::size_t axis = 0; axis < 6; ++axis) {
    const QString key = QLatin1String(spaceMouseAxisKey(axis));
    settings.axisOn[axis] = !off.contains(key);
    settings.reversed[axis] = reversed.contains(key);
  }
  settings.lockRotation = store.value(prefix + QStringLiteral("LockRotation"), false).toBool();
  settings.lockTranslation = store.value(prefix + QStringLiteral("LockTranslation"), false).toBool();
  settings.dominantAxis = store.value(prefix + QStringLiteral("DominantAxis"), false).toBool();
  // A JSON array, so that an unbound button ("") keeps its place.
  const QString buttonsKey = prefix + QStringLiteral("Buttons");
  if (store.contains(buttonsKey)) {
    const QJsonDocument document = QJsonDocument::fromJson(store.value(buttonsKey).toByteArray());
    if (document.isArray()) {
      settings.buttons.clear();
      for (const auto& value : document.array()) {
        if (settings.buttons.size() >= 32) {
          break;
        }
        settings.buttons.append(value.isString() ? value.toString() : QString());
      }
    }
  }
}

inline void saveSpaceMouseSettings(QSettings& store, const SpaceMouseSettings& settings) {
  const QString prefix = QStringLiteral("view/spaceMouse");
  store.setValue(prefix, settings.enabled);
  store.setValue(prefix + QStringLiteral("Speed"), sanitizeSpaceMouseSpeed(settings.speed));
  QStringList off;
  QStringList reversed;
  for (std::size_t axis = 0; axis < 6; ++axis) {
    const QString key = QLatin1String(spaceMouseAxisKey(axis));
    if (!settings.axisOn[axis]) {
      off.append(key);
    }
    if (settings.reversed[axis]) {
      reversed.append(key);
    }
  }
  store.setValue(prefix + QStringLiteral("AxesOff"), off);
  store.setValue(prefix + QStringLiteral("AxesReversed"), reversed);
  store.setValue(prefix + QStringLiteral("LockRotation"), settings.lockRotation);
  store.setValue(prefix + QStringLiteral("LockTranslation"), settings.lockTranslation);
  store.setValue(prefix + QStringLiteral("DominantAxis"), settings.dominantAxis);
  QJsonArray buttons;
  for (const QString& id : settings.buttons) {
    buttons.append(id);
  }
  store.setValue(prefix + QStringLiteral("Buttons"), QJsonDocument(buttons).toJson(QJsonDocument::Compact));
}

// The share of a full deflection that does nothing (the cap's rest noise).
constexpr double kSpaceMouseDeadzone = 0.04;
// At full deflection and speed 1: view heights per second, e-folds of the
// view's height per second, radians per second.
constexpr double kSpaceMousePanRate = 1.0;
constexpr double kSpaceMouseZoomRate = 1.2;
constexpr double kSpaceMouseTurnRate = 1.6;

// One axis past the dead zone, rescaled to 0..1 with a gentle curve (a
// quarter of the linear slope near rest, for fine moves), sign kept.
inline double spaceMouseResponse(double value) {
  if (!std::isfinite(value)) {
    return 0.0;
  }
  const double magnitude = std::min(std::abs(value), 1.0);
  if (magnitude <= kSpaceMouseDeadzone) {
    return 0.0;
  }
  const double t = (magnitude - kSpaceMouseDeadzone) / (1.0 - kSpaceMouseDeadzone);
  return std::copysign(t * (0.25 + 0.75 * t), value);
}

// The axes that move the view: the response curve, the axes switched off,
// the locks (a flat view has its rotation locked), the reversed directions
// and the dominant axis.
inline SpaceMouseAxes spaceMouseFiltered(const SpaceMouseSettings& settings, const SpaceMouseAxes& axes,
                                         bool flat) {
  SpaceMouseAxes result{};
  for (std::size_t axis = 0; axis < 6; ++axis) {
    const bool rotation = axis >= 3;
    if (!settings.axisOn[axis] || (rotation && (settings.lockRotation || flat)) ||
        (!rotation && settings.lockTranslation)) {
      continue;
    }
    result[axis] = spaceMouseResponse(axes[axis]) * (settings.reversed[axis] ? -1.0 : 1.0);
  }
  if (settings.dominantAxis) {
    std::size_t strongest = 0;
    for (std::size_t axis = 1; axis < 6; ++axis) {
      if (std::abs(result[axis]) > std::abs(result[strongest])) {
        strongest = axis;
      }
    }
    for (std::size_t axis = 0; axis < 6; ++axis) {
      if (axis != strongest) {
        result[axis] = 0.0;
      }
    }
  }
  return result;
}

// What the camera does per second, the model following the cap.
struct SpaceMouseRates {
  double panX = 0.0;  // view heights per second, the model moving right
  double panY = 0.0;  // view heights per second, the model moving up
  double zoom = 0.0;  // e-folds of the view's height per second, > 0 closer
  double pitch = 0.0; // radians per second the model turns about the view's x (right)
  double yaw = 0.0;   // ... about the view's y (up)
  double roll = 0.0;  // ... about the view's z (toward the user)
  bool moving() const {
    return panX != 0.0 || panY != 0.0 || zoom != 0.0 || pitch != 0.0 || yaw != 0.0 || roll != 0.0;
  }
  bool turning() const { return pitch != 0.0 || yaw != 0.0 || roll != 0.0; }
};

inline SpaceMouseRates spaceMouseRates(const SpaceMouseSettings& settings, const SpaceMouseAxes& axes, bool flat) {
  if (!settings.enabled) {
    return {};
  }
  const SpaceMouseAxes moved = spaceMouseFiltered(settings, axes, flat);
  const double speed = sanitizeSpaceMouseSpeed(settings.speed);
  SpaceMouseRates rates;
  rates.panX = moved[0] * kSpaceMousePanRate * speed;
  rates.panY = moved[1] * kSpaceMousePanRate * speed;
  rates.zoom = moved[2] * kSpaceMouseZoomRate * speed;
  rates.pitch = moved[3] * kSpaceMouseTurnRate * speed;
  rates.yaw = moved[4] * kSpaceMouseTurnRate * speed;
  rates.roll = moved[5] * kSpaceMouseTurnRate * speed;
  return rates;
}

// A camera as plain numbers: the eye, the target, the up direction, the
// camera's scale (`height`: Graphic3d_Camera::Scale, the view's shorter
// side at the target, which the zoom scales and SetScale takes back) and
// the view's visible height at the target (`panHeight`, what a pan of one
// view height moves; 0: the scale). The scale is not the visible height
// in a view taller than wide (mitcad#951: reading the visible height and
// setting it as the scale grew the view by its aspect at every frame).
using SpaceMouseVec = std::array<double, 3>;
struct SpaceMousePose {
  SpaceMouseVec eye{};
  SpaceMouseVec target{};
  SpaceMouseVec up{{0.0, 0.0, 1.0}};
  double height = 1.0;
  double panHeight = 0.0;
};

namespace spacemouse_detail {
inline SpaceMouseVec add(const SpaceMouseVec& a, const SpaceMouseVec& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
inline SpaceMouseVec sub(const SpaceMouseVec& a, const SpaceMouseVec& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
inline SpaceMouseVec scale(const SpaceMouseVec& a, double s) { return {a[0] * s, a[1] * s, a[2] * s}; }
inline double dot(const SpaceMouseVec& a, const SpaceMouseVec& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
inline SpaceMouseVec cross(const SpaceMouseVec& a, const SpaceMouseVec& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
inline double length(const SpaceMouseVec& a) { return std::sqrt(dot(a, a)); }
inline bool finite(const SpaceMouseVec& a) {
  return std::isfinite(a[0]) && std::isfinite(a[1]) && std::isfinite(a[2]);
}
// Rodrigues: `v` turned by `angle` about the unit `axis`.
inline SpaceMouseVec rotated(const SpaceMouseVec& v, const SpaceMouseVec& axis, double angle) {
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  return add(add(scale(v, c), scale(cross(axis, v), s)), scale(axis, dot(axis, v) * (1.0 - c)));
}
// A pose turned by `angle` about the line through `pivot` along `axis`.
inline void turn(SpaceMousePose& pose, const SpaceMouseVec& pivot, const SpaceMouseVec& axis, double angle) {
  pose.eye = add(pivot, rotated(sub(pose.eye, pivot), axis, angle));
  pose.target = add(pivot, rotated(sub(pose.target, pivot), axis, angle));
  pose.up = rotated(pose.up, axis, angle);
}
} // namespace spacemouse_detail

// The camera after `seconds` of `rates`: panned across the view, turned
// about `pivot` (the orbit's centre) and zoomed. With `zUp` (the
// constrained orbit) it turns about Z and across the view only, never
// rolls, and stops a degree short of looking straight along Z. A pose
// that makes no view comes back unchanged.
inline SpaceMousePose spaceMouseMove(const SpaceMousePose& pose, const SpaceMouseVec& pivot,
                                     const SpaceMouseRates& rates, double seconds, bool zUp) {
  using namespace spacemouse_detail;
  const SpaceMouseVec toTarget = sub(pose.target, pose.eye);
  const double distance = length(toTarget);
  if (!(distance > 0.0) || !std::isfinite(distance) || !(pose.height > 0.0) || !std::isfinite(seconds) ||
      seconds < 0.0 || !finite(pivot)) {
    return pose;
  }
  const SpaceMouseVec direction = scale(toTarget, 1.0 / distance);
  SpaceMouseVec up = sub(pose.up, scale(direction, dot(pose.up, direction)));
  const double upLength = length(up);
  if (!(upLength > 1e-12)) {
    return pose;
  }
  up = scale(up, 1.0 / upLength);
  const SpaceMouseVec right = cross(direction, up);
  SpaceMousePose result = pose;
  result.up = up;
  // The model moves with the cap: the camera the other way.
  const SpaceMouseVec offset =
      scale(add(scale(right, rates.panX), scale(up, rates.panY)),
            -seconds * (pose.panHeight > 0.0 && std::isfinite(pose.panHeight) ? pose.panHeight : pose.height));
  result.eye = add(result.eye, offset);
  result.target = add(result.target, offset);
  if (zUp) {
    const SpaceMouseVec z{{0.0, 0.0, dot(up, SpaceMouseVec{{0.0, 0.0, 1.0}}) < 0.0 ? -1.0 : 1.0}};
    turn(result, pivot, z, -rates.yaw * seconds);
    SpaceMousePose pitched = result;
    const SpaceMouseVec turnedRight = cross(scale(sub(result.target, result.eye), 1.0 / distance), result.up);
    const double sideways = length(turnedRight);
    if (sideways > 1e-12) {
      turn(pitched, pivot, scale(turnedRight, 1.0 / sideways), -rates.pitch * seconds);
      const auto along = [&distance](const SpaceMousePose& p) {
        return std::abs(sub(p.target, p.eye)[2]) / distance;
      };
      // Never over the pole: a pitch toward Z stops a degree short of it.
      const double limit = std::cos(3.14159265358979323846 / 180.0);
      if (along(pitched) <= limit || along(pitched) < along(result)) {
        result = pitched;
      }
    }
  } else {
    // The model turns by (pitch, yaw, roll) about the view's axes; the
    // camera by the opposite about the pivot.
    const SpaceMouseVec toward = scale(direction, -1.0);
    const SpaceMouseVec spin =
        add(add(scale(right, rates.pitch), scale(up, rates.yaw)), scale(toward, rates.roll));
    const double rate = length(spin);
    if (rate > 0.0) {
      turn(result, pivot, scale(spin, 1.0 / rate), -rate * seconds);
    }
  }
  result.height = pose.height * std::exp(-rates.zoom * seconds);
  result.panHeight = pose.panHeight * std::exp(-rates.zoom * seconds);
  if (!finite(result.eye) || !finite(result.target) || !finite(result.up) || !(result.height > 0.0) ||
      !std::isfinite(result.height)) {
    return pose;
  }
  return result;
}

// The 3D mouse as Preferences shows it (set by SpaceMouseController): what
// is connected, and how many buttons the device has used so far.
struct SpaceMouseStatus {
  QString text;
  int buttonsSeen = 0;
};

inline SpaceMouseStatus& spaceMouseStatus() {
  static SpaceMouseStatus status;
  return status;
}

} // namespace mitcad
