// SPDX-License-Identifier: MIT
#include "Autosave.hpp"

#include <algorithm>
#include <exception>
#include <utility>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLockFile>
#include <QSaveFile>
#include <QUuid>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/Diagnostics.hpp"

namespace mitcad {
namespace {

// A write that could not be made now (the model busy, the last write still
// going) is tried again this much later at most.
constexpr int kRetryMs = 5000;

// Writes the whole file or nothing: QSaveFile writes a temporary file next
// to it, and commit() flushes that to disk and renames it over the file.
bool writeAtomically(const QString& path, const QByteArray& data, QString& error) {
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
    // Not committed, the temporary file goes with `file`.
    error = QStringLiteral("%1: %2").arg(QDir::toNativeSeparators(path), file.errorString());
    return false;
  }
  return true;
}

} // namespace

QString fileDigest(QByteArrayView data) {
  quint64 hash = 0xcbf29ce484222325ULL;
  for (const char byte : data) {
    hash ^= static_cast<unsigned char>(byte);
    hash *= 0x100000001b3ULL;
  }
  return QStringLiteral("%1").arg(hash, 16, 16, QLatin1Char('0'));
}

void removeSessionFiles(const QString& directory, const QString& session) {
  const QDir dir(directory);
  QFile::remove(dir.filePath(session + QLatin1String(".json")));
  QFile::remove(dir.filePath(session + QLatin1String(".mitcad")));
  // QSaveFile's temporary files ("<id>.mitcad.AbCdEf") where a write was
  // cut off; the lock ("<id>.lock", and QLockFile's "<id>.lock.rmlock")
  // is its owner's.
  const QString lock = session + QLatin1String(".lock");
  for (const QString& name : dir.entryList({session + QLatin1String(".*")}, QDir::Files | QDir::Hidden)) {
    if (!name.startsWith(lock)) {
      QFile::remove(dir.filePath(name));
    }
  }
}

AutosaveManager::AutosaveManager(QObject* parent)
    : QObject(parent), m_directory(recoveryDirectory()),
      m_session(QUuid::createUuid().toString(QUuid::WithoutBraces)) {
  m_pool.setMaxThreadCount(1);
  connect(&m_timer, &QTimer::timeout, this, &AutosaveManager::tick);
  m_retry.setSingleShot(true);
  connect(&m_retry, &QTimer::timeout, this, &AutosaveManager::tick);
}

AutosaveManager::~AutosaveManager() {
  m_timer.stop();
  discard();
  if (m_lock) {
    m_lock->unlock(); // removes the lock file
  }
}

void AutosaveManager::apply(const GeneralSettings& settings) {
  const int seconds = testAutosaveSeconds() > 0 ? testAutosaveSeconds() : settings.autosaveMinutes * 60;
  const bool enabled = settings.autosave && !m_directory.isEmpty();
  if (m_applied && enabled == m_enabled && (!enabled || m_timer.interval() == seconds * 1000)) {
    return; // the countdown goes on
  }
  m_applied = true;
  m_enabled = enabled;
  if (!enabled) {
    m_timer.stop();
    m_retry.stop();
    qDebug().noquote() << (m_directory.isEmpty() ? QStringLiteral("Autosave off: no recovery folder")
                                                 : QStringLiteral("Autosave off"));
    return;
  }
  m_timer.start(seconds * 1000);
  qDebug().noquote() << QStringLiteral("Autosave every %1 s to %2").arg(QString::number(seconds), m_directory);
}

void AutosaveManager::replaced() {
  discard();
  m_baseDigest.clear();
}

void AutosaveManager::saved(const QString& digest) {
  discard();
  m_baseDigest = digest;
}

void AutosaveManager::saveSoon() {
  if (m_enabled) {
    m_retry.start(0);
  }
}

void AutosaveManager::recovered(const QString& baseDigest, const QString& session,
                                std::unique_ptr<QLockFile> lock) {
  releaseRecovered();
  m_baseDigest = baseDigest;
  m_recoveredSession = session;
  m_recoveredLock = std::move(lock);
}

void AutosaveManager::releaseRecovered() {
  if (m_recoveredSession.isEmpty()) {
    return;
  }
  removeSessionFiles(m_directory, m_recoveredSession);
  if (m_recoveredLock) {
    m_recoveredLock->unlock(); // removes the lock file
    m_recoveredLock.reset();
  }
  qDebug().noquote() << QStringLiteral("Recovered session %1 removed").arg(m_recoveredSession);
  m_recoveredSession.clear();
}

void AutosaveManager::retryLater() { m_retry.start(std::min(kRetryMs, m_timer.interval())); }

void AutosaveManager::tick() {
  m_retry.stop();
  if (!m_enabled || !state || !snapshot) {
    return;
  }
  const QString reason = blocked ? blocked() : QString();
  if (!reason.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Autosave skipped: %1").arg(reason);
    retryLater();
    return;
  }
  if (m_writing) {
    retryLater(); // the last write is still going
    return;
  }
  State now;
  Snapshot taken;
  try {
    now = state();
    if (!now.modified) {
      // As on disk (saved, or undone back to that): nothing to recover.
      discard();
      return;
    }
    if (m_written == now.revision || !lock()) {
      return;
    }
    ScopedTiming timing("autosave");
    taken = snapshot();
    timing.setDetail(QStringLiteral("%1 bytes").arg(taken.project.size()));
  } catch (const std::exception& e) {
    // Nothing may leave a timer's slot; the next tick tries again.
    qWarning().noquote() << QStringLiteral("Autosave failed: %1").arg(QString::fromUtf8(e.what()));
    return;
  }
  const QString project = filePath(".mitcad");
  QJsonObject about{
      {QStringLiteral("format"), QStringLiteral("mitcad-autosave")},
      {QStringLiteral("version"), 1},
      {QStringLiteral("session"), m_session},
      {QStringLiteral("pid"), QCoreApplication::applicationPid()},
      {QStringLiteral("application"), QCoreApplication::applicationName()},
      {QStringLiteral("application_version"), QCoreApplication::applicationVersion()},
      {QStringLiteral("document"), taken.name},
      {QStringLiteral("path"), taken.path},
      {QStringLiteral("base_digest"), m_baseDigest},
      {QStringLiteral("saved_at"), QDateTime::currentDateTimeUtc().toString(Qt::ISODate)},
      {QStringLiteral("revision"), static_cast<qint64>(now.revision)},
      {QStringLiteral("project"), QFileInfo(project).fileName()},
  };
  m_writing = true;
  m_haveFiles = true;
  m_writtenName = taken.name;
  const quint64 generation = m_generation;
  const quint64 revision = now.revision;
  m_pool.start([this, generation, revision, project, metadata = filePath(".json"), name = taken.name,
                data = std::move(taken.project), about = std::move(about)]() mutable {
    // The project file first: the metadata, written last, marks the pair
    // complete (and tells the project file's size and digest).
    QString error;
    bool ok = writeAtomically(project, data, error);
    if (ok) {
      about.insert(QStringLiteral("size"), static_cast<qint64>(data.size()));
      about.insert(QStringLiteral("digest"), fileDigest(data));
      ok = writeAtomically(metadata, QJsonDocument(about).toJson(QJsonDocument::Indented), error);
    }
    if (ok) {
      qDebug().noquote() << QStringLiteral("Autosaved %1: %2 bytes to %3")
                                .arg(name, QString::number(data.size()), project);
    } else {
      qWarning().noquote() << QStringLiteral("Autosave of %1 failed: %2").arg(name, error);
    }
    QMetaObject::invokeMethod(
        this, [this, generation, revision, ok] { written(generation, revision, ok); }, Qt::QueuedConnection);
  });
}

void AutosaveManager::written(quint64 generation, quint64 revision, bool ok) {
  if (generation != m_generation) {
    return; // discarded meanwhile
  }
  m_writing = false;
  if (ok) {
    m_written = revision;
    // This session keeps the recovered document now.
    releaseRecovered();
  }
}

bool AutosaveManager::lock() {
  if (m_lock) {
    return true;
  }
  if (!QDir().mkpath(m_directory)) {
    qWarning().noquote() << QStringLiteral("Autosave failed: cannot make %1")
                                .arg(QDir::toNativeSeparators(m_directory));
    return false;
  }
  auto lock = std::make_unique<QLockFile>(filePath(".lock"));
  // Stale only when its process is gone, however long the session ran.
  lock->setStaleLockTime(0);
  if (!lock->tryLock(0)) {
    qWarning().noquote() << QStringLiteral("Autosave failed: cannot lock %1 (error %2)")
                                .arg(QDir::toNativeSeparators(filePath(".lock")),
                                     QString::number(static_cast<int>(lock->error())));
    return false;
  }
  m_lock = std::move(lock);
  return true;
}

void AutosaveManager::discard() {
  m_retry.stop();
  m_pool.waitForDone(); // a write going on ends first
  ++m_generation;
  m_writing = false;
  m_written.reset();
  releaseRecovered();
  if (!m_haveFiles) {
    return;
  }
  m_haveFiles = false;
  removeSessionFiles(m_directory, m_session);
  qDebug().noquote() << QStringLiteral("Autosave removed for %1").arg(m_writtenName);
  m_writtenName.clear();
}

QString AutosaveManager::filePath(const char* suffix) const {
  return QDir(m_directory).filePath(m_session + QLatin1String(suffix));
}

} // namespace mitcad
