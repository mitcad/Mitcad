// SPDX-License-Identifier: MIT
#include "mcp/AssistantSetup.hpp"

#include <cstdio>
#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QObject>
#include <QSettings>
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
  // Claude Desktop's chat and its Code tab by names users know (mitcad#1299).
  CHECK(assistantDisplayName(QStringLiteral("claude-ai")) == QStringLiteral("Claude Desktop (chat)"));
  CHECK(assistantDisplayName(QStringLiteral("local-agent-mode-MitCad")) == QStringLiteral("Claude Desktop (Code tab)"));
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
  // Claude Desktop's Code tab (mitcad#1299): Claude Desktop, nothing else.
  CHECK(assistantLabelMatches(AssistantClient::ClaudeDesktop, QStringLiteral("local-agent-mode-MitCad")));
  CHECK(!assistantLabelMatches(AssistantClient::ClaudeCode, QStringLiteral("local-agent-mode-MitCad")));
  CHECK(!assistantLabelMatches(AssistantClient::Codex, QStringLiteral("local-agent-mode-MitCad")));
  CHECK(!assistantLabelMatches(AssistantClient::Other, QStringLiteral("local-agent-mode-MitCad")));
  CHECK(assistantClientOf(QStringLiteral("local-agent-mode-MitCad")) == AssistantClient::ClaudeDesktop);
  CHECK(assistantRowKey(QStringLiteral("local-agent-mode-MitCad")) == QStringLiteral("claude-desktop-code"));
  CHECK(assistantRowKey(QStringLiteral("claude-ai")) == QStringLiteral("claude-desktop-chat"));
  CHECK(assistantRowKey(QStringLiteral("claude-code")) == QStringLiteral("claude-code"));
  CHECK(assistantRowKey(QStringLiteral("codex-mcp-client")) == QStringLiteral("codex"));
  CHECK(assistantRowKey(QStringLiteral("example-client")) == QStringLiteral("client:example-client"));
  CHECK(assistantRowName(QStringLiteral("client:example-client")) == QStringLiteral("example-client"));
  CHECK(assistantRows(AssistantClient::ClaudeDesktop) ==
        QStringList({QStringLiteral("claude-desktop-chat"), QStringLiteral("claude-desktop-code")}));
  CHECK(assistantRows(AssistantClient::Other).isEmpty());
  CHECK(assistantRowClient(QStringLiteral("claude-desktop-code")) == AssistantClient::ClaudeDesktop);
  CHECK(assistantRowClient(QStringLiteral("client:example-client")) == AssistantClient::Other);
  CHECK(assistantLabelMatches(AssistantClient::Codex, QStringLiteral("codex-mcp-client")));
  CHECK(!assistantLabelMatches(AssistantClient::Other, QStringLiteral("Codex")));
  CHECK(assistantLabelMatches(AssistantClient::Other, QStringLiteral("example-client")));
  CHECK(!assistantLabelMatches(AssistantClient::Codex, QStringLiteral("example-client")));
  CHECK(json.value(QStringLiteral("command")) == bridge);
  CHECK(json.value(QStringLiteral("args")) == QJsonArray({QStringLiteral("mcp"), QStringLiteral("--connect")}));
  // An AppImage registers its own file with `cli` (mitcad#1170): the mount
  // folder's mitcad-cli is gone once MitCad closes.
  {
    QTemporaryDir apps;
    CHECK(apps.isValid());
    const QString image = apps.filePath(QStringLiteral("MitCad.AppImage"));
    CHECK(put(image, "AppImage"));
    const QString mounted = QStringLiteral("/tmp/.mount_MitCad1/usr/bin/mitcad-cli");
    const BridgeLaunch appImage = registeredBridge(mounted, image);
    CHECK(appImage.program == QFileInfo(image).absoluteFilePath());
    CHECK(appImage.leading == QStringList{QStringLiteral("cli")});
    const QStringList connect{QStringLiteral("cli"), QStringLiteral("mcp"), QStringLiteral("--connect")};
    CHECK(bridgeConnectArguments(appImage.leading) == connect);
    const auto claudeImage = assistantRegistration(AssistantClient::ClaudeCode, appImage.program, appImage.leading);
    CHECK(claudeImage.mid(6) == QStringList{appImage.program} + connect);
    const auto codexImage = assistantRegistration(AssistantClient::Codex, appImage.program, appImage.leading);
    CHECK(codexImage.mid(4) == QStringList{appImage.program} + connect);
    const auto imageJson = assistantJsonConfiguration(appImage.program, appImage.leading)
        .value(QStringLiteral("mcpServers")).toObject().value(QStringLiteral("mitcad")).toObject();
    CHECK(imageJson.value(QStringLiteral("command")) == appImage.program);
    CHECK(imageJson.value(QStringLiteral("args")) == QJsonArray::fromStringList(connect));
    // A path with a space is one quoted word of the command, in the
    // platform's native separators as assistantCommand writes them.
    const QString spaced = QStringLiteral("/home/user/My Apps/MitCad.AppImage");
    CHECK(assistantCommand(spaced, connect, false) ==
          QLatin1Char('\'') + QDir::toNativeSeparators(spaced) + QStringLiteral("' cli mcp --connect"));
    // Not an AppImage (no APPIMAGE, a relative path or a missing file): the bridge itself.
    const QStringList notImages{QString(), QStringLiteral("MitCad.AppImage"), apps.filePath(QStringLiteral("gone.AppImage"))};
    for (const QString& none : notImages) {
      const BridgeLaunch plain = registeredBridge(mounted, none);
      CHECK(plain.program == mounted);
      CHECK(plain.leading.isEmpty());
    }
    CHECK(bridgeConnectArguments() == QStringList({QStringLiteral("mcp"), QStringLiteral("--connect")}));
  }
  for (bool win : {false, true}) {
    const auto manifest =assistantBundleManifest(bridge, win, win ? QStringLiteral("win32") : QStringLiteral("darwin"));
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
  // The extension installed in Claude Desktop (mitcad#1288): its manifest
  // in a settings folder's Claude Extensions, the installer's and the
  // Microsoft Store package's; one that starts another bridge is not this
  // one's.
  {
    QTemporaryDir local, roaming;
    CHECK(local.isValid() && roaming.isValid());
    const QStringList folders = claudeDesktopWindowsSettingsFolders(local.path(), roaming.path());
    CHECK(folders.size() == 2 && folders.at(0) == QDir(roaming.path()).filePath(QStringLiteral("Claude")) &&
          folders.at(1) == QDir(local.path()).filePath(QStringLiteral(
                               "Packages/Claude_pzs8sxrjxfjjc/LocalCache/Roaming/Claude")));
    CHECK(claudeDesktopWindowsSettingsFolders(QString(), QString()).isEmpty());
    CHECK(!claudeDesktopExtension(folders).found);
    const QString other = QDir(folders.at(1)).filePath(QStringLiteral("Claude Extensions/local.mcpb.someone.notes"));
    CHECK(QDir().mkpath(other));
    CHECK(put(QDir(other).filePath(QStringLiteral("manifest.json")), QByteArrayLiteral("{\"name\": \"notes\"}")));
    CHECK(!claudeDesktopExtension(folders).found);
    const QString ours =
        QDir(folders.at(1)).filePath(QStringLiteral("Claude Extensions/local.mcpb.mitcad-developers.mitcad-desktop"));
    CHECK(QDir().mkpath(ours));
    const QString installed = QStringLiteral("C:\\Program Files\\MitCad\\bin\\mitcad-cli.exe");
    CHECK(put(QDir(ours).filePath(QStringLiteral("manifest.json")),
              QJsonDocument(assistantBundleManifest(installed, true, QStringLiteral("win32"))).toJson()));
    const InstalledDesktopExtension extension = claudeDesktopExtension(folders);
    CHECK(extension.found && extension.bridge == installed);
    CHECK(claudeDesktopExtensionStarts(extension, installed));
#ifdef Q_OS_WIN
    // Only Windows reads a backslash as a separator (QDir::fromNativeSeparators).
    CHECK(claudeDesktopExtensionStarts(extension, QStringLiteral("C:/Program Files/MitCad/bin/mitcad-cli.exe")));
#endif
    CHECK(!claudeDesktopExtensionStarts(extension, QStringLiteral("C:/Other/mitcad-cli.exe")));
    CHECK(!claudeDesktopExtensionStarts(InstalledDesktopExtension{}, installed));
  }
  // The Claude Code Claude Desktop keeps (mitcad#1289): the highest
  // version's under claude-code; an executable's place for people.
  {
    QTemporaryDir settings;
    CHECK(settings.isValid());
#ifdef Q_OS_WIN
    const QString program = QStringLiteral("claude.exe");
#else
    const QString program = QStringLiteral("claude");
#endif
    const auto executableFile = [](const QString& path) {
      return QDir().mkpath(QFileInfo(path).absolutePath()) && put(path, QByteArrayLiteral("x")) &&
          QFile::setPermissions(path, QFileDevice::ReadOwner | QFileDevice::WriteOwner | QFileDevice::ExeOwner);
    };
    CHECK(claudeDesktopClaudeCode({settings.path()}).isEmpty());
    const QDir versions(QDir(settings.path()).filePath(QStringLiteral("claude-code")));
    CHECK(executableFile(versions.filePath(QStringLiteral("2.0.9/")) + program));
    CHECK(executableFile(versions.filePath(QStringLiteral("2.0.10/")) + program));
    CHECK(QDir().mkpath(versions.filePath(QStringLiteral("2.1.0")))); // no program in it
    CHECK(QDir().mkpath(versions.filePath(QStringLiteral("not-a-version"))));
    CHECK(claudeDesktopClaudeCode({settings.path()}) ==
          QFileInfo(versions.filePath(QStringLiteral("2.0.10/")) + program).canonicalFilePath());
    // As the Windows application keeps it: under Claude-Data's
    // claude_<family>, a build folder inside the version's.
    QTemporaryDir local;
    CHECK(local.isValid());
    CHECK(claudeDesktopDataFolders(local.path()).isEmpty());
    const QDir data(QDir(local.path()).filePath(QStringLiteral("Claude-Data/claude_pzs8sxrjxfjjc")));
    CHECK(executableFile(data.filePath(QStringLiteral("claude-code/2.1.309/9078b9c1d466/")) + program));
    CHECK(claudeDesktopDataFolders(local.path()) == QStringList{data.path()});
    CHECK(claudeDesktopClaudeCode(QStringList{settings.path()} + claudeDesktopDataFolders(local.path())) ==
          QFileInfo(data.filePath(QStringLiteral("claude-code/2.1.309/9078b9c1d466/")) + program).canonicalFilePath());
    CHECK(claudeDesktopDataFolders(QString()).isEmpty());
    // Both found ones are Claude Desktop's by their paths (mitcad#1297).
    CHECK(claudeDesktopKeepsClaudeCode(claudeDesktopClaudeCode({settings.path()})));
    CHECK(claudeDesktopKeepsClaudeCode(claudeDesktopClaudeCode(claudeDesktopDataFolders(local.path()))));
  }
  // Claude Desktop's Claude Code by its path alone (mitcad#1297): in the
  // version's folder, or in a build folder under it as current versions keep
  // it; synthetic paths, nothing on disk.
  {
    CHECK(claudeDesktopKeepsClaudeCode(QStringLiteral("/home/someone/.config/Claude/claude-code/2.0.10/claude")));
    CHECK(claudeDesktopKeepsClaudeCode(
        QStringLiteral("/Users/someone/Library/Application Support/Claude/claude-code/2.1.4/claude")));
    CHECK(claudeDesktopKeepsClaudeCode(
        QStringLiteral("C:/Users/someone/AppData/Local/Claude-Data/claude_pzs8sxrjxfjjc/claude-code/2.1.309/"
                       "9078b9c1d466/claude.exe")));
    CHECK(claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/2.1.309/9078b9c1d466/claude")));
#ifdef Q_OS_WIN
    // Only Windows reads a backslash as a separator.
    CHECK(claudeDesktopKeepsClaudeCode(
        QStringLiteral("C:\\Users\\someone\\AppData\\Local\\Claude-Data\\claude_x\\claude-code\\2.1.309\\ab12\\claude.exe")));
#endif
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/home/someone/.local/bin/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/home/someone/.claude/local/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/not-a-version/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/not-a-version/9078b9c1d466/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/2.1.309/one/two/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/2.1.309/codex")));
    CHECK(!claudeDesktopKeepsClaudeCode(QStringLiteral("/data/claude-code/claude")));
    CHECK(!claudeDesktopKeepsClaudeCode(QString()));
    // The configuration folder a Claude Code signs in with: CLAUDE_CONFIG_DIR,
    // else .claude in the home folder.
    CHECK(claudeConfigurationFolder(QString(), QStringLiteral("/home/someone")) == QStringLiteral("/home/someone/.claude"));
    CHECK(claudeConfigurationFolder(QStringLiteral("  "), QStringLiteral("/home/someone")) ==
          QStringLiteral("/home/someone/.claude"));
    CHECK(claudeConfigurationFolder(QStringLiteral("/srv/claude-config/"), QStringLiteral("/home/someone")) ==
          QDir::cleanPath(QDir(QStringLiteral("/srv/claude-config")).absolutePath()));
    CHECK(assistantPlace(QStringLiteral("/home/someone/.local/bin/claude"), QStringLiteral("/home/someone")) ==
          QStringLiteral("~/.local/bin"));
    CHECK(assistantPlace(QStringLiteral("/opt/tools/claude"), QStringLiteral("/home/someone")) ==
          QDir::toNativeSeparators(QStringLiteral("/opt/tools")));
  }
  // Blocks (mitcad#1289, kept in the settings since mitcad#1299): a blocked
  // label's row, and the same row's other labels; Other only by its own
  // label; Claude Desktop's chat and Code tab apart; allowed again.
  {
    CHECK(assistantRowBlocked({QStringLiteral("claude-code")}, QStringLiteral("claude-code")));
    CHECK(assistantRowBlocked({QStringLiteral("claude-code")}, QStringLiteral("Claude Code (beta)")));
    CHECK(!assistantRowBlocked({QStringLiteral("claude-code")}, QStringLiteral("claude-ai")));
    CHECK(!assistantRowBlocked({QStringLiteral("claude-code")}, QStringLiteral("codex-mcp-client")));
    CHECK(assistantRowBlocked({QStringLiteral("client:example-client")}, QStringLiteral("example-client")));
    CHECK(!assistantRowBlocked({QStringLiteral("client:example-client")}, QStringLiteral("other-client")));
    CHECK(assistantRowBlocked({QStringLiteral("claude-desktop-code")}, QStringLiteral("local-agent-mode-MitCad")));
    CHECK(!assistantRowBlocked({QStringLiteral("claude-desktop-code")}, QStringLiteral("claude-ai")));
    CHECK(assistantClientOf(QStringLiteral("claude-ai")) == AssistantClient::ClaudeDesktop);
    CHECK(assistantClientOf(QStringLiteral("example-client")) == AssistantClient::Other);

    // The settings of a test's own, and a restart: the file as saved read
    // from a fresh folder (QSettings keeps a file it has read in memory).
    const QSettings::Format format = QSettings::defaultFormat();
    const QString organization = QCoreApplication::organizationName();
    const QString application = QCoreApplication::applicationName();
    QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
    QCoreApplication::setApplicationName(QStringLiteral("assistant_blocks"));
    QSettings::setDefaultFormat(QSettings::IniFormat);
    QTemporaryDir first;
    QTemporaryDir second;
    CHECK(first.isValid() && second.isValid());
    QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, first.path());
    QObject watched;
    int changes = 0;
    watchAssistantBlocks(&watched, [&changes] { ++changes; });
    CHECK(blockedAssistantRows().isEmpty());
    blockAssistants({QStringLiteral("claude-code"), QStringLiteral("example-client")});
    CHECK(changes == 1);
    CHECK(assistantBlocked(QStringLiteral("claude-code")));
    CHECK(assistantClientBlocked(AssistantClient::ClaudeCode));
    CHECK(assistantClientBlocked(AssistantClient::Other));
    CHECK(!assistantClientBlocked(AssistantClient::Codex));
    CHECK(!allowAssistant(AssistantClient::Codex));
    CHECK(allowAssistant(AssistantClient::ClaudeCode));
    CHECK(!assistantBlocked(QStringLiteral("claude-code")));
    CHECK(assistantBlocked(QStringLiteral("example-client")));
    CHECK(allowAssistant(AssistantClient::Other));
    CHECK(blockedAssistantRows().isEmpty());
    CHECK(changes == 3);
    // A row unchecked in the menu: Claude Desktop's Code tab, not its chat;
    // the refusal names the row and how to allow it.
    CHECK(blockAssistantRow(QStringLiteral("claude-desktop-code")));
    CHECK(!blockAssistantRow(QStringLiteral("claude-desktop-code")));
    CHECK(assistantRefusal(QStringLiteral("claude-ai")).isEmpty());
    CHECK(assistantRefusal(QStringLiteral("local-agent-mode-MitCad"))
              .contains(QStringLiteral("check Claude Desktop (Code tab) under Incoming connections")));
    rememberAssistantRows({QStringLiteral("claude-code"), QStringLiteral("client:example-client")});
    QSettings().sync();
    // The restart: still blocked, still refused, the remembered rows kept
    // (a known assistant's only).
    {
      const QString name = QCoreApplication::organizationName() + QLatin1Char('/') +
                           QCoreApplication::applicationName() + QStringLiteral(".ini");
      const QString copied = QDir(second.path()).filePath(name);
      CHECK(QDir().mkpath(QFileInfo(copied).path()));
      CHECK(QFile::copy(QDir(first.path()).filePath(name), copied));
      QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, second.path());
      CHECK(QSettings().fileName() == copied);
    }
    CHECK(blockedAssistantRows() == QStringList{QStringLiteral("claude-desktop-code")});
    CHECK(!assistantRefusal(QStringLiteral("local-agent-mode-MitCad")).isEmpty());
    CHECK(assistantRefusal(QStringLiteral("claude-ai")).isEmpty());
    CHECK(rememberedAssistantRows() == QStringList{QStringLiteral("claude-code")});
    // Checked again (or Connect for Claude Desktop): allowed.
    CHECK(allowAssistantRow(QStringLiteral("claude-desktop-code")));
    CHECK(!allowAssistantRow(QStringLiteral("claude-desktop-code")));
    CHECK(assistantRefusal(QStringLiteral("local-agent-mode-MitCad")).isEmpty());
    CHECK(changes == 5);
    QCoreApplication::setOrganizationName(organization);
    QCoreApplication::setApplicationName(application);
    QSettings::setDefaultFormat(format);
  }
#ifndef Q_OS_WIN
  const QString linked = directory.filePath(QStringLiteral("linked.toml"));
  CHECK(QFile::link(path, linked));
  CHECK(!writeAssistantConfiguration(linked, updated, QStringLiteral("wrong"), backup, error));
#endif
  return failures;
}
