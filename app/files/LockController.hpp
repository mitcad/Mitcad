// SPDX-License-Identifier: MIT
#pragma once

// Edit locks in the application (mitcad#89; the core's commands in
// core/model/src/api/commands.md, "Edit locks"). In a Cloud project with
// edit locks on, the design open in the window has one editor at a time:
//
// - Opening a design takes its lock on the remote (lock_take, on a thread
//   of its own; the window stays editable meanwhile). Held by someone else,
//   the window is read-only with a question (Continue Read-Only, Request
//   Edit Access); held by the same person elsewhere, Take Over or Open
//   Read-Only; the remote out of reach, editable with "Edit lock not
//   confirmed (offline)" until it answers.
// - Holding it: polls (lock_poll) every poll interval (2 minutes while live
//   updates are connected), which also write the receipts of requests, a
//   refresh every half idle time, and the idle time without activity (a
//   command, an edit, a selection, a view change): no unsaved changes, a
//   sync and a release; unsaved changes with autosave on, a version first;
//   with autosave off the lock stays, marked idle, and a request gets it at
//   once. A request shows a dialog that is not modal (Release, Keep 15 More
//   Minutes, Decline...); no answer within the idle time releases as idle
//   time does.
// - Read-only windows: editing is refused where every edit passes (the
//   window's command registry and its model commands, MainWindow); a
//   banner says why, with Request Edit Access..., Edit or Save as Copy...;
//   the holder's pushed versions are shown as they come, the file left as
//   it is. A request's state follows the polls: granted (the holder handed
//   the lock over), declined, kept, or taken when the holder's Mitcad did
//   not answer.
// - A lock that is no longer this session's (taken as stale, taken over)
//   turns the window read-only; unsaved changes stay for Save as Copy or
//   Save as New Version.
// - Closing the design or quitting sends its unpublished versions (a sync)
//   and releases its lock and requests; a sync that needs a choice asks
//   Resolve Sync, Release Without Sending or Keep Lock.
//
// Lock commands run as RemoteTasks one at a time with a Project of their
// own (the core keeps what it observed of a project's lock refs per folder
// in the process). MITCAD_LOCK_TIME_SCALE speeds up the timers as it does
// the core's clock (UI tests). What others should learn at once goes to
// the live controller through a LiveLink; its events come back through
// LockEvents.

#include <deque>
#include <functional>
#include <stdexcept>

#include <QDateTime>
#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QString>

#include "LiveLink.hpp"
#include "Projects.hpp"

class QAction;
class QMainWindow;
class QMessageBox;
class QTimer;
class QToolBar;

namespace mitcad {

class LockBanner;
class ProjectIndicator;
class RemoteController;
class RemoteTask;

// A model command refused because the window is read-only (MainWindow).
class ReadOnlyError : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};

class LockController : public QObject, public LockEvents {
  Q_OBJECT

public:
  // What the controller asks the window.
  struct Host {
    // The window's current project.
    std::function<ProjectState()> project;
    // This run's session (a UUID, autosave's too).
    std::function<QString()> session;
    // The design has unsaved changes.
    std::function<bool()> modified;
    // Preferences' autosave is on.
    std::function<bool()> autosaveOn;
    // Saving is possible now: no command panel, sketch, job or modal
    // dialog.
    std::function<bool()> canSaveNow;
    // Saves the design as a version (`message`; "" the automatic one);
    // false when that failed or was cancelled.
    std::function<bool(const QString& message)> saveVersion;
    // Shows the design as `commit` of the project has it, the file and the
    // view left as they are; false with the reason when it could not.
    std::function<bool(const QString& commit, QString& error)> showVersion;
    // The window turned read-only or editable.
    std::function<void()> readOnlyChanged;
    // The status bar's message, red when `error`.
    std::function<void(const QString& message, bool error)> status;
    // Save as Copy... and Save as New Version (a read-only window's
    // unsaved changes).
    std::function<void()> saveAsCopy;
    std::function<void()> saveAsNewVersion;
    // Runs `call` once the model's worker computes nothing (what a task's
    // answer changes in the window waits for that), and whether it does.
    std::function<void(std::function<void()> call)> whenIdle;
    std::function<bool()> busy;
    // The project's live updates for the lock's details (the broker, the
    // connection's state, dropped messages); "" without any.
    std::function<QString(const QString& root)> liveDetails;
  };

  LockController(QMainWindow& window, RemoteController& remote, ProjectIndicator& indicator, Host host,
                 QObject* parent = nullptr);
  ~LockController() override;

  // The banner under the toolbar; after the window's other bars.
  void startUp();
  // The live controller (null: none, or not connected).
  void setLiveLink(LiveLink* link);

  // The window is read-only: what refuses an edit says why.
  bool isReadOnly() const;
  QString readOnlyReason() const;
  // A read-only window has unsaved changes from before it turned read-only
  // (a lost lock, a hand-over without saving): Save as Copy or Save as New
  // Version keeps them.
  bool keptChanges() const { return isReadOnly() && m_kept; }

  // The window's design changed (installDocument): `path` opened (""
  // untitled), `readOnly` by Open Read-Only. The same file opened again (a
  // sync, a restore, the editor's version shown) keeps its lock.
  void designOpened(const QString& path, bool readOnly);
  // Before the window's design is replaced by `next` ("" untitled; opened
  // read-only): its versions sent and its lock released (a progress
  // dialog). False to keep the design (Resolve Sync chosen).
  bool designClosing(const QString& next, bool readOnly);
  // Save wrote the design to `path`: under another name (Save As, Save as
  // Copy), the lock of the old file goes and the new file's is taken.
  void designSaved(const QString& path);
  // The current project changed (shared, stopped syncing, another one).
  void projectChanged();
  // The window closes: as designClosing; false to stay open.
  bool quitting();
  // Project Settings changed the project's settings (edit locks on or off,
  // the idle time and the poll interval).
  void settingsChanged();
  // Before the project stops syncing: this session's locks released.
  void stoppingSync();
  // Whether the project's remote accepts lock refs (lock_probe, once per
  // remote): `done` gets it on this thread unless `context` is gone.
  void probe(QObject* context, std::function<void(bool accepted, const QString& message)> done);
  // The user did something in the window (a command, an edit, a
  // selection, a view change).
  void activity();
  // Before Sync sends versions: files someone else holds the lock of are
  // asked about (Send Anyway); false to cancel.
  bool confirmSend();
  // The remote has a newer version of the open file (`incoming`): true
  // when it was taken care of (RemoteController::Host::newerVersion).
  bool newerVersion(const QString& file, const QJsonObject& incoming);
  // Save as New Version recorded the read-only window's changes.
  void changesSaved();
  // The indicator's menu entries (Release Edit Lock, Edit Lock Details...).
  QList<QAction*> actions() const;

  // LockEvents (the live controller).
  void liveEvent(const QString& root, const QJsonObject& event) override;
  void liveStateChanged(const QString& root, bool connected) override;

private:
  enum class Mode {
    None,        // no edit lock: not a Cloud project, locks off, untitled
    Taking,      // the lock is being taken: editable
    Holding,     // this session holds it
    Unconfirmed, // the remote could not be reached: editable, tried again
    ReadOnly,    // see Why
  };
  enum class Why {
    HeldByOther, // someone else holds it
    Opened,      // Open Read-Only
    Released,    // released at the idle time or by Release Edit Lock
    Lost,        // taken by someone else while this session held it
    HandedOver,  // handed over to a requester
    Free,        // nobody holds it now (released by its holder)
  };

  // Tasks: one at a time; `done` gets the answer unless the design changed
  // meanwhile.
  void startTask(RemoteTask* task, std::function<void(const QJsonObject&)> done);
  void taskFinished();
  // A task while a progress dialog shows (after the running one, whose
  // answer is dropped); its answer.
  QJsonObject runNow(RemoteTask* task, const QString& label);
  void dropRunning();
  RemoteTask* lockTask(const QString& name, const QList<QJsonObject>& commands);
  QJsonObject lockCommand(const char* name, QJsonObject fields = {}) const;

  // The design.
  bool lockable(QString* why = nullptr) const;
  // The current project's remote refused lock refs in this run (a take or
  // the probe said so): no lock is taken there again.
  bool remoteRefuses() const;
  void readSettings();
  void resetDesign();
  QString fileName() const;

  // Taking, polling, refreshing.
  void take(const QString& takeOver = QString());
  void taken(const QJsonObject& answer);
  void becomeHolder(const QJsonObject& answer, const QJsonObject& view);
  void bringUpToDate(const QJsonObject& view);
  void schedulePoll(int seconds = -1);
  void poll();
  void handleView(const QJsonObject& view);
  void handleHolding(const QJsonObject& lock);
  void handleReadOnly(const QJsonObject& lock, const QJsonObject& view);
  // `base`: the version the holder edits now (after a Save), "" as it was.
  void refresh(bool stateChanged = false, const QString& base = QString());
  void startHoldingTimers();
  void stopTimers();
  void idleTimeout();
  void lost(const QJsonObject& lock);
  void turnReadOnly(Why why, const QJsonObject& lock);
  void locksTurnedOff();

  // Requests and answers.
  void askForAccess();
  void sendRequest(const QString& message);
  void showHeldDialog(const QJsonObject& lock);
  void showSameOwnerDialog(const QJsonObject& lock);
  void showAnswerDialog(const QJsonObject& request, const QJsonArray& requests);
  void closeAnswerDialog();
  void sendAnswer(const QJsonObject& request, const QString& kind, const QString& message);
  // The holder lets go: unsaved changes saved (unless `withoutSaving`),
  // the versions sent, the lock handed over to the request's session.
  void handOver(const QJsonObject& request, bool withoutSaving, const QString& why);
  // Sends the versions and releases this session's locks and requests;
  // false when the user chose to keep the design (Resolve Sync).
  bool closeLock(bool quit);
  bool releaseAll(const QString& label);
  void releaseByUser();
  void showDetails();
  void offlineWill(const QString& session, const QJsonObject& event);

  // What shows.
  void updateIndicator();
  void updateBanner();
  void publishOpen();
  void publishLock(const QJsonObject& lock);
  QString heldText(const QJsonObject& lock) const;
  // The project's author's name, as the live messages carry it.
  QString myName();
  // Who else has the design open (live updates): "Also open: Sam
  // (read-only, since 14:05)." and its entries.
  QString alsoOpenText() const;
  QString alsoOpenEntries() const;
  int pollSeconds() const;
  bool liveConnected() const;
  QString relativeOf(const QString& path) const;

  QMainWindow& m_window;
  RemoteController& m_remote;
  ProjectIndicator& m_indicator;
  Host m_host;
  LiveLink* m_live = nullptr;

  Mode m_mode = Mode::None;
  Why m_why = Why::HeldByOther;
  QString m_root;     // the design's project ("" none)
  QString m_path;     // the design's file (absolute, "" untitled)
  QString m_relative; // in the project, with "/"
  QString m_fileId;   // SHA-256 of m_relative (the lock's id)
  QString m_myName;   // the project's author's name (myName)
  bool m_locksOn = true;
  int m_idleMinutes = 10;
  int m_projectPollSeconds = 10;
  QJsonObject m_lock;          // the design's lock as last seen (empty: none)
  QJsonObject m_view;          // the last poll's view
  bool m_requested = false;    // this session asked for the lock
  bool m_granted = false;      // a lock handed over to this session is being taken
  QString m_requestState;      // the request's state as last logged
  QString m_declined;          // the holder declined: their message
  bool m_idle = false;         // holding, marked idle
  bool m_kept = false;         // read-only with unsaved changes from editing
  bool m_offline = false;      // the last poll could not reach the remote
  bool m_following = false;    // the window shows the editor's version, not the file
  QString m_lostText;          // the banner of a lost lock
  QString m_releasedText;      // the banner of a released lock
  QDateTime m_lastActivity;    // UTC
  QString m_checkedHead;       // the remote's head a check was asked for
  bool m_syncAfterCheck = false; // just taken: bring the design up to date
  bool m_newerWhileTaking = false; // a newer version came while the lock was taken
  bool m_checkedWhileTaking = false; // a check told of this file while the lock was taken
  QString m_shownRequest;      // the request whose answer dialog shows
  QSet<QString> m_answered;    // requests answered (or kept) here
  QString m_keptRequest;       // the request kept for 15 minutes
  QSet<QString> m_receipts;    // receipts published
  QHash<QString, QJsonObject> m_alsoOpen; // who else has the design open, by session
  QSet<QString> m_offlineAsked; // holders whose offline will was asked about
  int m_dropped = 0;           // malformed lock refs (the core's count)
  int m_liveDropped = 0;       // dropped live messages
  quint64 m_generation = 0;    // a new design or a blocking flow: answers of older tasks are dropped
  int m_takeRetries = 0;
  bool m_dialogOpen = false;   // a question that waits for the user (no second one)

  QPointer<RemoteTask> m_task;
  std::function<void(const QJsonObject&)> m_taskDone;
  quint64 m_taskGeneration = 0;
  std::deque<std::function<void()>> m_queue;
  int m_blocking = 0;

  QTimer* m_pollTimer = nullptr;
  QTimer* m_refreshTimer = nullptr;
  QTimer* m_idleTimer = nullptr;
  QTimer* m_keepTimer = nullptr;
  QTimer* m_answerTimer = nullptr;
  QPointer<QMessageBox> m_dialog;
  QPointer<QMessageBox> m_answerDialog;
  QToolBar* m_bar = nullptr;
  LockBanner* m_banner = nullptr;
  QAction* m_releaseAction = nullptr;
  QAction* m_detailsAction = nullptr;
  QString m_loggedState;
  QHash<QString, std::pair<bool, QString>> m_probes; // by remote address
};

} // namespace mitcad
