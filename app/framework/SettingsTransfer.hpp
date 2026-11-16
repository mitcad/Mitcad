// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QList>
#include <QMap>
#include <QString>
#include <QStringList>
#include <QVariant>

class QSettings;

namespace mitcad {

// Preferences › Export Settings and Import Settings (mitcad#1889, GitHub
// mitcad/Mitcad#357, app/COMMANDS.md "Exporting and importing settings"):
// one setup carried between machines in a `.mitcad-settings` file. The
// file is Qt's INI text (UTF-8, LF line ends, the same on every platform):
// a [MitCadSettings] section with the settings version, the MitCad and
// platform that wrote it, when, and the groups it holds, then the settings
// under their own keys. Only keys of a known group are written and read:
// a file cannot set anything else (what a chat allowed always, API keys,
// window places). Plain Qt Core.

// The settings version this MitCad writes and the newest it reads.
inline constexpr int kSettingsFileVersion = 1;
// The file's extension, without the dot.
inline constexpr const char* kSettingsFileSuffix = "mitcad-settings";

// A group of settings, as the export and import dialogs offer them.
struct SettingsGroup {
  const char* id;    // in the file and the logs: "preferences", "shortcuts", ...
  const char* name;  // translated in the "Settings" context
  bool defaultOn;    // checked by default in Export Settings
};
// Every group, in the dialogs' order: Preferences, Navigation and 3D mouse,
// Keyboard shortcuts, Pins, Ribbon and panels, Library sources, Recent
// files (off), Folders and programs (off).
const QList<SettingsGroup>& settingsGroups();
// The group's entry; nullptr for an id that is not one.
const SettingsGroup* settingsGroup(const QString& id);
// The ids of the groups checked by default.
QStringList defaultSettingsGroups();

// The group a settings key belongs to ("" for none: not carried).
QString settingsGroupOf(const QString& key);

// What the file holds.
struct SettingsFileContents {
  enum class Problem {
    None,
    Unreadable,  // no such file, or not readable
    NotSettings, // no MitCad settings in it
    Newer,       // a newer settings version than this MitCad reads
  };
  Problem problem = Problem::None;
  int version = 0;
  QString application; // "0.63.0"
  QString platform;    // "linux", "windows", "macos"
  QString written;     // ISO 8601, UTC
  // The groups the file says it holds that this MitCad knows, in the
  // order of settingsGroups().
  QStringList groups;
  // The settings of those groups, by key.
  QMap<QString, QVariant> values;
  // The number of settings of a group in the file.
  int count(const QString& group) const;
};

// The problem in words, for a message box ("" for None).
QString settingsFileProblemText(const SettingsFileContents& contents);

// Writes the settings of `groups` in `from` to `path` (replacing the
// file). False with `error` in words when the file cannot be written.
// `count` is the number of settings written.
bool writeSettingsFile(const QSettings& from, const QStringList& groups, const QString& path, QString* error,
                       int* count = nullptr);
// Reads a settings file; `problem` says why nothing can be imported.
SettingsFileContents readSettingsFile(const QString& path);

// A path in the file that is not on this machine: skipped, not written.
struct SkippedSettingsPath {
  QString group;
  QString key;
  QString path;
};

// What an import of `groups` (those of the file only) does to `current`:
// each group is replaced, so its settings in the file are written and its
// other settings here removed (back to their defaults). A folder, program
// or library folder of the file that `exists` does not find is skipped
// and listed; a list of folders keeps those that exist, and a program that
// is missing keeps this machine's own (its name and arguments with it).
struct SettingsImportPlan {
  QStringList groups;
  QMap<QString, QVariant> write;
  QStringList remove;
  QList<SkippedSettingsPath> skipped;
};
using PathExists = std::function<bool(const QString& path)>;
SettingsImportPlan planSettingsImport(const SettingsFileContents& contents, const QStringList& groups,
                                      const QSettings& current, const PathExists& exists = {});
// Writes and removes as planned.
void applySettingsImport(QSettings& to, const SettingsImportPlan& plan);

// The platform's name in the file ("linux", "windows", "macos") and in
// words ("Linux", "Windows", "macOS"; the name itself for another).
QString settingsPlatform();
QString settingsPlatformName(const QString& platform);

} // namespace mitcad
