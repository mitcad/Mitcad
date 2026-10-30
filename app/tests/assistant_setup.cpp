// SPDX-License-Identifier: MIT
#include "mcp/AssistantSetup.hpp"

#include <cstdio>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTemporaryDir>

namespace {
int failures = 0;
void check(bool value, const char* expression, int line) {
  if (!value) { std::fprintf(stderr, "assistant_setup.cpp:%d: %s\n", line, expression); ++failures; }
}
#define CHECK(value) check((value), #value, __LINE__)
bool put(const QString& path, const QByteArray& bytes) {
  QFile file(path);
  return file.open(QIODevice::WriteOnly) && file.write(bytes) == bytes.size();
}
}

int assistantSetupTests() {
  using namespace mitcad::mcp;
  const QString bridge = QStringLiteral("/Applications/Example's app/mitcad-cli");
  const auto codex = assistantRegistration(AssistantClient::Codex, bridge);
  CHECK(codex == QStringList({QStringLiteral("mcp"), QStringLiteral("add"), QStringLiteral("mitcad-desktop"),
        QStringLiteral("--"), bridge, QStringLiteral("mcp"), QStringLiteral("--connect")}));
  const auto claude = assistantRegistration(AssistantClient::ClaudeCode, bridge);
  CHECK(claude.mid(2, 2) == QStringList({QStringLiteral("--scope"), QStringLiteral("user")}));
  CHECK(claude.at(6) == bridge);
  CHECK(assistantCommand(QStringLiteral("codex"), codex, false).contains(QStringLiteral("Example'\\''s app")));
  // Only the words that need it are quoted; PowerShell's `--` always is, and
  // its call operator comes only before a quoted program.
  CHECK(assistantCommand(QStringLiteral("codex"), codex, true).startsWith(QStringLiteral("codex mcp add mitcad-desktop '--' '")));
  CHECK(assistantCommand(QStringLiteral("codex"), codex, true).contains(QStringLiteral("Example''s app")));
  CHECK(assistantCommand(QStringLiteral("codex"), codex, false).startsWith(QStringLiteral("codex mcp add mitcad-desktop -- '")));
  // A program without a folder, as the native separators differ by platform.
  CHECK(assistantCommand(QStringLiteral("mitcad-cli"), {QStringLiteral("mcp"), QStringLiteral("--connect")}, false) ==
        QStringLiteral("mitcad-cli mcp --connect"));
  CHECK(assistantCommand(QStringLiteral("/opt/My Apps/mitcad-cli"), {QStringLiteral("mcp")}, true).startsWith(QStringLiteral("& '")));
  CHECK(assistantShellWord(QStringLiteral("/home/user/.codex"), true, true) == QStringLiteral("'/home/user/.codex'"));
  // The connected clients' names for people (mitcad#706, mitcad#710).
  CHECK(assistantDisplayName(QStringLiteral("claude-ai")) == QStringLiteral("Claude Desktop"));
  CHECK(assistantDisplayName(QStringLiteral("claude-code")) == QStringLiteral("Claude Code"));
  // "ChatGPT / Codex" names its choice only; a sentence says "Codex" (mitcad#724).
  CHECK(assistantName(AssistantClient::Codex) == QStringLiteral("ChatGPT / Codex"));
  CHECK(assistantSentenceName(AssistantClient::Codex) == QStringLiteral("Codex"));
  CHECK(assistantSentenceName(AssistantClient::ClaudeCode) == QStringLiteral("Claude Code"));
  CHECK(assistantDisplayName(QStringLiteral("codex-mcp-client")) == QStringLiteral("Codex"));
  CHECK(assistantDisplayName(QStringLiteral("example-client")) == QStringLiteral("example-client"));
  const auto json = assistantJsonConfiguration(bridge).value(QStringLiteral("mcpServers")).toObject()
      .value(QStringLiteral("mitcad")).toObject();
  // Connected shows for the assistant whose label a client gave (mitcad#706).
  CHECK(assistantLabelMatches(AssistantClient::ClaudeDesktop, QStringLiteral("claude-ai")));
  CHECK(!assistantLabelMatches(AssistantClient::ClaudeCode, QStringLiteral("claude-ai")));
  CHECK(assistantLabelMatches(AssistantClient::ClaudeCode, QStringLiteral("claude-code")));
  CHECK(!assistantLabelMatches(AssistantClient::ClaudeDesktop, QStringLiteral("claude-code")));
  CHECK(assistantLabelMatches(AssistantClient::Codex, QStringLiteral("codex-mcp-client")));
  CHECK(!assistantLabelMatches(AssistantClient::Other, QStringLiteral("Codex")));
  CHECK(assistantLabelMatches(AssistantClient::Other, QStringLiteral("example-client")));
  CHECK(!assistantLabelMatches(AssistantClient::Codex, QStringLiteral("example-client")));
  CHECK(json.value(QStringLiteral("command")) == bridge);
  CHECK(json.value(QStringLiteral("args")) == QJsonArray({QStringLiteral("mcp"), QStringLiteral("--connect")}));
  for (bool win : {false, true}) {
    const auto manifest = assistantBundleManifest(bridge, win, win ? QStringLiteral("win32") : QStringLiteral("darwin"));
    const auto server = manifest.value(QStringLiteral("server")).toObject();
    CHECK(manifest.value(QStringLiteral("manifest_version")) == QStringLiteral("0.3"));
    CHECK(server.value(QStringLiteral("type")) == QStringLiteral("binary"));
    CHECK(server.value(QStringLiteral("entry_point")).toString().endsWith(win ? QStringLiteral(".exe") : QStringLiteral("launcher")));
    CHECK(server.value(QStringLiteral("mcp_config")).toObject().value(QStringLiteral("args")).toArray().first() == bridge);
  }

  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QString path = directory.filePath(QStringLiteral("config.toml"));
  const QString initial = QStringLiteral("# Retain this comment\nmodel = 'example'\n");
  const QString updated = initial + QStringLiteral("\n[mcp_servers.mitcad-desktop]\ncommand = 'example'\n");
  QString content, backup, error;
  CHECK(readAssistantConfiguration(path, content, error));
  CHECK(content.isEmpty());
  CHECK(put(path, initial.toUtf8()));
  CHECK(writeAssistantConfiguration(path, initial, updated, backup, error));
  CHECK(!backup.isEmpty() && backup != path);
  QFile previous(backup);
  CHECK(previous.open(QIODevice::ReadOnly));
  CHECK(previous.readAll() == initial.toUtf8());
  CHECK(readAssistantConfiguration(path, content, error) && content == updated);
  const auto files = QDir(directory.path()).entryList(QDir::Files);
  CHECK(writeAssistantConfiguration(path, updated, updated, backup, error));
  CHECK(backup.isEmpty());
  CHECK(QDir(directory.path()).entryList(QDir::Files) == files);
  CHECK(!writeAssistantConfiguration(path, initial, QStringLiteral("wrong"), backup, error));
  CHECK(error.contains(QStringLiteral("changed")));
  CHECK(readAssistantConfiguration(path, content, error) && content == updated);
  CHECK(!readAssistantConfiguration(QStringLiteral("relative/config.toml"), content, error));
  CHECK(put(directory.filePath(QStringLiteral("invalid.toml")), QByteArray("\xff", 1)));
  CHECK(!readAssistantConfiguration(directory.filePath(QStringLiteral("invalid.toml")), content, error));
  CHECK(put(directory.filePath(QStringLiteral("requirements.toml")), QByteArrayLiteral("# Managed\n")));
  CHECK(!assistantConfigurationPolicy(path).isEmpty());
  // Claude Code's settings file with years of projects' history, over the
  // 4 MiB that other settings files may have (mitcad#760): read, verified
  // and replaced with Claude Code's own limit.
  {
    QJsonObject entry{{QStringLiteral("command"), bridge},
                      {QStringLiteral("args"), QJsonArray{QStringLiteral("mcp"), QStringLiteral("--connect")}}};
    QJsonObject history{{QStringLiteral("display"), QString(5 * 1024 * 1024, QLatin1Char('x'))}};
    QJsonObject projects{{QStringLiteral("/home/user/project"), QJsonObject{{QStringLiteral("history"), QJsonArray{history}}}}};
    QJsonObject large{{QStringLiteral("projects"), projects},
                      {QStringLiteral("mcpServers"), QJsonObject{{QStringLiteral("mitcad"), entry}}}};
    const QByteArray bytes = QJsonDocument(large).toJson(QJsonDocument::Compact);
    CHECK(bytes.size() > 4 * 1024 * 1024);
    const QString claudePath = directory.filePath(QStringLiteral(".claude.json"));
    CHECK(put(claudePath, bytes));
    CHECK(!readAssistantConfiguration(claudePath, content, error));
    CHECK(error.contains(QStringLiteral("larger")));
    CHECK(readAssistantConfiguration(claudePath, content, error, kClaudeConfigurationLimit));
    CHECK(content.toUtf8() == bytes);
    const QJsonObject parsed = QJsonDocument::fromJson(content.toUtf8()).object();
    CHECK(parsed.value(QStringLiteral("mcpServers")).toObject().value(QStringLiteral("mitcad")).toObject() == entry);
    CHECK(parsed.value(QStringLiteral("projects")) == large.value(QStringLiteral("projects")));
    // The compare-before-write reads it with the same limit.
    QJsonObject changed = large;
    changed.insert(QStringLiteral("keep"), QStringLiteral("unchanged"));
    const QString written = QString::fromUtf8(QJsonDocument(changed).toJson(QJsonDocument::Compact));
    CHECK(!writeAssistantConfiguration(claudePath, content, written, backup, error));
    CHECK(writeAssistantConfiguration(claudePath, content, written, backup, error, kClaudeConfigurationLimit));
    CHECK(!backup.isEmpty());
    CHECK(readAssistantConfiguration(claudePath, content, error, kClaudeConfigurationLimit) && content == written);
  }
  // Claude Desktop from the Microsoft Store (mitcad#763): its package's
  // folder under the local application data's Packages, found without any
  // `.mcpb` association; a fake LOCALAPPDATA and APPDATA here.
  {
    QTemporaryDir local, roaming, other;
    CHECK(local.isValid() && roaming.isValid() && other.isValid());
    CHECK(!claudeDesktopPackaged(QString()));
    CHECK(!claudeDesktopPackaged(local.path()));
    CHECK(!claudeDesktopInWindowsFolders(local.path(), roaming.path()));
    CHECK(QDir(local.path()).mkpath(QStringLiteral("Packages/Example.App_abc123")));
    CHECK(put(QDir(local.path()).filePath(QStringLiteral("Packages/ClaudeNotes.txt")), QByteArrayLiteral("x")));
    CHECK(!claudeDesktopPackaged(local.path()));
    // Another family's folder, however named, is not this package.
    CHECK(QDir(local.path()).mkpath(QStringLiteral("Packages/Claude_other123")));
    CHECK(QDir(local.path()).mkpath(QStringLiteral("Packages/Example.Claude_other123")));
    CHECK(!claudeDesktopPackaged(local.path()));
    CHECK(!claudeDesktopApplicationInWindowsFolders(local.path()));
    // A file with the family's name is no package.
    CHECK(QDir(other.path()).mkpath(QStringLiteral("Packages")));
    CHECK(put(QDir(other.path()).filePath(QStringLiteral("Packages/") + QLatin1String(kClaudeDesktopPackageFamily)),
              QByteArrayLiteral("x")));
    CHECK(!claudeDesktopPackaged(other.path()));
    CHECK(!claudeDesktopApplicationInWindowsFolders(other.path()));
    CHECK(!claudeDesktopApplicationInWindowsFolders(QString()));
    CHECK(QDir(local.path()).mkpath(QStringLiteral("Packages/") + QLatin1String(kClaudeDesktopPackageFamily)));
    CHECK(claudeDesktopPackaged(local.path()));
    CHECK(claudeDesktopApplicationInWindowsFolders(local.path()));
    CHECK(claudeDesktopInWindowsFolders(local.path(), QString()));
    // The settings folder is Claude Desktop found, but not its application:
    // an uninstalled Claude Desktop leaves it behind.
    CHECK(!claudeDesktopInWindowsFolders(other.path(), roaming.path()));
    CHECK(QDir(roaming.path()).mkpath(QStringLiteral("Claude")));
    CHECK(claudeDesktopInWindowsFolders(other.path(), roaming.path()));
    CHECK(!claudeDesktopApplicationInWindowsFolders(other.path()));
    // The installer's application folder.
    CHECK(QDir(other.path()).mkpath(QStringLiteral("AnthropicClaude")));
    CHECK(claudeDesktopApplicationInWindowsFolders(other.path()));
    CHECK(claudeDesktopInWindowsFolders(other.path(), QString()));
  }
  // Claude Code and Codex outside PATH (mitcad#956): Claude Code's own
  // installer's `.local/bin` and npm's global folder under the roaming
  // application data, with Windows' suffixes, tested on every platform
  // through the suffix list; on Windows by assistantExecutable itself.
  {
    QTemporaryDir home;
    CHECK(home.isValid());
    const QDir root(home.path());
    const QString roaming = root.filePath(QStringLiteral("AppData/Roaming"));
    const auto executableFile = [](const QString& path) {
      return QDir().mkpath(QFileInfo(path).absolutePath()) && put(path, QByteArrayLiteral("x")) &&
          QFile::setPermissions(path, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    };
    const QString claudeExe = root.filePath(QStringLiteral(".local/bin/claude.exe"));
    const QString codexCmd = root.filePath(QStringLiteral("AppData/Roaming/npm/codex.cmd"));
    CHECK(executableFile(claudeExe));
    CHECK(executableFile(codexCmd));
    // Windows' suffixes in PATHEXT's order, only these three; Windows' own
    // order without them; none elsewhere.
    const QStringList windowsSuffixes = assistantExecutableSuffixes(true, QStringLiteral(".COM;.EXE;.BAT;.CMD;.VBS;.JS"));
    CHECK(windowsSuffixes == QStringList({QStringLiteral(".exe"), QStringLiteral(".bat"), QStringLiteral(".cmd")}));
    CHECK(assistantExecutableSuffixes(true, QStringLiteral(".cmd; .EXE")) ==
          QStringList({QStringLiteral(".cmd"), QStringLiteral(".exe")}));
    CHECK(assistantExecutableSuffixes(true, QString()) == windowsSuffixes);
    CHECK(assistantExecutableSuffixes(false, QStringLiteral(".EXE")) == QStringList({QString()}));
    // The folders after PATH, the stored PATH's last and only once.
    const QString stored = root.filePath(QStringLiteral("stored"));
    const QStringList folders = assistantFallbackFolders(home.path(), roaming, {stored, root.filePath(QStringLiteral(".local/bin"))});
    CHECK(folders == QStringList({root.filePath(QStringLiteral(".local/bin")), QDir(roaming).filePath(QStringLiteral("npm")),
                                  root.filePath(QStringLiteral(".cargo/bin")), stored}));
    CHECK(!assistantFallbackFolders(home.path(), QString(), {}).contains(QDir(roaming).filePath(QStringLiteral("npm"))));
    CHECK(findAssistantExecutable(QStringLiteral("claude"), folders, windowsSuffixes) == QFileInfo(claudeExe).canonicalFilePath());
    CHECK(findAssistantExecutable(QStringLiteral("codex"), folders, windowsSuffixes) == QFileInfo(codexCmd).canonicalFilePath());
    CHECK(findAssistantExecutable(QStringLiteral("claude"), folders, {QString()}).isEmpty());
    // A folder's suffixes in their order; an earlier folder first.
    CHECK(executableFile(QDir(stored).filePath(QStringLiteral("tool.cmd"))));
    CHECK(executableFile(QDir(stored).filePath(QStringLiteral("tool.exe"))));
    CHECK(findAssistantExecutable(QStringLiteral("tool"), folders, windowsSuffixes).endsWith(QStringLiteral("tool.exe")));
    CHECK(findAssistantExecutable(QStringLiteral("tool"), folders, {QStringLiteral(".cmd"), QStringLiteral(".exe")})
              .endsWith(QStringLiteral("tool.cmd")));
    CHECK(executableFile(root.filePath(QStringLiteral(".cargo/bin/tool.bat"))));
    CHECK(findAssistantExecutable(QStringLiteral("tool"), folders, windowsSuffixes).endsWith(QStringLiteral("tool.bat")));
    // A stored PATH: quotes removed, `%NAME%` expanded, an unknown name kept.
    qputenv("MITCAD_TEST_STORED_HOME", QByteArrayLiteral("C:\\Users\\Example"));
    CHECK(expandWindowsVariables(QStringLiteral("%MITCAD_TEST_STORED_HOME%\\.local\\bin")) ==
          QStringLiteral("C:\\Users\\Example\\.local\\bin"));
    CHECK(expandWindowsVariables(QStringLiteral("%MITCAD_TEST_NOT_SET_956%\\bin")) == QStringLiteral("%MITCAD_TEST_NOT_SET_956%\\bin"));
    CHECK(windowsPathEntries(QStringLiteral("\"%MITCAD_TEST_STORED_HOME%\\npm\";;C:\\Tools ; ")) ==
          QStringList({QStringLiteral("C:/Users/Example/npm"), QStringLiteral("C:/Tools")}));
    qunsetenv("MITCAD_TEST_STORED_HOME");
#ifdef Q_OS_WIN
    // The home's own folders: `.local\bin\claude.exe`, `AppData\Roaming\npm\codex.cmd`.
    CHECK(assistantExecutable(QStringLiteral("claude"), home.path()) == QFileInfo(claudeExe).canonicalFilePath());
    CHECK(assistantExecutable(QStringLiteral("codex"), home.path()) == QFileInfo(codexCmd).canonicalFilePath());
#else
    // No suffix here: the installer's `claude`, never `claude.exe`.
    CHECK(assistantExecutable(QStringLiteral("claude"), home.path()).isEmpty());
    CHECK(executableFile(root.filePath(QStringLiteral(".local/bin/claude"))));
    CHECK(assistantExecutable(QStringLiteral("claude"), home.path()) ==
          QFileInfo(root.filePath(QStringLiteral(".local/bin/claude"))).canonicalFilePath());
#endif
  }
#ifndef Q_OS_WIN
  const QString linked = directory.filePath(QStringLiteral("linked.toml"));
  CHECK(QFile::link(path, linked));
  CHECK(!writeAssistantConfiguration(linked, updated, QStringLiteral("wrong"), backup, error));
#endif
  return failures;
}
