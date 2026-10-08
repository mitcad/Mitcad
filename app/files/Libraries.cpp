// SPDX-License-Identifier: MIT
#include "Libraries.hpp"

#include <exception>

#include <QDir>
#include <QEventLoop>
#include <QJsonArray>
#include <QProgressDialog>
#include <QSettings>
#include <QStandardPaths>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

const QString kSourcesKey = QStringLiteral("libraries/sources");

QJsonObject failed(const QString& message) {
  return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                {QStringLiteral("message"), message}}}};
}

} // namespace

QList<LibrarySource> defaultLibrarySources() {
  // Where Mitcad's own repositories are published; fetched only when the
  // user asks.
  return {{QStringLiteral("https://github.com/mitcad/fasteners.git"), true},
          {QStringLiteral("https://github.com/mitcad/community-index.git"), true}};
}

LibrarySettings LibrarySettings::load() {
  LibrarySettings settings;
  QSettings store;
  if (!store.contains(kSourcesKey)) {
    settings.sources = defaultLibrarySources();
    return settings;
  }
  const QJsonArray list = QJsonDocument::fromJson(store.value(kSourcesKey).toByteArray()).array();
  for (const QJsonValue& value : list) {
    const QJsonObject source = value.toObject();
    const QString url = source.value(QStringLiteral("url")).toString().trimmed();
    if (!url.isEmpty()) {
      settings.sources.append({url, source.value(QStringLiteral("enabled")).toBool(true)});
    }
  }
  return settings;
}

void LibrarySettings::save() const {
  QJsonArray list;
  for (const LibrarySource& source : sources) {
    list.append(QJsonObject{{QStringLiteral("url"), source.url}, {QStringLiteral("enabled"), source.enabled}});
  }
  QSettings().setValue(kSourcesKey, QJsonDocument(list).toJson(QJsonDocument::Compact));
}

QStringList LibrarySettings::enabledUrls() const {
  QStringList urls;
  for (const LibrarySource& source : sources) {
    if (source.enabled) {
      urls << source.url;
    }
  }
  return urls;
}

bool LibrarySettings::add(const QString& url) {
  const QString trimmed = url.trimmed();
  for (const LibrarySource& source : sources) {
    if (source.url == trimmed) {
      return false;
    }
  }
  sources.append({trimmed, true});
  return true;
}

QString librariesDirectory() {
  const QString dir = qEnvironmentVariable("MITCAD_LIBRARIES_DIR");
  if (!dir.isEmpty()) {
    return dir;
  }
  return QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation) + QStringLiteral("/libraries");
}

void configureLibraries() {
  const QJsonObject config{{QStringLiteral("root"), QDir::toNativeSeparators(librariesDirectory())}};
  try {
    const QJsonObject answer = parseObject(configure_libraries(rustStr(compactJson(config))));
    qDebug().noquote() << QStringLiteral("Libraries in %1").arg(answer.value(QStringLiteral("root")).toString());
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Libraries not set up: %1").arg(errorText(e));
  }
}

QJsonObject libraryCommand(const QJsonObject& command) {
  try {
    const rust::Box<SyncControl> control = new_sync_control();
    return parseObject(library_command(rustStr(compactJson(command)), *control));
  } catch (const std::exception& e) {
    return failed(errorText(e));
  }
}

QList<QJsonObject> fetchLibraries(QWidget* parent, const QStringList& urls) {
  QList<QJsonObject> answers;
  QProgressDialog progress(QObject::tr("Fetching libraries..."), QObject::tr("Cancel"), 0, int(urls.size()), parent);
  progress.setWindowTitle(QObject::tr("Fetch Libraries"));
  progress.setMinimumDuration(300);
  progress.setAutoClose(false);
  progress.setAutoReset(false);
  prepareModal(&progress);
  bool cancelled = false;
  for (int i = 0; i < urls.size() && !cancelled; ++i) {
    const QString& url = urls[i];
    progress.setValue(i);
    progress.setLabelText(QObject::tr("Fetching %1").arg(url));
    RemoteTask* task = RemoteTask::library(
        {{QStringLiteral("cmd"), QStringLiteral("library_fetch")}, {QStringLiteral("url"), url}}, nullptr);
    QEventLoop loop;
    QObject::connect(task, &RemoteTask::finished, &loop, &QEventLoop::quit);
    QObject::connect(task, &RemoteTask::progressed, &progress, [&progress, url](const QString& text, int percent) {
      progress.setLabelText(percent >= 0 ? QObject::tr("Fetching %1\n%2 %3%").arg(url, text).arg(percent)
                                         : QObject::tr("Fetching %1\n%2").arg(url, text));
    });
    QObject::connect(&progress, &QProgressDialog::canceled, task, [task, &cancelled] {
      cancelled = true;
      task->cancel();
    });
    task->start();
    loop.exec();
    const QJsonObject answer = task->answer();
    delete task;
    const QJsonObject error = answer.value(QStringLiteral("error")).toObject();
    if (error.isEmpty()) {
      QStringList labels;
      for (const QJsonValue& version : answer.value(QStringLiteral("versions")).toArray()) {
        labels << version.toObject().value(QStringLiteral("text")).toString();
      }
      qInfo().noquote() << QStringLiteral("Library fetched: %1 %2 (%3) from %4; versions %5")
                               .arg(answer.value(QStringLiteral("kind")).toString(),
                                    answer.value(QStringLiteral("name")).toString(),
                                    answer.value(QStringLiteral("id")).toString(), url,
                                    labels.join(QStringLiteral(", ")));
    } else {
      qWarning().noquote() << QStringLiteral("Library fetch failed: %1: %2")
                                  .arg(url, error.value(QStringLiteral("message")).toString());
    }
    answers.append(answer);
  }
  progress.setValue(int(urls.size()));
  return answers;
}

QString licenseNote(const QString& license) {
  static QJsonArray licenses;
  if (licenses.isEmpty()) {
    licenses = libraryCommand({{QStringLiteral("cmd"), QStringLiteral("licenses")}})
                   .value(QStringLiteral("licenses"))
                   .toArray();
  }
  for (const QJsonValue& value : licenses) {
    const QJsonObject entry = value.toObject();
    if (entry.value(QStringLiteral("id")).toString().compare(license.trimmed(), Qt::CaseInsensitive) == 0) {
      return entry.value(QStringLiteral("note")).toString();
    }
  }
  return {};
}

QStringList acceptedLicenses() {
  QStringList ids;
  const QJsonArray licenses =
      libraryCommand({{QStringLiteral("cmd"), QStringLiteral("licenses")}}).value(QStringLiteral("licenses")).toArray();
  for (const QJsonValue& value : licenses) {
    ids << value.toObject().value(QStringLiteral("id")).toString();
  }
  return ids;
}

} // namespace mitcad
