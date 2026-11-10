# SPDX-License-Identifier: MIT
# check-all sources: app/agent app/tests/chat_test_agent.cpp tools/ui-chat-test.py app/framework/TestDriverWindowFit.cpp
"""Sign in Again in the chat panel (mitcad#1585) through the real UI, with
the scripted test agent (app/tests/chat_test_agent.cpp) in place of Claude
Code and Codex and a copy of it as the terminal (no AI, no real terminal).

For each agent, an application of its own with private settings, data,
discovery and chat folders:

  claude, codex  ready: no Sign in Again; an account error: Sign in Again
            beside Check again, offered with the stand-in terminal; its
            confirmation names the commands (and for Claude Code that
            signing out also signs out Claude Desktop) and that the same
            blocked account is not fixed by signing in again; Cancel hides
            it; confirmed, the terminal starts the script that runs the
            chat's own executable with `auth logout`, then `auth login`
            (Codex: `logout`, `login`), the button waits while it is open,
            and when it ends Check again runs by itself and passes; the
            confirmation's text and its buttons are shown whole (the text
            scrolls in a short panel, mitcad#1590) in the new window
            (1280 x 800 where the screen has the room) and in a 1280 x 600
            one (or as short as the window's minimum allows: the Docked
            layout's grows with the chat dock's); Claude Code in the
            Floating and the Docked layout, Codex in the Floating one
  no-terminal  (Linux) no terminal on PATH and no $TERMINAL: the button is
            offered but disabled, How to fix shows the commands, Copy copies
            them

The stand-in terminal is given as $TERMINAL on Linux (MitCad's own Linux
choice runs it with -e and the script) and as MITCAD_TEST_TERMINAL on
Windows and macOS (Windows Terminal, cmd.exe and Terminal.app are not
started in a test).

Usage: ui-chat-sign-in-again-test.py APP STUB OUTPUT [QT_BIN]
"""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile

HERE = Path(__file__).resolve().parent
_spec = importlib.util.spec_from_file_location("ui_chat_test", HERE / "ui-chat-test.py")
chat = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(chat)

LINUX = sys.platform.startswith("linux")
WINDOWS = os.name == "nt"
# The characters MitCad shows without quoting (agent/SignInCommands.hpp).
PLAIN = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789" +
            ("_-./:\\" if WINDOWS else "_-./:@%+=,"))


def same_path(a, b):
    return os.path.normcase(os.path.realpath(a)) == os.path.normcase(os.path.realpath(b))


def stand_in_terminal(stub, clients):
    target = clients / ("test-terminal.exe" if WINDOWS else "test-terminal")
    shutil.copy2(stub, target)
    target.chmod(0o755)
    return target


def terminal_env(terminal):
    if LINUX:
        return {"TERMINAL": str(terminal), "MITCAD_TEST_TERMINAL": ""}
    return {"MITCAD_TEST_TERMINAL": str(terminal)}


def account_failure(shown_name, error_code):
    return [
        "wait-idle", "command view.chat", "wait-log 'Chat panel: ready' 60",
        # Not for an agent without an account error.
        "expect-widget-property chatSignInAgain visible false",
        *chat.send("account failure"), "wait-log 'Chat panel: notice'",
        "button '^Continue$' chatContinue",
        "wait-log 'Chat: " + shown_name + " account error: " + error_code + "' 60",
        "wait-log 'Chat: turn ended' 60",
        "wait-log 'Chat panel: account-error' 30",
        "expect-widget-property chatPanel chatState account-error",
        "expect-widget-property chatCheckAccount visible true",
        "expect-widget-property chatSignInAgain visible true",
    ]


def whole_confirmation():
    """The confirmation's text shown whole or scrollable to its end, its
    buttons whole (mitcad#1590)."""
    return ["expect-widget-whole " + name for name in
            ("chatSignInAgainText", "chatSignInAgainCancel", "chatSignInAgainStart")]


def sign_in_runs(app, stub, out):
    for agent_name, shown_name, error_code, logout, login, desktop, chrome in (
            ("claude", "Claude Code", "org-disabled", ["auth", "logout"], ["auth", "login"], True, "floating"),
            ("claude", "Claude Code", "org-disabled", ["auth", "logout"], ["auth", "login"], True, "docked"),
            ("codex", "Codex", "signed-out", ["logout"], ["login"], False, "floating")):
        label = agent_name if chrome == "floating" else agent_name + "-" + chrome
        with tempfile.TemporaryDirectory(prefix="chat-sign-in-" + label + "-", dir=out) as temporary:
            work = Path(temporary)
            (work / "designs").mkdir()
            chat.install_stub(stub, work / "clients", agent_name)
            terminal = stand_in_terminal(stub, work / "clients")
            (work / (agent_name + "-signed-in")).write_text("", encoding="utf-8")
            (work / "account-blocked").write_text("", encoding="utf-8")
            program = work / "clients" / (agent_name + ".exe" if WINDOWS else agent_name)
            kind = "\\$TERMINAL" if LINUX else "test"
            script = [
                *account_failure(shown_name, error_code),
                "wait-log 'Chat panel: Sign in Again offered \\(terminal " + kind + "\\)' 30",
                "expect-widget-property chatSignInAgain enabled true",
                # The confirmation; Cancel hides it and starts nothing.
                "button '^Sign in Again$' chatSignInAgain",
                "expect-widget-property chatSignInAgainConfirm visible true",
                "wait-log 'Chat: Sign in Again asked: ' 10",
                # Its whole text readable (or scrollable to its end) and its
                # buttons visible (mitcad#1590), at the new window's size and
                # in a shorter window.
                *whole_confirmation(),
                "screenshot '" + str(out / ("chat-sign-in-again-" + label + ".png")) + "'",
                "fit-main-frame 1280 600 least",
                "expect-main-frame 1280 600 least",
                *whole_confirmation(),
                "screenshot '" + str(out / ("chat-sign-in-again-" + label + "-short.png")) + "'",
                "button '^Cancel$' chatSignInAgainCancel",
                "wait-log 'Chat: Sign in Again cancelled' 10",
                "expect-widget-property chatSignInAgainConfirm visible false",
                # Confirmed: the terminal runs the agent's own commands, and
                # Check again follows its end.
                "button '^Sign in Again$' chatSignInAgain",
                "mark", "button '^Sign in Again$' chatSignInAgainStart",
                "wait-log 'Chat: Sign in Again: terminal " + kind + " at ' 10",
                "wait-log 'Chat: Sign in Again: started ' 10",
                "wait-log 'Chat panel: signing-in-again' 10",
                "expect-widget-property chatSignInAgain enabled false",
                "wait-log 'Chat: Sign in Again: the terminal ended \\(exit 0\\)' 60",
                "wait-log 'Chat: Sign in Again ended; Check again for " + shown_name + "' 10",
                "wait-log 'Chat: checking " + shown_name + " account' 10",
                "wait-log 'Chat: " + shown_name + " account check passed' 60",
                "wait-log 'Chat panel: ready' 60",
                "expect-widget-property chatSignInAgain visible false",
                "expect-widget-property chatSignInAgainConfirm visible false",
            ]
            text = chat.run(app, "sign-in-" + label, work, out, chrome, script,
                            extra_env=terminal_env(terminal))
            # The confirmation: the commands, the shared sign-in, the
            # blocked account.
            asked = [line for line in text.splitlines() if "Chat: Sign in Again asked: " in line]
            if not asked or "does not fix it" not in asked[0] or program.name not in asked[0] or \
                    " ".join([""] + logout) not in asked[0] or " ".join([""] + login) not in asked[0]:
                raise RuntimeError(agent_name + ": the confirmation did not name the commands and the blocked "
                                   "account: " + repr(asked))
            if desktop and "signs out Claude Desktop" not in asked[0]:
                raise RuntimeError(agent_name + ": the confirmation did not say Claude Desktop is signed out too")
            # The logged command line: the chat's own executable.
            started = [line for line in text.splitlines() if "Chat: Sign in Again: started " in line]
            if not started:
                raise RuntimeError(agent_name + ": the terminal's start was not logged")
            if all(c in PLAIN for c in str(program)):
                runs = " ".join([str(program)] + logout) + " | " + " ".join([str(program)] + login)
                if not started[0].endswith("; it runs " + runs):
                    raise RuntimeError(agent_name + ": the logged commands are not the chat's executable's: " +
                                       started[0])
            # The terminal got the script, which ran the stand-in's
            # sign-out and sign-in in that order.
            starts = chat.lines(work / "terminal-starts.jsonl")
            extension = ".cmd" if WINDOWS else ".command" if sys.platform == "darwin" else ".sh"
            expected_script = work / "mitcad_chat_dir" / (agent_name + "-sign-in-again" + extension)
            if len(starts) != 1 or not same_path(starts[0]["argv"][-1], expected_script) or \
                    (LINUX and starts[0]["argv"][:-1] != ["-e"]):
                raise RuntimeError(agent_name + ": the terminal was not given the script: " + repr(starts))
            commands = [entry for entry in chat.lines(work / "sign-in-commands.jsonl")
                        if entry["argv"] in (logout, login)]
            if [entry["argv"] for entry in commands] != [logout, login] or \
                    not all(same_path(entry["program"], program) for entry in commands):
                raise RuntimeError(agent_name + ": the terminal did not run the chat's executable's sign-out, "
                                   "then sign-in: " + repr(commands))
            if (work / "account-blocked").exists():
                raise RuntimeError(agent_name + ": the stand-in's sign-out did not run")


def no_terminal_run(app, stub, out):
    """Linux: no terminal found. The button is offered disabled; How to fix
    shows the commands and Copy copies them."""
    with tempfile.TemporaryDirectory(prefix="chat-sign-in-none-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        (work / "empty-path").mkdir()
        chat.install_stub(stub, work / "clients", "claude")
        (work / "claude-signed-in").write_text("", encoding="utf-8")
        (work / "account-blocked").write_text("", encoding="utf-8")
        program = work / "clients" / "claude"
        script = [
            *account_failure("Claude Code", "org-disabled"),
            "wait-log 'Chat panel: Sign in Again offered, no terminal found' 30",
            "expect-widget-property chatSignInAgain enabled false",
            "button '^How to fix$' chatSetup",
            "expect-widget-property chatAccountHelp visible true",
            "expect-widget-property chatSignInCommandsRow visible true",
            "button '^Copy$' chatSignInCommandsCopy",
            "wait-log 'Chat: sign-in commands copied: ' 10",
            "screenshot '" + str(out / "chat-sign-in-again-no-terminal.png") + "'",
        ]
        text = chat.run(app, "sign-in-none", work, out, "floating", script,
                        extra_env={"TERMINAL": "", "MITCAD_TEST_TERMINAL": "", "PATH": str(work / "empty-path")})
        copied = [line for line in text.splitlines() if "Chat: sign-in commands copied: " in line]
        expected = str(program) + " auth logout | " + str(program) + " auth login"
        if all(c in PLAIN for c in str(program)) and (not copied or not copied[0].endswith(expected)):
            raise RuntimeError("the copied commands are not the chat's executable's: " + repr(copied))
        if "Chat: Sign in Again: started" in text:
            raise RuntimeError("a terminal started although none was found")


def main():
    app, stub, output = sys.argv[1:4]
    if len(sys.argv) > 4:
        os.environ["PATH"] = sys.argv[4] + os.pathsep + os.environ.get("PATH", "")
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    sign_in_runs(app, stub, out)
    if LINUX:
        no_terminal_run(app, stub, out)
    print("PASS: chat panel: Sign in Again for Claude Code and Codex (confirmation shown whole in both layouts, "
          "the agent's own sign-out and sign-in in the terminal, Check again at its end)" + (", without a terminal" if LINUX else ""))


if __name__ == "__main__":
    main()
