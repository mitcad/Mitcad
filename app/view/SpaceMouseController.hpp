// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>
#include <vector>

#include <QCoreApplication>
#include <QObject>
#include <QString>

#include "../platform/SpaceMouseDevice.hpp"
#include "SpaceMouseNavigation.hpp"

namespace mitcad {

class SpaceMouseController;

// The process's one connection to the 3D mouse (mitcad#2297, GitHub
// mitcad/Mitcad#587): one platform backend, owned by the application and
// shared by every design's SpaceMouseController. Windows keeps one Raw
// Input target per device class and process, so a backend per design took
// the device from the designs opened before it, and the first one closed
// removed it for all of them; the 3DxWare driver, given a session per
// design for the same frame window, sent its events to the newest session
// only, and to none after that one closed (the reporter's log);
// spacenavd and IOKit HID gave every design's backend the same events,
// which moved the hidden designs' views too.
//
// The backend runs while at least one of its controllers has the 3D mouse
// on (each design's settings, as today): turned off in one design, or that
// design closed, it runs on for the others; it stops when the last one
// turns it off ("3D mouse: off") or closes. Motion and buttons go to the
// active design's controller only (`current`, activate()): the one the
// window manager made active last, and when that one closes the one active
// before it. A design left while the cap is held stops moving. The
// backend's `acceptsInput` and `nativeWindow` are the active design's, and
// when it changes (another design activated, the active one closed or put
// in another frame) the backend hears windowPlaced(): the Windows driver's
// session follows the active design's top-level window.
//
// Logs: "3D mouse: <backend> connected (<device>)", "3D mouse: none",
// "3D mouse: off" and "3D mouse: <backend> disconnected", once for the
// process, no longer once per design.
class SpaceMouseConnection {
  Q_DECLARE_TR_FUNCTIONS(mitcad::SpaceMouseController)

public:
  using Factory = std::function<std::unique_ptr<SpaceMouseBackend>()>;

  explicit SpaceMouseConnection(Factory factory);
  ~SpaceMouseConnection();
  SpaceMouseConnection(const SpaceMouseConnection&) = delete;
  SpaceMouseConnection& operator=(const SpaceMouseConnection&) = delete;

  // The application's connection: the one that lives now, or a new one
  // that makes its backend with `factory` (createSpaceMouseBackend). The
  // controllers hold it; it ends with the last of them.
  static std::shared_ptr<SpaceMouseConnection> shared(const Factory& factory);

  // Whether the backend runs, and its name.
  bool running() const { return m_backend != nullptr; }
  QString backendName() const;
  // The active design's controller, which gets the input (null: none).
  SpaceMouseController* current() const;
  // Input as the backend gives it, to the active design's controller only;
  // `device` false for a test's input (the test driver's `space-mouse`).
  void motion(const SpaceMouseAxes& axes, bool device = true);
  void button(int index, bool pressed);

private:
  friend class SpaceMouseController;
  void add(SpaceMouseController* client);
  void remove(SpaceMouseController* client);
  void activate(SpaceMouseController* client);
  // Whether `client`'s settings have the 3D mouse on (refresh()).
  void setWanted(SpaceMouseController* client, bool wanted);
  void windowPlaced();
  void start();
  void connected(const QString& device);
  void disconnected(const QString& device);
  // The active design changed: a held motion of the one left stops, and
  // the backend may follow the new one's window.
  void currentChanged(SpaceMouseController* previous);

  Factory m_factory;
  std::unique_ptr<SpaceMouseBackend> m_backend;
  // The controllers, the active design's first, then the ones active
  // before it, most recent first.
  std::vector<SpaceMouseController*> m_clients;
  // The controllers whose settings have the 3D mouse on.
  std::vector<SpaceMouseController*> m_wanting;
  bool m_loggedOff = false;
};

// The 3D mouse of the window (mitcad#951): runs the platform's backend
// (through the process's SpaceMouseConnection, mitcad#2297) while
// Preferences has the 3D mouse on, turns its axes into the camera's
// rates (SpaceMouseNavigation.hpp) and its buttons into commands. It knows
// the view and the window only through its hooks, so that the unit tests
// drive it with a backend of their own.
//
// Logs (app/COMMANDS.md, "Logs that UI tests read"): "3D mouse: <backend>
// connected (<device>)", "3D mouse: none", "3D mouse: off", "3D mouse:
// <backend> disconnected" (these four the connection's, once for the
// process), "3D mouse: button <n> runs <command>" (debug;
// "3D mouse: button <n> has no command"), "3D mouse axes: tx …, ty …, tz …,
// rx …, ry …, rz …; strongest <axis>: in Object mode the model …" (debug,
// when the cap comes to rest; mitcad#1720), "3D mouse: rotation locked"
// and "3D mouse: rotation unlocked" (debug, Lock Rotation; " in the
// sketch" in a sketch), "3D mouse: button <n> runs <command> by default"
// (a device's known rotation lock key; mitcad#1910).
//
// Lock Rotation (mitcad#1910): outside a sketch it toggles the settings'
// Lock rotation (saved through the `change` hook, as Preferences saves
// it). In a sketch (a flat view) the rotation starts locked whatever the
// settings say, and the toggle frees it for that sketch only: the next
// sketch, or the same one entered again, starts locked (sketchChanged).
class SpaceMouseController : public QObject {
  Q_DECLARE_TR_FUNCTIONS(mitcad::SpaceMouseController)

public:
  struct Hooks {
    // The settings in use (the view's navigation).
    std::function<SpaceMouseSettings()> settings;
    // Whether the view is flat now (sketch mode): pan and zoom only.
    std::function<bool()> flat;
    // The camera's rates; zero rates stop the motion.
    std::function<void(const SpaceMouseRates&)> rates;
    // A button's command by its id.
    std::function<void(const QString& id)> runCommand;
    // Saves changed settings (Lock Rotation outside a sketch, a device's
    // default button) as Preferences saves them; `settings` follows.
    std::function<void(const SpaceMouseSettings&)> change;
    // Whether the rotation is locked now, after anything that may have
    // changed it (a toggle, a sketch begun or ended, new settings): Lock
    // Rotation's check mark follows.
    std::function<void(bool locked)> rotationLock;
    // Whether the associated window is active.
    std::function<bool()> acceptsInput;
    std::function<std::uintptr_t()> nativeWindow;
    // At every input, whether the cap is deflected past the dead zone on
    // any axis, switched off, locked or not (mitcad#1981: the view ignores
    // the wheel then, which the driver may emulate from the cap).
    std::function<void(bool deflected)> deflected;
  };
  using Factory = SpaceMouseConnection::Factory;

  // On the process's connection (the application's design windows,
  // SpaceMouseConnection::shared), or on one of its own made with
  // `factory` (a unit test's single controller).
  SpaceMouseController(Hooks hooks, std::shared_ptr<SpaceMouseConnection> connection, QObject* parent = nullptr);
  SpaceMouseController(Hooks hooks, Factory factory, QObject* parent = nullptr);
  ~SpaceMouseController() override;

  // Asks the connection for the backend when the settings have the 3D
  // mouse on (and gives it up when they have it off; the backend stops
  // when no design wants it); applies changed settings to the motion.
  void refresh();
  // The device's axes (the view's frame) or a test's; `device` false for a
  // test's input, which the application's activation does not stop. This
  // controller's own: the connection gives each its input
  // (SpaceMouseConnection::motion).
  void motion(const SpaceMouseAxes& axes, bool device = true);
  // A button (0 the first) went down or up: a press runs its command.
  void button(int index, bool pressed);
  // Whether a backend runs for this design, and its name.
  bool running() const { return m_wanted && m_connection->running(); }
  QString backendName() const;
  // The design became the active one (mitcad#2297): the 3D mouse's input
  // comes here from now on.
  void activate();
  SpaceMouseConnection& connection() const { return *m_connection; }
  // Lock Rotation (mitcad#1910): whether the rotation is locked in the view
  // as it is now, and the toggle, which returns the new state.
  bool rotationLocked() const;
  bool toggleRotationLock();
  // A sketch began or ended: the rotation in a sketch starts locked again.
  void sketchChanged();
  // The design's window is shown in its frame (mitcad#2152): the backend
  // may now find the native window it found none of at start.
  void windowPlaced();

private:
  friend class SpaceMouseConnection;
  void apply();
  // A device is there (the connection logged it and showed it in
  // Preferences): its reach is learned afresh, and its rotation lock key
  // bound by default when its number is known.
  void connected(const QString& device);
  // The device went, or the connection stopped: no motion, its reach
  // forgotten.
  void deviceGone();
  // Whether the view is flat now (a sketch); outside one a sketch's freed
  // rotation is forgotten.
  bool inSketch();
  void reportRotationLock();

  Hooks m_hooks;
  std::shared_ptr<SpaceMouseConnection> m_connection;
  // The settings have the 3D mouse on (refresh()).
  bool m_wanted = false;
  SpaceMouseAxes m_axes{};
  SpaceMouseAxesLog m_axesLog;
  // The device's reach and the held dominant axis (mitcad#1720), afresh for
  // each device that connects.
  SpaceMouseMemory m_memory;
  bool m_fromDevice = false;
  // Lock Rotation freed the rotation in the sketch being edited.
  bool m_sketchFree = false;
};

} // namespace mitcad
