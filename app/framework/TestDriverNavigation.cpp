// SPDX-License-Identifier: MIT
#include "TestDriver.hpp"

#include "../MainWindow.hpp"
#include "../view/NavigationPreferences.hpp"
#include "GlassCard.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <memory>
#include <utility>

#include <QApplication>
#include <QCheckBox>
#include <QComboBox>
#include <QMouseEvent>
#include <QKeyEvent>
#include <QPointer>
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

// These are application-event regressions, not physical-trackpad acceptance.
void TestDriver::testNavigationRelease() {
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (!window || window->modelBusy()) {
    fail(m_line, QStringLiteral("navigation release requires an idle document"));
    return;
  }
  struct State {
    QPointer<OcctViewer> view;
    NavigationSettings original;
    CameraState camera;
    CameraState before;
    QPointF at;
  };
  auto state = std::make_shared<State>();
  state->view = &window->viewer();
  state->view->settleCamera();
  state->original = state->view->navigation();
  state->camera = state->view->camera();
  const auto mouse = [state](QEvent::Type type, QPointF offset, Qt::MouseButtons buttons,
                             Qt::KeyboardModifiers modifiers) {
    const QPointF point = state->at + offset;
    QMouseEvent event(type, point, QPointF(state->view->mapToGlobal(point.toPoint())),
                      type == QEvent::MouseMove ? Qt::NoButton : Qt::LeftButton, buttons, modifiers);
    QCoreApplication::sendEvent(state->view, &event);
  };
  const auto key = [state](QEvent::Type type, int code, Qt::KeyboardModifiers modifiers) {
    QKeyEvent event(type, code, modifiers);
    QCoreApplication::sendEvent(state->view, &event);
  };
  const auto sameCamera = [](const CameraState& a, const CameraState& b) {
    return a.eye.Distance(b.eye) < 1e-6 && a.target.Distance(b.target) < 1e-6 &&
           a.up.Angle(b.up) < 1e-9 && std::abs(a.height - b.height) < 1e-6;
  };
  for (const auto scheme : {NavigationScheme::Trackpad, NavigationScheme::TrackpadShift,
                            NavigationScheme::Custom}) {
    for (bool keyFirst : {false, true}) {
      queue([state, scheme] {
        auto settings = state->original;
        settings.scheme = scheme;
        if (scheme == NavigationScheme::Custom) settings.customBindings = navigationBindings(NavigationScheme::Trackpad);
        settings.animate = false;
        state->view->setNavigation(settings);
        state->view->setCamera(state->camera);
        state->view->settleCamera();
        state->at = QPointF(state->view->width() * 0.48, state->view->height() * 0.50);
        state->before = state->view->camera();
      });
      queue([mouse] { mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::AltModifier); });
      queue([mouse] { mouse(QEvent::MouseMove, {24, 12}, Qt::LeftButton, Qt::AltModifier); });
      queue([key, mouse, keyFirst] {
        if (keyFirst) key(QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier);
        mouse(QEvent::MouseButtonRelease, {24, 12}, Qt::NoButton, keyFirst ? Qt::NoModifier : Qt::AltModifier);
        if (!keyFirst) key(QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier);
      });
      queue([] {});
      queue([this, state, sameCamera] {
        state->view->settleCamera();
        if (sameCamera(state->before, state->view->camera())) {
          fail(m_line, QStringLiteral("intentional Alt orbit did not move the camera"));
          return;
        }
        state->before = state->view->camera();
      });
      queue([key, mouse, keyFirst] {
        key(QEvent::KeyPress, Qt::Key_Control, Qt::ControlModifier); // Cmd in Qt on macOS
        mouse(QEvent::MouseButtonPress, {24, 12}, Qt::LeftButton, Qt::ControlModifier);
        if (keyFirst) key(QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier);
        mouse(QEvent::MouseButtonRelease, {24, 12}, Qt::NoButton,
              keyFirst ? Qt::NoModifier : Qt::ControlModifier);
        if (!keyFirst) key(QEvent::KeyRelease, Qt::Key_Control, Qt::NoModifier);
      });
      queue([mouse] { mouse(QEvent::MouseMove, {60, 30}, Qt::NoButton, Qt::NoModifier); });
      queue([] {});
      queue([this, state, sameCamera] {
        state->view->settleCamera();
        if (state->view->PressedMouseButtons() != Aspect_VKeyMouse_NONE ||
            !sameCamera(state->before, state->view->camera())) {
          fail(m_line, QStringLiteral("Cmd click or button-free hover continued an earlier orbit"));
        }
      });
      // A phase-only end carries no displacement. It must still release
      // the synthetic left button before any subsequent selection click.
      queue([this, state, scheme] {
        const auto scroll = [state, scheme](Qt::ScrollPhase phase, QPoint delta) {
          QWheelEvent event(state->at, QPointF(state->view->mapToGlobal(state->at.toPoint())),
                            delta, {}, Qt::NoButton, scheme == NavigationScheme::TrackpadShift ? Qt::ShiftModifier : Qt::AltModifier,
                            phase, false);
          QCoreApplication::sendEvent(state->view, &event);
        };
        scroll(Qt::ScrollBegin, {});
        scroll(Qt::ScrollUpdate, {18, 9});
        if (state->view->PressedMouseButtons() != Aspect_VKeyMouse_LeftButton) {
          fail(m_line, QStringLiteral("pixel-scroll regression did not start a synthetic drag"));
          return;
        }
        scroll(Qt::ScrollEnd, {});
        if (state->view->PressedMouseButtons() != Aspect_VKeyMouse_NONE) {
          fail(m_line, QStringLiteral("zero-delta ScrollEnd left the synthetic trackpad button held"));
        }
      });
    }
    queue([this, state, scheme] {
      auto settings = state->original;
      settings.scheme = scheme;
      if (scheme == NavigationScheme::Custom) settings.customBindings = navigationBindings(NavigationScheme::Trackpad);
      settings.animate = false;
      state->view->setNavigation(settings);
      state->view->setCamera(state->camera);
      state->view->settleCamera();
      const auto scroll = [state](Qt::ScrollPhase phase, QPoint delta, Qt::KeyboardModifiers modifiers) {
        QWheelEvent event(state->at, QPointF(state->view->mapToGlobal(state->at.toPoint())),
                          delta, {}, Qt::NoButton, modifiers, phase, false);
        QCoreApplication::sendEvent(state->view, &event);
      };
      const auto orbitKey = scheme == NavigationScheme::TrackpadShift ? Qt::ShiftModifier : Qt::AltModifier;
      scroll(Qt::ScrollBegin, {}, orbitKey);
      scroll(Qt::ScrollUpdate, {18, 9}, orbitKey);
      // Render the real viewport synchronously: QOpenGLWidget::repaint()
      // can defer its paint, leaving the camera unchanged at this check.
      // Keep one event turn so the 150 ms quiet fallback cannot interrupt
      // this gesture when a software-rendered frame takes longer.
      state->view->grabFramebuffer();
      for (const auto keys : {Qt::NoModifier, Qt::ControlModifier}) {
        const CameraState before = state->view->camera();
        scroll(Qt::ScrollMomentum, {18, 9}, keys);
        state->view->grabFramebuffer();
        const CameraState after = state->view->camera();
        const double angle = gp_Vec(before.eye, before.target).Angle(gp_Vec(after.eye, after.target));
        if (angle < 1e-8 || std::abs(before.height - after.height) > 1e-6 ||
            state->view->PressedMouseButtons() != Aspect_VKeyMouse_LeftButton) {
          fail(m_line, QStringLiteral("trackpad momentum changed the original orbit after its modifier changed: angle %1, height delta %2, pressed buttons %3, scheme %4, keys %5")
                           .arg(angle, 0, 'g', 16).arg(after.height - before.height, 0, 'g', 16)
                           .arg(static_cast<int>(state->view->PressedMouseButtons()))
                           .arg(static_cast<int>(scheme)).arg(static_cast<int>(keys)));
          return;
        }
      }
      scroll(Qt::ScrollEnd, {}, Qt::NoModifier);
      if (state->view->PressedMouseButtons() != Aspect_VKeyMouse_NONE) {
        fail(m_line, QStringLiteral("phase-only end left the momentum orbit button held"));
      }
    });
  }
  queue([state] {
    state->view->setNavigation(state->original);
    state->view->setCamera(state->camera);
    state->view->settleCamera();
    qInfo() << "Navigation release: both modifier/button orders, Cmd after orbit, hover and phase-only scroll end passed";
    qInfo() << "Navigation momentum: orbit retained with no modifier and Cmd, phase-only end released the button";
  });
}

// view-navigation <scheme>: the view's navigation in this test process only
// (Preferences keep theirs), without camera animations.
void TestDriver::setViewNavigation(const QString& scheme) {
  static const std::array<std::pair<const char*, NavigationScheme>, 7> schemes = {{
      {"middle-pan", NavigationScheme::MiddlePan},
      {"alt-buttons", NavigationScheme::AltButtons},
      {"middle-pan-f4", NavigationScheme::MiddlePanF4},
      {"middle-orbit", NavigationScheme::MiddleOrbit},
      {"right-orbit", NavigationScheme::RightOrbit},
      {"trackpad", NavigationScheme::Trackpad},
      {"trackpad-shift", NavigationScheme::TrackpadShift},
  }};
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  const auto found = std::find_if(schemes.cbegin(), schemes.cend(), [&scheme](const auto& entry) {
    return scheme == QLatin1String(entry.first);
  });
  if (!window || found == schemes.cend()) {
    fail(m_line, QStringLiteral("view-navigation middle-pan|alt-buttons|middle-pan-f4|middle-orbit|right-orbit|"
                                "trackpad|trackpad-shift requires a document window"));
    return;
  }
  const QPointer<MainWindow> target = window;
  const NavigationScheme value = found->second;
  queue([target, value, scheme] {
    if (!target) return;
    NavigationSettings settings = target->viewer().navigation();
    settings.scheme = value;
    settings.animate = false;
    target->viewer().setNavigation(settings);
    qInfo().noquote() << QStringLiteral("View navigation: %1").arg(scheme);
  });
}

// Keys changed while a button is held (mitcad#1571), a trackpad scroll
// while a button is held and the scroll phases Qt makes of macOS's
// (mitcad#1572), and the window deactivated during a drag (mitcad#1573).
// Application events only, as testNavigationRelease: physical input is the
// checklist's (docs/macos-trackpad-checklist.md).
void TestDriver::testNavigationGestureState() {
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (!window || window->modelBusy()) {
    fail(m_line, QStringLiteral("navigation gesture state requires an idle document"));
    return;
  }
  struct State {
    QPointer<OcctViewer> view;
    QPointer<QWidget> host; // the view's window, which the panels' windows belong to
    NavigationSettings original;
    CameraState camera;
    CameraState before;
    QPointF at;
    int clicks = 0;      // picks the view reported as clicks
    int windowPicks = 0; // and as window selections
    QMetaObject::Connection picked;
  };
  auto state = std::make_shared<State>();
  state->view = &window->viewer();
  state->host = GlassCard::hostWindow(state->view);
  state->view->settleCamera();
  state->original = state->view->navigation();
  state->camera = state->view->camera();
  const std::weak_ptr<State> weak = state;
  state->picked = QObject::connect(state->view.data(), &OcctViewer::picked, state->view.data(),
                                   [weak](const Selection&, Qt::KeyboardModifiers, bool windowPick) {
    if (const auto alive = weak.lock()) {
      ++(windowPick ? alive->windowPicks : alive->clicks);
    }
  });
  const auto use = [state](NavigationScheme scheme) {
    auto settings = state->original;
    settings.scheme = scheme;
    settings.animate = false;
    state->view->setNavigation(settings);
    state->view->setCamera(state->camera);
    state->view->settleCamera();
    state->at = QPointF(state->view->width() * 0.48, state->view->height() * 0.50);
    state->before = state->view->camera();
    state->clicks = 0;
    state->windowPicks = 0;
  };
  const auto mouse = [state](QEvent::Type type, QPointF offset, Qt::MouseButtons buttons,
                             Qt::KeyboardModifiers modifiers) {
    const QPointF point = state->at + offset;
    QMouseEvent event(type, point, QPointF(state->view->mapToGlobal(point.toPoint())),
                      type == QEvent::MouseMove ? Qt::NoButton : Qt::LeftButton, buttons, modifiers);
    QCoreApplication::sendEvent(state->view, &event);
  };
  const auto key = [state](QEvent::Type type, int code, Qt::KeyboardModifiers modifiers) {
    QKeyEvent event(type, code, modifiers);
    QCoreApplication::sendEvent(state->view, &event);
  };
  const auto scroll = [state](Qt::ScrollPhase phase, QPoint delta, Qt::KeyboardModifiers modifiers,
                              Qt::MouseButtons buttons) {
    QWheelEvent event(state->at, QPointF(state->view->mapToGlobal(state->at.toPoint())), delta, {}, buttons,
                      modifiers, phase, false);
    QCoreApplication::sendEvent(state->view, &event);
  };
  // A frame now, in this turn of the event loop: OCCT applies the input
  // (camera moves, picks) without the scroll's 150 ms quiet time passing.
  const auto frame = [state] { state->view->grabFramebuffer(); };
  const auto sameCamera = [](const CameraState& a, const CameraState& b) {
    return a.eye.Distance(b.eye) < 1e-6 && a.target.Distance(b.target) < 1e-6 &&
           a.up.Angle(b.up) < 1e-9 && std::abs(a.height - b.height) < 1e-6;
  };
  const auto held = [state](Aspect_VKeyMouse buttons) { return state->view->PressedMouseButtons() == buttons; };
  // Qt's own activation state, as keyReceiver sets it where the system
  // activates nothing (a session without a desktop).
  const auto activate = [](QWidget* active) {
    QT_WARNING_PUSH
    QT_WARNING_DISABLE_DEPRECATED
    QApplication::setActiveWindow(active);
    QT_WARNING_POP
  };
  const auto check = [this](bool ok, const QString& why) {
    if (!ok) fail(m_line, why);
    return ok;
  };

  // --- mitcad#1571: the keys of the press decide it until the button is up.
  for (const auto scheme : {NavigationScheme::Trackpad, NavigationScheme::TrackpadShift,
                            NavigationScheme::AltButtons}) {
    const int number = static_cast<int>(scheme);
    // Option pressed during a plain select: still a window selection.
    queue([use, mouse, scheme] {
      use(scheme);
      mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::NoModifier);
    });
    queue([state, key, mouse, frame, sameCamera, held, check, number] {
      key(QEvent::KeyPress, Qt::Key_Alt, Qt::AltModifier);
      mouse(QEvent::MouseMove, {24, 12}, Qt::LeftButton, Qt::AltModifier);
      mouse(QEvent::MouseMove, {48, 24}, Qt::LeftButton, Qt::AltModifier);
      frame();
      check(sameCamera(state->before, state->view->camera()) && held(Aspect_VKeyMouse_LeftButton),
            QStringLiteral("Option pressed during a select turned it into an orbit (scheme %1)").arg(number));
    });
    queue([state, key, mouse, frame, sameCamera, held, check, number] {
      mouse(QEvent::MouseButtonRelease, {48, 24}, Qt::NoButton, Qt::AltModifier);
      key(QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier);
      frame();
      check(state->windowPicks == 1 && held(Aspect_VKeyMouse_NONE) && sameCamera(state->before, state->view->camera()),
            QStringLiteral("a select with Option pressed during it did not end as a window selection: "
                           "%1 window picks, buttons %2 (scheme %3)")
                .arg(state->windowPicks).arg(static_cast<int>(state->view->PressedMouseButtons())).arg(number));
    });
    // Option released during an orbit: the orbit goes on, nothing picked.
    queue([use, mouse, scheme] {
      use(scheme);
      mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::AltModifier);
    });
    queue([state, mouse, frame, sameCamera, check, number] {
      mouse(QEvent::MouseMove, {24, 12}, Qt::LeftButton, Qt::AltModifier);
      frame();
      if (check(!sameCamera(state->before, state->view->camera()),
                QStringLiteral("Option+drag did not orbit (scheme %1)").arg(number))) {
        state->before = state->view->camera();
      }
    });
    queue([state, key, mouse, frame, sameCamera, check, number] {
      key(QEvent::KeyRelease, Qt::Key_Alt, Qt::NoModifier);
      mouse(QEvent::MouseMove, {48, 24}, Qt::LeftButton, Qt::NoModifier);
      frame();
      check(!sameCamera(state->before, state->view->camera()),
            QStringLiteral("the orbit stopped when Option was released before the button (scheme %1)").arg(number));
    });
    queue([state, mouse, frame, held, check, number] {
      mouse(QEvent::MouseButtonRelease, {48, 24}, Qt::NoButton, Qt::NoModifier);
      frame();
      check(state->windowPicks == 0 && state->clicks == 0 && held(Aspect_VKeyMouse_NONE),
            QStringLiteral("an orbit with Option released before the button picked: %1 window picks, %2 clicks "
                           "(scheme %3)").arg(state->windowPicks).arg(state->clicks).arg(number));
    });
  }

  // --- mitcad#1572: a scroll while a button is held, and the phases Qt
  // gives macOS's (qnsview_mouse.mm, scrollWheel:): MayBegin is a
  // zero-delta ScrollBegin, Cancelled a zero-delta ScrollEnd; Ended is a
  // ScrollEnd that keeps its delta when Qt sees no momentum queued, and
  // a momentum Began after it is a new ScrollBegin; the momentum keeps the
  // keys of the scroll's last event. Each in one turn: no quiet time.
  for (const auto scheme : {NavigationScheme::Trackpad, NavigationScheme::TrackpadShift}) {
    const int number = static_cast<int>(scheme);
    const Qt::KeyboardModifiers orbit =
        scheme == NavigationScheme::TrackpadShift ? Qt::ShiftModifier : Qt::AltModifier;
    queue([state, use, mouse, scroll, frame, sameCamera, held, check, scheme, orbit, number] {
      use(scheme);
      mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::NoModifier);
      scroll(Qt::ScrollBegin, {}, orbit, Qt::LeftButton);
      scroll(Qt::ScrollUpdate, {18, 9}, orbit, Qt::LeftButton);
      frame();
      if (!check(held(Aspect_VKeyMouse_LeftButton) && sameCamera(state->before, state->view->camera()),
                 QStringLiteral("an orbit scroll replaced the held button's press (scheme %1)").arg(number))) {
        return;
      }
      mouse(QEvent::MouseButtonRelease, {}, Qt::NoButton, Qt::NoModifier);
      frame();
      if (!check(state->clicks == 1 && state->windowPicks == 0 && held(Aspect_VKeyMouse_NONE),
                 QStringLiteral("the press held during an orbit scroll was not a click: %1 clicks, %2 window "
                                "picks (scheme %3)").arg(state->clicks).arg(state->windowPicks).arg(number))) {
        return;
      }
      scroll(Qt::ScrollUpdate, {18, 9}, orbit, Qt::NoButton);
      frame();
      check(held(Aspect_VKeyMouse_NONE) && sameCamera(state->before, state->view->camera()),
            QStringLiteral("the scroll went on orbiting after the click (scheme %1)").arg(number));
      scroll(Qt::ScrollEnd, {}, orbit, Qt::NoButton);
    });
    queue([state, use, scroll, frame, sameCamera, held, check, scheme, orbit, number] {
      use(scheme);
      // Ended with a delta: applied, and the button up at once.
      scroll(Qt::ScrollBegin, {}, orbit, Qt::NoButton);
      scroll(Qt::ScrollUpdate, {18, 9}, orbit, Qt::NoButton);
      frame();
      if (!check(held(Aspect_VKeyMouse_LeftButton) && !sameCamera(state->before, state->view->camera()),
                 QStringLiteral("an orbit scroll did not orbit (scheme %1)").arg(number))) {
        return;
      }
      scroll(Qt::ScrollUpdate, {18, 9}, orbit, Qt::NoButton);
      scroll(Qt::ScrollEnd, {12, 6}, orbit, Qt::NoButton);
      if (!check(held(Aspect_VKeyMouse_NONE),
                 QStringLiteral("a ScrollEnd with a delta left the trackpad button held (scheme %1)").arg(number))) {
        return;
      }
      frame();
      // Resting fingers: MayBegin, then Cancelled.
      state->before = state->view->camera();
      scroll(Qt::ScrollBegin, {}, orbit, Qt::NoButton);
      scroll(Qt::ScrollEnd, {}, orbit, Qt::NoButton);
      frame();
      if (!check(held(Aspect_VKeyMouse_NONE) && sameCamera(state->before, state->view->camera()),
                 QStringLiteral("resting fingers moved the camera or held the button (scheme %1)").arg(number))) {
        return;
      }
      // Ended with a delta, then the momentum Qt did not see coming.
      scroll(Qt::ScrollBegin, {}, orbit, Qt::NoButton);
      scroll(Qt::ScrollUpdate, {18, 9}, orbit, Qt::NoButton);
      scroll(Qt::ScrollEnd, {12, 6}, orbit, Qt::NoButton);
      if (!check(held(Aspect_VKeyMouse_NONE),
                 QStringLiteral("Ended with a delta left the button held (scheme %1)").arg(number))) {
        return;
      }
      scroll(Qt::ScrollBegin, {12, 6}, orbit, Qt::NoButton);
      scroll(Qt::ScrollMomentum, {9, 4}, orbit, Qt::NoButton);
      if (!check(held(Aspect_VKeyMouse_LeftButton),
                 QStringLiteral("a momentum begun as a new scroll did not orbit (scheme %1)").arg(number))) {
        return;
      }
      scroll(Qt::ScrollEnd, {}, orbit, Qt::NoButton);
      check(held(Aspect_VKeyMouse_NONE),
            QStringLiteral("the momentum's end left the trackpad button held (scheme %1)").arg(number));
    });
  }

  // --- mitcad#1573: the window deactivated during a drag ends it.
  // An F4 orbit (the cube's flags) and an Option orbit, then a click.
  for (const auto scheme : {NavigationScheme::MiddlePanF4, NavigationScheme::AltButtons}) {
    const bool f4 = scheme == NavigationScheme::MiddlePanF4;
    const Qt::KeyboardModifiers keys = f4 ? Qt::NoModifier : Qt::AltModifier;
    queue([state, use, key, mouse, activate, check, scheme, f4, keys] {
      use(scheme);
      activate(state->host);
      if (f4) key(QEvent::KeyPress, Qt::Key_F4, Qt::NoModifier);
      mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, keys);
      check(!f4 || state->view->cubeInteraction(), QStringLiteral("F4 and the left button did not start an F4 orbit"));
    });
    queue([state, mouse, frame, sameCamera, held, activate, check, f4, keys] {
      mouse(QEvent::MouseMove, {24, 12}, Qt::LeftButton, keys);
      frame();
      if (!check(!sameCamera(state->before, state->view->camera()) && held(Aspect_VKeyMouse_LeftButton),
                 QStringLiteral("the %1 orbit did not move the camera").arg(f4 ? QStringLiteral("F4") : QStringLiteral("Option")))) {
        return;
      }
      activate(nullptr); // Cmd-Tab: the release and F4's release go elsewhere
      check(held(Aspect_VKeyMouse_NONE) && !state->view->cubeInteraction(),
            QStringLiteral("the window deactivated during the %1 orbit left it held")
                .arg(f4 ? QStringLiteral("F4") : QStringLiteral("Option")));
      activate(state->host);
      state->before = state->view->camera();
    });
    queue([state, mouse, frame, sameCamera, held, check, f4] {
      state->clicks = 0;
      state->windowPicks = 0;
      mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::NoModifier);
      check(!state->view->cubeInteraction(), QStringLiteral("after the deactivation a click was an F4 orbit"));
      mouse(QEvent::MouseButtonRelease, {}, Qt::NoButton, Qt::NoModifier);
      frame();
      check(state->clicks == 1 && held(Aspect_VKeyMouse_NONE) && sameCamera(state->before, state->view->camera()),
            QStringLiteral("after the window was deactivated during the %1 orbit, a click did not pick: %2 clicks")
                .arg(f4 ? QStringLiteral("F4") : QStringLiteral("Option")).arg(state->clicks));
    });
  }
  // A trackpad scroll's drag, and the window deactivated.
  queue([use, scroll, held, activate, check, state] {
    use(NavigationScheme::Trackpad);
    activate(state->host);
    scroll(Qt::ScrollBegin, {}, Qt::AltModifier, Qt::NoButton);
    scroll(Qt::ScrollUpdate, {18, 9}, Qt::AltModifier, Qt::NoButton);
    if (!check(held(Aspect_VKeyMouse_LeftButton), QStringLiteral("an orbit scroll did not start its drag"))) return;
    activate(nullptr);
    check(held(Aspect_VKeyMouse_NONE), QStringLiteral("the window deactivated during a scroll's orbit left it held"));
    activate(state->host);
  });
  // The keyboard taken by a panel during a drag (a field of the view's own
  // window, or a floating panel's own window) does not end the drag.
  const auto panelTakes = std::make_shared<QString>();
  queue([use, mouse, frame, sameCamera, held, check, state] {
    use(NavigationScheme::AltButtons);
    mouse(QEvent::MouseButtonPress, {}, Qt::LeftButton, Qt::AltModifier);
    mouse(QEvent::MouseMove, {24, 12}, Qt::LeftButton, Qt::AltModifier);
    frame();
    if (check(!sameCamera(state->before, state->view->camera()) && held(Aspect_VKeyMouse_LeftButton),
              QStringLiteral("Option+drag did not orbit"))) {
      state->before = state->view->camera();
    }
  });
  queue([mouse, frame, sameCamera, held, activate, check, state, panelTakes] {
    QFocusEvent out(QEvent::FocusOut, Qt::TabFocusReason);
    QCoreApplication::sendEvent(state->view, &out);
    QWidget* card = nullptr;
    for (QWidget* top : QApplication::topLevelWidgets()) {
      if (top != state->host && top->isVisible() && qobject_cast<GlassCard*>(top) != nullptr &&
          GlassCard::hostWindow(top) == state->host) {
        card = top;
        break;
      }
    }
    if (card != nullptr) {
      activate(state->host);
      activate(card);
      *panelTakes = QStringLiteral("a floating panel's window took the activation");
    } else {
      *panelTakes = QStringLiteral("no floating panel window in this layout");
    }
    if (!check(held(Aspect_VKeyMouse_LeftButton),
               QStringLiteral("the keyboard taken by a panel ended the drag on the view (%1)").arg(*panelTakes))) {
      activate(state->host);
      return;
    }
    mouse(QEvent::MouseMove, {48, 24}, Qt::LeftButton, Qt::AltModifier);
    frame();
    activate(state->host);
    check(!sameCamera(state->before, state->view->camera()),
          QStringLiteral("the orbit stopped when a panel took the keyboard (%1)").arg(*panelTakes));
    mouse(QEvent::MouseButtonRelease, {48, 24}, Qt::NoButton, Qt::AltModifier);
  });
  queue([state, panelTakes] {
    QObject::disconnect(state->picked);
    state->view->setNavigation(state->original);
    state->view->setCamera(state->camera);
    state->view->settleCamera();
    qInfo() << "Navigation gesture state: keys changed during a press kept its gesture and its click";
    qInfo() << "Navigation gesture state: a scroll with a button held, ScrollEnd with a delta and Qt's macOS phases released the button";
    qInfo().noquote() << QStringLiteral("Navigation gesture state: a deactivated window ended F4, Option and scroll drags; %1 kept the drag")
                             .arg(*panelTakes);
  });
}

} // namespace mitcad
