// SPDX-License-Identifier: MIT
// The chat's account check (Check again, mitcad#1155, agent/AccountCheck.hpp):
// right after the agent's check() a readiness it already had (the program
// gone, as at the last look; too old; signed out) ends the check, since no
// readiness change will come; Checking waits for one; an unfinished sign-in
// waits (the controller's timeout bounds it) but one that starts during the
// check ends it; Ready goes on to the probe. Check again is offered only for
// an account error while the agent can be used and no check runs. Entries
// with one configuration folder share one sign-in (mitcad#1297). Sign In
// Again (mitcad#1585, agent/SignInCommands.hpp): offered beside Check again
// for an agent whose sign-in MitCad knows only; its commands quoted for
// people, its scripts, and each platform's terminal; a second agent's
// start while one terminal runs (mitcad#1610).
#include "agent/AccountCheck.hpp"
#include "agent/SignInCommands.hpp"

#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "chat_account_check.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int chatAccountCheckTests() {
  using mitcad::agent::AccountCheckStep;
  using mitcad::agent::accountCheckOffered;
  using mitcad::agent::accountCheckStep;
  using mitcad::agent::agentUnusable;
  using Readiness = mitcad::agent::ChatAgent::Readiness;

  // Right after check() returned: nothing else will say how it ended.
  CHECK(accountCheckStep(Readiness::Missing, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Outdated, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SignedOut, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Checking, true) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::SigningIn, true) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::Ready, true) == AccountCheckStep::Probe);

  // A readiness change during the check.
  CHECK(accountCheckStep(Readiness::Checking, false) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::Ready, false) == AccountCheckStep::Probe);
  CHECK(accountCheckStep(Readiness::Missing, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Outdated, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SignedOut, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SigningIn, false) == AccountCheckStep::Fail);

  // The agent's own setup comes before the account error.
  CHECK(agentUnusable(Readiness::Missing));
  CHECK(agentUnusable(Readiness::Outdated));
  CHECK(!agentUnusable(Readiness::Ready));
  CHECK(!agentUnusable(Readiness::Checking));
  CHECK(!agentUnusable(Readiness::SignedOut));
  CHECK(!agentUnusable(Readiness::SigningIn));

  // Check again: an account error, the agent usable, no check running.
  CHECK(accountCheckOffered(true, Readiness::Ready, false));
  CHECK(accountCheckOffered(true, Readiness::SignedOut, false));
  CHECK(accountCheckOffered(true, Readiness::SigningIn, false));
  CHECK(!accountCheckOffered(false, Readiness::Ready, false));
  CHECK(!accountCheckOffered(true, Readiness::Ready, true));
  CHECK(!accountCheckOffered(true, Readiness::Missing, false));
  CHECK(!accountCheckOffered(true, Readiness::Outdated, false));

  // One sign-in shared by the entries with one configuration folder
  // (mitcad#1297): a Claude Code on PATH, Claude Desktop's, a third one with a
  // CLAUDE_CONFIG_DIR of its own, and agents with a sign-in of their own.
  using mitcad::agent::sameSignIn;
  const QStringList folders{QStringLiteral("/home/someone/.claude"), QStringLiteral("/home/someone/.claude/"),
                            QStringLiteral("/srv/other-claude"), QString(), QString(),
                            QStringLiteral("/home/someone/./.claude")};
  CHECK(sameSignIn(folders, 0) == (QList<int>{1, 5}));
  CHECK(sameSignIn(folders, 1) == (QList<int>{0, 5}));
  CHECK(sameSignIn(folders, 5) == (QList<int>{0, 1}));
  CHECK(sameSignIn(folders, 2).isEmpty());
  // An empty folder shares with none, not with another empty one.
  CHECK(sameSignIn(folders, 3).isEmpty());
  CHECK(sameSignIn(folders, 4).isEmpty());
  CHECK(sameSignIn(folders, -1).isEmpty());
  CHECK(sameSignIn(folders, 6).isEmpty());
  CHECK(sameSignIn({QStringLiteral("/home/someone/.claude")}, 0).isEmpty());
#ifdef Q_OS_WIN
  // Windows compares folders in any case and with either separator.
  CHECK(sameSignIn({QStringLiteral("C:/Users/Someone/.claude"), QStringLiteral("c:\\users\\someone\\.claude")}, 0) ==
        (QList<int>{1}));
#else
  CHECK(sameSignIn({QStringLiteral("/home/Someone/.claude"), QStringLiteral("/home/someone/.claude")}, 0).isEmpty());
#endif

  // Sign in Again: as Check again, and only with the agent's own commands
  // (not a custom command or an API agent).
  using mitcad::agent::signInAgainOffered;
  CHECK(signInAgainOffered(true, Readiness::Ready, false, true));
  CHECK(signInAgainOffered(true, Readiness::SignedOut, false, true));
  CHECK(!signInAgainOffered(true, Readiness::Ready, false, false));
  CHECK(!signInAgainOffered(false, Readiness::Ready, false, true));
  CHECK(!signInAgainOffered(true, Readiness::Ready, true, true));
  CHECK(!signInAgainOffered(true, Readiness::Missing, false, true));

  using mitcad::agent::ScriptShell;
  using mitcad::agent::commandLines;
  using mitcad::agent::shellQuoted;
  using mitcad::agent::signInScript;
  CHECK(shellQuoted(QStringLiteral("/home/someone/.local/bin/claude"), ScriptShell::Posix) ==
        QStringLiteral("/home/someone/.local/bin/claude"));
  CHECK(shellQuoted(QStringLiteral("/Users/some one/it's/claude"), ScriptShell::Posix) ==
        QStringLiteral("'/Users/some one/it'\\''s/claude'"));
  CHECK(shellQuoted(QString(), ScriptShell::Posix) == QStringLiteral("''"));
  CHECK(shellQuoted(QStringLiteral("C:\\Users\\someone\\.local\\bin\\claude.exe"), ScriptShell::WindowsCmd) ==
        QStringLiteral("C:\\Users\\someone\\.local\\bin\\claude.exe"));
  CHECK(shellQuoted(QStringLiteral("C:\\Program Files\\A&B\\claude.exe"), ScriptShell::WindowsCmd) ==
        QStringLiteral("\"C:\\Program Files\\A&B\\claude.exe\""));
  const QList<QStringList> claude{{QStringLiteral("/opt/my tools/claude"), QStringLiteral("auth"), QStringLiteral("logout")},
                                  {QStringLiteral("/opt/my tools/claude"), QStringLiteral("auth"), QStringLiteral("login")}};
  CHECK(commandLines(claude, ScriptShell::Posix) ==
        QStringLiteral("'/opt/my tools/claude' auth logout\n'/opt/my tools/claude' auth login"));
  // A sh script: each command shown and run in order, then a wait for Enter.
  const QString posix = signInScript(QStringLiteral("Heading"), QStringLiteral("Closing"), claude, ScriptShell::Posix);
  CHECK(posix.startsWith(QStringLiteral("#!/bin/sh\n")));
  const qsizetype logout = posix.indexOf(QStringLiteral("\n'/opt/my tools/claude' auth logout\n"));
  const qsizetype login = posix.indexOf(QStringLiteral("\n'/opt/my tools/claude' auth login\n"));
  CHECK(logout > 0 && login > logout);
  CHECK(posix.contains(QStringLiteral("printf '%s\\n' '$ '\\''/opt/my tools/claude'\\'' auth logout'\n")));
  CHECK(posix.endsWith(QStringLiteral("printf '%s ' Closing\nread answer\nexit 0\n")));
  // A cmd.exe script: CRLF, % doubled where it runs, quotes kept, the
  // characters cmd.exe acts on escaped in echo outside quotes.
  const QList<QStringList> codex{{QStringLiteral("C:\\Users\\100% sure\\codex.exe"), QStringLiteral("logout")},
                                 {QStringLiteral("C:\\Users\\100% sure\\codex.exe"), QStringLiteral("login")}};
  const QString cmd = signInScript(QStringLiteral("Sign out & in"), QStringLiteral("Done"), codex, ScriptShell::WindowsCmd);
  CHECK(cmd.startsWith(QStringLiteral("@echo off\r\nchcp 65001 >nul\r\necho Sign out ^& in\r\n")));
  CHECK(cmd.contains(QStringLiteral("\r\n\"C:\\Users\\100%% sure\\codex.exe\" logout\r\n")));
  CHECK(cmd.contains(QStringLiteral("\r\necho ^> \"C:\\Users\\100%% sure\\codex.exe\" login\r\n")));
  CHECK(cmd.indexOf(QStringLiteral("codex.exe\" logout")) < cmd.indexOf(QStringLiteral("codex.exe\" login")));
  CHECK(cmd.endsWith(QStringLiteral("echo Done\r\n")));

  // The terminals: $TERMINAL first, then the known ones in order.
  using mitcad::agent::TerminalLaunch;
  QStringList installed;
  const mitcad::agent::FindProgram find = [&installed](const QString& name) {
    for (const QString& path : installed) {
      if (path == name || path.endsWith(QLatin1Char('/') + name)) {
        return path;
      }
    }
    return QString();
  };
  const QString script = QStringLiteral("/home/someone/.local/share/MitCad/chat/claude-sign-in-again.sh");
  CHECK(mitcad::agent::linuxTerminal(QString(), find, script).program.isEmpty());
  installed = {QStringLiteral("/usr/bin/xterm"), QStringLiteral("/usr/bin/gnome-terminal")};
  TerminalLaunch onLinux = mitcad::agent::linuxTerminal(QString(), find, script);
  CHECK(onLinux.kind == QStringLiteral("gnome-terminal") && onLinux.program == QStringLiteral("/usr/bin/gnome-terminal"));
  CHECK(onLinux.arguments == (QStringList{QStringLiteral("--wait"), QStringLiteral("--"), script}));
  CHECK(!onLinux.returnsAtOnce);
  installed << QStringLiteral("/usr/bin/x-terminal-emulator") << QStringLiteral("/opt/kitty/kitty");
  onLinux = mitcad::agent::linuxTerminal(QStringLiteral("kitty --single-instance"), find, script);
  CHECK(onLinux.kind == QStringLiteral("$TERMINAL") && onLinux.program == QStringLiteral("/opt/kitty/kitty"));
  CHECK(onLinux.arguments == (QStringList{QStringLiteral("--single-instance"), QStringLiteral("-e"), script}));
  // $TERMINAL not found: the known ones.
  onLinux = mitcad::agent::linuxTerminal(QStringLiteral("no-such-terminal"), find, script);
  CHECK(onLinux.kind == QStringLiteral("x-terminal-emulator"));
  CHECK(onLinux.arguments == (QStringList{QStringLiteral("-e"), script}));
  installed = {QStringLiteral("/usr/bin/xfce4-terminal")};
  onLinux = mitcad::agent::linuxTerminal(QString(), find, script);
  CHECK(onLinux.arguments == (QStringList{QStringLiteral("--disable-server"), QStringLiteral("-x"), script}));

  // Windows: Windows Terminal (it returns at once), else cmd.exe /k in a
  // console of its own; both in the script's folder.
  const QString cmdScript = QStringLiteral("C:/Users/some one/AppData/Roaming/MitCad/chat/claude-sign-in-again.cmd");
  const QString cmdExe = QStringLiteral("C:\\Windows\\system32\\cmd.exe");
  installed.clear();
  TerminalLaunch windows = mitcad::agent::windowsTerminal(find, cmdExe, cmdScript);
  CHECK(windows.kind == QStringLiteral("cmd.exe") && windows.program == cmdExe && windows.newConsole &&
        !windows.returnsAtOnce);
  CHECK(windows.arguments == (QStringList{QStringLiteral("/k"), QStringLiteral("claude-sign-in-again.cmd")}));
  CHECK(windows.workingDirectory == QStringLiteral("C:/Users/some one/AppData/Roaming/MitCad/chat"));
  installed = {QStringLiteral("C:/Users/some one/AppData/Local/Microsoft/WindowsApps/wt")};
  windows = mitcad::agent::windowsTerminal(find, cmdExe, cmdScript);
  CHECK(windows.kind == QStringLiteral("wt.exe") && windows.returnsAtOnce && !windows.newConsole);
  CHECK(windows.arguments == (QStringList{QStringLiteral("-d"), QStringLiteral("C:/Users/some one/AppData/Roaming/MitCad/chat"),
                                          cmdExe, QStringLiteral("/k"), QStringLiteral("claude-sign-in-again.cmd")}));
  CHECK(mitcad::agent::windowsTerminal(find, QString(), cmdScript).arguments.at(2) == QStringLiteral("cmd.exe"));

  // macOS: Terminal.app through open, which returns at once.
  const TerminalLaunch mac = mitcad::agent::macTerminal(QStringLiteral("/Users/someone/chat/claude-sign-in-again.command"));
  CHECK(mac.program == QStringLiteral("/usr/bin/open") && mac.returnsAtOnce);
  CHECK(mac.arguments == (QStringList{QStringLiteral("-a"), QStringLiteral("Terminal"),
                                      QStringLiteral("/Users/someone/chat/claude-sign-in-again.command")}));
  // The UI tests' stand-in: the script as its one argument.
  const TerminalLaunch test = mitcad::agent::testTerminal(QStringLiteral("/work/clients/test-terminal"), script);
  CHECK(test.kind == QStringLiteral("test") && test.arguments == QStringList{script} && !test.returnsAtOnce);

  // One terminal at a time (mitcad#1610): a start for a second agent while
  // the first agent's terminal runs starts the second's (the first is let
  // go), not nothing; the same agent's open terminal is kept.
  using mitcad::agent::SignInStart;
  using mitcad::agent::signInStart;
  const QString claudeId = QStringLiteral("claude");
  const QString codexId = QStringLiteral("codex");
  CHECK(signInStart(false, QString(), claudeId) == SignInStart::Start);
  CHECK(signInStart(false, codexId, claudeId) == SignInStart::Start);
  CHECK(signInStart(false, claudeId, claudeId) == SignInStart::Start);
  CHECK(signInStart(true, claudeId, claudeId) == SignInStart::AlreadyOpen);
  CHECK(signInStart(true, claudeId, codexId) == SignInStart::ReplaceOther);
  CHECK(signInStart(true, codexId, claudeId) == SignInStart::ReplaceOther);
  return failures;
}
