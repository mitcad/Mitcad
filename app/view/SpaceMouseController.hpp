// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <memory>

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
// "3D mouse: button <n> has no command").
class SpaceMouseController : public QObject {
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

private:
  void apply();
  void setStatus(const QString& text);

  Hooks m_hooks;
  Factory m_factory;
  std::unique_ptr<SpaceMouseBackend> m_backend;
  SpaceMouseAxes m_axes{};
  bool m_fromDevice = false;
  bool m_loggedOff = false;
};

} // namespace mitcad
