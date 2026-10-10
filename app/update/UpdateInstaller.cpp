// SPDX-License-Identifier: MIT
#include "UpdateInstaller.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QProcessEnvironment>
#include <QSettings>
#include <QStringList>
#include <QtLogging>

#include "UpdateSettings.hpp"

namespace mitcad {
namespace {

const QString kInstalling = QStringLiteral("updates/installing");
const QString kInstallingFrom = QStringLiteral("updates/installingFrom");
const QString kInstallLog = QStringLiteral("updates/installLog");
const QString kLastResult = QStringLiteral("updates/lastResult");
const QString kUpdater = QStringLiteral("mitcad-updater.exe");

// What runs once Mitcad has quit.
struct Staged {
  Installation::Kind kind = Installation::Kind::NotifyOnly;
  QString program;
  QStringList arguments;
  QString workingDirectory;
  // An AppImage: the download that replaces the target first.
  QString download;
  QString target;
};

std::optional<Staged>& staged() {
  static std::optional<Staged> update;
  return update;
}

// From QCoreApplication's destructor (qAddPostRoutine): the window, its
// document and the session's autosave are gone by then.
void launchStaged() {
  const std::optional<Staged> update = staged();
  staged().reset();
  if (!update) {
    return;
  }
  if (update->kind == Installation::Kind::AppImage) {
    // A rename within the folder replaces the file at once; this process
    // keeps running from the old one, which goes when it ends.
    if (std::rename(QFile::encodeName(update->download).constData(), QFile::encodeName(update->target).constData()) !=
        0) {
      qWarning().noquote() << "Update: cannot replace" << update->target << "- starting the old version";
      QFile::remove(update->download);
    } else {
      qInfo().noquote() << "Update: replaced" << update->target;
    }
  }
  QProcess process;
  process.setProgram(update->program);
  process.setArguments(update->arguments);
  process.setWorkingDirectory(update->workingDirectory);
  // The tests' version is this process's only; the AppImage's runtime sets
  // its variables for the new one again.
  QProcessEnvironment environment = QProcessEnvironment::systemEnvironment();
  for (const char* variable : {kTestVersionVariable, "APPIMAGE", "APPDIR", "ARGV0", "OWD"}) {
    environment.remove(QString::fromLatin1(variable));
  }
  process.setProcessEnvironment(environment);
  qint64 pid = 0;
  if (process.startDetached(&pid)) {
    qInfo().noquote() << QStringLiteral("Update: started %1 (process %2)").arg(update->program).arg(pid);
  } else {
    qWarning().noquote() << "Update: cannot start" << update->program << "-" << process.errorString();
  }
}

QString lastLine(const QString& path) {
  QFile file(path);
  if (path.isEmpty() || !file.open(QIODevice::ReadOnly | QIODevice::Text)) {
    return QString();
  }
  const QStringList lines = QString::fromUtf8(file.readAll()).split(QLatin1Char('\n'), Qt::SkipEmptyParts);
  return lines.isEmpty() ? QString() : lines.last().trimmed();
}

} // namespace

Installation Installation::detect() {
  Installation installation;
#ifdef _WIN32
  const QDir bin(QCoreApplication::applicationDirPath());
  const QString root = QDir::cleanPath(bin.absoluteFilePath(QStringLiteral("..")));
  if (bin.dirName().compare(QStringLiteral("bin"), Qt::CaseInsensitive) == 0 &&
      QFileInfo::exists(QDir(root).filePath(QStringLiteral("Uninstall.exe"))) &&
      QFileInfo::exists(bin.filePath(kUpdater))) {
    installation.kind = Kind::WindowsInstaller;
    installation.location = root;
  } else {
    installation.reason = QStringLiteral("not installed by the installer (the portable .zip or a development build)");
  }
#elif defined(__APPLE__)
  // A new version comes as a disk image the user installs from; the
  // application bundle is not replaced in place.
  installation.reason = QStringLiteral("a macOS application bundle (updates come as disk images)");
#else
  const QString appImage = qEnvironmentVariable("APPIMAGE");
  const QFileInfo file(appImage);
  if (appImage.isEmpty()) {
    installation.reason = QStringLiteral("not an AppImage (a development build or a system package)");
  } else if (!file.isFile()) {
    installation.reason = QStringLiteral("the AppImage %1 is not a file").arg(appImage);
  } else if (!file.isWritable() || !QFileInfo(file.absolutePath()).isWritable()) {
    installation.reason = QStringLiteral("the user cannot write to the AppImage %1 or its folder").arg(appImage);
  } else {
    installation.kind = Kind::AppImage;
    installation.location = file.absoluteFilePath();
  }
#endif
  return installation;
}

QString Installation::describe() const {
  switch (kind) {
  case Kind::WindowsInstaller:
    return QStringLiteral("installed in %1").arg(QDir::toNativeSeparators(location));
  case Kind::AppImage:
    return QStringLiteral("the AppImage %1").arg(location);
  case Kind::NotifyOnly:
    break;
  }
  return QStringLiteral("announce only: %1").arg(reason);
}

QString Installation::downloadPath(const QString& version) const {
  if (kind == Kind::AppImage) {
    const QFileInfo file(location);
    return file.absoluteDir().filePath(QStringLiteral(".%1.update").arg(file.fileName()));
  }
  const QString folder = QDir::temp().filePath(QStringLiteral("mitcad-update"));
  QDir().mkpath(folder);
  return QDir(folder).filePath(QStringLiteral("mitcad-%1-windows-x64.exe").arg(version));
}

void Installation::removeLeftovers() const {
  if (kind == Kind::AppImage) {
    QFile::remove(downloadPath(QString()));
  } else if (kind == Kind::WindowsInstaller) {
    QDir(QDir::temp().filePath(QStringLiteral("mitcad-update"))).removeRecursively();
  }
}

bool stageUpdate(const Installation& installation, const QString& file, const QString& from, const QString& to,
                 QString& error) {
  Staged update;
  update.kind = installation.kind;
  QString log;
  if (installation.kind == Installation::Kind::WindowsInstaller) {
    // The installer replaces bin, so the updater runs from the download's
    // folder.
    const QDir folder = QFileInfo(file).absoluteDir();
    const QString updater = folder.filePath(kUpdater);
    QFile::remove(updater);
    if (!QFile::copy(QDir(QCoreApplication::applicationDirPath()).filePath(kUpdater), updater)) {
      error = QCoreApplication::translate("Update", "Cannot copy %1 to %2.")
                  .arg(kUpdater, QDir::toNativeSeparators(folder.absolutePath()));
      return false;
    }
    log = folder.filePath(QStringLiteral("updater.log"));
    QFile::remove(log);
    update.program = updater;
    update.workingDirectory = folder.absolutePath();
    update.arguments = {QStringLiteral("--wait"),
                        QString::number(QCoreApplication::applicationPid()),
                        QStringLiteral("--installer"),
                        QDir::toNativeSeparators(file),
                        QStringLiteral("--dir"),
                        QDir::toNativeSeparators(installation.location),
                        QStringLiteral("--restart"),
                        QDir::toNativeSeparators(QDir(installation.location).filePath(QStringLiteral("bin/mitcad.exe"))),
                        QStringLiteral("--log"),
                        QDir::toNativeSeparators(log)};
  } else if (installation.kind == Installation::Kind::AppImage) {
    QFile download(file);
    const QFile::Permissions permissions = QFile(installation.location).permissions() | QFile::ReadOwner |
                                           QFile::WriteOwner | QFile::ExeOwner | QFile::ReadUser |
                                           QFile::WriteUser | QFile::ExeUser;
    if (!download.setPermissions(permissions)) {
      error = QCoreApplication::translate("Update", "Cannot make %1 executable.").arg(file);
      return false;
    }
    update.program = installation.location;
    update.workingDirectory = QFileInfo(installation.location).absolutePath();
    update.download = file;
    update.target = installation.location;
  } else {
    error = QCoreApplication::translate("Update", "This installation cannot update itself.");
    return false;
  }
  QSettings settings;
  settings.setValue(kInstalling, to);
  settings.setValue(kInstallingFrom, from);
  settings.setValue(kInstallLog, log);
  settings.sync();
  static bool routineAdded = false;
  if (!routineAdded) {
    qAddPostRoutine(launchStaged);
    routineAdded = true;
  }
  staged() = update;
  qInfo().noquote() << QStringLiteral("Update staged: %1 -> %2, %3 when Mitcad has quit").arg(from, to, update.program);
  return true;
}

void unstageUpdate() {
  staged().reset();
  QSettings settings;
  settings.remove(kInstalling);
  settings.remove(kInstallingFrom);
  settings.remove(kInstallLog);
  qInfo() << "Update unstaged";
}

std::optional<FinishedUpdate> takeFinishedUpdate(const QString& currentVersion) {
  QSettings settings;
  const QString to = settings.value(kInstalling).toString();
  if (to.isEmpty()) {
    return std::nullopt;
  }
  FinishedUpdate finished;
  finished.to = to;
  finished.from = settings.value(kInstallingFrom).toString();
  finished.installed = currentVersion == to;
  if (!finished.installed) {
    finished.detail = lastLine(settings.value(kInstallLog).toString());
  }
  settings.remove(kInstalling);
  settings.remove(kInstallingFrom);
  settings.remove(kInstallLog);
  settings.setValue(kLastResult, QStringLiteral("%1 %2 -> %3%4")
                                     .arg(finished.installed ? QStringLiteral("installed") : QStringLiteral("failed"),
                                          finished.from, finished.to,
                                          finished.detail.isEmpty() ? QString() : QStringLiteral(": ") + finished.detail));
  return finished;
}

} // namespace mitcad
