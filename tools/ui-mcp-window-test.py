# SPDX-License-Identifier: MIT
# check-all sources: app/mcp app/framework/TestDriverAssistant.cpp app/framework/TitleBar.cpp tools/cli
"""Two actual GUI processes: activation-based MCP routing and window labels.

Run only in the approved UI environment (Xvfb or a platform VM):
python3 ui-mcp-window-test.py <app> <cli> <output>
The shared assistant-setup harness runs this on Linux, Windows and macOS.
"""
import importlib.util
import json
import os
import re
import socket
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def private_record(path):
    if os.name != "nt":
        path.chmod(0o600)
        return
    user = subprocess.run(["whoami"], capture_output=True, text=True, check=True).stdout.strip()
    subprocess.run(["icacls", str(path), "/setowner", user], capture_output=True, check=True)
    subprocess.run(["icacls", str(path), "/inheritance:r", "/grant:r", user + ":F",
                    "/grant:r", "*S-1-5-18:F"], capture_output=True, check=True)


def remove(path, seconds=10.0):
    # On Windows a file the application is still reading cannot be removed
    # (WinError 32); it lets go within moments, so try again until then.
    deadline = time.monotonic() + seconds
    while True:
        try:
            path.unlink()
            return
        except PermissionError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.1)


def run(app, cli, output, binary_path=None):
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    spec = importlib.util.spec_from_file_location("mcp_live_windows", Path(__file__).with_name("ui-mcp-live.py"))
    live = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(live)
    guis, logs, bridges = [], [], []
    with tempfile.TemporaryDirectory(prefix="session-", dir=out) as temporary:
        work = Path(temporary)
        discovery = work / "discovery"
        env = dict(os.environ, MITCAD_MCP_DISCOVERY_DIR=str(discovery),
                   MITCAD_RESULT_STORE="off", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1")

        if binary_path:
            env["PATH"] = binary_path + os.pathsep + env.get("PATH", "")

        def states():
            answer = {}
            for path in discovery.glob("*.window"):
                state = json.loads(path.read_text(encoding="utf-8"))
                answer[state["title"]] = state
            return answer

        def wait(predicate, what):
            deadline = time.monotonic() + 30
            while time.monotonic() < deadline:
                for gui, log_path in guis:
                    text = log_path.read_text(encoding="utf-8", errors="replace")
                    if gui.poll() is not None or any(line.startswith("TestDriver: FAIL") for line in text.splitlines()):
                        raise AssertionError(what + ": GUI failed\n" + text)
                if predicate():
                    return
                time.sleep(0.05)
            raise AssertionError("Timed out: " + what + " " + json.dumps(states()))

        def activations(label):
            # The driver's window-activate steps done so far. A window also
            # records an activation when it is first shown, so its
            # activated_at alone does not say that the step has run: under
            # load the other window's step could come before it.
            text = (out / (label + ".log")).read_text(encoding="utf-8", errors="replace")
            return text.count("Assistant window activated")

        try:
            for label, title in (("first", "First design"), ("second", "Second design")):
                own = work / label
                own.mkdir()
                own_env = dict(env, MITCAD_ASSISTANT_TEST_WORK=str(own), MITCAD_MCP_WINDOW_LABEL=title,
                               MITCAD_TEST_ASSISTANT_EXECUTABLES="none")
                for variable in ("MITCAD_SETTINGS_DIR", "MITCAD_AUTOSAVE_DIR", "MITCAD_PROJECTS_DIR",
                                 "MITCAD_THUMBNAIL_DIR", "MITCAD_CRASH_DIR"):
                    own_env[variable] = str(own / variable.lower())
                script = ["wait-idle", "expect-ever 'MCP live instance:'",
                          "expect-file '" + str(work / ("activate-" + label)) + "' 60",
                          "expect-assistant-setup window-activate"]
                if label == "first":
                    script += ["expect-file '" + str(work / "activate-again") + "' 60",
                               "expect-assistant-setup window-activate"]
                script += ["expect-file '" + str(work / "inspect") + "' 60",
                           "command help.connect_assistant", "focus-dialog '^Connect AI Assistant$'",
                           "expect-assistant-setup window-" + ("other" if label == "first" else "local"),
                           "expect-assistant-setup window-stale",
                           "button '^Close$'", "expect-dialog-gone '^Connect AI Assistant$'",
                           "expect-file '" + str(work / "finish") + "' 60", "quit"]
                source = own / "window.mitcad-ui"
                source.write_text("\n".join(script) + "\n", encoding="utf-8")
                own_env["MITCAD_TEST_INPUT"] = str(source)
                log_path = out / (label + ".log")
                log = log_path.open("wb")
                logs.append(log)
                gui = subprocess.Popen([app, "--ui=qt", "--chrome=floating", "--no-recovery"],
                                       env=own_env, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)
                guis.append((gui, log_path))
            wait(lambda: len(list(discovery.glob("*.json"))) == 2, "two real listeners")
            (work / "activate-first").touch()
            wait(lambda: activations("first") == 1 and
                 states().get("First design", {}).get("activated_at", 0) > 0, "first activation")
            (work / "activate-second").touch()
            wait(lambda: activations("second") == 1 and states().get("Second design", {}).get("activated_at", 0) >
                 states()["First design"]["activated_at"], "second activation")
            # The windows are titled as a modified design ("Second design[*]
            # - MitCad", mitcad#1149): the published title is the design's
            # name alone, without Qt's "[*]" placeholder.
            for title in states():
                live.check("[*]" not in title and "MitCad" not in title,
                           "A published title kept the raw window title: " + title)
            bridge = live.Bridge(cli, env, out / "bridge.stderr.log")
            bridges.append(bridge)
            response = bridge.request("initialize", {"protocolVersion": live.PROTOCOL, "capabilities": {},
                                      "clientInfo": {"name": "claude-code", "version": "window-test"}})
            live.check("result" in response, "Bridge failed to initialize")
            bridge.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
            document = bridge.tool("document_list", {})["documents"][0]["document"]
            wait(lambda: states().get("Second design", {}).get("agents") == ["claude-code"], "selected window client")
            live.check(states()["First design"]["agents"] == [], "The other window displayed the agent")
            live.check("the window active last" in bridge.stderr(), "Bridge did not explain its choice on stderr")
            live.check("(Second design)," in bridge.stderr() and "[*]" not in bridge.stderr(),
                       "Bridge did not name the window by its design: " + bridge.stderr())
            # Activating another design does not retarget an initialized session.
            (work / "activate-again").touch()
            wait(lambda: activations("first") == 2 and
                 states()["First design"]["activated_at"] > states()["Second design"]["activated_at"],
                 "first window reactivation")
            live.check(bridge.tool("document_list", {})["documents"][0]["document"] == document,
                       "Activation moved the connected bridge to another design")
            live.check(bridge.tool("ui_state", {})["agent_clients"] == ["claude-code"], "Agent left its selected window")
            # A crashed endpoint whose PID was reused is not a live connection.
            # It is private and well formed, but no listener owns its token.
            with socket.socket() as reserved:
                reserved.bind(("127.0.0.1", 0))
                stale = discovery / ("c" * 32 + ".json")
                stale.write_text(json.dumps({"protocol": 1, "host": "127.0.0.1", "instance": "c" * 32,
                    "token": "d" * 64, "pid": os.getpid(), "port": reserved.getsockname()[1]}), encoding="utf-8")
                private_record(stale)
                metadata = stale.with_suffix(".window")
                metadata.write_text(json.dumps({"pid": os.getpid(), "activated_at": 1,
                    "title": "Crashed design", "agents": ["codex"]}), encoding="utf-8")
                private_record(metadata)
            (work / "inspect").touch()
            wait(lambda: all("Assistant setup UI: window-" in path.read_text(encoding="utf-8", errors="replace")
                             for _, path in guis), "both dialogs and indicators name the connected window")
            for state_path in discovery.glob("*.window"):
                live.check("token" not in state_path.read_text(encoding="utf-8"), "Window metadata contains a token")
            wait(lambda: all("Assistant setup UI: window-stale" in path.read_text(encoding="utf-8", errors="replace")
                             for _, path in guis), "stale remote connection rejected")
            remove(stale)
            remove(metadata)
            bridge.finish()
            (work / "finish").touch()
            for gui, log_path in guis:
                gui.wait(timeout=30)
                text = log_path.read_text(encoding="utf-8", errors="replace")
                completed = re.search(r"^TestDriver: quit \((\d+) of \1 steps\)$", text, re.MULTILINE)
                live.check(gui.returncode == 0 and completed and "TestDriver: FAIL" not in text, text)
            live.check(not list(discovery.glob("*.window")), "Clean exit left window metadata")
        finally:
            for bridge in bridges:
                bridge.stop()
            for gui, _ in guis:
                if gui.poll() is None:
                    gui.terminate()
                    try:
                        gui.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        gui.kill()
                        gui.wait(timeout=5)
            for log in logs:
                log.close()
    print("PASS: last-active window, fixed MCP session, both connection dialogs and local-only indicators")


if __name__ == "__main__":
    run(*sys.argv[1:4])
