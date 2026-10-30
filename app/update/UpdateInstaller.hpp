// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9): how this installation can replace itself,
// and the replacement once Mitcad has quit.

#include <optional>

#include <QString>
#include <QStringList>
#include <QtGlobal>

namespace mitcad {

struct Installation {
  enum class Kind {
    // The portable .zip, a development build, a system package or an
    // installation the user cannot write to: a newer release is announced
    // with its release page, nothing is installed.
    NotifyOnly,
    // Installed by the Windows installer (NSIS): Uninstall.exe next to bin.
    WindowsInstaller,
    // A Linux AppImage (APPIMAGE, set by its runtime) the user can replace.
    AppImage,
    // A writable macOS application bundle, outside App Translocation.
    MacBundle,
  };
  Kind kind = Kind::NotifyOnly;
  // The installation folder (Windows), AppImage (Linux), or macOS bundle.
  QString location;
  // Windows: installed for all users (Program Files, HKLM; an update asks
  // for elevation) rather than just for the current user (mitcad#118). The
  // installer's installation.ini says which; an installation without it is
  // an earlier release's, which was always for all users.
  bool allUsers = false;
  // Why it can only announce (for the log).
  QString reason;

  static Installation detect();
  bool canInstall() const { return kind != Kind::NotifyOnly; }
  QString describe() const;
  // Where the download of `version` goes: on Windows and macOS a private,
  // unpredictable folder in the user's temp folder (mitcad-update-<random>,
  // one per run, owner-only on macOS; mitcad#603), which Windows remembers
  // in updates/downloadFolders until a later start has removed it; a
  // hidden file next to the AppImage, so that a rename replaces it (Linux);
  // empty when it cannot install.
  QString downloadPath(const QString& version) const;
  // Removes what an earlier update left there: this run's folder, and on
  // Windows the remembered folders of earlier runs (only such folders,
  // directly in the temp folder) and the fixed folder of earlier releases.
  void removeLeftovers() const;
};

// Whether this process has administrator rights already (Windows: a member
// of the Administrators group with an elevated token); false elsewhere.
bool processHasAdministratorRights();

// mitcad#921: an update from a test's source (UpdateSource::testSource: an
// address, key or certificate a test build took from the environment)
// never asks for administrator rights. A Windows installation for all users
// then only announces, unless this process has those rights already
// (`administrator`; the installer tests run so), when the update elevates
// nothing. True when it changed `installation` (with the reason).
bool restrictToUserRights(Installation& installation, bool testSource, bool administrator);

// mitcad-updater.exe's command line (app/update/UpdaterWindows.cpp) for the
// verified installer `file` with its SHA-256 `sha256` from the verified
// manifest (64 lower-case hex digits), which the updater checks again right
// before it runs the installer; it waits for process `pid` and logs to
// `log`.
QStringList windowsUpdaterArguments(const Installation& installation, const QString& file, const QString& sha256,
                                    const QString& log, qint64 pid);

// Arranges for the verified download `file` to replace the installation once
// Mitcad has quit, and for the new version to start then: on Windows
// mitcad-updater.exe, copied next to the download, waits for this process
// to end, checks the installer again against `sha256` (the SHA-256 of the
// verified manifest; refused when it is not 64 lower-case hex digits),
// runs it silently into the installation folder and its scope (elevated
// only for all users) and starts Mitcad; an AppImage is
// replaced by the download (an atomic rename) and started. On macOS a
// helper waits for this process, mounts the image read-only, copies the app
// beside this bundle, swaps it with rollback, and relaunches through open.
// The settings remember the update (updates/installing,
// updates/installingFrom, updates/installLog), so that the next start tells
// how it went. False with the reason when it cannot be arranged.
bool stageUpdate(const Installation& installation, const QString& file, const QString& sha256, const QString& from,
                 const QString& to, QString& error);
// The window did not close after all: nothing happens at the end.
void unstageUpdate();

// At start-up after an update: whether it installed (this is the version it
// installed) and the updater's last words when not. Read once: the
// settings keep the outcome in updates/lastResult only.
struct FinishedUpdate {
  bool installed = false;
  QString from;
  QString to;
  QString detail;
};
std::optional<FinishedUpdate> takeFinishedUpdate(const QString& currentVersion);

} // namespace mitcad
