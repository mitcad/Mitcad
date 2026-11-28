// SPDX-License-Identifier: MIT
#pragma once

// Navigation with a 3D mouse (mitcad#951): its settings (Preferences >
// Navigation > 3D mouse, saved in the group "view") and the pure mapping
// from the device's six axes to the camera, with its cross-talk filter,
// the device's reach and Dominant axis only's hold (mitcad#1720), which
// the unit tests check.
//
// The device's axes come in the view's frame (platform/SpaceMouseDevice.hpp).
// Its Navigation mode (mitcad#978) says what follows the cap. Object (the
// default): the model, pushed to the right it moves to the right, lifted
// it moves up, pulled toward the user it comes closer (zooms in), and
// turned it turns the same way about the mouse orbit's centre. Camera:
// the camera, every motion the other way round, still about the orbit's
// centre. Fly: the camera as in Camera, but turning about the eye (in
// perspective; in orthographic as Camera). The 3D mouse's own Lock horizon
// keeps world Z upright and takes no roll, whatever the mouse's
// Constrained orbit. A flat view (sketch mode) takes the pan and the zoom
// only, in the mode's direction, until Lock Rotation frees the rotation
// for that sketch (mitcad#1910, SpaceMouseController).

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <initializer_list>
#include <vector>

#include <QJsonArray>
#include <QJsonDocument>
#include <QSettings>
#include <QString>
#include <QStringList>

#include "../platform/SpaceMouseDevice.hpp"
#include "SpaceMouseButtons.hpp"

namespace mitcad {

// What follows the cap (mitcad#978): the model, the camera turning about
// the orbit's centre, or the camera turning about the eye.
enum class SpaceMouseMode { Object, Camera, Fly };

// The modes' names in the settings and the log.
inline const char* spaceMouseModeId(SpaceMouseMode mode) {
  switch (mode) {
  case SpaceMouseMode::Camera:
    return "camera";
  case SpaceMouseMode::Fly:
    return "fly";
  case SpaceMouseMode::Object:
    break;
  }
  return "object";
}

inline SpaceMouseMode spaceMouseModeFromId(const QString& id) {
  for (const SpaceMouseMode mode : {SpaceMouseMode::Camera, SpaceMouseMode::Fly}) {
    if (id == QLatin1String(spaceMouseModeId(mode))) {
      return mode;
    }
  }
  return SpaceMouseMode::Object;
}

struct SpaceMouseSettings {
  bool enabled = true;
  double speed = 1.0;
  SpaceMouseMode mode = SpaceMouseMode::Object;
  // World Z stays upright and the roll axis does nothing (mitcad#978; not
  // the mouse's Constrained orbit, whose value it takes only when the
  // settings have none of their own).
  bool lockHorizon = false;
  // Per axis (tx, ty, tz, rx, ry, rz): whether it moves the view, and
  // whether its direction is reversed.
  std::array<bool, 6> axisOn{{true, true, true, true, true, true}};
  std::array<bool, 6> reversed{{false, false, false, false, false, false}};
  bool lockRotation = false;
  bool lockTranslation = false;
  // Only the axis deflected most moves the view, held until the cap comes
  // to rest (SpaceMouseDominantAxis, mitcad#1720).
  bool dominantAxis = false;
  // The cross-talk filter (mitcad#1720), in percent, 0 off: an axis
  // deflected less than this share of the axis deflected most does nothing
  // in that frame. A cap that couples its axes moves the view along
  // several of them for a motion along one (a device moved along a single
  // axis was seen reporting 30-80 % of it on others), which reads as every
  // axis doing the same. The default, 50 %, drops a weaker axis that is
  // less than half the strongest, the stray part of a mildly coupled cap,
  // and still lets a deliberate motion along two axes (a diagonal pan, a
  // pan while zooming) move both whenever the weaker is at least half the
  // stronger, as a hand moving two ways on purpose gives them. A cap that
  // couples more takes a higher share, up to 90 %, or Dominant axis only.
  int crossTalk = 50;
  // The command each button runs (0 the first), empty for none.
  QStringList buttons{QStringLiteral("view.fit"), QStringLiteral("view.home")};
};

inline double sanitizeSpaceMouseSpeed(double speed) {
  return std::isfinite(speed) ? std::clamp(speed, 0.1, 4.0) : 1.0;
}

// The cross-talk filter's highest share, in percent: at 100 only axes
// deflected exactly as far as the strongest would move the view.
constexpr int kSpaceMouseCrossTalkMax = 90;

inline int sanitizeSpaceMouseCrossTalk(int percent) { return std::clamp(percent, 0, kSpaceMouseCrossTalkMax); }

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
  settings.mode = spaceMouseModeFromId(store.value(prefix + QStringLiteral("Mode")).toString());
  // Before mitcad#978 the 3D mouse kept the horizon with the mouse's
  // Constrained orbit: settings from then keep doing so.
  settings.lockHorizon =
      store.value(prefix + QStringLiteral("LockHorizon"), store.value(QStringLiteral("view/constrainedOrbit"), false))
          .toBool();
  settings.lockRotation = store.value(prefix + QStringLiteral("LockRotation"), false).toBool();
  settings.lockTranslation = store.value(prefix + QStringLiteral("LockTranslation"), false).toBool();
  settings.dominantAxis = store.value(prefix + QStringLiteral("DominantAxis"), false).toBool();
  const int crossTalk = store.value(prefix + QStringLiteral("CrossTalk"), settings.crossTalk).toInt(&ok);
  settings.crossTalk = ok ? sanitizeSpaceMouseCrossTalk(crossTalk) : SpaceMouseSettings().crossTalk;
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
  store.setValue(prefix + QStringLiteral("Mode"), QLatin1String(spaceMouseModeId(settings.mode)));
  store.setValue(prefix + QStringLiteral("LockHorizon"), settings.lockHorizon);
  store.setValue(prefix + QStringLiteral("LockRotation"), settings.lockRotation);
  store.setValue(prefix + QStringLiteral("LockTranslation"), settings.lockTranslation);
  store.setValue(prefix + QStringLiteral("DominantAxis"), settings.dominantAxis);
  store.setValue(prefix + QStringLiteral("CrossTalk"), sanitizeSpaceMouseCrossTalk(settings.crossTalk));
  QJsonArray buttons;
  for (const QString& id : settings.buttons) {
    buttons.append(id);
  }
  store.setValue(prefix + QStringLiteral("Buttons"), QJsonDocument(buttons).toJson(QJsonDocument::Compact));
}

// Lock Rotation (mitcad#1910): the checkable view command a button can run
// to lock and free the 3D mouse's rotation.
inline QString spaceMouseLockRotationCommand() { return QStringLiteral("view.space_mouse_lock_rotation"); }

// Whether the 3D mouse's rotation is locked (mitcad#1910): in a flat view
// (a sketch) until Lock Rotation frees it for that sketch
// (`freedInSketch`), whatever the settings say; elsewhere as the
// settings' Lock rotation says.
inline bool spaceMouseRotationLocked(const SpaceMouseSettings& settings, bool flat, bool freedInSketch) {
  return flat ? !freedInSketch : settings.lockRotation;
}

// A device's HID vendor and product ids from its name as a backend gives
// it, "SpaceMouse Pro (HID 046d:c62b)" or "HID 046d:c62b" (Raw Input and
// IOKit HID; spacenavd's socket names no device): false when the name has
// none.
inline bool spaceMouseHidIds(const QString& device, int& vendor, int& product) {
  const int at = int(device.lastIndexOf(QStringLiteral("HID ")));
  if (at < 0) {
    return false;
  }
  const QString ids = device.mid(at + 4, 9);
  if (ids.size() != 9 || ids.at(4) != QLatin1Char(':')) {
    return false;
  }
  bool vendorOk = false;
  bool productOk = false;
  const int readVendor = ids.left(4).toInt(&vendorOk, 16);
  const int readProduct = ids.mid(5, 4).toInt(&productOk, 16);
  if (!vendorOk || !productOk) {
    return false;
  }
  vendor = readVendor;
  product = readProduct;
  return true;
}

// The button (0 the first) of a device's rotation lock key, by its HID ids,
// or -1 when its number is not known. Without the vendor's driver the key
// is a plain numbered button of the device's button report. Known: the
// SpaceMouse Pro (product c62b) and the SpaceMouse Pro Wireless (c631 by
// cable, c632 through its receiver, c638), vendor 046d or 256f, whose
// lock key is the report's bit 26, button 27. A universal receiver (c652)
// is left out: the device behind it may have no such key. The SpaceMouse
// Enterprise (c633, mitcad#2642) numbers its keys as the Pro does (its
// Rotation Toggle the same virtual key 27, SpaceMouseButtons.hpp).
inline int spaceMouseLockKeyButton(int vendor, int product) {
  if (vendor != 0x046d && vendor != 0x256f) {
    return -1;
  }
  switch (product) {
  case 0xc62b:
  case 0xc631:
  case 0xc632:
  case 0xc633:
  case 0xc638:
    return 26;
  default:
    return -1;
  }
}

// A button's command that a device brings by default.
struct SpaceMouseDefaultButton {
  int button = -1; // 0 the first
  QString command;
};

// A device's labeled keys' default commands, by its HID ids, lowest button
// first: its rotation lock key's Lock Rotation (mitcad#1910) and, on a
// SpaceMouse Enterprise, its QuickView keys' standard views (mitcad#2642):
// T/B Top (button 3), R/L Right (5), F/BK Front (6), each held the
// opposite view (SpaceMouseButtons.hpp), and the held views' own numbers,
// Left (4), Bottom (7) and Back (8), for a route that sends those instead
// of a hold.
inline std::vector<SpaceMouseDefaultButton> spaceMouseDeviceDefaultButtons(int vendor, int product) {
  std::vector<SpaceMouseDefaultButton> defaults;
  if (isSpaceMouseEnterprise(vendor, product)) {
    defaults = {{2, QStringLiteral("view.top")},   {3, QStringLiteral("view.left")},
                {4, QStringLiteral("view.right")}, {5, QStringLiteral("view.front")},
                {6, QStringLiteral("view.bottom")}, {7, QStringLiteral("view.back")}};
  }
  const int key = spaceMouseLockKeyButton(vendor, product);
  if (key >= 0) {
    defaults.push_back({key, spaceMouseLockRotationCommand()});
  }
  return defaults;
}

// A connected device's default buttons (mitcad#1910, mitcad#2642): each
// of its labeled keys (spaceMouseDeviceDefaultButtons) runs its default
// command where the settings have no entry for that button yet (their
// list ended before it); the list grows with empty entries (None) up to
// it. Every other button, and a key the user has bound or set to None,
// stays as it is. True when it changed the settings; `added` gets the
// buttons it bound.
inline bool addSpaceMouseDeviceButtons(SpaceMouseSettings& settings, const QString& device,
                                       std::vector<int>* added = nullptr) {
  int vendor = 0;
  int product = 0;
  if (!spaceMouseHidIds(device, vendor, product)) {
    return false;
  }
  const int entries = int(settings.buttons.size());
  bool changed = false;
  for (const SpaceMouseDefaultButton& entry : spaceMouseDeviceDefaultButtons(vendor, product)) {
    if (entry.button < entries || entry.button >= 32) {
      continue;
    }
    while (int(settings.buttons.size()) <= entry.button) {
      settings.buttons.append(QString());
    }
    settings.buttons[entry.button] = entry.command;
    if (added != nullptr) {
      added->push_back(entry.button);
    }
    changed = true;
  }
  return changed;
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

// One axis's deflection, 0 to a full one (beyond a full one counts as a
// full one, as in spaceMouseResponse; a value that is no number as none).
inline double spaceMouseDeflection(double value) {
  return std::isfinite(value) ? std::min(std::abs(value), 1.0) : 0.0;
}

// Whether any axis is deflected past the dead zone (mitcad#1981).
inline bool spaceMouseDeflected(const SpaceMouseAxes& axes) {
  return std::any_of(axes.begin(), axes.end(),
                     [](double value) { return spaceMouseDeflection(value) > kSpaceMouseDeadzone; });
}

// How far a device's cap goes (mitcad#1720): the largest deflection it has
// shown of each kind, translation and rotation, so that axes are compared
// as shares of their own kind's full deflection and one kind does not win
// by its scale. The devices report both kinds on one nominal scale
// (kSpaceMouseFullScale), but caps reach them unevenly: a SpaceMouse
// Compact was seen reaching 350 counts in translation and 280 in rotation,
// a SpaceNavigator 190 in translation and 340 in rotation. A reach starts
// at `start` for both kinds (the same, so that a new device's axes compare
// as they come) and grows with every deflection beyond it, up to a full
// one. The default start, a full deflection, is the nominal scale and
// learns nothing; the controller starts each device at
// kSpaceMouseReachStart.
class SpaceMouseReach {
public:
  explicit SpaceMouseReach(double start = 1.0)
      : m_start(sanitized(start)), m_translation(m_start), m_rotation(m_start) {}

  // A frame's axes: a deflection beyond its kind's reach widens it.
  void add(const SpaceMouseAxes& axes) {
    for (std::size_t axis = 0; axis < 6; ++axis) {
      double& reach = axis < 3 ? m_translation : m_rotation;
      reach = std::max(reach, spaceMouseDeflection(axes[axis]));
    }
  }
  // The reach of an axis's kind (tx, ty, tz the translation; rx, ry, rz
  // the rotation).
  double of(std::size_t axis) const { return axis < 3 ? m_translation : m_rotation; }
  // An axis's deflection as a share of its kind's reach, what the
  // comparisons between axes use.
  double share(std::size_t axis, double value) const { return spaceMouseDeflection(value) / of(axis); }
  void reset() { m_translation = m_rotation = m_start; }

private:
  static double sanitized(double start) { return std::isfinite(start) ? std::clamp(start, 0.1, 1.0) : 1.0; }
  double m_start;
  double m_translation;
  double m_rotation;
};

// The reach a device starts with, in both kinds, until it shows more.
constexpr double kSpaceMouseReachStart = 0.5;
// Dominant axis only (mitcad#1720): the deflection the strongest axis must
// reach before it is chosen, and before another axis may take over from
// the held one. Above the dead zone, so that the first stray counts of a
// motion (a cap pushed in was seen moving 0.06-0.09 up first) choose
// nothing, and so that the axes that linger as the cap returns to rest
// (seen up to 0.08 after the held axis was back in the dead zone) do not
// take over.
constexpr double kSpaceMouseDominantChoice = 0.1;
// How many times the held axis's share of its reach another axis's share
// must be before it takes over: a cap pushed left was seen reporting up to
// 1.34 times as much in and out as left during the push.
constexpr double kSpaceMouseDominantMargin = 1.5;

// Dominant axis only with a hold (mitcad#1720, GitHub mitcad/Mitcad#343):
// the one axis that moves the view. The axis is chosen once the strongest
// eligible axis (switched on and not locked) is deflected
// kSpaceMouseDominantChoice, and kept until the cap is back at rest (every
// axis, eligible or not, in the dead zone); before that another axis takes
// over only when it is deflected as far and its share of its reach is
// more than kSpaceMouseDominantMargin times the held one's (a held axis
// switched off or locked meanwhile counts as none). Axes are compared as
// shares of their kind's reach. Choosing again at every frame let a cap
// that couples its axes flip between two of similar size during one
// motion: the view alternated between them, mixed motion and jumps.
class SpaceMouseDominantAxis {
public:
  // The axis (0 tx … 5 rz) that moves the view for the device's `axes` this
  // frame, or -1 for none.
  int update(const SpaceMouseAxes& axes, const std::array<bool, 6>& eligible, const SpaceMouseReach& reach) {
    bool rest = true;
    for (const double value : axes) {
      if (spaceMouseDeflection(value) > kSpaceMouseDeadzone) {
        rest = false;
      }
    }
    if (rest) {
      m_held = -1;
      return m_held;
    }
    std::array<double, 6> shares{};
    int strongest = -1;
    for (std::size_t axis = 0; axis < 6; ++axis) {
      if (eligible[axis] && spaceMouseDeflection(axes[axis]) > kSpaceMouseDeadzone) {
        shares[axis] = reach.share(axis, axes[axis]);
        if (strongest < 0 || shares[axis] > shares[static_cast<std::size_t>(strongest)]) {
          strongest = static_cast<int>(axis);
        }
      }
    }
    if (strongest < 0 || strongest == m_held ||
        spaceMouseDeflection(axes[static_cast<std::size_t>(strongest)]) < kSpaceMouseDominantChoice) {
      return m_held;
    }
    if (m_held < 0 || shares[static_cast<std::size_t>(strongest)] >
                          kSpaceMouseDominantMargin * shares[static_cast<std::size_t>(m_held)]) {
      m_held = strongest;
    }
    return m_held;
  }
  int held() const { return m_held; }
  void reset() { m_held = -1; }

private:
  int m_held = -1;
};

// What the 3D mouse's mapping keeps from one frame to the next
// (mitcad#1720): the device's reach and Dominant axis only's held axis.
// The controller keeps one for its device; without one, every frame is
// taken as a motion's first, on the nominal scale.
struct SpaceMouseMemory {
  SpaceMouseReach reach{kSpaceMouseReachStart};
  SpaceMouseDominantAxis dominant;
};

// The cross-talk filter (mitcad#1720): the device's axes with every axis
// deflected less than `percent` % of the axis deflected most set to zero,
// each deflection a share of its kind's `reach` (SpaceMouseReach; by
// default the nominal scale); 0 keeps them all. It compares the device's
// own values, before the response curve, the axes switched off and the
// locks: the coupling is the cap's, so a motion along an axis switched off
// or locked does not leak into the others either.
inline SpaceMouseAxes spaceMouseWithoutCrossTalk(const SpaceMouseAxes& axes, int percent,
                                                 const SpaceMouseReach& reach = SpaceMouseReach()) {
  SpaceMouseAxes result = axes;
  const int share = sanitizeSpaceMouseCrossTalk(percent);
  if (share == 0) {
    return result;
  }
  double strongest = 0.0;
  for (std::size_t axis = 0; axis < 6; ++axis) {
    strongest = std::max(strongest, reach.share(axis, axes[axis]));
  }
  const double least = strongest * share / 100.0;
  for (std::size_t axis = 0; axis < 6; ++axis) {
    if (!(std::isfinite(result[axis]) && reach.share(axis, result[axis]) >= least)) {
      result[axis] = 0.0;
    }
  }
  return result;
}

// The axes that move the view: the cross-talk filter (unless the dominant
// axis alone moves it), the response curve, the axes switched off, the
// locks (a flat view has its rotation locked), the reversed directions and
// the dominant axis. `memory` (the controller's) learns the device's reach
// and holds the dominant axis; without it the frame stands alone.
inline SpaceMouseAxes spaceMouseFiltered(const SpaceMouseSettings& settings, const SpaceMouseAxes& axes, bool flat,
                                         SpaceMouseMemory* memory = nullptr) {
  SpaceMouseReach nominal;
  SpaceMouseDominantAxis single;
  const SpaceMouseReach* reach = &nominal;
  SpaceMouseDominantAxis* dominant = &single;
  if (memory != nullptr) {
    memory->reach.add(axes);
    reach = &memory->reach;
    dominant = &memory->dominant;
  }
  std::array<bool, 6> eligible{};
  for (std::size_t axis = 0; axis < 6; ++axis) {
    const bool rotation = axis >= 3;
    eligible[axis] = settings.axisOn[axis] && !(rotation && (settings.lockRotation || flat)) &&
                     !(!rotation && settings.lockTranslation);
  }
  int held = -1;
  if (settings.dominantAxis) {
    held = dominant->update(axes, eligible, *reach);
  } else {
    dominant->reset();
  }
  const SpaceMouseAxes device =
      settings.dominantAxis ? axes : spaceMouseWithoutCrossTalk(axes, settings.crossTalk, *reach);
  SpaceMouseAxes result{};
  for (std::size_t axis = 0; axis < 6; ++axis) {
    if (!eligible[axis] || (settings.dominantAxis && static_cast<int>(axis) != held)) {
      continue;
    }
    result[axis] = spaceMouseResponse(device[axis]) * (settings.reversed[axis] ? -1.0 : 1.0);
  }
  return result;
}

// What the camera does per second, as the cap asks for it in Object mode
// (the model following the cap); spaceMouseMove turns it round for the
// other modes.
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

// `memory` as in spaceMouseFiltered.
inline SpaceMouseRates spaceMouseRates(const SpaceMouseSettings& settings, const SpaceMouseAxes& axes, bool flat,
                                       SpaceMouseMemory* memory = nullptr) {
  if (!settings.enabled) {
    return {};
  }
  const SpaceMouseAxes moved = spaceMouseFiltered(settings, axes, flat, memory);
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

// A motion's axes for the debug log (mitcad#1720), on every platform: the
// largest deflection of each axis while the cap moved (sign kept, the
// view's frame, before an axis's Reverse), and what the strongest axis
// does to the model in Object mode, so that a check with a device reads
// from the log alone whether each motion of the cap follows it. For
// example "3D mouse axes: tx 0.00, ty 0.00, tz 0.00, rx -0.62, ry 0.03,
// rz 0.00; strongest rx: in Object mode the model tilts its top away from
// the user".
class SpaceMouseAxesLog {
public:
  // Axes that moved the view.
  void add(const SpaceMouseAxes& axes) {
    for (std::size_t axis = 0; axis < 6; ++axis) {
      if (std::isfinite(axes[axis]) && std::abs(axes[axis]) > std::abs(m_peak[axis])) {
        m_peak[axis] = axes[axis];
      }
    }
    m_moved = true;
  }

  // What each axis, positive and negative, does to the model in Object
  // mode.
  static const char* objectMotion(std::size_t axis, bool positive) {
    static const char* const motions[6][2] = {
        {"moves left", "moves right"},
        {"moves down", "moves up"},
        {"moves away", "comes closer"},
        {"tilts its top away from the user", "tilts its top toward the user"},
        {"turns clockwise seen from above", "turns counter-clockwise seen from above"},
        {"rolls clockwise as the user sees it", "rolls counter-clockwise as the user sees it"}};
    return axis < 6 ? motions[axis][positive ? 1 : 0] : "";
  }

  // The line once the cap has come to rest after a motion (empty when
  // nothing moved since the last one); starts the next motion afresh.
  QString take() {
    if (!m_moved) {
      return QString();
    }
    QString text = QStringLiteral("3D mouse axes: ");
    std::size_t strongest = 0;
    for (std::size_t axis = 0; axis < 6; ++axis) {
      if (axis > 0) {
        text += QStringLiteral(", ");
      }
      text += QStringLiteral("%1 %2").arg(QLatin1String(spaceMouseAxisKey(axis))).arg(m_peak[axis], 0, 'f', 2);
      if (std::abs(m_peak[axis]) > std::abs(m_peak[strongest])) {
        strongest = axis;
      }
    }
    if (m_peak[strongest] != 0.0) {
      text += QStringLiteral("; strongest %1: in Object mode the model %2")
                  .arg(QLatin1String(spaceMouseAxisKey(strongest)),
                       QLatin1String(objectMotion(strongest, m_peak[strongest] > 0.0)));
    }
    m_peak = SpaceMouseAxes{};
    m_moved = false;
    return text;
  }

private:
  SpaceMouseAxes m_peak{};
  bool m_moved = false;
};

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

// How spaceMouseMove follows the cap (mitcad#978): the Navigation mode, the
// 3D mouse's Lock horizon, and whether the view is in perspective (Fly
// turns about the eye only then).
struct SpaceMouseStyle {
  SpaceMouseMode mode = SpaceMouseMode::Object;
  bool lockHorizon = false;
  bool perspective = true;
};

// The camera after `seconds` of `rates`: panned across the view, turned
// and zoomed. Object mode moves the camera opposite to the cap, the model
// following it; Camera and Fly move it with the cap. Object and Camera
// turn about `pivot` (the orbit's centre), Fly in perspective about the
// eye. With Lock horizon it turns about Z and across the view only, never
// rolls, and stops a degree short of looking straight along Z. A pose
// that makes no view comes back unchanged.
inline SpaceMousePose spaceMouseMove(const SpaceMousePose& pose, const SpaceMouseVec& pivot,
                                     const SpaceMouseRates& asked, double seconds, const SpaceMouseStyle& style) {
  using namespace spacemouse_detail;
  // The camera with the cap: every rate the other way round. An axis's
  // Reverse (spaceMouseRates) turns it round again.
  SpaceMouseRates rates = asked;
  if (style.mode != SpaceMouseMode::Object) {
    for (double* rate : {&rates.panX, &rates.panY, &rates.zoom, &rates.pitch, &rates.yaw, &rates.roll}) {
      *rate = -*rate;
    }
  }
  const bool aboutEye = style.mode == SpaceMouseMode::Fly && style.perspective;
  const bool zUp = style.lockHorizon;
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
  // The model moves with the cap: the camera the other way (in Object
  // mode; the rates are turned round for the others).
  const SpaceMouseVec offset =
      scale(add(scale(right, rates.panX), scale(up, rates.panY)),
            -seconds * (pose.panHeight > 0.0 && std::isfinite(pose.panHeight) ? pose.panHeight : pose.height));
  result.eye = add(result.eye, offset);
  result.target = add(result.target, offset);
  // Fly turns in place: about the eye, which the turns then keep.
  const SpaceMouseVec centre = aboutEye ? result.eye : pivot;
  if (zUp) {
    const SpaceMouseVec z{{0.0, 0.0, dot(up, SpaceMouseVec{{0.0, 0.0, 1.0}}) < 0.0 ? -1.0 : 1.0}};
    turn(result, centre, z, -rates.yaw * seconds);
    SpaceMousePose pitched = result;
    const SpaceMouseVec turnedRight = cross(scale(sub(result.target, result.eye), 1.0 / distance), result.up);
    const double sideways = length(turnedRight);
    if (sideways > 1e-12) {
      turn(pitched, centre, scale(turnedRight, 1.0 / sideways), -rates.pitch * seconds);
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
    // camera by the opposite about the centre.
    const SpaceMouseVec toward = scale(direction, -1.0);
    const SpaceMouseVec spin =
        add(add(scale(right, rates.pitch), scale(up, rates.yaw)), scale(toward, rates.roll));
    const double rate = length(spin);
    if (rate > 0.0) {
      turn(result, centre, scale(spin, 1.0 / rate), -rate * seconds);
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
// is connected, and how many buttons the device has used so far; whether
// the vendor's driver runs (set as Preferences opens, mitcad#1981: its
// tab then says to turn off the driver's emulated input).
struct SpaceMouseStatus {
  QString text;
  int buttonsSeen = 0;
  bool vendorDriver = false;
};

inline SpaceMouseStatus& spaceMouseStatus() {
  static SpaceMouseStatus status;
  return status;
}

} // namespace mitcad
