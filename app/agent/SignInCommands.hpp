// SPDX-License-Identifier: MIT
#pragma once

// Sign in Again (mitcad#1585) as plain functions, so that the unit tests
// reach every platform's choices (tests/chat_account_check.cpp): the
// agent's own sign-out and sign-in command lines as people type them, the
// script that runs them in a terminal window, and the terminal that runs
// the script on each platform. SignInTerminal writes and starts them.

#include <algorithm>
#include <functional>
#include <utility>

#include <QChar>
#include <QList>
#include <QProcess>
#include <QString>
#include <QStringList>

namespace mitcad::agent {

// The shell a script is written for: sh on Linux and macOS, cmd.exe on
// Windows.
enum class ScriptShell { Posix, WindowsCmd };

// One argument as the shell reads it: as it is when nothing in it is
// special, else quoted (sh: in single quotes; cmd.exe: in double quotes,
// which a Windows path never contains).
inline QString shellQuoted(const QString& argument, ScriptShell shell) {
  const QString plain = shell == ScriptShell::Posix ? QStringLiteral("_-./:@%+=,") : QStringLiteral("_-./:\\");
  bool safe = !argument.isEmpty();
  for (const QChar c : argument) {
    if (!(c.isLetterOrNumber() && c.unicode() < 128) && !plain.contains(c)) {
      safe = false;
      break;
    }
  }
  if (safe) {
    return argument;
  }
  if (shell == ScriptShell::Posix) {
    QString quoted = argument;
    quoted.replace(QLatin1Char('\''), QStringLiteral("'\\''"));
    return QLatin1Char('\'') + quoted + QLatin1Char('\'');
  }
  return QLatin1Char('"') + argument + QLatin1Char('"');
}

// A command (its program first) as one line people can type.
inline QString commandLine(const QStringList& command, ScriptShell shell) {
  QStringList parts;
  for (const QString& argument : command) {
    parts << shellQuoted(argument, shell);
  }
  return parts.join(QLatin1Char(' '));
}

// The commands, one per line.
inline QString commandLines(const QList<QStringList>& commands, ScriptShell shell) {
  QStringList lines;
  for (const QStringList& command : commands) {
    lines << commandLine(command, shell);
  }
  return lines.join(QLatin1Char('\n'));
}

// A text for cmd.exe's echo: % doubled (a script expands it), and the
// characters cmd.exe acts on escaped where they are outside quotes.
inline QString cmdEchoed(const QString& text) {
  QString out;
  bool quoted = false;
  for (const QChar c : text) {
    if (c == QLatin1Char('"')) {
      quoted = !quoted;
    }
    if (c == QLatin1Char('%')) {
      out += QStringLiteral("%%");
      continue;
    }
    if (!quoted && QStringLiteral("^&|<>").contains(c)) {
      out += QLatin1Char('^');
    }
    out += c;
  }
  return out;
}

// The script the terminal runs: `heading`, then each command shown and run
// (the next one runs whatever the previous one ended with: a sign-out of
// an account already signed out fails harmlessly), then `closing`. A sh
// script waits for Enter so that its window stays open (xterm closes with
// it); a cmd.exe script is run by `cmd.exe /k`, whose window stays open by
// itself. Its lines end in CRLF on Windows.
inline QString signInScript(const QString& heading, const QString& closing, const QList<QStringList>& commands,
                            ScriptShell shell) {
  QStringList lines;
  if (shell == ScriptShell::Posix) {
    lines << QStringLiteral("#!/bin/sh");
    lines << QStringLiteral("printf '%s\\n' ") + shellQuoted(heading, shell);
    for (const QStringList& command : commands) {
      const QString line = commandLine(command, shell);
      lines << QStringLiteral("echo");
      lines << QStringLiteral("printf '%s\\n' ") + shellQuoted(QStringLiteral("$ ") + line, shell);
      lines << line;
    }
    lines << QStringLiteral("echo");
    lines << QStringLiteral("printf '%s ' ") + shellQuoted(closing, shell);
    lines << QStringLiteral("read answer");
    // Its end is the terminal's: not the read's status at an input's end.
    lines << QStringLiteral("exit 0");
    return lines.join(QLatin1Char('\n')) + QLatin1Char('\n');
  }
  lines << QStringLiteral("@echo off");
  lines << QStringLiteral("chcp 65001 >nul");
  lines << QStringLiteral("echo ") + cmdEchoed(heading);
  for (const QStringList& command : commands) {
    const QString line = commandLine(command, shell);
    lines << QStringLiteral("echo.");
    lines << QStringLiteral("echo ^> ") + cmdEchoed(line);
    QString run = line;
    run.replace(QLatin1Char('%'), QStringLiteral("%%"));
    lines << run;
  }
  lines << QStringLiteral("echo.");
  lines << QStringLiteral("echo ") + cmdEchoed(closing);
  return lines.join(QStringLiteral("\r\n")) + QStringLiteral("\r\n");
}

// How a terminal window is started for the script.
struct TerminalLaunch {
  QString kind;    // as logged: $TERMINAL, xterm, wt.exe, cmd.exe, Terminal.app, test
  QString program; // empty: none found
  QStringList arguments;
  QString workingDirectory; // empty: the user's home folder
  bool newConsole = false;  // Windows: cmd.exe in a console window of its own
  bool returnsAtOnce = false; // it hands the window to another process and ends at once
};

// Finds a program on PATH by its name (QStandardPaths::findExecutable;
// an absolute path is taken as it is when it can be run); empty: none.
using FindProgram = std::function<QString(const QString& name)>;

// Linux and the other X11 or Wayland desktops: $TERMINAL (a program, with
// arguments of its own if it has any, given -e and the script), then the
// first one found of x-terminal-emulator, gnome-terminal (--wait, so that
// it ends with the window), konsole, xfce4-terminal (without its server,
// for the same reason) and xterm.
inline TerminalLaunch linuxTerminal(const QString& terminalVariable, const FindProgram& find, const QString& script) {
  const QStringList variable = QProcess::splitCommand(terminalVariable.trimmed());
  if (!variable.isEmpty()) {
    const QString program = find(variable.first());
    if (!program.isEmpty()) {
      return {QStringLiteral("$TERMINAL"), program,
              variable.mid(1) + QStringList{QStringLiteral("-e"), script}, QString(), false, false};
    }
  }
  const QList<std::pair<QString, QStringList>> known{
      {QStringLiteral("x-terminal-emulator"), {QStringLiteral("-e")}},
      {QStringLiteral("gnome-terminal"), {QStringLiteral("--wait"), QStringLiteral("--")}},
      {QStringLiteral("konsole"), {QStringLiteral("-e")}},
      {QStringLiteral("xfce4-terminal"), {QStringLiteral("--disable-server"), QStringLiteral("-x")}},
      {QStringLiteral("xterm"), {QStringLiteral("-e")}},
  };
  for (const auto& [name, before] : known) {
    const QString program = find(name);
    if (!program.isEmpty()) {
      return {name, program, before + QStringList{script}, QString(), false, false};
    }
  }
  return {};
}

// Windows: Windows Terminal (wt.exe, which hands the tab to its own
// process and ends at once) when it is there, else cmd.exe in a console
// window of its own; both run `cmd.exe /k <script>` in the script's
// folder, so that the script's path needs no quoting on the command line.
inline TerminalLaunch windowsTerminal(const FindProgram& find, const QString& comSpec, const QString& script) {
  // Split by either separator (the path is a Windows one on every platform
  // the unit tests run on).
  const qsizetype cut = (std::max)(script.lastIndexOf(QLatin1Char('/')), script.lastIndexOf(QLatin1Char('\\')));
  const QString folder = script.left(cut);
  const QString name = script.mid(cut + 1);
  const QString cmd = comSpec.isEmpty() ? QStringLiteral("cmd.exe") : comSpec;
  const QString wt = find(QStringLiteral("wt"));
  if (!wt.isEmpty()) {
    return {QStringLiteral("wt.exe"), wt, {QStringLiteral("-d"), folder, cmd, QStringLiteral("/k"), name}, folder, false,
            true};
  }
  return {QStringLiteral("cmd.exe"), cmd, {QStringLiteral("/k"), name}, folder, true, false};
}

// macOS: Terminal.app opens the .command script (`open -a Terminal`, which
// ends at once).
inline TerminalLaunch macTerminal(const QString& script) {
  return {QStringLiteral("Terminal.app"), QStringLiteral("/usr/bin/open"),
          {QStringLiteral("-a"), QStringLiteral("Terminal"), script}, QString(), false, true};
}

// The UI tests' stand-in (MITCAD_TEST_TERMINAL): the program given the
// script as its one argument, on every platform; it ends with the script.
inline TerminalLaunch testTerminal(const QString& program, const QString& script) {
  return {QStringLiteral("test"), program, {script}, QString(), false, false};
}

} // namespace mitcad::agent
