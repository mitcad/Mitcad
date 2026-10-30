// SPDX-License-Identifier: MIT
#include "UpdateInstaller.hpp"

#include <cstdio>
#include <string>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QProcess>
#include <QProcessEnvironment>
#include <QSettings>
#include <QStandardPaths>
#include <QStringList>
#include <QTemporaryDir>
#include <QtLogging>

#ifdef __APPLE__
#include <sys/mount.h>
#include <sys/xattr.h>
#endif
#ifdef _WIN32
#include <windows.h>
#endif

#include "UpdateSettings.hpp"
#include "UpdaterChecks.hpp"

namespace mitcad {
namespace {

const QString kDownloadFolders = QStringLiteral("updates/downloadFolders");
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

QString downloadFolderPrefix() { return QStringLiteral("mitcad-update-"); }

// A private, unpredictable folder (macOS; Windows since mitcad#603): the
// verified download and the helper that installs it survive this process
// and are never shared with another run, and no other program knows the
// folder's name in advance. One per run, made on first use in the user's
// temp folder with a random name: owner-only on Unix (mkdtemp, 0700), with
// the user's temp folder's permissions on Windows.
QString privateDownloadDirectory(bool create = true) {
  static QString path;
  if (create && !QFileInfo(path).isDir()) {
    QTemporaryDir folder(QDir::temp().filePath(downloadFolderPrefix() + QStringLiteral("XXXXXX")));
    folder.setAutoRemove(false);
    path = folder.isValid() ? folder.path() : QString();
  }
  return path;
}

// Whether `path` is such a folder, directly in the temp folder: nothing
// else is removed as a leftover, whatever the settings say.
bool isPrivateDownloadDirectory(const QString& path) {
#ifdef _WIN32
  const Qt::CaseSensitivity sensitivity = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity sensitivity = Qt::CaseSensitive;
#endif
  const QFileInfo folder(path);
  const QString name = folder.fileName();
  return !path.isEmpty() && folder.isAbsolute() && name.startsWith(downloadFolderPrefix()) &&
         name.size() > downloadFolderPrefix().size() &&
         QDir::cleanPath(folder.absolutePath()).compare(QDir::cleanPath(QDir::temp().absolutePath()), sensitivity) == 0;
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
  for (const char* variable : {kTestVersionVariable, "MITCAD_TEST_INPUT", "APPIMAGE", "APPDIR", "ARGV0", "OWD"}) {
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
    const QSettings scope(QDir(root).filePath(QStringLiteral("installation.ini")), QSettings::IniFormat);
    installation.allUsers = scope.value(QStringLiteral("Installation/Scope")).toString() != QStringLiteral("user");
  } else {
    installation.reason = QStringLiteral("not installed by the installer (the portable .zip or a development build)");
  }
#elif defined(__APPLE__)
  const QDir bin(QCoreApplication::applicationDirPath());
  const QFileInfo bundle(QDir::cleanPath(bin.absoluteFilePath(QStringLiteral("../.."))));
  const QString path = bundle.canonicalFilePath();
  // statfs follows APFS firmlinks to their actual volume. A mount-list
  // prefix match can mistake writable /Users paths for the sealed system
  // volume mounted at /, instead of the Data volume backing those paths.
  struct statfs volume {};
  const bool haveVolume = !path.isEmpty() && statfs(QFile::encodeName(path).constData(), &volume) == 0;
  const QString downloadsLocation = QStandardPaths::writableLocation(QStandardPaths::DownloadLocation);
  const QString downloads = downloadsLocation.isEmpty() ? QString() : QDir(downloadsLocation).canonicalPath();
  const bool quarantined = !path.isEmpty() &&
                           getxattr(QFile::encodeName(path).constData(), "com.apple.quarantine", nullptr, 0, 0, 0) >= 0;
  if (bin.dirName() != QStringLiteral("MacOS") ||
      QFileInfo(QDir::cleanPath(bin.absoluteFilePath(QStringLiteral("..")))).fileName() != QStringLiteral("Contents") ||
      !bundle.fileName().endsWith(QStringLiteral(".app"), Qt::CaseInsensitive) ||
      !QFileInfo::exists(QDir(path).filePath(QStringLiteral("Contents/Info.plist")))) {
    installation.reason = QStringLiteral("not a macOS application bundle (a development build)");
  } else if (path.contains(QStringLiteral("/AppTranslocation/")) ||
             bundle.absoluteFilePath().contains(QStringLiteral("/AppTranslocation/"))) {
    installation.reason = QStringLiteral("the application is running from App Translocation; move it to Applications first");
  } else if (quarantined && !downloads.isEmpty() && path.startsWith(downloads + QLatin1Char('/'))) {
    installation.reason = QStringLiteral("the quarantined application is in Downloads; move it to Applications first");
  } else if (!haveVolume) {
    installation.reason = QStringLiteral("the application bundle's volume cannot be determined");
  } else if ((volume.f_flags & MNT_RDONLY) != 0) {
    installation.reason = QStringLiteral("the application bundle is on a read-only volume; copy it to Applications first");
  } else if (bundle.isSymLink() || !bundle.isWritable() || !QFileInfo(bundle.absolutePath()).isWritable()) {
    installation.reason = QStringLiteral("the user cannot replace the application bundle or write to its folder");
  } else {
    installation.kind = Kind::MacBundle;
    installation.location = path;
  }
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
    return QStringLiteral("installed in %1 %2")
        .arg(QDir::toNativeSeparators(location),
             allUsers ? QStringLiteral("for all users") : QStringLiteral("for the current user"));
  case Kind::AppImage:
    return QStringLiteral("the AppImage %1").arg(location);
  case Kind::MacBundle:
    return QStringLiteral("the macOS application bundle %1").arg(location);
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
  if (kind != Kind::WindowsInstaller && kind != Kind::MacBundle) {
    return QString();
  }
  const QString folder = privateDownloadDirectory();
  if (folder.isEmpty()) {
    return QString();
  }
  if (kind == Kind::WindowsInstaller) {
    // mitcad-updater.exe runs from this folder until Mitcad has started
    // again, so the next start removes it (removeLeftovers).
    QSettings settings;
    QStringList folders = settings.value(kDownloadFolders).toStringList();
    if (!folders.contains(folder)) {
      folders.append(folder);
      settings.setValue(kDownloadFolders, folders);
    }
  }
  return QDir(folder).filePath((kind == Kind::MacBundle ? QStringLiteral("Mitcad-%1-macos-arm64.dmg")
                                                        : QStringLiteral("mitcad-%1-windows-x64.exe"))
                                   .arg(version));
}

void Installation::removeLeftovers() const {
  if (kind == Kind::AppImage) {
    QFile::remove(downloadPath(QString()));
    return;
  }
  if (kind != Kind::WindowsInstaller && kind != Kind::MacBundle) {
    return;
  }
  // This run's folder.
  const QString current = privateDownloadDirectory(false);
  if (!current.isEmpty()) {
    QDir(current).removeRecursively();
  }
  if (kind != Kind::WindowsInstaller) {
    return;
  }
  // Earlier runs' folders on Windows; one whose updater still runs stays
  // listed for the next start.
  QSettings settings;
  QStringList kept;
  for (const QString& folder : settings.value(kDownloadFolders).toStringList()) {
    if (isPrivateDownloadDirectory(folder) && QFileInfo::exists(folder) && !QDir(folder).removeRecursively()) {
      kept.append(folder);
    }
  }
  if (kept.isEmpty()) {
    settings.remove(kDownloadFolders);
  } else {
    settings.setValue(kDownloadFolders, kept);
  }
  // The fixed folder of earlier releases.
  QDir(QDir::temp().filePath(QStringLiteral("mitcad-update"))).removeRecursively();
}

bool processHasAdministratorRights() {
#ifdef _WIN32
  SID_IDENTIFIER_AUTHORITY authority = SECURITY_NT_AUTHORITY;
  PSID administrators = nullptr;
  if (!AllocateAndInitializeSid(&authority, 2, SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_ADMINS, 0, 0, 0, 0, 0,
                                0, &administrators)) {
    return false;
  }
  BOOL member = FALSE;
  if (!CheckTokenMembership(nullptr, administrators, &member)) {
    member = FALSE;
  }
  FreeSid(administrators);
  return member != FALSE;
#else
  return false;
#endif
}

bool restrictToUserRights(Installation& installation, bool testSource, bool administrator) {
  if (!testSource || administrator || installation.kind != Installation::Kind::WindowsInstaller ||
      !installation.allUsers) {
    return false;
  }
  installation.kind = Installation::Kind::NotifyOnly;
  installation.reason = QStringLiteral("an update from a test's source (MITCAD_UPDATE_URL, MITCAD_UPDATE_RELEASES_URL, "
                                       "MITCAD_UPDATE_TEST_KEY or MITCAD_UPDATE_TEST_CA) never asks for administrator "
                                       "rights, and this installation for all users would need them");
  return true;
}

QStringList windowsUpdaterArguments(const Installation& installation, const QString& file, const QString& sha256,
                                    const QString& log, qint64 pid) {
  return {QStringLiteral("--wait"),
          QString::number(pid),
          QStringLiteral("--installer"),
          QDir::toNativeSeparators(file),
          QStringLiteral("--sha256"),
          sha256,
          QStringLiteral("--dir"),
          QDir::toNativeSeparators(installation.location),
          QStringLiteral("--scope"),
          installation.allUsers ? QStringLiteral("all") : QStringLiteral("user"),
          QStringLiteral("--restart"),
          QDir::toNativeSeparators(QDir(installation.location).filePath(QStringLiteral("bin/mitcad.exe"))),
          QStringLiteral("--log"),
          QDir::toNativeSeparators(log)};
}

bool stageUpdate(const Installation& installation, const QString& file, const QString& sha256, const QString& from,
                 const QString& to, QString& error) {
  Staged update;
  update.kind = installation.kind;
  QString log;
  if (installation.kind == Installation::Kind::WindowsInstaller) {
    // The updater checks the installer again with this hash right before
    // it runs it (mitcad#603); it refuses to run one without.
    if (!updater::isSha256Hex(sha256.toStdWString())) {
      error = QCoreApplication::translate("Update", "The release gives no SHA-256 of the installer.");
      return false;
    }
    // The installer replaces bin, so the updater runs from the download's
    // folder.
    const QDir folder = QFileInfo(file).absoluteDir();
    const QString updaterCopy = folder.filePath(kUpdater);
    QFile::remove(updaterCopy);
    if (!QFile::copy(QDir(QCoreApplication::applicationDirPath()).filePath(kUpdater), updaterCopy)) {
      error = QCoreApplication::translate("Update", "Cannot copy %1 to %2.")
                  .arg(kUpdater, QDir::toNativeSeparators(folder.absolutePath()));
      return false;
    }
    log = folder.filePath(QStringLiteral("updater.log"));
    QFile::remove(log);
    update.program = updaterCopy;
    update.workingDirectory = folder.absolutePath();
    update.arguments =
        windowsUpdaterArguments(installation, file, sha256, log, QCoreApplication::applicationPid());
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
#ifdef __APPLE__
  } else if (installation.kind == Installation::Kind::MacBundle) {
    // Recheck after the download: permissions or location may have changed.
    const Installation current = Installation::detect();
    if (current.kind != Installation::Kind::MacBundle || current.location != installation.location) {
      error = QCoreApplication::translate("Update", "The application bundle can no longer be replaced: %1.")
                  .arg(current.reason);
      return false;
    }
    const QDir folder = QFileInfo(file).absoluteDir();
    const QString helper = folder.filePath(QStringLiteral("updater.sh"));
    const QString swapper = folder.filePath(QStringLiteral("mitcad-updater"));
    QFile::remove(helper);
    QFile::remove(swapper);
    if (!QFile::copy(QStringLiteral(":/update/UpdaterMacos.sh"), helper) ||
        !QFile::copy(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("mitcad-updater")), swapper) ||
        !QFile(swapper).setPermissions(QFile::ReadOwner | QFile::WriteOwner | QFile::ExeOwner)) {
      error = QCoreApplication::translate("Update", "Cannot prepare the macOS update helper.");
      return false;
    }
    log = folder.filePath(QStringLiteral("updater.log"));
    update.program = QStringLiteral("/bin/bash");
    update.workingDirectory = folder.absolutePath();
    update.arguments = {helper, QString::number(QCoreApplication::applicationPid()), file, installation.location, to, log};
#endif
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
