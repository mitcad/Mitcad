// SPDX-License-Identifier: MIT
#include "AssistantSetup.hpp"

#include <algorithm>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QObject>
#include <QPointer>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSettings>
#include <QStandardPaths>
#include <QUuid>
#include <QVersionNumber>

namespace mitcad::mcp {
namespace {
QString executable(const QString& path) {
  const QFileInfo file(path);
  return file.isFile() && file.isExecutable() ? file.canonicalFilePath() : QString();
}
QString quoted(const QString& value, bool windows) {
  QString text = value;
  text.replace(QLatin1Char('\''), windows ? QStringLiteral("''") : QStringLiteral("'\\''"));
  return QLatin1Char('\'') + text + QLatin1Char('\'');
}
}

QString installedBridge(const QString& applicationDirectory) {
#ifdef Q_OS_WIN
  const QString name = QStringLiteral("mitcad-cli.exe");
#else
  const QString name = QStringLiteral("mitcad-cli");
#endif
  const QDir folder(applicationDirectory);
  // Installed applications always use the sibling. The remaining candidates
  // support a development build without making a checkout part of setup.
  for (const QString& candidate : {folder.filePath(name), folder.filePath(QStringLiteral("../tools/cli/") + name),
                                   folder.filePath(QStringLiteral("../../../../tools/cli/") + name)}) {
    if (const QString found = executable(candidate); !found.isEmpty()) return found;
  }
  return {};
}

QString executableDirectory() {
#ifdef Q_OS_LINUX
  // An AppImage's file is the application for Qt; the executables are in
  // its mount folder (mitcad#1170, as RenderBatch's workerExecutable).
  const QString self = QFileInfo(QStringLiteral("/proc/self/exe")).symLinkTarget();
  if (!self.isEmpty()) return QFileInfo(self).absolutePath();
#endif
  return QCoreApplication::applicationDirPath();
}

QString bundledBridge() { return installedBridge(executableDirectory()); }

BridgeLaunch registeredBridge(const QString& bridge, const QString& appImage) {
  const QFileInfo image(appImage);
  if (!appImage.isEmpty() && image.isAbsolute() && image.isFile()) {
    return {image.absoluteFilePath(), {QStringLiteral("cli")}};
  }
  return {bridge, {}};
}

BridgeLaunch registeredBridge() {
#ifdef Q_OS_LINUX
  const QString appImage = qEnvironmentVariable("APPIMAGE");
#else
  const QString appImage;
#endif
  return registeredBridge(bundledBridge(), appImage);
}

QStringList bridgeConnectArguments(const QStringList& leading) {
  return leading + QStringList{QStringLiteral("mcp"), QStringLiteral("--connect")};
}

QStringList assistantExecutableSuffixes(bool windows, const QString& pathext) {
  if (!windows) return {QString()};
  const QStringList known{QStringLiteral(".exe"), QStringLiteral(".cmd"), QStringLiteral(".bat")};
  QStringList suffixes;
  for (const QString& entry : pathext.split(QLatin1Char(';'), Qt::SkipEmptyParts)) {
    const QString suffix = entry.trimmed().toLower();
    if (known.contains(suffix) && !suffixes.contains(suffix)) suffixes << suffix;
  }
  // Windows' own order when PATHEXT names none of them.
  return suffixes.isEmpty() ? QStringList{QStringLiteral(".exe"), QStringLiteral(".bat"), QStringLiteral(".cmd")} : suffixes;
}

QString expandWindowsVariables(const QString& text) {
  static const QRegularExpression variable(QStringLiteral("%([^%;]+)%"));
  QString expanded;
  qsizetype last = 0;
  for (auto matches = variable.globalMatch(text); matches.hasNext();) {
    const QRegularExpressionMatch match = matches.next();
    const QString value = qEnvironmentVariable(match.captured(1).toLocal8Bit().constData());
    expanded += text.mid(last, match.capturedStart() - last);
    expanded += value.isEmpty() ? match.captured() : value;
    last = match.capturedEnd();
  }
  return expanded + text.mid(last);
}

QStringList windowsPathEntries(const QString& value) {
  QStringList entries;
  for (const QString& entry : value.split(QLatin1Char(';'), Qt::SkipEmptyParts)) {
    QString folder = entry.trimmed();
    if (folder.size() >= 2 && folder.startsWith(QLatin1Char('"')) && folder.endsWith(QLatin1Char('"'))) {
      folder = folder.mid(1, folder.size() - 2);
    }
    // Windows' separators on every platform (a test reads it anywhere).
    folder = expandWindowsVariables(folder).replace(QLatin1Char('\\'), QLatin1Char('/'));
    if (!folder.isEmpty()) entries << folder;
  }
  return entries;
}

QStringList storedUserPath() {
#ifdef Q_OS_WIN
  const QSettings environment(QStringLiteral("HKEY_CURRENT_USER\\Environment"), QSettings::NativeFormat);
  return windowsPathEntries(environment.value(QStringLiteral("Path")).toString());
#else
  return {};
#endif
}

QStringList assistantFallbackFolders(const QString& home, const QString& roamingAppData, const QStringList& storedPath) {
  const QDir root(home);
  QStringList folders{root.filePath(QStringLiteral(".local/bin"))};
  if (!roamingAppData.isEmpty()) folders << QDir(roamingAppData).filePath(QStringLiteral("npm"));
  folders << root.filePath(QStringLiteral(".cargo/bin"));
  for (const QString& folder : storedPath) {
    if (!folders.contains(folder)) folders << folder;
  }
  return folders;
}

QString findAssistantExecutable(const QString& name, const QStringList& folders, const QStringList& suffixes) {
  for (const QString& folder : folders) {
    for (const QString& suffix : suffixes) {
      if (const QString found = executable(QDir(folder).filePath(name + suffix)); !found.isEmpty()) return found;
    }
  }
  return {};
}

namespace {
constexpr bool kWindows =
#ifdef Q_OS_WIN
    true;
#else
    false;
#endif

// The lookup behind assistantExecutable; `onPath` says whether PATH found it.
QString lookupAssistant(const QString& name, const QString& home, bool& onPath) {
  onPath = false;
  const QStringList suffixes = assistantExecutableSuffixes(kWindows, qEnvironmentVariable("PATHEXT"));
  // The roaming application data where Windows keeps it under a home.
  const auto roaming = [](const QString& base) {
    return kWindows ? QDir(base).filePath(QStringLiteral("AppData/Roaming")) : QString();
  };
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    const QString folder = qEnvironmentVariable("MITCAD_TEST_ASSISTANT_EXECUTABLES");
    const QString testHome = qEnvironmentVariable("MITCAD_TEST_ASSISTANT_HOME");
    if (!folder.isEmpty() || !testHome.isEmpty()) {
      if (folder == QStringLiteral("none")) return {};
      if (!folder.isEmpty()) {
        if (const QString found = findAssistantExecutable(name, {folder}, suffixes); !found.isEmpty()) return found;
      }
      return testHome.isEmpty() ? QString()
                                : findAssistantExecutable(name, assistantFallbackFolders(testHome, roaming(testHome), {}), suffixes);
    }
  }
  if (!home.isEmpty()) return findAssistantExecutable(name, assistantFallbackFolders(home, roaming(home), {}), suffixes);
  if (QString found = QStandardPaths::findExecutable(name); !found.isEmpty()) {
    onPath = true;
    return found;
  }
  const QString roamingAppData = kWindows ? qEnvironmentVariable("APPDATA") : QString();
  if (const QString found = findAssistantExecutable(
          name, assistantFallbackFolders(QDir::homePath(), roamingAppData, storedUserPath()), suffixes);
      !found.isEmpty()) {
    return found;
  }
#ifdef Q_OS_MACOS
  const QString root = QDir::homePath();
  if (name == QStringLiteral("codex")) {
    for (const QString& applications : {QStringLiteral("/Applications"), QDir(root).filePath(QStringLiteral("Applications"))}) {
      for (const QString& binary : {QStringLiteral("ChatGPT.app/Contents/Resources/codex"),
                                    QStringLiteral("ChatGPT.app/Contents/Resources/CodexCLI"),
                                    QStringLiteral("ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex"),
                                    QStringLiteral("ChatGPT.app/Contents/Resources/codex-cli/bin/codex"),
                                    QStringLiteral("Codex.app/Contents/Resources/codex")}) {
        if (const QString found = executable(QDir(applications).filePath(binary)); !found.isEmpty()) return found;
      }
    }
  }
#endif
  return {};
}
} // namespace

QString assistantExecutable(const QString& name, const QString& home) {
  bool onPath = false;
  return lookupAssistant(name, home, onPath);
}

QString assistantProgram(const QString& name) {
  bool onPath = false;
  const QString found = lookupAssistant(name, {}, onPath);
  return found.isEmpty() || onPath ? name : found;
}

QStringList assistantExecutables(const QString& name) {
  QStringList found;
  // One entry per file: PATH's result and a folder's can name it differently.
  const auto add = [&found](const QString& path) {
    const QString file = path.isEmpty() ? QString() : QFileInfo(path).canonicalFilePath();
    if (!file.isEmpty() && !found.contains(file)) found << file;
  };
  const QStringList suffixes = assistantExecutableSuffixes(kWindows, qEnvironmentVariable("PATHEXT"));
  const auto roaming = [](const QString& base) {
    return kWindows ? QDir(base).filePath(QStringLiteral("AppData/Roaming")) : QString();
  };
  const auto eachFolder = [&](const QStringList& folders) {
    for (const QString& folder : folders) add(findAssistantExecutable(name, {folder}, suffixes));
  };
  const bool claude = name == QStringLiteral("claude");
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    const QString folder = qEnvironmentVariable("MITCAD_TEST_ASSISTANT_EXECUTABLES");
    const QString testHome = qEnvironmentVariable("MITCAD_TEST_ASSISTANT_HOME");
    if (!folder.isEmpty() || !testHome.isEmpty()) {
      if (folder == QStringLiteral("none")) return {};
      if (!folder.isEmpty()) eachFolder({folder});
      if (!testHome.isEmpty()) eachFolder(assistantFallbackFolders(testHome, roaming(testHome), {}));
      const QString work = qEnvironmentVariable("MITCAD_ASSISTANT_TEST_WORK");
      if (claude && !work.isEmpty()) {
        const QString local = QDir(work).filePath(QStringLiteral("local-app-data"));
        add(claudeDesktopClaudeCode(
            claudeDesktopWindowsSettingsFolders(local, QDir(work).filePath(QStringLiteral("roaming-app-data"))) +
            claudeDesktopDataFolders(local)));
      }
      return found;
    }
  }
  add(QStandardPaths::findExecutable(name));
  eachFolder(assistantFallbackFolders(QDir::homePath(), kWindows ? qEnvironmentVariable("APPDATA") : QString(),
                                      storedUserPath()));
  add(assistantExecutable(name)); // the applications' own (macOS)
  if (claude) {
    add(claudeDesktopClaudeCode(claudeDesktopSettingsFolders() +
                                claudeDesktopDataFolders(kWindows ? qEnvironmentVariable("LOCALAPPDATA") : QString())));
  }
  return found;
}

QString claudeDesktopClaudeCode(const QStringList& settingsFolders) {
  const QString program = kWindows ? QStringLiteral("claude.exe") : QStringLiteral("claude");
  QString best;
  QVersionNumber highest;
  for (const QString& settings : settingsFolders) {
    const QDir versions(QDir(settings).filePath(QStringLiteral("claude-code")));
    if (!versions.exists()) continue;
    for (const QString& entry : versions.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
      const QVersionNumber version = QVersionNumber::fromString(entry);
      if (version.isNull()) continue;
      // The program in the version's folder, or in a build folder under it
      // (`<version>/<build id>/claude.exe` on Windows); the newest such.
      const QDir folder(versions.filePath(entry));
      QString found = executable(folder.filePath(program));
      if (found.isEmpty()) {
        QDateTime newest;
        for (const QString& build : folder.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) {
          const QString candidate = executable(QDir(folder.filePath(build)).filePath(program));
          const QDateTime written = QFileInfo(candidate).lastModified();
          if (!candidate.isEmpty() && (found.isEmpty() || written > newest)) {
            found = candidate;
            newest = written;
          }
        }
      }
      if (!found.isEmpty() && (best.isEmpty() || version > highest)) {
        best = found;
        highest = version;
      }
    }
  }
  return best;
}

bool claudeDesktopKeepsClaudeCode(const QString& program) {
  const QStringList parts = QDir::fromNativeSeparators(program).split(QLatin1Char('/'), Qt::SkipEmptyParts);
  const qsizetype count = parts.size();
  if (count < 3) return false;
  const QString file = parts.last();
  if (file.compare(QStringLiteral("claude"), Qt::CaseInsensitive) != 0 &&
      file.compare(QStringLiteral("claude.exe"), Qt::CaseInsensitive) != 0) {
    return false;
  }
  static const QRegularExpression version(QStringLiteral("^[0-9]+(\\.[0-9]+)*$"));
  // `claude-code/<version>/claude`, or a build folder between.
  const auto keptAt = [&parts](qsizetype versionAt) {
    return versionAt >= 1 && parts.at(versionAt - 1).compare(QStringLiteral("claude-code"), Qt::CaseInsensitive) == 0 &&
        version.match(parts.at(versionAt)).hasMatch();
  };
  return keptAt(count - 2) || (count >= 4 && keptAt(count - 3));
}

QString claudeConfigurationFolder(const QString& configured, const QString& home) {
  if (!configured.trimmed().isEmpty()) return QDir::cleanPath(QDir(configured.trimmed()).absolutePath());
  return QDir::cleanPath(QDir(home).filePath(QStringLiteral(".claude")));
}

QStringList claudeDesktopDataFolders(const QString& localAppData) {
  QStringList folders;
  if (localAppData.isEmpty()) return folders;
  const QDir data(QDir(localAppData).filePath(QStringLiteral("Claude-Data")));
  for (const QString& entry : data.entryList(QDir::Dirs | QDir::NoDotAndDotDot)) folders << data.filePath(entry);
  return folders;
}

QStringList claudeDesktopSettingsFolders() {
#if defined(Q_OS_WIN)
  return claudeDesktopWindowsSettingsFolders(qEnvironmentVariable("LOCALAPPDATA"), qEnvironmentVariable("APPDATA"));
#elif defined(Q_OS_MACOS)
  return {QDir::home().filePath(QStringLiteral("Library/Application Support/Claude"))};
#else
  return {};
#endif
}

QString assistantPlace(const QString& executable, const QString& home) {
  const QString folder = QFileInfo(executable).path();
  const QString root = QDir::cleanPath(QDir::fromNativeSeparators(home));
  const QString clean = QDir::cleanPath(QDir::fromNativeSeparators(folder));
  const Qt::CaseSensitivity sensitivity = kWindows ? Qt::CaseInsensitive : Qt::CaseSensitive;
  if (!root.isEmpty() && clean.startsWith(root + QLatin1Char('/'), sensitivity)) {
    return QStringLiteral("~/") + clean.mid(root.size() + 1);
  }
  return QDir::toNativeSeparators(clean);
}

namespace {
// The settings of the assistant menu's rows (mitcad#1299).
const QString kBlockedKey = QStringLiteral("assistant/blocked");
const QString kRowsKey = QStringLiteral("assistant/rows");
const QString kClientRow = QStringLiteral("client:");
const QString kDesktopChat = QStringLiteral("claude-desktop-chat");
const QString kDesktopCode = QStringLiteral("claude-desktop-code");
const QString kClaudeCode = QStringLiteral("claude-code");
const QString kCodex = QStringLiteral("codex");

// The known assistant a label is, if any.
bool knownAssistant(const QString& label, AssistantClient& client) {
  for (const AssistantClient known : {AssistantClient::ClaudeCode, AssistantClient::ClaudeDesktop, AssistantClient::Codex}) {
    if (assistantLabelMatches(known, label)) {
      client = known;
      return true;
    }
  }
  return false;
}

bool desktopCodeTab(const QString& label) {
  return label.trimmed().startsWith(QStringLiteral("local-agent-mode"), Qt::CaseInsensitive);
}

struct BlockWatcher {
  QPointer<QObject> context;
  std::function<void()> changed;
};

std::vector<BlockWatcher>& blockWatchers() {
  static std::vector<BlockWatcher> watchers;
  return watchers;
}

void saveBlocked(const QStringList& rows) {
  QSettings settings;
  if (rows.isEmpty()) {
    settings.remove(kBlockedKey);
  } else {
    settings.setValue(kBlockedKey, rows);
  }
  // A copy: a watcher may add another one.
  std::vector<BlockWatcher>& watchers = blockWatchers();
  watchers.erase(std::remove_if(watchers.begin(), watchers.end(),
                                [](const BlockWatcher& watcher) { return watcher.context.isNull(); }),
                 watchers.end());
  const std::vector<BlockWatcher> calls = watchers;
  for (const BlockWatcher& watcher : calls) {
    if (!watcher.context.isNull() && watcher.changed) watcher.changed();
  }
}
} // namespace

AssistantClient assistantClientOf(const QString& label) {
  AssistantClient client = AssistantClient::Other;
  knownAssistant(label, client);
  return client;
}

QString assistantRowKey(const QString& label) {
  if (desktopCodeTab(label)) return kDesktopCode;
  switch (assistantClientOf(label)) {
  case AssistantClient::ClaudeDesktop: return kDesktopChat;
  case AssistantClient::ClaudeCode: return kClaudeCode;
  case AssistantClient::Codex: return kCodex;
  case AssistantClient::Other: break;
  }
  return kClientRow + label;
}

QString assistantRowName(const QString& key) {
  if (key == kDesktopChat) return QObject::tr("Claude Desktop (chat)");
  if (key == kDesktopCode) return QObject::tr("Claude Desktop (Code tab)");
  if (key == kClaudeCode) return assistantSentenceName(AssistantClient::ClaudeCode);
  if (key == kCodex) return assistantSentenceName(AssistantClient::Codex);
  return key.startsWith(kClientRow) ? key.mid(kClientRow.size()) : key;
}

QStringList knownAssistantRows() { return {kDesktopChat, kDesktopCode, kClaudeCode, kCodex}; }

QStringList assistantRows(AssistantClient client) {
  switch (client) {
  case AssistantClient::ClaudeDesktop: return {kDesktopChat, kDesktopCode};
  case AssistantClient::ClaudeCode: return {kClaudeCode};
  case AssistantClient::Codex: return {kCodex};
  case AssistantClient::Other: break;
  }
  return {};
}

AssistantClient assistantRowClient(const QString& key) {
  if (key == kDesktopChat || key == kDesktopCode) return AssistantClient::ClaudeDesktop;
  if (key == kClaudeCode) return AssistantClient::ClaudeCode;
  if (key == kCodex) return AssistantClient::Codex;
  return AssistantClient::Other;
}

QStringList blockedAssistantRows() { return QSettings().value(kBlockedKey).toStringList(); }

bool blockAssistantRow(const QString& key) {
  QStringList rows = blockedAssistantRows();
  if (key.isEmpty() || key == kClientRow || rows.contains(key)) return false;
  rows << key;
  saveBlocked(rows);
  return true;
}

bool allowAssistantRow(const QString& key) {
  QStringList rows = blockedAssistantRows();
  if (rows.removeAll(key) == 0) return false;
  saveBlocked(rows);
  return true;
}

void blockAssistants(const QStringList& labels) {
  QStringList rows = blockedAssistantRows();
  const qsizetype before = rows.size();
  for (const QString& label : labels) {
    const QString key = assistantRowKey(label);
    if (!label.isEmpty() && !rows.contains(key)) rows << key;
  }
  if (rows.size() != before) saveBlocked(rows);
}

bool allowAssistant(AssistantClient client) {
  QStringList rows = blockedAssistantRows();
  const qsizetype before = rows.size();
  rows.erase(std::remove_if(rows.begin(), rows.end(),
                            [client](const QString& key) { return assistantRowClient(key) == client; }),
             rows.end());
  if (rows.size() == before) return false;
  saveBlocked(rows);
  return true;
}

bool assistantRowBlocked(const QStringList& blocked, const QString& label) {
  return blocked.contains(assistantRowKey(label));
}

bool assistantBlocked(const QString& label) { return assistantRowBlocked(blockedAssistantRows(), label); }

bool assistantClientBlocked(AssistantClient client) {
  const QStringList rows = blockedAssistantRows();
  return std::any_of(rows.cbegin(), rows.cend(),
                     [client](const QString& key) { return assistantRowClient(key) == client; });
}

QString assistantRefusal(const QString& label) {
  if (!assistantBlocked(label)) return {};
  return QObject::tr("Blocked in MitCad: check %1 under Incoming connections in MitCad's assistant menu "
                     "to connect it again")
      .arg(assistantRowName(assistantRowKey(label)));
}

void watchAssistantBlocks(QObject* context, std::function<void()> changed) {
  if (context != nullptr && changed) blockWatchers().push_back({context, std::move(changed)});
}

QStringList rememberedAssistantRows() { return QSettings().value(kRowsKey).toStringList(); }

void rememberAssistantRows(const QStringList& keys) {
  QStringList rows = rememberedAssistantRows();
  const qsizetype before = rows.size();
  for (const QString& key : keys) {
    // Known assistants only: another client's row is this run's.
    if (knownAssistantRows().contains(key) && !rows.contains(key)) rows << key;
  }
  if (rows.size() != before) QSettings().setValue(kRowsKey, rows);
}

QString codexConfigurationPath() {
  const QString home = qEnvironmentVariable("CODEX_HOME");
  return QDir(home.isEmpty() ? QDir::home().filePath(QStringLiteral(".codex")) : home)
      .absoluteFilePath(QStringLiteral("config.toml"));
}

QString assistantShellWord(const QString& value, bool windows, bool always) {
  static const QRegularExpression posix(QStringLiteral("^[A-Za-z0-9_@%+=:,./-]+$"));
  static const QRegularExpression powershell(QStringLiteral("^[A-Za-z0-9_.:/\\\\-]+$"));
  // PowerShell takes a bare `--` as the end of a script's own parameters
  // (an npm-installed client is a script there).
  const bool plain = !always && (windows ? value != QStringLiteral("--") && powershell.match(value).hasMatch()
                                         : posix.match(value).hasMatch());
  return plain ? value : quoted(value, windows);
}

QString assistantCommand(const QString& program, const QStringList& arguments, bool windows) {
  const QString first = assistantShellWord(QDir::toNativeSeparators(program), windows);
  QStringList words{first};
  for (const QString& argument : arguments) words << assistantShellWord(argument, windows);
  const bool callOperator = windows && first.startsWith(QLatin1Char('\''));
  return (callOperator ? QStringLiteral("& ") : QString()) + words.join(QLatin1Char(' '));
}

QStringList assistantRegistration(AssistantClient client, const QString& bridge, const QStringList& leading) {
  const QStringList start = client == AssistantClient::ClaudeCode
      ? QStringList{QStringLiteral("mcp"), QStringLiteral("add"), QStringLiteral("--scope"), QStringLiteral("user"),
                    QStringLiteral("mitcad"), QStringLiteral("--"), bridge}
      : QStringList{QStringLiteral("mcp"), QStringLiteral("add"), QStringLiteral("mitcad-desktop"), QStringLiteral("--"),
                    bridge};
  return start + bridgeConnectArguments(leading);
}

QJsonObject assistantJsonConfiguration(const QString& bridge, const QStringList& leading) {
  const QJsonObject server{{QStringLiteral("command"), bridge},
                           {QStringLiteral("args"), QJsonArray::fromStringList(bridgeConnectArguments(leading))}};
  const QJsonObject servers{{QStringLiteral("mitcad"), server}};
  return {{QStringLiteral("mcpServers"), servers}};
}

QJsonObject assistantBundleManifest(const QString& bridge, bool windows, const QString& platform) {
  const QString entry = QStringLiteral("server/mitcad-mcp-launcher") + (windows ? QStringLiteral(".exe") : QString());
  return {{QStringLiteral("manifest_version"), QStringLiteral("0.3")},
          {QStringLiteral("name"), QStringLiteral("mitcad-desktop")},
          {QStringLiteral("display_name"), QStringLiteral("MitCad")},
          {QStringLiteral("version"), QStringLiteral("1.0.0")},
          {QStringLiteral("description"), QStringLiteral("Inspect and edit the open MitCad design through local MCP")},
          {QStringLiteral("author"), QJsonObject{{QStringLiteral("name"), QStringLiteral("MitCad developers")}}},
          {QStringLiteral("server"), QJsonObject{{QStringLiteral("type"), QStringLiteral("binary")},
              {QStringLiteral("entry_point"), entry},
              {QStringLiteral("mcp_config"), QJsonObject{
                  {QStringLiteral("command"), QStringLiteral("${__dirname}/") + entry},
                  {QStringLiteral("args"), QJsonArray{bridge, QStringLiteral("mcp"), QStringLiteral("--connect")}}}}}},
          {QStringLiteral("compatibility"), QJsonObject{{QStringLiteral("platforms"), QJsonArray{platform}}}}};
}

bool readAssistantConfiguration(const QString& path, QString& content, QString& error, qint64 limit) {
  content.clear(); error.clear();
  const QFileInfo info(path);
  if (!info.isAbsolute() || info.isSymLink() || (info.exists() && !info.isFile())) {
    error = QObject::tr("Choose a regular configuration file with an absolute path."); return false;
  }
  if (!info.exists()) return true;
  if (info.size() > limit) {
    error = QObject::tr("The configuration is larger than %1 MB.").arg(limit / (1024 * 1024)); return false;
  }
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    error = QObject::tr("The configuration could not be read."); return false;
  }
  const QByteArray bytes = file.readAll();
  if (bytes.size() > limit) {
    error = QObject::tr("The configuration is larger than %1 MB.").arg(limit / (1024 * 1024)); return false;
  }
  content = QString::fromUtf8(bytes);
  if (content.toUtf8() != bytes) {
    error = QObject::tr("The configuration must be valid UTF-8."); return false;
  }
  return true;
}

bool writeAssistantConfiguration(const QString& path, const QString& expected, const QString& content,
                                 QString& backup, QString& error, qint64 limit) {
  backup.clear();
  QString current;
  if (!readAssistantConfiguration(path, current, error, limit)) return false;
  if (current != expected) {
    error = QObject::tr("The configuration changed while setup was open. Refresh and try again."); return false;
  }
  if (current == content) return true;
  const QFileInfo info(path);
  if (!QDir().mkpath(info.absolutePath())) {
    error = QObject::tr("The configuration folder could not be created."); return false;
  }
  if (info.exists()) {
    backup = path + QStringLiteral(".mitcad-backup-") + QUuid::createUuid().toString(QUuid::Id128);
    if (!QFile::copy(path, backup)) {
      error = QObject::tr("The configuration backup could not be created."); return false;
    }
    QFile::setPermissions(backup, QFileDevice::ReadOwner | QFileDevice::WriteOwner);
  }
  QSaveFile file(path);
  file.setDirectWriteFallback(false);
  if (!file.open(QIODevice::WriteOnly)) { error = file.errorString(); return false; }
  const QByteArray bytes = content.toUtf8();
  file.setPermissions(info.exists() ? info.permissions() : QFileDevice::ReadOwner | QFileDevice::WriteOwner);
  if (file.write(bytes) != bytes.size()) { error = file.errorString(); return false; }
  QString latest;
  if (!readAssistantConfiguration(path, latest, error, limit) || latest != current) {
    file.cancelWriting();
    if (error.isEmpty()) error = QObject::tr("The configuration changed during setup. Refresh and try again.");
    return false;
  }
  if (!file.commit()) { error = file.errorString(); return false; }
  return true;
}

QString assistantConfigurationPolicy(const QString& path) {
  const QDir home(QFileInfo(path).absolutePath());
  QStringList policies{home.filePath(QStringLiteral("requirements.toml")), home.filePath(QStringLiteral("managed_config.toml")),
                       QStringLiteral("/etc/codex/requirements.toml")};
#ifdef Q_OS_MACOS
  policies << QStringLiteral("/Library/Managed Preferences/com.openai.codex.plist")
           << QStringLiteral("/Library/Managed Preferences/com.openai.chat.plist");
#endif
  for (const QString& policy : policies) {
    if (QFileInfo::exists(policy)) return QObject::tr("Managed settings block this change. Ask your administrator to allow MitCad.");
  }
  return {};
}

bool assistantLabelMatches(AssistantClient client, const QString& label) {
  const QString name = label.toLower();
  // Claude Desktop's Code tab (mitcad#1299), whatever its name goes on with.
  const bool tab = desktopCodeTab(name);
  const bool code = !tab && (name.contains(QStringLiteral("claude-code")) || name.contains(QStringLiteral("claude code")));
  const bool claude = name.contains(QStringLiteral("claude"));
  const bool codex = name.contains(QStringLiteral("codex")) || name.contains(QStringLiteral("chatgpt")) ||
                     name.contains(QStringLiteral("openai"));
  switch (client) {
  case AssistantClient::ClaudeDesktop: return tab || (claude && !code);
  case AssistantClient::ClaudeCode: return code;
  case AssistantClient::Codex: return codex && !claude && !tab;
  case AssistantClient::Other: return !claude && !codex && !tab;
  }
  return false;
}

QString assistantName(AssistantClient client) {
  switch (client) {
  case AssistantClient::ClaudeDesktop: return QObject::tr("Claude Desktop");
  case AssistantClient::ClaudeCode: return QObject::tr("Claude Code");
  case AssistantClient::Codex: return QObject::tr("ChatGPT / Codex");
  case AssistantClient::Other: return QObject::tr("Other");
  }
  return {};
}

QString assistantSentenceName(AssistantClient client) {
  return client == AssistantClient::Codex ? QObject::tr("Codex") : assistantName(client);
}

QString assistantDisplayName(const QString& label) { return assistantRowName(assistantRowKey(label)); }

bool claudeDesktopPackaged(const QString& localAppData) {
  if (localAppData.isEmpty()) return false;
  return QFileInfo(QDir(localAppData).filePath(QStringLiteral("Packages/") + QLatin1String(kClaudeDesktopPackageFamily))).isDir();
}

bool claudeDesktopApplicationInWindowsFolders(const QString& localAppData) {
  return !localAppData.isEmpty() &&
      (QFileInfo(QDir(localAppData).filePath(QStringLiteral("AnthropicClaude"))).isDir() || claudeDesktopPackaged(localAppData));
}

bool claudeDesktopInWindowsFolders(const QString& localAppData, const QString& roamingAppData) {
  return claudeDesktopApplicationInWindowsFolders(localAppData) ||
      (!roamingAppData.isEmpty() && QFileInfo(QDir(roamingAppData).filePath(QStringLiteral("Claude"))).isDir());
}

QStringList claudeDesktopWindowsSettingsFolders(const QString& localAppData, const QString& roamingAppData) {
  QStringList folders;
  if (!roamingAppData.isEmpty()) folders << QDir(roamingAppData).filePath(QStringLiteral("Claude"));
  if (!localAppData.isEmpty()) {
    folders << QDir(localAppData).filePath(QStringLiteral("Packages/") + QLatin1String(kClaudeDesktopPackageFamily) +
                                           QStringLiteral("/LocalCache/Roaming/Claude"));
  }
  return folders;
}

InstalledDesktopExtension claudeDesktopExtension(const QStringList& settingsFolders) {
  // The settings are read every second while Connect AI Assistant is open:
  // a bounded number of small manifests.
  constexpr int kMostExtensions = 256;
  constexpr qint64 kManifestLimit = 1024 * 1024;
  const QString name = assistantBundleManifest(QString(), false, QString()).value(QStringLiteral("name")).toString();
  for (const QString& settings : settingsFolders) {
    const QDir extensions(QDir(settings).filePath(QStringLiteral("Claude Extensions")));
    if (!extensions.exists()) continue;
    const QFileInfoList entries = extensions.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot, QDir::Name);
    for (qsizetype index = 0; index < entries.size() && index < kMostExtensions; ++index) {
      QFile file(QDir(entries.at(index).absoluteFilePath()).filePath(QStringLiteral("manifest.json")));
      if (!file.exists() || file.size() > kManifestLimit || !file.open(QIODevice::ReadOnly)) continue;
      const QJsonObject manifest = QJsonDocument::fromJson(file.read(kManifestLimit)).object();
      if (manifest.value(QStringLiteral("name")).toString() != name) continue;
      const QJsonArray arguments = manifest.value(QStringLiteral("server")).toObject().value(QStringLiteral("mcp_config"))
          .toObject().value(QStringLiteral("args")).toArray();
      return {true, arguments.isEmpty() ? QString() : arguments.first().toString()};
    }
  }
  return {};
}

bool claudeDesktopExtensionStarts(const InstalledDesktopExtension& extension, const QString& bridge) {
  if (!extension.found) return false;
  if (extension.bridge.isEmpty()) return true; // nothing to tell it by
#ifdef Q_OS_WIN
  const Qt::CaseSensitivity sensitivity = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity sensitivity = Qt::CaseSensitive;
#endif
  return QDir::cleanPath(QDir::fromNativeSeparators(extension.bridge))
             .compare(QDir::cleanPath(QDir::fromNativeSeparators(bridge)), sensitivity) == 0;
}

} // namespace mitcad::mcp
