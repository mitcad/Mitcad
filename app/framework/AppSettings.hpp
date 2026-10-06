// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

namespace mitcad {

// The application's general settings (Preferences, General), kept in the
// user's settings (autosave/enabled, autosave/minutes). Minutes, like every
// unit the application chooses, are metric; there is no unit to pick.
struct GeneralSettings {
  static constexpr int kMinAutosaveMinutes = 1;
  static constexpr int kMaxAutosaveMinutes = 60;

  // Autosave (P8): a modified document is written to the recovery folder
  // this often (app/files/Autosave.hpp).
  bool autosave = true;
  int autosaveMinutes = 5;

  static GeneralSettings load();
  void save() const;
};

// Who the versions of projects are recorded by (P12d, Preferences, General;
// the settings versions/useGit, versions/name, versions/email,
// versions/confirmed): git's configured user (user.name and user.email)
// while useGit is on and git has one, else the name and email here. The
// first version saved shows the author once (confirmed).
struct VersionSettings {
  bool useGit = true;
  QString name;
  QString email;
  bool confirmed = false;

  // Both a name and an email address are given.
  bool complete() const;
  // "Name <email>", the form the version history takes.
  QString author() const;

  static VersionSettings load();
  void save() const;
};

// Where File > New Project puts projects by default: "Mitcad" in the
// user's documents (MITCAD_PROJECTS_DIR for tests).
QString projectsDirectory();

// Remote repositories (P12 remote, Preferences, Version Control; the
// settings remote/git, remote/checkMinutes, remote/autoPush): the git
// program that does the network work (empty: found on PATH, on Windows
// also where Git for Windows installs itself), how often the remote of
// the open design's project is checked for newer versions (when the
// design opens and then every so many minutes; 0: never on its own), and
// whether each version saved is sent to the remote at once.
struct RemoteSettings {
  static constexpr int kMaxCheckMinutes = 240;

  QString git;
  int checkMinutes = 10;
  bool autoPush = true;

  static RemoteSettings load();
  void save() const;
  // Gives the remote work (core/vcs) the git program: MITCAD_GIT set to
  // the one chosen, or as the environment had it when none is. Only while
  // no remote work runs (the variable is read on its thread).
  void apply() const;
};

// The recovery folder of autosave: "autosave" in the user's local
// application data (~/.local/share/Mitcad/Mitcad/autosave on Linux,
// %LOCALAPPDATA%\Mitcad\Mitcad\autosave on Windows), or MITCAD_AUTOSAVE_DIR
// (tests). Never a project's folder; empty when the system gives no place.
QString recoveryDirectory();

} // namespace mitcad
