// SPDX-License-Identifier: MIT
#pragma once

#include <limits>

#include <QTemporaryDir>

#include "view/Navigation.hpp"

namespace mitcad::tests {

inline void navigationSettingsTests(void (*verify)(bool, const char*, int)) {
#define NAV_CHECK(expression) verify((expression), #expression, __LINE__)
  using Action = NavigationAction;
  using Input = NavigationInput;
  NavigationSettings settings;
  NAV_CHECK(settings.scheme == NavigationScheme::MiddlePan);
  NAV_CHECK(settings.orbitSpeed == 1.0 && settings.panSpeed == 1.0 && settings.zoomSpeed == 1.0 &&
            settings.trackpadSpeed == 1.0);
  NAV_CHECK(settings.turnStep == 15.0); // the arrow keys' step (mitcad#1211)
  NAV_CHECK(sanitizeTurnStep(0.0) == 1.0 && sanitizeTurnStep(120.0) == 90.0 && sanitizeTurnStep(22.5) == 22.5 &&
            sanitizeTurnStep(std::numeric_limits<double>::quiet_NaN()) == 15.0);
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::NoModifier) == Action::Pan);
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::ShiftModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::LeftButton, Qt::NoModifier) == Action::None);
  settings.scheme = NavigationScheme::MiddleOrbit;
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::NoModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::ControlModifier) == Action::Pan);
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::ShiftModifier) == Action::Zoom);
  settings.scheme = NavigationScheme::AltButtons;
  NAV_CHECK(navigationAction(settings, Input::LeftButton, Qt::AltModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::AltModifier) == Action::Pan);
  NAV_CHECK(navigationAction(settings, Input::RightButton, Qt::AltModifier) == Action::Zoom);
  settings.scheme = NavigationScheme::RightOrbit;
  NAV_CHECK(navigationAction(settings, Input::RightButton, Qt::NoModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::RightButton, Qt::ShiftModifier) == Action::Pan);
  settings.scheme = NavigationScheme::MiddlePanF4;
  NAV_CHECK(navigationAction(settings, Input::LeftButton, Qt::NoModifier, true) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::LeftButton, Qt::ShiftModifier, true) == Action::Orbit);
  settings.scheme = NavigationScheme::Trackpad;
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::NoModifier) == Action::Pan);
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::AltModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::AltModifier | Qt::ShiftModifier) == Action::Zoom);
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::ControlModifier) == Action::Zoom);
  settings.scheme = NavigationScheme::TrackpadShift;
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::ShiftModifier) == Action::Orbit);
  NAV_CHECK(navigationAction(settings, Input::TrackpadScroll, Qt::AltModifier) == Action::Zoom);

  // The editable canonical table is unique and never exposes left selection
  // drags or the orientation cube's private synthetic flags as navigation.
  auto custom = navigationBindings(NavigationScheme::Trackpad);
  NAV_CHECK(custom.size() == 33);
  for (qsizetype i = 0; i < custom.size(); ++i) {
    for (qsizetype j = i + 1; j < custom.size(); ++j) {
      NAV_CHECK(custom[i].input != custom[j].input || custom[i].modifiers != custom[j].modifiers ||
                custom[i].f4 != custom[j].f4);
    }
    if (custom[i].input == Input::MiddleButton && custom[i].modifiers == Qt::NoModifier) {
      custom[i].action = Action::Zoom;
    }
  }
  settings.scheme = NavigationScheme::Custom;
  settings.customBindings = custom;
  NAV_CHECK(navigationAction(settings, Input::MiddleButton, Qt::NoModifier) == Action::Zoom);
  settings.customBindings.prepend({Input::LeftButton, Qt::NoModifier, false, Action::Orbit});
  NAV_CHECK(navigationAction(settings, Input::LeftButton, Qt::NoModifier) == Action::None);
  NAV_CHECK(!navigationBindingAllowed({Input::LeftButton, Qt::ShiftModifier, false, Action::Orbit}));
  NAV_CHECK(!navigationBindingAllowed({Input::LeftButton, Qt::ControlModifier, false, Action::Orbit}));
  NAV_CHECK(!navigationBindingAllowed({Input::LeftButton, Qt::MetaModifier, false, Action::Orbit}));
  NAV_CHECK(!navigationBindingAllowed({Input::MiddleButton, Qt::NoModifier, true, Action::Orbit}));
  NAV_CHECK(navigationBindingAllowed({Input::LeftButton, Qt::NoModifier, true, Action::Pan}));
  NAV_CHECK(sanitizeNavigationSpeed(-1.0) == 0.1);
  NAV_CHECK(sanitizeNavigationSpeed(100.0) == 4.0);
  NAV_CHECK(sanitizeNavigationSpeed(std::numeric_limits<double>::quiet_NaN()) == 1.0);
  NAV_CHECK(sanitizeNavigationSpeed(std::numeric_limits<double>::infinity()) == 1.0);

  QTemporaryDir directory;
  NAV_CHECK(directory.isValid());
  const auto path = directory.filePath(QStringLiteral("navigation.ini"));
  {
    QSettings store(path, QSettings::IniFormat);
    NavigationSettings fresh;
    loadNavigationDetails(store, fresh);
    NAV_CHECK(fresh.customBindings.isEmpty());
    fresh.scheme = NavigationScheme::MiddleOrbit;
    saveNavigationDetails(store, fresh);
    NAV_CHECK(!store.contains(QStringLiteral("view/navigationBindings")));
    NavigationSettings reopened;
    reopened.scheme = NavigationScheme::MiddleOrbit;
    loadNavigationDetails(store, reopened);
    NAV_CHECK(reopened.customBindings.isEmpty());
    // A damaged Custom id with no table still recovers usable bindings.
    reopened.scheme = NavigationScheme::Custom;
    loadNavigationDetails(store, reopened);
    NAV_CHECK(!reopened.customBindings.isEmpty());
    NAV_CHECK(navigationAction(reopened, Input::MiddleButton, Qt::NoModifier) == Action::Pan);
  }
  {
    QSettings store(path, QSettings::IniFormat);
    settings.orbitSpeed = 0.5;
    settings.panSpeed = 2.5;
    settings.zoomSpeed = 3.0;
    settings.trackpadSpeed = 0.25;
    settings.turnStep = 30.0;
    saveNavigationDetails(store, settings);
    store.sync();
    NAV_CHECK(store.status() == QSettings::NoError);
  }
  // A fresh settings object/reader exercises a restart rather than retaining
  // shared table state from the writer.
  {
    QSettings store(path, QSettings::IniFormat);
    NavigationSettings loaded;
    loaded.scheme = NavigationScheme::Custom;
    loadNavigationDetails(store, loaded);
    NAV_CHECK(navigationAction(loaded, Input::MiddleButton, Qt::NoModifier) == Action::Zoom);
    NAV_CHECK(navigationAction(loaded, Input::TrackpadScroll, Qt::AltModifier) == Action::Orbit);
    NAV_CHECK(navigationAction(loaded, Input::LeftButton, Qt::NoModifier) == Action::None);
    NAV_CHECK(loaded.orbitSpeed == 0.5 && loaded.panSpeed == 2.5 && loaded.zoomSpeed == 3.0 &&
              loaded.trackpadSpeed == 0.25);
    NAV_CHECK(loaded.turnStep == 30.0);
    store.setValue(QStringLiteral("view/orbitSpeed"), QStringLiteral("invalid"));
    store.setValue(QStringLiteral("view/panSpeed"), -100.0);
    store.setValue(QStringLiteral("view/turnStep"), QStringLiteral("invalid"));
    store.setValue(QStringLiteral("view/navigationBindings"), QByteArrayLiteral(
        "[{\"input\":0,\"modifiers\":0,\"f4\":false,\"action\":1},"
        "{\"input\":999,\"modifiers\":0,\"f4\":false,\"action\":1},"
        "{\"input\":1,\"modifiers\":0,\"f4\":false,\"action\":999}]"));
    loadNavigationDetails(store, loaded);
    NAV_CHECK(loaded.orbitSpeed == 1.0 && loaded.panSpeed == 0.1 && loaded.turnStep == 15.0);
    NAV_CHECK(navigationAction(loaded, Input::LeftButton, Qt::NoModifier) == Action::None);
    NAV_CHECK(navigationAction(loaded, Input::MiddleButton, Qt::NoModifier) == Action::Pan);
  }
#undef NAV_CHECK
}

} // namespace mitcad::tests
