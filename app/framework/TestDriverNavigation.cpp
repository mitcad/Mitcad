// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"

#include "../MainWindow.hpp"
#include "../view/NavigationPreferences.hpp"

#include <array>
#include <cmath>
#include <memory>

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QMouseEvent>
#include <QPushButton>
#include <QSlider>
#include <QWheelEvent>

namespace mitcad {

void TestDriver::testNavigationControls(const QString& mode) {
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  NavigationPreferences* controls = nullptr;
  for (QWidget* top : QApplication::topLevelWidgets()) {
    if (!top->isVisible()) continue;
    for (QWidget* widget : top->findChildren<QWidget*>()) {
      if (auto* candidate = dynamic_cast<NavigationPreferences*>(widget); candidate && candidate->isVisible()) {
        controls = candidate;
        break;
      }
    }
  }
  if (!window || !controls) {
    fail(m_line, QStringLiteral("navigation controls require the visible Navigation settings page"));
    return;
  }
  const auto check = [this](bool value, const QString& why) {
    if (!value) fail(m_line, why);
    return value;
  };
  auto* scheme = controls->findChild<QComboBox*>(QStringLiteral("navigationScheme"));
  auto* table = controls->findChild<QTableWidget*>(QStringLiteral("navigationBindings"));
  auto* guide = controls->findChild<QWidget*>(QStringLiteral("navigationVisualGuide"));
  if (!check(scheme && table && guide && !guide->accessibleDescription().isEmpty(),
             QStringLiteral("navigation table or active visual guide is missing"))) return;
  if (!check(table->rowCount() == navigationBindings(NavigationScheme::MiddlePan).size(),
             QStringLiteral("navigation binding table omits inputs"))) return;
  const std::array<const char*, 4> names{{"navigationOrbitSpeed", "navigationPanSpeed",
                                         "navigationZoomSpeed", "navigationTrackpadSpeed"}};
  std::array<QSlider*, 4> sliders{};
  for (std::size_t i = 0; i < sliders.size(); ++i) {
    sliders[i] = controls->findChild<QSlider*>(QString::fromLatin1(names[i]));
    if (!check(sliders[i] && sliders[i]->minimum() == 10 && sliders[i]->maximum() == 400,
               QStringLiteral("navigation speed control has no safe range"))) return;
  }
  // A real disabled editor prevents reserved left drags from becoming a
  // camera gesture through the preferences page.
  const auto canonical = navigationBindings(NavigationScheme::MiddlePan);
  int middle = -1;
  for (int row = 0; row < canonical.size(); ++row) {
    const auto& binding = canonical[row];
    auto* editor = qobject_cast<QComboBox*>(table->cellWidget(row, 1));
    if (!check(editor != nullptr, QStringLiteral("navigation row has no action editor"))) return;
    if (!navigationBindingAllowed(binding) &&
        !check(!editor->isEnabled(), QStringLiteral("reserved selection binding can be edited"))) return;
    if (binding.input == NavigationInput::MiddleButton && binding.modifiers == Qt::NoModifier) middle = row;
  }
  if (!check(middle >= 0, QStringLiteral("middle-button navigation row is missing"))) return;
  // The 3D Mouse tab's Navigation mode and Lock horizon (mitcad#978).
  auto* spaceMode = controls->findChild<QComboBox*>(QStringLiteral("navigationSpaceMouseMode"));
  auto* lockHorizon = controls->findChild<QCheckBox*>(QStringLiteral("navigationSpaceMouseLockHorizon"));
  if (!check(spaceMode && lockHorizon && spaceMode->count() == 3 &&
                 spaceMode->findData(int(SpaceMouseMode::Object)) >= 0 &&
                 spaceMode->findData(int(SpaceMouseMode::Camera)) >= 0 &&
                 spaceMode->findData(int(SpaceMouseMode::Fly)) >= 0,
             QStringLiteral("the 3D Mouse tab has no Navigation mode or Lock horizon"))) return;
  if (mode == QStringLiteral("configure")) {
    scheme->setCurrentIndex(scheme->findData(int(NavigationScheme::MiddlePan)));
    auto* editor = qobject_cast<QComboBox*>(table->cellWidget(middle, 1));
    editor->setCurrentIndex(editor->findData(int(NavigationAction::Orbit)));
    if (!check(scheme->currentData().toInt() == int(NavigationScheme::Custom),
               QStringLiteral("editing a preset did not select Custom"))) return;
    for (std::size_t i = 0; i < sliders.size(); ++i) sliders[i]->setValue(125 + int(i) * 25);
    spaceMode->setCurrentIndex(spaceMode->findData(int(SpaceMouseMode::Camera)));
    lockHorizon->setChecked(true);
  } else if (mode == QStringLiteral("reset")) {
    for (const char* name : names) {
      auto* reset = controls->findChild<QPushButton*>(QString::fromLatin1(name) + QStringLiteral("Reset"));
      if (!check(reset != nullptr, QStringLiteral("navigation speed has no reset button"))) return;
      reset->click();
    }
  } else if (mode != QStringLiteral("default") && mode != QStringLiteral("restored")) {
    fail(m_line, QStringLiteral("navigation controls mode must be default, configure, restored or reset"));
    return;
  }
  const auto chosen = controls->chosen();
  const auto& live = window->viewer().navigation();
  if (mode == QStringLiteral("default")) {
#ifdef Q_OS_MACOS
    constexpr auto expected = NavigationScheme::Trackpad;
#else
    constexpr auto expected = NavigationScheme::MiddlePan;
#endif
    if (!check(chosen.scheme == expected && live.scheme == expected,
               QStringLiteral("fresh settings changed the platform's default navigation preset"))) return;
  }
  if (mode == QStringLiteral("configure") || mode == QStringLiteral("restored")) {
    if (!check(chosen.scheme == NavigationScheme::Custom && live.scheme == NavigationScheme::Custom &&
                   navigationAction(chosen, NavigationInput::MiddleButton, Qt::NoModifier) == NavigationAction::Orbit &&
                   navigationAction(live, NavigationInput::MiddleButton, Qt::NoModifier) == NavigationAction::Orbit,
               QStringLiteral("custom navigation binding was not applied or restored"))) return;
  }
  // Fresh settings: Object and no Lock horizon; chosen ones apply at once
  // and survive a restart (a speed's reset leaves them).
  const bool chosenSpace = mode != QStringLiteral("default");
  const SpaceMouseMode expectedMode = chosenSpace ? SpaceMouseMode::Camera : SpaceMouseMode::Object;
  if (!check(spaceMode->currentData().toInt() == int(expectedMode) && lockHorizon->isChecked() == chosenSpace &&
                 chosen.spaceMouse.mode == expectedMode && live.spaceMouse.mode == expectedMode &&
                 chosen.spaceMouse.lockHorizon == chosenSpace && live.spaceMouse.lockHorizon == chosenSpace,
             QStringLiteral("the 3D mouse's Navigation mode or Lock horizon was not applied or restored"))) return;
  const std::array<double, 4> actual{{live.orbitSpeed, live.panSpeed, live.zoomSpeed, live.trackpadSpeed}};
  for (std::size_t i = 0; i < sliders.size(); ++i) {
    const int expected = mode == QStringLiteral("configure") || mode == QStringLiteral("restored")
                             ? 125 + int(i) * 25 : 100;
    if (!check(sliders[i]->value() == expected && std::abs(actual[i] - expected / 100.0) < 1e-9,
               QStringLiteral("navigation speed was not applied or restored: %1").arg(QString::fromLatin1(names[i])))) return;
  }
  qInfo().noquote() << QStringLiteral("Navigation controls: %1, reserved rows disabled, live speeds match, visual guide populated, "
                                      "3D mouse %2 mode, horizon %3")
                            .arg(mode, QLatin1String(spaceMouseModeId(live.spaceMouse.mode)),
                                 live.spaceMouse.lockHorizon ? QStringLiteral("locked") : QStringLiteral("free"));
}

void TestDriver::testNavigationRuntime() {
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (!window || window->modelBusy()) {
    fail(m_line, QStringLiteral("navigation runtime requires an idle document"));
    return;
  }
  struct State {
    QPointer<OcctViewer> view;
    NavigationSettings original;
    CameraState originalCamera;
    CameraState before;
    std::array<std::array<double, 2>, 6> values{};
  };
  auto state = std::make_shared<State>();
  state->view = &window->viewer();
  state->view->settleCamera();
  state->original = state->view->navigation();
  state->originalCamera = state->view->camera();
  const auto send = [state](QEvent::Type type, const QPointF& point, Qt::MouseButton button,
                             Qt::KeyboardModifiers modifiers) {
    if (!state->view) return;
    QMouseEvent event(type, point, QPointF(state->view->mapToGlobal(point.toPoint())),
                      type == QEvent::MouseMove ? Qt::NoButton : button,
                      type == QEvent::MouseButtonRelease ? Qt::NoButton : button, modifiers);
    QCoreApplication::sendEvent(state->view, &event);
  };
  for (int action = 0; action < 6; ++action) {
    for (int speed = 0; speed < 2; ++speed) {
      queue([state, action, speed] {
        NavigationSettings settings;
        settings.scheme = action == 1 || action == 4 ? NavigationScheme::MiddleOrbit :
                          action == 3 ? NavigationScheme::Trackpad : NavigationScheme::MiddlePan;
        if (action == 5) {
          settings.customBindings = navigationBindings(NavigationScheme::MiddlePan);
          for (auto& binding : settings.customBindings) {
            if (binding.input == NavigationInput::MiddleButton && binding.modifiers == Qt::NoModifier) {
              binding.action = NavigationAction::Orbit;
            }
          }
          settings.scheme = NavigationScheme::Custom;
        }
        settings.animate = false;
        settings.zoomToCursor = false;
        const double multiplier = speed == 0 ? 1.0 : 2.0;
        if (action == 0) settings.panSpeed = multiplier;
        if (action == 1 || action == 5) settings.orbitSpeed = multiplier;
        if (action == 2 || action == 4) settings.zoomSpeed = multiplier;
        if (action == 3) settings.trackpadSpeed = multiplier;
        state->view->setNavigation(settings);
        CameraState camera;
        camera.eye = gp_Pnt(200, -300, 200);
        camera.target = gp_Pnt(0, 0, 0);
        camera.up = gp_Dir(0, 0, 1);
        camera.height = 200;
        state->view->setCamera(camera);
        state->view->settleCamera();
        state->before = state->view->camera();
      });
      if (action <= 1 || action >= 4) {
        for (int event = 0; event <= 7; ++event) {
          queue([state, send, event, action] {
            const QPointF at(state->view->width() * 0.48 + std::min(event, 6) * 4,
                             state->view->height() * 0.50 + std::min(event, 6) * 2);
            send(event == 0 ? QEvent::MouseButtonPress : event == 7 ? QEvent::MouseButtonRelease : QEvent::MouseMove,
                 at, Qt::MiddleButton, action == 4 ? Qt::ShiftModifier : Qt::NoModifier);
          });
        }
      } else {
        for (int event = 0; event < (action == 3 ? 3 : 1); ++event) {
          queue([state, action, event] {
            const QPointF at(state->view->width() * 0.5, state->view->height() * 0.5);
            const QPoint pixel = action == 3 && event == 1 ? QPoint(24, 12) : QPoint();
            const QPoint angle = action == 2 ? QPoint(0, 120) : QPoint();
            QWheelEvent wheel(at, QPointF(state->view->mapToGlobal(at.toPoint())), pixel, angle,
                              Qt::NoButton, Qt::NoModifier,
                              action == 2 ? Qt::NoScrollPhase : event == 0 ? Qt::ScrollBegin :
                              event == 1 ? Qt::ScrollUpdate : Qt::ScrollEnd, false);
            QCoreApplication::sendEvent(state->view, &wheel);
          });
        }
      }
      queue([] {}); // Let OCCT consume the queued gesture in a normal paint.
      queue([this, state, action, speed] {
        state->view->settleCamera();
        const auto after = state->view->camera();
        double value = state->before.target.Distance(after.target);
        if (action == 1 || action == 5) value = gp_Vec(state->before.eye, state->before.target).Angle(gp_Vec(after.eye, after.target));
        if (action == 2 || action == 4) value = std::abs(std::log(after.height / state->before.height));
        state->values[action][speed] = value;
        if (!std::isfinite(value) || value < 1e-5) {
          fail(m_line, QStringLiteral("navigation gesture %1 at speed %2 did not move its actual camera").arg(action).arg(speed));
        }
      });
    }
  }
  queue([this, state] {
    state->view->setNavigation(state->original);
    state->view->setCamera(state->originalCamera);
    state->view->settleCamera();
    const std::array<const char*, 6> labels{{"Pan", "Orbit", "Zoom", "Trackpad", "Drag zoom", "Custom orbit"}};
    for (int action = 0; action < 6; ++action) {
      const double ratio = state->values[action][1] / state->values[action][0];
      if (!std::isfinite(ratio) || ratio < 1.5 || ratio > 2.5) {
        fail(m_line, QStringLiteral("%1 speed does not scale actual camera motion: ratio %2")
                         .arg(QString::fromLatin1(labels[action])).arg(ratio));
        return;
      }
    }
    qInfo() << "Navigation runtime: actual pan, orbit, wheel zoom and trackpad camera motion follows speed";
    qInfo() << "Navigation runtime: drag zoom and a custom binding change the actual camera";
  });
}

} // namespace mitcad
