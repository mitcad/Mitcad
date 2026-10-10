// SPDX-License-Identifier: MIT
#pragma once

// The remote of the current Cloud project in the application (P12 remote,
// mitcad#11, mitcad#89): the project's versions shared through a git server
// or a folder.
//
// - File > Sync, Check for Newer Versions, Open in Browser; Project
//   Settings (files/ProjectSettings.hpp) shares a project, changes its
//   address or stops syncing it, and New Project and Open from Cloud make
//   Cloud projects (files/ProjectDialogs.hpp).
// - Each version Save records is sent to the remote at once (Project
//   Settings, with Preferences' Cloud page as the default); without a
//   connection the versions wait, the project indicator shows them, and
//   they go when the remote can be reached again.
// - The remote is checked for newer versions when a design opens, every
//   so many minutes, and at the first change after opening; a newer
//   version of the open file shows a notice under the toolbar ("A newer
//   version of part.mitcad is on the remote, saved by ... Sync now?"), and
//   Save tells it too.
// - Sync fetches and brings the versions together (the version history's
//   `sync`); files changed on both sides are chosen in Resolve Sync
//   Conflicts. The open design is saved first, and opened again when the
//   sync changed its file.
// - Its state (synced, versions to send, newer ones, syncing, a conflict,
//   offline, sign-in needed) goes to the project indicator
//   (files/ProjectIndicator.hpp).
//
// The project is the window's current project (Host::projectRoot), never
// the design's path. The remote work runs on threads of its own
// (files/RemoteTask.hpp), one at a time.

#include <functional>
#include <optional>

#include <QDateTime>
#include <QJsonArray>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>

#include "ProjectIndicator.hpp"
#include "ProjectSettings.hpp"
#include "mitcad_bridge/lib.h"

class QMainWindow;
class QTimer;
class QToolBar;

namespace mitcad {

class CommandRegistry;
class RemoteNotice;
class RemoteTask;

class RemoteController : public QObject {
  Q_OBJECT

public:
  // What the controller asks the window.
  struct Host {
    // The current project's folder when it has versions ("" none).
    std::function<QString()> projectRoot;
    // The open design's file (absolute; empty when untitled).
    std::function<QString()> filePath;
    // Whether it has unsaved changes.
    std::function<bool()> modified;
    // Save (with its version); false when cancelled or failed.
    std::function<bool()> save;
    // Who records versions (files/MainWindowVersions.cpp), none when
    // cancelled.
    std::function<std::optional<QString>(const Project& project)> author;
    // Opens a project file as the window's design; false with the reason,
    // or with none when cancelled.
    std::function<bool(const QString& path, QString& error)> open;
    // The project's history moved (a sync): what Save compares the file
    // with, and the indicator's version.
    std::function<void()> versionsChanged;
    // The status bar's message, red when `error`.
    std::function<void(const QString& message, bool error)> status;
    // Opens Project Settings (a project without a remote is shared there).
    std::function<void()> projectSettings;
    // Runs `call` once the model's worker computes nothing (the window's
    // document is read then).
    std::function<void(std::function<void()> call)> whenIdle;
    // Edit locks (mitcad#89, files/LockController.hpp): before Sync sends
    // versions (false: cancelled, "Send Anyway" was not chosen), and the
    // remote's newer version of the open file after a fetch (`incoming`
    // with `path`): true when the lock controller took care of it (a
    // read-only window shows it, a design whose lock was just taken is
    // brought up to date), so no notice shows.
    std::function<bool()> confirmSend;
    std::function<bool(const QString& file, const QJsonObject& incoming)> newerVersion;
  };

  RemoteController(QMainWindow& window, Host host, QObject* parent = nullptr);
  ~RemoteController() override;

  void registerCommands(CommandRegistry& registry);
  // The notice under the toolbar and the settings applied; after the
  // window's other bars.
  void startUp();
  // The remote's state for the project indicator.
  ProjectIndicator::Remote indicatorState() const;
  // "Up to date, last sync 10:42" (Project Settings' Status row).
  QString statusText() const;

  // The current project changed (opened, made, shared, stopped syncing):
  // its remote's state, and a check for newer versions when `check`.
  void projectChanged(bool check);
  // A project file opened as the design: its remote's state, and a check
  // for newer versions.
  void fileOpened(const QString& path);
  // Save (or Project Settings) recorded a version in the project: sent to
  // the remote at once (unless turned off), and a newer version on the
  // remote told.
  void versionRecorded(const QString& path);
  // The open design's first change since it opened: the remote is checked
  // when it was not lately.
  void firstChange();
  // The state of the current project's remote, read from the project (no
  // network).
  void refreshStatus();
  // Preferences or Project Settings changed: the git program, the checks,
  // sending at once.
  void settingsChanged();
  // Before Save or Restore records a version: while a sync or a connection
  // moves the project's branch, waits for it (with a Cancel that stops it).
  void waitForBranch();
  // Stops a running background check or push and waits for it; false (and
  // `why` can wait, said) while a sync runs.
  bool settle(const QString& why);
  // Runs a task while a progress dialog with Cancel shows; its answer.
  QJsonObject runModal(RemoteTask* task, const QString& label);
  // Schedules sending the versions that wait, after a push failed.
  void retryLater();
  // Before the window closes: running work is stopped (versions not sent
  // stay waiting, and go at the next start).
  void shutDown();

  // The commands.
  void sync();
  void check();
  void openInBrowser();

  // Live updates (mitcad#89, files/LiveController.hpp): while the current
  // project's live connection is up, newer versions are announced and the
  // remote is not checked on the timer; an announced version (or a
  // connection back after missing messages) checks it at once, which shows
  // it in the indicator and the notice of a newer version.
  void setLive(bool live);
  void versionAnnounced();

  // Edit locks (mitcad#89): a fetch without telling its outcome (a
  // read-only window follows the editor's versions), and the project's
  // unpublished versions sent before an edit lock is released or handed
  // over: a sync while a progress dialog shows, after the running work.
  // Its answer ({"case": "nothing"} when nothing waits to be sent; a
  // conflict is the error class "conflict", which is not asked about).
  void checkQuietly();
  QJsonObject syncNow(const QString& label);

signals:
  // The remote's state changed (indicatorState()).
  void stateChanged();
  // Versions of the project in `root` went to the remote (a push, or a sync
  // that pushed): the branch there and its new commit.
  void versionsSent(const QString& root, const QString& branch, const QString& commit);

private:
  enum class Mode { Quiet, Shown };
  // The remote of the current project.
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

  // A command of the project in `path` on this thread (no network:
  // remote_info, incoming, status, commit, remote_remove); empty when it
  // fails.
  QJsonObject local(const QString& path, const QJsonObject& command) const;
  // The current project with history, or none.
  std::optional<rust::Box<Project>> project() const;
  // Starts a background task (false, and the task deleted, when one runs
  // or work is held); `done` gets its answer once the model is idle.
  bool startTask(RemoteTask* task, std::function<void(const QJsonObject&)> done);
  void taskFinished();
  // Starts what waited for the task that ended.
  void runWaiting();
  void publishState();
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
  State m_state;
  // Sending at once and the check interval of the current project.
  ProjectSync m_sync;
  QString m_syncRoot; // the project m_sync was read for
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
  QToolBar* m_bar = nullptr;
  RemoteNotice* m_notice = nullptr;
  QString m_noticeId;  // the remote's version the notice tells of
  QString m_dismissed; // the one whose notice was dismissed
  QTimer* m_checkTimer = nullptr;
  QTimer* m_retryTimer = nullptr;
  int m_retries = 0;
  bool m_live = false; // the current project's live updates are connected
};

} // namespace mitcad
