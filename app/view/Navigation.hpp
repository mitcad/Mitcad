// SPDX-License-Identifier: MIT
#pragma once

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
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
};

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
