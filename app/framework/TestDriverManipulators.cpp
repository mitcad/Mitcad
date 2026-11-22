// SPDX-License-Identifier: MIT
// Real overlay events exercise subpixel input without involving a model.
// The held Extrude tests separately cover the document worker and commit.
#include "TestDriver.hpp"

#include <cmath>
#include <limits>
#include <memory>

#include <QCoreApplication>
#include <QMouseEvent>
#include <QtLogging>

#include "../MainWindow.hpp"
#include "ManipulatorOverlay.hpp"

namespace mitcad {

void TestDriver::testManipulatorFeedback() {
  auto* window = qobject_cast<MainWindow*>(m_main.data());
  if (!window || window->modelBusy()) {
    fail(m_line, QStringLiteral("manipulator feedback checks require an idle document"));
    return;
  }
  constexpr double pi = 3.14159265358979323846;
  constexpr double degree = pi / 180.0;
  struct State {
    QPointer<OcctViewer> view;
    QPointer<ManipulatorOverlay> overlay;
    CameraState camera;
    ManipulatorOverlay::Handle handle;
    QPointF grab;
    int changes = 0;
    int releases = 0;
  };
  auto state = std::make_shared<State>();
  state->view = &window->viewer();
  state->view->settleCamera();
  state->camera = state->view->camera();
  const auto check = [this](bool condition, const QString& message) {
    if (!condition) { fail(m_line, message); }
  };
  const auto send = [state](QEvent::Type type, const QPointF& point) {
    const bool release = type == QEvent::MouseButtonRelease;
    const Qt::MouseButton button = type == QEvent::MouseMove ? Qt::NoButton : Qt::LeftButton;
    QMouseEvent event(type, point, QPointF(state->view->mapToGlobal(point.toPoint())), button,
                      release ? Qt::NoButton : Qt::LeftButton, Qt::ShiftModifier);
    QCoreApplication::sendEvent(state->view, &event);
  };
  queue([state] {
    state->overlay = new ManipulatorOverlay(*state->view, QStringLiteral("Feedback test"));
    connect(state->overlay, &ManipulatorOverlay::dragged, state->overlay, [state](const QString&, double value) {
      ++state->changes;
      state->handle.value = value;
      // A command's form refresh must retain the continuous feedback and
      // must not cause a second semantic action inside the same snap band.
      state->overlay->setHandles({state->handle});
    });
    connect(state->overlay, &ManipulatorOverlay::dragFinished, state->overlay,
            [state](const QString&) { ++state->releases; });
  });
  for (bool ring : {false, true}) {
    for (double height : {100.0, 250.0}) {
      queue([state, ring, height] {
        CameraState camera;
        camera.eye = gp_Pnt(0, -500, 0);
        camera.target = gp_Pnt(0, 0, 0);
        camera.up = gp_Dir(0, 0, 1);
        camera.height = height;
        state->view->setCamera(camera);
        state->view->settleCamera();
        state->changes = state->releases = 0;
        state->handle = {QStringLiteral("value"), QStringLiteral("Value"), {}, ring ? 10.0 * degree : 20.0};
        Manipulator& manipulator = state->handle.manipulator;
        manipulator.origin = gp_Pnt(13, 7, -10); // a placed handle, away from the origin
        if (ring) {
          manipulator.kind = Manipulator::Kind::Ring;
          manipulator.direction = state->view->viewDirection();
          manipulator.reference = state->view->viewUp();
        } else {
          manipulator.base = 4;
          manipulator.factor = height == 100.0 ? 1.0 : -0.5;
        }
        state->overlay->setHandles({state->handle});
      });
      const auto atValue = [state](double value) {
        auto handle = state->handle;
        handle.value = value;
        return state->overlay->draw(handle).handle + state->grab;
      };
      queue([state, ring, send] {
        const auto drawn = state->overlay->draw(state->handle);
        // Grab inside the arrow, rather than at its base. Rings preserve
        // the angular offset of the initial press through dragStart.
        state->grab = ring ? QPointF() : (drawn.tip - drawn.handle) * 0.37;
        send(QEvent::MouseButtonPress, drawn.handle + state->grab);
      });
      queue([state, ring, atValue, send] { send(QEvent::MouseMove, atValue(ring ? 30.0 * degree : 40.0)); });
      queue([state, ring, check] {
        check(state->overlay->isDragging(), QStringLiteral("placed handle failed to start dragging"));
        check(state->changes == 1, QStringLiteral("initial drag did not produce exactly one snapped change"));
        check(std::abs(state->handle.value - (ring ? 30.0 * degree : 40.0)) < 1e-9,
              QStringLiteral("length/degree snapping changed"));
      });
      for (int step = 1; step <= 8; ++step) {
        queue([state, ring, step, atValue, send, check] {
          QPointF point;
          if (ring) {
            point = atValue((30.0 + step * 0.025) * degree);
          } else {
            const QPointF a = atValue(40);
            const QPointF delta = atValue(41) - a;
            const double length = std::hypot(delta.x(), delta.y());
            point = a + delta / length * (step * 0.025);
          }
          send(QEvent::MouseMove, point);
          check(state->changes == 1, QStringLiteral("sub-snap motion repeated semantic evaluation"));
          check(state->overlay->m_visualValue.has_value(), QStringLiteral("continuous feedback disappeared"));
        });
      }
      queue([this, state, check, ring, height] {
        check(state->overlay->m_visualUpdates >= 9, QStringLiteral("subpixel moves did not update continuous feedback"));
        check(state->overlay->m_dragPaints > 0, QStringLiteral("the overlay did not paint during the held drag"));
        const double before = *state->overlay->m_visualValue;
        state->overlay->setHandles({state->handle});
        check(state->overlay->m_visualValue && *state->overlay->m_visualValue == before,
              QStringLiteral("form refresh snapped continuous feedback back to the model value"));
        const QString output = qEnvironmentVariable("MITCAD_TEST_WORK");
        if (height == 100.0 && !output.isEmpty()) {
          check(saveScreenshot(output + (ring ? QStringLiteral("/feedback-angle.png")
                                              : QStringLiteral("/feedback-length.png")), false),
                QStringLiteral("could not capture the held feedback"));
        }
      });
      if (ring) {
        // Two turns cross atan2's wrap in both zooms, preserving the
        // unwrapped semantic angle rather than jumping back to one turn.
        for (int angle = 120; angle <= 750; angle += 90) {
          queue([angle, atValue, send] { send(QEvent::MouseMove, atValue(angle * degree)); });
        }
      }
      queue([state, ring, height, atValue, send, check] {
        const double expected = ring ? 750.0 * degree : 40.0;
        send(QEvent::MouseButtonRelease, atValue(expected));
        check(!state->overlay->isDragging() && !state->overlay->m_visualValue,
              QStringLiteral("release retained provisional feedback"));
        check(state->releases == 1 && std::abs(state->handle.value - expected) < 1e-9,
              QStringLiteral("release changed the snapped final value"));
        const int changes = state->changes;
        state->handle.value = 12.34567; // typed values are not quantized by presentation
        state->overlay->setHandles({state->handle});
        check(state->handle.value == 12.34567 && state->changes == changes,
              QStringLiteral("presentation modified precise typed input"));
        qInfo() << "Manipulator feedback checked:" << (ring ? "angle" : "length") << "view_height=" << height;
      });
    }
  }
  for (bool ring : {false, true}) {
    queue([state, ring, send] {
      state->changes = state->releases = 0;
      state->handle.manipulator.kind = ring ? Manipulator::Kind::Ring : Manipulator::Kind::Arrow;
      state->handle.manipulator.direction = ring ? state->view->viewUp() : state->view->viewDirection();
      state->handle.manipulator.reference = gp_Dir(1, 0, 0);
      state->overlay->setHandles({state->handle});
      const QPointF point = state->overlay->draw(state->handle).handle;
      send(QEvent::MouseButtonPress, point);
      send(QEvent::MouseMove, point + QPointF(0, 20));
    });
    queue([state, check] {
      check(state->changes == 0, QStringLiteral("edge-on handle changed its value"));
      QEvent blocked(QEvent::WindowBlocked);
      QCoreApplication::sendEvent(state->view, &blocked);
      check(!state->overlay->isDragging() && !state->overlay->m_visualValue,
            QStringLiteral("blocked drag retained continuous feedback"));
    });
  }
  queue([state, send, check] {
    state->handle.manipulator.kind = Manipulator::Kind::Arrow;
    state->handle.manipulator.direction = state->view->viewUp();
    state->overlay->setHandles({state->handle});
    const auto drawn = state->overlay->draw(state->handle);
    const QPointF point = (drawn.handle + drawn.tip) / 2;
    send(QEvent::MouseButtonPress, point);
    send(QEvent::MouseMove, point + QPointF(0, -10));
    check(state->overlay->isDragging(), QStringLiteral("placement-change check did not start a drag"));
    state->handle.manipulator.origin.Translate(gp_Vec(10, 0, 0));
    state->overlay->setHandles({state->handle});
    check(!state->overlay->isDragging() && !state->overlay->m_visualValue,
          QStringLiteral("changed placement retained an obsolete grab"));
    send(QEvent::MouseButtonRelease, point + QPointF(0, -10));
  });
  queue([state, send] {
    // An arrow's minimum (mitcad#693): dragged as far past its origin,
    // the value stops there.
    state->changes = state->releases = 0;
    state->handle = {QStringLiteral("value"), QStringLiteral("Value"), {}, 5.0};
    state->handle.manipulator.kind = Manipulator::Kind::Arrow;
    state->handle.manipulator.direction = state->view->viewUp();
    state->handle.manipulator.minimum = 0.0;
    state->overlay->setHandles({state->handle});
    const auto drawn = state->overlay->draw(state->handle);
    send(QEvent::MouseButtonPress, drawn.handle);
    send(QEvent::MouseMove, drawn.origin * 2 - drawn.handle);
  });
  queue([state, send, check] {
    check(state->overlay->isDragging() && state->changes == 1 && state->handle.value == 0.0,
          QStringLiteral("a drag went past the arrow's minimum"));
    check(state->overlay->m_visualValue && *state->overlay->m_visualValue == 0.0,
          QStringLiteral("the feedback went past the arrow's minimum"));
    send(QEvent::MouseButtonRelease, state->overlay->draw(state->handle).handle);
  });
  queue([state, send, check] {
    state->handle.manipulator.kind = Manipulator::Kind::Arrow;
    state->handle.manipulator.direction = state->view->viewUp();
    state->handle.manipulator.minimum = -std::numeric_limits<double>::infinity();
    state->overlay->setHandles({state->handle});
    const QPointF point = state->overlay->draw(state->handle).handle + QPointF(0, -10);
    send(QEvent::MouseButtonPress, point);
    state->overlay->setHandles({}); // removal between press and first move
    send(QEvent::MouseMove, point + QPointF(0, -20));
    send(QEvent::MouseButtonRelease, point + QPointF(0, -20));
    check(state->overlay->m_pending < 0 && !state->overlay->isDragging(),
          QStringLiteral("a removed pending handle retained its old index"));
    delete state->overlay;
    state->view->setCamera(state->camera);
    state->view->settleCamera();
    qInfo("Manipulator feedback checks passed: length/angle, two zooms, placement, grab, typed precision, degenerate, minimum, cancel");
  });
}

// This file's steps (mitcad#2173): their specs and their chain.
namespace {

const TestDriver::StepSpec kStepSpecs[] = {
    {"test-manipulator-feedback", 0, 0, TestDriver::kInputOfMain},
};

} // namespace

const TestDriver::StepArea TestDriver::kManipulatorSteps{
    kStepSpecs, std::size(kStepSpecs), &TestDriver::runManipulatorStep};

void TestDriver::runManipulatorStep(const QString& name, const QStringList&, int) {
  if (name == QLatin1String("test-manipulator-feedback")) {
    testManipulatorFeedback();
  }
}

} // namespace mitcad
