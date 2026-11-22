// SPDX-License-Identifier: MIT
#include "SettingsTransfer.hpp"

#include <QByteArray>
#include <QCoreApplication>
#include <QDateTime>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaType>
#include <QSaveFile>
#include <QSettings>
#include <QTemporaryDir>

#include <utility>

#include "ShortcutBindings.hpp"

namespace mitcad {
namespace {

// The file's own section.
const QString kHeader = QStringLiteral("MitCadSettings/");

// How a setting's value names places on the machine.
enum class PathKind {
  None,
  Folders,        // a list of folders (texture, font, AI assistants' folders)
  Program,        // one program; a bare name (found on the PATH) is no path
  LibrarySources, // libraries/sources: a JSON array whose "url" may be a folder
};

// A key, or every key under a prefix ending in '/' or '.', of a group.
// The first rule that matches a key decides its group.
struct SettingsRule {
  const char* key;
  const char* group;
  PathKind kind;
};

// The keys each group carries. Nothing else is written or read: what a
// chat allowed always (chat/always), notices, window places, file dialogs'
// folders, caches and the like stay on their machine.
const SettingsRule kRules[] = {
    // Navigation and 3D mouse: Preferences › Navigation.
    {"view/navigationScheme", "navigation", PathKind::None},
    {"view/navigation", "navigation", PathKind::None},
    {"view/navigationBindings", "navigation", PathKind::None},
    {"view/zoomToCursor", "navigation", PathKind::None},
    {"view/reverseZoom", "navigation", PathKind::None},
    {"view/constrainedOrbit", "navigation", PathKind::None},
    {"view/animate", "navigation", PathKind::None},
    {"view/orbitSpeed", "navigation", PathKind::None},
    {"view/panSpeed", "navigation", PathKind::None},
    {"view/zoomSpeed", "navigation", PathKind::None},
    {"view/trackpadSpeed", "navigation", PathKind::None},
    {"view/reversePanHorizontal", "navigation", PathKind::None},
    {"view/reversePanVertical", "navigation", PathKind::None},
    {"view/reverseScrollOrbit", "navigation", PathKind::None},
    {"view/orbitCenter", "navigation", PathKind::None},
    {"view/turnStep", "navigation", PathKind::None},

    // Folders and programs on this machine (off by default).
    {"textures/folders", "folders", PathKind::Folders},
    {"fonts/folders", "folders", PathKind::Folders},
    {"mcp/folders", "folders", PathKind::Folders},
    {"remote/git", "folders", PathKind::Program},
    {"render/worker", "folders", PathKind::Program},
    {"chat/acp/command", "folders", PathKind::Program},
    {"print/slicerProgram", "folders", PathKind::Program},
    {"print/slicerName", "folders", PathKind::None},
    {"print/slicerArguments", "folders", PathKind::None},
    {"print/slicerArgumentsAfter", "folders", PathKind::None},

    // Preferences: the other pages, and the View menu's display choices.
    // General.
    {"autosave/", "preferences", PathKind::None},
    {"import/geometricTolerance", "preferences", PathKind::None},
    {"units/", "preferences", PathKind::None},
    {"documents/openInCurrentWindow", "preferences", PathKind::None},
    {"documents/openDesignsIn", "preferences", PathKind::None},
    {"documents/quitOnLastClose", "preferences", PathKind::None},
    {"documents/newDesignSave", "preferences", PathKind::None},
    {"interface/language", "preferences", PathKind::None},
    {"components/askActivate", "preferences", PathKind::None},
    {"view/showVersion", "preferences", PathKind::None},
    {"browser/consumeSketches", "preferences", PathKind::None},
    {"extrude/startOperation", "preferences", PathKind::None},
    // Display.
    {"view/perspective", "preferences", PathKind::None},
    {"view/cubeProjection", "preferences", PathKind::None},
    {"view/cubeSize", "preferences", PathKind::None},
    {"view/focalLength", "preferences", PathKind::None},
    {"view/anaglyph", "preferences", PathKind::None},
    {"view/anaglyphFocalLength", "preferences", PathKind::None},
    {"view/stereoDepth", "preferences", PathKind::None},
    {"view/stereoConvergence", "preferences", PathKind::None},
    {"view/background", "preferences", PathKind::None},
    {"view/top", "preferences", PathKind::None},
    {"view/bottom", "preferences", PathKind::None},
    {"view/ghostInactive", "preferences", PathKind::None},
    {"view/activePickPriority", "preferences", PathKind::None},
    {"view/navigationBar", "preferences", PathKind::None},
    {"view/selectionColor", "preferences", PathKind::None},
    {"view/selectedLineWidth", "preferences", PathKind::None},
    {"view/menuBar", "preferences", PathKind::None},
    {"view/windowMenuBar", "preferences", PathKind::None},
    {"view/smallCircles", "preferences", PathKind::None},
    {"view/originStyle", "preferences", PathKind::None},
    {"view/originSize", "preferences", PathKind::None},
    {"view/originSizeMm", "preferences", PathKind::None},
    {"render/device", "preferences", PathKind::None},
    {"appearance/", "preferences", PathKind::None},
    {"quickAccess/visible", "preferences", PathKind::None},
    {"drawing/alwaysShowCorners", "preferences", PathKind::None},
    {"drawing/cornerDotSize", "preferences", PathKind::None},
    // The View menu.
    {"view/style", "preferences", PathKind::None},
    {"view/componentColors", "preferences", PathKind::None},
    {"view/grid", "preferences", PathKind::None},
    {"view/gridSpacing", "preferences", PathKind::None},
    {"view/snapToGrid", "preferences", PathKind::None},
    {"view/commentPins", "preferences", PathKind::None},
    // Cache, Cloud, Updates, 3D Print, Diagnostics, Textures.
    {"cache/", "preferences", PathKind::None},
    {"remote/checkMinutes", "preferences", PathKind::None},
    {"remote/autoPush", "preferences", PathKind::None},
    {"remote/allowLive", "preferences", PathKind::None},
    {"remote/defaultBroker", "preferences", PathKind::None},
    {"versions/name", "preferences", PathKind::None},
    {"versions/email", "preferences", PathKind::None},
    {"updates/automatic", "preferences", PathKind::None},
    {"updates/channel", "preferences", PathKind::None},
    {"print/folderOnly", "preferences", PathKind::None},
    {"print/format", "preferences", PathKind::None},
    {"print/refinement", "preferences", PathKind::None},
    {"print/deviation", "preferences", PathKind::None},
    {"print/angle", "preferences", PathKind::None},
    {"print/reuseWindow", "preferences", PathKind::None},
    {"diagnostics/debugLog", "preferences", PathKind::None},
    {"textures/allowNetwork", "preferences", PathKind::None},
    {"textures/resolution", "preferences", PathKind::None},
    // AI Assistants (never chat/always, chat/notice/ or API keys, which
    // live in the keychain).
    {"chat/agent", "preferences", PathKind::None},
    {"chat/approvals", "preferences", PathKind::None},
    {"chat/history", "preferences", PathKind::None},
    {"chat/off", "preferences", PathKind::None},
    {"chat/model/", "preferences", PathKind::None},
    {"chat/api/", "preferences", PathKind::None},
    {"chat/acp/enabled", "preferences", PathKind::None},
    {"chat/acp/arguments", "preferences", PathKind::None},
    {"ai/agentsMayResolveComments", "preferences", PathKind::None},
    // Sketch, Suppliers, Assembly.
    {"sketch/scaleToFirstDimension", "preferences", PathKind::None},
    {"sketch/clickAddsToSelection", "preferences", PathKind::None},
    {"sketch/escLeavesUnchangedSketch", "preferences", PathKind::None},
    {"sketch/showProfilesOnOpen", "preferences", PathKind::None},
    {"sketch/projectFaceOnCreate", "preferences", PathKind::None},
    {"suppliers/region", "preferences", PathKind::None},
    {"suppliers/shops", "preferences", PathKind::None},
    {"assembly/jointComponent", "preferences", PathKind::None},

    // Keyboard shortcuts: "shortcuts/<id>" and "shortcuts.<context>/<id>".
    {"shortcuts/", "shortcuts", PathKind::None},
    {"shortcuts.", "shortcuts", PathKind::None},

    // Pins: the navigation bar's, the toolbar's and the quick-access
    // group's commands.
    {"navigationBarPins", "pins", PathKind::None},
    // An earlier version's toolbar pins (read from its files; moved into
    // the command layout's user layer once imported, mitcad#1951).
    {"toolbarPins/", "pins", PathKind::None},
    // The user's layer of the command layout (mitcad#1951): where the
    // commands sit, as a difference from the default.
    {"commandLayout/user", "pins", PathKind::None},
    {"quickAccess/commands", "pins", PathKind::None},
    // The top bar's left group (mitcad#2129): its ordered items, and that
    // the old choice was migrated into them.
    {"quickAccess/items", "pins", PathKind::None},
    {"quickAccess/itemsMigrated", "pins", PathKind::None},

    // The ribbon's and panels' layout: the left column's split, the panels'
    // and cards' widths, the command panels' lists of picks (mitcad#2064),
    // the Configuration card's disclosure.
    {"floating/leftSplit", "layout", PathKind::None},
    {"native/leftSplit", "layout", PathKind::None},
    {"commandPanel/dockWidth", "layout", PathKind::None},
    {"commandPanel/picksShown", "layout", PathKind::None},
    {"browser/cardWidth", "layout", PathKind::None},
    {"inspector/cardWidth", "layout", PathKind::None},
    {"projects/cardWidth", "layout", PathKind::None},
    {"projects/dockWidth", "layout", PathKind::None},
    {"projects/panel", "layout", PathKind::None},
    {"drawings/panelWidth", "layout", PathKind::None},
    {"configuration/panelCollapsed", "layout", PathKind::None},
    // Not here: the window's place (mainWindow/, mitcad#1710) and the Docked
    // layout's kept dock arrangement (dockState/, mitcad#2070). They name
    // this machine's screens, and Qt warns against restoreState with data
    // of unknown origin.

    // Library sources (their folders checked on import).
    {"libraries/sources", "libraries", PathKind::LibrarySources},
    {"libraries/useGit", "libraries", PathKind::None},

    // Recent files (off by default).
    {"recentFiles", "recent", PathKind::None},
    {"start/recentOpened", "recent", PathKind::None},
    {"start/pinnedDesigns", "recent", PathKind::None},
};

// The program's companions: skipped with it when it is missing, so that a
// slicer's name and arguments stay with this machine's program.
const char* const kSlicerCompanions[] = {"print/slicerName", "print/slicerArguments", "print/slicerArgumentsAfter"};

bool matches(const char* rule, const QString& key) {
  const QLatin1String text(rule);
  if (text.endsWith(QLatin1Char('/')) || text.endsWith(QLatin1Char('.'))) {
    return key.startsWith(text) && key.size() > text.size();
  }
  return key == text;
}

const SettingsRule* ruleOf(const QString& key) {
  for (const SettingsRule& rule : kRules) {
    if (matches(rule.key, key)) {
      return &rule;
    }
  }
  // The 3D mouse's settings (view/spaceMouse, view/spaceMouseSpeed, ...).
  static const SettingsRule spaceMouse{"view/spaceMouse", "navigation", PathKind::None};
  if (key.startsWith(QLatin1String(spaceMouse.key))) {
    return &spaceMouse;
  }
  return nullptr;
}

PathKind kindOf(const QString& key) {
  const SettingsRule* rule = ruleOf(key);
  return rule != nullptr ? rule->kind : PathKind::None;
}

// A value as a list: a list, or one text (an INI file's single entry).
QStringList listOf(const QVariant& value) {
  if (value.metaType().id() == QMetaType::QStringList || value.metaType().id() == QMetaType::QVariantList) {
    return value.toStringList();
  }
  const QString text = value.toString();
  return text.isEmpty() ? QStringList() : QStringList{text};
}

// A program's value names a place when it has a folder in it; a bare name
// is looked up on the PATH where it runs.
bool namesPlace(const QString& program) {
  return program.contains(QLatin1Char('/')) || program.contains(QLatin1Char('\\'));
}

// A library source that is a folder, not an address ("https://...",
// "ssh://...", "git@host:...").
bool isFolderSource(const QString& url) {
  if (url.contains(QLatin1String("://"))) {
    return false;
  }
  const qsizetype at = url.indexOf(QLatin1Char('@'));
  const qsizetype colon = url.indexOf(QLatin1Char(':'));
  if (at > 0 && colon > at) {
    return false; // scp-like git address
  }
  return !url.trimmed().isEmpty();
}

} // namespace

const QList<SettingsGroup>& settingsGroups() {
  static const QList<SettingsGroup> groups = {
      {"preferences", QT_TRANSLATE_NOOP("Settings", "Preferences"), true},
      {"navigation", QT_TRANSLATE_NOOP("Settings", "Navigation and 3D mouse"), true},
      {"shortcuts", QT_TRANSLATE_NOOP("Settings", "Keyboard shortcuts"), true},
      {"pins", QT_TRANSLATE_NOOP("Settings", "Navigation bar, toolbar and quick-access pins"), true},
      {"layout", QT_TRANSLATE_NOOP("Settings", "Toolbar and panels layout"), true},
      {"libraries", QT_TRANSLATE_NOOP("Settings", "Library sources"), true},
      {"recent", QT_TRANSLATE_NOOP("Settings", "Recent files"), false},
      {"folders", QT_TRANSLATE_NOOP("Settings", "Folders and programs on this computer"), false},
  };
  return groups;
}

const SettingsGroup* settingsGroup(const QString& id) {
  for (const SettingsGroup& group : settingsGroups()) {
    if (id == QLatin1String(group.id)) {
      return &group;
    }
  }
  return nullptr;
}

QStringList defaultSettingsGroups() {
  QStringList ids;
  for (const SettingsGroup& group : settingsGroups()) {
    if (group.defaultOn) {
      ids << QLatin1String(group.id);
    }
  }
  return ids;
}

QString settingsGroupOf(const QString& key) {
  const SettingsRule* rule = ruleOf(key);
  return rule != nullptr ? QLatin1String(rule->group) : QString();
}

int SettingsFileContents::count(const QString& group) const {
  int n = 0;
  for (auto it = values.cbegin(); it != values.cend(); ++it) {
    if (settingsGroupOf(it.key()) == group) {
      ++n;
    }
  }
  return n;
}

QString settingsPlatform() {
#if defined(Q_OS_WIN)
  return QStringLiteral("windows");
#elif defined(Q_OS_MACOS)
  return QStringLiteral("macos");
#elif defined(Q_OS_LINUX)
  return QStringLiteral("linux");
#else
  return QStringLiteral("other");
#endif
}

QString settingsPlatformName(const QString& platform) {
  if (platform == QLatin1String("windows")) {
    return QStringLiteral("Windows");
  }
  if (platform == QLatin1String("macos")) {
    return QStringLiteral("macOS");
  }
  if (platform == QLatin1String("linux")) {
    return QStringLiteral("Linux");
  }
  return platform;
}

QString settingsFileProblemText(const SettingsFileContents& contents) {
  switch (contents.problem) {
  case SettingsFileContents::Problem::None:
    return {};
  case SettingsFileContents::Problem::Unreadable:
    return QCoreApplication::translate("Settings", "The file could not be read.");
  case SettingsFileContents::Problem::NotSettings:
    return QCoreApplication::translate("Settings", "The file does not hold MitCad settings.");
  case SettingsFileContents::Problem::Newer:
    return QCoreApplication::translate("Settings",
                                       "The file was written by a newer MitCad, in settings version %1. This MitCad "
                                       "reads settings up to version %2: update MitCad to import them.")
        .arg(contents.version)
        .arg(kSettingsFileVersion);
  }
  return {};
}

bool writeSettingsFile(const QSettings& from, const QStringList& groups, const QString& path, QString* error,
                       int* count) {
  // Written by Qt as INI text in a folder of its own, then with LF line
  // ends (Qt writes CRLF on Windows) in place of the file.
  QTemporaryDir folder;
  if (!folder.isValid()) {
    if (error != nullptr) {
      *error = QCoreApplication::translate("Settings", "No temporary folder could be made.");
    }
    return false;
  }
  const QString staged = folder.filePath(QStringLiteral("staged.ini"));
  int written = 0;
  QStringList kept;
  for (const SettingsGroup& group : settingsGroups()) {
    if (groups.contains(QLatin1String(group.id))) {
      kept << QLatin1String(group.id);
    }
  }
  {
    QSettings file(staged, QSettings::IniFormat);
    file.setValue(kHeader + QStringLiteral("version"), kSettingsFileVersion);
    file.setValue(kHeader + QStringLiteral("application"), QCoreApplication::applicationVersion());
    file.setValue(kHeader + QStringLiteral("platform"), settingsPlatform());
    file.setValue(kHeader + QStringLiteral("written"),
                  QDateTime::currentDateTimeUtc().toString(Qt::ISODate));
    file.setValue(kHeader + QStringLiteral("groups"), kept);
    for (const QString& key : from.allKeys()) {
      if (kept.contains(settingsGroupOf(key))) {
        file.setValue(key, from.value(key));
        ++written;
      }
    }
    file.sync();
    if (file.status() != QSettings::NoError) {
      if (error != nullptr) {
        *error = QCoreApplication::translate("Settings", "The settings could not be written.");
      }
      return false;
    }
  }
  QFile in(staged);
  if (!in.open(QIODevice::ReadOnly)) {
    if (error != nullptr) {
      *error = QCoreApplication::translate("Settings", "The settings could not be written.");
    }
    return false;
  }
  QByteArray text = in.readAll();
  in.close();
  text.replace("\r\n", "\n");
  QSaveFile out(path);
  if (!out.open(QIODevice::WriteOnly) || out.write(text) != text.size() || !out.commit()) {
    if (error != nullptr) {
      *error = QCoreApplication::translate("Settings", "The file could not be written: %1").arg(out.errorString());
    }
    return false;
  }
  if (count != nullptr) {
    *count = written;
  }
  return true;
}

SettingsFileContents readSettingsFile(const QString& path) {
  SettingsFileContents contents;
  const QFileInfo info(path);
  if (!info.isFile() || !info.isReadable()) {
    contents.problem = SettingsFileContents::Problem::Unreadable;
    return contents;
  }
  const QSettings file(path, QSettings::IniFormat);
  if (file.status() != QSettings::NoError) {
    contents.problem = file.status() == QSettings::AccessError ? SettingsFileContents::Problem::Unreadable
                                                               : SettingsFileContents::Problem::NotSettings;
    return contents;
  }
  bool ok = false;
  contents.version = file.value(kHeader + QStringLiteral("version")).toInt(&ok);
  if (!ok || contents.version < 1) {
    contents.problem = SettingsFileContents::Problem::NotSettings;
    return contents;
  }
  if (contents.version > kSettingsFileVersion) {
    contents.problem = SettingsFileContents::Problem::Newer;
    return contents;
  }
  contents.application = file.value(kHeader + QStringLiteral("application")).toString();
  contents.platform = file.value(kHeader + QStringLiteral("platform")).toString();
  contents.written = file.value(kHeader + QStringLiteral("written")).toString();
  const QStringList listed = listOf(file.value(kHeader + QStringLiteral("groups")));
  for (const SettingsGroup& group : settingsGroups()) {
    if (listed.contains(QLatin1String(group.id))) {
      contents.groups << QLatin1String(group.id);
    }
  }
  for (const QString& key : file.allKeys()) {
    if (key.startsWith(kHeader)) {
      continue;
    }
    if (contents.groups.contains(settingsGroupOf(key))) {
      contents.values.insert(key, file.value(key));
    }
  }
  return contents;
}

SettingsImportPlan planSettingsImport(const SettingsFileContents& contents, const QStringList& groups,
                                      const QSettings& current, const PathExists& exists) {
  const PathExists found =
      exists ? exists : PathExists([](const QString& path) { return QFileInfo::exists(path); });
  SettingsImportPlan plan;
  for (const QString& group : contents.groups) {
    if (groups.contains(group)) {
      plan.groups << group;
    }
  }
  // Keys this machine keeps as they are (a missing program and its
  // companions, a list of folders none of which is here).
  QStringList keep;
  for (auto it = contents.values.cbegin(); it != contents.values.cend(); ++it) {
    const QString& key = it.key();
    const QString group = settingsGroupOf(key);
    if (!plan.groups.contains(group)) {
      continue;
    }
    switch (kindOf(key)) {
    case PathKind::None:
      if (group == QLatin1String("shortcuts") && it.value().typeId() == QMetaType::QStringList) {
        // A key sequence written by hand without its quotes ("E, X, C",
        // mitcad#2095) reads as a list of its combinations: kept as the
        // text the Keyboard Shortcuts window saves.
        plan.write.insert(key, shortcutTextFromList(it.value().toStringList()));
      } else {
        plan.write.insert(key, it.value());
      }
      break;
    case PathKind::Folders: {
      const QStringList folders = listOf(it.value());
      QStringList present;
      for (const QString& folder : folders) {
        if (found(folder)) {
          present << folder;
        } else {
          plan.skipped.append({group, key, folder});
        }
      }
      if (folders.isEmpty() || !present.isEmpty()) {
        plan.write.insert(key, present);
      } else {
        keep << key;
      }
      break;
    }
    case PathKind::Program: {
      const QString program = it.value().toString().trimmed();
      if (program.isEmpty() || !namesPlace(program) || found(program)) {
        plan.write.insert(key, it.value());
      } else {
        plan.skipped.append({group, key, program});
        keep << key;
        if (key == QLatin1String("print/slicerProgram")) {
          for (const char* companion : kSlicerCompanions) {
            keep << QLatin1String(companion);
          }
        }
      }
      break;
    }
    case PathKind::LibrarySources: {
      // As LibrarySettings keeps it: a JSON array of {url, enabled,
      // autoUpdate} as bytes.
      const QJsonArray sources = QJsonDocument::fromJson(it.value().toByteArray()).array();
      QJsonArray present;
      for (const QJsonValue& source : sources) {
        const QString url = source.toObject().value(QStringLiteral("url")).toString().trimmed();
        if (isFolderSource(url) && !found(url)) {
          plan.skipped.append({group, key, url});
        } else {
          present.append(source);
        }
      }
      if (present.size() == sources.size()) {
        plan.write.insert(key, it.value()); // as it is (also a value that is not JSON)
      } else {
        plan.write.insert(key, QJsonDocument(present).toJson(QJsonDocument::Compact));
      }
      break;
    }
    }
  }
  for (const QString& key : std::as_const(keep)) {
    plan.write.remove(key);
  }
  // The replaced groups' other settings here go back to their defaults.
  for (const QString& key : current.allKeys()) {
    if (plan.groups.contains(settingsGroupOf(key)) && !plan.write.contains(key) && !keep.contains(key)) {
      plan.remove << key;
    }
  }
  return plan;
}

void applySettingsImport(QSettings& to, const SettingsImportPlan& plan) {
  for (const QString& key : plan.remove) {
    to.remove(key);
  }
  for (auto it = plan.write.cbegin(); it != plan.write.cend(); ++it) {
    to.setValue(it.key(), it.value());
  }
  to.sync();
}

} // namespace mitcad
