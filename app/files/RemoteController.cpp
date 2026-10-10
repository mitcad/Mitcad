// SPDX-License-Identifier: MIT
#include "RemoteController.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDir>
#include <QEventLoop>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QLabel>
#include <QMainWindow>
#include <QMessageBox>
#include <QProgressDialog>
#include <QPushButton>
#include <QTimer>
#include <QToolBar>
#include <QUrl>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "../report/ReportCenter.hpp"
#include "CloudAddress.hpp"
#include "RemoteDialogs.hpp"
#include "RemoteTask.hpp"
#include "VersionDialogs.hpp"

namespace mitcad {
namespace {

// After a failed push the versions wait; the push is tried again after
// this long, then twice as long each time, up to kRetryMaxSeconds
// (MITCAD_TEST_REMOTE_RETRY_SECONDS for tests).
constexpr int kRetrySeconds = 60;
constexpr int kRetryMaxSeconds = 15 * 60;
// The first change after opening checks the remote when the last check is
// this old (MITCAD_TEST_REMOTE_CHANGE_CHECK_SECONDS for tests).
constexpr int kChangeCheckSeconds = 120;

QJsonObject named(const char* name) { return {{QStringLiteral("cmd"), QLatin1String(name)}}; }

QString errorClass(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("class")).toString();
}

QString errorMessage(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("message")).toString();
}

QString errorDetail(const QJsonObject& answer) {
  return answer.value(QStringLiteral("error")).toObject().value(QStringLiteral("detail")).toString();
}

QStringList texts(const QJsonValue& list) {
  QStringList out;
  for (const QJsonValue& value : list.toArray()) {
    out << value.toString();
  }
  return out;
}

// A failure that waiting may cure: the versions are sent again later.
bool passing(const QString& cls) {
  return cls == QLatin1String("network") || cls == QLatin1String("timed_out") || cls == QLatin1String("not_found") ||
         cls == QLatin1String("other");
}

int retrySeconds() {
  bool ok = false;
  const int seconds = qEnvironmentVariableIntValue("MITCAD_TEST_REMOTE_RETRY_SECONDS", &ok);
  return ok && seconds > 0 ? seconds : kRetrySeconds;
}

// Whether a file is in a project's folder (or below).
bool inFolder(const QString& file, const QString& root) {
  if (file.isEmpty() || root.isEmpty()) {
    return false;
  }
  const QString folder = QDir::cleanPath(root) + QLatin1Char('/');
#ifdef _WIN32
  return QDir::cleanPath(file).startsWith(folder, Qt::CaseInsensitive);
#else
  return QDir::cleanPath(file).startsWith(folder);
#endif
}

int changeCheckSeconds() {
  bool ok = false;
  const int seconds = qEnvironmentVariableIntValue("MITCAD_TEST_REMOTE_CHANGE_CHECK_SECONDS", &ok);
  return ok && seconds >= 0 ? seconds : kChangeCheckSeconds;
}

// "14:02" today, else "2026-10-05 14:02".
QString shortTime(const QDateTime& time) {
  const QDateTime local = time.toLocalTime();
  return local.date() == QDate::currentDate() ? local.toString(QStringLiteral("HH:mm"))
                                              : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

QDateTime timeOf(const QJsonValue& attempt) {
  const QJsonObject tried = attempt.toObject();
  return tried.isEmpty() ? QDateTime() : QDateTime::fromSecsSinceEpoch(tried.value(QStringLiteral("time")).toInteger());
}

// A message box with git's own words under Show Details.
void warn(QWidget* parent, const QString& title, const QString& text, const QString& detail) {
  QMessageBox box(QMessageBox::Warning, title, text, QMessageBox::Ok, parent);
  if (!detail.isEmpty()) {
    box.setDetailedText(detail);
  }
  box.exec();
}

} // namespace

// The notice under the toolbar of a newer version of the open file on the
// remote (R4): Sync Now or Dismiss. Its state is logged ("Remote notice:
// ..."), the UI tests read it, and where its buttons are.
class RemoteNotice : public QWidget {
public:
  explicit RemoteNotice(QWidget* parent) : QWidget(parent) {
    setObjectName(QStringLiteral("remoteNotice"));
    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(8, 2, 8, 2);
    auto* icon = new QLabel;
    icon->setPixmap(themeIcon(QStringLiteral("sync")).pixmap(20, 20));
    layout->addWidget(icon);
    m_text = new QLabel;
    m_text->setWordWrap(true);
    m_text->setTextFormat(Qt::PlainText);
    layout->addWidget(m_text, 1);
    sync = new QPushButton(QCoreApplication::translate("RemoteController", "S&ync Now"));
    sync->setAutoDefault(false);
    dismiss = new QPushButton(QCoreApplication::translate("RemoteController", "&Dismiss"));
    dismiss->setAutoDefault(false);
    layout->addWidget(sync);
    layout->addWidget(dismiss);
  }

  void present(const QString& text) {
    m_text->setText(text);
    qInfo().noquote() << QStringLiteral("Remote notice: %1 [Sync Now, Dismiss]").arg(text);
    TestSync::singleShot(50, this, [this] {
      QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
      for (const QPushButton* button : {sync, dismiss}) {
        const QPoint center = button->mapTo(window(), button->rect().center());
        qDebug().noquote() << QStringLiteral("Remote notice %1 at %2,%3")
                                  .arg(button->text().remove(QLatin1Char('&')))
                                  .arg(center.x())
                                  .arg(center.y());
      }
    });
  }

  QString text() const { return m_text->text(); }

  QPushButton* sync = nullptr;
  QPushButton* dismiss = nullptr;

private:
  QLabel* m_text = nullptr;
};

RemoteController::RemoteController(QMainWindow& window, Host host, QObject* parent)
    : QObject(parent), m_window(window), m_host(std::move(host)) {
  m_checkTimer = new QTimer(this);
  connect(m_checkTimer, &QTimer::timeout, this, [this] {
    if (!m_state.name.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Remote check: every %1 min").arg(m_sync.checkMinutes);
      startCheck(Mode::Quiet);
    }
  });
  m_retryTimer = new QTimer(this);
  m_retryTimer->setSingleShot(true);
  connect(m_retryTimer, &QTimer::timeout, this, [this] {
    refreshStatus();
    if (m_state.ahead > 0 && m_sync.sendAtOnce) {
      qInfo().noquote() << QStringLiteral("Remote push: trying again (%1 version(s) waiting)").arg(m_state.ahead);
      push();
    }
  });
}

RemoteController::~RemoteController() { delete m_task; }

void RemoteController::registerCommands(CommandRegistry& registry) {
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      std::function<void()> run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Any;
    d.tab.clear(); // the File menu and the project indicator's menu
    d.run = std::move(run);
    return d;
  };
  CommandDef syncDef = def("file.sync", tr("Sync"), "sync",
                           tr("Brings the project's versions and the remote's together: takes the newer ones, "
                              "sends yours"),
                           [this] { sync(); });
  syncDef.shortcut = QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_Y);
  syncDef.keywords = {QStringLiteral("push"),  QStringLiteral("pull"),   QStringLiteral("fetch"),
                      QStringLiteral("remote"), QStringLiteral("cloud"), QStringLiteral("share"),
                      QStringLiteral("upload"), QStringLiteral("download"), QStringLiteral("git")};
  registry.add(syncDef);
  CommandDef checkDef = def("file.check_remote", tr("Check for Newer Versions"), "sync",
                            tr("Asks the remote whether it has versions this project lacks (git fetch); "
                               "nothing changes here"),
                            [this] { check(); });
  checkDef.keywords = {QStringLiteral("fetch"), QStringLiteral("remote"), QStringLiteral("newer"),
                       QStringLiteral("git"), QStringLiteral("cloud")};
  registry.add(checkDef);
  CommandDef browserDef = def("file.remote_browser", tr("Open in Browser"), "remote-open",
                              tr("The Cloud project's repository on its service's web page"),
                              [this] { openInBrowser(); });
  browserDef.keywords = {QStringLiteral("remote"), QStringLiteral("web"), QStringLiteral("github"),
                         QStringLiteral("cloud"), QStringLiteral("repository")};
  registry.add(browserDef);
}

void RemoteController::startUp() {
  m_bar = new QToolBar(tr("Remote"), &m_window);
  m_bar->setObjectName(QStringLiteral("remoteBar"));
  m_bar->setMovable(false);
  m_bar->setFloatable(false);
  m_bar->toggleViewAction()->setVisible(false);
  m_notice = new RemoteNotice(m_bar);
  m_bar->addWidget(m_notice);
  m_window.addToolBarBreak(Qt::TopToolBarArea);
  m_window.addToolBar(Qt::TopToolBarArea, m_bar);
  m_bar->hide();
  connect(m_notice->sync, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Remote notice: Sync Now");
    m_bar->hide();
    sync();
  });
  connect(m_notice->dismiss, &QPushButton::clicked, this, [this] {
    qInfo().noquote() << QStringLiteral("Remote notice: dismissed");
    m_dismissed = m_noticeId; // until the remote has a newer one still
    m_bar->hide();
  });
  RemoteSettings::load().apply();
  m_sync = projectSync(QString()); // Preferences' until a project opens
  applyTimer();
}

// ---------------------------------------------------------------------------
// The project and the state

std::optional<rust::Box<Project>> RemoteController::project() const {
  const QString root = m_host.projectRoot();
  if (root.isEmpty()) {
    return std::nullopt;
  }
  try {
    return std::optional<rust::Box<Project>>(open_project(rustStr(root.toUtf8())));
  } catch (const std::exception&) {
    return std::nullopt;
  }
}

ProjectIndicator::Remote RemoteController::indicatorState() const {
  ProjectIndicator::Remote remote;
  remote.ahead = m_state.ahead;
  remote.behind = m_state.behind;
  remote.problem = m_state.problem;
  remote.problemMessage = m_state.problemMessage;
  remote.conflict = m_state.conflict;
  remote.lastDone = m_state.lastDone;
  remote.newest = m_state.newest;
  if (m_task != nullptr && m_task->name() != QLatin1String("clone")) {
    remote.busy = m_task->name();
    remote.percent = m_task->progressPercent();
  }
  return remote;
}

QString RemoteController::statusText() const {
  if (m_state.name.isEmpty()) {
    return {};
  }
  const auto when = [](const QDateTime& time) {
    const QDateTime local = time.toLocalTime();
    return local.date() == QDate::currentDate() ? local.toString(QStringLiteral("HH:mm"))
                                                : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
  };
  QStringList parts;
  if (m_task != nullptr) {
    parts << tr("Busy: %1").arg(m_task->name());
  } else if (m_state.conflict) {
    parts << tr("A sync stopped for files changed on both sides");
  } else if (!m_state.problem.isEmpty()) {
    parts << m_state.problemMessage;
  }
  if (m_state.ahead > 0) {
    parts << tr("%n version(s) to send", nullptr, m_state.ahead);
  }
  if (m_state.behind > 0) {
    parts << tr("%n newer version(s) on the remote", nullptr, m_state.behind);
  }
  if (parts.isEmpty()) {
    parts << (m_state.ahead == 0 && m_state.behind == 0 ? tr("Up to date") : tr("Not synced yet"));
  }
  if (m_state.lastDone.isValid()) {
    parts << tr("last sync %1").arg(when(m_state.lastDone));
  }
  return parts.join(QStringLiteral(", "));
}

void RemoteController::projectChanged(bool check) {
  m_dismissed.clear();
  m_state.conflict = false;
  if (m_bar != nullptr) {
    m_bar->hide();
  }
  refreshStatus();
  applyTimer();
  if (check && !m_state.name.isEmpty()) {
    if (m_sync.checkMinutes > 0) {
      startCheck(Mode::Quiet);
    } else if (m_state.ahead > 0 && m_sync.sendAtOnce) {
      push();
    }
  }
}

void RemoteController::retryLater() {
  refreshStatus();
  if (m_state.ahead > 0) {
    scheduleRetry();
  }
}

QJsonObject RemoteController::local(const QString& path, const QJsonObject& command) const {
  try {
    const rust::Box<Project> repo = open_project(rustStr(path.toUtf8()));
    return parseObject(repo->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Remote: %1: %2").arg(command.value(QStringLiteral("cmd")).toString(),
                                                                 errorText(e));
    return {};
  }
}

void RemoteController::refreshStatus() {
  State state;
  QJsonObject info;
  if (const std::optional<rust::Box<Project>> repo = project()) {
    try {
      state.root = parseObject((*repo)->command(rustStr(compactJson(named("info")))))
                       .value(QStringLiteral("root"))
                       .toString();
      info = parseObject((*repo)->command(rustStr(compactJson(named("remote_info")))));
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("Remote: %1").arg(errorText(e));
    }
  }
  if (state.root != m_syncRoot) {
    // Another project: its own sending and checking (Project Settings).
    m_syncRoot = state.root;
    m_sync = projectSync(state.root);
  }
  state.name = info.value(QStringLiteral("name")).toString();
  if (!state.name.isEmpty()) {
    state.url = info.value(QStringLiteral("url")).toString();
    state.upstream = info.value(QStringLiteral("upstream")).toString();
    const QJsonValue ahead = info.value(QStringLiteral("ahead"));
    const QJsonValue behind = info.value(QStringLiteral("behind"));
    state.ahead = ahead.isDouble() ? ahead.toInt() : -1;
    state.behind = behind.isDouble() ? behind.toInt() : -1;
    // The latest attempt tells whether the remote can be reached.
    QDateTime latest;
    for (const char* key : {"last_fetch", "last_push", "last_sync"}) {
      const QJsonValue attempt = info.value(QLatin1String(key));
      const QDateTime time = timeOf(attempt);
      if (!time.isValid()) {
        continue;
      }
      const QJsonObject error = attempt.toObject().value(QStringLiteral("error")).toObject();
      if (error.isEmpty() && (!state.lastDone.isValid() || time > state.lastDone)) {
        state.lastDone = time;
      }
      if (!latest.isValid() || time >= latest) {
        latest = time;
        state.problem = error.value(QStringLiteral("class")).toString();
        state.problemMessage = error.value(QStringLiteral("message")).toString();
      }
    }
    state.lastFetch = timeOf(info.value(QStringLiteral("last_fetch")));
    if (state.behind > 0) {
      const QJsonObject incoming = local(state.root, named("incoming"));
      const QJsonObject version = incoming.value(QStringLiteral("versions")).toArray().at(0).toObject();
      if (!version.isEmpty()) {
        state.newest =
            tr(" (the newest by %1, %2)")
                .arg(version.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString(),
                     shortTime(QDateTime::fromSecsSinceEpoch(version.value(QStringLiteral("time")).toInteger())));
      }
    }
    // A sync that stopped for choices stays a conflict until one goes.
    state.conflict = m_state.conflict && m_state.root == state.root;
  }
  m_state = state;
  publishState();
}

void RemoteController::publishState() {
  if (m_state.name.isEmpty()) {
    if (m_loggedState != QLatin1String("none")) {
      m_loggedState = QStringLiteral("none");
      qInfo().noquote() << QStringLiteral("Remote status: none");
    }
    emit stateChanged();
    return;
  }
  QStringList logged;
  const bool busy = m_task != nullptr && m_task->name() != QLatin1String("clone");
  if (busy) {
    logged << m_task->name();
  } else if (m_state.conflict) {
    logged << QStringLiteral("conflict");
  } else if (!m_state.problem.isEmpty()) {
    const QString cls = m_state.problem;
    if (cls == QLatin1String("network") || cls == QLatin1String("timed_out")) {
      logged << QStringLiteral("offline");
    } else if (cls == QLatin1String("auth_failed") || cls == QLatin1String("host_key_unknown")) {
      logged << QStringLiteral("sign-in needed");
    } else if (cls == QLatin1String("not_found")) {
      logged << QStringLiteral("remote not found");
    } else if (cls == QLatin1String("git_missing")) {
      logged << QStringLiteral("git missing");
    }
  }
  if (m_state.ahead > 0) {
    logged << QStringLiteral("ahead %1").arg(m_state.ahead);
  }
  if (m_state.behind > 0) {
    logged << QStringLiteral("behind %1").arg(m_state.behind);
  }
  if (logged.isEmpty()) {
    logged << (m_state.ahead == 0 && m_state.behind == 0 ? QStringLiteral("synced") : QStringLiteral("not synced"));
  }
  // Logged when it changes, or is another project's.
  const QString line = logged.join(QStringLiteral(", "));
  if (m_state.root + QLatin1Char('|') + line != m_loggedState) {
    m_loggedState = m_state.root + QLatin1Char('|') + line;
    qInfo().noquote() << QStringLiteral("Remote status: %1").arg(line);
  }
  emit stateChanged();
}

// ---------------------------------------------------------------------------
// Tasks

bool RemoteController::startTask(RemoteTask* task, std::function<void(const QJsonObject&)> done) {
  if (m_task != nullptr || m_holding) {
    delete task;
    return false;
  }
  m_task = task;
  m_taskDone = std::move(done);
  connect(task, &RemoteTask::progressed, this, &RemoteController::publishState);
  connect(task, &RemoteTask::finished, this, &RemoteController::taskFinished);
  qInfo().noquote() << QStringLiteral("Remote task %1 started").arg(task->name());
  task->start();
  publishState();
  return true;
}

void RemoteController::taskFinished() {
  RemoteTask* task = m_task;
  if (task == nullptr) {
    return;
  }
  m_task = nullptr;
  const QJsonObject answer = task->answer();
  std::function<void(const QJsonObject&)> done = std::move(m_taskDone);
  m_taskDone = nullptr;
  const QString cls = errorClass(answer);
  qInfo().noquote() << (cls.isEmpty() ? QStringLiteral("Remote task %1 done").arg(task->name())
                                      : QStringLiteral("Remote task %1 failed (%2): %3")
                                            .arg(task->name(), cls, errorMessage(answer)));
  task->deleteLater();
  publishState();
  if (m_branchWait) {
    // Save waits for the sync: its outcome is told, not asked about.
    m_interactive = false;
    if (done) {
      done(answer);
    }
    m_interactive = true;
    return;
  }
  // The window's document may be the model worker's now: after its job.
  m_host.whenIdle([this, done = std::move(done), answer] {
    if (done) {
      done(answer);
    }
    runWaiting();
  });
}

void RemoteController::runWaiting() {
  if (m_task != nullptr || m_holding) {
    return;
  }
  if (m_settingsWaiting) {
    m_settingsWaiting = false;
    RemoteSettings::load().apply();
  }
  if (m_syncWaiting) {
    m_syncWaiting = false;
    sync();
  } else if (m_pushWaiting) {
    m_pushWaiting = false;
    push();
  } else if (m_checkWaiting) {
    m_checkWaiting = false;
    startCheck(Mode::Quiet);
  }
}

QJsonObject RemoteController::runModal(RemoteTask* task, const QString& label) {
  QProgressDialog dialog(label, tr("Cancel"), 0, 0, &m_window);
  dialog.setWindowTitle(tr("Remote Repository"));
  dialog.setWindowModality(Qt::WindowModal);
  dialog.setMinimumDuration(400);
  dialog.setAutoReset(false);
  dialog.setAutoClose(false);
  QEventLoop loop;
  connect(task, &RemoteTask::progressed, &dialog, [&dialog, label](const QString& text, int percent) {
    if (percent >= 0) {
      dialog.setRange(0, 100);
      dialog.setValue(percent);
    }
    dialog.setLabelText(text.isEmpty() ? label : QStringLiteral("%1\n%2").arg(label, text));
  });
  connect(&dialog, &QProgressDialog::canceled, task, &RemoteTask::cancel);
  connect(task, &RemoteTask::finished, &loop, &QEventLoop::quit);
  m_task = task;
  qInfo().noquote() << QStringLiteral("Remote task %1 started").arg(task->name());
  task->start();
  publishState();
  loop.exec();
  disconnect(&dialog, nullptr, task, nullptr);
  dialog.reset();
  m_task = nullptr;
  const QJsonObject answer = task->answer();
  const QString cls = errorClass(answer);
  qInfo().noquote() << (cls.isEmpty() ? QStringLiteral("Remote task %1 done").arg(task->name())
                                      : QStringLiteral("Remote task %1 failed (%2): %3")
                                            .arg(task->name(), cls, errorMessage(answer)));
  delete task;
  publishState();
  return answer;
}

bool RemoteController::settle(const QString& why) {
  if (m_task == nullptr) {
    return true;
  }
  if (m_task->name() == QLatin1String("sync")) {
    m_host.status(tr("A sync is running: %1 when it is done.").arg(why), true);
    return false;
  }
  // A check or a push of the open project: stopped (the versions wait).
  m_holding = true;
  m_pushWaiting = m_checkWaiting = false;
  QEventLoop loop;
  connect(m_task, &RemoteTask::finished, &loop, &QEventLoop::quit);
  m_task->cancel();
  loop.exec();
  m_holding = false;
  return m_task == nullptr;
}

void RemoteController::waitForBranch() {
  if (m_task == nullptr || m_task->name() != QLatin1String("sync")) {
    return;
  }
  QProgressDialog dialog(tr("Waiting for the sync to finish..."), tr("Stop the Sync"), 0, 0, &m_window);
  dialog.setWindowTitle(tr("Sync"));
  dialog.setWindowModality(Qt::WindowModal);
  dialog.setMinimumDuration(300);
  QEventLoop loop;
  RemoteTask* task = m_task;
  connect(&dialog, &QProgressDialog::canceled, task, &RemoteTask::cancel);
  connect(task, &RemoteTask::finished, &loop, &QEventLoop::quit);
  qInfo().noquote() << QStringLiteral("Remote: waiting for the sync before saving");
  m_branchWait = true;
  loop.exec();
  m_branchWait = false;
  disconnect(&dialog, nullptr, task, nullptr);
  dialog.reset();
  // A push or check that waited goes after the save.
  m_host.whenIdle([this] { runWaiting(); });
}

void RemoteController::shutDown() {
  m_holding = true;
  m_checkTimer->stop();
  m_retryTimer->stop();
  if (m_task != nullptr) {
    qInfo().noquote() << QStringLiteral("Remote task %1 stopped: Mitcad closes").arg(m_task->name());
    delete m_task; // cancels and waits
  }
}

void RemoteController::logFailure(const QString& what, const QJsonObject& answer) {
  qWarning().noquote() << QStringLiteral("%1 failed (%2): %3").arg(what, errorClass(answer), errorMessage(answer));
}

// ---------------------------------------------------------------------------
// Checks and pushes

void RemoteController::fileOpened(const QString& path) {
  Q_UNUSED(path);
  m_dismissed.clear();
  m_state.conflict = false;
  if (m_bar != nullptr) {
    m_bar->hide();
  }
  refreshStatus();
  if (m_state.name.isEmpty()) {
    return;
  }
  if (m_skipCheck) {
    // Opened again after a sync, which fetched.
    m_skipCheck = false;
    return;
  }
  if (m_sync.checkMinutes > 0) {
    startCheck(Mode::Quiet);
  } else if (m_state.ahead > 0 && m_sync.sendAtOnce) {
    push();
  }
}

void RemoteController::firstChange() {
  if (m_state.name.isEmpty() || m_sync.checkMinutes == 0) {
    return;
  }
  if (!m_state.lastFetch.isValid() ||
      m_state.lastFetch.secsTo(QDateTime::currentDateTimeUtc()) >= changeCheckSeconds()) {
    qInfo().noquote() << QStringLiteral("Remote check: the first change since opening");
    startCheck(Mode::Quiet);
  }
}

void RemoteController::versionRecorded(const QString& path) {
  refreshStatus();
  if (m_state.name.isEmpty()) {
    return;
  }
  // Only the current project's.
  const QJsonObject info = local(path, named("info"));
  if (QDir::cleanPath(info.value(QStringLiteral("root")).toString()) != QDir::cleanPath(m_state.root)) {
    return;
  }
  updateNotice(true);
  if (m_sync.sendAtOnce) {
    push();
  }
}

void RemoteController::check() {
  if (m_state.name.isEmpty()) {
    refreshStatus();
  }
  if (m_state.name.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Check for Newer Versions: not a Cloud project");
    QMessageBox box(QMessageBox::Information, tr("Check for Newer Versions"),
                    tr("The project is not a Cloud project: its versions are on this computer only."),
                    QMessageBox::Close, &m_window);
    box.setInformativeText(tr("Project Settings shares it through a git repository."));
    QPushButton* settings = m_host.projectRoot().isEmpty()
                                ? nullptr
                                : box.addButton(tr("Project &Settings..."), QMessageBox::AcceptRole);
    box.exec();
    if (settings != nullptr && box.clickedButton() == settings && m_host.projectSettings) {
      m_host.projectSettings();
    }
    return;
  }
  startCheck(Mode::Shown);
}

void RemoteController::startCheck(Mode mode) {
  if (m_state.name.isEmpty()) {
    return;
  }
  const QString root = m_state.root;
  auto* task = RemoteTask::command(root, named("fetch"), this);
  const bool started = startTask(task, [this, mode, root](const QJsonObject& answer) {
    refreshStatus();
    if (m_state.root != root) {
      return; // another project's design is open now
    }
    const QString cls = errorClass(answer);
    if (!cls.isEmpty()) {
      logFailure(QStringLiteral("Remote check"), answer);
      if (mode == Mode::Shown && m_interactive) {
        warn(&m_window, tr("Check for Newer Versions"), errorMessage(answer), errorDetail(answer));
      }
      return;
    }
    qInfo().noquote() << QStringLiteral("Remote check: ahead %1, behind %2").arg(m_state.ahead).arg(m_state.behind);
    if (mode == Mode::Shown) {
      m_host.status(m_state.behind > 0
                        ? tr("%n newer version(s) on the remote: Sync takes them.", nullptr, m_state.behind)
                        : tr("No newer versions on the remote."),
                    false);
    }
    updateNotice(false);
    // Versions that waited go now, unless the remote has newer ones.
    if (m_state.ahead > 0 && m_state.behind == 0 && m_sync.sendAtOnce) {
      push();
    }
  });
  if (!started && !m_holding) {
    m_checkWaiting = true;
  }
}

void RemoteController::push() {
  if (m_state.name.isEmpty()) {
    return;
  }
  const QString root = m_state.root;
  auto* task = RemoteTask::command(root, named("push"), this);
  const bool started = startTask(task, [this, root](const QJsonObject& answer) {
    refreshStatus();
    const QString cls = errorClass(answer);
    if (cls.isEmpty()) {
      m_retries = 0;
      m_retryTimer->stop();
      if (answer.value(QStringLiteral("pushed")).toBool()) {
        qInfo().noquote() << QStringLiteral("Remote push: sent %1 version(s) to %2/%3")
                                 .arg(answer.value(QStringLiteral("versions")).toInt())
                                 .arg(answer.value(QStringLiteral("remote")).toString(),
                                      answer.value(QStringLiteral("branch")).toString());
        emit versionsSent(root, answer.value(QStringLiteral("branch")).toString(),
                          answer.value(QStringLiteral("head")).toString());
      }
      for (const QString& warning : texts(answer.value(QStringLiteral("warnings")))) {
        qWarning().noquote() << QStringLiteral("Remote push warning: %1").arg(warning);
        m_host.status(warning, true);
      }
      return;
    }
    logFailure(QStringLiteral("Remote push"), answer);
    if (cls == QLatin1String("rejected")) {
      // The remote has versions this project lacks: learn which.
      if (m_state.root == root) {
        m_checkWaiting = true;
      }
    } else if (cls == QLatin1String("too_large")) {
      if (m_interactive) {
        warn(&m_window, tr("Send Versions"), errorMessage(answer), errorDetail(answer));
      }
    } else if (passing(cls) && m_state.root == root) {
      scheduleRetry();
    }
  });
  if (!started && !m_holding) {
    m_pushWaiting = true;
  }
}

void RemoteController::scheduleRetry() {
  const int base = retrySeconds();
  int seconds = base;
  for (int i = 0; i < m_retries && seconds < kRetryMaxSeconds; ++i) {
    seconds *= 2;
  }
  seconds = std::min(seconds, std::max(base, kRetryMaxSeconds));
  ++m_retries;
  qInfo().noquote() << QStringLiteral("Remote push: %1 version(s) wait; trying again in %2 s")
                           .arg(std::max(m_state.ahead, 0))
                           .arg(seconds);
  m_retryTimer->start(seconds * 1000);
}

void RemoteController::setLive(bool live) {
  if (live == m_live) {
    return;
  }
  m_live = live;
  qInfo().noquote() << (live ? QStringLiteral("Remote check: no timer while live updates are connected")
                             : QStringLiteral("Remote check: the timer again (every %1 min)").arg(m_sync.checkMinutes));
  applyTimer();
}

void RemoteController::versionAnnounced() {
  if (m_state.name.isEmpty()) {
    refreshStatus();
  }
  if (m_state.name.isEmpty()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Remote check: announced by live updates");
  startCheck(Mode::Quiet);
}

void RemoteController::applyTimer() {
  const int minutes = m_sync.checkMinutes;
  if (m_live) {
    // Versions are announced; the lock refs' polls catch what was missed.
    m_checkTimer->stop();
  } else if (minutes > 0) {
    if (m_checkTimer->interval() != minutes * 60 * 1000 || !m_checkTimer->isActive()) {
      m_checkTimer->start(minutes * 60 * 1000);
    }
  } else {
    m_checkTimer->stop();
  }
}

void RemoteController::settingsChanged() {
  if (m_task != nullptr) {
    m_settingsWaiting = true;
  } else {
    RemoteSettings::load().apply();
  }
  m_syncRoot = QStringLiteral("\n"); // read again
  refreshStatus();
  applyTimer();
}

void RemoteController::updateNotice(bool saved) {
  if (m_bar == nullptr) {
    return;
  }
  const QString file = m_host.filePath();
  if (m_state.name.isEmpty() || !inFolder(file, m_state.root)) {
    m_bar->hide();
    return;
  }
  const QJsonObject incoming = local(file, {{QStringLiteral("cmd"), QStringLiteral("incoming")},
                                            {QStringLiteral("path"), file}});
  // A design with an edit lock (mitcad#89): a read-only window shows the
  // newer version itself, the editor's is brought up to date.
  if (!saved && !incoming.isEmpty() && errorClass(incoming).isEmpty() && m_host.newerVersion &&
      m_host.newerVersion(file, incoming)) {
    if (m_bar->isVisible()) {
      qInfo().noquote() << QStringLiteral("Remote notice: hidden");
    }
    m_bar->hide();
    return;
  }
  const QJsonArray versions = incoming.value(QStringLiteral("file_versions")).toArray();
  if (incoming.isEmpty() || !errorClass(incoming).isEmpty() || !incoming.value(QStringLiteral("differs")).toBool() ||
      versions.isEmpty()) {
    if (m_bar->isVisible()) {
      qInfo().noquote() << QStringLiteral("Remote notice: hidden");
    }
    m_bar->hide();
    return;
  }
  const QJsonObject newest = versions.first().toObject();
  const QString id = newest.value(QStringLiteral("id")).toString();
  if (id == m_dismissed && !saved) {
    return;
  }
  const QString name = QFileInfo(file).fileName();
  const QString who = newest.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString();
  const QString at = shortTime(QDateTime::fromSecsSinceEpoch(newest.value(QStringLiteral("time")).toInteger()));
  QString text = tr("A newer version of %1 is on the remote, saved by %2 at %3.").arg(name, who, at);
  if (incoming.value(QStringLiteral("changed_here")).toBool()) {
    text += QLatin1Char(' ') + (saved ? tr("Yours is saved here as a version too: Sync asks which to keep.")
                                      : tr("You changed it too: Sync asks which to keep."));
  } else {
    text += QLatin1Char(' ') + tr("Sync takes it.");
  }
  m_noticeId = id;
  m_notice->present(text);
  m_bar->show();
}

// ---------------------------------------------------------------------------
// Sync

void RemoteController::sync() {
  const std::optional<rust::Box<Project>> repo = project();
  if (!repo) {
    qInfo().noquote() << QStringLiteral("Sync: no project with versions");
    QMessageBox::information(&m_window, tr("Sync"),
                             tr("Sync shares the versions of a Cloud project's designs: make one with File > New "
                                "Project, or share a Local project in File > Project Settings."));
    return;
  }
  refreshStatus();
  if (m_state.name.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Sync: %1 is a Local project").arg(m_state.root);
    QMessageBox box(QMessageBox::Question, tr("Sync"),
                    tr("%1 is a Local project: its versions are on this computer only.")
                        .arg(QFileInfo(m_state.root).fileName()),
                    QMessageBox::Cancel, &m_window);
    box.setInformativeText(tr("Project Settings shares it through a git repository."));
    QPushButton* settingsButton = box.addButton(tr("Project &Settings..."), QMessageBox::AcceptRole);
    box.setDefaultButton(settingsButton);
    box.exec();
    if (box.clickedButton() == settingsButton && m_host.projectSettings) {
      m_host.projectSettings();
    }
    return;
  }
  if (m_task != nullptr) {
    if (m_task->name() == QLatin1String("sync")) {
      m_host.status(tr("A sync is running."), false);
    } else {
      // After the check or push that runs.
      m_syncWaiting = true;
    }
    return;
  }
  const QString file = m_host.filePath();
  const QString name = QFileInfo(file).fileName();
  if (inFolder(file, m_state.root) && m_host.modified()) {
    qInfo().noquote() << QStringLiteral("Sync: %1 has unsaved changes: asked to save").arg(name);
    QMessageBox box(QMessageBox::Question, tr("Sync"), tr("Save %1 before the sync?").arg(name),
                    QMessageBox::Cancel, &m_window);
    box.setInformativeText(tr("A sync shares versions: the open design's changes are recorded as a version "
                              "first."));
    QPushButton* save = box.addButton(tr("&Save and Sync"), QMessageBox::AcceptRole);
    box.setDefaultButton(save);
    box.exec();
    if (box.clickedButton() != save || !m_host.save()) {
      qInfo().noquote() << QStringLiteral("Sync cancelled");
      return;
    }
    if (m_task != nullptr) {
      // The save's push runs: the sync after it.
      m_syncWaiting = true;
      return;
    }
  }
  const std::optional<QString> author = m_host.author(**repo);
  if (!author) {
    qInfo().noquote() << QStringLiteral("Sync cancelled");
    return;
  }
  // Versions of files someone else holds the edit lock of (mitcad#89).
  if (m_host.confirmSend && !m_host.confirmSend()) {
    qInfo().noquote() << QStringLiteral("Sync cancelled");
    m_host.status(tr("Sync cancelled: nothing was sent."), false);
    return;
  }
  if (m_bar != nullptr) {
    m_bar->hide();
  }
  runSync(*author, {}, true);
}

void RemoteController::checkQuietly() {
  if (m_state.name.isEmpty()) {
    refreshStatus();
  }
  if (!m_state.name.isEmpty()) {
    startCheck(Mode::Quiet);
  }
}

QJsonObject RemoteController::syncNow(const QString& label) {
  const QJsonObject nothing{{QStringLiteral("case"), QStringLiteral("nothing")}};
  refreshStatus();
  if (m_state.name.isEmpty() || m_state.ahead == 0) {
    return nothing;
  }
  // The running work first: a sync is waited for, a check or a push stops
  // (this sync sends what the push would have).
  if (m_task != nullptr && m_task->name() == QLatin1String("sync")) {
    QEventLoop loop;
    connect(m_task, &RemoteTask::finished, &loop, &QEventLoop::quit);
    m_branchWait = true;
    loop.exec();
    m_branchWait = false;
    m_host.whenIdle([this] { runWaiting(); });
  } else if (m_task != nullptr && !settle(label)) {
    return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                  {QStringLiteral("message"), tr("The remote is busy.")},
                                                  {QStringLiteral("detail"), QString()}}}};
  }
  refreshStatus();
  if (m_state.name.isEmpty() || m_state.ahead == 0) {
    return nothing;
  }
  const std::optional<rust::Box<Project>> repo = project();
  const std::optional<QString> author = repo ? m_host.author(**repo) : std::nullopt;
  if (!author) {
    return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("cancelled")},
                                                  {QStringLiteral("message"), tr("No author for the versions.")},
                                                  {QStringLiteral("detail"), QString()}}}};
  }
  const QString root = m_state.root;
  auto* task = RemoteTask::command(root,
                                   {{QStringLiteral("cmd"), QStringLiteral("sync")},
                                    {QStringLiteral("author"), *author},
                                    {QStringLiteral("fetch"), true}},
                                   this);
  const QJsonObject answer = runModal(task, label);
  const QString cls = errorClass(answer);
  qInfo().noquote() << QStringLiteral("Sync: %1%2, %3 replayed, %4")
                           .arg(answer.value(QStringLiteral("case")).toString(),
                                cls.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(cls))
                           .arg(answer.value(QStringLiteral("replayed")).toArray().size())
                           .arg(answer.value(QStringLiteral("pushed")).toBool() ? QStringLiteral("sent")
                                                                                 : QStringLiteral("nothing sent"));
  const QJsonObject pushed = answer.value(QStringLiteral("push")).toObject();
  if (pushed.value(QStringLiteral("pushed")).toBool()) {
    emit versionsSent(root, pushed.value(QStringLiteral("branch")).toString(),
                      pushed.value(QStringLiteral("head")).toString());
  }
  refreshStatus();
  if (cls == QLatin1String("conflict")) {
    m_state.conflict = true;
    publishState();
  }
  return answer;
}

void RemoteController::runSync(const QString& author, const QJsonObject& resolutions, bool fetch) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("sync")},
                      {QStringLiteral("author"), author},
                      {QStringLiteral("fetch"), fetch}};
  if (!resolutions.isEmpty()) {
    command.insert(QStringLiteral("resolutions"), resolutions);
  }
  auto* task = RemoteTask::command(m_state.root, command, this);
  if (!startTask(task, [this, author](const QJsonObject& answer) { syncFinished(author, answer); })) {
    m_host.status(tr("The remote is busy: Sync again in a moment."), true);
    return;
  }
  m_host.status(tr("Syncing with %1...").arg(m_state.upstream), false);
}

void RemoteController::syncFinished(const QString& author, const QJsonObject& answer) {
  const QString cls = errorClass(answer);
  const QString upstream = answer.value(QStringLiteral("upstream")).toString();
  const QJsonArray conflicts = answer.value(QStringLiteral("conflicts")).toArray();
  qInfo().noquote() << QStringLiteral("Sync: %1%2, %3 replayed, %4")
                           .arg(answer.value(QStringLiteral("case")).toString(),
                                cls.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(cls))
                           .arg(answer.value(QStringLiteral("replayed")).toArray().size())
                           .arg(answer.value(QStringLiteral("pushed")).toBool() ? QStringLiteral("sent")
                                                                                 : QStringLiteral("nothing sent"));
  refreshStatus();
  const QJsonObject pushed = answer.value(QStringLiteral("push")).toObject();
  if (pushed.value(QStringLiteral("pushed")).toBool()) {
    emit versionsSent(m_state.root, pushed.value(QStringLiteral("branch")).toString(),
                      pushed.value(QStringLiteral("head")).toString());
  }
  if (cls == QLatin1String("conflict")) {
    m_state.conflict = true;
    publishState();
    if (!m_interactive) {
      m_host.status(tr("Sync stopped: %n file(s) changed both here and on the remote. Sync again to choose "
                       "what to keep.",
                       nullptr, static_cast<int>(conflicts.size())),
                    true);
      return;
    }
    const std::optional<QJsonObject> choices = askResolveConflicts(
        &m_window, conflicts, [this](const QJsonObject& conflict) { return compareConflict(conflict); });
    if (!choices) {
      m_host.status(tr("Sync stopped: nothing changed. Sync again to choose what to keep."), true);
      return;
    }
    for (const QJsonValue& value : conflicts) {
      const QJsonObject conflict = value.toObject();
      const QString path = conflict.value(QStringLiteral("path")).toString();
      qInfo().noquote() << QStringLiteral("Sync conflict: %1 (%2): %3")
                               .arg(path, conflict.value(QStringLiteral("kind")).toString(),
                                    choices->value(path).toString());
    }
    m_state.conflict = false;
    // The remote as fetched for the choices.
    runSync(author, *choices, false);
    return;
  }
  m_state.conflict = false;
  if (cls == QLatin1String("local_changes") && m_interactive) {
    if (recordBeforeSync(author, answer)) {
      runSync(author, {}, false);
      return;
    }
    publishState();
    return;
  }
  // What the sync did stays done, also when it stopped later.
  if (m_interactive && (!answer.value(QStringLiteral("backup")).isNull() ||
                        !answer.value(QStringLiteral("changed_paths")).toArray().isEmpty())) {
    afterSync(answer);
  } else if (m_interactive) {
    m_host.versionsChanged();
  }
  publishState();
  if (cls == QLatin1String("cancelled")) {
    m_host.status(tr("Sync cancelled."), false);
    return;
  }
  if (!cls.isEmpty()) {
    if (m_interactive) {
      warn(&m_window, tr("Sync"), errorMessage(answer), errorDetail(answer));
    } else {
      m_host.status(errorMessage(answer), true);
    }
    return;
  }
  // What it did.
  const int taken = answer.value(QStringLiteral("fetch")).toObject().value(QStringLiteral("behind")).toInt();
  const int sent = answer.value(QStringLiteral("push")).toObject().value(QStringLiteral("versions")).toInt();
  const QString kind = answer.value(QStringLiteral("case")).toString();
  QString text;
  if (kind == QLatin1String("up_to_date")) {
    text = tr("Up to date with %1.").arg(upstream);
  } else if (kind == QLatin1String("push")) {
    text = tr("Sent %n version(s) to %1.", nullptr, sent).arg(upstream);
  } else if (kind == QLatin1String("fast_forward")) {
    text = tr("Took %n newer version(s) from %1.", nullptr, taken).arg(upstream);
  } else {
    text = tr("Took %n newer version(s) from %1", nullptr, taken).arg(upstream) + QStringLiteral(" ") +
           tr("and sent %n of yours after them.", nullptr, sent);
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("copies")).toArray()) {
    const QJsonObject copy = value.toObject();
    text += QLatin1Char(' ') + tr("Your %1 is kept as %2.")
                                   .arg(copy.value(QStringLiteral("path")).toString(),
                                        copy.value(QStringLiteral("copy")).toString());
    qInfo().noquote() << QStringLiteral("Sync: %1 kept as %2")
                             .arg(copy.value(QStringLiteral("path")).toString(),
                                  copy.value(QStringLiteral("copy")).toString());
  }
  const QStringList warnings = texts(answer.value(QStringLiteral("warnings")));
  for (const QString& warning : warnings) {
    qWarning().noquote() << QStringLiteral("Sync warning: %1").arg(warning);
  }
  if (!warnings.isEmpty()) {
    text += QLatin1Char(' ') + warnings.join(QStringLiteral(" "));
  }
  qInfo().noquote() << QStringLiteral("Sync done: %1").arg(text);
  m_host.status(text, !warnings.isEmpty());
}

void RemoteController::afterSync(const QJsonObject& answer) {
  const QString file = m_host.filePath();
  const QStringList changed = texts(answer.value(QStringLiteral("changed_paths")));
  const QString relative = QDir(m_state.root).relativeFilePath(file);
  if (file.isEmpty() || !changed.contains(relative)) {
    m_host.versionsChanged();
    return;
  }
  const QString name = QFileInfo(file).fileName();
  if (m_host.modified()) {
    // Changed meanwhile: Save compares the file and asks.
    qInfo().noquote() << QStringLiteral("Sync: %1 changed, the open design has unsaved changes").arg(name);
    m_host.status(tr("%1 changed with the sync; your unsaved changes stay open, and Save asks how to keep them.")
                      .arg(name),
                  true);
    return;
  }
  if (!QFileInfo::exists(file)) {
    qInfo().noquote() << QStringLiteral("Sync: %1 was deleted on the remote").arg(name);
    m_host.versionsChanged();
    m_host.status(tr("The sync removed %1, deleted on the remote; the design stays open, and Save keeps it.")
                      .arg(name),
                  true);
    return;
  }
  qInfo().noquote() << QStringLiteral("Sync: opening %1 again").arg(name);
  m_skipCheck = true;
  QString error;
  if (!m_host.open(file, error)) {
    m_skipCheck = false;
    if (!error.isEmpty()) {
      warn(&m_window, tr("Sync"), tr("Could not open %1 after the sync:\n%2").arg(name, error), QString());
    }
  }
}

bool RemoteController::recordBeforeSync(const QString& author, const QJsonObject& answer) {
  // The project's files with changes no version holds: project files, and
  // files the project's versions have (another program's new files stay
  // out).
  QStringList paths;
  QStringList names;
  for (const QString& path : texts(answer.value(QStringLiteral("uncommitted")))) {
    const QString full = QDir(m_state.root).filePath(path);
    bool record = path.endsWith(QLatin1String(".mitcad"), Qt::CaseInsensitive);
    if (!record) {
      const QJsonObject status = local(m_state.root, {{QStringLiteral("cmd"), QStringLiteral("status")},
                                                      {QStringLiteral("path"), full}});
      record = status.value(QStringLiteral("head")).isString();
    }
    if (record) {
      paths << full;
      names << path;
    }
  }
  if (paths.isEmpty()) {
    warn(&m_window, tr("Sync"), errorMessage(answer), errorDetail(answer));
    return false;
  }
  qInfo().noquote() << QStringLiteral("Sync: changes no version holds in %1: asked to record them")
                           .arg(names.join(QStringLiteral(", ")));
  QMessageBox box(QMessageBox::Question, tr("Sync"), errorMessage(answer), QMessageBox::Cancel, &m_window);
  box.setInformativeText(tr("Record %1 as a version and sync again?").arg(names.join(QStringLiteral(", "))));
  QPushButton* record = box.addButton(tr("&Record and Sync"), QMessageBox::AcceptRole);
  box.setDefaultButton(record);
  box.exec();
  if (box.clickedButton() != record) {
    return false;
  }
  const QJsonObject committed =
      local(m_state.root, {{QStringLiteral("cmd"), QStringLiteral("commit")},
                           {QStringLiteral("paths"), QJsonArray::fromStringList(paths)},
                           {QStringLiteral("message"), tr("Save %1 before sync").arg(names.join(QStringLiteral(", ")))},
                           {QStringLiteral("author"), author}});
  if (committed.isEmpty()) {
    return false;
  }
  qInfo().noquote() << QStringLiteral("Sync: recorded %1 before the sync").arg(names.join(QStringLiteral(", ")));
  return true;
}

QString RemoteController::compareConflict(const QJsonObject& conflict) const {
  // The remote's latest version of the file against the project's, both
  // read from their commits; nothing computed.
  const QString path = QDir(m_state.root).filePath(conflict.value(QStringLiteral("path")).toString());
  const QString from = conflict.value(QStringLiteral("theirs"))
                           .toObject()
                           .value(QStringLiteral("version"))
                           .toObject()
                           .value(QStringLiteral("id"))
                           .toString();
  const QString to = conflict.value(QStringLiteral("mine"))
                         .toObject()
                         .value(QStringLiteral("version"))
                         .toObject()
                         .value(QStringLiteral("id"))
                         .toString();
  try {
    const rust::Box<Project> repo = open_project(rustStr(m_state.root.toUtf8()));
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("diff")},
                              {QStringLiteral("path"), path},
                              {QStringLiteral("from"), from},
                              {QStringLiteral("to"), to}};
    const QString text = parseObject(repo->command(rustStr(compactJson(command)))).value(QStringLiteral("text")).toString();
    return tr("From the remote's version %1 to yours, %2:").arg(from.left(7), to.left(7)) + QLatin1Char('\n') + text;
  } catch (const std::exception& e) {
    return tr("Could not compare them: %1").arg(errorText(e));
  }
}

// ---------------------------------------------------------------------------
// The repository's page

void RemoteController::openInBrowser() {
  refreshStatus();
  const QString page = addressWebPage(m_state.url);
  if (page.isEmpty()) {
    m_host.status(m_state.name.isEmpty() ? tr("The project is not a Cloud project: it has no repository to show.")
                                         : tr("%1 has no web page Mitcad knows of.").arg(m_state.url),
                  true);
    return;
  }
  qInfo().noquote() << QStringLiteral("Remote: open in browser %1").arg(page);
  openExternalUrl(QUrl(page));
}

} // namespace mitcad
