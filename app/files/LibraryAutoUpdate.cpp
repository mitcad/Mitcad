// SPDX-License-Identifier: MIT
#include "LibraryAutoUpdate.hpp"

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "Libraries.hpp"
#include "RemoteTask.hpp"

namespace mitcad {
namespace {

// The texts of the "versions" of a library_fetch or library_list answer
// ("v1.0.0 (3f9a2c1)"), newest first.
QStringList versionTexts(const QJsonArray& versions) {
  QStringList texts;
  for (const QJsonValue& version : versions) {
    const QString text = version.toObject().value(QStringLiteral("text")).toString();
    if (!text.isEmpty()) {
      texts << text;
    }
  }
  return texts;
}

// The versions of the fetched source `url` now: a local command, so quick
// that it runs on the UI thread as the browser's do. Empty for a source that
// was not fetched.
QStringList knownVersions(const QString& url) {
  const QJsonObject answer = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_list")}});
  const QString wanted = normalLibraryUrl(url);
  for (const QJsonValue& value : answer.value(QStringLiteral("libraries")).toArray()) {
    const QJsonObject library = value.toObject();
    if (normalLibraryUrl(library.value(QStringLiteral("url")).toString()) == wanted) {
      return versionTexts(library.value(QStringLiteral("versions")).toArray());
    }
  }
  return {};
}

} // namespace

LibraryAutoUpdate& LibraryAutoUpdate::instance() {
  // A child of the application, so that it goes with it (and a fetch under
  // way is ended: see the constructor).
  static LibraryAutoUpdate* const own = new LibraryAutoUpdate(QCoreApplication::instance());
  return *own;
}

LibraryAutoUpdate::LibraryAutoUpdate(QObject* parent) : QObject(parent) {
  if (QCoreApplication* app = QCoreApplication::instance()) {
    connect(app, &QCoreApplication::aboutToQuit, this, [this] { cancel(); });
  }
}

bool LibraryAutoUpdate::isRunning() const { return !m_task.isNull() || !m_queue.isEmpty(); }

void LibraryAutoUpdate::start() {
  if (isRunning()) {
    return;
  }
  const QStringList urls = LibrarySettings::load().autoUpdateUrls();
  if (urls.isEmpty()) {
    return;
  }
  m_queue = urls;
  m_news.clear();
  m_fetched = 0;
  m_failed = 0;
  qInfo().noquote() << QStringLiteral("Library auto-update: fetching %1 source(s) in the background")
                           .arg(urls.size());
  next();
}

void LibraryAutoUpdate::cancel() {
  if (!isRunning()) {
    return;
  }
  m_queue.clear();
  m_news.clear();
  if (RemoteTask* task = m_task.data()) {
    m_task.clear();
    delete task; // ends git and waits for the thread
  }
  qInfo().noquote() << QStringLiteral("Library auto-update ended before it was done");
}

void LibraryAutoUpdate::next() {
  if (m_queue.isEmpty()) {
    finish();
    return;
  }
  const QString url = m_queue.takeFirst();
  m_before = knownVersions(url);
  RemoteTask* task = RemoteTask::library(libraryFetchCommand(url), this);
  m_task = task;
  connect(task, &RemoteTask::finished, this, [this, url] { fetched(url); });
  task->start();
}

void LibraryAutoUpdate::fetched(const QString& url) {
  RemoteTask* task = m_task.data();
  if (task == nullptr) {
    return;
  }
  m_task.clear();
  const QJsonObject answer = task->answer();
  task->deleteLater();
  logLibraryFetch(url, answer);
  const QJsonObject error = answer.value(QStringLiteral("error")).toObject();
  if (!error.isEmpty()) {
    // Offline or the server down: logged (above), nothing more; a
    // cancelled fetch is no failure.
    if (error.value(QStringLiteral("class")).toString() != QLatin1String("cancelled")) {
      ++m_failed;
    }
  } else {
    ++m_fetched;
    // Something new: a version that was not listed before (all of them for
    // a source fetched the first time).
    const QStringList now = versionTexts(answer.value(QStringLiteral("versions")).toArray());
    QString newest;
    for (const QString& text : now) {
      if (!m_before.contains(text)) {
        newest = text; // newest first: the first one is it
        break;
      }
    }
    if (!newest.isEmpty()) {
      const QString name = answer.value(QStringLiteral("name")).toString();
      m_news << QStringLiteral("%1 %2").arg(name.isEmpty() ? url : name, newest.section(QLatin1Char(' '), 0, 0));
    }
  }
  next();
}

void LibraryAutoUpdate::finish() {
  qInfo().noquote() << QStringLiteral("Library auto-update: %1 source(s) fetched, %2 failed; new: %3")
                           .arg(m_fetched)
                           .arg(m_failed)
                           .arg(m_news.isEmpty() ? QStringLiteral("nothing") : m_news.join(QStringLiteral(", ")));
  const QStringList news = m_news;
  const int failed = m_failed;
  m_news.clear();
  if (!news.isEmpty()) {
    emit updated();
  }
  emit finished(summary(news, failed));
}

QString LibraryAutoUpdate::summary(const QStringList& news, int failed) {
  if (news.isEmpty()) {
    return failed > 0 ? tr("Libraries not updated: %n source(s) could not be fetched", nullptr, failed) : QString();
  }
  const QString text = tr("Libraries updated: %1").arg(news.join(QStringLiteral(", ")));
  return failed > 0 ? tr("%1; %n source(s) could not be fetched", nullptr, failed).arg(text) : text;
}

} // namespace mitcad
