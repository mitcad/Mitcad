// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QJsonObject>
#include <QString>
#include <QStringList>

class QObject;

namespace mitcad::mcp {

enum class AssistantClient { ClaudeDesktop, ClaudeCode, Codex, Other };

QString installedBridge(const QString& applicationDirectory);

// MitCad's bridge, mitcad-cli, beside the running executable (mitcad#1170):
// the one lookup of Connect AI Assistant, the chat panel, the assistant
// indicator and the UI tests' driver.
//
// The folder of the running executable. On Linux the real executable's
// (/proc/self/exe), as the render worker is found: for an AppImage, Qt names
// the AppImage file's folder as the application's, while the executables are
// in the AppImage's mount folder. Elsewhere QCoreApplication's
// applicationDirPath.
QString executableDirectory();
// installedBridge of executableDirectory(): the bundled mitcad-cli (a path
// inside a running AppImage's mount folder, valid while MitCad runs).
QString bundledBridge();
// How an assistant's settings start the bridge: the program, and the
// arguments before `mcp --connect` (`cli` for an AppImage).
struct BridgeLaunch {
  QString program;
  QStringList leading;
};
// The launch an assistant registers. For an AppImage (`appImage`, the
// runtime's APPIMAGE: an absolute path of an existing file) the AppImage file
// itself with `cli`, whose AppRun then starts the bundled mitcad-cli: the
// mount folder changes on each start and is gone once MitCad closes. Else the
// bridge, with no leading arguments.
BridgeLaunch registeredBridge(const QString& bridge, const QString& appImage);
// The same for this process: bundledBridge() and, on Linux, APPIMAGE.
BridgeLaunch registeredBridge();
// The bridge's arguments to connect: `leading`, then `mcp --connect`.
QStringList bridgeConnectArguments(const QStringList& leading = {});

// Finding Claude Code (`claude`) and Codex (`codex`), the one lookup that
// Connect AI Assistant and the chat panel (mitcad#947) share (mitcad#956):
// PATH first, then the folders their installers use, which a PATH may not
// have, or not yet in a running MitCad's environment.
//
// The suffixes of an executable's name: on Windows `.exe`, `.cmd` and `.bat`
// (npm's shims) in PATHEXT's order, those three in Windows' order when
// PATHEXT names none of them; elsewhere none.
QStringList assistantExecutableSuffixes(bool windows, const QString& pathext);
// `%NAME%` replaced by the environment's value, as Windows expands a stored
// PATH; an unknown name stays as it is.
QString expandWindowsVariables(const QString& text);
// A PATH value's folders (`;`-separated, quotes removed, expanded).
QStringList windowsPathEntries(const QString& value);
// The user's PATH as stored (`HKCU\Environment`, `Path`), expanded: an
// installer's change is there before a running MitCad's environment has it.
// Empty outside Windows.
QStringList storedUserPath();
// The folders after PATH, in order: the home's `.local/bin` (Claude Code's
// own installer), `npm` in the roaming application data when there is one
// (npm's global installs on Windows), the home's `.cargo/bin`, then the
// stored PATH's folders.
QStringList assistantFallbackFolders(const QString& home, const QString& roamingAppData, const QStringList& storedPath);
// The first executable file `name` + suffix, folder by folder, each folder's
// suffixes in their order.
QString findAssistantExecutable(const QString& name, const QStringList& folders, const QStringList& suffixes);
// The client's executable: PATH, then assistantFallbackFolders with this
// platform's suffixes (on Windows APPDATA and the stored PATH), then on macOS
// the ChatGPT application's Codex. With `home`, only that home's folders (its
// roaming application data at `AppData/Roaming` on Windows), no PATH or
// registry: an isolated test. A test run (MITCAD_TEST_INPUT) looks only in
// MITCAD_TEST_ASSISTANT_EXECUTABLES (`none`: nothing), then in the folders of
// the home MITCAD_TEST_ASSISTANT_HOME, when either is set.
QString assistantExecutable(const QString& name, const QString& home = {});
// Every executable of the client the lookup finds, each once and in the
// lookup's order (mitcad#1289): PATH, then each of the folders after it on
// its own, then on macOS the ChatGPT application's Codex; for `claude` last
// the Claude Code that Claude Desktop keeps (claudeDesktopClaudeCode). The
// first is assistantExecutable's. A test run as assistantExecutable says,
// each folder on its own, and for `claude` the Claude Desktop folders in
// the work folder (MITCAD_ASSISTANT_TEST_WORK's `local-app-data` and
// `roaming-app-data`, on every platform).
QStringList assistantExecutables(const QString& name);
// The Claude Code that Claude Desktop keeps in one of its folders
// (`claude-code/<version>/claude`, or `claude-code/<version>/<build id>/
// claude.exe` as the Windows application keeps it): the highest version's,
// empty when none is there. Whether it runs on its own and with whose
// sign-in is its own `claude auth status`'s to tell (the chat checks it as
// any other).
QString claudeDesktopClaudeCode(const QStringList& settingsFolders);
// Whether an executable is one Claude Desktop keeps (mitcad#1297), by its
// path alone: `claude-code/<version>/claude` or, as current versions keep
// it, `claude-code/<version>/<build id>/claude` (`claude.exe` on Windows).
bool claudeDesktopKeepsClaudeCode(const QString& program);
// The configuration folder a Claude Code started by MitCad signs in with
// (mitcad#1297): CLAUDE_CONFIG_DIR as MitCad's environment gives it
// (`configured`), else `.claude` in `home`. Programs with one folder share
// one sign-in.
QString claudeConfigurationFolder(const QString& configured, const QString& home);
// The Windows application's data folders, each subfolder of
// `<localAppData>/Claude-Data` (`claude_<package family id>`), where it
// keeps its Claude Code; none without localAppData.
QStringList claudeDesktopDataFolders(const QString& localAppData);
// Claude Desktop's settings folders on this computer (on Windows
// claudeDesktopWindowsSettingsFolders, on macOS Application Support's
// `Claude`), none elsewhere.
QStringList claudeDesktopSettingsFolders();
// An executable's folder for people, beside its name when several of one
// client are found: under `home`, `~/` and the rest with forward slashes;
// else the folder with the platform's separators.
QString assistantPlace(const QString& executable, const QString& home);
// The program of a registration command: the bare name when PATH finds it,
// else the full path where it was found, else the bare name.
QString assistantProgram(const QString& name);
QString codexConfigurationPath();
// One word of a command for PowerShell (`windows`) or a POSIX shell, quoted
// only when it needs it (or `always`).
QString assistantShellWord(const QString& value, bool windows, bool always = false);
// A command line: the program (with the platform's separators) and its
// arguments, each quoted only when it needs it; PowerShell's call operator
// only before a quoted program.
QString assistantCommand(const QString& program, const QStringList& arguments, bool windows);
// The registration's arguments and the JSON configuration start `bridge`
// with `leading`, then `mcp --connect` (BridgeLaunch).
QStringList assistantRegistration(AssistantClient client, const QString& bridge, const QStringList& leading = {});
QJsonObject assistantJsonConfiguration(const QString& bridge, const QStringList& leading = {});
QJsonObject assistantBundleManifest(const QString& bridge, bool windows, const QString& platform);
// The largest settings file read: Codex's TOML (and a file chosen under
// Advanced) is small; Claude Code keeps its projects' state and history in
// its .claude.json, which grows to tens of megabytes (mitcad#760), and only
// its parse and the compare-before-write need the content.
constexpr qint64 kAssistantConfigurationLimit = 4LL * 1024 * 1024;
constexpr qint64 kClaudeConfigurationLimit = 256LL * 1024 * 1024;
bool readAssistantConfiguration(const QString& path, QString& content, QString& error,
                                qint64 limit = kAssistantConfigurationLimit);
// Compare the file again before replacing it; a symlink or concurrent edit
// never gets overwritten. Existing bytes are backed up with a unique name.
bool writeAssistantConfiguration(const QString& path, const QString& expected, const QString& content,
                                 QString& backup, QString& error, qint64 limit = kAssistantConfigurationLimit);
// Managed settings are never changed by this user-level setup flow.
QString assistantConfigurationPolicy(const QString& path);
// Whether a connected client's display label (its clientInfo.name) is this
// assistant, so that the dialog shows Connected for the right choice. Labels
// are supplied by the clients: this picks a state to show, never a right.
// Claude Desktop's chat calls itself `claude-ai`, its Code tab
// `local-agent-mode-*` (mitcad#1299): both are Claude Desktop.
bool assistantLabelMatches(AssistantClient client, const QString& label);
// The assistant a client's label is (Other when no known one matches).
AssistantClient assistantClientOf(const QString& label);

// The rows of the assistant menu's Incoming connections (mitcad#1299), one
// per assistant, each by a key: `claude-desktop-chat` (`claude-ai` and
// Claude Desktop's other labels), `claude-desktop-code` (its Code tab,
// `local-agent-mode-*`), `claude-code`, `codex`, and `client:<label>` for a
// client no known assistant matches.
QString assistantRowKey(const QString& label);
// A row's name for people: "Claude Desktop (chat)", "Claude Desktop (Code
// tab)", "Claude Code", "Codex", or the other client's own label.
QString assistantRowName(const QString& key);
// The known assistants' rows, in the menu's order.
QStringList knownAssistantRows();
// The rows of a choice of Connect AI Assistant (Claude Desktop: its chat and
// its Code tab); none for Other, whose rows are its clients' labels.
QStringList assistantRows(AssistantClient client);
// The assistant a row is (Other for a client's own row).
AssistantClient assistantRowClient(const QString& key);

// Blocked rows (mitcad#1289; kept since mitcad#1299): a row unchecked in
// the assistant menu, or an assistant disconnected in Connect AI Assistant.
// MitCad refuses a blocked row's clients at their `initialize` until the
// row is checked again (or Connect allows the assistant). Kept in the
// user's settings (`assistant/blocked`), so a block holds after a restart;
// used on the GUI thread.
QStringList blockedAssistantRows();
// Each returns whether the row's state changed.
bool blockAssistantRow(const QString& key);
bool allowAssistantRow(const QString& key);
// Blocks the rows of these clients' labels (Disconnect).
void blockAssistants(const QStringList& labels);
// Allows every row of this assistant (Other: each client's own row);
// returns whether any was blocked.
bool allowAssistant(AssistantClient client);
// Whether this label's row is one of `blocked`.
bool assistantRowBlocked(const QStringList& blocked, const QString& label);
bool assistantBlocked(const QString& label);
// Whether one of this assistant's rows is blocked (Other: a client's own).
bool assistantClientBlocked(AssistantClient client);
// Why a client of this label is refused: the row it is and how to allow it
// again; empty while its row is not blocked.
QString assistantRefusal(const QString& label);
// Calls `changed` after every change of the blocked rows while `context`
// lives (each window's indicator).
void watchAssistantBlocks(QObject* context, std::function<void()> changed);
// The known assistants' rows set up with Connect AI Assistant or connected
// before (`assistant/rows`): the menu lists them after a restart too.
QStringList rememberedAssistantRows();
void rememberAssistantRows(const QStringList& keys);

// The assistant's name on its choice ("Claude Desktop", "ChatGPT / Codex").
QString assistantName(AssistantClient client);
// The assistant's name in a sentence ("Ready to connect Codex"): its
// choice's name, but "Codex" for "ChatGPT / Codex".
QString assistantSentenceName(AssistantClient client);
// A connected client's name for people: its row's name ("Claude Desktop
// (chat)" for "claude-ai", "Codex"), else the label as the client gave it.
QString assistantDisplayName(const QString& label);

// Claude Desktop installed from the Microsoft Store, an MSIX package
// (mitcad#763): its package family, and whether the package's folder is under
// Packages in this local application data folder (LOCALAPPDATA): only this
// family's, not another `Claude_*` one. The application asks the package
// manager; a test run takes the folder for the package. Such a package
// registers no `.mcpb` file association: that no application opens the
// extension then does not mean that Claude Desktop is missing.
inline constexpr char kClaudeDesktopPackageFamily[] = "Claude_pzs8sxrjxfjjc";
bool claudeDesktopPackaged(const QString& localAppData);
// Claude Desktop's application on Windows by the local application data
// folder alone: the installer's application folder or the package's folder
// above, never the settings folder, which an uninstalled Claude Desktop
// leaves behind.
bool claudeDesktopApplicationInWindowsFolders(const QString& localAppData);
// Claude Desktop on Windows by the user's application data folders
// (LOCALAPPDATA, APPDATA): the installer's application folder, the package
// above, or the settings folder Claude Desktop makes on its first start.
bool claudeDesktopInWindowsFolders(const QString& localAppData, const QString& roamingAppData);

// Claude Desktop's settings folders on Windows (mitcad#1288): the
// installer's in the roaming application data (`Claude`), the Microsoft Store
// package's in its LocalCache (`Packages/<family>/LocalCache/Roaming/Claude`);
// those of the empty arguments are left out.
QStringList claudeDesktopWindowsSettingsFolders(const QString& localAppData, const QString& roamingAppData);
// The MitCad extension installed in Claude Desktop: a folder in a settings
// folder's `Claude Extensions` whose `manifest.json` is named as
// assistantBundleManifest names it, and the bridge it starts (the first of
// its arguments; empty when it names none).
struct InstalledDesktopExtension {
  bool found = false;
  QString bridge;
};
InstalledDesktopExtension claudeDesktopExtension(const QStringList& settingsFolders);
// Whether that extension starts this bridge (with either separator; case
// aside on Windows): one that starts another MitCad's is installed again.
bool claudeDesktopExtensionStarts(const InstalledDesktopExtension& extension, const QString& bridge);

} // namespace mitcad::mcp
