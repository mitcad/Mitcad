// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad::mcp {

enum class AssistantClient { ClaudeDesktop, ClaudeCode, Codex, Other };

QString installedBridge(const QString& applicationDirectory);

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
QStringList assistantRegistration(AssistantClient client, const QString& bridge);
QJsonObject assistantJsonConfiguration(const QString& bridge);
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
bool assistantLabelMatches(AssistantClient client, const QString& label);
// The assistant's name on its choice ("Claude Desktop", "ChatGPT / Codex").
QString assistantName(AssistantClient client);
// The assistant's name in a sentence ("Ready to connect Codex"): its
// choice's name, but "Codex" for "ChatGPT / Codex".
QString assistantSentenceName(AssistantClient client);
// A connected client's name for people: the assistant its label matches, in
// a sentence's form ("Claude Desktop" for "claude-ai"), else the label as the
// client gave it.
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

} // namespace mitcad::mcp
