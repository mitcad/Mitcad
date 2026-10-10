// SPDX-License-Identifier: MIT
#include "LockController.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEventLoop>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QLabel>
#include <QLineEdit>
#include <QMainWindow>
#include <QMessageBox>
#include <QProgressDialog>
#include <QPushButton>
#include <QTimer>
#include <QToolBar>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Json.hpp"
#include "../framework/TestSync.hpp"
#include "ProjectIndicator.hpp"
#include "RemoteController.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QString ltr(const char* text) { return QCoreApplication::translate("LockController", text); }

// MITCAD_LOCK_TIME_SCALE speeds the timers up as it does the core's lock
// clock (a factor from 1 to 10000; UI tests).
double timeScale() {
  static const double scale = [] {
    bool ok = false;
    const double value = qEnvironmentVariable("MITCAD_LOCK_TIME_SCALE").trimmed().toDouble(&ok);
    return ok && std::isfinite(value) && value >= 1.0 && value <= 10000.0 ? value : 1.0;
  }();
  return scale;
}

// A timer's milliseconds for `seconds` of the lock clock.
int scaledMs(double seconds) {
  return static_cast<int>(std::clamp(std::lround(seconds * 1000.0 / timeScale()), 50L, 24L * 3600L * 1000L));
}

QString str(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }
QJsonObject obj(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toObject(); }

QString ownerName(const QJsonObject& lock) {
  const QString name = str(obj(lock, "owner"), "name");
  return name.isEmpty() ? ltr("someone") : name;
}

QString requesterName(const QJsonObject& request) {
  const QString name = str(obj(request, "requester"), "name");
  return name.isEmpty() ? ltr("someone") : name;
}

QDateTime timeOf(const QJsonValue& value) { return QDateTime::fromString(value.toString(), Qt::ISODate); }

// "14:02" today, else "2026-10-05 14:02" (another computer's clock: only
// shown).
QString clockText(const QDateTime& time) {
  if (!time.isValid()) {
    return ltr("an unknown time");
  }
  const QDateTime local = time.toLocalTime();
  return local.date() == QDate::currentDate() ? local.toString(QStringLiteral("HH:mm"))
                                              : local.toString(QStringLiteral("yyyy-MM-dd HH:mm"));
}

// "just now", "2 min ago", "3 h ago".
QString agoText(const QDateTime& time) {
  if (!time.isValid()) {
    return ltr("at an unknown time");
  }
  const qint64 minutes = std::max<qint64>(0, time.secsTo(QDateTime::currentDateTimeUtc()) / 60);
  if (minutes < 1) {
    return ltr("just now");
  }
  if (minutes < 120) {
    return ltr("%1 min ago").arg(minutes);
  }
  return ltr("%1 h ago").arg(minutes / 60);
}

QString isoNow(const QDateTime& time) { return time.toUTC().toString(Qt::ISODate); }

// Failures that mean the remote could not be reached (or not signed in to):
// the lock is tried again later.
bool unreachable(const QString& cls) {
  return cls == QLatin1String("network") || cls == QLatin1String("timed_out") || cls == QLatin1String("not_found") ||
         cls == QLatin1String("auth_failed") || cls == QLatin1String("host_key_unknown") ||
         cls == QLatin1String("git_missing") || cls == QLatin1String("cancelled");
}

bool samePath(const QString& a, const QString& b) {
  if (a.isEmpty() || b.isEmpty()) {
    return a.isEmpty() && b.isEmpty();
  }
  const QString left = QDir::cleanPath(QFileInfo(a).absoluteFilePath());
  const QString right = QDir::cleanPath(QFileInfo(b).absoluteFilePath());
#ifdef _WIN32
  return left.compare(right, Qt::CaseInsensitive) == 0;
#else
  return left == right;
#endif
}

bool inside(const QString& path, const QString& folder) {
  if (path.isEmpty() || folder.isEmpty()) {
    return false;
  }
  const QString file = QDir::cleanPath(QFileInfo(path).absoluteFilePath());
  const QString root = QDir::cleanPath(folder) + QLatin1Char('/');
#ifdef _WIN32
  return file.startsWith(root, Qt::CaseInsensitive);
#else
  return file.startsWith(root);
#endif
}

// A command of the project in `root` on this thread (no network); a failure
// as {"error": {...}}.
QJsonObject localCommand(const QString& root, const QJsonObject& command) {
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    return parseObject(project->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                  {QStringLiteral("message"), errorText(e)},
                                                  {QStringLiteral("detail"), QString()}}}};
  }
}

// The lock of a file in a view (`locks`), by its id or path; empty: none.
QJsonObject lockIn(const QJsonObject& view, const QString& id, const QString& path) {
  for (const QJsonValue& value : view.value(QStringLiteral("locks")).toArray()) {
    const QJsonObject lock = value.toObject();
    if (str(lock, "id") == id || (!path.isEmpty() && str(lock, "path") == path)) {
      return lock;
    }
  }
  return {};
}

} // namespace

// The banner under the toolbar of a read-only window: why, and what can be
// done (Request Edit Access..., Edit, Save as Copy..., Save as New
// Version). Its text and buttons are logged ("Lock banner: ..."), and where
// the buttons are, for the UI tests.
class LockBanner : public QWidget {
public:
  explicit LockBanner(QWidget* parent) : QWidget(parent) {
    setObjectName(QStringLiteral("lockBanner"));
    auto* layout = new QHBoxLayout(this);
    layout->setContentsMargins(8, 2, 8, 2);
    auto* icon = new QLabel;
    icon->setPixmap(themeIcon(QStringLiteral("project")).pixmap(20, 20));
    layout->addWidget(icon);
    m_text = new QLabel;
    m_text->setWordWrap(true);
    m_text->setTextFormat(Qt::PlainText);
    layout->addWidget(m_text, 1);
    m_buttons = new QHBoxLayout;
    layout->addLayout(m_buttons);
  }

  // Shows `text` with buttons named as given; `clicked` gets the name.
  void present(const QString& text, const QStringList& buttons, const std::function<void(const QString&)>& clicked) {
    const QString line = QStringLiteral("%1 [%2]").arg(text, buttons.join(QStringLiteral(", ")));
    if (line == m_shown) {
      return;
    }
    m_shown = line;
    m_text->setText(text);
    for (QPushButton* button : std::as_const(m_list)) {
      button->deleteLater();
    }
    m_list.clear();
    for (const QString& name : buttons) {
      auto* button = new QPushButton(name);
      button->setAutoDefault(false);
      m_buttons->addWidget(button);
      m_list << button;
      const QString plain = QString(name).remove(QLatin1Char('&'));
      connect(button, &QPushButton::clicked, this, [clicked, plain] {
        qInfo().noquote() << QStringLiteral("Lock banner: %1").arg(plain);
        clicked(plain);
      });
    }
    qInfo().noquote() << QStringLiteral("Lock banner: %1").arg(QString(line).remove(QLatin1Char('&')));
    TestSync::singleShot(50, this, [this] {
      QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
      if (!isVisible() || window() == nullptr) {
        return;
      }
      for (const QPushButton* button : std::as_const(m_list)) {
        const QPoint center = button->mapTo(window(), button->rect().center());
        qDebug().noquote() << QStringLiteral("Lock banner %1 at %2,%3")
                                  .arg(button->text().remove(QLatin1Char('&')))
                                  .arg(center.x())
                                  .arg(center.y());
      }
    });
  }

  void clear() { m_shown.clear(); }

private:
  QLabel* m_text = nullptr;
  QHBoxLayout* m_buttons = nullptr;
  QList<QPushButton*> m_list;
  QString m_shown;
};

LockController::LockController(QMainWindow& window, RemoteController& remote, ProjectIndicator& indicator, Host host,
                               QObject* parent)
    : QObject(parent), m_window(window), m_remote(remote), m_indicator(indicator), m_host(std::move(host)) {
  m_pollTimer = new QTimer(this);
  m_pollTimer->setSingleShot(true);
  connect(m_pollTimer, &QTimer::timeout, this, &LockController::poll);
  m_refreshTimer = new QTimer(this);
  connect(m_refreshTimer, &QTimer::timeout, this, [this] { refresh(false); });
  m_idleTimer = new QTimer(this);
  m_idleTimer->setSingleShot(true);
  connect(m_idleTimer, &QTimer::timeout, this, &LockController::idleTimeout);
  m_keepTimer = new QTimer(this);
  m_keepTimer->setSingleShot(true);
  connect(m_keepTimer, &QTimer::timeout, this, [this] {
    // The 15 minutes are over: the request's dialog comes back.
    qInfo().noquote() << QStringLiteral("Edit lock: the time kept for the request is over");
    m_answered.remove(m_keptRequest);
    m_keptRequest.clear();
    schedulePoll(0);
  });
  m_answerTimer = new QTimer(this);
  m_answerTimer->setSingleShot(true);
  connect(m_answerTimer, &QTimer::timeout, this, [this] {
    // No answer within the idle time: released as at the idle time.
    if (m_mode != Mode::Holding || m_shownRequest.isEmpty()) {
      return;
    }
    if (m_blocking > 0 || m_host.busy() || QApplication::activeModalWidget() != nullptr) {
      m_answerTimer->start(1000); // after the job or the dialog
      return;
    }
    QJsonObject request;
    for (const QJsonValue& value : m_lock.value(QStringLiteral("requests")).toArray()) {
      if (str(value.toObject(), "id") == m_shownRequest) {
        request = value.toObject();
      }
    }
    if (request.isEmpty()) {
      return;
    }
    qInfo().noquote() << QStringLiteral("Edit lock request of %1 not answered within %2 min: handing over")
                             .arg(requesterName(request))
                             .arg(m_idleMinutes);
    // Autosave on: a version first; off: the changes stay in the window.
    if (m_host.modified() && m_host.autosaveOn() && m_host.canSaveNow()) {
      m_host.saveVersion(tr("Saved automatically before releasing the edit lock"));
    }
    handOver(request, true, QStringLiteral("no answer"));
  });
  m_releaseAction = new QAction(tr("&Release Edit Lock"), this);
  m_releaseAction->setMenuRole(QAction::NoRole);
  connect(m_releaseAction, &QAction::triggered, this, [this] {
    qInfo().noquote() << QStringLiteral("Project indicator menu chose Release Edit Lock");
    releaseByUser();
  });
  m_detailsAction = new QAction(tr("Edit Lock &Details..."), this);
  m_detailsAction->setMenuRole(QAction::NoRole);
  connect(m_detailsAction, &QAction::triggered, this, [this] {
    qInfo().noquote() << QStringLiteral("Project indicator menu chose Edit Lock Details");
    showDetails();
  });
  m_lastActivity = QDateTime::currentDateTimeUtc();
}

LockController::~LockController() {
  m_queue.clear();
  delete m_task;
}

void LockController::startUp() {
  m_bar = new QToolBar(tr("Edit Lock"), &m_window);
  m_bar->setObjectName(QStringLiteral("lockBar"));
  m_bar->setMovable(false);
  m_bar->setFloatable(false);
  m_bar->toggleViewAction()->setVisible(false);
  m_banner = new LockBanner(m_bar);
  m_bar->addWidget(m_banner);
  m_window.addToolBarBreak(Qt::TopToolBarArea);
  m_window.addToolBar(Qt::TopToolBarArea, m_bar);
  m_bar->hide();
  updateIndicator();
}

void LockController::setLiveLink(LiveLink* link) {
  m_live = link;
  publishOpen();
}

bool LockController::isReadOnly() const { return m_mode == Mode::ReadOnly; }

QString LockController::readOnlyReason() const {
  if (!isReadOnly()) {
    return {};
  }
  if (!m_lock.isEmpty() && !m_lock.value(QStringLiteral("mine")).toBool()) {
    return tr("%1 is read-only: %2 is editing it.").arg(fileName(), ownerName(m_lock));
  }
  if (m_why == Why::Opened) {
    return tr("%1 is read-only: it was opened without an edit lock.").arg(fileName());
  }
  return tr("%1 is read-only: it has no edit lock of yours.").arg(fileName());
}

QString LockController::fileName() const {
  return m_path.isEmpty() ? tr("The design") : QFileInfo(m_path).fileName();
}

QString LockController::relativeOf(const QString& path) const {
  return QDir::fromNativeSeparators(QDir(m_root).relativeFilePath(path));
}

QList<QAction*> LockController::actions() const {
  QList<QAction*> list;
  if (m_mode == Mode::Holding) {
    list << m_releaseAction;
  }
  if (m_mode != Mode::None) {
    list << m_detailsAction;
  }
  return list;
}

// ---------------------------------------------------------------------------
// Tasks

QJsonObject LockController::lockCommand(const char* name, QJsonObject fields) const {
  fields.insert(QStringLiteral("cmd"), QLatin1String(name));
  fields.insert(QStringLiteral("session"), m_host.session().toLower());
  // The holder or requester is the project's author, as for versions;
  // Preferences' default author when the project has none.
  const VersionSettings defaults = VersionSettings::load();
  if (defaults.complete()) {
    fields.insert(QStringLiteral("fallback_author"), defaults.author());
  }
  return fields;
}

RemoteTask* LockController::lockTask(const QString& name, const QList<QJsonObject>& commands) {
  QList<QByteArray> jsons;
  for (const QJsonObject& command : commands) {
    jsons << compactJson(command);
  }
  const QByteArray root = m_root.toUtf8();
  return RemoteTask::custom(
      name,
      [root, jsons](const SyncControl& control) {
        // A project of the thread's own; the core shares what it saw of the
        // lock refs between the projects of a folder.
        const rust::Box<Project> project = open_project(rustStr(root));
        QJsonObject answer = parseObject(project->command_with(rustStr(jsons.first()), control));
        if (jsons.size() > 1 && errorClassOf(answer).isEmpty()) {
          // The view after it (lock_status without the network).
          answer.insert(QStringLiteral("view"), parseObject(project->command_with(rustStr(jsons.at(1)), control)));
        }
        return answer;
      },
      this);
}

void LockController::startTask(RemoteTask* task, std::function<void(const QJsonObject&)> done) {
  if (m_task != nullptr || m_blocking > 0) {
    // After the running one (or the blocking flow).
    QPointer<RemoteTask> pending(task);
    const quint64 generation = m_generation;
    m_queue.push_back([this, pending, generation, done = std::move(done)]() mutable {
      if (pending == nullptr) {
        return;
      }
      if (generation != m_generation) {
        pending->deleteLater();
        return;
      }
      startTask(pending, std::move(done));
    });
    return;
  }
  m_task = task;
  m_taskDone = std::move(done);
  m_taskGeneration = m_generation;
  connect(task, &RemoteTask::finished, this, &LockController::taskFinished);
  task->start();
}

void LockController::taskFinished() {
  RemoteTask* task = m_task;
  if (task == nullptr) {
    return;
  }
  m_task = nullptr;
  const QJsonObject answer = task->answer();
  std::function<void(const QJsonObject&)> done = std::move(m_taskDone);
  m_taskDone = nullptr;
  const quint64 generation = m_taskGeneration;
  task->deleteLater();
  // The window's document may be the model worker's now: after its job.
  m_host.whenIdle([this, generation, done = std::move(done), answer] {
    if (generation == m_generation && done) {
      done(answer);
    }
    if (m_task == nullptr && m_blocking == 0 && !m_queue.empty()) {
      std::function<void()> next = std::move(m_queue.front());
      m_queue.pop_front();
      next();
    }
  });
}

void LockController::dropRunning() {
  ++m_generation;
  m_queue.clear();
  if (m_task != nullptr) {
    QEventLoop loop;
    connect(m_task, &RemoteTask::finished, &loop, &QEventLoop::quit);
    m_task->cancel();
    loop.exec();
  }
}

QJsonObject LockController::runNow(RemoteTask* task, const QString& label) {
  ++m_blocking;
  m_pollTimer->stop();
  dropRunning();
  QJsonObject answer;
  {
    QProgressDialog dialog(label, tr("Cancel"), 0, 0, &m_window);
    dialog.setWindowTitle(tr("Edit Lock"));
    dialog.setWindowModality(Qt::WindowModal);
    dialog.setMinimumDuration(400);
    dialog.setAutoReset(false);
    dialog.setAutoClose(false);
    QEventLoop loop;
    connect(&dialog, &QProgressDialog::canceled, task, &RemoteTask::cancel);
    connect(task, &RemoteTask::finished, &loop, &QEventLoop::quit);
    task->start();
    loop.exec();
    disconnect(&dialog, nullptr, task, nullptr);
    dialog.reset();
    answer = task->answer();
    delete task;
  }
  --m_blocking;
  if (m_blocking == 0 && m_task == nullptr && !m_queue.empty()) {
    std::function<void()> next = std::move(m_queue.front());
    m_queue.pop_front();
    next();
  }
  return answer;
}

// ---------------------------------------------------------------------------
// The design

bool LockController::lockable(QString* why) const {
  const ProjectState project = m_host.project();
  QString reason;
  if (m_path.isEmpty()) {
    reason = QStringLiteral("an untitled design");
  } else if (project.kind != ProjectState::Kind::Cloud) {
    reason = QStringLiteral("not a Cloud project");
  } else if (!inside(m_path, project.root)) {
    reason = QStringLiteral("outside the project");
  } else if (QFileInfo(m_path).suffix().compare(QLatin1String("mitcad"), Qt::CaseInsensitive) != 0) {
    reason = QStringLiteral("not a design");
  } else if (QFileInfo::exists(QDir(project.root).filePath(QStringLiteral("mitcad-library.json")))) {
    reason = QStringLiteral("a library repository");
  }
  if (why != nullptr) {
    *why = reason;
  }
  return reason.isEmpty();
}

bool LockController::remoteRefuses() const {
  const ProjectState project = m_host.project();
  const auto known = m_probes.constFind(project.root + QLatin1Char('|') + project.remoteUrl);
  return known != m_probes.constEnd() && !known->first;
}

void LockController::readSettings() {
  const QString root = m_root.isEmpty() ? m_host.project().root : m_root;
  if (root.isEmpty()) {
    return;
  }
  const QJsonObject settings = localCommand(root, {{QStringLiteral("cmd"), QStringLiteral("project_settings")}});
  const QJsonObject locks = obj(obj(settings, "shared"), "edit_locks");
  m_locksOn = locks.value(QStringLiteral("enabled")).toBool(true);
  m_idleMinutes = std::clamp(locks.value(QStringLiteral("idle_minutes")).toInt(10), 1, 120);
  m_projectPollSeconds = std::clamp(locks.value(QStringLiteral("poll_seconds")).toInt(10), 5, 600);
}

void LockController::resetDesign() {
  ++m_generation;
  m_queue.clear();
  stopTimers();
  m_pollTimer->stop();
  m_keepTimer->stop();
  m_answerTimer->stop();
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  closeAnswerDialog();
  m_mode = Mode::None;
  m_why = Why::HeldByOther;
  m_root.clear();
  m_path.clear();
  m_relative.clear();
  m_fileId.clear();
  m_myName.clear();
  m_lock = QJsonObject();
  m_view = QJsonObject();
  m_requested = false;
  m_granted = false;
  m_requestState.clear();
  m_declined.clear();
  m_idle = false;
  m_kept = false;
  m_offline = false;
  m_following = false;
  m_lostText.clear();
  m_releasedText.clear();
  m_checkedHead.clear();
  m_syncAfterCheck = false;
  m_newerWhileTaking = false;
  m_checkedWhileTaking = false;
  m_shownRequest.clear();
  m_answered.clear();
  m_keptRequest.clear();
  m_receipts.clear();
  m_alsoOpen.clear();
  m_offlineAsked.clear();
  m_takeRetries = 0;
}

void LockController::designOpened(const QString& path, bool readOnly) {
  const QString absolute = path.isEmpty() ? QString() : QDir::cleanPath(QFileInfo(path).absoluteFilePath());
  const bool wasReadOnly = isReadOnly();
  if (!absolute.isEmpty() && samePath(absolute, m_path) && !(readOnly && !wasReadOnly)) {
    // The same file again (a sync, a restore, the editor's version shown):
    // its lock stays; changes kept from editing are gone if it was read
    // from the file (or the editor's version shown, newerVersion).
    m_kept = m_kept && m_host.modified();
    m_following = false;
    if (m_mode == Mode::None && !m_root.isEmpty() && !remoteRefuses()) {
      // A sync may have turned edit locks on for everyone.
      readSettings();
      if (m_locksOn) {
        m_path.clear();
        designOpened(absolute, false);
        return;
      }
    }
    updateBanner();
    return;
  }
  resetDesign();
  m_path = absolute;
  const ProjectState project = m_host.project();
  QString why;
  const bool canLock = lockable(&why);
  if (canLock) {
    m_root = QDir::cleanPath(project.root);
    m_relative = relativeOf(m_path);
    m_fileId = QString::fromLatin1(QCryptographicHash::hash(m_relative.toUtf8(), QCryptographicHash::Sha256).toHex());
    readSettings();
  }
  if (readOnly) {
    m_mode = Mode::ReadOnly;
    m_why = Why::Opened;
    qInfo().noquote() << QStringLiteral("Edit lock: %1 opened read-only, without an edit lock").arg(fileName());
    if (!wasReadOnly) {
      m_host.readOnlyChanged();
    }
    if (canLock && m_locksOn) {
      schedulePoll(0);
    }
    publishOpen();
    updateIndicator();
    updateBanner();
    return;
  }
  if (!canLock || !m_locksOn || remoteRefuses()) {
    if (project.kind == ProjectState::Kind::Cloud && !m_path.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Edit lock: none for %1 (%2)")
                               .arg(fileName(), !canLock   ? why
                                                : m_locksOn ? QStringLiteral("the remote does not accept lock references")
                                                            : QStringLiteral("edit locks off"));
    }
    if (wasReadOnly) {
      m_host.readOnlyChanged();
    }
    // Who has the design open is told all the same (a Cloud project's).
    publishOpen();
    updateIndicator();
    updateBanner();
    return;
  }
  m_mode = Mode::Taking;
  m_lastActivity = QDateTime::currentDateTimeUtc();
  if (wasReadOnly) {
    m_host.readOnlyChanged();
  }
  updateIndicator();
  updateBanner();
  take();
}

bool LockController::designClosing(const QString& next, bool readOnly) {
  if (!next.isEmpty() && samePath(next, m_path) && !(readOnly && !isReadOnly())) {
    return true; // the same design again
  }
  if (!closeLock(false)) {
    return false;
  }
  // The design is gone: what follows (its project, the next design) starts
  // afresh.
  const bool wasReadOnly = isReadOnly();
  resetDesign();
  if (wasReadOnly) {
    m_host.readOnlyChanged();
  }
  updateIndicator();
  updateBanner();
  return true;
}

void LockController::designSaved(const QString& path) {
  if (samePath(path, m_path)) {
    if (m_mode == Mode::Holding) {
      // Its base is the version just recorded.
      const QString head =
          str(localCommand(m_root, {{QStringLiteral("cmd"), QStringLiteral("info")}}), "head");
      refresh(false, head);
    }
    return;
  }
  // Saved under another name: that file is the window's design now.
  if (!m_path.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit lock: %1 saved as %2").arg(fileName(), QFileInfo(path).fileName());
  }
  designClosing(path, false);
  designOpened(path, false);
}

void LockController::projectChanged() {
  if (m_path.isEmpty()) {
    return;
  }
  const ProjectState project = m_host.project();
  const bool sameProject = !m_root.isEmpty() && project.kind == ProjectState::Kind::Cloud &&
                           sameFolderPath(project.root, m_root);
  if (sameProject || (m_root.isEmpty() && m_mode == Mode::ReadOnly)) {
    return;
  }
  if (!m_root.isEmpty() && !sameFolderPath(project.root, m_root)) {
    // Another project is the current one before the design goes (New
    // Project, Open from Cloud): its lock goes when it closes
    // (designClosing).
    return;
  }
  if (!m_root.isEmpty()) {
    // No longer a Cloud project (stopped syncing).
    const QString path = m_path;
    const bool wasReadOnly = isReadOnly();
    resetDesign();
    m_path = path;
    qInfo().noquote() << QStringLiteral("Edit lock: none for %1 (the project changed)").arg(fileName());
    if (wasReadOnly) {
      m_host.readOnlyChanged();
    }
    updateIndicator();
    updateBanner();
    return;
  }
  if (m_mode == Mode::None && lockable()) {
    // Shared (Local -> Cloud): the open design takes its lock.
    const QString path = m_path;
    m_path.clear();
    designOpened(path, false);
  }
}

bool LockController::quitting() {
  if (!closeLock(true)) {
    return false;
  }
  resetDesign();
  return true;
}

void LockController::settingsChanged() {
  if (m_path.isEmpty()) {
    return;
  }
  const bool wasOn = m_locksOn;
  readSettings();
  if (!m_locksOn && m_mode != Mode::None && wasOn) {
    locksTurnedOff();
  } else if (m_locksOn && m_mode == Mode::None && lockable()) {
    const QString path = m_path;
    m_path.clear();
    designOpened(path, false);
  } else if (m_mode == Mode::Holding) {
    startHoldingTimers();
    refresh(true);
  }
}

void LockController::stoppingSync() {
  if (m_mode == Mode::None || m_root.isEmpty()) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Edit lock: the project stops syncing");
  releaseAll(tr("Releasing the edit lock of %1...").arg(fileName()));
  const QString path = m_path;
  const bool wasReadOnly = isReadOnly();
  publishLock(QJsonObject());
  if (m_live != nullptr) {
    m_live->publishOpen(m_root, m_relative, QString());
  }
  resetDesign();
  m_path = path;
  if (wasReadOnly) {
    m_host.readOnlyChanged();
  }
  updateIndicator();
  updateBanner();
}

void LockController::changesSaved() {
  m_kept = false;
  updateBanner();
}

// ---------------------------------------------------------------------------
// Taking, polling, refreshing

int LockController::pollSeconds() const { return liveConnected() ? 120 : m_projectPollSeconds; }

bool LockController::liveConnected() const {
  return m_live != nullptr && !m_root.isEmpty() && m_live->liveConnected(m_root);
}

void LockController::take(const QString& takeOver) {
  if (m_root.isEmpty() || m_relative.isEmpty()) {
    return;
  }
  QJsonObject fields{{QStringLiteral("path"), m_path},
                     {QStringLiteral("poll_seconds"), pollSeconds()},
                     {QStringLiteral("mqtt"), liveConnected()}};
  if (!takeOver.isEmpty()) {
    fields.insert(QStringLiteral("take_over"), takeOver);
  }
  qInfo().noquote() << QStringLiteral("Edit lock: taking %1%2")
                           .arg(fileName(), takeOver.isEmpty() ? QString() : QStringLiteral(" over"));
  m_pollTimer->stop();
  startTask(lockTask(QStringLiteral("lock_take"),
                     {lockCommand("lock_take", fields),
                      lockCommand("lock_status", {{QStringLiteral("network"), false}, {QStringLiteral("path"), m_path}})}),
            [this](const QJsonObject& answer) { taken(answer); });
}

void LockController::taken(const QJsonObject& answer) {
  const QString cls = errorClassOf(answer);
  const QJsonObject view = obj(answer, "view");
  if (!cls.isEmpty()) {
    const QString message = errorMessageOf(answer);
    if (cls == QLatin1String("unsupported")) {
      // The remote refuses lock refs: no locks with it.
      qInfo().noquote() << QStringLiteral("Edit locks: %1: %2").arg(fileName(), message);
      const ProjectState project = m_host.project();
      m_probes.insert(project.root + QLatin1Char('|') + project.remoteUrl, {false, message});
      const bool wasReadOnly = isReadOnly();
      m_mode = Mode::None;
      if (wasReadOnly) {
        m_host.readOnlyChanged();
      }
      m_host.status(tr("This remote does not accept Mitcad's lock references: %1 has no edit lock.").arg(fileName()),
                    true);
      updateIndicator();
      updateBanner();
      return;
    }
    qInfo().noquote() << QStringLiteral("Edit lock not confirmed (%1): %2: %3").arg(cls, fileName(), message);
    if (m_mode == Mode::Taking || m_mode == Mode::Unconfirmed) {
      m_mode = Mode::Unconfirmed;
      publishOpen();
    } else {
      m_host.status(tr("The edit lock of %1 could not be taken: %2").arg(fileName(), message), true);
    }
    updateIndicator();
    updateBanner();
    schedulePoll();
    return;
  }
  if (!view.isEmpty() && errorClassOf(view).isEmpty()) {
    const QJsonObject settings = obj(view, "settings");
    m_locksOn = settings.value(QStringLiteral("enabled")).toBool(true);
  }
  const QString outcome = str(answer, "outcome");
  const QJsonObject lock = obj(answer, "lock");
  const bool granted = std::exchange(m_granted, false);
  if (outcome == QLatin1String("taken") || outcome == QLatin1String("refreshed")) {
    m_takeRetries = 0;
    QJsonObject result = answer;
    if (granted && outcome == QLatin1String("refreshed")) {
      result.insert(QStringLiteral("outcome"), QStringLiteral("granted"));
    }
    becomeHolder(result, view);
    return;
  }
  if (outcome == QLatin1String("held")) {
    m_takeRetries = 0;
    qInfo().noquote() << QStringLiteral("Edit lock held: %1 by %2%3 (since %4)")
                             .arg(fileName(), ownerName(lock),
                                  lock.value(QStringLiteral("same_owner")).toBool() ? QStringLiteral(", another session")
                                                                                    : QString(),
                                  str(lock, "taken_at"));
    if (m_mode == Mode::Unconfirmed) {
      // Edited while the remote could not be reached: someone else holds it.
      lost(lock);
      return;
    }
    const bool first = m_mode == Mode::Taking;
    turnReadOnly(Why::HeldByOther, lock);
    if (first || !m_requested) {
      if (lock.value(QStringLiteral("same_owner")).toBool()) {
        showSameOwnerDialog(lock);
      } else {
        showHeldDialog(lock);
      }
    }
    schedulePoll();
    return;
  }
  if (outcome == QLatin1String("changed") && m_takeRetries++ < 2) {
    take();
    return;
  }
  qInfo().noquote() << QStringLiteral("Edit lock of %1 changed meanwhile: looking again").arg(fileName());
  if (m_mode == Mode::Taking) {
    m_mode = Mode::Unconfirmed;
  }
  schedulePoll(0);
}

void LockController::becomeHolder(const QJsonObject& answer, const QJsonObject& view) {
  const bool wasReadOnly = isReadOnly();
  const bool granted = str(answer, "outcome") == QLatin1String("granted");
  m_mode = Mode::Holding;
  m_idle = false;
  m_requested = false;
  m_requestState.clear();
  m_declined.clear();
  m_lostText.clear();
  m_releasedText.clear();
  m_offline = false;
  m_kept = false;
  m_lock = obj(answer, "lock");
  m_lastActivity = QDateTime::currentDateTimeUtc();
  const QJsonObject from = obj(answer, "taken_from");
  const QJsonObject previous = obj(answer, "previous");
  const QString reason = str(from, "reason");
  if (granted) {
    const QJsonObject handed = obj(m_lock, "handed_over_from");
    qInfo().noquote() << QStringLiteral("Edit lock granted: %1 by %2")
                             .arg(fileName(), str(handed, "name").isEmpty() ? ltr("its holder") : str(handed, "name"));
    m_host.status(tr("%1 handed you the edit lock of %2.")
                      .arg(str(handed, "name").isEmpty() ? tr("The holder") : str(handed, "name"), fileName()),
                  false);
  } else if (!str(from, "name").isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit lock taken: %1 from %2 (%3)").arg(fileName(), str(from, "name"), reason);
    const QString name = str(from, "name");
    if (reason == QLatin1String("no_receipt") || reason == QLatin1String("unanswered")) {
      m_host.status(tr("%1's Mitcad did not answer; the edit lock of %2 is yours.").arg(name, fileName()), false);
    } else if (reason == QLatin1String("take_over")) {
      m_host.status(tr("You took over the edit lock of %1.").arg(fileName()), false);
    } else {
      m_host.status(tr("%1's edit lock had expired (last active %2); it is yours now.")
                        .arg(name, clockText(timeOf(previous.value(QStringLiteral("active_at"))))),
                    false);
    }
  } else {
    qInfo().noquote() << QStringLiteral("Edit lock taken: %1").arg(fileName());
  }
  if (wasReadOnly) {
    m_host.readOnlyChanged();
  }
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  startHoldingTimers();
  publishLock(m_lock);
  publishOpen();
  updateIndicator();
  updateBanner();
  bringUpToDate(view);
  schedulePoll();
}

void LockController::bringUpToDate(const QJsonObject& view) {
  // The working copy follows the remote when the lock is taken: a newer
  // version of the file there is taken first (a sync; a conflict opens
  // Resolve Sync), as is the version a read-only window showed. Newer
  // versions of other files only are not (mitcad#89: the project is
  // brought up to date when the remote has a newer version of this file);
  // they wait for Sync.
  const bool checked = std::exchange(m_checkedWhileTaking, false);
  if (m_following || m_newerWhileTaking) {
    qInfo().noquote() << QStringLiteral("Edit lock: bringing %1 up to date (sync)").arg(fileName());
    m_following = false;
    m_newerWhileTaking = false;
    QTimer::singleShot(0, &m_remote, [remote = &m_remote] { remote->sync(); });
    return;
  }
  // Behind, but no check told whether this file changed since the lock was
  // being taken: a check tells (newerVersion).
  if (view.value(QStringLiteral("newer")).toBool() || (!checked && m_remote.indicatorState().behind > 0)) {
    qInfo().noquote() << QStringLiteral("Edit lock: the remote has newer versions: checking %1").arg(fileName());
    m_syncAfterCheck = true;
    m_remote.checkQuietly();
  }
}

void LockController::schedulePoll(int seconds) {
  if (m_root.isEmpty() || !m_locksOn || m_mode == Mode::None || m_mode == Mode::Taking) {
    return;
  }
  m_pollTimer->start(scaledMs(seconds < 0 ? pollSeconds() : seconds));
}

void LockController::poll() {
  if (m_root.isEmpty() || m_mode == Mode::None || m_mode == Mode::Taking) {
    return;
  }
  if (m_mode == Mode::Unconfirmed) {
    take();
    return;
  }
  if (m_task != nullptr || m_blocking > 0) {
    m_pollTimer->start(500); // after the running work
    return;
  }
  // The poll interval goes into this session's waiting requests, which the
  // poll refreshes when due (mitcad#89).
  const QJsonObject fields{{QStringLiteral("mqtt"), liveConnected()}, {QStringLiteral("poll_seconds"), pollSeconds()}};
  startTask(lockTask(QStringLiteral("lock_poll"), {lockCommand("lock_poll", fields)}),
            [this](const QJsonObject& view) { handleView(view); });
}

void LockController::handleView(const QJsonObject& view) {
  const QString cls = errorClassOf(view);
  if (!cls.isEmpty()) {
    if (!m_offline) {
      qInfo().noquote() << QStringLiteral("Edit lock poll failed (%1): %2").arg(cls, errorMessageOf(view));
    }
    m_offline = unreachable(cls);
    updateIndicator();
    schedulePoll();
    return;
  }
  if (m_offline) {
    qInfo().noquote() << QStringLiteral("Edit locks: the remote answers again");
    m_offline = false;
  }
  m_view = view;
  const QJsonObject settings = obj(view, "settings");
  m_locksOn = settings.value(QStringLiteral("enabled")).toBool(true);
  m_idleMinutes = std::clamp(settings.value(QStringLiteral("idle_minutes")).toInt(m_idleMinutes), 1, 120);
  m_projectPollSeconds = std::clamp(settings.value(QStringLiteral("poll_seconds")).toInt(m_projectPollSeconds), 5, 600);
  if (!m_locksOn) {
    locksTurnedOff();
    return;
  }
  m_dropped = view.value(QStringLiteral("dropped")).toInt();
  // Requests kept alive, and others' that went stale (their Mitcad no
  // longer refreshes them) removed by this poll (mitcad#89).
  for (const QJsonValue& value : view.value(QStringLiteral("refreshed_requests")).toArray()) {
    qInfo().noquote() << QStringLiteral("Edit lock request refreshed: %1").arg(value.toString());
  }
  for (const QJsonValue& value : view.value(QStringLiteral("removed_requests")).toArray()) {
    const QJsonObject removed = value.toObject();
    qInfo().noquote() << QStringLiteral("Edit lock: a stale request removed: %1 by %2")
                             .arg(str(removed, "path"), str(obj(removed, "requester"), "name"));
  }
  const QJsonObject lock = lockIn(view, m_fileId, m_relative);
  // Receipts the poll wrote (lock_poll): others learn of them at once.
  if (m_mode == Mode::Holding && lock.value(QStringLiteral("mine")).toBool()) {
    for (const QJsonValue& value : lock.value(QStringLiteral("requests")).toArray()) {
      const QJsonObject request = value.toObject();
      const QString id = str(request, "id");
      if (request.value(QStringLiteral("seen")).toBool() && !request.value(QStringLiteral("mine")).toBool() &&
          !m_receipts.contains(id)) {
        m_receipts.insert(id);
        qInfo().noquote() << QStringLiteral("Edit lock receipt: %1 for %2").arg(fileName(), requesterName(request));
        if (m_live != nullptr) {
          m_live->publishRequest(m_root, {{QStringLiteral("type"), QStringLiteral("receipt")},
                                          {QStringLiteral("name"), myName()},
                                          {QStringLiteral("path"), m_relative},
                                          {QStringLiteral("for"), str(request, "session")}});
        }
      }
    }
  }
  if (m_mode == Mode::Holding) {
    handleHolding(lock);
  } else if (m_mode == Mode::ReadOnly) {
    handleReadOnly(lock, view);
  }
  if (m_mode == Mode::ReadOnly && !m_kept && view.value(QStringLiteral("newer")).toBool()) {
    // The editor sent a version: the window follows it once fetched.
    const QString head = str(view, "head");
    if (head != m_checkedHead) {
      m_checkedHead = head;
      qInfo().noquote() << QStringLiteral("Edit lock: a newer version on the remote: checking %1").arg(fileName());
      m_remote.checkQuietly();
    }
  }
  updateIndicator();
  updateBanner();
  schedulePoll();
}

void LockController::handleHolding(const QJsonObject& lock) {
  if (lock.isEmpty() || !lock.value(QStringLiteral("mine")).toBool()) {
    lost(lock);
    return;
  }
  m_lock = lock;
  // The requests waiting, in the order served: others', readable, not
  // stale (a requester whose Mitcad no longer refreshes its request), and
  // not declined already (an answer of this or an earlier run stands).
  QJsonArray requests;
  for (const QJsonValue& value : lock.value(QStringLiteral("requests")).toArray()) {
    const QJsonObject request = value.toObject();
    if (!request.value(QStringLiteral("mine")).toBool() && request.value(QStringLiteral("readable")).toBool() &&
        !request.value(QStringLiteral("stale")).isString() &&
        str(obj(request, "answer"), "answer") != QLatin1String("declined")) {
      requests.append(request);
    }
  }
  if (requests.isEmpty()) {
    if (!m_shownRequest.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Edit lock request withdrawn: %1").arg(fileName());
    }
    closeAnswerDialog();
    return;
  }
  const QJsonObject first = requests.first().toObject();
  const QString id = str(first, "id");
  if (m_idle) {
    // Marked idle: a request gets the lock at once, the changes stay.
    qInfo().noquote() << QStringLiteral("Edit lock: %1 asks while %2 is idle: handing it over")
                             .arg(requesterName(first), fileName());
    QTimer::singleShot(0, this, [this, first, generation = m_generation] {
      if (generation == m_generation && m_mode == Mode::Holding) {
        handOver(first, true, QStringLiteral("idle"));
      }
    });
    return;
  }
  if (m_answered.contains(id) || m_shownRequest == id) {
    return;
  }
  showAnswerDialog(first, requests);
}

void LockController::handleReadOnly(const QJsonObject& lock, const QJsonObject& view) {
  if (!lock.isEmpty() && lock.value(QStringLiteral("mine")).toBool()) {
    // Handed over to this session: taken as its own (lock_take writes this
    // session's values and withdraws its request).
    m_granted = true;
    take();
    return;
  }
  m_lock = lock;
  QJsonObject mine = obj(lock, "my_request");
  if (mine.isEmpty()) {
    for (const QJsonValue& value : view.value(QStringLiteral("my_requests")).toArray()) {
      if (str(value.toObject(), "file_id") == m_fileId) {
        mine = value.toObject();
      }
    }
  }
  if (m_requested) {
    const QString state = lock.isEmpty() ? QStringLiteral("free") : str(mine, "state");
    if (state == QLatin1String("free") || (lock.isEmpty() && mine.isEmpty())) {
      qInfo().noquote() << QStringLiteral("Edit lock of %1 is free: taking it").arg(fileName());
      take();
      return;
    }
    if (!mine.value(QStringLiteral("stale")).isNull() && mine.contains(QStringLiteral("stale"))) {
      qInfo().noquote() << QStringLiteral("Edit lock request: %1 did not answer (%2): taking the lock")
                               .arg(ownerName(lock), str(mine, "stale"));
      take();
      return;
    }
    if (mine.isEmpty()) {
      // The request is gone (removed with the lock's requests).
      m_requested = false;
      m_requestState.clear();
      return;
    }
    if (state != m_requestState) {
      m_requestState = state;
      const QJsonObject given = obj(mine, "answer");
      if (state == QLatin1String("seen")) {
        qInfo().noquote() << QStringLiteral("Edit lock request seen by %1").arg(ownerName(lock));
      } else if (state == QLatin1String("declined")) {
        m_requested = false;
        m_declined = str(given, "message");
        qInfo().noquote() << QStringLiteral("Edit lock request declined by %1%2")
                                 .arg(ownerName(lock), m_declined.isEmpty() ? QString() : QStringLiteral(": ") + m_declined);
        m_host.status(tr("%1 declined your request for %2.").arg(ownerName(lock), fileName()), false);
      } else if (state == QLatin1String("kept")) {
        qInfo().noquote() << QStringLiteral("Edit lock request: %1 keeps the edit lock until %2")
                                 .arg(ownerName(lock), str(given, "until"));
      }
    }
    return;
  }
  if (lock.isEmpty() && m_why == Why::HeldByOther) {
    qInfo().noquote() << QStringLiteral("Edit lock of %1 released by its holder").arg(fileName());
    m_why = Why::Free;
  } else if (!lock.isEmpty() && m_why == Why::Free) {
    m_why = Why::HeldByOther;
  }
}

void LockController::refresh(bool stateChanged, const QString& base) {
  if (m_mode != Mode::Holding || m_root.isEmpty()) {
    return;
  }
  QJsonObject fields{{QStringLiteral("path"), m_path},
                     {QStringLiteral("active_at"), isoNow(m_lastActivity)},
                     {QStringLiteral("state"), m_idle ? QStringLiteral("idle") : QStringLiteral("active")},
                     {QStringLiteral("idle_minutes"), m_idleMinutes},
                     {QStringLiteral("poll_seconds"), pollSeconds()},
                     {QStringLiteral("mqtt"), liveConnected()}};
  if (!base.isEmpty()) {
    fields.insert(QStringLiteral("base"), base);
  }
  startTask(lockTask(QStringLiteral("lock_refresh"), {lockCommand("lock_refresh", fields)}),
            [this, stateChanged](const QJsonObject& answer) {
              const QString cls = errorClassOf(answer);
              if (!cls.isEmpty()) {
                if (!m_offline) {
                  qInfo().noquote() << QStringLiteral("Edit lock refresh failed (%1): %2").arg(cls, errorMessageOf(answer));
                }
                m_offline = unreachable(cls);
                updateIndicator();
                return;
              }
              if (m_mode != Mode::Holding) {
                return;
              }
              if (str(answer, "outcome") == QLatin1String("lost")) {
                lost(obj(answer, "lock"));
                return;
              }
              m_offline = false;
              m_lock = obj(answer, "lock");
              if (stateChanged) {
                qInfo().noquote() << QStringLiteral("Edit lock refreshed: %1 (%2)")
                                         .arg(fileName(), m_idle ? QStringLiteral("idle") : QStringLiteral("active"));
                publishLock(m_lock);
              }
              updateIndicator();
            });
}

void LockController::startHoldingTimers() {
  m_refreshTimer->start(scaledMs(m_idleMinutes * 60.0 / 2.0));
  m_idleTimer->start(scaledMs(m_idleMinutes * 60.0));
}

void LockController::stopTimers() {
  m_refreshTimer->stop();
  m_idleTimer->stop();
}

void LockController::activity() {
  m_lastActivity = QDateTime::currentDateTimeUtc();
  if (m_mode != Mode::Holding) {
    return;
  }
  m_idleTimer->start(scaledMs(m_idleMinutes * 60.0));
  if (m_idle) {
    m_idle = false;
    qInfo().noquote() << QStringLiteral("Edit lock: %1 active again").arg(fileName());
    refresh(true);
    updateIndicator();
  }
}

void LockController::idleTimeout() {
  if (m_mode != Mode::Holding || m_idle) {
    return;
  }
  if (m_blocking > 0 || m_host.busy() || QApplication::activeModalWidget() != nullptr ||
      QApplication::activePopupWidget() != nullptr) {
    m_idleTimer->start(1000); // after the job or the dialog
    return;
  }
  const bool modified = m_host.modified();
  qInfo().noquote() << QStringLiteral("Edit lock idle: %1 after %2 min without activity (%3)")
                           .arg(fileName())
                           .arg(m_idleMinutes)
                           .arg(modified ? QStringLiteral("unsaved changes") : QStringLiteral("no unsaved changes"));
  if (modified && !(m_host.autosaveOn() && m_host.canSaveNow())) {
    // The lock stays, marked idle; a request gets it at once.
    m_idle = true;
    qInfo().noquote() << QStringLiteral("Edit lock idle: %1 kept, marked idle (autosave %2)")
                             .arg(fileName(), m_host.autosaveOn() ? QStringLiteral("waits") : QStringLiteral("off"));
    refresh(true);
    updateIndicator();
    schedulePoll(0);
    return;
  }
  if (modified) {
    qInfo().noquote() << QStringLiteral("Edit lock idle: saving %1").arg(fileName());
    if (!m_host.saveVersion(tr("Saved automatically before releasing the edit lock")) || m_mode != Mode::Holding) {
      m_idle = true;
      refresh(true);
      updateIndicator();
      return;
    }
  }
  const QJsonObject synced = m_remote.syncNow(tr("Sending the versions of %1...").arg(fileName()));
  const QString cls = errorClassOf(synced);
  if (cls == QLatin1String("conflict") || (!cls.isEmpty() && unreachable(cls))) {
    // A sync that needs a choice keeps the lock, marked idle, until it is
    // made; offline, the lock stays too.
    m_idle = true;
    qInfo().noquote() << QStringLiteral("Edit lock idle: %1 kept, marked idle (sync: %2)").arg(fileName(), cls);
    refresh(true);
    updateIndicator();
    return;
  }
  if (m_mode != Mode::Holding) {
    return;
  }
  if (!releaseAll(tr("Releasing the edit lock of %1...").arg(fileName()))) {
    return;
  }
  m_releasedText = m_idleMinutes == 1
                       ? tr("Your edit lock was released after 1 minute without activity.")
                       : tr("Your edit lock was released after %1 minutes without activity.").arg(m_idleMinutes);
  publishLock(QJsonObject());
  turnReadOnly(Why::Released, QJsonObject());
  schedulePoll();
}

void LockController::lost(const QJsonObject& lock) {
  const QString name = ownerName(lock);
  const QJsonObject from = obj(lock, "taken_from");
  const QString reason = str(from, "reason");
  const QString at = clockText(timeOf(lock.value(QStringLiteral("taken_at"))));
  QString text;
  if (lock.isEmpty()) {
    text = tr("Your edit lock of %1 was removed.").arg(fileName());
  } else if (reason == QLatin1String("take_over") && lock.value(QStringLiteral("same_owner")).toBool()) {
    text = tr("You took over the edit lock of %1 in another window at %2.").arg(fileName(), at);
  } else if (reason == QLatin1String("take_over")) {
    text = tr("%1 took over the edit lock at %2.").arg(name, at);
  } else if (!reason.isEmpty()) {
    text = tr("%1 took the edit lock at %2 (your Mitcad did not answer).").arg(name, at);
  } else {
    text = tr("%1 has the edit lock since %2.").arg(name, at);
  }
  qInfo().noquote() << QStringLiteral("Edit lock lost: %1 to %2 (%3)")
                           .arg(fileName(), lock.isEmpty() ? QStringLiteral("nobody") : name,
                                reason.isEmpty() ? QStringLiteral("removed") : reason);
  closeAnswerDialog();
  m_lostText = text;
  turnReadOnly(Why::Lost, lock);
  if (m_kept) {
    qInfo().noquote() << QStringLiteral("Edit lock lost: the unsaved changes of %1 stay").arg(fileName());
  }
  m_host.status(text, true);
  schedulePoll();
}

void LockController::turnReadOnly(Why why, const QJsonObject& lock) {
  const bool was = isReadOnly();
  if (!was) {
    m_kept = m_host.modified();
  }
  m_mode = Mode::ReadOnly;
  m_why = why;
  m_lock = lock;
  m_idle = false;
  m_requested = false;
  m_requestState.clear();
  stopTimers();
  m_answerTimer->stop();
  m_keepTimer->stop();
  if (!was) {
    m_host.readOnlyChanged();
  }
  publishOpen();
  updateIndicator();
  updateBanner();
}

void LockController::locksTurnedOff() {
  qInfo().noquote() << QStringLiteral("Edit locks off: %1").arg(fileName());
  const bool had = m_mode == Mode::Holding || m_mode == Mode::Unconfirmed || m_requested;
  if (had && !m_root.isEmpty()) {
    // This session's locks and requests go.
    const QJsonObject answer =
        runNow(lockTask(QStringLiteral("lock_release"), {lockCommand("lock_release", {{QStringLiteral("all"), true}})}),
               tr("Releasing the edit lock of %1...").arg(fileName()));
    if (errorClassOf(answer).isEmpty()) {
      qInfo().noquote() << QStringLiteral("Edit locks off: released %1")
                               .arg(answer.value(QStringLiteral("released")).toVariant().toStringList().join(
                                   QStringLiteral(", ")));
    } else {
      qInfo().noquote() << QStringLiteral("Edit lock release failed (%1): %2")
                               .arg(errorClassOf(answer), errorMessageOf(answer));
    }
    publishLock(QJsonObject());
  }
  const bool wasReadOnly = isReadOnly();
  stopTimers();
  m_pollTimer->stop();
  closeAnswerDialog();
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  m_mode = Mode::None;
  m_lock = QJsonObject();
  m_requested = false;
  m_kept = false;
  if (wasReadOnly) {
    m_host.readOnlyChanged();
  }
  publishOpen(); // editable now
  updateIndicator();
  updateBanner();
}

// ---------------------------------------------------------------------------
// Requests and answers

QString LockController::heldText(const QJsonObject& lock) const {
  const QString name = ownerName(lock);
  if (str(lock, "state") == QLatin1String("idle")) {
    return tr("%1 is being edited by %2, who is away (idle since %3): a request gets the edit lock at once.")
        .arg(fileName(), name, clockText(timeOf(lock.value(QStringLiteral("idle_since")))));
  }
  return tr("%1 is being edited by %2 (since %3, last active %4).")
      .arg(fileName(), name, clockText(timeOf(lock.value(QStringLiteral("taken_at")))),
           agoText(timeOf(lock.value(QStringLiteral("active_at")))));
}

QString LockController::myName() {
  if (m_myName.isEmpty() && !m_root.isEmpty()) {
    m_myName = str(localCommand(m_root, {{QStringLiteral("cmd"), QStringLiteral("identity")}}), "name");
    if (m_myName.isEmpty()) {
      m_myName = VersionSettings::load().name;
    }
  }
  return m_myName.isEmpty() ? ltr("someone") : m_myName;
}

QString LockController::alsoOpenText() const {
  const QString entries = alsoOpenEntries();
  return entries.isEmpty() ? QString() : tr("Also open: %1.").arg(entries);
}

QString LockController::alsoOpenEntries() const {
  QStringList entries;
  for (auto it = m_alsoOpen.cbegin(); it != m_alsoOpen.cend(); ++it) {
    const QJsonObject entry = it.value();
    const QString mode = str(entry, "mode") == QLatin1String("editing") ? tr("editing") : tr("read-only");
    const QDateTime since = timeOf(entry.value(QStringLiteral("since")));
    entries << (since.isValid() ? tr("%1 (%2, since %3)").arg(str(entry, "name"), mode, clockText(since))
                                : tr("%1 (%2)").arg(str(entry, "name"), mode));
  }
  entries.sort();
  return entries.join(QStringLiteral(", "));
}

void LockController::showHeldDialog(const QJsonObject& lock) {
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  const QString text = heldText(lock);
  auto* box = new QMessageBox(QMessageBox::Information, tr("Edit Lock"), text, QMessageBox::NoButton, &m_window);
  box->setAttribute(Qt::WA_DeleteOnClose);
  box->setTextFormat(Qt::PlainText);
  const QString also = alsoOpenText();
  if (!also.isEmpty()) {
    box->setInformativeText(also);
  }
  QPushButton* readOnly = box->addButton(tr("&Continue Read-Only"), QMessageBox::RejectRole);
  QPushButton* ask = box->addButton(tr("&Request Edit Access..."), QMessageBox::AcceptRole);
  box->setDefaultButton(readOnly);
  qInfo().noquote() << QStringLiteral("Edit lock dialog: %1 [Continue Read-Only, Request Edit Access...]").arg(text);
  connect(box, &QMessageBox::finished, this, [this, box, ask, generation = m_generation] {
    if (generation != m_generation) {
      return;
    }
    if (box->clickedButton() == ask) {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Request Edit Access");
      QTimer::singleShot(0, this, &LockController::askForAccess);
    } else {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Continue Read-Only");
    }
  });
  m_dialog = box;
  prepareModal(box);
  box->open();
}

void LockController::showSameOwnerDialog(const QJsonObject& lock) {
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  const QString text = tr("You have this design open elsewhere (since %1, last active %2).")
                           .arg(clockText(timeOf(lock.value(QStringLiteral("taken_at")))),
                                agoText(timeOf(lock.value(QStringLiteral("active_at")))));
  auto* box = new QMessageBox(QMessageBox::Question, tr("Edit Lock"), text, QMessageBox::NoButton, &m_window);
  box->setAttribute(Qt::WA_DeleteOnClose);
  box->setTextFormat(Qt::PlainText);
  box->setInformativeText(tr("Take Over makes %1 editable here at once; the other window turns read-only and keeps "
                             "its unsaved changes.")
                              .arg(fileName()));
  QPushButton* takeOver = box->addButton(tr("&Take Over"), QMessageBox::AcceptRole);
  QPushButton* readOnly = box->addButton(tr("&Open Read-Only"), QMessageBox::RejectRole);
  box->setDefaultButton(readOnly);
  qInfo().noquote() << QStringLiteral("Edit lock dialog: %1 [Take Over, Open Read-Only]").arg(text);
  const QString commit = str(lock, "commit");
  connect(box, &QMessageBox::finished, this, [this, box, takeOver, commit, generation = m_generation] {
    if (generation != m_generation) {
      return;
    }
    if (box->clickedButton() == takeOver) {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Take Over");
      take(commit);
    } else {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Open Read-Only");
    }
  });
  m_dialog = box;
  prepareModal(box);
  box->open();
}

void LockController::askForAccess() {
  if (m_mode != Mode::ReadOnly || m_root.isEmpty()) {
    return;
  }
  const QString holder = m_lock.isEmpty() ? tr("its holder") : ownerName(m_lock);
  QDialog dialog(&m_window);
  dialog.setWindowTitle(tr("Request Edit Access"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* label = new QLabel(tr("Ask %1 for the edit lock of %2. A message (optional):").arg(holder, fileName()));
  label->setTextFormat(Qt::PlainText);
  label->setWordWrap(true);
  layout->addWidget(label);
  auto* message = new QLineEdit;
  message->setMaxLength(200);
  message->setPlaceholderText(tr("What you would like to change"));
  layout->addWidget(message);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QPushButton* send = buttons->button(QDialogButtonBox::Ok);
  send->setText(tr("&Send Request"));
  send->setDefault(true);
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  layout->addWidget(buttons);
  connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  connect(message, &QLineEdit::returnPressed, &dialog, &QDialog::accept);
  qInfo().noquote() << QStringLiteral("Request Edit Access dialog: %1, %2").arg(fileName(), holder);
  prepareModal(&dialog);
  const quint64 generation = m_generation;
  const int result = dialog.exec();
  if (result != QDialog::Accepted) {
    qInfo().noquote() << QStringLiteral("Request Edit Access cancelled");
    return;
  }
  if (generation != m_generation || m_mode != Mode::ReadOnly) {
    qInfo().noquote() << QStringLiteral("Request Edit Access: the design changed meanwhile");
    return;
  }
  sendRequest(message->text().trimmed());
}

void LockController::sendRequest(const QString& message) {
  QJsonObject fields{{QStringLiteral("path"), m_path},
                     {QStringLiteral("mqtt"), liveConnected()},
                     {QStringLiteral("poll_seconds"), pollSeconds()}};
  if (!message.isEmpty()) {
    fields.insert(QStringLiteral("message"), message);
  }
  m_declined.clear();
  startTask(lockTask(QStringLiteral("lock_request"), {lockCommand("lock_request", fields)}),
            [this, message](const QJsonObject& answer) {
              const QString cls = errorClassOf(answer);
              if (!cls.isEmpty()) {
                qInfo().noquote() << QStringLiteral("Edit lock request failed (%1): %2").arg(cls, errorMessageOf(answer));
                m_host.status(tr("The request for %1 could not be sent: %2").arg(fileName(), errorMessageOf(answer)),
                              true);
                return;
              }
              const QString outcome = str(answer, "outcome");
              if (outcome == QLatin1String("free")) {
                take();
                return;
              }
              if (outcome == QLatin1String("mine")) {
                take();
                return;
              }
              if (outcome != QLatin1String("requested")) {
                schedulePoll(0);
                return;
              }
              m_requested = true;
              m_requestState = QStringLiteral("waiting");
              const QJsonObject lock = obj(answer, "lock");
              if (!lock.isEmpty()) {
                m_lock = lock;
              }
              qInfo().noquote() << QStringLiteral("Edit lock request sent: %1 to %2%3")
                                       .arg(fileName(), ownerName(m_lock),
                                            message.isEmpty() ? QString() : QStringLiteral(": \"%1\"").arg(message));
              if (m_live != nullptr) {
                QJsonObject published{{QStringLiteral("type"), QStringLiteral("request")},
                                      {QStringLiteral("name"), myName()},
                                      {QStringLiteral("path"), m_relative}};
                if (!message.isEmpty()) {
                  published.insert(QStringLiteral("message"), message);
                }
                m_live->publishRequest(m_root, published);
              }
              updateBanner();
              updateIndicator();
              schedulePoll();
            });
}

void LockController::showAnswerDialog(const QJsonObject& request, const QJsonArray& requests) {
  closeAnswerDialog();
  m_shownRequest = str(request, "id");
  const QString name = requesterName(request);
  const QString message = str(request, "message");
  const QString text = message.isEmpty() ? tr("%1 asks to edit %2.").arg(name, fileName())
                                         : tr("%1 asks to edit %2: \"%3\"").arg(name, fileName(), message);
  QStringList waiting;
  for (int i = 1; i < requests.size(); ++i) {
    waiting << requesterName(requests.at(i).toObject());
  }
  QStringList details;
  if (!waiting.isEmpty()) {
    details << tr("Also waiting: %1.").arg(waiting.join(QStringLiteral(", ")));
  }
  const QString also = alsoOpenText();
  if (!also.isEmpty()) {
    details << also;
  }
  const bool modified = m_host.modified();
  auto* box = new QMessageBox(QMessageBox::Question, tr("Edit Access Request"), text, QMessageBox::NoButton, &m_window);
  box->setAttribute(Qt::WA_DeleteOnClose);
  box->setTextFormat(Qt::PlainText);
  box->setWindowModality(Qt::NonModal);
  if (!details.isEmpty()) {
    box->setInformativeText(details.join(QLatin1Char(' ')));
  }
  QPushButton* release = box->addButton(modified ? tr("&Release (Save First)") : tr("&Release"),
                                        QMessageBox::AcceptRole);
  QPushButton* without = modified ? box->addButton(tr("Release &Without Saving"), QMessageBox::ActionRole) : nullptr;
  QPushButton* keep = box->addButton(tr("&Keep 15 More Minutes"), QMessageBox::ActionRole);
  QPushButton* decline = box->addButton(tr("&Decline..."), QMessageBox::ActionRole);
  box->setDefaultButton(release);
  QStringList names{release->text().remove(QLatin1Char('&'))};
  if (without != nullptr) {
    names << without->text().remove(QLatin1Char('&'));
  }
  names << keep->text().remove(QLatin1Char('&')) << decline->text().remove(QLatin1Char('&'));
  qInfo().noquote() << QStringLiteral("Edit lock request: %1 [%2]").arg(text, names.join(QStringLiteral(", ")));
  if (!waiting.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit lock request: also waiting %1").arg(waiting.join(QStringLiteral(", ")));
  }
  connect(box, &QMessageBox::buttonClicked, this,
          [this, request, release, without, keep, decline, generation = m_generation](QAbstractButton* clicked) {
            if (generation != m_generation || m_mode != Mode::Holding) {
              return;
            }
            m_answerTimer->stop();
            if (clicked == release || (without != nullptr && clicked == without)) {
              const bool withoutSaving = without != nullptr && clicked == without;
              qInfo().noquote() << QStringLiteral("Edit lock request: Release%1")
                                       .arg(withoutSaving ? QStringLiteral(" Without Saving") : QString());
              QTimer::singleShot(0, this, [this, request, withoutSaving] {
                handOver(request, withoutSaving, QStringLiteral("released"));
              });
            } else if (clicked == keep) {
              qInfo().noquote() << QStringLiteral("Edit lock request: Keep 15 More Minutes");
              m_shownRequest.clear();
              sendAnswer(request, QStringLiteral("keep"), QString());
            } else if (clicked == decline) {
              qInfo().noquote() << QStringLiteral("Edit lock request: Decline");
              m_shownRequest.clear();
              QTimer::singleShot(0, this, [this, request] {
                bool ok = false;
                qInfo().noquote() << QStringLiteral("Decline dialog: %1").arg(requesterName(request));
                const QString why =
                    QInputDialog::getText(&m_window, tr("Decline"),
                                          tr("Decline %1's request (a message, optional):").arg(requesterName(request)),
                                          QLineEdit::Normal, QString(), &ok);
                sendAnswer(request, QStringLiteral("declined"), ok ? why.left(200).trimmed() : QString());
              });
            }
          });
  m_answerDialog = box;
  box->show();
  box->raise();
  m_answerTimer->start(scaledMs(m_idleMinutes * 60.0));
}

void LockController::closeAnswerDialog() {
  if (m_answerDialog != nullptr) {
    m_answerDialog->close();
  }
  m_answerDialog = nullptr;
  m_shownRequest.clear();
  m_answerTimer->stop();
}

void LockController::sendAnswer(const QJsonObject& request, const QString& kind, const QString& message) {
  const QString id = str(request, "id");
  m_answered.insert(id);
  QJsonObject fields{{QStringLiteral("path"), m_path},
                     {QStringLiteral("request"), id},
                     {QStringLiteral("answer"), kind}};
  if (kind == QLatin1String("keep")) {
    fields.insert(QStringLiteral("minutes"), 15);
  }
  if (!message.isEmpty()) {
    fields.insert(QStringLiteral("message"), message);
  }
  startTask(lockTask(QStringLiteral("lock_answer"), {lockCommand("lock_answer", fields)}),
            [this, request, kind, message, id](const QJsonObject& reply) {
              const QString cls = errorClassOf(reply);
              if (!cls.isEmpty()) {
                qInfo().noquote() << QStringLiteral("Edit lock answer failed (%1): %2").arg(cls, errorMessageOf(reply));
                m_answered.remove(id);
                return;
              }
              const QString outcome = str(reply, "outcome");
              if (outcome == QLatin1String("lost")) {
                lost(obj(reply, "lock"));
                return;
              }
              if (outcome != QLatin1String("answered")) {
                qInfo().noquote() << QStringLiteral("Edit lock answer: %1").arg(outcome);
                return;
              }
              m_lock = obj(reply, "lock");
              QString until;
              for (const QJsonValue& value : m_lock.value(QStringLiteral("requests")).toArray()) {
                if (str(value.toObject(), "id") == id) {
                  until = str(obj(value.toObject(), "answer"), "until");
                }
              }
              if (kind == QLatin1String("keep")) {
                m_keptRequest = id;
                m_keepTimer->start(scaledMs(15 * 60.0));
                qInfo().noquote() << QStringLiteral("Edit lock request of %1: kept until %2")
                                         .arg(requesterName(request), until);
              } else {
                qInfo().noquote() << QStringLiteral("Edit lock request of %1 declined%2")
                                         .arg(requesterName(request),
                                              message.isEmpty() ? QString() : QStringLiteral(": ") + message);
              }
              if (m_live != nullptr) {
                QJsonObject published{{QStringLiteral("type"), QStringLiteral("answer")},
                                      {QStringLiteral("name"), myName()},
                                      {QStringLiteral("path"), m_relative},
                                      {QStringLiteral("for"), str(request, "session")},
                                      {QStringLiteral("answer"), kind}};
                if (!until.isEmpty()) {
                  published.insert(QStringLiteral("until"), until);
                }
                if (!message.isEmpty()) {
                  published.insert(QStringLiteral("message"), message);
                }
                m_live->publishRequest(m_root, published);
              }
            });
}

void LockController::handOver(const QJsonObject& request, bool withoutSaving, const QString& why) {
  if (m_mode != Mode::Holding) {
    return;
  }
  closeAnswerDialog();
  const QString to = requesterName(request);
  qInfo().noquote() << QStringLiteral("Edit lock: handing %1 over to %2 (%3)").arg(fileName(), to, why);
  if (!withoutSaving && m_host.modified()) {
    // Saved as with Save.
    if (!m_host.canSaveNow() || !m_host.saveVersion(QString()) || m_mode != Mode::Holding) {
      qInfo().noquote() << QStringLiteral("Edit lock hand-over cancelled: %1 was not saved").arg(fileName());
      m_host.status(tr("%1 was not saved: the edit lock stays yours.").arg(fileName()), true);
      return;
    }
  }
  const QJsonObject synced = m_remote.syncNow(tr("Sending the versions of %1 to %2...").arg(fileName(), to));
  const QString cls = errorClassOf(synced);
  if (cls == QLatin1String("conflict") || (!cls.isEmpty() && unreachable(cls))) {
    qInfo().noquote() << QStringLiteral("Edit lock hand-over stopped: the sync of %1 failed (%2)").arg(fileName(), cls);
    m_idle = true;
    refresh(true);
    m_host.status(cls == QLatin1String("conflict")
                      ? tr("The versions of %1 could not be sent: Sync needs your choice first. The edit lock stays "
                           "yours, marked idle.")
                            .arg(fileName())
                      : tr("The versions of %1 could not be sent: %2").arg(fileName(), errorMessageOf(synced)),
                  true);
    updateIndicator();
    return;
  }
  if (m_mode != Mode::Holding) {
    return;
  }
  const QJsonObject handed =
      runNow(lockTask(QStringLiteral("lock_hand_over"),
                      {lockCommand("lock_hand_over", {{QStringLiteral("path"), m_path},
                                                  {QStringLiteral("to"), str(request, "session")}})}),
             tr("Handing the edit lock of %1 over to %2...").arg(fileName(), to));
  const QString failure = errorClassOf(handed);
  if (!failure.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit lock hand-over failed (%1): %2").arg(failure, errorMessageOf(handed));
    m_host.status(tr("The edit lock of %1 could not be handed over: %2").arg(fileName(), errorMessageOf(handed)), true);
    schedulePoll();
    return;
  }
  const QString outcome = str(handed, "outcome");
  if (outcome == QLatin1String("handed_over")) {
    qInfo().noquote() << QStringLiteral("Edit lock handed over: %1 to %2").arg(fileName(), to);
    if (m_live != nullptr) {
      m_live->publishRequest(m_root, {{QStringLiteral("type"), QStringLiteral("answer")},
                                      {QStringLiteral("name"), myName()},
                                      {QStringLiteral("path"), m_relative},
                                      {QStringLiteral("for"), str(request, "session")},
                                      {QStringLiteral("answer"), QStringLiteral("released")}});
    }
    const QJsonObject lock = obj(handed, "lock");
    publishLock(lock);
    m_lostText = tr("You handed the edit lock of %1 to %2.").arg(fileName(), to);
    turnReadOnly(Why::HandedOver, lock);
    if (m_kept) {
      qInfo().noquote() << QStringLiteral("Edit lock handed over: the unsaved changes of %1 stay").arg(fileName());
    }
    updateBanner();
    schedulePoll();
    return;
  }
  if (outcome == QLatin1String("lost") || outcome == QLatin1String("free")) {
    lost(obj(handed, "lock"));
    return;
  }
  qInfo().noquote() << QStringLiteral("Edit lock hand-over: %1").arg(outcome);
  schedulePoll(0);
}

bool LockController::releaseAll(const QString& label) {
  if (m_root.isEmpty()) {
    return true;
  }
  const QJsonObject answer =
      runNow(lockTask(QStringLiteral("lock_release"), {lockCommand("lock_release", {{QStringLiteral("all"), true}})}), label);
  const QString cls = errorClassOf(answer);
  if (!cls.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Edit lock release failed (%1): %2").arg(cls, errorMessageOf(answer));
    return false;
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("released")).toArray()) {
    qInfo().noquote() << QStringLiteral("Edit lock released: %1").arg(value.toString());
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("withdrawn")).toArray()) {
    qInfo().noquote() << QStringLiteral("Edit lock request withdrawn: %1").arg(value.toString());
    if (m_live != nullptr && !value.toString().isEmpty()) {
      m_live->publishRequest(m_root, {{QStringLiteral("type"), QStringLiteral("withdrawn")},
                                      {QStringLiteral("path"), value.toString()}});
    }
  }
  return true;
}

bool LockController::closeLock(bool quit) {
  if (m_root.isEmpty()) {
    return true;
  }
  if (m_mode == Mode::None) {
    // No lock (edit locks off): who has the design open is told still.
    if (m_live != nullptr) {
      m_live->publishOpen(m_root, m_relative, QString());
    }
    return true;
  }
  m_pollTimer->stop();
  stopTimers();
  closeAnswerDialog();
  if (m_dialog != nullptr) {
    m_dialog->close();
  }
  bool release = m_mode == Mode::Holding || m_mode == Mode::Unconfirmed || m_mode == Mode::Taking || m_requested;
  // The versions are sent while the design's project is the current one
  // (the remote's work is the current project's); another project opened
  // already, they wait to be sent there.
  if (m_mode == Mode::Holding && sameFolderPath(m_host.project().root, m_root)) {
    const QJsonObject synced = m_remote.syncNow(tr("Sending the versions of %1...").arg(fileName()));
    const QString cls = errorClassOf(synced);
    if (cls == QLatin1String("conflict")) {
      qInfo().noquote() << QStringLiteral("Edit lock: the sync of %1 needs a choice before the release: asked")
                               .arg(fileName());
      QMessageBox box(QMessageBox::Question, tr("Edit Lock"),
                      tr("The versions of %1 cannot be sent: the remote has newer versions of files you changed too, "
                         "and a sync needs your choice.")
                          .arg(fileName()),
                      QMessageBox::NoButton, &m_window);
      box.setInformativeText(tr("Without them sent, others do not see your changes when they get the edit lock."));
      QPushButton* resolve = box.addButton(tr("&Resolve Sync"), QMessageBox::AcceptRole);
      QPushButton* without = box.addButton(tr("Release &Without Sending"), QMessageBox::DestructiveRole);
      QPushButton* keep = box.addButton(tr("&Keep Lock"), QMessageBox::RejectRole);
      box.setDefaultButton(resolve);
      prepareModal(&box);
      box.exec();
      if (box.clickedButton() == resolve) {
        qInfo().noquote() << QStringLiteral("Edit lock: Resolve Sync");
        startHoldingTimers();
        schedulePoll();
        QTimer::singleShot(0, &m_remote, [remote = &m_remote] { remote->sync(); });
        return false;
      }
      if (box.clickedButton() == without) {
        qInfo().noquote() << QStringLiteral("Edit lock: Release Without Sending");
      } else {
        qInfo().noquote() << QStringLiteral("Edit lock: Keep Lock");
        (void)keep;
        release = false;
      }
    } else if (!cls.isEmpty() && unreachable(cls)) {
      // Offline: the lock stays until it is stale.
      qInfo().noquote() << QStringLiteral("Edit lock stays: %1 (%2)").arg(fileName(), cls);
      release = false;
    }
  }
  if (release) {
    releaseAll(quit ? tr("Releasing the edit lock of %1...").arg(fileName())
                        : tr("Closing %1: releasing its edit lock...").arg(fileName()));
    if (m_mode == Mode::Holding) {
      publishLock(QJsonObject());
    }
  }
  if (m_live != nullptr) {
    m_live->publishOpen(m_root, m_relative, QString());
  }
  return true;
}

void LockController::releaseByUser() {
  if (m_mode != Mode::Holding) {
    return;
  }
  if (m_host.modified()) {
    QMessageBox box(QMessageBox::Question, tr("Release Edit Lock"),
                    tr("%1 has unsaved changes. Save them before releasing the edit lock?").arg(fileName()),
                    QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, &m_window);
    box.button(QMessageBox::Save)->setText(tr("&Save and Release"));
    box.button(QMessageBox::Discard)->setText(tr("Release &Without Saving"));
    prepareModal(&box);
    const int chosen = box.exec();
    if (chosen == QMessageBox::Cancel) {
      return;
    }
    if (chosen == QMessageBox::Save && (!m_host.saveVersion(QString()) || m_mode != Mode::Holding)) {
      return;
    }
  }
  const QJsonObject synced = m_remote.syncNow(tr("Sending the versions of %1...").arg(fileName()));
  const QString cls = errorClassOf(synced);
  if (cls == QLatin1String("conflict")) {
    m_host.status(tr("The versions of %1 could not be sent: Sync needs your choice first.").arg(fileName()), true);
    return;
  }
  if (!releaseAll(tr("Releasing the edit lock of %1...").arg(fileName()))) {
    m_host.status(tr("The edit lock of %1 could not be released.").arg(fileName()), true);
    return;
  }
  m_releasedText = tr("You released the edit lock of %1.").arg(fileName());
  publishLock(QJsonObject());
  turnReadOnly(Why::Released, QJsonObject());
  schedulePoll();
}

void LockController::showDetails() {
  QStringList lines;
  lines << tr("Design: %1").arg(m_relative.isEmpty() ? fileName() : m_relative);
  if (m_mode == Mode::Holding) {
    lines << tr("Edit lock: yours (this window), since %1%2")
                 .arg(clockText(timeOf(m_lock.value(QStringLiteral("taken_at")))),
                      m_idle ? tr(", marked idle") : QString());
  } else if (!m_lock.isEmpty()) {
    lines << tr("Edit lock: %1 <%2>, since %3, last active %4%5")
                 .arg(ownerName(m_lock), str(obj(m_lock, "owner"), "email"),
                      clockText(timeOf(m_lock.value(QStringLiteral("taken_at")))),
                      agoText(timeOf(m_lock.value(QStringLiteral("active_at")))),
                      str(m_lock, "state") == QLatin1String("idle") ? tr(" (idle)") : QString());
  } else if (m_mode == Mode::Unconfirmed) {
    lines << tr("Edit lock: not confirmed (the remote could not be reached)");
  } else {
    lines << tr("Edit lock: none");
  }
  QStringList requests;
  for (const QJsonValue& value : m_lock.value(QStringLiteral("requests")).toArray()) {
    const QJsonObject request = value.toObject();
    if (request.value(QStringLiteral("mine")).toBool()) {
      requests << tr("yours");
    } else if (request.value(QStringLiteral("stale")).isString()) {
      requests << tr("%1 (stale: their Mitcad no longer asks)").arg(requesterName(request));
    } else {
      requests << requesterName(request);
    }
  }
  if (!requests.isEmpty()) {
    lines << tr("Requests: %1").arg(requests.join(QStringLiteral(", ")));
  }
  const QString also = alsoOpenText();
  lines << (also.isEmpty() ? tr("Also open: nobody else known (live updates show it)") : also);
  const QString live = m_host.liveDetails && !m_root.isEmpty() ? m_host.liveDetails(m_root) : QString();
  if (live.isEmpty()) {
    lines << tr("Live updates: not connected; the remote is read every %1 s").arg(pollSeconds());
  } else {
    lines << live << tr("The remote is read every %1 s").arg(pollSeconds());
  }
  lines << tr("Malformed lock references dropped: %1; live messages dropped: %2").arg(m_dropped).arg(m_liveDropped);
  qInfo().noquote() << QStringLiteral("Edit lock details: %1").arg(lines.join(QStringLiteral(" | ")));
  QMessageBox box(QMessageBox::Information, tr("Edit Lock Details"), lines.join(QLatin1Char('\n')), QMessageBox::Close,
                  &m_window);
  box.setTextFormat(Qt::PlainText);
  prepareModal(&box);
  box.exec();
}

// ---------------------------------------------------------------------------
// Sync, newer versions, the probe

bool LockController::confirmSend() {
  const ProjectState project = m_host.project();
  if (project.kind != ProjectState::Kind::Cloud) {
    return true;
  }
  const QJsonObject plan = localCommand(project.root, {{QStringLiteral("cmd"), QStringLiteral("sync_plan")},
                                                       {QStringLiteral("fetch"), false},
                                                       {QStringLiteral("locks"), QStringLiteral("last")},
                                                       {QStringLiteral("session"), m_host.session().toLower()}});
  const QJsonArray locked = plan.value(QStringLiteral("locked")).toArray();
  if (locked.isEmpty()) {
    return true;
  }
  QStringList files;
  for (const QJsonValue& value : locked) {
    const QJsonObject entry = value.toObject();
    const QJsonObject lock = obj(entry, "lock");
    files << tr("%1 (%2)").arg(str(entry, "path"),
                               lock.value(QStringLiteral("same_owner")).toBool() ? tr("you, in another window")
                                                                                  : ownerName(lock));
  }
  qInfo().noquote() << QStringLiteral("Sync: files someone else holds the edit lock of: %1: asked")
                           .arg(files.join(QStringLiteral(", ")));
  QMessageBox box(QMessageBox::Warning, tr("Sync"),
                  tr("Someone else holds the edit lock of %n file(s) whose versions Sync would send: %1.", nullptr,
                     static_cast<int>(files.size()))
                      .arg(files.join(QStringLiteral(", "))),
                  QMessageBox::Cancel, &m_window);
  box.setInformativeText(tr("Sent anyway, the editor's next sync asks which version to keep."));
  QPushButton* send = box.addButton(tr("Send &Anyway"), QMessageBox::AcceptRole);
  box.setDefaultButton(QMessageBox::Cancel);
  prepareModal(&box);
  box.exec();
  const bool anyway = box.clickedButton() == send;
  qInfo().noquote() << (anyway ? QStringLiteral("Sync: Send Anyway") : QStringLiteral("Sync: not sent (locked files)"));
  return anyway;
}

bool LockController::newerVersion(const QString& file, const QJsonObject& incoming) {
  if (!samePath(file, m_path)) {
    return false;
  }
  const QJsonArray versions = incoming.value(QStringLiteral("file_versions")).toArray();
  const bool differs = incoming.value(QStringLiteral("differs")).toBool() && !versions.isEmpty();
  if (m_mode == Mode::Taking) {
    // Brought up to date once the lock is taken.
    m_newerWhileTaking = differs;
    m_checkedWhileTaking = true;
    return differs;
  }
  if (m_mode == Mode::Holding) {
    if (!m_syncAfterCheck) {
      return false;
    }
    m_syncAfterCheck = false;
    if (differs) {
      qInfo().noquote() << QStringLiteral("Edit lock: bringing %1 up to date (sync)").arg(fileName());
      QTimer::singleShot(0, &m_remote, [remote = &m_remote] { remote->sync(); });
      return true;
    }
    return false;
  }
  if (m_mode != Mode::ReadOnly || !differs || m_kept) {
    return false;
  }
  // A read-only window shows the editor's version as it comes.
  const QJsonObject newest = versions.first().toObject();
  const QString name = str(obj(newest, "author"), "name");
  const QString at = clockText(QDateTime::fromSecsSinceEpoch(newest.value(QStringLiteral("time")).toInteger()));
  QString error;
  if (!m_host.showVersion(str(newest, "id"), error)) {
    qInfo().noquote() << QStringLiteral("Edit lock: the newer version of %1 could not be shown: %2").arg(fileName(), error);
    return false;
  }
  m_following = true;
  const QString text = tr("Updated to %1's version saved at %2.").arg(name, at);
  qInfo().noquote() << QStringLiteral("Updated to %1's version saved at %2.").arg(name, at);
  m_host.status(text, false);
  return true;
}

void LockController::probe(QObject* context, std::function<void(bool accepted, const QString& message)> done) {
  const ProjectState project = m_host.project();
  if (project.kind != ProjectState::Kind::Cloud) {
    return;
  }
  const QString key = project.root + QLatin1Char('|') + project.remoteUrl;
  const QPointer<QObject> guard(context);
  if (m_probes.contains(key)) {
    const std::pair<bool, QString> known = m_probes.value(key);
    QTimer::singleShot(0, this, [guard, done, known] {
      if (guard != nullptr) {
        done(known.first, known.second);
      }
    });
    return;
  }
  const QByteArray root = project.root.toUtf8();
  const QByteArray json = compactJson(lockCommand("lock_probe"));
  RemoteTask* task = RemoteTask::custom(
      QStringLiteral("lock_probe"),
      [root, json](const SyncControl& control) {
        const rust::Box<Project> repository = open_project(rustStr(root));
        return parseObject(repository->command_with(rustStr(json), control));
      },
      this);
  connect(task, &RemoteTask::finished, this, [this, task, key, guard, done] {
    const QJsonObject answer = task->answer();
    task->deleteLater();
    const QString cls = errorClassOf(answer);
    if (!cls.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Edit locks probe failed (%1): %2").arg(cls, errorMessageOf(answer));
      return; // asked again next time
    }
    const bool accepted = answer.value(QStringLiteral("accepted")).toBool();
    const QString message = str(answer, "message");
    m_probes.insert(key, {accepted, message});
    qInfo().noquote() << (accepted ? QStringLiteral("Edit locks probe: the remote accepts Mitcad's lock references")
                                   : QStringLiteral("Edit locks probe: %1 (%2)").arg(message, str(answer, "reason")));
    if (guard != nullptr) {
      done(accepted, message);
    }
  });
  task->start();
}

// ---------------------------------------------------------------------------
// Live updates

void LockController::liveEvent(const QString& root, const QJsonObject& event) {
  if (m_root.isEmpty() || !sameFolderPath(root, m_root)) {
    return;
  }
  const QString type = str(event, "type");
  const QString me = m_host.session().toLower();
  if (type == QLatin1String("lock") || type == QLatin1String("request") || type == QLatin1String("version")) {
    if (!event.value(QStringLiteral("own")).toBool()) {
      schedulePoll(0); // only the refs grant a lock: read them at once
    }
    return;
  }
  if (type == QLatin1String("open")) {
    const QString session = str(event, "session");
    if (str(event, "file") != m_fileId || session == me) {
      return;
    }
    const QJsonValue entry = event.value(QStringLiteral("entry"));
    if (entry.isObject()) {
      m_alsoOpen.insert(session, entry.toObject());
    } else {
      m_alsoOpen.remove(session);
    }
    const QString entries = alsoOpenEntries();
    qInfo().noquote() << QStringLiteral("Edit lock: also open: %1")
                             .arg(entries.isEmpty() ? QStringLiteral("nobody else") : entries);
    updateBanner();
    return;
  }
  if (type == QLatin1String("session")) {
    const QString session = str(event, "session");
    const QString state = str(event, "state");
    if (state == QLatin1String("offline") || state == QLatin1String("left")) {
      m_alsoOpen.remove(session);
      updateBanner();
    }
    if (state == QLatin1String("offline") && m_mode == Mode::ReadOnly && str(m_lock, "session") == session) {
      offlineWill(session, event);
    }
    return;
  }
  if (type == QLatin1String("subscribed")) {
    m_alsoOpen.clear();
    schedulePoll(0);
    return;
  }
  if (type == QLatin1String("dropped")) {
    m_liveDropped = event.value(QStringLiteral("count")).toInt();
  }
}

void LockController::liveStateChanged(const QString& root, bool connected) {
  if (m_root.isEmpty() || !sameFolderPath(root, m_root)) {
    return;
  }
  qInfo().noquote() << QStringLiteral("Edit locks: live updates %1: the remote is read every %2 s")
                           .arg(connected ? QStringLiteral("connected") : QStringLiteral("not connected"))
                           .arg(pollSeconds());
  if (m_mode == Mode::Holding) {
    refresh(true); // the lock's poll interval and `mqtt`
  }
  schedulePoll(0);
}

void LockController::offlineWill(const QString& session, const QJsonObject& event) {
  if (m_offlineAsked.contains(session) || m_dialog != nullptr) {
    return;
  }
  m_offlineAsked.insert(session);
  const QString name = ownerName(m_lock);
  const QString at = clockText(timeOf(event.value(QStringLiteral("since"))));
  const QString text = tr("%1's Mitcad lost its connection at %2. Take the edit lock of %3?").arg(name, at, fileName());
  auto* box = new QMessageBox(QMessageBox::Question, tr("Edit Lock"), text, QMessageBox::NoButton, &m_window);
  box->setAttribute(Qt::WA_DeleteOnClose);
  box->setTextFormat(Qt::PlainText);
  QPushButton* takeIt = box->addButton(tr("&Take the Edit Lock"), QMessageBox::AcceptRole);
  box->addButton(tr("&Not Now"), QMessageBox::RejectRole);
  qInfo().noquote() << QStringLiteral("Edit lock dialog: %1 [Take the Edit Lock, Not Now]").arg(text);
  const QString commit = str(m_lock, "commit");
  connect(box, &QMessageBox::finished, this, [this, box, takeIt, commit, generation = m_generation] {
    if (generation != m_generation) {
      return;
    }
    if (box->clickedButton() == takeIt) {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Take the Edit Lock");
      take(commit);
    } else {
      qInfo().noquote() << QStringLiteral("Edit lock dialog: Not Now");
    }
  });
  m_dialog = box;
  prepareModal(box);
  box->open();
}

// ---------------------------------------------------------------------------
// What shows

void LockController::publishOpen() {
  if (m_live == nullptr || m_root.isEmpty()) {
    return;
  }
  // A design of a Cloud project (m_root) is open, editable or read-only,
  // with or without edit locks.
  m_live->publishOpen(m_root, m_relative,
                      m_mode == Mode::ReadOnly ? QStringLiteral("read-only") : QStringLiteral("editing"));
}

void LockController::publishLock(const QJsonObject& lock) {
  if (m_live != nullptr && !m_root.isEmpty()) {
    m_live->publishLock(m_root, m_relative, lock);
  }
}

void LockController::updateIndicator() {
  QString text;
  QString state;
  switch (m_mode) {
  case Mode::None:
    state = QStringLiteral("none");
    break;
  case Mode::Taking:
    state = QStringLiteral("taking");
    break;
  case Mode::Holding:
    if (m_offline) {
      text = tr("Edit lock not confirmed (offline)");
    } else {
      text = m_idle ? tr("Editing (idle)") : tr("Editing");
    }
    state = m_idle ? QStringLiteral("editing, idle") : QStringLiteral("editing");
    break;
  case Mode::Unconfirmed:
    text = tr("Edit lock not confirmed (offline)");
    state = QStringLiteral("not confirmed");
    break;
  case Mode::ReadOnly:
    if (!m_lock.isEmpty() && !m_lock.value(QStringLiteral("mine")).toBool()) {
      text = tr("Read-only (%1)").arg(ownerName(m_lock));
    } else {
      text = tr("Read-only");
    }
    state = QStringLiteral("read-only");
    break;
  }
  m_indicator.setLockText(text);
  m_indicator.setLockActions(actions());
  const QString line = QStringLiteral("%1 %2").arg(state, fileName());
  if (line != m_loggedState) {
    m_loggedState = line;
    qInfo().noquote() << QStringLiteral("Edit lock state: %1%2")
                             .arg(state, m_path.isEmpty() ? QString() : QStringLiteral(", ") + fileName());
  }
}

void LockController::updateBanner() {
  if (m_bar == nullptr) {
    return;
  }
  if (!isReadOnly()) {
    if (m_bar->isVisible()) {
      qInfo().noquote() << QStringLiteral("Lock banner: hidden");
    }
    m_bar->hide();
    m_banner->clear();
    return;
  }
  const QString request = tr("&Request Edit Access...");
  const QString edit = tr("&Edit");
  const QString copy = tr("Save as &Copy...");
  const QString version = tr("Save as New &Version");
  QString text;
  QStringList buttons;
  const bool canRequest = !m_root.isEmpty();
  switch (m_why) {
  case Why::HeldByOther:
    if (m_lock.isEmpty()) {
      text = tr("Read-only: nobody is editing %1 now.").arg(fileName());
      buttons << edit;
    } else if (m_requested && m_requestState == QLatin1String("kept")) {
      text = tr("Read-only: %1 keeps the edit lock until %2.")
                 .arg(ownerName(m_lock),
                      clockText(timeOf(obj(obj(m_lock, "my_request"), "answer").value(QStringLiteral("until")))));
    } else if (m_requested) {
      text = tr("Read-only: waiting for %1's answer.").arg(ownerName(m_lock));
    } else if (!m_declined.isEmpty() || m_requestState == QLatin1String("declined")) {
      text = m_declined.isEmpty() ? tr("Read-only: %1 declined your request.").arg(ownerName(m_lock))
                                  : tr("Read-only: %1 declined your request: \"%2\"").arg(ownerName(m_lock), m_declined);
      buttons << request;
    } else if (str(m_lock, "state") == QLatin1String("idle")) {
      text = tr("Read-only: %1 is away (idle since %2).")
                 .arg(ownerName(m_lock), clockText(timeOf(m_lock.value(QStringLiteral("idle_since")))));
      buttons << request;
    } else {
      text = tr("Read-only: %1 is editing (since %2, last active %3).")
                 .arg(ownerName(m_lock), clockText(timeOf(m_lock.value(QStringLiteral("taken_at")))),
                      agoText(timeOf(m_lock.value(QStringLiteral("active_at")))));
      buttons << request;
    }
    break;
  case Why::Free:
    text = tr("Read-only: nobody is editing %1 now.").arg(fileName());
    buttons << edit;
    break;
  case Why::Opened:
    text = tr("Read-only: %1 was opened without an edit lock.").arg(fileName());
    buttons << edit;
    break;
  case Why::Released:
    text = m_releasedText;
    buttons << edit;
    break;
  case Why::Lost:
  case Why::HandedOver:
    text = m_lostText;
    if (m_requested) {
      text += QLatin1Char(' ') + tr("Waiting for %1's answer.").arg(ownerName(m_lock));
    } else if (!m_declined.isEmpty() || m_requestState == QLatin1String("declined")) {
      text += QLatin1Char(' ') + (m_declined.isEmpty() ? tr("%1 declined your request.").arg(ownerName(m_lock))
                                                       : tr("%1 declined your request: \"%2\"").arg(ownerName(m_lock),
                                                                                                   m_declined));
    }
    if (m_kept) {
      buttons << copy << version;
    }
    if (!m_requested) {
      buttons << (m_lock.isEmpty() ? edit : request);
    }
    break;
  }
  if (!canRequest) {
    buttons.removeAll(request);
  }
  if (m_kept && !buttons.contains(copy)) {
    buttons.prepend(copy);
  }
  const QString also = alsoOpenText();
  if (!also.isEmpty()) {
    text += QLatin1Char(' ') + also;
  }
  m_banner->present(text, buttons, [this](const QString& name) {
    if (name == QLatin1String("Request Edit Access...")) {
      askForAccess();
    } else if (name == QLatin1String("Edit")) {
      if (m_root.isEmpty()) {
        // No lock to take (a design opened read-only outside Cloud projects).
        m_mode = Mode::None;
        m_kept = false;
        m_host.readOnlyChanged();
        updateIndicator();
        updateBanner();
      } else {
        take();
      }
    } else if (name == QLatin1String("Save as Copy...")) {
      m_host.saveAsCopy();
    } else if (name == QLatin1String("Save as New Version")) {
      m_host.saveAsNewVersion();
    }
  });
  m_bar->show();
}

} // namespace mitcad
