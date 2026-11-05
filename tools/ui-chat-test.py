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
  new-design  a design from a design (mitcad#1266): the agent reads the
            open design, makes nipple.mitcad beside it with document_new
            (no card, no dialog), builds in it without a handle and reads
            the first design again, which is unchanged
  codex     Claude Code and Codex installed: the switcher shows both,
            Codex chosen starts a new conversation in it and is kept as the
            last used one; through Codex's app server a message adds two
            boxes as one undo step, the model Codex lists is chosen and the
            thread resumed with it, Save As asks, Stop interrupts the turn
  codex-again  the next start: Codex and its model are still chosen
  desktop   only Claude Desktop installed: the panel points to Connect AI
            Assistant
  account   each agent (mitcad#1126): an account failure (a message queued
            meanwhile is not sent and says so), How to fix, Check again
            failing for the account, for the network (the account's reason
            stays) and with the agent's program gone (it ends at once and
            shows the agent missing; found again, Check again is back), then
            passing; New Conversation in a fresh session
  api       the Anthropic API (phase 2), against a scripted service of
            this script's own on localhost: offered in Preferences, chosen
            in the switcher, the API key typed in the panel (kept for the
            run: the tests keep out of the keychain), a message whose two
            calls merge into one undo step (MitCad runs the tool loop: the
            calls go through its own listener, the thinking blocks go back
            with their signatures), Save As asks, Stop ends a streamed
            answer, the next message goes on
  api-reopen  the saved design opened again: its conversation shown, the
            API agent resumes it (the kept conversation goes to the service)
            with ANTHROPIC_API_KEY of the environment
  ollama    a local model through the OpenAI-compatible endpoint: its
            server not running (Get Ollama; found once it answers, without
            a restart), its models listed, one without tool calls reported,
            one with tool calls adds two boxes as one undo step
  acp       a custom command over the Agent Client Protocol (mitcad#1198),
            offered in Preferences and chosen in the switcher: one that does
            not answer the handshake is said in the state line; the scripted
            agent set meanwhile is found without a restart; a message whose
            two calls merge into one undo step (MitCad rejects the agent's
            own edit and refuses its file read, allows its call of MitCad's
            tool, and the live server decides), the agent's model chosen
            (the session loaded again, its replay not shown twice), Save As
            asks, Stop cancels the prompt
  acp-reopen  the saved design opened again: its conversation shown, the
            custom agent loads its session

Usage: ui-chat-test.py APP STUB OUTPUT [QT_BIN]
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time

INSTALL_PAGE = "Open URL (test, not opened): https://code.claude.com/docs/en/setup"


def install_stub(stub, clients, name="claude"):
    clients.mkdir(exist_ok=True)
    target = clients / (name + ".exe" if os.name == "nt" else name)
    shutil.copy2(stub, target)
    target.chmod(0o755)


def run(app, name, work, out, chrome, script, extra_args=(), on_log=None, timeout=300, extra_env=None):
    env = dict(os.environ)
    # The API agents' keys stay out of the user's keychain (phase 2).
    env.pop("ANTHROPIC_API_KEY", None)
    env["MITCAD_TEST_KEYCHAIN"] = "off"
    env.update(extra_env or {})
    for variable in ("MITCAD_SETTINGS_DIR", "MITCAD_AUTOSAVE_DIR", "MITCAD_PROJECTS_DIR", "MITCAD_THUMBNAIL_DIR",
                     "MITCAD_CRASH_DIR", "MITCAD_MCP_DISCOVERY_DIR", "MITCAD_CHAT_DIR", "MITCAD_CHAT_AGENT_DIR"):
        env[variable] = str(work / variable.lower())
    env.update(MITCAD_TEST_ASSISTANT_EXECUTABLES=str(work / "clients"), MITCAD_ASSISTANT_TEST_WORK=str(work),
               MITCAD_TEST_CHAT_SAVE_PATH=str(work / "designs" / "chat design.mitcad"),
               MITCAD_TEST_LOG_URLS="1", MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NO_UPDATE_CHECK="1",
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


API_KEY = "test-key"
BOX_ONE = {"command": {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20,
                                                   "width": 30, "height": 10, "operation": "new_body"}}}
BOX_TWO = {"command": {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0], "length": 10,
                                                   "width": 10, "height": 10, "operation": "new_body"}}}


def free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


class Service(BaseHTTPRequestHandler):
    """A scripted model service (no AI): the Anthropic Messages API under
    /v1/messages, an OpenAI-compatible endpoint under /v1/chat/completions,
    both streamed; each answer is chosen by the user's last message and how
    many rounds of tool results followed it."""

    log_path = None
    save_path = ""

    def log_message(self, *args):
        pass

    def record(self, entry):
        with open(self.log_path, "a", encoding="utf-8") as log:
            log.write(json.dumps(entry) + "\n")

    def reply_json(self, status, value):
        data = json.dumps(value).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def start_stream(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()

    def put(self, text):
        self.wfile.write(text.encode("utf-8"))
        self.wfile.flush()

    def keyed(self):
        return self.headers.get("x-api-key") == API_KEY

    def do_GET(self):
        if self.path.startswith("/v1/models?"):
            if not self.keyed():
                return self.reply_json(401, {"type": "error", "error": {"type": "authentication_error",
                                                                        "message": "invalid x-api-key"}})
            return self.reply_json(200, {"data": [{"id": "claude-test", "display_name": "Claude Test"}],
                                         "has_more": False})
        if self.path == "/v1/models":
            return self.reply_json(200, {"object": "list", "data": [{"id": "tiny", "object": "model"},
                                                                    {"id": "llama-test", "object": "model"}]})
        return self.reply_json(404, {"error": {"message": "not found"}})

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", "0"))) or b"{}")
        if self.path == "/v1/messages":
            return self.anthropic(body)
        if self.path == "/v1/chat/completions":
            return self.openai(body)
        return self.reply_json(404, {"error": {"message": "not found"}})

    @staticmethod
    def texts(content):
        if isinstance(content, str):
            return [content]
        return [part.get("text", "") for part in content or [] if part.get("type") == "text"]

    def anthropic(self, body):
        messages = body.get("messages", [])
        last = max(i for i, m in enumerate(messages) if m["role"] == "user" and self.texts(m["content"]))
        said = [t for t in self.texts(messages[last]["content"]) if not t.startswith(("Selected in", "Open comments"))]
        text = said[-1] if said else ""
        step = sum(1 for m in messages[last + 1:] if m["role"] == "user")
        echoed = any(block.get("type") == "thinking" and block.get("signature") == "sig-test"
                     for m in messages if m["role"] == "assistant" for block in m["content"])
        self.record({"api": "anthropic", "key": self.headers.get("x-api-key"), "model": body.get("model"),
                     "text": text, "step": step, "messages": len(messages), "thinking_echoed": echoed,
                     "tools": [tool["name"] for tool in body.get("tools", [])], "system": body.get("system", "")})
        if not self.keyed():
            return self.reply_json(401, {"type": "error", "error": {"type": "authentication_error",
                                                                    "message": "invalid x-api-key"}})
        if "box" in text and step < 2:
            return self.anthropic_call("toolu_box%d" % step, "model_command", BOX_ONE if step == 0 else BOX_TWO,
                                       "Adding the box." if step == 0 else "")
        if "save" in text and step == 0:
            return self.anthropic_call("toolu_save", "document_save", {"path": self.save_path, "overwrite": True},
                                       "Saving the design.")
        if "wait" in text:
            return self.anthropic_wait()
        reply = {"box": "Two boxes are in the design.", "save": "Saved."}
        return self.anthropic_text(next((answer for word, answer in reply.items() if word in text), "Hello."))

    def event(self, name, data):
        self.put("event: %s\ndata: %s\n\n" % (name, json.dumps(data)))

    def anthropic_call(self, tool_id, name, arguments, text):
        self.start_stream()
        self.event("message_start", {"type": "message_start", "message": {"id": "msg_test", "type": "message",
                                                                           "role": "assistant", "content": []}})
        self.event("content_block_start", {"type": "content_block_start", "index": 0,
                                           "content_block": {"type": "thinking", "thinking": ""}})
        self.event("content_block_delta", {"type": "content_block_delta", "index": 0,
                                           "delta": {"type": "thinking_delta", "thinking": "A call."}})
        self.event("content_block_delta", {"type": "content_block_delta", "index": 0,
                                           "delta": {"type": "signature_delta", "signature": "sig-test"}})
        self.event("content_block_stop", {"type": "content_block_stop", "index": 0})
        index = 1
        if text:
            self.event("content_block_start", {"type": "content_block_start", "index": index,
                                               "content_block": {"type": "text", "text": ""}})
            self.event("content_block_delta", {"type": "content_block_delta", "index": index,
                                               "delta": {"type": "text_delta", "text": text}})
            self.event("content_block_stop", {"type": "content_block_stop", "index": index})
            index += 1
        self.event("content_block_start", {"type": "content_block_start", "index": index,
                                           "content_block": {"type": "tool_use", "id": tool_id, "name": name,
                                                             "input": {}}})
        payload = json.dumps(arguments)
        half = len(payload) // 2
        for piece in (payload[:half], payload[half:]):
            self.event("content_block_delta", {"type": "content_block_delta", "index": index,
                                               "delta": {"type": "input_json_delta", "partial_json": piece}})
        self.event("content_block_stop", {"type": "content_block_stop", "index": index})
        self.event("message_delta", {"type": "message_delta", "delta": {"stop_reason": "tool_use"}})
        self.event("message_stop", {"type": "message_stop"})

    def anthropic_text(self, text):
        self.start_stream()
        self.event("message_start", {"type": "message_start", "message": {"id": "msg_test", "content": []}})
        self.event("content_block_start", {"type": "content_block_start", "index": 0,
                                           "content_block": {"type": "text", "text": ""}})
        self.event("content_block_delta", {"type": "content_block_delta", "index": 0,
                                           "delta": {"type": "text_delta", "text": text}})
        self.event("content_block_stop", {"type": "content_block_stop", "index": 0})
        self.event("message_delta", {"type": "message_delta", "delta": {"stop_reason": "end_turn"}})
        self.event("message_stop", {"type": "message_stop"})

    def anthropic_wait(self):
        # Streams until the client ends the request (Stop), a minute at most.
        self.start_stream()
        try:
            self.event("content_block_start", {"type": "content_block_start", "index": 0,
                                               "content_block": {"type": "text", "text": "Waiting to be stopped."}})
            for _ in range(200):
                self.event("ping", {"type": "ping"})
                time.sleep(0.3)
        except OSError:
            self.record({"api": "anthropic", "stopped": True})

    def openai(self, body):
        messages = body.get("messages", [])
        last = max(i for i, m in enumerate(messages) if m["role"] == "user")
        step = sum(1 for m in messages[last + 1:] if m["role"] == "assistant" and m.get("tool_calls"))
        text = (self.texts(messages[last]["content"]) or [""])[0]
        self.record({"api": "openai", "model": body.get("model"), "text": text, "step": step,
                     "tools": [tool["function"]["name"] for tool in body.get("tools", [])]})
        if body.get("model") == "tiny":
            return self.reply_json(400, {"error": {"message": "registry.ollama.ai/library/tiny:latest does not "
                                                              "support tools", "type": "api_error"}})
        self.start_stream()

        def chunk(delta, finish=None):
            self.put("data: %s\n\n" % json.dumps({"choices": [{"index": 0, "delta": delta,
                                                                 "finish_reason": finish}]}))
        if "box" in text and step < 2:
            payload = json.dumps(BOX_ONE if step == 0 else BOX_TWO)
            half = len(payload) // 2
            chunk({"role": "assistant", "content": "Adding a box."})
            chunk({"tool_calls": [{"index": 0, "id": "call_%d" % step, "type": "function",
                                   "function": {"name": "model_command", "arguments": payload[:half]}}]})
            chunk({"tool_calls": [{"index": 0, "function": {"arguments": payload[half:]}}]}, "tool_calls")
        else:
            chunk({"role": "assistant", "content": "Two boxes are in the design." if "box" in text else "Hello."},
                  "stop")
        self.put("data: [DONE]\n\n")


def serve(port, log_path, save_path):
    handler = type("Scripted", (Service,), {"log_path": log_path, "save_path": save_path})
    server = ThreadingHTTPServer(("127.0.0.1", port), handler)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


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

    # A design from a design (mitcad#1266): the agent reads the open design,
    # makes a new one beside it with document_new and builds in it, with no
    # hand of the user's (no card, no dialog).
    with tempfile.TemporaryDirectory(prefix="chat-new-design-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        install_stub(stub, work / "clients")
        (work / "claude-signed-in").write_text("", encoding="utf-8")
        block = {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 30,
                                               "width": 20, "height": 10, "operation": "new_body"}}
        new_design = [
            "wait-idle",
            "model-command '" + json.dumps(block) + "'",
            "wait-idle",
            "expect-model-bodies 1 6000",
            "capture-document-window first",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            *send("make a new design from this one"),
            "button '^Continue$' chatContinue",
            "wait-log 'MCP: an agent made a new design .*nipple\\.mitcad' 60",
            "wait-log 'Chat: turn ended \\(success\\)' 120",
            "expect-no-log 'Chat: approval asked'",
            "expect-document-windows 2",
            # The new design is the active one, with only the agent's box.
            "capture-document-window second",
            "expect-model-bodies 1 1000",
            "screenshot '" + str(out / "chat-new-design.png") + "'",
            # The design the chat started in is as it was.
            "focus-document-window first",
            "expect-model-bodies 1 6000",
        ]
        run(app, "new-design", work, out, "floating", new_design)
        record = lines(work / "chat-new-design.jsonl")
        if len(record) != 1 or not record[0]["ok"]:
            raise RuntimeError("the agent's design from a design failed: " + repr(record))
        made = work / "designs" / "nipple.mitcad"
        if not made.is_file() or Path(record[0]["path"]).resolve() != made.resolve():
            raise RuntimeError("document_new did not make designs/nipple.mitcad: " + repr(record))
        if record[0]["built_in"] != record[0]["design"] or record[0]["own_bodies_after"] != 1:
            raise RuntimeError("the agent's box went to the design it came from: " + repr(record))

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
    for agent_name, shown_name, error_code, reason in (
            ("claude", "Claude Code", "org-disabled", "your organization has disabled access"),
            ("codex", "Codex", "signed-out", "sign-in is missing or has expired")):
        with tempfile.TemporaryDirectory(prefix="chat-account-" + agent_name + "-", dir=out) as temporary:
            work = Path(temporary)
            (work / "designs").mkdir()
            install_stub(stub, work / "clients", agent_name)
            (work / (agent_name + "-signed-in")).write_text("", encoding="utf-8")
            blocked = work / "account-blocked"
            blocked.write_text("", encoding="utf-8")
            recovered = out / ("chat-account-recovered-" + agent_name + ".png")
            recovered.unlink(missing_ok=True)
            # Signals of the UI script to the harness (screenshots into the
            # work folder) and the harness's answer (mitcad#1155).
            agent_gone = work / "agent-gone.png"
            agent_removed = work / "agent-removed"
            agent_back = work / "agent-back.png"
            state_line = "'" + shown_name + " cannot use this account: " + reason + "'"
            account = [
                "wait-idle", "command view.chat", "wait-log 'Chat panel: ready' 60",
                # The failure waits for account-go.png: a message written
                # meanwhile waits in the queue and is not sent (mitcad#1155).
                *send("account failure, slowly"), "wait-log 'Chat panel: notice'",
                "button '^Continue$' chatContinue",
                "wait-log 'Chat: message [0-9]+ sent' 30",
                *send("queued at the account failure"),
                "wait-log 'Chat: message queued \\(1 waiting\\)' 30",
                "screenshot '" + str(work / "account-go.png") + "'",
                "wait-log 'Chat: " + shown_name + " account error: " + error_code + "' 60",
                "wait-log 'Chat notice \\(warning\\): 1 waiting message\\(s\\) were not sent\\.' 30",
                "wait-log 'Chat: turn ended' 60",
                "expect-widget-property chatPanel chatState account-error",
                "expect-widget-property chatState text " + state_line,
                "expect-widget-property chatSetup text 'How to fix'",
                "expect-widget-property chatInput visible false",
                "button '^How to fix$' chatSetup",
                "expect-widget-property chatAccountHelp visible true",
                "screenshot '" + str(out / ("chat-account-" + agent_name + ".png")) + "'",
                "mark", "button '^Check again$' chatCheckAccount",
                "wait-log 'Chat: " + shown_name + " account check failed:' 60",
                "expect-widget-property chatPanel chatState account-error",
                # A probe that fails for the network: the account's reason
                # stays in the state line, the network error is a notice.
                "screenshot '" + str(work / "network-down.png") + "'",
                "mark", "button '^Check again$' chatCheckAccount",
                "wait-log 'Chat: " + shown_name + " account check failed: .*Connection error' 60",
                "wait-log 'Chat notice \\(warning\\): The account check failed: .*Connection error' 30",
                "expect-widget-property chatPanel chatState account-error",
                "expect-widget-property chatState text " + state_line,
                "expect-widget-property chatCheckAccount enabled true",
                "screenshot '" + str(work / "network-up.png") + "'",
                # The agent's program gone: Check again ends at once instead
                # of staying in Checking; the panel shows the agent missing,
                # Check again hidden but usable again once it is back.
                "screenshot '" + str(agent_gone) + "'",
                "expect-file '" + str(agent_removed) + "' 30",
                "mark", "button '^Check again$' chatCheckAccount",
                "wait-log 'Chat: " + shown_name + " account check failed:' 10",
                "wait-log 'Chat panel: missing' 10",
                "expect-widget-property chatPanel chatState missing",
                "expect-widget-property chatCheckAccount visible false",
                "expect-widget-property chatCheckAccount enabled true",
                "expect-widget-property chatNew enabled true",
                "mark", "screenshot '" + str(agent_back) + "'",
                "wait-log 'Chat: " + shown_name + " appeared' 60",
                "wait-log 'Chat panel: account-error' 60",
                "expect-widget-property chatCheckAccount visible true",
                "expect-widget-property chatState text " + state_line,
                # Change the scripted service's state, then run the same
                # production recheck path again in another fresh process.
                "screenshot '" + str(recovered) + "'", "sleep 500",
                "mark", "button '^Check again$' chatCheckAccount",
                "wait-log 'Chat: " + shown_name + " account check passed' 60",
                "wait-log 'Chat panel: ready' 60",
                "expect-widget-property chatAccountHelp visible false",
                "expect-model-bodies 0 0",
                *send("hello after account check"), "wait-log 'Chat: turn ended' 60",
                "mark", "button '^New Conversation$' chatNew",
                *send("hello with fresh sign-in"), "wait-log 'Chat: turn ended' 60",
            ]
            program = work / "clients" / (agent_name + ".exe" if os.name == "nt" else agent_name)

            def account_harness(_text):
                if agent_gone.exists() and not agent_removed.exists():
                    try:
                        program.unlink(missing_ok=True)
                    except OSError:
                        return False  # still in use (Windows): the next poll
                    agent_removed.write_text("", encoding="utf-8")
                if agent_back.exists() and not program.exists():
                    install_stub(stub, work / "clients", agent_name)
                if recovered.exists():
                    blocked.unlink(missing_ok=True)
                    return True
                return False
            text = run(app, "account-" + agent_name, work, out, "floating", account, on_log=account_harness)
            if text.count("Chat notice (error):") != 1:
                raise RuntimeError(agent_name + ": the account error must appear once")
            messages = lines(work / "chat-messages.jsonl")
            if any(message["text"] == "queued at the account failure" for message in messages):
                raise RuntimeError(agent_name + ": a message queued at the account failure was sent")
            probes = [message for message in messages if message["text"].startswith("Reply with OK only.")]
            if len(probes) != 3 or any(probe["context"] or probe["pictures"] for probe in probes):
                raise RuntimeError(agent_name + ": account probe must have no design context or pictures")
            sessions = {messages[0]["session"], messages[-1]["session"], *(probe["session"] for probe in probes)}
            if len(sessions) != 5:
                raise RuntimeError(agent_name + ": the probe and New Conversation need fresh sessions")

    api_runs(app, out)
    acp_runs(app, stub, out)
    print("PASS: chat panel: setup, approvals, one undo step per message, stop, history and resume, both layouts, "
          "Codex, the agent switcher and the model choice, the Anthropic API and a local model (phase 2), "
          "a custom command over the Agent Client Protocol")


def api_runs(app, out):
    """Phase 2: the API agents against this script's scripted service."""
    with tempfile.TemporaryDirectory(prefix="chat-api-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        (work / "clients").mkdir()
        saved = work / "designs" / "chat design.mitcad"
        requests = work / "api-requests.jsonl"
        port = free_port()
        server = serve(port, requests, str(saved))
        try:
            api = [
                "wait-idle", "expect-model-bodies 0 0",
                "set-setting chat/api/anthropic/enabled true",
                "set-setting chat/api/anthropic/url http://127.0.0.1:%d" % port,
                "command view.chat",
                "wait-log 'Chat panel: missing' 60",
                # Offered: the switcher shows it beside Claude Code.
                "expect-widget-property chatAgentChoice visible true",
                "mark",
                "qt-choice chatAgentChoice 'Anthropic API'",
                "wait-log 'Chat: agent Anthropic API' 30",
                "wait-log 'Chat: Anthropic API has no API key' 60",
                "wait-log 'Chat panel: signed-out' 60",
                "expect-widget-property chatApiKey visible true",
                "expect-widget-property chatSetup text 'Save Key'",
                "focus-widget chatApiKey",
                "type '" + API_KEY + "'",
                "key Return",
                "wait-log 'Chat: an API key was given for Anthropic API' 30",
                "wait-log 'Chat: Anthropic API answers at http://127\\.0\\.0\\.1:%d' 60" % port,
                "wait-log 'Chat panel: ready' 60",
                "expect-widget-property chatApiKey visible false",
                "capture-undo-depth start",
                *send("a box from the api"),
                "wait-log 'Chat panel: notice'",
                "button '^Continue$' chatContinue",
                "wait-log 'Chat: Anthropic API started \\(new session' 30",
                "wait-log 'Chat: Anthropic API started its session' 30",
                "wait-log 'Agent indicator: chat working: Anthropic API' 30",
                "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a box from the api.' 120",
                "expect-model-bodies 2 7000",
                "expect-undo-depth ${start} 1",
                "focus-widget chatConversation",
                "key ctrl+z",
                "expect-model-bodies 0 0",
                # Save As asks through the listener's gate.
                *send("save the design"),
                "wait-log 'Chat: approval asked for document_save ' 60",
                "button '^Allow once$' chatAllowOnce",
                "wait-log 'MCP: an agent saved' 60",
                "wait-log 'Chat: turn ended \\(success\\)' 60",
                "expect-file '" + str(saved) + "'",
                # Stop ends the streamed answer; the next message goes on.
                *send("wait for me"),
                "wait-log 'Chat panel: working' 30",
                "sleep 800",
                "button '^Stop$' chatSend",
                "wait-log 'Chat: stopping Anthropic API' 30",
                "wait-log 'Chat notice: Stopped\\.' 30",
                "wait-log 'Chat panel: ready' 30",
                *send("hello again"),
                "wait-log 'Chat: turn ended \\(success\\)' 60",
                "screenshot '" + str(out / "chat-api.png") + "'",
            ]
            text = run(app, "api", work, out, "floating", api)
            session = re.search(r"Chat: Anthropic API started \(new session ([0-9a-f-]+)\)", text).group(1)
            sent = lines(requests)
            anthropic = [entry for entry in sent if entry.get("api") == "anthropic" and "key" in entry]
            if not anthropic or any(entry["key"] != API_KEY for entry in anthropic):
                raise RuntimeError("a request went to the Anthropic API without the key typed in the panel")
            if any(entry["model"] != "claude-opus-5-5" for entry in anthropic):
                raise RuntimeError("the Anthropic API was not asked with its default model")
            if not any(entry["thinking_echoed"] for entry in anthropic):
                raise RuntimeError("the thinking blocks did not go back with their signatures")
            if "model_command" not in anthropic[0]["tools"] or "MitCad" not in anthropic[0]["system"]:
                raise RuntimeError("the model was not given MitCad's live tool catalog and instructions")
            if not any(entry.get("stopped") for entry in sent):
                raise RuntimeError("Stop did not end the streamed answer")
            if not list((work / "mitcad_chat_dir" / "api-sessions").glob(session + ".json")):
                raise RuntimeError("the API agent's conversation was not kept for resuming")

            reopen = [
                "wait-idle",
                "expect-ever 'Chat: history of [1-9][0-9]* entries shown, session " + session + " to resume'",
                "command view.chat",
                "wait-log 'Chat: Anthropic API uses ANTHROPIC_API_KEY|Chat panel: ready' 60",
                "wait-log 'Chat panel: ready' 60",
                *send("hello after reopening"),
                "wait-log 'Chat: Anthropic API started \\(resuming session " + session + "\\)' 30",
                "wait-log 'Chat: turn ended \\(success\\)' 60",
            ]
            run(app, "api-reopen", work, out, "floating", reopen, extra_args=(str(saved),),
                extra_env={"ANTHROPIC_API_KEY": API_KEY})
            resumed = [entry for entry in lines(requests) if entry.get("text") == "hello after reopening"]
            if not resumed or resumed[-1]["messages"] < 3:
                raise RuntimeError("the reopened design's conversation did not go to the service")
        finally:
            server.shutdown()

    with tempfile.TemporaryDirectory(prefix="chat-ollama-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        (work / "clients").mkdir()
        requests = work / "api-requests.jsonl"
        port = free_port()
        servers = []
        ollama = [
            "wait-idle",
            "set-setting chat/api/ollama/enabled true",
            "set-setting chat/api/ollama/url http://localhost:%d/v1" % port,
            "command view.chat",
            "wait-log 'Chat panel: missing' 60",
            "mark",
            "qt-choice chatAgentChoice Ollama",
            "wait-log 'Chat: agent Ollama' 30",
            # Its server is not running: Get Ollama; found once it answers.
            "wait-log 'Chat: no answer from Ollama at' 60",
            "wait-log 'Chat panel: missing' 30",
            "expect-widget-property chatSetup text 'Get Ollama'",
            "button '^Get Ollama$' chatSetup",
            "wait-log 'Open URL \\(test, not opened\\): https://ollama\\.com/download' 30",
            "wait-log 'Chat: Ollama offers 2 model\\(s\\)' 60",
            "wait-log 'Chat panel: ready' 60",
            # A model without tool calls is reported.
            "qt-choice chatModelChoice tiny",
            "wait-log 'Chat: model tiny for Ollama' 30",
            *send("a box locally"),
            "wait-log 'Chat panel: notice'",
            "button '^Continue$' chatContinue",
            "wait-log 'Chat notice \\(error\\): This model cannot edit the design' 60",
            "wait-log 'Chat panel: ready' 30",
            # One with tool calls edits the design: one undo step.
            "qt-choice chatModelChoice llama-test",
            "wait-log 'Chat: model llama-test for Ollama' 30",
            "capture-undo-depth start",
            *send("a box locally"),
            "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a box locally.' 120",
            "expect-model-bodies 2 7000",
            "expect-undo-depth ${start} 1",
            "screenshot '" + str(out / "chat-ollama.png") + "'",
        ]

        def server_when_missing(log):
            if "Chat: no answer from Ollama at" in log and "Open URL (test, not opened): https://ollama.com" in log:
                servers.append(serve(port, requests, ""))
                return True
            return False
        try:
            run(app, "ollama", work, out, "floating", ollama, on_log=server_when_missing)
        finally:
            for server in servers:
                server.shutdown()
        sent = lines(requests)
        if [entry["model"] for entry in sent if entry.get("api") == "openai"][:1] != ["tiny"]:
            raise RuntimeError("the model chosen in the panel did not go to the local server")
        if not any(entry.get("step") == 2 and entry["model"] == "llama-test" for entry in sent):
            raise RuntimeError("the local model's tool loop did not run two rounds")


def acp_runs(app, stub, out):
    """A custom command over the Agent Client Protocol (mitcad#1198)."""
    with tempfile.TemporaryDirectory(prefix="chat-acp-", dir=out) as temporary:
        work = Path(temporary)
        (work / "designs").mkdir()
        install_stub(stub, work / "clients", "not-acp")
        install_stub(stub, work / "clients", "acp-test-agent")
        saved = work / "designs" / "chat design.mitcad"
        acp = [
            "wait-idle", "expect-model-bodies 0 0",
            "set-setting chat/acp/enabled true",
            "set-setting chat/acp/command not-acp",
            "command view.chat",
            "wait-log 'Chat panel: missing' 60",
            # Offered: the switcher shows it beside Claude Code (not found).
            "expect-widget-property chatAgentChoice visible true",
            "mark",
            "qt-choice chatAgentChoice 'Custom command'",
            "wait-log 'Chat: agent Custom command' 30",
            # A command that does not answer the handshake: the state line.
            "wait-log 'Chat: the custom command .*not-acp.* did not answer the Agent Client Protocol handshake' 60",
            "wait-log 'Chat panel: missing' 30",
            "expect-widget-property chatPanel chatState missing",
            "expect-widget-property chatState text 'not-acp did not answer the Agent Client Protocol handshake: "
            "it wrote “This program does not speak the Agent Client Protocol.” instead.'",
            "expect-widget-property chatSetup text 'Chat Preferences…'",
            "screenshot '" + str(out / "chat-acp-handshake.png") + "'",
            # The agent's own command set meanwhile: found without a restart.
            "mark",
            "set-setting chat/acp/arguments --acp-test",
            "set-setting chat/acp/command acp-test-agent",
            "wait-log 'Chat: Custom command appeared' 30",
            "wait-log 'Chat: the custom command .*acp-test-agent.* answered the Agent Client Protocol handshake "
            "\\(ACP test agent 1\\.0\\.0\\)' 60",
            "wait-log 'Chat panel: ready' 60",
            "expect-setting chat/agent acp",
            "capture-undo-depth start",
            *send("a box from the custom agent"),
            "wait-log 'Chat panel: notice'",
            "button '^Continue$' chatContinue",
            "wait-log 'Chat: Custom command started \\(new session\\)' 30",
            "wait-log 'Chat: Custom command started its session sess_[0-9a-f]+, model test-acp-model' 30",
            "wait-log 'Chat: the custom command offers 2 model\\(s\\)' 30",
            "wait-log 'Agent indicator: chat working: Custom command' 30",
            "wait-log 'Chat: the message.s 2 step\\(s\\) merged into .AI: a box from the custom agent.' 120",
            "expect-model-bodies 2 7000",
            "expect-undo-depth ${start} 1",
            "focus-widget chatConversation",
            "key ctrl+z",
            "expect-model-bodies 0 0",
            # The agent's model: the next message loads the session again
            # with it.
            "mark",
            "qt-choice chatModelChoice 'Test ACP large'",
            "wait-log 'Chat: model test-acp-large for Custom command' 30",
            "wait-log 'Chat: the custom command.s conversation closed' 30",
            *send("hello with the large model"),
            "wait-log 'Chat: Custom command started \\(resuming session sess_[0-9a-f]+\\)' 30",
            "wait-log 'Chat: Custom command started its session sess_[0-9a-f]+, model test-acp-large' 30",
            "wait-log 'Chat: turn ended \\(end_turn\\)' 60",
            "expect-widget-property chatModel text test-acp-large",
            # Save As asks through the same gate; Allow once saves.
            *send("save the design"),
            "wait-log 'Chat: approval asked for document_save ' 60",
            "button '^Allow once$' chatAllowOnce",
            "wait-log 'MCP: an agent saved' 60",
            "wait-log 'Chat: turn ended \\(end_turn\\)' 60",
            "expect-file '" + str(saved) + "'",
            # Stop: session/cancel ends the running prompt.
            *send("wait for me"),
            "wait-log 'Chat panel: working' 30",
            "sleep 500",
            "button '^Stop$' chatSend",
            "wait-log 'Chat: stopping the custom command \\(session/cancel\\)' 30",
            "wait-log 'Chat notice: Stopped\\.' 30",
            "wait-log 'Chat panel: ready' 30",
            "expect-no-log 'Chat: the custom command did not stop its prompt'",
            "screenshot '" + str(out / "chat-acp.png") + "'",
        ]
        text = run(app, "acp", work, out, "floating", acp)
        session = re.search(r"Chat: Custom command started its session (sess_[0-9a-f]+), model test-acp-model",
                            text).group(1)
        starts = [start for start in lines(work / "chat-starts.jsonl") if start.get("agent") == "acp"]
        argvs = [start["argv"] for start in starts if "argv" in start]
        if not argvs or any(argv != ["--acp-test"] for argv in argvs):
            raise RuntimeError("the custom command was not started with its arguments: " + str(argvs))
        methods = [start["method"] for start in starts if "method" in start]
        if methods != ["session/new", "session/load", "session/set_config_option"]:
            raise RuntimeError("the custom agent's session was not started, loaded and given its model: "
                               + str(methods))
        sessions = [start for start in starts if start.get("method") in ("session/new", "session/load")]
        servers = sessions[0]["params"]["mcpServers"]
        if len(servers) != 1 or servers[0]["name"] != "mitcad" or "--chat-session" not in servers[0]["args"]:
            raise RuntimeError("the custom agent was not given MitCad's bridge alone")
        if sessions[1]["params"]["sessionId"] != session:
            raise RuntimeError("the custom agent's session was not loaded again")
        model = [start for start in starts if start.get("method") == "session/set_config_option"][0]["params"]
        if model != {"sessionId": session, "configId": "model", "value": "test-acp-large"}:
            raise RuntimeError("the chosen model did not go to the agent's model selector: " + str(model))
        checks = lines(work / "acp-checks.jsonl")
        own = [check for check in checks if check.get("title") == "Write notes.txt"]
        if not own or own[0]["option"] != "opt_reject_once":
            raise RuntimeError("the agent's own edit was not rejected: " + str(own))
        tool = [check for check in checks if check.get("title") == "mcp__mitcad__model_command"]
        if not tool or tool[0]["option"] != "opt_allow_once":
            raise RuntimeError("the agent's call of MitCad's tool was not allowed once: " + str(tool))
        read = [check for check in checks if check["check"] == "fs/read_text_file"]
        if not read or read[0]["error"] != -32601:
            raise RuntimeError("the agent's file read through MitCad was not refused")
        messages = [message for message in lines(work / "chat-messages.jsonl") if message.get("agent") == "acp"]
        large = [message for message in messages if message["text"] == "hello with the large model"]
        if not large or large[0]["model"] != "test-acp-large":
            raise RuntimeError("the prompt did not run with the chosen model")
        if len({message["session"] for message in messages}) != 1:
            raise RuntimeError("the custom agent's session changed within the conversation")
        history = "".join(path.read_text(encoding="utf-8") for path in (work / "mitcad_chat_dir").glob("*.jsonl"))
        if not history or "replayed" in history:
            raise RuntimeError("the loaded session's replay was shown again (or no history was kept)")

        reopen = [
            "wait-idle",
            "expect-ever 'Chat: history of [1-9][0-9]* entries shown, session " + session + " to resume'",
            "command view.chat",
            "wait-log 'Chat panel: ready' 60",
            "expect-widget-property chatAgentChoice currentText 'Custom command'",
            *send("hello after reopening"),
            "wait-log 'Chat: Custom command started \\(resuming session " + session + "\\)' 30",
            "wait-log 'Chat: turn ended \\(end_turn\\)' 60",
            "screenshot '" + str(out / "chat-acp-reopened.png") + "'",
        ]
        run(app, "acp-reopen", work, out, "floating", reopen, extra_args=(str(saved),))
        loads = [start for start in lines(work / "chat-starts.jsonl") if start.get("method") == "session/load"]
        if not loads or loads[-1]["params"]["sessionId"] != session:
            raise RuntimeError("the reopened design did not load the custom agent's session")
        history = "".join(path.read_text(encoding="utf-8") for path in (work / "mitcad_chat_dir").glob("*.jsonl"))
        if "replayed" in history:
            raise RuntimeError("the loaded session's replay was shown again after reopening")


if __name__ == "__main__":
    main()
