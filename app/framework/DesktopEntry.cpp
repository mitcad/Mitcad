// SPDX-License-Identifier: MIT
#include "DesktopEntry.hpp"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QSaveFile>
#include <QStandardPaths>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kDesktopFile = QStringLiteral("applications/mitcad.desktop");
const QString kIcons = QStringLiteral("icons/hicolor");

// A desktop file's Exec argument: quoted, with the specification's escapes
// inside the quotes, and then a value's (a backslash doubled).
QString execArgument(const QString& program) {
  QString quoted = QStringLiteral("\"");
  for (const QChar c : program) {
    if (c == QLatin1Char('"') || c == QLatin1Char('`') || c == QLatin1Char('$') || c == QLatin1Char('\\')) {
      quoted += QLatin1Char('\\');
    }
    quoted += c;
  }
  quoted += QLatin1Char('"');
  return quoted.replace(QLatin1Char('\\'), QStringLiteral("\\\\"));
}

// Writes a file when its content differs; false with the reason on failure.
bool writeIfChanged(const QString& path, const QByteArray& content, QStringList& written, QString& error) {
  QFile current(path);
  if (current.open(QIODevice::ReadOnly) && current.readAll() == content) {
    return true;
  }
  current.close();
  QDir().mkpath(QFileInfo(path).absolutePath());
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(content) != content.size() || !file.commit()) {
    error = QStringLiteral("cannot write %1: %2").arg(path, file.errorString());
    return false;
  }
  written << path;
  return true;
}

} // namespace

QString desktopEntryFor(const QString& bundled, const QString& program) {
  QStringList lines = bundled.split(QLatin1Char('\n'));
  for (QString& line : lines) {
    if (line.startsWith(QStringLiteral("Exec="))) {
      line = QStringLiteral("Exec=") + execArgument(program);
    }
  }
  return lines.join(QLatin1Char('\n'));
}

DesktopIntegration integrateAppImage(const QString& appImage, const QString& appDir, const QString& dataHome) {
  DesktopIntegration result;
  const QDir share(QDir(appDir).filePath(QStringLiteral("usr/share")));
  QFile bundled(share.filePath(kDesktopFile));
  if (!bundled.open(QIODevice::ReadOnly)) {
    result.error = QStringLiteral("no desktop file %1").arg(bundled.fileName());
    return result;
  }
  const QDir data(dataHome);
  // The icons first, so that the desktop finds them when the desktop file
  // appears.
  QDirIterator icons(share.filePath(kIcons), {QStringLiteral("mitcad.*")}, QDir::Files,
                     QDirIterator::Subdirectories);
  while (icons.hasNext()) {
    const QString icon = icons.next();
    QFile file(icon);
    if (!file.open(QIODevice::ReadOnly)) {
      result.error = QStringLiteral("cannot read %1").arg(icon);
      return result;
    }
    if (!writeIfChanged(data.filePath(share.relativeFilePath(icon)), file.readAll(), result.written, result.error)) {
      return result;
    }
  }
  const QString entry = desktopEntryFor(QString::fromUtf8(bundled.readAll()), appImage);
  writeIfChanged(data.filePath(kDesktopFile), entry.toUtf8(), result.written, result.error);
  return result;
}

void integrateDesktop() {
#ifdef __linux__
  const QString appImage = qEnvironmentVariable("APPIMAGE");
  const QString appDir = qEnvironmentVariable("APPDIR");
  if (appImage.isEmpty() || appDir.isEmpty()) {
    return;
  }
  const QString dataHome = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation);
  const DesktopIntegration result = integrateAppImage(appImage, appDir, dataHome);
  if (!result.error.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Desktop entry: %1").arg(result.error);
  } else if (result.written.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Desktop entry: up to date in %1").arg(dataHome);
  } else {
    for (const QString& path : result.written) {
      qDebug().noquote() << QStringLiteral("Desktop entry: wrote %1").arg(path);
    }
  }
#endif
}

} // namespace mitcad
