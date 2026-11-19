// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

#include <QCoreApplication>
#include <QObject>
#include <QString>

#include "../platform/SpaceMouseDevice.hpp"
#include "SpaceMouseNavigation.hpp"

namespace mitcad {

// The 3D mouse of the window (mitcad#951): runs the platform's backend
// while Preferences has the 3D mouse on, turns its axes into the camera's
// rates (SpaceMouseNavigation.hpp) and its buttons into commands. It knows
// the view and the window only through its hooks, so that the unit tests
// drive it with a backend of their own.
//
// Logs (app/COMMANDS.md, "Logs that UI tests read"): "3D mouse: <backend>
// connected (<device>)", "3D mouse: none", "3D mouse: off", "3D mouse:
// <backend> disconnected", "3D mouse: button <n> runs <command>" (debug;
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
  };
  using Factory = std::function<std::unique_ptr<SpaceMouseBackend>()>;

  SpaceMouseController(Hooks hooks, Factory factory, QObject* parent = nullptr);
  ~SpaceMouseController() override;

  // Starts the backend when the settings have the 3D mouse on (and stops
  // it when they have it off); applies changed settings to the motion.
  void refresh();
  // The device's axes (the view's frame) or a test's; `device` false for a
  // test's input, which the application's activation does not stop.
  void motion(const SpaceMouseAxes& axes, bool device = true);
  // A button (0 the first) went down or up: a press runs its command.
  void button(int index, bool pressed);
  // Whether a backend runs, and its name.
  bool running() const { return m_backend != nullptr; }
  QString backendName() const;
  // Lock Rotation (mitcad#1910): whether the rotation is locked in the view
  // as it is now, and the toggle, which returns the new state.
  bool rotationLocked() const;
  bool toggleRotationLock();
  // A sketch began or ended: the rotation in a sketch starts locked again.
  void sketchChanged();

private:
  void apply();
  void setStatus(const QString& text);
  // A device is there: logged, shown in Preferences, and its rotation lock
  // key bound by default when its number is known.
  void connected(const QString& device);
  // Whether the view is flat now (a sketch); outside one a sketch's freed
  // rotation is forgotten.
  bool inSketch();
  void reportRotationLock();

  Hooks m_hooks;
  Factory m_factory;
  std::unique_ptr<SpaceMouseBackend> m_backend;
  SpaceMouseAxes m_axes{};
  SpaceMouseAxesLog m_axesLog;
  // The device's reach and the held dominant axis (mitcad#1720), afresh for
  // each device that connects.
  SpaceMouseMemory m_memory;
  bool m_fromDevice = false;
  bool m_loggedOff = false;
  // Lock Rotation freed the rotation in the sketch being edited.
  bool m_sketchFree = false;
};

} // namespace mitcad
