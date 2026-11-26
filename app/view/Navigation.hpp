// SPDX-License-Identifier: MIT
#pragma once

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QPointF>
#include <QSettings>
#include <QString>
#include <QVector>
#include <Qt>

#include "SpaceMouseNavigation.hpp"

namespace mitcad {

// Preset indices remain stable for settings written before named schemes.
enum class NavigationScheme {
  MiddlePan, AltButtons, MiddlePanF4, MiddleOrbit, RightOrbit, Trackpad, TrackpadShift, Custom,
};

enum class NavigationAction { None, Orbit, Pan, Zoom };
enum class NavigationInput { LeftButton, MiddleButton, RightButton, TrackpadScroll };

// What a mouse orbit turns about (mitcad#1791, `view/orbitCenter`): the
// middle of what is shown, or the model point under the cursor where the
// orbit starts (the middle when the cursor is over empty space). A fixed
// center set from the view's context menu comes before either.
enum class OrbitCenter { Model, Cursor };

inline const char* orbitCenterId(OrbitCenter center) {
  return center == OrbitCenter::Cursor ? "cursor" : "model";
}

inline OrbitCenter orbitCenterFromId(const QString& id) {
  return id == QLatin1String("cursor") ? OrbitCenter::Cursor : OrbitCenter::Model;
}

struct NavigationBinding {
  NavigationInput input = NavigationInput::MiddleButton;
  Qt::KeyboardModifiers modifiers = Qt::NoModifier;
  bool f4 = false;
  NavigationAction action = NavigationAction::None;
};

struct NavigationSettings {
  NavigationScheme scheme = NavigationScheme::MiddlePan;
  bool zoomToCursor = true;
  bool reverseZoom = false;
  bool constrainedOrbit = false;
  bool animate = true;
  QVector<NavigationBinding> customBindings;
  double orbitSpeed = 1.0;
  double panSpeed = 1.0;
  double zoomSpeed = 1.0;
  double trackpadSpeed = 1.0;
  // The 3D mouse (mitcad#951, SpaceMouseNavigation.hpp).
  SpaceMouseSettings spaceMouse;
  // The arrow keys' turn of the view (mitcad#1211, `view/turnStep`), in
  // degrees, 1 to 90.
  double turnStep = 15.0;
  // What an orbit turns about (mitcad#1791).
  OrbitCenter orbitCenter = OrbitCenter::Model;
  // The two-finger trackpad scroll's pan, each axis on its own, and its
  // orbit the other way (mitcad#1873, GitHub mitcad/Mitcad#340;
  // `view/reversePanHorizontal`, `view/reversePanVertical`,
  // `view/reverseScrollOrbit`): mouse drags keep their direction.
  bool reversePanHorizontal = false;
  bool reversePanVertical = false;
  bool reverseScrollOrbit = false;
};

// How far a two-finger scroll moves the view's pan or orbit drag, from the
// scroll's delta in pixels (mitcad#1873): a pan's horizontal and vertical
// parts reversed on their own, an orbit reversed as a whole, as the
// settings say. Zoom and other actions keep the delta.
inline QPointF scrollDragDelta(const NavigationSettings& settings, NavigationAction action, QPointF delta) {
  if (action == NavigationAction::Pan) {
    if (settings.reversePanHorizontal) {
      delta.setX(-delta.x());
    }
    if (settings.reversePanVertical) {
      delta.setY(-delta.y());
    }
  } else if (action == NavigationAction::Orbit && settings.reverseScrollOrbit) {
    delta = -delta;
  }
  return delta;
}

// Whether Qt's platform plugin swapped a trackpad scroll's axes because Alt
// is held, so the view swaps them back (a trackpad's two fingers keep their
// own directions under Alt, and Alt + scroll orbits the way they move). Qt
// swaps the axes of every scroll while Alt is held, so that Alt + a wheel
// scrolls sideways:
// - xcb (GitHub mitcad/Mitcad#381): a touchpad's scroll arrives as wheel
//   steps, without phase or pixel deltas, so only its touchpad device tells
//   it from a wheel;
// - Wayland (GitHub mitcad/Mitcad#662): Qt gives pixel deltas only for a
//   finger or continuous scroll, never for a wheel, so they tell a
//   trackpad's scroll; its device is no help there.
// Other platforms do not swap. A wheel keeps Qt's swap everywhere.
inline bool trackpadScrollAxesSwapped(const QString& platform, Qt::KeyboardModifiers keys, bool touchpadDevice,
                                      bool hasPixelDelta) {
  if (!keys.testFlag(Qt::AltModifier)) {
    return false;
  }
  if (platform == QLatin1String("xcb")) {
    return touchpadDevice;
  }
  // "wayland", "wayland-egl" and the other Wayland plugins.
  if (platform.startsWith(QLatin1String("wayland"))) {
    return hasPixelDelta;
  }
  return false;
}

// The arrow keys' step: 1 to 90 degrees, 15 when it is no number.
inline double sanitizeTurnStep(double degrees) {
  return std::isfinite(degrees) ? std::clamp(degrees, 1.0, 90.0) : 15.0;
}

inline double sanitizeNavigationSpeed(double speed) {
  return std::isfinite(speed) ? std::clamp(speed, 0.1, 4.0) : 1.0;
}

inline bool navigationBindingAllowed(const NavigationBinding& binding) {
  const auto keys = Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier;
  if ((binding.modifiers & ~keys) != Qt::NoModifier ||
      int(binding.input) < 0 || int(binding.input) > int(NavigationInput::TrackpadScroll) ||
      int(binding.action) < 0 || int(binding.action) > int(NavigationAction::Zoom)) {
    return false;
  }
  if (binding.f4) {
    return binding.input == NavigationInput::LeftButton && binding.modifiers == Qt::NoModifier;
  }
  // Left drag without Alt belongs to selection and component dragging. The
  // orientation cube always keeps its own gesture, regardless of this table.
  return binding.input != NavigationInput::LeftButton || binding.modifiers.testFlag(Qt::AltModifier);
}

inline QVector<NavigationBinding> navigationBindings(NavigationScheme scheme) {
  using Action = NavigationAction;
  using Input = NavigationInput;
  QVector<NavigationBinding> bindings;
  for (int input = 0; input <= int(Input::TrackpadScroll); ++input) {
    for (int mask = 0; mask < 8; ++mask) {
      const auto keys = (mask & 1 ? Qt::ShiftModifier : Qt::NoModifier) |
                        (mask & 2 ? Qt::ControlModifier : Qt::NoModifier) |
                        (mask & 4 ? Qt::AltModifier : Qt::NoModifier);
      bindings.push_back({Input(input), keys, false, Action::None});
    }
  }
  bindings.push_back({Input::LeftButton, Qt::NoModifier, true, Action::None});
  const auto bind = [&](Input input, Qt::KeyboardModifiers keys, Action action, bool f4 = false) {
    for (auto& binding : bindings) {
      if (binding.input == input && binding.modifiers == keys && binding.f4 == f4) {
        binding.action = action;
        return;
      }
    }
  };
  const bool trackpad = scheme == NavigationScheme::Trackpad || scheme == NavigationScheme::TrackpadShift;
  for (auto& binding : bindings) {
    if (binding.input == Input::TrackpadScroll) {
      const bool shiftOrbit = scheme == NavigationScheme::TrackpadShift;
      const auto orbitKey = shiftOrbit ? Qt::ShiftModifier : Qt::AltModifier;
      const auto zoomKey = shiftOrbit ? Qt::AltModifier : Qt::ShiftModifier;
      binding.action = !trackpad || (binding.modifiers & (zoomKey | Qt::ControlModifier)) ? Action::Zoom
                       : binding.modifiers.testFlag(orbitKey) ? Action::Orbit : Action::Pan;
    }
  }
  switch (scheme) {
  case NavigationScheme::MiddlePan:
  case NavigationScheme::MiddlePanF4:
    bind(Input::MiddleButton, Qt::NoModifier, Action::Pan);
    bind(Input::MiddleButton, Qt::ShiftModifier, Action::Orbit);
    if (scheme == NavigationScheme::MiddlePanF4) {
      bind(Input::LeftButton, Qt::NoModifier, Action::Orbit, true);
    }
    break;
  case NavigationScheme::AltButtons:
    bind(Input::LeftButton, Qt::AltModifier, Action::Orbit);
    bind(Input::MiddleButton, Qt::AltModifier, Action::Pan);
    bind(Input::RightButton, Qt::AltModifier, Action::Zoom);
    bind(Input::MiddleButton, Qt::NoModifier, Action::Pan);
    break;
  case NavigationScheme::MiddleOrbit:
    bind(Input::MiddleButton, Qt::NoModifier, Action::Orbit);
    bind(Input::MiddleButton, Qt::ControlModifier, Action::Pan);
    bind(Input::MiddleButton, Qt::ShiftModifier, Action::Zoom);
    break;
  case NavigationScheme::RightOrbit:
    bind(Input::RightButton, Qt::NoModifier, Action::Orbit);
    bind(Input::RightButton, Qt::ShiftModifier, Action::Pan);
    bind(Input::MiddleButton, Qt::NoModifier, Action::Pan);
    break;
  case NavigationScheme::Trackpad:
  case NavigationScheme::TrackpadShift:
    bind(Input::LeftButton, Qt::AltModifier, Action::Orbit);
    bind(Input::LeftButton, Qt::AltModifier | Qt::ShiftModifier, Action::Pan);
    bind(Input::MiddleButton, Qt::NoModifier, Action::Pan);
    bind(Input::MiddleButton, Qt::ShiftModifier, Action::Orbit);
    bind(Input::RightButton, Qt::NoModifier, Action::Orbit);
    bind(Input::RightButton, Qt::ShiftModifier, Action::Pan);
    break;
  case NavigationScheme::Custom:
    break;
  }
  return bindings;
}

inline NavigationAction navigationAction(const NavigationSettings& settings, NavigationInput input,
                                         Qt::KeyboardModifiers modifiers, bool f4 = false) {
  if (settings.scheme == NavigationScheme::MiddlePanF4 && input == NavigationInput::LeftButton && f4) {
    return NavigationAction::Orbit; // the legacy F4 gesture ignores modifiers
  }
  modifiers &= Qt::ShiftModifier | Qt::ControlModifier | Qt::AltModifier;
  const auto bindings = settings.scheme == NavigationScheme::Custom ? settings.customBindings
                                                                   : navigationBindings(settings.scheme);
  for (const auto& binding : bindings) {
    if (binding.input == input && binding.modifiers == modifiers && binding.f4 == f4 &&
        navigationBindingAllowed(binding)) {
      return binding.action;
    }
  }
  return NavigationAction::None;
}

// Store only recognized table rows. Loading a duplicate, reserved gesture or
// unknown enum never installs an extra OCCT gesture from hand-edited settings.
inline QVector<NavigationBinding> normalizedNavigationBindings(const QVector<NavigationBinding>& source,
                                                               NavigationScheme fallback) {
  auto result = navigationBindings(fallback);
  for (auto& row : result) {
    for (const auto& candidate : source) {
      if (row.input == candidate.input && row.modifiers == candidate.modifiers && row.f4 == candidate.f4 &&
          navigationBindingAllowed(candidate)) {
        row.action = candidate.action;
        break;
      }
    }
  }
  return result;
}

inline void loadNavigationDetails(const QSettings& store, NavigationSettings& settings) {
#ifdef Q_OS_MACOS
  constexpr auto fallback = NavigationScheme::Trackpad;
#else
  constexpr auto fallback = NavigationScheme::MiddlePan;
#endif
  const QString prefix = QStringLiteral("view/");
  const auto speed = [&](const char* name) {
    bool ok = false;
    const double value = store.value(prefix + QLatin1String(name), 1.0).toDouble(&ok);
    return ok ? sanitizeNavigationSpeed(value) : 1.0;
  };
  settings.orbitSpeed = speed("orbitSpeed");
  settings.panSpeed = speed("panSpeed");
  settings.zoomSpeed = speed("zoomSpeed");
  settings.trackpadSpeed = speed("trackpadSpeed");
  bool stepOk = false;
  const double step = store.value(prefix + QStringLiteral("turnStep"), 15.0).toDouble(&stepOk);
  settings.turnStep = stepOk ? sanitizeTurnStep(step) : 15.0;
  settings.orbitCenter = orbitCenterFromId(store.value(prefix + QStringLiteral("orbitCenter")).toString());
  // The trackpad scroll's directions (mitcad#1873), off unless saved on.
  settings.reversePanHorizontal = store.value(prefix + QStringLiteral("reversePanHorizontal"), false).toBool();
  settings.reversePanVertical = store.value(prefix + QStringLiteral("reversePanVertical"), false).toBool();
  settings.reverseScrollOrbit = store.value(prefix + QStringLiteral("reverseScrollOrbit"), false).toBool();
  loadSpaceMouseSettings(store, settings.spaceMouse);
  QVector<NavigationBinding> bindings;
  const QJsonDocument document = QJsonDocument::fromJson(
      store.value(prefix + QStringLiteral("navigationBindings")).toByteArray());
  if (document.isArray()) {
    for (const auto& value : document.array()) {
      if (!value.isObject()) {
        continue;
      }
      const auto row = value.toObject();
      if (!row.value(QStringLiteral("input")).isDouble() ||
          !row.value(QStringLiteral("modifiers")).isDouble() ||
          !row.value(QStringLiteral("f4")).isBool() || !row.value(QStringLiteral("action")).isDouble()) {
        continue;
      }
      const NavigationBinding binding{
          NavigationInput(row.value(QStringLiteral("input")).toInt(-1)),
          Qt::KeyboardModifiers(row.value(QStringLiteral("modifiers")).toInt(-1)),
          row.value(QStringLiteral("f4")).toBool(),
          NavigationAction(row.value(QStringLiteral("action")).toInt(-1))};
      if (navigationBindingAllowed(binding)) {
        bindings.push_back(binding);
      }
    }
  }
  if (!store.contains(prefix + QStringLiteral("navigationBindings")) &&
      settings.scheme != NavigationScheme::Custom) {
    // An untouched Custom table must stay absent: choosing Custom can then
    // start from the preset currently selected in Preferences.
    settings.customBindings.clear();
  } else {
    settings.customBindings = normalizedNavigationBindings(bindings, fallback);
  }
}

inline void saveNavigationDetails(QSettings& store, const NavigationSettings& settings) {
  const QString prefix = QStringLiteral("view/");
  store.setValue(prefix + QStringLiteral("orbitSpeed"), sanitizeNavigationSpeed(settings.orbitSpeed));
  store.setValue(prefix + QStringLiteral("panSpeed"), sanitizeNavigationSpeed(settings.panSpeed));
  store.setValue(prefix + QStringLiteral("zoomSpeed"), sanitizeNavigationSpeed(settings.zoomSpeed));
  store.setValue(prefix + QStringLiteral("trackpadSpeed"), sanitizeNavigationSpeed(settings.trackpadSpeed));
  store.setValue(prefix + QStringLiteral("turnStep"), sanitizeTurnStep(settings.turnStep));
  store.setValue(prefix + QStringLiteral("orbitCenter"), QLatin1String(orbitCenterId(settings.orbitCenter)));
  store.setValue(prefix + QStringLiteral("reversePanHorizontal"), settings.reversePanHorizontal);
  store.setValue(prefix + QStringLiteral("reversePanVertical"), settings.reversePanVertical);
  store.setValue(prefix + QStringLiteral("reverseScrollOrbit"), settings.reverseScrollOrbit);
  saveSpaceMouseSettings(store, settings.spaceMouse);
  if (settings.customBindings.isEmpty()) {
    store.remove(prefix + QStringLiteral("navigationBindings"));
    return;
  }
  QJsonArray bindings;
  for (const auto& binding : normalizedNavigationBindings(settings.customBindings, NavigationScheme::Custom)) {
    if (navigationBindingAllowed(binding)) {
      bindings.push_back(QJsonObject{{QStringLiteral("input"), int(binding.input)},
                                    {QStringLiteral("modifiers"), int(binding.modifiers)},
                                    {QStringLiteral("f4"), binding.f4},
                                    {QStringLiteral("action"), int(binding.action)}});
    }
  }
  store.setValue(prefix + QStringLiteral("navigationBindings"), QJsonDocument(bindings).toJson(QJsonDocument::Compact));
}

} // namespace mitcad
