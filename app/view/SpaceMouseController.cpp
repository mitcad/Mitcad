// SPDX-License-Identifier: MIT
#include "SpaceMouseController.hpp"

#include <algorithm>
#include <utility>

#include <QDebug>
#include <QGuiApplication>

namespace mitcad {

SpaceMouseController::SpaceMouseController(Hooks hooks, Factory factory, QObject* parent)
    : QObject(parent), m_hooks(std::move(hooks)), m_factory(std::move(factory)) {
  // A device's motion stops when the application goes to the background:
  // its release may go to another application.
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state != Qt::ApplicationActive && m_fromDevice) {
      motion(SpaceMouseAxes{}, true);
    }
  });
}

SpaceMouseController::~SpaceMouseController() = default;

QString SpaceMouseController::backendName() const {
  return m_backend ? m_backend->name() : QString();
}

void SpaceMouseController::setStatus(const QString& text) {
  spaceMouseStatus().text = text;
}

void SpaceMouseController::refresh() {
  const SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  if (!settings.enabled) {
    if (m_backend || !m_loggedOff) {
      m_backend.reset();
      m_loggedOff = true;
      m_axes = {};
      m_memory = SpaceMouseMemory();
      qInfo().noquote() << QStringLiteral("3D mouse: off");
      setStatus(tr("Off"));
    }
    apply();
    reportRotationLock();
    return;
  }
  m_loggedOff = false;
  if (!m_backend && m_factory) {
    m_backend = m_factory();
    if (m_backend) {
      m_backend->acceptsInput = m_hooks.acceptsInput;
      m_backend->nativeWindow = m_hooks.nativeWindow;
      m_backend->motion = [this](const SpaceMouseAxes& axes) { motion(axes, true); };
      m_backend->button = [this](int index, bool pressed) { button(index, pressed); };
      m_backend->connection = [this](bool present, const QString& device) {
        if (present) {
          connected(device);
        } else {
          qInfo().noquote() << QStringLiteral("3D mouse: %1 disconnected").arg(m_backend->name());
          setStatus(tr("No 3D mouse found"));
          motion(SpaceMouseAxes{}, true);
          m_memory = SpaceMouseMemory();
        }
      };
      QString device;
      if (m_backend->start(device)) {
        connected(device);
      } else {
        qInfo().noquote() << QStringLiteral("3D mouse: none");
        const auto status = m_backend->unavailableStatus();
        setStatus(status.isEmpty() ? tr("No 3D mouse found") : status);
      }
    } else {
      qInfo().noquote() << QStringLiteral("3D mouse: none");
      setStatus(tr("No 3D mouse found"));
    }
  }
  apply();
  reportRotationLock();
}

void SpaceMouseController::connected(const QString& device) {
  qInfo().noquote() << QStringLiteral("3D mouse: %1 connected (%2)").arg(backendName(), device);
  setStatus(tr("Connected: %1").arg(device));
  // Another device may reach further or less far (mitcad#1720).
  m_memory = SpaceMouseMemory();
  // The device's rotation lock key runs Lock Rotation by default when its
  // number is known (mitcad#1910); the settings keep it from then on.
  SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  if (addSpaceMouseDeviceButtons(settings, device)) {
    qInfo().noquote() << QStringLiteral("3D mouse: button %1 runs %2 by default")
                             .arg(int(settings.buttons.size()))
                             .arg(spaceMouseLockRotationCommand());
    if (m_hooks.change) {
      m_hooks.change(settings);
    }
  }
}

bool SpaceMouseController::inSketch() {
  const bool flat = m_hooks.flat && m_hooks.flat();
  if (!flat) {
    m_sketchFree = false;
  }
  return flat;
}

bool SpaceMouseController::rotationLocked() const {
  const SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  const bool flat = m_hooks.flat && m_hooks.flat();
  return spaceMouseRotationLocked(settings, flat, m_sketchFree);
}

bool SpaceMouseController::toggleRotationLock() {
  const bool flat = inSketch();
  bool locked = true;
  if (flat) {
    // In a sketch: for this sketch only; the settings stay.
    m_sketchFree = !m_sketchFree;
    locked = !m_sketchFree;
  } else {
    SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
    settings.lockRotation = !settings.lockRotation;
    locked = settings.lockRotation;
    if (m_hooks.change) {
      m_hooks.change(settings);
    }
  }
  qDebug().noquote() << QStringLiteral("3D mouse: rotation %1%2")
                            .arg(locked ? QStringLiteral("locked") : QStringLiteral("unlocked"),
                                 flat ? QStringLiteral(" in the sketch") : QString());
  apply(); // a cap held now follows at once
  reportRotationLock();
  return locked;
}

void SpaceMouseController::sketchChanged() {
  m_sketchFree = false;
  apply();
  reportRotationLock();
}

void SpaceMouseController::reportRotationLock() {
  if (m_hooks.rotationLock) {
    m_hooks.rotationLock(rotationLocked());
  }
}

void SpaceMouseController::motion(const SpaceMouseAxes& axes, bool device) {
  m_axes = axes;
  m_fromDevice = device;
  if (m_hooks.deflected) {
    m_hooks.deflected(spaceMouseDeflected(axes));
  }
  apply();
}

void SpaceMouseController::apply() {
  if (!m_hooks.rates) {
    return;
  }
  // A sketch's rotation stays locked until Lock Rotation frees it for that
  // sketch (mitcad#1910); then the cap moves the view as outside a sketch.
  SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  const bool flat = inSketch();
  settings.lockRotation = spaceMouseRotationLocked(settings, flat, m_sketchFree);
  // The memory learns the device's reach and holds Dominant axis only's
  // axis until the cap comes to rest (mitcad#1720).
  const SpaceMouseRates rates = spaceMouseRates(settings, m_axes, flat && !m_sketchFree, &m_memory);
  // The motion's axes in the debug log (mitcad#1720), written as the cap
  // comes to rest, before the view's own line of the motion.
  if (rates.moving()) {
    m_axesLog.add(m_axes);
  } else {
    const QString line = m_axesLog.take();
    if (!line.isEmpty()) {
      qDebug().noquote() << line;
    }
  }
  m_hooks.rates(rates);
}

void SpaceMouseController::button(int index, bool pressed) {
  if (index < 0 || index >= 32) {
    return;
  }
  SpaceMouseStatus& status = spaceMouseStatus();
  status.buttonsSeen = std::max(status.buttonsSeen, index + 1);
  if (!pressed) {
    return;
  }
  const SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  if (!settings.enabled) {
    return;
  }
  const QString id = settings.buttons.value(index);
  if (id.isEmpty()) {
    qDebug().noquote() << QStringLiteral("3D mouse: button %1 has no command").arg(index + 1);
    return;
  }
  qDebug().noquote() << QStringLiteral("3D mouse: button %1 runs %2").arg(index + 1).arg(id);
  if (m_hooks.runCommand) {
    m_hooks.runCommand(id);
  }
}

} // namespace mitcad
