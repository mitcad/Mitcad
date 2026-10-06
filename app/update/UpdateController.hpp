// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9, docs/updates.md): the check at start-up (at
// most once a day, unless turned off) and Help > Check for Updates; the
// notice of a newer release; with the user's consent the download, its
// verification, closing the window (which asks to save) and the
// installation once Mitcad has quit, which starts the new version.
//
// Logs for the UI tests: "Update check: ...", "Update manifest: ...",
// "Update notice (<state>): <text> [<buttons>]", "Update verified: ...",
// "Update rejected: ...", "Update staged: ...", "Update installed: <from>
// -> <to>".

#include <functional>

#include <QObject>
#include <QString>

#include "UpdateClient.hpp"
#include "UpdateInstaller.hpp"

class QMainWindow;
class QToolBar;

namespace mitcad {

class CommandRegistry;
class UpdateNotice;

class UpdateController : public QObject {
  Q_OBJECT

public:
  struct Host {
    // Closes the main window as File > Exit does, asking to save the
    // document's changes; true when it closed.
    std::function<bool()> closeWindow;
    // Calls `call` once the model computes nothing (a job's progress
    // dialog would otherwise be in the way of closing).
    std::function<void(std::function<void()> call)> whenIdle;
  };

  UpdateController(QMainWindow& window, Host host, QObject* parent = nullptr);

  // Help > Check for Updates (help.check_updates).
  void registerCommands(CommandRegistry& registry);
  // Once the window is made: the notice's bar under the toolbar, what an
  // update just installed did, and the automatic check when it is due.
  void startUp();
  // Help > Check for Updates.
  void checkNow();

private:
  enum class State { Idle, Checking, Downloading, Installing };

  void check(bool manual);
  void onChecked(const UpdateOffer& offer);
  void onFailed(const QString& message);
  void offer(const UpdateOffer& offer);
  void install();
  void onDownloaded(const QString& path);
  // The verified download replaces the installation once Mitcad has quit.
  void finish(const QString& path);
  void showNotice();
  void hideNotice();
  bool installable() const;

  QMainWindow& m_window;
  Host m_host;
  Installation m_installation;
  UpdateClient* m_client = nullptr;
  QToolBar* m_bar = nullptr;
  UpdateNotice* m_notice = nullptr;
  State m_state = State::Idle;
  bool m_manual = false;
  UpdateOffer m_offer;
};

} // namespace mitcad
