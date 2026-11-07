// SPDX-License-Identifier: MIT
#include "Libraries.hpp"

#include <exception>

#include <QJsonArray>
#include <QProgressDialog>
#include <QPointer>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "LibraryAutoUpdate.hpp"
#include "RemoteTask.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QJsonObject failed(const QString& message) {
  return {{QStringLiteral("error"), QJsonObject{{QStringLiteral("class"), QStringLiteral("other")},
                                                {QStringLiteral("message"), message}}}};
}

} // namespace

// The settings (LibrarySettings, the defaults) are in LibrarySettings.cpp.

void configureLibraries() {
  try {
    const QJsonObject answer = parseObject(configure_libraries(""));
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

QJsonObject markLibrarySource(const QString& url, const QStringList& remaining, bool removed) {
  return libraryCommand({{QStringLiteral("cmd"), QStringLiteral("library_remove")},
                         {QStringLiteral("url"), url},
                         {QStringLiteral("sources"), QJsonArray::fromStringList(remaining)},
                         {QStringLiteral("removed"), removed}});
}

void logLibraryFetch(const QString& url, const QJsonObject& answer) {
  const QJsonObject error = answer.value(QStringLiteral("error")).toObject();
  if (error.isEmpty()) {
    QStringList labels;
    for (const QJsonValue& version : answer.value(QStringLiteral("versions")).toArray()) {
      labels << version.toObject().value(QStringLiteral("text")).toString();
    }
    // The version Browse Components and the BUILD pickers read from now on:
    // the newest (the versions come newest first, mitcad#1423).
    const QString kind = answer.value(QStringLiteral("kind")).toString();
    const QString pickerVersion = kind == QLatin1String("library") && !labels.isEmpty()
                                      ? QStringLiteral("; the pickers read %1").arg(labels.constFirst())
                                      : QString();
    qInfo().noquote() << QStringLiteral("Library fetched: %1 %2 (%3) from %4; versions %5%6")
                             .arg(kind, answer.value(QStringLiteral("name")).toString(),
                                  answer.value(QStringLiteral("id")).toString(), url,
                                  labels.join(QStringLiteral(", ")), pickerVersion);
  } else {
    qWarning().noquote() << QStringLiteral("Library fetch failed: %1: %2")
                                .arg(url, error.value(QStringLiteral("message")).toString());
  }
}

QList<QJsonObject> fetchLibraries(QWidget* parent, const QStringList& urls) {
  // A fetch the user asks for takes over from the one at start-up: two
  // fetches into one cache at once are not wanted.
  LibraryAutoUpdate::instance().cancel();
  QList<QJsonObject> answers;
  QProgressDialog progress(QObject::tr("Fetching libraries…"), QObject::tr("Cancel"), 0, int(urls.size()), parent);
  progress.setWindowTitle(QObject::tr("Fetch Libraries"));
  prepareModal(&progress);
  prepareProgressDialog(&progress);
  progress.setMinimumDuration(300);
  progress.setAutoClose(false);
  progress.setAutoReset(false);
  bool cancelled = false;
  for (int i = 0; i < urls.size() && !cancelled; ++i) {
    const QString& url = urls[i];
    progress.setValue(i);
    progress.setLabelText(QObject::tr("Fetching %1").arg(url));
    RemoteTask* task = RemoteTask::library(libraryFetchCommand(url), nullptr);
    QObject waiting;
    bool finished = false;
    const QPointer<RemoteTask> pending = task;
    QObject::connect(task, &RemoteTask::finished, &waiting, [&finished] { finished = true; });
    QObject::connect(task, &RemoteTask::progressed, &progress, [&progress, url](const QString& text, int percent) {
      progress.setLabelText(percent >= 0 ? QObject::tr("Fetching %1\n%2 %3%").arg(url, text).arg(percent)
                                         : QObject::tr("Fetching %1\n%2").arg(url, text));
    });
    QObject::connect(&progress, &QProgressDialog::canceled, task, [task, &cancelled] {
      cancelled = true;
      task->cancel();
    });
    task->start();
    waitDialogUntil([&] { return finished || pending == nullptr; });
    if (pending == nullptr) { cancelled = true; break; }
    const QJsonObject answer = task->answer();
    delete task;
    logLibraryFetch(url, answer);
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
