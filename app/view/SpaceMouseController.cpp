// SPDX-License-Identifier: MIT
#include "SpaceMouseController.hpp"

#include <algorithm>
#include <utility>

#include <QDebug>
#include <QGuiApplication>

namespace mitcad {

namespace {
void setStatus(const QString& text) {
  spaceMouseStatus().text = text;
}
} // namespace

// ---------------------------------------------------------------------------
// The process's connection (mitcad#2297)

SpaceMouseConnection::SpaceMouseConnection(Factory factory) : m_factory(std::move(factory)) {}

SpaceMouseConnection::~SpaceMouseConnection() = default;

std::shared_ptr<SpaceMouseConnection> SpaceMouseConnection::shared(const Factory& factory) {
  static std::weak_ptr<SpaceMouseConnection> live;
  if (auto connection = live.lock()) {
    return connection;
  }
  auto connection = std::make_shared<SpaceMouseConnection>(factory);
  live = connection;
  return connection;
}

QString SpaceMouseConnection::backendName() const {
  return m_backend ? m_backend->name() : QString();
}

SpaceMouseController* SpaceMouseConnection::current() const {
  return m_clients.empty() ? nullptr : m_clients.front();
}

void SpaceMouseConnection::motion(const SpaceMouseAxes& axes, bool device) {
  if (SpaceMouseController* client = current()) {
    client->motion(axes, device);
  }
}

void SpaceMouseConnection::button(int index, bool pressed) {
  if (SpaceMouseController* client = current()) {
    client->button(index, pressed);
  }
}

void SpaceMouseConnection::add(SpaceMouseController* client) {
  // The first design is the active one; a later one when the window
  // manager activates it (activate()).
  m_clients.push_back(client);
}

void SpaceMouseConnection::remove(SpaceMouseController* client) {
  const bool wasCurrent = current() == client;
  m_clients.erase(std::remove(m_clients.begin(), m_clients.end(), client), m_clients.end());
  m_wanting.erase(std::remove(m_wanting.begin(), m_wanting.end(), client), m_wanting.end());
  if (m_wanting.empty()) {
    // The last design that had the 3D mouse on closed: the device goes
    // with it, as a design's own backend went with its window before.
    m_backend.reset();
    return;
  }
  if (wasCurrent && m_backend) {
    // The design active before it gets the input now; the driver follows
    // its window.
    m_backend->windowPlaced();
  }
}

void SpaceMouseConnection::activate(SpaceMouseController* client) {
  const auto found = std::find(m_clients.begin(), m_clients.end(), client);
  if (found == m_clients.end() || found == m_clients.begin()) {
    return;
  }
  SpaceMouseController* previous = m_clients.front();
  std::rotate(m_clients.begin(), found, found + 1);
  currentChanged(previous);
}

void SpaceMouseConnection::currentChanged(SpaceMouseController* previous) {
  // A cap held while the design changes moves the one left no further.
  if (previous != nullptr && previous->m_axes != SpaceMouseAxes{}) {
    previous->motion(SpaceMouseAxes{}, previous->m_fromDevice);
  }
  if (m_backend) {
    m_backend->windowPlaced();
  }
}

void SpaceMouseConnection::windowPlaced() {
  // A connection the backend makes now comes through its `connection`
  // hook, as a device that comes later does.
  if (m_backend) {
    m_backend->windowPlaced();
  }
}

void SpaceMouseConnection::setWanted(SpaceMouseController* client, bool wanted) {
  const auto found = std::find(m_wanting.begin(), m_wanting.end(), client);
  if (wanted && found == m_wanting.end()) {
    m_wanting.push_back(client);
  } else if (!wanted && found != m_wanting.end()) {
    m_wanting.erase(found);
  }
  if (m_wanting.empty()) {
    if (m_backend || !m_loggedOff) {
      m_backend.reset();
      m_loggedOff = true;
      for (SpaceMouseController* each : m_clients) {
        each->m_axes = {};
        each->m_memory = SpaceMouseMemory();
      }
      qInfo().noquote() << QStringLiteral("3D mouse: off");
      setStatus(tr("Off"));
    }
    return;
  }
  m_loggedOff = false;
  if (!m_backend && m_factory) {
    start();
  }
}

void SpaceMouseConnection::start() {
  m_backend = m_factory();
  if (!m_backend) {
    qInfo().noquote() << QStringLiteral("3D mouse: none");
    setStatus(tr("No 3D mouse found"));
    return;
  }
  // The active design's: its window's activation and its native window.
  m_backend->acceptsInput = [this] {
    const SpaceMouseController* client = current();
    return client != nullptr && (!client->m_hooks.acceptsInput || client->m_hooks.acceptsInput());
  };
  m_backend->nativeWindow = [this] {
    const SpaceMouseController* client = current();
    return client != nullptr && client->m_hooks.nativeWindow ? client->m_hooks.nativeWindow() : std::uintptr_t{0};
  };
  m_backend->motion = [this](const SpaceMouseAxes& axes) { motion(axes, true); };
  m_backend->button = [this](int index, bool pressed) { button(index, pressed); };
  m_backend->connection = [this](bool present, const QString& device) {
    if (present) {
      connected(device);
    } else {
      disconnected(device);
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
}

void SpaceMouseConnection::connected(const QString& device) {
  qInfo().noquote() << QStringLiteral("3D mouse: %1 connected (%2)").arg(backendName(), device);
  setStatus(tr("Connected: %1").arg(device));
  // A copy: a design's settings change (its default button) may bring
  // its refresh along.
  const std::vector<SpaceMouseController*> wanting = m_wanting;
  for (SpaceMouseController* client : wanting) {
    client->connected(device);
  }
}

void SpaceMouseConnection::disconnected(const QString&) {
  qInfo().noquote() << QStringLiteral("3D mouse: %1 disconnected").arg(backendName());
  setStatus(tr("No 3D mouse found"));
  const std::vector<SpaceMouseController*> clients = m_clients;
  for (SpaceMouseController* client : clients) {
    client->deviceGone();
  }
}

// ---------------------------------------------------------------------------
// A design's controller

SpaceMouseController::SpaceMouseController(Hooks hooks, std::shared_ptr<SpaceMouseConnection> connection,
                                           QObject* parent)
    : QObject(parent), m_hooks(std::move(hooks)), m_connection(std::move(connection)) {
  if (!m_connection) {
    m_connection = std::make_shared<SpaceMouseConnection>(Factory());
  }
  m_connection->add(this);
  // A device's motion stops when the application goes to the background:
  // its release may go to another application.
  connect(qGuiApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state != Qt::ApplicationActive && m_fromDevice) {
      motion(SpaceMouseAxes{}, true);
    }
  });
}

SpaceMouseController::SpaceMouseController(Hooks hooks, Factory factory, QObject* parent)
    : SpaceMouseController(std::move(hooks), std::make_shared<SpaceMouseConnection>(std::move(factory)), parent) {}

SpaceMouseController::~SpaceMouseController() {
  m_connection->remove(this);
}

QString SpaceMouseController::backendName() const {
  return m_connection->backendName();
}

void SpaceMouseController::activate() {
  m_connection->activate(this);
}

void SpaceMouseController::refresh() {
  const SpaceMouseSettings settings = m_hooks.settings ? m_hooks.settings() : SpaceMouseSettings();
  if (!settings.enabled) {
    if (m_wanted) {
      m_axes = {};
      m_memory = SpaceMouseMemory();
    }
    m_wanted = false;
    m_connection->setWanted(this, false);
  } else {
    m_wanted = true;
    m_connection->setWanted(this, true);
  }
  apply();
  reportRotationLock();
}

void SpaceMouseController::deviceGone() {
  motion(SpaceMouseAxes{}, true);
  m_memory = SpaceMouseMemory();
}

void SpaceMouseController::connected(const QString& device) {
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

void SpaceMouseController::windowPlaced() {
  // The backend's window is the active design's: any design placed may be
  // it, or share its frame (mitcad#2297).
  m_connection->windowPlaced();
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
