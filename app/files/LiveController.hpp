// SPDX-License-Identifier: MIT
#pragma once

// Live updates in the application (mitcad#89, sections 9 and 12): the MQTT
// connections of the core's LiveHub (core/ffi/src/live.rs, commands.md
// "Live updates") for the Cloud projects that name a broker. They only make
// things arrive sooner: edit locks, requests, versions sent and who has a
// design open; git stays what grants a lock and holds the versions.
//
// - One hub for the application. Its commands run on the UI thread; its
//   checked events are read on a thread of the controller's own and handled
//   here, on the UI thread. One connection per broker, user and prefix is
//   shared by the open projects on it (the hub's); a project subscribes when
//   it opens (or its settings or Preferences let it) and unsubscribes when
//   it closes. Quitting closes the hub, waiting a moment so that the others
//   see a clean leave rather than the connection's will.
// - What a project uses: Project Settings' live updates for everyone, or
//   this computer's own (`project_settings`' `live_updates`), only while
//   Preferences > Cloud allows live updates. Its topics are under its id,
//   the repository's first commit (`project_id`).
// - Trust: a broker is used only after the user said Connect for its
//   address (the trust question, LiveBrokers.hpp); Not Now is kept too, and
//   nothing connects to that address until Preferences forgets the answer.
// - Credentials: the keychain's for the broker's host and port
//   (Keychain.hpp), read once a session; none: anonymous. A broker that
//   refuses shows the sign-in notice under the toolbar (never blocking an
//   open); signing in keeps them in the keychain or, without one, for the
//   session. A password goes only over TLS (the core refuses mqtt:// with
//   one).
// - The indicator's text (`liveText`: "Live", "Live offline (polling)"),
//   the remote's checks (versionAnnounced: another session sent a version,
//   or the connection came back), the lock controller's events (LockEvents,
//   LiveLink.hpp) and what it publishes (LiveLink, implemented here).
//
// Logged as "Live: ..." and "Live event: ..." (app/COMMANDS.md).

#include <functional>
#include <map>

#include <QHash>
#include <QJsonObject>
#include <QObject>
#include <QString>

#include "Keychain.hpp"
#include "LiveLink.hpp"
#include "Projects.hpp"
#include "mitcad_bridge/lib.h"
#include "rust/cxx.h"

class QMainWindow;
class QThread;
class QToolBar;
class QWidget;

namespace mitcad {

class LiveNotice;

class LiveController : public QObject, public LiveLink {
  Q_OBJECT

public:
  struct Host {
    // This run's session (autosave's id, a UUID).
    std::function<QString()> session;
    // The status bar's message, red when `error`.
    std::function<void(const QString& message, bool error)> status;
  };

  LiveController(QMainWindow& window, Host host, QObject* parent = nullptr);
  ~LiveController() override;

  // The notice under the toolbar; after the window's other bars.
  void startUp();
  // Before the window closes: every project unsubscribed, the hub closed
  // (waiting up to two seconds for clean leaves), its thread ended.
  void shutDown();

  // The window's current project (a Cloud project with versions is
  // followed; any other closes the one before).
  void setProject(const ProjectState& project);
  // The open design of the current project ("" none): who has it open is
  // published from here while no lock controller does it (LockEvents).
  void setDesign(const QString& path, bool readOnly);
  // Project Settings changed the project's live updates (for everyone or
  // here): connected again as they say.
  void projectSettingsChanged(const QString& root);
  // Preferences changed (live updates allowed or not).
  void preferencesChanged();
  // The project stops syncing (Cloud -> Local): its live updates end.
  void stoppingSync(const QString& root);
  // The lock controller's side (LiveLink.hpp); nullptr: none.
  void setLockEvents(LockEvents* events);
  // Project Settings' Test: connects, subscribes and publishes once under
  // the prefix, on a thread of its own, and shows the steps.
  void testBroker(QWidget* parent, const QString& broker, const QString& prefix);

  // The indicator's live text for the project: "Live", "Live offline
  // (polling)", or "" (no live updates).
  QString liveText(const QString& root) const;
  // The project's live updates for people (the lock's details, section 8):
  // the broker, the user, the connection's state and its dropped messages
  // (the hub's `status`); "" without live updates.
  QString details(const QString& root);

  // LiveLink
  bool liveConnected(const QString& root) const override;
  void publishLock(const QString& root, const QString& path, const QJsonObject& lock) override;
  void publishRequest(const QString& root, const QJsonObject& message) override;
  void publishOpen(const QString& root, const QString& path, const QString& mode) override;
  void publishVersion(const QString& root, const QString& branch, const QString& commit) override;

signals:
  // The project's live state changed (liveText, liveConnected).
  void stateChanged(const QString& root);
  // The remote may have newer versions now: another session sent one, or
  // the connection came back after missing messages.
  void versionAnnounced(const QString& root);

private:
  enum class Step {
    Off,           // no live updates (none set, not allowed, not a Cloud project)
    Declined,      // Not Now for the broker
    AskingTrust,   // the trust question waits or shows
    Credentials,   // reading the keychain
    SignIn,        // the broker refused: the sign-in notice
    Connected,     // subscribed (see `state` for the connection)
    Failed,        // a command failed (the message in `error`)
  };
  struct Open {
    QString mode;
    QString since;
  };
  struct Project {
    QString root;
    QString name;    // the folder's
    QString id;      // the first commit
    QString broker;  // "mqtts://host:port" as the project gives it
    QString prefix;
    QString author;  // the name others see
    Step step = Step::Off;
    QString connection; // the hub's ("c1"), "" none
    QString user;       // signed in as ("" anonymous)
    QString state;      // the connection's: connecting, connected, offline, failed, closed
    QString error;      // the latest failure for people
    QString errorClass;
    bool subscribed = false;  // a `subscribed` event without an error
    bool wasLive = false;     // live at some point (a later subscribe catches up)
    bool liveSignalled = false;
    std::map<QString, Open> open;         // this session's windows (path: mode, since)
    std::map<QString, QJsonObject> locks; // this session's lock summaries (path: lock)
  };
  struct Broker {
    bool read = false;    // the keychain was asked this session
    bool reading = false; // ... and has not answered yet
    BrokerCredentials credentials;
    bool notNow = false; // Not Now on the sign-in notice
  };

  bool allowed() const;
  QWidget* dialogParent() const;
  // A hub command; its answer, or {"error": message} when it failed.
  QJsonObject command(const QJsonObject& json);
  void startEvents();
  void handleEvents(const QJsonObject& batch);
  void handleEvent(const QJsonObject& event);
  // Reads the project's live settings, id and author.
  void readProject(Project& project);
  // Brings the project to where its settings say: off, asking, connected.
  void advance(const QString& root);
  void connectProject(Project& project);
  void subscribeProject(Project& project);
  void unsubscribe(Project& project);
  void close(const QString& root);
  void setStep(Project& project, Step step);
  void publishState(Project& project);
  void publish(Project& project, const QJsonObject& message);
  void askTrust(const QString& root);
  void showSignIn(const QString& root);
  void signIn(const QString& root);
  QString brokerKey(const Project& project) const;
  Project* find(const QString& root);
  const Project* find(const QString& root) const;
  void announce(Project& project, const QString& why);

  QMainWindow& m_window;
  Host m_host;
  rust::Box<LiveHub> m_hub;
  QThread* m_events = nullptr;
  bool m_closed = false;
  std::map<QString, Project> m_projects; // by root
  QHash<QString, Broker> m_brokers;      // by host:port
  QString m_current;                     // the window's current project's root
  QString m_design;                      // its open design (relative), "" none
  bool m_designReadOnly = false;
  LockEvents* m_lockEvents = nullptr;
  bool m_trustShowing = false;
  // The keychain failed this session: credentials are kept for it only.
  bool m_keychainUnavailable = false;
  QString m_keychainError;
  QToolBar* m_bar = nullptr;
  LiveNotice* m_notice = nullptr;
  QString m_noticeRoot;
  QString m_lastPublishedVersion;
};

} // namespace mitcad
