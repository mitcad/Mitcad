// SPDX-License-Identifier: MIT
// Preferences › Export Settings and Import Settings (mitcad#1889): the
// groups of keys, a file written and read back (LF line ends, the header,
// only the chosen groups), a file written on Windows (CRLF line ends,
// Windows paths, the registry's values as text: tests/data/
// windows.mitcad-settings) read on any platform, a newer settings version
// and a file that is no settings file refused, and an import's plan: the
// groups replaced, keys of no group never written, missing folders and
// programs skipped and listed.
#include "UnitTests.hpp"
#include "framework/SettingsTransfer.hpp"

#include <cstdio>

#include <QByteArray>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "settings_transfer.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QString text(const char* value) { return QString::fromUtf8(value); }

bool writeText(const QString& path, const QByteArray& bytes) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size();
}

QByteArray readBytes(const QString& path) {
  QFile file(path);
  return file.open(QIODevice::ReadOnly) ? file.readAll() : QByteArray();
}

void groupTests() {
  using namespace mitcad;
  CHECK(settingsGroupOf(text("view/smallCircles")) == text("preferences"));
  CHECK(settingsGroupOf(text("documents/openInCurrentWindow")) == text("preferences"));
  CHECK(settingsGroupOf(text("autosave/minutes")) == text("preferences"));
  CHECK(settingsGroupOf(text("view/navigationScheme")) == text("navigation"));
  CHECK(settingsGroupOf(text("view/spaceMouseLockHorizon")) == text("navigation"));
  CHECK(settingsGroupOf(text("view/navigationBar")) == text("preferences"));
  CHECK(settingsGroupOf(text("shortcuts/view.iso")) == text("shortcuts"));
  CHECK(settingsGroupOf(text("shortcuts.sketch/sketch.line")) == text("shortcuts"));
  CHECK(settingsGroupOf(text("navigationBarPins")) == text("pins"));
  CHECK(settingsGroupOf(text("toolbarPins/SOLID")) == text("pins"));
  CHECK(settingsGroupOf(text("quickAccess/commands")) == text("pins"));
  CHECK(settingsGroupOf(text("quickAccess/visible")) == text("preferences"));
  CHECK(settingsGroupOf(text("floating/leftSplit")) == text("layout"));
  CHECK(settingsGroupOf(text("libraries/sources")) == text("libraries"));
  CHECK(settingsGroupOf(text("recentFiles")) == text("recent"));
  CHECK(settingsGroupOf(text("textures/folders")) == text("folders"));
  CHECK(settingsGroupOf(text("print/slicerProgram")) == text("folders"));
  CHECK(settingsGroupOf(text("print/folderOnly")) == text("preferences"));
  // Never carried: what a chat allowed always, window places, a prefix's
  // own name.
  CHECK(settingsGroupOf(text("chat/always/test-agent")).isEmpty());
  CHECK(settingsGroupOf(text("mainWindow/geometry")).isEmpty());
  CHECK(settingsGroupOf(text("shortcuts")).isEmpty());
  CHECK(settingsGroupOf(text("autosave/")).isEmpty());
  // Recent files and the folders and programs are left out by default.
  const QStringList defaults = defaultSettingsGroups();
  CHECK(defaults.contains(text("preferences")) && defaults.contains(text("shortcuts")) &&
        defaults.contains(text("pins")));
  CHECK(!defaults.contains(text("recent")) && !defaults.contains(text("folders")));
  CHECK(settingsGroup(text("layout")) != nullptr && settingsGroup(text("nothing")) == nullptr);
}

void roundTripTests(const QString& folder) {
  using namespace mitcad;
  QSettings from(folder + text("/from.ini"), QSettings::IniFormat);
  from.setValue(text("view/smallCircles"), text("full"));
  from.setValue(text("autosave/minutes"), 7);
  from.setValue(text("shortcuts/view.iso"), text("Shift+F8"));
  from.setValue(text("navigationBarPins"), text("solid.parameters,inspect.measure"));
  from.setValue(text("textures/folders"), QStringList{text("/a"), text("/b")});
  from.setValue(text("recentFiles"), text("/designs/one.mitcad"));
  from.setValue(text("chat/always/test-agent"), text("edit"));
  from.sync();

  const QString path = folder + text("/out.mitcad-settings");
  QString error;
  int count = 0;
  CHECK(writeSettingsFile(from, {text("preferences"), text("shortcuts"), text("pins")}, path, &error, &count));
  CHECK(error.isEmpty());
  CHECK(count == 4);
  const QByteArray bytes = readBytes(path);
  CHECK(!bytes.contains('\r'));
  CHECK(bytes.contains("[MitCadSettings]"));
  CHECK(!bytes.contains("recentFiles"));
  CHECK(!bytes.contains("always"));
  CHECK(!bytes.contains("textures"));

  const SettingsFileContents read = readSettingsFile(path);
  CHECK(read.problem == SettingsFileContents::Problem::None);
  CHECK(read.version == kSettingsFileVersion);
  CHECK(read.platform == settingsPlatform());
  CHECK(read.groups == (QStringList{text("preferences"), text("shortcuts"), text("pins")}));
  CHECK(read.values.value(text("view/smallCircles")).toString() == text("full"));
  CHECK(read.values.value(text("autosave/minutes")).toInt() == 7);
  CHECK(read.values.value(text("shortcuts/view.iso")).toString() == text("Shift+F8"));
  CHECK(read.values.value(text("navigationBarPins")).toString() == text("solid.parameters,inspect.measure"));
  CHECK(read.count(text("preferences")) == 2);
  CHECK(read.count(text("pins")) == 1);

  // Written again over the same file: replaced, not merged.
  CHECK(writeSettingsFile(from, {text("shortcuts")}, path, &error, &count));
  CHECK(count == 1);
  const SettingsFileContents again = readSettingsFile(path);
  CHECK(again.groups == QStringList{text("shortcuts")});
  CHECK(again.values.size() == 1);

  // Replacing the groups: the file's settings written, the group's other
  // settings removed, other groups and keys of no group as they were.
  QSettings to(folder + text("/to.ini"), QSettings::IniFormat);
  to.setValue(text("shortcuts/edit.undo"), text("Ctrl+Y"));
  to.setValue(text("view/smallCircles"), text("adaptive"));
  to.setValue(text("chat/always/test-agent"), text("all"));
  const SettingsImportPlan plan = planSettingsImport(again, {text("shortcuts"), text("preferences")}, to);
  CHECK(plan.groups == QStringList{text("shortcuts")});
  CHECK(plan.write.value(text("shortcuts/view.iso")).toString() == text("Shift+F8"));
  CHECK(plan.remove == QStringList{text("shortcuts/edit.undo")});
  applySettingsImport(to, plan);
  CHECK(to.value(text("shortcuts/view.iso")).toString() == text("Shift+F8"));
  CHECK(!to.contains(text("shortcuts/edit.undo")));
  CHECK(to.value(text("view/smallCircles")).toString() == text("adaptive"));
  CHECK(to.value(text("chat/always/test-agent")).toString() == text("all"));
}

void windowsFileTests(const QString& folder) {
  using namespace mitcad;
  // The committed fixture as a file of its own (CRLF line ends kept: the
  // repository does not convert *.mitcad-settings).
  const QByteArray fixture = readBytes(text(":/settings-transfer/windows.mitcad-settings"));
  CHECK(fixture.contains("\r\n"));
  const QString path = folder + text("/windows.mitcad-settings");
  CHECK(writeText(path, fixture));

  const SettingsFileContents read = readSettingsFile(path);
  CHECK(read.problem == SettingsFileContents::Problem::None);
  CHECK(read.version == 1);
  CHECK(read.application == text("0.63.0"));
  CHECK(read.platform == text("windows"));
  CHECK(settingsPlatformName(read.platform) == text("Windows"));
  CHECK(read.written == text("2026-11-15T09:30:00Z"));
  // A group this version does not know ("someday") is passed over.
  CHECK(read.groups == (QStringList{text("preferences"), text("navigation"), text("shortcuts"), text("pins"),
                                    text("libraries"), text("folders")}));
  // The registry's numbers and flags as text, read as the settings read them.
  CHECK(read.values.value(text("autosave/minutes")).toInt() == 5);
  CHECK(read.values.value(text("documents/openInCurrentWindow")).toBool());
  CHECK(read.values.value(text("view/smallCircles")).toString() == text("full"));
  CHECK(read.values.value(text("view/orbitSpeed")).toInt() == 150);
  CHECK(read.values.value(text("shortcuts/view.iso")).toString() == text("Shift+F8"));
  CHECK(read.values.value(text("shortcuts.sketch/sketch.line")).toString() == text("Shift+L"));
  CHECK(read.values.value(text("navigationBarPins")).toString() == text("solid.parameters,inspect.measure"));
  CHECK(read.values.value(text("toolbarPins/SOLID")).toString() == text("solid.sweep,solid.loft"));
  CHECK(read.values.value(text("textures/folders")).toStringList() ==
        (QStringList{text("C:/Shared/Textures"), text("D:/Old/Textures")}));
  // Keys of no group, or of a group the file does not list, are not read.
  CHECK(!read.values.contains(text("chat/always/test-agent")));
  CHECK(!read.values.contains(text("mainWindow/maximized")));
  CHECK(!read.values.contains(text("recentFiles")));
  CHECK(read.values.contains(text("chat/agent")));

  // On this machine only C:/Shared/Textures is there (a stand-in for the
  // file system, the same on every platform).
  const PathExists exists = [](const QString& candidate) { return candidate == QLatin1String("C:/Shared/Textures"); };
  QSettings to(folder + text("/here.ini"), QSettings::IniFormat);
  to.setValue(text("remote/git"), text("/usr/bin/git"));
  to.setValue(text("fonts/folders"), QStringList{text("/home/fonts")});
  to.setValue(text("print/slicerName"), text("Here"));
  to.setValue(text("print/slicerProgram"), text("/usr/bin/slicer"));
  to.setValue(text("mcp/folders"), QStringList{text("/designs")});
  const SettingsImportPlan plan = planSettingsImport(read, read.groups, to, exists);
  QStringList skipped;
  for (const SkippedSettingsPath& entry : plan.skipped) {
    skipped << entry.path;
  }
  skipped.sort();
  CHECK(skipped == (QStringList{text("C:/Program Files/Git/cmd/git.exe"), text("C:/Program Files/Slicer/slicer.exe"),
                                text("D:/Fonts"), text("D:/Libraries/Own"), text("D:/Old/Textures")}));
  // A list keeps the folders that are here; a missing program keeps this
  // machine's, its name with it; a bare program name is no path.
  CHECK(plan.write.value(text("textures/folders")).toStringList() == QStringList{text("C:/Shared/Textures")});
  CHECK(!plan.write.contains(text("remote/git")) && !plan.remove.contains(text("remote/git")));
  CHECK(!plan.write.contains(text("fonts/folders")) && !plan.remove.contains(text("fonts/folders")));
  CHECK(!plan.write.contains(text("print/slicerProgram")) && !plan.write.contains(text("print/slicerName")));
  CHECK(!plan.remove.contains(text("print/slicerName")));
  CHECK(plan.write.value(text("chat/acp/command")).toString() == text("acp-agent"));
  // The folders group is replaced: a setting of it the file does not have
  // goes back to its default.
  CHECK(plan.remove.contains(text("mcp/folders")));
  // A library source that is a folder not here is dropped, an address kept.
  const QJsonArray sources =
      QJsonDocument::fromJson(plan.write.value(text("libraries/sources")).toByteArray()).array();
  CHECK(sources.size() == 1);
  CHECK(sources.at(0).toObject().value(QStringLiteral("url")).toString() == text("https://example.invalid/parts.git"));

  applySettingsImport(to, plan);
  CHECK(to.value(text("remote/git")).toString() == text("/usr/bin/git"));
  CHECK(to.value(text("print/slicerName")).toString() == text("Here"));
  CHECK(to.value(text("fonts/folders")).toStringList() == QStringList{text("/home/fonts")});
  CHECK(!to.contains(text("mcp/folders")));
  CHECK(to.value(text("shortcuts/view.iso")).toString() == text("Shift+F8"));
  CHECK(to.value(text("view/perspective")).toBool());
  CHECK(!to.contains(text("chat/always/test-agent")));
}

void refusalTests(const QString& folder) {
  using namespace mitcad;
  const QString newer = folder + text("/newer.mitcad-settings");
  CHECK(writeText(newer, "[MitCadSettings]\nversion=99\ngroups=preferences\n\n[view]\nsmallCircles=full\n"));
  const SettingsFileContents fromNewer = readSettingsFile(newer);
  CHECK(fromNewer.problem == SettingsFileContents::Problem::Newer);
  CHECK(fromNewer.version == 99);
  CHECK(fromNewer.values.isEmpty());
  // Said in words, with both versions.
  const QString said = settingsFileProblemText(fromNewer);
  CHECK(said.contains(text("newer MitCad")) && said.contains(text("99")) && said.contains(text("version 1")));

  const QString other = folder + text("/other.mitcad-settings");
  CHECK(writeText(other, "Just some notes, not settings.\n"));
  CHECK(readSettingsFile(other).problem == SettingsFileContents::Problem::NotSettings);
  CHECK(!settingsFileProblemText(readSettingsFile(other)).isEmpty());
  CHECK(readSettingsFile(folder + text("/missing.mitcad-settings")).problem ==
        SettingsFileContents::Problem::Unreadable);
}

} // namespace

int settingsTransferTests() {
  QTemporaryDir folder;
  CHECK(folder.isValid());
  groupTests();
  roundTripTests(folder.path());
  windowsFileTests(folder.path());
  refusalTests(folder.path());
  return failures;
}
MITCAD_UNIT_TEST(settingsTransferTests);
