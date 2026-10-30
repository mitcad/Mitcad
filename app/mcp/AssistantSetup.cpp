// SPDX-License-Identifier: MIT
#include "AssistantSetup.hpp"

#include <QCoreApplication>
#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QRegularExpression>
#include <QSaveFile>
#include <QSettings>
#include <QStandardPaths>
#include <QUuid>

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

QStringList assistantRegistration(AssistantClient client, const QString& bridge) {
  if (client == AssistantClient::ClaudeCode) {
    return {QStringLiteral("mcp"), QStringLiteral("add"), QStringLiteral("--scope"), QStringLiteral("user"),
            QStringLiteral("mitcad"), QStringLiteral("--"), bridge, QStringLiteral("mcp"), QStringLiteral("--connect")};
  }
  return {QStringLiteral("mcp"), QStringLiteral("add"), QStringLiteral("mitcad-desktop"), QStringLiteral("--"),
          bridge, QStringLiteral("mcp"), QStringLiteral("--connect")};
}

QJsonObject assistantJsonConfiguration(const QString& bridge) {
  const QJsonObject server{{QStringLiteral("command"), bridge},
                           {QStringLiteral("args"), QJsonArray{QStringLiteral("mcp"), QStringLiteral("--connect")}}};
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
  const bool code = name.contains(QStringLiteral("claude-code")) || name.contains(QStringLiteral("claude code"));
  const bool claude = name.contains(QStringLiteral("claude"));
  const bool codex = name.contains(QStringLiteral("codex")) || name.contains(QStringLiteral("chatgpt")) ||
                     name.contains(QStringLiteral("openai"));
  switch (client) {
  case AssistantClient::ClaudeDesktop: return claude && !code;
  case AssistantClient::ClaudeCode: return code;
  case AssistantClient::Codex: return codex && !claude;
  case AssistantClient::Other: return !claude && !codex;
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

QString assistantDisplayName(const QString& label) {
  for (const AssistantClient client : {AssistantClient::ClaudeCode, AssistantClient::ClaudeDesktop, AssistantClient::Codex}) {
    if (assistantLabelMatches(client, label)) return assistantSentenceName(client);
  }
  return label;
}

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

} // namespace mitcad::mcp
