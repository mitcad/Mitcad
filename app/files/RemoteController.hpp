// SPDX-License-Identifier: MIT
#pragma once

// Remote repositories in the application (P12 remote, mitcad#11): a
// project's versions shared through a git server or a folder.
//
// - File > Connect Project to Remote, Open Project from Remote, Sync,
//   Check for Newer Versions, Remote Settings (files/RemoteDialogs.hpp).
// - Each version Save records is sent to the remote at once (Preferences,
//   Version Control); without a connection the versions wait, the status
//   bar shows them, and they go when the remote can be reached again.
// - The remote is checked for newer versions when a design opens, every
//   so many minutes, and at the first change after opening; a newer
//   version of the open file shows a notice under the toolbar ("A newer
//   version of part.mitcad is on the remote, saved by ... Sync now?"), and
//   Save tells it too.
// - Sync fetches and brings the versions together (the version history's
//   `sync`); files changed on both sides are chosen in Resolve Sync
//   Conflicts. The open design is saved first, and opened again when the
//   sync changed its file.
// - The status bar shows the remote's state next to the version: synced,
//   versions to send (an up arrow), newer ones on the remote (a down
//   arrow), syncing, a conflict, offline, sign-in needed; a click gives its
//   menu.
//
// The remote work runs on threads of its own (files/RemoteTask.hpp), one
// at a time; the window asks it about the open file through Host.

#include <functional>
#include <optional>

#include <QDateTime>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>

#include "mitcad_bridge/lib.h"

class QMainWindow;
class QMenu;
class QTimer;
class QToolBar;
class QToolButton;

namespace mitcad {

class CommandRegistry;
class RemoteNotice;
class RemoteTask;

class RemoteController : public QObject {
  Q_OBJECT

public:
  // What the controller asks the window.
  struct Host {
    // The open design's file (absolute; empty when untitled).
    std::function<QString()> filePath;
    // Whether it has unsaved changes.
    std::function<bool()> modified;
    // Save (with its version); false when cancelled or failed.
    std::function<bool()> save;
    // Asks about unsaved changes before another design opens; false: cancel.
    std::function<bool()> maybeSave;
    // Who records versions (files/MainWindowVersions.cpp), none when
    // cancelled.
    std::function<std::optional<QString>(const Project& project)> author;
    // Opens a project file as the window's design; false with the reason,
    // or with none when cancelled.
    std::function<bool(const QString& path, QString& error)> open;
    // The project's history moved (a sync): what Save compares the file
    // with, and the status bar's version.
    std::function<void()> versionsChanged;
    // The status bar's message, red when `error`.
    std::function<void(const QString& message, bool error)> status;
    // Starts Version History for a design without one.
    std::function<void()> startHistory;
    // Runs `call` once the model's worker computes nothing (the window's
    // document is read then).
    std::function<void(std::function<void()> call)> whenIdle;
  };

  RemoteController(QMainWindow& window, Host host, QObject* parent = nullptr);
  ~RemoteController() override;

  void registerCommands(CommandRegistry& registry);
  // The notice under the toolbar and the settings applied; after the
  // window's other bars.
  void startUp();
  // The status bar's remote state (a button with the remote's menu).
  QToolButton* statusButton() const { return m_button; }

  // A project file opened as the design: its remote's state, and a check
  // for newer versions.
  void fileOpened(const QString& path);
  // Save recorded a version of `path`: sent to the remote at once (unless
  // turned off), and a newer version on the remote told.
  void versionRecorded(const QString& path);
  // The open design's first change since it opened: the remote is checked
  // when it was not lately.
  void firstChange();
  // The state of the open file's remote, read from the project (no
  // network).
  void refreshStatus();
  // Preferences changed: the git program, the checks.
  void settingsChanged();
  // Before Save or Restore records a version: while a sync or a connection
  // moves the project's branch, waits for it (with a Cancel that stops it).
  void waitForBranch();
  // Before the window closes: running work is stopped (versions not sent
  // stay waiting, and go at the next start).
  void shutDown();

  // The commands.
  void connectProject();
  void openFromRemote();
  void sync();
  void check();
  void showSettings();
  void openInBrowser();

private:
  enum class Mode { Quiet, Shown };
  // What the status bar shows: the remote of the open file's project.
  struct State {
    QString root; // the project's folder ("" none)
    QString name; // the remote ("" none)
    QString url;
    QString upstream;
    int ahead = -1; // -1: not known yet
    int behind = -1;
    QString problem; // the class of the latest failure, "" none
    QString problemMessage;
    QDateTime lastDone; // the latest fetch, push or sync that went well
    QDateTime lastFetch;
    QString newest; // " (the newest by Name, 14:02)" of the remote's
    bool conflict = false; // a sync stopped for choices
  };

  // A command of the project of `path` on this thread (no network:
  // remote_info, incoming, status, commit, remote_remove); empty when it
  // fails.
  QJsonObject local(const QString& path, const QJsonObject& command) const;
  // The open file's project with history, or none.
  std::optional<rust::Box<Project>> project() const;
  // Starts a background task (false, and the task deleted, when one runs
  // or work is held); `done` gets its answer once the model is idle.
  bool startTask(RemoteTask* task, std::function<void(const QJsonObject&)> done);
  // Runs a task while a progress dialog with Cancel shows; its answer.
  QJsonObject runModal(RemoteTask* task, const QString& label);
  // Stops a running background check or push and waits for it; false (and
  // `why` can wait, said) while a sync runs.
  bool settle(const QString& why);
  void taskFinished();
  // Starts what waited for the task that ended.
  void runWaiting();
  void updateButton();
  void logFailure(const QString& what, const QJsonObject& answer);
  // A fetch (`Shown`: asked for, its outcome told).
  void startCheck(Mode mode);
  void push();
  void runSync(const QString& author, const QJsonObject& resolutions, bool fetch);
  void syncFinished(const QString& author, const QJsonObject& answer);
  // After a sync changed the project: the open file opened again when it
  // changed and has no unsaved changes; else what Save compares updated.
  void afterSync(const QJsonObject& answer);
  // Offers to record the changes no version holds before a sync again.
  bool recordBeforeSync(const QString& author, const QJsonObject& answer);
  // Shows or hides the notice of a newer version of the open file (after
  // a fetch; `saved`: Save just recorded one).
  void updateNotice(bool saved);
  void scheduleRetry();
  void applyTimer();
  QString compareConflict(const QJsonObject& conflict) const;

  QMainWindow& m_window;
  Host m_host;
  CommandRegistry* m_registry = nullptr;
  State m_state;
  QString m_loggedState;
  QPointer<RemoteTask> m_task;
  std::function<void(const QJsonObject&)> m_taskDone;
  // What waits for the running task: a push, a check, a sync.
  bool m_pushWaiting = false;
  bool m_checkWaiting = false;
  bool m_syncWaiting = false;
  bool m_settingsWaiting = false; // the git program changes when idle
  // No new task starts (stopping work for another project, or closing).
  bool m_holding = false;
  // Save waits for a sync: its outcome is told, not asked about.
  bool m_branchWait = false;
  bool m_interactive = true;
  // The file opened again after a sync or a clone: no check of its own.
  bool m_skipCheck = false;
  QToolButton* m_button = nullptr;
  QMenu* m_menu = nullptr;
  QToolBar* m_bar = nullptr;
  RemoteNotice* m_notice = nullptr;
  QString m_noticeId;  // the remote's version the notice tells of
  QString m_dismissed; // the one whose notice was dismissed
  QTimer* m_checkTimer = nullptr;
  QTimer* m_retryTimer = nullptr;
  int m_retries = 0;
};

} // namespace mitcad
