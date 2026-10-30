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
      qInfo().noquote() << QStringLiteral("3D mouse: off");
      setStatus(tr("Off"));
    }
    apply();
    return;
  }
  m_loggedOff = false;
  if (!m_backend && m_factory) {
    m_backend = m_factory();
    if (m_backend) {
      m_backend->motion = [this](const SpaceMouseAxes& axes) { motion(axes, true); };
      m_backend->button = [this](int index, bool pressed) { button(index, pressed); };
      m_backend->connection = [this](bool connected, const QString& device) {
        if (connected) {
          qInfo().noquote() << QStringLiteral("3D mouse: %1 connected (%2)").arg(m_backend->name(), device);
          setStatus(tr("Connected: %1").arg(device));
        } else {
          qInfo().noquote() << QStringLiteral("3D mouse: %1 disconnected").arg(m_backend->name());
          setStatus(tr("No 3D mouse found"));
          motion(SpaceMouseAxes{}, true);
        }
      };
      QString device;
      if (m_backend->start(device)) {
        qInfo().noquote() << QStringLiteral("3D mouse: %1 connected (%2)").arg(m_backend->name(), device);
        setStatus(tr("Connected: %1").arg(device));
      } else {
        qInfo().noquote() << QStringLiteral("3D mouse: none");
        setStatus(tr("No 3D mouse found"));
      }
    } else {
      qInfo().noquote() << QStringLiteral("3D mouse: none");
      setStatus(tr("No 3D mouse found"));
    }
  }
  apply();
}

void SpaceMouseController::motion(const SpaceMouseAxes& axes, bool device) {
  m_axes = axes;
  m_fromDevice = device;
  apply();
}

void SpaceMouseController::apply() {
  if (!m_hooks.rates) {
    return;
  }
  const SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  const bool flat = m_hooks.flat && m_hooks.flat();
  m_hooks.rates(spaceMouseRates(settings, m_axes, flat));
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
