// SPDX-License-Identifier: MIT
#include "render/RenderWorkerChoice.hpp"

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QSettings>
#include <QtLogging>

#include "render/RenderProtocol.hpp"

namespace mitcad::render {

namespace {

const char* const kWorkerKey = "render/worker";

// The picked worker that failed in this run.
QString& failedWorker() {
  static QString path;
  return path;
}

QString shippedName() { return QString::fromLatin1(kWorkerName); }

} // namespace

QString pickedWorker() {
  const QSettings settings;
  return settings.value(QLatin1String(kWorkerKey)).toString();
}

void setPickedWorker(const QString& path) {
  QSettings settings;
  if (path.isEmpty()) {
    settings.remove(QLatin1String(kWorkerKey));
  } else {
    settings.setValue(QLatin1String(kWorkerKey), QDir::fromNativeSeparators(path));
  }
  // Another choice is tried again, even the one that failed before.
  failedWorker().clear();
}

QString executableFile(const QString& path) {
  if (path.isEmpty()) {
    return {};
  }
  const QFileInfo file(path);
  return file.isFile() && file.isExecutable() ? file.absoluteFilePath() : QString();
}

WorkerChoice chooseWorker(const QString& forced, const QString& picked, const QString& failed,
                          const QString& shipped) {
  WorkerChoice choice;
  if (!forced.isEmpty()) {
    choice.path = executableFile(forced);
    return choice;
  }
  choice.path = shipped;
  if (picked.isEmpty()) {
    return choice;
  }
  const QString file = executableFile(picked);
  if (file.isEmpty()) {
    choice.fallback =
        QCoreApplication::translate("RenderWorker", "The render worker %1 is not there or cannot be run; rendering "
                                                    "with the shipped %2")
            .arg(QDir::toNativeSeparators(picked), shippedName());
    return choice;
  }
  if (file == shipped || (!failed.isEmpty() && file == executableFile(failed))) {
    return choice;
  }
  choice.path = file;
  choice.picked = true;
  return choice;
}

QString failedPickedWorker() { return failedWorker(); }

QString pickedWorkerFailed(const QString& worker, const QString& reason) {
  failedWorker() = worker;
  const QString text =
      QCoreApplication::translate("RenderWorker", "The render worker %1 cannot be used (%2); rendering with the "
                                                  "shipped %3")
          .arg(QDir::toNativeSeparators(worker), reason, shippedName());
  qWarning().noquote() << QStringLiteral("Render worker fallback: %1").arg(text);
  return text;
}

} // namespace mitcad::render
