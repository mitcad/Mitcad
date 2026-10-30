# SPDX-License-Identifier: MIT
# check-all sources: app/agent app/mcp/ChatGate.hpp app/mcp/LiveServer.cpp app/mcp/AgentIndicator.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/TitleBar.cpp app/tests/chat_test_agent.cpp
# check-all sources: core/mcp/src/chat core/ffi/src/chat.rs tools/cli/mcp.cpp tools/cli/mcp-connect.cpp
"""The chat panel (mitcad#947) through the real UI, the real live MCP server
and bridge, with a scripted agent instead of Claude Code (no AI).

Runs, each an application of its own with private settings, data,
discovery and chat folders:

  floating  not installed: Install Claude Code opens the setup page; the
            agent copied in meanwhile is found without a restart; not
            signed in: Sign in to Claude runs its own sign-in; the privacy
            notice once; a message that adds two boxes in two calls merges
            into one undo step "AI: ..." that one Ctrl+Z takes back; a
            message with a picture of the view; Save As asks (Allow once)
            and saves; an undo past the message's start asks (Deny) and is
            refused; a user's change during a turn keeps the steps
            separate; Stop ends a running turn; the next message goes on;
            calls in a row are one group row (three calls, one failed, a
            text, two calls: two groups) that opens to their rows
  reopen    the saved design opened again shows its conversation, its
            calls grouped as before, and the agent resumes its session
  docked    the Docked layout: installed and signed in, the model chosen
            (Sonnet, given as --model), one message, one undo step
  codex     Claude Code and Codex installed: the switcher shows both,
            Codex chosen starts a new conversation in it and is kept as the
            last used one; through Codex's app server a message adds two
            boxes as one undo step, the model Codex lists is chosen and the
            thread resumed with it, Save As asks, Stop interrupts the turn
  codex-again  the next start: Codex and its model are still chosen
  desktop   only Claude Desktop installed: the panel points to Connect AI
            Assistant

Usage: ui-chat-test.py APP STUB OUTPUT [QT_BIN]
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

INSTALL_PAGE = "Open URL (test, not opened): https://code.claude.com/docs/en/setup"


def install_stub(stub, clients, name="claude"):
    clients.mkdir(exist_ok=True)
    target = clients / (name + ".exe" if os.name == "nt" else name)
    shutil.copy2(stub, target)
    target.chmod(0o755)


def run(app, name, work, out, chrome, script, extra_args=(), on_log=None, timeout=300):
    env = dict(os.environ)
    for variable in ("MITCAD_SETTINGS_DIR", "MITCAD_AUTOSAVE_DIR", "MITCAD_PROJECTS_DIR", "MITCAD_THUMBNAIL_DIR",
                     "MITCAD_CRASH_DIR", "MITCAD_MCP_DISCOVERY_DIR", "MITCAD_CHAT_DIR", "MITCAD_CHAT_AGENT_DIR"):
        env[variable] = str(work / variable.lower())
    env.update(MITCAD_TEST_ASSISTANT_EXECUTABLES=str(work / "clients"), MITCAD_ASSISTANT_TEST_WORK=str(work),
               MITCAD_TEST_CHAT_SAVE_PATH=str(work / "designs" / "chat design.mitcad"),
               MITCAD_TEST_LOG_URLS="1", MITCAD_RESULT_STORE="off", MITCAD_NO_UPDATE_CHECK="1",
               QT_FORCE_STDERR_LOGGING="1", MITCAD_LOG_VOLUMES="1")
    source = work / (name + ".mitcad-ui")
    source.write_text("\n".join(script) + "\n", encoding="utf-8")
    env["MITCAD_TEST_INPUT"] = str(source)
    log_path = out / (name + ".log")
    with log_path.open("wb") as log:
        process = subprocess.Popen([app, "--ui=qt", "--chrome=" + chrome, "--no-recovery",
                                    "--mcp-folder", str(work / "designs"), *extra_args],
                                   env=env, stdout=log, stderr=subprocess.STDOUT)
        deadline = time.monotonic() + timeout
        acted = False
        while process.poll() is None:
            if time.monotonic() > deadline:
                process.kill()
                raise RuntimeError(name + ": timed out; see " + str(log_path))
            if on_log is not None and not acted:
                if on_log(log_path.read_text(encoding="utf-8", errors="replace")):
                    acted = True
            time.sleep(0.2)
    text = log_path.read_text(encoding="utf-8", errors="replace")
    if process.returncode or "TestDriver: done" not in text or \
            any(line.startswith("TestDriver: FAIL") for line in text.splitlines()):
        raise RuntimeError(name + ": the chat UI failed; see " + str(log_path))
    return text


def send(text):
    return ["mark", "focus-widget chatInput", "type '" + text + "'", "key Return"]


def lines(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def main():
    app, stub, output = sys.argv[1:4]
    if len(sys.argv) > 4:
        os.environ["PATH"] = sys.argv[4] + os.pathsep + os.environ.get("PATH", "")
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="chat-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        (work / "clients").mkdir()
        saved = work / "designs" / "chat design.mitcad"

        floating = [
            "wait-idle", "expect-model-bodies 0 0",
            "command view.chat",
            "wait-log 'Chat panel: missing' 60",
            "expect-widget-property chatConnectAssistant visible false",
            "button '^Install Claude Code$'",
            "wait-log 'Open URL \\(test, not opened\\): https://code\\.claude\\.com/docs/en/setup'",
            # The harness copies the agent in now: found without a restart.
            "wait-log 'Chat: Claude Code appeared' 60",
            "wait-log 'Chat panel: signed-out' 60",
            "button '^Sign in to Claude$'",
            "wait-log 'Chat: Claude Code.s sign-in ended' 60",
            "wait-log 'Chat panel: ready' 60",
            "capture-undo-depth start",
            # The first message: the privacy notice once, then two calls of
            # the agent merged into one step.
            *send("a 30 x 20 x 10 mm box"),
            "wait-log 'Chat panel: notice'",
            "button '^Continue$' chatContinue",
            "wait-log 'Chat: Claude Code started \\(new session' 30",
            "wait-log 'Agent indicator: chat working: Claude Code' 30",
            "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a 30 x 20 x 10 mm box.' 120",
            "expect-model-bodies 2 7000",
            "expect-undo-depth ${start} 1",
            "focus-widget chatConversation",
            "key ctrl+z",
            "expect-model-bodies 0 0",
            "expect-undo-depth ${start} 0",
            # A picture of the view goes with the next message.
            "button '^View$' chatAttachView",
            *send("hello with the view"),
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            "button '^View$' chatAttachView",
            # Save As asks; Allow once saves.
            *send("save the design"),
            "wait-log 'Chat: approval asked for document_save ' 60",
            "wait-log 'Chat panel: approval card' 30",
            "button '^Allow once$' chatAllowOnce",
            "wait-log 'MCP: an agent saved' 60",
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            "wait-log 'Chat: the conversation follows the design to ' 30",
            "expect-file '" + str(saved) + "'",
            # An undo past the message's start asks; Deny refuses it.
            *send("undo the last change"),
            "wait-log 'Chat: approval asked for undo_past_start' 60",
            "button '^Deny$' chatDeny",
            "wait-log 'Chat: Deny for undo_past_start'",
            "wait-log 'Chat: tool model_command failed' 60",
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            # The user's own change during a turn: the steps stay the agent's.
            *send("two boxes, slowly"),
            "wait-log 'Chat: tool model_command done' 60",
            "focus-widget chatConversation",
            "key ctrl+z",
            "screenshot '" + str(work / "continue.png") + "'",
            "wait-log 'Chat: the design changed between the agent.s calls' 60",
            "wait-log 'Chat: the message.s steps stay separate \\(user_edited\\)' 120",
            "expect-model-bodies 1 1000",
            # Stop ends a running turn; the next message goes on.
            *send("wait for me"),
            "wait-log 'Chat panel: working' 30",
            "sleep 500",
            "button '^Stop$' chatSend",
            "wait-log 'Chat notice: Stopped\\.' 30",
            "wait-log 'Chat panel: ready' 30",
            *send("hello again"),
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            # Calls in a row are one group row that opens to their rows; the
            # agent's text starts the next group (mitcad#983).
            *send("measure with several calls"),
            "wait-log 'Chat tool group: 3 calls, 1 failed' 60",
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            "button '^3 calls \\(1 failed\\) . document_list$' chatToolGroupHead",
            "wait-log 'Chat tool group opened: 3 calls, 1 failed, 3 rows shown'",
            "button '^2 calls . ui_state$' chatToolGroupHead",
            "wait-log 'Chat tool group opened: 2 calls, 2 rows shown'",
            "button '^model_query . bodies . failed$' chatToolHead",
            "screenshot '" + str(out / "chat-floating.png") + "'",
        ]

        def installed_when_asked(log):
            if INSTALL_PAGE in log:
                install_stub(stub, work / "clients")
                return True
            return False

        run(app, "floating", work, out, "floating", floating, on_log=installed_when_asked)
        starts = lines(work / "chat-starts.jsonl")
        first = starts[0]["argv"]
        session = first[first.index("--session-id") + 1]
        messages = lines(work / "chat-messages.jsonl")
        with_view = [m for m in messages if m["text"] == "hello with the view"]
        if not with_view or with_view[0]["pictures"] != ["image/png"]:
            raise RuntimeError("the picture of the view did not go with the message")
        if any(m["pictures"] for m in messages if m["text"] != "hello with the view"):
            raise RuntimeError("a picture went with a message without View")
        if not any(path.suffix == ".jsonl" for path in (work / "mitcad_chat_dir").iterdir()):
            raise RuntimeError("the saved design has no history file")

        reopen = [
            "wait-idle",
            "expect-ever 'Chat: history of [1-9][0-9]* entries shown, session " + session + " to resume'",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            # The history's calls are grouped as they were (mitcad#983).
            "expect-ever 'Chat tool group: 3 calls, 1 failed'",
            "button '^3 calls \\(1 failed\\) . document_list$' chatToolGroupHead",
            "wait-log 'Chat tool group opened: 3 calls, 1 failed, 3 rows shown'",
            *send("hello after reopening"),
            "wait-log 'Chat: Claude Code started \\(resuming session " + session + "\\)' 30",
            "wait-log 'Chat: turn ended \\(success\\)' 60",
            "screenshot '" + str(out / "chat-reopened.png") + "'",
        ]
        run(app, "reopen", work, out, "floating", reopen, extra_args=(str(saved),))
        resumed = lines(work / "chat-starts.jsonl")[-1]["argv"]
        if "--resume" not in resumed or resumed[resumed.index("--resume") + 1] != session:
            raise RuntimeError("the reopened design did not resume its session")

    with tempfile.TemporaryDirectory(prefix="chat-docked-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        install_stub(stub, work / "clients")
        (work / "claude-signed-in").write_text("", encoding="utf-8")
        docked = [
            "wait-idle",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            # The model: Claude Code's alias, given as --model.
            "expect-widget-property chatModelChoice currentText 'Default model'",
            "qt-choice chatModelChoice Sonnet",
            "wait-log 'Chat: model sonnet for Claude Code' 30",
            "capture-undo-depth start",
            *send("a box in the docked layout"),
            "button '^Continue$' chatContinue",
            "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a box in the docked layout.' 120",
            "expect-model-bodies 2 7000",
            "expect-undo-depth ${start} 1",
            "focus-widget chatConversation",
            "key ctrl+z",
            "expect-model-bodies 0 0",
            "screenshot '" + str(out / "chat-docked.png") + "'",
        ]
        run(app, "docked", work, out, "docked", docked)
        argv = lines(work / "chat-starts.jsonl")[-1]["argv"]
        if "--model" not in argv or argv[argv.index("--model") + 1] != "sonnet":
            raise RuntimeError("the chosen model did not go to Claude Code as --model")

    with tempfile.TemporaryDirectory(prefix="chat-codex-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        install_stub(stub, work / "clients")
        install_stub(stub, work / "clients", "codex")
        (work / "claude-signed-in").write_text("", encoding="utf-8")
        (work / "codex-signed-in").write_text("", encoding="utf-8")
        codex = [
            "wait-idle",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            # Both found: the switcher, Claude Code first.
            "expect-widget-property chatAgentChoice visible true",
            "expect-widget-property chatAgentChoice currentText 'Claude Code'",
            "mark",
            "qt-choice chatAgentChoice Codex",
            "wait-log 'Chat: agent Codex' 30",
            "wait-log 'Chat: new conversation' 30",
            "wait-log 'Chat: Codex 0\\.95\\.0 found, signed in' 60",
            "wait-log 'Chat panel: ready' 60",
            "expect-setting chat/agent codex",
            "capture-undo-depth start",
            *send("a box from codex"),
            "wait-log 'Chat panel: notice'",
            "button '^Continue$' chatContinue",
            "wait-log 'Chat: Codex started \\(new thread\\)' 30",
            "wait-log 'Chat: Codex started its session thr_[0-9a-f]+, model test-codex-model' 30",
            "wait-log 'Chat: Codex offers 2 model\\(s\\)' 30",
            "wait-log 'Chat: MitCad.s tools connected in Codex' 30",
            "wait-log 'Agent indicator: chat working: Codex' 30",
            "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a box from codex.' 120",
            "expect-model-bodies 2 7000",
            "expect-undo-depth ${start} 1",
            "focus-widget chatConversation",
            "key ctrl+z",
            "expect-model-bodies 0 0",
            # A model Codex listed: the next message resumes the thread with it.
            "mark",
            "qt-choice chatModelChoice 'Test large'",
            "wait-log 'Chat: model test-codex-large for Codex' 30",
            "wait-log 'Chat: Codex.s conversation closed' 30",
            *send("hello with the large model"),
            "wait-log 'Chat: Codex started \\(resuming thread thr_[0-9a-f]+\\)' 30",
            "wait-log 'Chat: Codex started its session thr_[0-9a-f]+, model test-codex-large' 30",
            "wait-log 'Chat: turn ended \\(completed\\)' 60",
            "expect-widget-property chatModel text test-codex-large",
            # Save As asks through the same gate; Allow once saves.
            *send("save the design"),
            "wait-log 'Chat: approval asked for document_save ' 60",
            "button '^Allow once$' chatAllowOnce",
            "wait-log 'MCP: an agent saved' 60",
            "wait-log 'Chat: turn ended \\(completed\\)' 60",
            "expect-file '" + str(work / "designs" / "chat design.mitcad") + "'",
            # Stop: turn/interrupt ends the running turn.
            *send("wait for me"),
            "wait-log 'Chat panel: working' 30",
            "sleep 500",
            "button '^Stop$' chatSend",
            "wait-log 'Chat: stopping Codex \\(turn/interrupt\\)' 30",
            "wait-log 'Chat notice: Stopped\\.' 30",
            "wait-log 'Chat panel: ready' 30",
            "expect-no-log 'Chat: Codex did not stop its turn'",
            "screenshot '" + str(out / "chat-codex.png") + "'",
        ]
        run(app, "codex", work, out, "floating", codex)
        starts = lines(work / "chat-starts.jsonl")
        if any(start.get("agent") != "codex" for start in starts):
            raise RuntimeError("Claude Code was started after Codex was chosen")
        threads = [start for start in starts if "method" in start]
        if [start["method"] for start in threads] != ["thread/start", "thread/resume"]:
            raise RuntimeError("Codex's thread was not started and then resumed: "
                               + str([start["method"] for start in threads]))
        if "model" in threads[0]["params"] or threads[1]["params"].get("model") != "test-codex-large":
            raise RuntimeError("the chosen model did not go to Codex's thread")
        servers = threads[0]["params"]["config"]["mcp_servers"]
        if list(servers) != ["mitcad"] or "--chat-session" not in servers["mitcad"]["args"]:
            raise RuntimeError("Codex was not given MitCad's bridge alone")
        messages = lines(work / "chat-messages.jsonl")
        large = [m for m in messages if m["text"] == "hello with the large model"]
        if not large or large[0]["model"] != "test-codex-large":
            raise RuntimeError("the turn did not carry the chosen model")
        if len({m["session"] for m in messages}) != 1:
            raise RuntimeError("Codex's thread changed within the conversation")

        again = [
            "wait-idle",
            "expect-ever 'Chat: agent Codex \\(found: claude, codex\\)'",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            "expect-widget-property chatAgentChoice currentText Codex",
            "expect-widget-property chatModelChoice currentText 'Test large'",
        ]
        run(app, "codex-again", work, out, "floating", again)

    with tempfile.TemporaryDirectory(prefix="chat-desktop-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        (work / "clients").mkdir()
        (work / "clients" / "claude-desktop").write_text("", encoding="utf-8")
        desktop = [
            "wait-idle",
            "command view.chat",
            "wait-log 'Chat panel: missing' 60",
            "expect-widget-property chatConnectAssistant visible true",
            "expect-widget-property chatSetup text 'Install Claude Code'",
        ]
        run(app, "desktop", work, out, "floating", desktop)
    print("PASS: chat panel: setup, approvals, one undo step per message, stop, history and resume, both layouts, "
          "Codex, the agent switcher and the model choice")


if __name__ == "__main__":
    main()
