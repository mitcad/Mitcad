// SPDX-License-Identifier: MIT
#pragma once

// Automatic updates (mitcad#9): how this installation can replace itself,
// and the replacement once Mitcad has quit.

#include <optional>

#include <QString>

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
  };
  Kind kind = Kind::NotifyOnly;
  // The installation folder (Windows) or the AppImage file (Linux).
  QString location;
  // Why it can only announce (for the log).
  QString reason;

  static Installation detect();
  bool canInstall() const { return kind != Kind::NotifyOnly; }
  QString describe() const;
  // Where the download of `version` goes: a folder of Mitcad's in the
  // user's temp folder (Windows), a hidden file next to the AppImage, so
  // that a rename replaces it (Linux).
  QString downloadPath(const QString& version) const;
  // Removes what an earlier update left there.
  void removeLeftovers() const;
};

// Arranges for the verified download `file` to replace the installation once
// Mitcad has quit, and for the new version to start then: on Windows
// mitcad-updater.exe, copied next to the download, waits for this process
// to end, runs the installer silently into the installation folder and
// starts Mitcad; an AppImage is replaced by the download (an atomic rename)
// and started. The settings remember the update (updates/installing,
// updates/installingFrom, updates/installLog), so that the next start tells
// how it went. False with the reason when it cannot be arranged.
bool stageUpdate(const Installation& installation, const QString& file, const QString& from, const QString& to,
                 QString& error);
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
