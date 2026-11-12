// SPDX-License-Identifier: MIT
#include "DesktopEntry.hpp"

#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QSaveFile>
#include <QStandardPaths>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kDesktopFile = QStringLiteral("applications/mitcad.desktop");
const QString kIcons = QStringLiteral("icons/hicolor");
// File managers' thumbnails of designs (mitcad#1539).
const QString kThumbnailer = QStringLiteral("usr/bin/mitcad-thumbnailer");
const QString kThumbnailerCopy = QStringLiteral("mitcad/bin/mitcad-thumbnailer");
const QString kThumbnailerEntry = QStringLiteral("thumbnailers/mitcad.thumbnailer");
const QString kMimePackage = QStringLiteral("mime/packages/mitcad.xml");

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

QString thumbnailerEntryFor(const QString& bundled, const QString& program) {
  QStringList lines = bundled.split(QLatin1Char('\n'));
  for (QString& line : lines) {
    if (line.startsWith(QStringLiteral("TryExec="))) {
      // A string value: only a backslash is escaped.
      line = QStringLiteral("TryExec=") + QString(program).replace(QLatin1Char('\\'), QStringLiteral("\\\\"));
    } else if (line.startsWith(QStringLiteral("Exec="))) {
      const qsizetype space = line.indexOf(QLatin1Char(' '));
      line = QStringLiteral("Exec=") + execArgument(program) + (space < 0 ? QString() : line.mid(space));
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
  // File managers' thumbnails of designs (mitcad#1539), when the AppImage
  // has them (older ones do not): the program first, then the MIME type,
  // then the entry that runs the program for it.
  QFile thumbnailer(QDir(appDir).filePath(kThumbnailer));
  QFile thumbnailerEntry(share.filePath(kThumbnailerEntry));
  QFile mimePackage(share.filePath(kMimePackage));
  if (thumbnailer.open(QIODevice::ReadOnly) && thumbnailerEntry.open(QIODevice::ReadOnly) &&
      mimePackage.open(QIODevice::ReadOnly)) {
    const QString copy = data.filePath(kThumbnailerCopy);
    if (!writeIfChanged(copy, thumbnailer.readAll(), result.written, result.error)) {
      return result;
    }
    // Run by the file manager: executable, as an installed program is.
    const QFileDevice::Permissions executable = QFileDevice::ReadOwner | QFileDevice::WriteOwner |
                                                QFileDevice::ExeOwner | QFileDevice::ReadUser |
                                                QFileDevice::WriteUser | QFileDevice::ExeUser |
                                                QFileDevice::ReadGroup | QFileDevice::ExeGroup |
                                                QFileDevice::ReadOther | QFileDevice::ExeOther;
    if (!QFile::setPermissions(copy, executable)) {
      result.error = QStringLiteral("cannot make %1 executable").arg(copy);
      return result;
    }
    if (!writeIfChanged(data.filePath(kMimePackage), mimePackage.readAll(), result.written, result.error)) {
      return result;
    }
    const QString entry = thumbnailerEntryFor(QString::fromUtf8(thumbnailerEntry.readAll()), copy);
    if (!writeIfChanged(data.filePath(kThumbnailerEntry), entry.toUtf8(), result.written, result.error)) {
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
  // A new or changed MIME type counts once the MIME database file managers
  // read is made again from the packages (mitcad#1539).
  const QString mime = QDir(dataHome).filePath(kMimePackage);
  if (result.written.contains(mime)) {
    const QString update = QStandardPaths::findExecutable(QStringLiteral("update-mime-database"));
    const QString database = QDir(dataHome).filePath(QStringLiteral("mime"));
    if (update.isEmpty() || !QProcess::startDetached(update, {database})) {
      qWarning().noquote() << QStringLiteral("Desktop entry: the MIME database in %1 was not updated").arg(database);
    } else {
      qDebug().noquote() << QStringLiteral("Desktop entry: updating the MIME database in %1").arg(database);
    }
  }
#endif
}

} // namespace mitcad
