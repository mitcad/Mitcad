// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9, docs/updates.md): the check at start-up (at
// most once a day, unless turned off) and Help > Check for Updates; the
// notice of a newer release; with the user's consent the download, its
// verification, closing the windows (which ask to save) and the
// installation once Mitcad has quit, which starts the new version.
//
// One for the whole application (mitcad#1890): the update's state (an
// offer, a check or download under way, verified and waiting to install)
// and its row (UpdateRow) are the application's, every main window shows
// the same row, and closing a design's window keeps the download going
// while another window stays open. The last window asks before it drops a
// download under way (mayCloseLastWindow).
//
// Logs for the UI tests: "Update check: ...", "Update manifest: ...",
// "Update notice (<state>): <text> [<buttons>]", "Update row: in N
// window(s)", "Update download: N%", "Update verified: ...", "Update
// rejected: ...", "Update staged: ...", "Update installed: <from> -> <to>",
// "Update question: ...".

#include <functional>

#include <QObject>
#include <QString>

#include "UpdateClient.hpp"
#include "UpdateInstaller.hpp"

class QMainWindow;
class QTimer;
class QWidget;

namespace mitcad {

class CommandRegistry;
class UpdateRow;

class UpdateController : public QObject {
  Q_OBJECT

public:
  // What the update needs of the application's windows.
  struct Host {
    // Closes every design's window as Quit does, each asking to save its
    // design's changes; true when they all closed.
    std::function<bool()> closeWindows;
    // Calls `call` once no window's model computes anything (a job's
    // progress dialog would otherwise be in the way of closing).
    std::function<void(std::function<void()> call)> whenIdle;
  };

  // The application's one controller, made at the first call.
  static UpdateController& application();

  // The first window gives the host; later calls change nothing.
  void setHost(Host host);
  // Help > Check for Updates (help.check_updates) in a window's commands.
  void registerCommands(CommandRegistry& registry);
  // A window made: its bar under the toolbar, showing the update's row as
  // it is. The first window's also starts the updates: what an update just
  // installed did, and the automatic check when it is due.
  void attach(QMainWindow& window);
  // Help > Check for Updates.
  void checkNow();
  // The last design's window asks before it closes (Quit or Close): a
  // download under way is not dropped silently. MitCad says it is still
  // downloading and offers to keep going (the window stays open, false) or
  // to stop it (true). A verified update waiting to install is staged, so
  // that it installs once MitCad has quit (true); a check is stopped.
  bool mayCloseLastWindow(QWidget* window);
  // The UI test driver's step update-fake-offer (mitcad#1890): from now
  // on the updates offer the tests' fake release, MitCad 999.0.0, without
  // a request and installable whatever the installation; its download
  // takes `seconds` in steps and is then neither verified nor installed.
  // Only the test driver (MITCAD_TEST_INPUT) calls it.
  void useTestFakeOffer(int seconds);

  // The row every window shows (the unit tests read it).
  UpdateRow& row() { return *m_row; }

private:
  enum class State { Idle, Checking, Downloading, Installing };

  explicit UpdateController(QObject* parent = nullptr);

  void startUp();
  void check(bool manual);
  void onChecked(const UpdateOffer& offer);
  void onFailed(const QString& message);
  void offer(const UpdateOffer& offer);
  void install();
  void onProgress(qint64 received, qint64 total);
  void onDownloaded(const QString& path);
  // The verified download replaces the installation once Mitcad has quit.
  void finish(const QString& path);
  void cancelDownload();
  bool installable() const;
  bool canInstall() const;
  // The tests' fake offer (useTestFakeOffer).
  bool fake() const;
  void fakeStep();

  Host m_host;
  Installation m_installation;
  UpdateClient* m_client = nullptr;
  UpdateRow* m_row = nullptr;
  bool m_started = false;
  State m_state = State::Idle;
  bool m_manual = false;
  UpdateOffer m_offer;
  // The tens of percent last logged ("Update download: N%").
  int m_loggedTenths = -1;
  // The verified download waiting to install (State::Installing).
  QString m_verified;
  // The windows close to install: the last one does not ask.
  bool m_closingToInstall = false;
  // The last window's question is open: the install waits for its answer.
  bool m_asking = false;
  QTimer* m_fakeTimer = nullptr;
  qint64 m_fakeReceived = 0;
  int m_fakeSeconds = 0;
};

} // namespace mitcad
