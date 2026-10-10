// SPDX-License-Identifier: MIT
#include "LiveController.hpp"

#include <exception>
#include <utility>

#include <QApplication>
#include <QCoreApplication>
#include <QDateTime>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QLabel>
#include <QMainWindow>
#include <QPointer>
#include <QPushButton>
#include <QThread>
#include <QTimer>
#include <QToolBar>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "LiveBrokers.hpp"
#include "LiveDialogs.hpp"
#include "RemoteTask.hpp"

namespace mitcad {
namespace {

// How long a read of the hub's events waits for one (the thread then reads
// again; `close` ends a waiting read at once).
constexpr unsigned kEventsWaitMs = 500;
// Quitting: how long the hub may take to leave cleanly.
constexpr int kCloseWaitMs = 2000;
// Project Settings' Test: per step.
constexpr int kTestStepMs = 10000;

// A command of the project in `root` on this thread (no network); empty
// when it fails.
QJsonObject projectCommand(const QString& root, const QJsonObject& command) {
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    return parseObject(project->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Live: %1 of %2: %3")
                                .arg(command.value(QStringLiteral("cmd")).toString(), root, errorText(e));
    return {};
  }
}

// Whether a file is in a folder (or below).
bool inFolder(const QString& file, const QString& folder) {
  if (file.isEmpty() || folder.isEmpty()) {
    return false;
  }
  const QString root = QDir::cleanPath(folder) + QLatin1Char('/');
#ifdef _WIN32
  return QDir::cleanPath(file).startsWith(root, Qt::CaseInsensitive);
#else
  return QDir::cleanPath(file).startsWith(root);
#endif
}

QString now() { return QDateTime::currentDateTimeUtc().toString(Qt::ISODate); }

// A certificate authority the tests' broker is signed by (a PEM file),
// trusted besides the system's.
QString testAuthorities() { return qEnvironmentVariable("MITCAD_TEST_LIVE_CA"); }

QString shortId(const QString& id) { return id.left(8); }

// What a lock answer of the core carries for others (a live lock summary;
// lock.json's owner without the email).
QJsonObject lockSummary(const QJsonObject& lock) {
  QJsonObject summary;
  const QJsonObject owner = lock.value(QStringLiteral("owner")).toObject();
  if (!owner.isEmpty()) {
    summary.insert(QStringLiteral("owner"),
                   QJsonObject{{QStringLiteral("name"), owner.value(QStringLiteral("name"))}});
  }
  for (const char* key : {"session", "state", "taken_at", "active_at", "idle_since", "idle_minutes", "poll_seconds",
                          "commit"}) {
    const QJsonValue value = lock.value(QLatin1String(key));
    if (!value.isUndefined() && !value.isNull()) {
      summary.insert(QLatin1String(key), value);
    }
  }
  return summary;
}

} // namespace

LiveController::LiveController(QMainWindow& window, Host host, QObject* parent)
    : QObject(parent), m_window(window), m_host(std::move(host)), m_hub(new_live_hub()) {
  BrokerNotices& notices = BrokerNotices::instance();
  // Preferences' Sign Out: the projects on that broker go on without the
  // credentials (anonymous, or the sign-in notice when it refuses that).
  connect(&notices, &BrokerNotices::signedOut, this, [this](const QString& key) {
    Broker& broker = m_brokers[key];
    broker = Broker();
    broker.read = true;
    BrokerNotices::instance().setUser(key, QString());
    qInfo().noquote() << QStringLiteral("Live: signed out of %1").arg(key);
    for (auto& [root, project] : m_projects) {
      if (brokerKey(project) == key && project.step != Step::Off && project.step != Step::Declined) {
        unsubscribe(project);
        setStep(project, Step::Off);
        advance(root);
      }
    }
  });
  // Preferences' Forget: nothing connects to that address any more until
  // the trust question is answered again (when a project opens).
  connect(&notices, &BrokerNotices::forgotten, this, [this](const QString& address) {
    const QString key = brokers::credentialKey(address);
    m_brokers.remove(key);
    BrokerNotices::instance().setUser(key, QString());
    for (auto& [root, project] : m_projects) {
      if (project.broker == address) {
        qInfo().noquote() << QStringLiteral("Live: %1 forgotten: %2 disconnected").arg(address, project.name);
        unsubscribe(project);
        setStep(project, Step::Declined);
      }
    }
  });
}

LiveController::~LiveController() {
  shutDown();
  delete m_events;
}

void LiveController::startUp() {
  m_bar = new QToolBar(tr("Live Updates"), &m_window);
  m_bar->setObjectName(QStringLiteral("liveBar"));
  m_bar->setMovable(false);
  m_bar->setFloatable(false);
  m_bar->toggleViewAction()->setVisible(false);
  m_notice = new LiveNotice(m_bar);
  m_bar->addWidget(m_notice);
  m_window.addToolBarBreak(Qt::TopToolBarArea);
  m_window.addToolBar(Qt::TopToolBarArea, m_bar);
  m_bar->hide();
  connect(m_notice->signIn, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Live notice: Sign In");
    m_bar->hide();
    signIn(m_noticeRoot);
  });
  connect(m_notice->notNow, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Live notice: not now");
    m_bar->hide();
    if (const Project* project = find(m_noticeRoot)) {
      // Not again this session; the project polls git meanwhile.
      m_brokers[brokerKey(*project)].notNow = true;
    }
  });
}

void LiveController::shutDown() {
  if (m_closed) {
    return;
  }
  m_closed = true;
  if (m_events == nullptr) {
    return; // nothing was connected
  }
  const QJsonObject answer =
      command({{QStringLiteral("cmd"), QStringLiteral("close")}, {QStringLiteral("wait_ms"), kCloseWaitMs}});
  qInfo().noquote() << QStringLiteral("Live: closed%1")
                           .arg(answer.value(QStringLiteral("finished")).toBool() ? QStringLiteral(", every connection "
                                                                                                   "left cleanly")
                                                                                 : QString());
  m_events->wait();
}

// ---------------------------------------------------------------------------
// The hub

bool LiveController::allowed() const { return RemoteSettings::load().allowLive; }

QWidget* LiveController::dialogParent() const {
  QWidget* modal = QApplication::activeModalWidget();
  return modal != nullptr ? modal : &m_window;
}

QJsonObject LiveController::command(const QJsonObject& json) {
  try {
    return parseObject(m_hub->command(rustStr(compactJson(json))));
  } catch (const std::exception& e) {
    return {{QStringLiteral("error"), errorText(e)}};
  }
}

void LiveController::startEvents() {
  if (m_events != nullptr || m_closed) {
    return;
  }
  // Who this run is to the others (its presence and messages carry it).
  qInfo().noquote() << QStringLiteral("Live: session %1").arg(m_host.session());
  const LiveHub* hub = &*m_hub;
  m_events = QThread::create([this, hub] {
    for (;;) {
      const rust::String json = hub->events(kEventsWaitMs);
      const QJsonObject batch = parseObject(json);
      const bool closed = batch.value(QStringLiteral("closed")).toBool();
      if (!batch.value(QStringLiteral("events")).toArray().isEmpty() ||
          batch.value(QStringLiteral("lost")).toInt() > 0) {
        QMetaObject::invokeMethod(this, [this, batch] { handleEvents(batch); }, Qt::QueuedConnection);
      }
      if (closed) {
        return;
      }
    }
  });
  m_events->setObjectName(QStringLiteral("Live events"));
  m_events->start();
}

void LiveController::handleEvents(const QJsonObject& batch) {
  if (m_closed) {
    return;
  }
  if (const int lost = batch.value(QStringLiteral("lost")).toInt(); lost > 0) {
    qWarning().noquote() << QStringLiteral("Live: %1 event(s) lost").arg(lost);
  }
  for (const QJsonValue& value : batch.value(QStringLiteral("events")).toArray()) {
    handleEvent(value.toObject());
  }
}

void LiveController::handleEvent(const QJsonObject& event) {
  const QString type = event.value(QStringLiteral("type")).toString();
  const QString connection = event.value(QStringLiteral("connection")).toString();
  const QString projectId = event.value(QStringLiteral("project")).toString();
  const bool own = event.value(QStringLiteral("own")).toBool();
  const bool retained = event.value(QStringLiteral("retained")).toBool();
  // The projects the event is of: by id, or every one on the connection
  // (its state, presence, drops).
  QStringList roots;
  for (const auto& [root, project] : m_projects) {
    if (project.connection == connection && (projectId.isEmpty() || project.id == projectId)) {
      roots << root;
    }
  }
  if (type == QLatin1String("state")) {
    const QString state = event.value(QStringLiteral("state")).toString();
    const QJsonObject error = event.value(QStringLiteral("error")).toObject();
    const QString cls = error.value(QStringLiteral("class")).toString();
    QString line = QStringLiteral("Live: connection %1 (%2) %3")
                       .arg(connection, event.value(QStringLiteral("broker")).toString(), state);
    if (state == QLatin1String("offline")) {
      line += QStringLiteral(", again in %1 ms").arg(event.value(QStringLiteral("retry_in_ms")).toInt());
    }
    if (!error.isEmpty()) {
      line += QStringLiteral(" (%1: %2)").arg(cls, error.value(QStringLiteral("message")).toString());
    }
    qInfo().noquote() << line;
    for (const QString& root : roots) {
      Project& project = m_projects[root];
      project.state = state;
      if (!error.isEmpty()) {
        project.errorClass = cls;
        project.error = error.value(QStringLiteral("message")).toString();
      } else if (state == QLatin1String("connected")) {
        project.errorClass.clear();
        project.error.clear();
      }
      if (state != QLatin1String("connected")) {
        project.subscribed = false;
      }
      if (state == QLatin1String("failed") && cls == QLatin1String("auth_failed")) {
        setStep(project, Step::SignIn);
        showSignIn(root);
      }
      publishState(project);
    }
    return;
  }
  for (const QString& root : roots) {
    Project& project = m_projects[root];
    if (type == QLatin1String("subscribed")) {
      const QJsonObject error = event.value(QStringLiteral("error")).toObject();
      if (error.isEmpty()) {
        project.subscribed = true;
        qInfo().noquote() << QStringLiteral("Live: %1 subscribed").arg(project.name);
        // Messages may have been missed while it was not: the remote is
        // checked as when a version was announced.
        if (project.wasLive) {
          announce(project, QStringLiteral("connected again"));
        }
        project.wasLive = true;
      } else {
        project.subscribed = false;
        project.errorClass = error.value(QStringLiteral("class")).toString();
        project.error = error.value(QStringLiteral("message")).toString();
        qInfo().noquote() << QStringLiteral("Live: %1 subscription refused (%2: %3)")
                                 .arg(project.name, project.errorClass, project.error);
      }
      publishState(project);
    } else if (!own) {
      // For people and the tests; names are cleaned by the core.
      QString line;
      if (type == QLatin1String("version")) {
        line = QStringLiteral("version %1 on %2 by %3")
                   .arg(event.value(QStringLiteral("commit")).toString().left(7),
                        event.value(QStringLiteral("branch")).toString(),
                        event.value(QStringLiteral("name")).toString());
      } else if (type == QLatin1String("open")) {
        const QJsonObject entry = event.value(QStringLiteral("entry")).toObject();
        line = entry.isEmpty() ? QStringLiteral("open %1 closed by session %2%3")
                                     .arg(shortId(event.value(QStringLiteral("file")).toString()),
                                          shortId(event.value(QStringLiteral("session")).toString()),
                                          event.value(QStringLiteral("cleared")).toBool()
                                              ? QStringLiteral(" (cleared: offline)")
                                              : QString())
                               : QStringLiteral("open %1 by %2 (%3) in session %4")
                                     .arg(shortId(event.value(QStringLiteral("file")).toString()),
                                          entry.value(QStringLiteral("name")).toString(),
                                          entry.value(QStringLiteral("mode")).toString(),
                                          shortId(event.value(QStringLiteral("session")).toString()));
      } else if (type == QLatin1String("lock")) {
        const QJsonObject lock = event.value(QStringLiteral("lock")).toObject();
        line = lock.isEmpty() ? QStringLiteral("lock %1 released")
                                    .arg(shortId(event.value(QStringLiteral("file")).toString()))
                              : QStringLiteral("lock %1 held by %2 (%3)")
                                    .arg(shortId(event.value(QStringLiteral("file")).toString()),
                                         lock.value(QStringLiteral("owner")).toObject().value(QStringLiteral("name")).toString(),
                                         lock.value(QStringLiteral("state")).toString());
      } else if (type == QLatin1String("request")) {
        line = QStringLiteral("request %1 %2 from %3")
                   .arg(event.value(QStringLiteral("kind")).toString(),
                        shortId(event.value(QStringLiteral("file")).toString()),
                        event.value(QStringLiteral("name")).toString());
      } else if (type == QLatin1String("session")) {
        line = QStringLiteral("session %1 %2")
                   .arg(shortId(event.value(QStringLiteral("session")).toString()),
                        event.value(QStringLiteral("state")).toString());
      } else if (type == QLatin1String("dropped")) {
        line = QStringLiteral("dropped %1 (%2 so far)")
                   .arg(event.value(QStringLiteral("reason")).toString())
                   .arg(event.value(QStringLiteral("count")).toInt());
      } else {
        line = type;
      }
      qInfo().noquote() << QStringLiteral("Live event: %1: %2%3")
                               .arg(project.name, line, retained ? QStringLiteral(" (retained)") : QString());
      if (type == QLatin1String("version") && !retained) {
        announce(project, QStringLiteral("version %1").arg(event.value(QStringLiteral("commit")).toString().left(7)));
      }
    }
    if (m_lockEvents != nullptr) {
      m_lockEvents->liveEvent(root, event);
    }
  }
}

void LiveController::announce(Project& project, const QString& why) {
  qInfo().noquote() << QStringLiteral("Live: %1: the remote is checked (%2)").arg(project.name, why);
  emit versionAnnounced(project.root);
}

// ---------------------------------------------------------------------------
// Projects

LiveController::Project* LiveController::find(const QString& root) {
  const auto found = m_projects.find(root);
  return found == m_projects.end() ? nullptr : &found->second;
}

const LiveController::Project* LiveController::find(const QString& root) const {
  const auto found = m_projects.find(root);
  return found == m_projects.end() ? nullptr : &found->second;
}

QString LiveController::brokerKey(const Project& project) const { return brokers::credentialKey(project.broker); }

void LiveController::setProject(const ProjectState& state) {
  const QString root = state.kind == ProjectState::Kind::Cloud ? QDir::cleanPath(state.root) : QString();
  if (root == m_current || m_closed) {
    return;
  }
  if (!m_current.isEmpty()) {
    close(m_current);
  }
  m_current = root;
  m_design.clear();
  if (root.isEmpty()) {
    return;
  }
  Project project;
  project.root = root;
  project.name = state.name();
  m_projects[root] = project;
  readProject(m_projects[root]);
  advance(root);
}

void LiveController::readProject(Project& project) {
  const QJsonObject settings = projectCommand(project.root, {{QStringLiteral("cmd"), QStringLiteral("project_settings")}});
  const QJsonObject live = settings.value(QStringLiteral("live_updates")).toObject();
  project.broker = live.value(QStringLiteral("broker")).toString();
  project.prefix = live.value(QStringLiteral("prefix")).toString(QStringLiteral("mitcad"));
  project.id = projectCommand(project.root, {{QStringLiteral("cmd"), QStringLiteral("project_id")}})
                   .value(QStringLiteral("project"))
                   .toString();
  // The name others see: the project's author, as for versions.
  const VersionSettings defaults = VersionSettings::load();
  QJsonObject identity{{QStringLiteral("cmd"), QStringLiteral("identity")}};
  if (defaults.complete()) {
    identity.insert(QStringLiteral("fallback_author"), defaults.author());
  }
  project.author = projectCommand(project.root, identity).value(QStringLiteral("name")).toString();
  if (project.author.isEmpty()) {
    project.author = tr("Someone");
  }
}

void LiveController::advance(const QString& root) {
  Project* project = find(root);
  if (project == nullptr || m_closed) {
    return;
  }
  QString why;
  if (project->broker.isEmpty()) {
    why = QStringLiteral("no broker");
  } else if (!allowed()) {
    why = QStringLiteral("not allowed in Preferences");
  } else if (project->id.isEmpty()) {
    why = QStringLiteral("no versions");
  }
  if (!why.isEmpty()) {
    if (project->step != Step::Off || !project->connection.isEmpty()) {
      unsubscribe(*project);
      setStep(*project, Step::Off);
    }
    qInfo().noquote() << QStringLiteral("Live: %1: off (%2)").arg(project->name, why);
    return;
  }
  const std::optional<bool> trusted = brokers::trust(project->broker);
  if (!trusted) {
    if (project->step == Step::Declined) {
      return; // forgotten in Preferences meanwhile: asked when it opens again
    }
    setStep(*project, Step::AskingTrust);
    askTrust(root);
    return;
  }
  if (!*trusted) {
    if (project->step != Step::Declined) {
      qInfo().noquote() << QStringLiteral("Live: %1: %2 not trusted (Not Now): not connected")
                               .arg(project->name, project->broker);
      unsubscribe(*project);
      setStep(*project, Step::Declined);
    }
    return;
  }
  connectProject(*project);
}

void LiveController::askTrust(const QString& root) {
  // After what opens the project has finished, never inside it.
  QTimer::singleShot(0, this, [this, root] {
    Project* project = find(root);
    if (project == nullptr || project->step != Step::AskingTrust || m_trustShowing || m_closed) {
      return;
    }
    const QString address = project->broker;
    if (!brokers::trust(address)) {
      m_trustShowing = true;
      const bool trusted = askTrustBroker(dialogParent(), project->name, address);
      m_trustShowing = false;
      brokers::setTrust(address, trusted);
    }
    // Every project waiting for this broker's answer goes on.
    QStringList waiting;
    for (const auto& [other, state] : m_projects) {
      if (state.step == Step::AskingTrust) {
        waiting << other;
      }
    }
    for (const QString& other : waiting) {
      if (Project* next = find(other); next != nullptr && next->broker == address) {
        setStep(*next, Step::Off);
      }
      advance(other);
    }
  });
}

void LiveController::connectProject(Project& project) {
  if ((project.step == Step::Connected && !project.connection.isEmpty()) || project.step == Step::Credentials ||
      project.step == Step::SignIn) {
    return; // connected, or waiting for the keychain or the user
  }
  const QString key = brokerKey(project);
  Broker& broker = m_brokers[key];
  if (brokers::isPlain(project.broker) || broker.read) {
    subscribeProject(project);
    return;
  }
  setStep(project, Step::Credentials);
  if (broker.reading) {
    return;
  }
  broker.reading = true;
  keychain::read(key, this,
                 [this, key](keychain::Read result, const BrokerCredentials& credentials, const QString& error) {
                   Broker& done = m_brokers[key];
                   done.reading = false;
                   done.read = true;
                   if (result == keychain::Read::Found) {
                     done.credentials = credentials;
                     BrokerNotices::instance().setUser(key, credentials.user);
                   } else if (result == keychain::Read::Unavailable) {
                     m_keychainUnavailable = true;
                     m_keychainError = error;
                   }
                   for (auto& [root, waiting] : m_projects) {
                     if (waiting.step == Step::Credentials && brokerKey(waiting) == key) {
                       subscribeProject(waiting);
                     }
                   }
                 });
}

void LiveController::subscribeProject(Project& project) {
  const QString key = brokerKey(project);
  const Broker& broker = m_brokers.value(key);
  QJsonObject json{{QStringLiteral("cmd"), QStringLiteral("subscribe")},
                   {QStringLiteral("broker"), project.broker},
                   {QStringLiteral("prefix"), project.prefix},
                   {QStringLiteral("session"), m_host.session()},
                   {QStringLiteral("project"), project.id}};
  // A password goes only over TLS, and only to its own host and port.
  QString user;
  if (!brokers::isPlain(project.broker) && !broker.credentials.isEmpty()) {
    user = broker.credentials.user;
    json.insert(QStringLiteral("user"), user);
    json.insert(QStringLiteral("password"), broker.credentials.password);
  }
  if (!testAuthorities().isEmpty()) {
    json.insert(QStringLiteral("ca"), testAuthorities());
  }
  const QJsonObject answer = command(json);
  if (answer.value(QStringLiteral("error")).isString()) {
    project.error = answer.value(QStringLiteral("error")).toString();
    qWarning().noquote() << QStringLiteral("Live: %1: cannot connect to %2: %3")
                                .arg(project.name, project.broker, project.error);
    setStep(project, Step::Failed);
    return;
  }
  const QString connection = answer.value(QStringLiteral("connection")).toString();
  if (!project.connection.isEmpty() && project.connection != connection) {
    // Another user's connection now (signed in, or out).
    unsubscribe(project);
  }
  project.connection = connection;
  project.user = user;
  project.state = answer.value(QStringLiteral("state")).toString();
  project.subscribed = false;
  qInfo().noquote() << QStringLiteral("Live: %1 subscribes through %2, prefix %3, as %4 (%5, %6)")
                           .arg(project.name, project.broker, project.prefix,
                                user.isEmpty() ? QStringLiteral("anonymous") : user, connection, project.state);
  startEvents();
  // What this session keeps published: its windows and lock summaries
  // (the hub publishes them again after each reconnect).
  for (const auto& [path, open] : project.open) {
    publish(project, {{QStringLiteral("type"), QStringLiteral("open")},
                      {QStringLiteral("path"), path},
                      {QStringLiteral("name"), project.author},
                      {QStringLiteral("mode"), open.mode},
                      {QStringLiteral("since"), open.since}});
  }
  for (const auto& [path, lock] : project.locks) {
    publish(project, {{QStringLiteral("type"), QStringLiteral("lock")},
                      {QStringLiteral("path"), path},
                      {QStringLiteral("lock"), lock}});
  }
  setStep(project, Step::Connected);
}

void LiveController::unsubscribe(Project& project) {
  if (project.connection.isEmpty()) {
    return;
  }
  const QJsonObject answer = command({{QStringLiteral("cmd"), QStringLiteral("unsubscribe")},
                                      {QStringLiteral("connection"), project.connection},
                                      {QStringLiteral("project"), project.id}});
  qInfo().noquote() << QStringLiteral("Live: %1 unsubscribed from %2%3")
                           .arg(project.name, project.connection,
                                answer.value(QStringLiteral("closed")).toBool() ? QStringLiteral(", which closed")
                                                                                : QString());
  project.connection.clear();
  project.subscribed = false;
  project.state.clear();
}

void LiveController::close(const QString& root) {
  Project* project = find(root);
  if (project == nullptr) {
    return;
  }
  const bool wasConnected = liveConnected(root);
  unsubscribe(*project);
  m_projects.erase(root);
  if (m_noticeRoot == root) {
    m_noticeRoot.clear();
    if (m_bar != nullptr) {
      m_bar->hide();
    }
  }
  if (wasConnected && m_lockEvents != nullptr) {
    m_lockEvents->liveStateChanged(root, false);
  }
  emit stateChanged(root);
}

void LiveController::setStep(Project& project, Step step) {
  if (project.step == step) {
    publishState(project);
    return;
  }
  project.step = step;
  if (step != Step::SignIn && m_noticeRoot == project.root && m_bar != nullptr && m_bar->isVisible()) {
    m_bar->hide();
  }
  publishState(project);
}

void LiveController::publishState(Project& project) {
  const bool connected = liveConnected(project.root);
  if (connected != project.liveSignalled) {
    project.liveSignalled = connected;
    if (m_lockEvents != nullptr) {
      m_lockEvents->liveStateChanged(project.root, connected);
    }
  }
  emit stateChanged(project.root);
}

QString LiveController::liveText(const QString& root) const {
  const Project* project = find(root);
  if (project == nullptr) {
    return {};
  }
  switch (project->step) {
  case Step::Off:
  case Step::Declined:
  case Step::AskingTrust:
  case Step::Credentials:
    return {};
  case Step::SignIn:
  case Step::Failed:
    return tr("Live offline (polling)");
  case Step::Connected:
    if (liveConnected(root)) {
      return tr("Live");
    }
    // Connecting the first time: nothing yet.
    return project->wasLive || (project->state != QLatin1String("connecting") && !project->state.isEmpty())
               ? tr("Live offline (polling)")
               : QString();
  }
  return {};
}

QString LiveController::details(const QString& root) {
  const Project* project = find(QDir::cleanPath(root));
  if (project == nullptr || project->step == Step::Off) {
    return {};
  }
  QString text;
  switch (project->step) {
  case Step::Declined:
    return tr("Live updates through %1: not connected (Not Now)").arg(project->broker);
  case Step::AskingTrust:
    return tr("Live updates through %1: waiting for your answer").arg(project->broker);
  case Step::Credentials:
    return tr("Live updates through %1: reading the keychain").arg(project->broker);
  case Step::SignIn:
    return tr("Live updates through %1: sign-in needed (%2)").arg(project->broker, project->error);
  case Step::Failed:
    return tr("Live updates through %1: %2").arg(project->broker, project->error);
  case Step::Off:
  case Step::Connected:
    break;
  }
  const QJsonObject status = command({{QStringLiteral("cmd"), QStringLiteral("status")}});
  for (const QJsonValue& value : status.value(QStringLiteral("connections")).toArray()) {
    const QJsonObject connection = value.toObject();
    if (connection.value(QStringLiteral("connection")).toString() != project->connection) {
      continue;
    }
    const QString user = connection.value(QStringLiteral("user")).toString();
    text = tr("Live updates through %1%2: %3")
               .arg(project->broker,
                    user.isEmpty() ? QString() : tr(" as %1").arg(user),
                    connection.value(QStringLiteral("state")).toString());
    const QString error = connection.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
    if (!error.isEmpty()) {
      text += QStringLiteral(" (%1)").arg(error);
    }
    if (const int dropped = connection.value(QStringLiteral("dropped")).toInt(); dropped > 0) {
      text += tr(", %n malformed message(s) dropped", nullptr, dropped);
    }
  }
  return text;
}

bool LiveController::liveConnected(const QString& root) const {
  const Project* project = find(root);
  return project != nullptr && project->step == Step::Connected && project->subscribed &&
         project->state == QLatin1String("connected");
}

// ---------------------------------------------------------------------------
// Signing in

void LiveController::showSignIn(const QString& root) {
  const Project* project = find(root);
  if (project == nullptr || m_bar == nullptr) {
    return;
  }
  if (m_brokers.value(brokerKey(*project)).notNow) {
    qInfo().noquote() << QStringLiteral("Live: %1 refused the sign-in; not asked again this session")
                             .arg(project->broker);
    return;
  }
  if (root != m_current) {
    return; // the notice is the current project's
  }
  m_noticeRoot = root;
  m_notice->present(tr("Live updates need you to sign in to %1").arg(brokers::hostOf(project->broker)));
  m_bar->show();
}

void LiveController::signIn(const QString& root) {
  Project* project = find(root);
  if (project == nullptr) {
    return;
  }
  const QString key = brokerKey(*project);
  const QString address = project->broker;
  const Broker broker = m_brokers.value(key);
  const QString reason =
      project->error.isEmpty()
          ? QString()
          : tr("%1 refused the connection: %2").arg(brokers::hostOf(address), project->error);
  const QString session = m_host.session();
  const QString prefix = project->prefix;
  const std::optional<BrokerSignIn> chosen = askBrokerSignIn(
      dialogParent(), address, broker.credentials.user.isEmpty() ? project->user : broker.credentials.user,
      !m_keychainUnavailable, reason, [this, address, prefix, session](const BrokerSignIn& answer) -> QString {
        if (!brokers::isPlain(address)) {
          return {};
        }
        // The core refuses a password over plain text before anything is
        // sent: its message says why.
        const QJsonObject refused = command({{QStringLiteral("cmd"), QStringLiteral("connect")},
                                             {QStringLiteral("broker"), address},
                                             {QStringLiteral("prefix"), prefix},
                                             {QStringLiteral("session"), session},
                                             {QStringLiteral("user"), answer.credentials.user},
                                             {QStringLiteral("password"), answer.credentials.password}});
        return refused.value(QStringLiteral("error")).toString();
      });
  project = find(root);
  if (!chosen || project == nullptr) {
    return;
  }
  Broker& kept = m_brokers[key];
  kept.read = true;
  kept.notNow = false;
  kept.credentials = chosen->credentials;
  BrokerNotices::instance().setUser(key, chosen->credentials.user);
  if (chosen->remember) {
    keychain::write(key, chosen->credentials, this, [this, key](const QString& error) {
      if (!error.isEmpty()) {
        m_keychainUnavailable = true;
        m_keychainError = error;
        if (m_host.status) {
          m_host.status(tr("The keychain did not take the password for %1 (%2): it is kept until Mitcad closes.")
                            .arg(key, error),
                        true);
        }
      }
    });
  }
  // Every project on that broker signs in again.
  for (auto& [other, state] : m_projects) {
    if (brokerKey(state) == key && state.step != Step::Off && state.step != Step::Declined &&
        state.step != Step::AskingTrust) {
      subscribeProject(state);
    }
  }
}

// ---------------------------------------------------------------------------
// Changes

void LiveController::setDesign(const QString& path, bool readOnly) {
  Project* project = find(m_current);
  const QString relative =
      project != nullptr && inFolder(path, project->root) ? QDir(project->root).relativeFilePath(path) : QString();
  if (relative == m_design) {
    return;
  }
  const QString before = m_design;
  m_design = relative;
  m_designReadOnly = readOnly;
  // Who has a design open is the lock controller's to publish when there is
  // one (it knows the edit lock); else the window's own is published here.
  if (project == nullptr || m_lockEvents != nullptr) {
    return;
  }
  if (!before.isEmpty()) {
    publishOpen(project->root, before, QString());
  }
  if (!relative.isEmpty()) {
    publishOpen(project->root, relative, readOnly ? QStringLiteral("read-only") : QStringLiteral("editing"));
  }
}

void LiveController::projectSettingsChanged(const QString& root) {
  Project* project = find(QDir::cleanPath(root));
  if (project == nullptr) {
    return;
  }
  const QString broker = project->broker;
  const QString prefix = project->prefix;
  const QString id = project->id;
  readProject(*project);
  if (project->broker != broker || project->prefix != prefix || project->id != id) {
    qInfo().noquote() << QStringLiteral("Live: %1: live updates %2")
                             .arg(project->name, project->broker.isEmpty()
                                                     ? QStringLiteral("none")
                                                     : QStringLiteral("through %1, prefix %2")
                                                           .arg(project->broker, project->prefix));
    unsubscribe(*project);
    project->wasLive = false;
    setStep(*project, Step::Off);
  }
  advance(project->root);
}

void LiveController::preferencesChanged() {
  QStringList roots;
  for (const auto& [root, project] : m_projects) {
    roots << root;
  }
  for (const QString& root : roots) {
    advance(root);
  }
}

void LiveController::stoppingSync(const QString& root) {
  const QString clean = QDir::cleanPath(root);
  if (clean == m_current) {
    close(clean);
    m_current.clear();
    m_design.clear();
  }
}

void LiveController::setLockEvents(LockEvents* events) { m_lockEvents = events; }

// ---------------------------------------------------------------------------
// LiveLink

void LiveController::publish(Project& project, const QJsonObject& message) {
  if (project.connection.isEmpty()) {
    return; // kept (retained state) or dropped (the rest) until connected
  }
  const QJsonObject answer = command({{QStringLiteral("cmd"), QStringLiteral("publish")},
                                      {QStringLiteral("connection"), project.connection},
                                      {QStringLiteral("project"), project.id},
                                      {QStringLiteral("message"), message}});
  const QString type = message.value(QStringLiteral("type")).toString();
  if (answer.value(QStringLiteral("error")).isString()) {
    qWarning().noquote() << QStringLiteral("Live: %1: publishing %2 failed: %3")
                                .arg(project.name, type, answer.value(QStringLiteral("error")).toString());
    return;
  }
  qInfo().noquote() << QStringLiteral("Live: %1 published %2 %3")
                           .arg(project.name, type,
                                type == QLatin1String("version")
                                    ? message.value(QStringLiteral("commit")).toString().left(7)
                                    : message.value(QStringLiteral("path")).toString());
}

void LiveController::publishLock(const QString& root, const QString& path, const QJsonObject& lock) {
  Project* project = find(QDir::cleanPath(root));
  if (project == nullptr) {
    return;
  }
  if (lock.isEmpty()) {
    if (project->locks.erase(path) == 0 && project->connection.isEmpty()) {
      return;
    }
    publish(*project, {{QStringLiteral("type"), QStringLiteral("lock")},
                       {QStringLiteral("path"), path},
                       {QStringLiteral("lock"), QJsonValue()}});
    return;
  }
  const QJsonObject summary = lockSummary(lock);
  project->locks[path] = summary;
  publish(*project, {{QStringLiteral("type"), QStringLiteral("lock")},
                     {QStringLiteral("path"), path},
                     {QStringLiteral("lock"), summary}});
}

void LiveController::publishRequest(const QString& root, const QJsonObject& message) {
  Project* project = find(QDir::cleanPath(root));
  if (project == nullptr) {
    return;
  }
  QJsonObject sent = message;
  const QString type = message.value(QStringLiteral("type")).toString();
  if (type != QLatin1String("withdrawn") && !sent.contains(QStringLiteral("name"))) {
    sent.insert(QStringLiteral("name"), project->author);
  }
  publish(*project, sent);
}

void LiveController::publishOpen(const QString& root, const QString& path, const QString& mode) {
  Project* project = find(QDir::cleanPath(root));
  if (project == nullptr || path.isEmpty()) {
    return;
  }
  if (mode.isEmpty()) {
    if (project->open.erase(path) > 0) {
      publish(*project, {{QStringLiteral("type"), QStringLiteral("closed")}, {QStringLiteral("path"), path}});
    }
    return;
  }
  Open& entry = project->open[path];
  if (entry.mode == mode) {
    return;
  }
  entry.mode = mode;
  if (entry.since.isEmpty()) {
    entry.since = now();
  }
  publish(*project, {{QStringLiteral("type"), QStringLiteral("open")},
                     {QStringLiteral("path"), path},
                     {QStringLiteral("name"), project->author},
                     {QStringLiteral("mode"), mode},
                     {QStringLiteral("since"), entry.since}});
}

void LiveController::publishVersion(const QString& root, const QString& branch, const QString& commit) {
  Project* project = find(QDir::cleanPath(root));
  if (project == nullptr || commit.isEmpty()) {
    return;
  }
  // The lock controller and the remote's pushes may both tell of it.
  const QString key = project->root + QLatin1Char('|') + commit;
  if (key == m_lastPublishedVersion) {
    return;
  }
  m_lastPublishedVersion = key;
  publish(*project, {{QStringLiteral("type"), QStringLiteral("version")},
                     {QStringLiteral("branch"), branch},
                     {QStringLiteral("commit"), commit},
                     {QStringLiteral("name"), project->author}});
}

// ---------------------------------------------------------------------------
// Test

void LiveController::testBroker(QWidget* parent, const QString& broker, const QString& prefix) {
  const QString key = brokers::credentialKey(broker);
  const QPointer<QWidget> owner(parent);
  const auto run = [this, owner, broker, prefix](const BrokerCredentials& credentials) {
    QJsonObject json{{QStringLiteral("cmd"), QStringLiteral("test")},
                     {QStringLiteral("broker"), broker},
                     {QStringLiteral("timeout_ms"), kTestStepMs}};
    if (!prefix.isEmpty()) {
      json.insert(QStringLiteral("prefix"), prefix);
    }
    if (!brokers::isPlain(broker) && !credentials.isEmpty()) {
      json.insert(QStringLiteral("user"), credentials.user);
      json.insert(QStringLiteral("password"), credentials.password);
    }
    if (!testAuthorities().isEmpty()) {
      json.insert(QStringLiteral("ca"), testAuthorities());
    }
    const QByteArray text = compactJson(json);
    const LiveHub* hub = &*m_hub;
    // The test blocks up to its timeout per step: on a thread of its own,
    // which may outlive the dialog.
    RemoteTask* task = RemoteTask::custom(
        QStringLiteral("live_test"),
        [hub, text](const SyncControl&) -> QJsonObject {
          try {
            return parseObject(hub->command(rustStr(text)));
          } catch (const std::exception& e) {
            return {{QStringLiteral("ok"), false},
                    {QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("invalid")},
                                                          {QStringLiteral("message"), errorText(e)}}}};
          }
        },
        this);
    QDialog* dialog = new QDialog(owner != nullptr ? owner.data() : dialogParent());
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    dialog->setWindowTitle(tr("Test Live Updates"));
    auto* layout = new QVBoxLayout(dialog);
    auto* label = new QLabel(tr("Testing live updates through %1...").arg(broker));
    label->setTextFormat(Qt::PlainText);
    label->setWordWrap(true);
    label->setMinimumWidth(360);
    layout->addWidget(label);
    auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
    layout->addWidget(buttons);
    connect(buttons, &QDialogButtonBox::rejected, dialog, &QDialog::reject);
    const QPointer<QLabel> shown(label);
    connect(task, &RemoteTask::finished, this, [task, shown, broker] {
      const QJsonObject answer = task->answer();
      task->deleteLater();
      QStringList steps;
      for (const QJsonValue& value : answer.value(QStringLiteral("steps")).toArray()) {
        const QJsonObject step = value.toObject();
        steps << QStringLiteral("%1 %2").arg(step.value(QStringLiteral("step")).toString(),
                                             step.value(QStringLiteral("ok")).toBool() ? QStringLiteral("ok")
                                                                                       : QStringLiteral("failed"));
      }
      const QJsonObject error = answer.value(QStringLiteral("error")).toObject();
      qInfo().noquote() << QStringLiteral("Live test: %1: %2; steps %3")
                               .arg(broker,
                                    answer.value(QStringLiteral("ok")).toBool()
                                        ? QStringLiteral("ok")
                                        : QStringLiteral("failed (%1: %2)")
                                              .arg(error.value(QStringLiteral("class")).toString(),
                                                   error.value(QStringLiteral("message")).toString()),
                                    steps.join(QStringLiteral(", ")));
      if (shown != nullptr) {
        shown->setText(describeLiveTest(answer));
      }
    });
    qInfo().noquote() << QStringLiteral("Live test dialog: %1, prefix %2%3")
                             .arg(broker, prefix.isEmpty() ? QStringLiteral("mitcad") : prefix,
                                  credentials.isEmpty() ? QString() : QStringLiteral(", as %1").arg(credentials.user));
    task->start();
    prepareModal(dialog);
    dialog->open();
  };
  if (brokers::isPlain(broker) || key.isEmpty() || m_brokers.value(key).read) {
    run(m_brokers.value(key).credentials);
    return;
  }
  keychain::read(key, this, [this, key, run](keychain::Read result, const BrokerCredentials& credentials,
                                             const QString& error) {
    Broker& entry = m_brokers[key];
    if (!entry.read) {
      entry.read = true;
      if (result == keychain::Read::Found) {
        entry.credentials = credentials;
        BrokerNotices::instance().setUser(key, credentials.user);
      } else if (result == keychain::Read::Unavailable) {
        m_keychainUnavailable = true;
        m_keychainError = error;
      }
    }
    run(entry.credentials);
  });
}

} // namespace mitcad
