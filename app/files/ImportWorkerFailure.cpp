// SPDX-License-Identifier: MIT
#include "ImportWorkerFailure.hpp"

#include <QCoreApplication>

#include "../framework/UserText.hpp"

namespace mitcad {

namespace {

QString tr(const char* text) {
  return QCoreApplication::translate("mitcad::F3dImport", text);
}

} // namespace

QString importWorkerLineShown(const QString& line) {
  const QString shown = userText(line).trimmed();
  if (shown.startsWith(QLatin1String("[placed:")) || shown.startsWith(QLatin1Char('{')) ||
      shown.startsWith(QLatin1Char('['))) {
    return QString();
  }
  // (A tail starts its own line in a model message; one inline is cut too.)
  const qsizetype tail = shown.indexOf(QLatin1String("[placed:"));
  return tail < 0 ? shown : shown.left(tail).trimmed();
}

QString importWorkerStopped(bool crashed, int exitCode, const QString& item, const QString& lastLine) {
  QString message =
      crashed ? tr("the import process stopped unexpectedly") : tr("the import process stopped (exit code %1)").arg(exitCode);
  if (!item.isEmpty()) {
    message = tr("%1 while it imported %2").arg(message, item);
  }
  const QString shown = crashed ? QString() : importWorkerLineShown(lastLine);
  return shown.isEmpty() ? message : tr("%1: %2").arg(message, shown);
}

} // namespace mitcad
