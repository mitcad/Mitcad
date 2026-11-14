# SPDX-License-Identifier: MIT
"""Actual GUI/CLI localhost MCP integration; run only in the macOS VM.

Besides discovery, authentication and the model tools, the live debugging
flow of tools/ui-mcp-live.py (mitcad#265) runs against a GUI that opened a
design whose sketch has a projection that no longer follows: queries and
sketch.add_line with the sketch open, ui_state with the palette's warning,
ui_select, ui_screenshot (the native window as the window server composes
it), ui_command Clean Up Stale Projections and Finish Sketch,
document_save and ui_log, through the native or the Qt frontend.

Usage: python3 ui-macos-mcp-test.py <app> <cli> <build-output> [native|qt]
The in-process TestDriver verifies the GUI model and native history without
Accessibility permission. All discovery, settings and documents are private.
"""

import copy
import importlib.util
import json
import os
from pathlib import Path
import queue
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import threading
import time

# python3 -I (the Linux wrappers) leaves this script's folder off the
# path; ui_macos_lock lives beside it.
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ui_macos_lock import gui_lock


PROTOCOL = "2025-11-25"
TIMEOUT = 20
FRAME_LIMIT = 8 * 1024 * 1024


def load_live():
    """The live debugging flow shared with the Linux test (ui-mcp-live.py)."""
    spec = importlib.util.spec_from_file_location("ui_mcp_live", Path(__file__).with_name("ui-mcp-live.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LIVE = load_live()
# The first GUI has no --mcp-folder: test its restricted input profile.
# The later debugger flow uses the shared module's folder-enabled catalog.
LIVE_TOOLS = LIVE.LIVE_TOOLS - {"ui_click", "ui_type", "ui_dialog"}
BOX = {"cmd": "add_feature", "def": {
    "type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 30, "width": 20, "height": 10,
    "operation": "new_body"}}


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def wait_for(predicate, description, gui=None, timeout=TIMEOUT):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        if gui is not None:
            check(gui.process.poll() is None, "GUI exited before " + description + "\n" + gui.log_text())
            check("TestDriver: FAIL" not in gui.log_text(), gui.log_text())
        time.sleep(0.05)
    raise AssertionError("Timed out: " + description + ("\n" + gui.log_text() if gui else ""))


class Gui:
    def __init__(self, app, env, out, label, script, ui, extra=(), *, gui_options):
        self.log_path = out / (label + ".log")
        self.log = self.log_path.open("wb")
        script_path = out / (label + ".mitcad-ui")
        script_path.write_text(script, encoding="utf-8")
        own_env = dict(env, MITCAD_TEST_INPUT=str(script_path))
        self.process = subprocess.Popen(
            [app, "--ui=" + ui, "--no-recovery", *extra], env=own_env,
            stdin=subprocess.DEVNULL, stdout=self.log, stderr=subprocess.STDOUT, **gui_options)

    def log_text(self):
        return self.log_path.read_text(encoding="utf-8", errors="replace")

    def stop(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        self.log.close()


class Bridge:
    def __init__(self, cli, env, out, label, instance=None):
        self.err = (out / (label + ".stderr.log")).open("w+b")
        args = [cli, "mcp", "--connect"]
        if instance is not None:
            args += ["--instance", instance]
        self.process = subprocess.Popen(args, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.err)
        self.messages = queue.Queue()
        self.reader = threading.Thread(target=self.read_output, daemon=True)
        self.reader.start()
        self.next_id = 0

    def read_output(self):
        while True:
            line = self.process.stdout.readline()
            self.messages.put(line)
            if not line:
                return

    def send_raw(self, line):
        self.process.stdin.write(line + b"\n")
        self.process.stdin.flush()

    def send(self, message):
        self.send_raw(json.dumps(message, separators=(",", ":")).encode("utf-8"))

    def receive(self):
        try:
            line = self.messages.get(timeout=TIMEOUT)
        except queue.Empty as error:
            raise AssertionError("Bridge response timed out") from error
        if not line:
            self.err.seek(0)
            raise AssertionError("Bridge stdout closed: " + self.err.read().decode("utf-8", "replace"))
        message = json.loads(line)
        check(isinstance(message, dict) and message.get("jsonrpc") == "2.0", "Invalid bridge stdout")
        return message

    def request(self, method, params=None, identifier=None):
        self.next_id += 1
        identifier = self.next_id if identifier is None else identifier
        message = {"jsonrpc": "2.0", "id": identifier, "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)
        result = self.receive()
        check(result.get("id") == identifier, "JSON-RPC request ID was changed: " + repr(result))
        return result

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def initialize(self, identifier=None):
        response = self.request("initialize", {
            "protocolVersion": PROTOCOL, "capabilities": {},
            "clientInfo": {"name": "mitcad-live-integration", "version": "1"}}, identifier=identifier)
        check(response["result"]["protocolVersion"] == PROTOCOL, "Incorrect negotiated protocol")
        self.notify("notifications/initialized")

    def tool(self, name, arguments, error=False):
        response = self.request("tools/call", {"name": name, "arguments": arguments})
        check("error" not in response, "Tool protocol failure: " + repr(response))
        result = response["result"]
        check(bool(result.get("isError", False)) == error, "Unexpected tool result: " + repr(result))
        value = result["structuredContent"]
        check(json.loads(result["content"][0]["text"]) == value, "Tool text and JSON disagree")
        # Screenshot bytes are a separate MCP image block, not part of
        # structuredContent. Keep the metadata equality check above intact.
        images = [item for item in result["content"][1:] if item.get("type") == "image"]
        if name == "ui_screenshot" and not error:
            check(len(images) == 1, "Screenshot must include exactly one MCP image block")
        if images:
            value = dict(value, image=images[0])
        return value

    def finish(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.reader.join(timeout=TIMEOUT)
        self.err.seek(0)
        check(self.process.returncode == 0, self.err.read().decode("utf-8", "replace"))
        check(self.messages.get(timeout=TIMEOUT) == b"", "Extra MCP output before EOF")
        check(self.messages.empty(), "Extra MCP output after EOF")

    def stop(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)
        if not self.process.stdin.closed:
            self.process.stdin.close()
        self.reader.join(timeout=5)
        self.process.stdout.close()
        self.err.close()


def socket_json(reader):
    line = reader.readline(FRAME_LIMIT + 2)
    check(line and len(line) <= FRAME_LIMIT + 1, "Socket closed or sent an oversized response")
    return json.loads(line)


def raw_socket(endpoint):
    return socket.create_connection(("127.0.0.1", endpoint["port"]), timeout=TIMEOUT)


def authenticate(sock, reader, endpoint):
    sock.sendall(json.dumps({"mitcadAuth": endpoint["token"]}).encode("utf-8") + b"\n")
    check(socket_json(reader) == {"authenticated": True}, "Socket authentication failed")


def public_instances(cli, env):
    result = subprocess.run([cli, "mcp", "--list-instances"], env=env,
                            capture_output=True, timeout=TIMEOUT)
    check(result.returncode == 0, result.stderr.decode("utf-8", "replace"))
    answer = json.loads(result.stdout)
    check(set(answer) == {"instances"}, "Unexpected discovery shape")
    check(all("token" not in record for record in answer["instances"]), "Discovery exposed a token")
    return answer["instances"]


def body_volume(bridge, document):
    bodies = bridge.tool("model_query", {
        "document": document, "query": {"query": "bodies", "volumes": True}})["result"]
    return len(bodies), sum(body["volume"] for body in bodies)


def check_png(path, gui, following_gate):
    # QImage creates the destination before it finishes encoding the PNG.
    # The next TestDriver step is reached only after saving has returned.
    marker = ": expect-file '${MITCAD_MCP_TEST_WORK}/" + following_gate + "'"
    wait_for(lambda: marker in gui.log_text(), "GUI screenshot saved: " + path.name, gui)
    header = path.read_bytes()[:24]
    check(header[:8] == b"\x89PNG\r\n\x1a\n", "GUI screenshot is not PNG")
    width, height = struct.unpack(">II", header[16:24])
    check(width >= 300 and height >= 200, "GUI screenshot is too small")


def run(app, cli, out, ui):
    # The coordinated windows belong to one workflow and share ownership;
    # every actual GUI child inherits its descriptor through Gui below.
    with gui_lock() as gui_options:
        _run_locked(app, cli, out, ui, gui_options)


def _run_locked(app, cli, out, ui, gui_options):
    out.mkdir(parents=True, exist_ok=True)
    guis, bridges = [], []
    with tempfile.TemporaryDirectory(prefix="live-", dir=out) as directory:
        work = Path(directory)
        env = dict(os.environ)
        for key in ("EMAIL", "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL",
                    "MITCAD_TEST_INPUT", "MITCAD_TEST_RECOMPUTE_DELAY_MS"):
            env.pop(key, None)
        for key, name in (("MITCAD_SETTINGS_DIR", "settings"), ("MITCAD_AUTOSAVE_DIR", "autosave"),
                          ("MITCAD_PROJECTS_DIR", "projects"), ("MITCAD_THUMBNAIL_DIR", "thumbnails"),
                          ("MITCAD_CRASH_DIR", "crashes"), ("MITCAD_MCP_DISCOVERY_DIR", "discovery")):
            env[key] = str(work / name)
        env.update(QT_FORCE_STDERR_LOGGING="1", MITCAD_LOG_VOLUMES="1", MITCAD_RESULT_STORE="off",
                   MITCAD_MCP_TEST_WORK=str(work), MITCAD_MCP_TEST_OUT=str(out),
                   MITCAD_START_PAGE="0", MITCAD_NEW_DESIGN_VIEW="0", MITCAD_LANGUAGE="en", MITCAD_NO_UPDATE_CHECK="1", MITCAD_TEST_LOG_URLS="1",
                   GIT_CONFIG_NOSYSTEM="1")
        gitconfig = work / "gitconfig"
        gitconfig.write_text("", encoding="utf-8")
        env["GIT_CONFIG_GLOBAL"] = str(gitconfig)
        settings = work / "settings" / "nocodo.fi"
        settings.mkdir(parents=True)
        # This workflow replaces a dirty document and rejects its old handle.
        # Multi-document tests separately check the default new-window policy.
        (settings / "Mitcad.ini").write_text(
            "[autosave]\nenabled=false\n[documents]\nopenInCurrentWindow=true\n", encoding="utf-8")
        for filename in ("created.png", "edited.png", "replaced.png"):
            (out / filename).unlink(missing_ok=True)
        script = "expect-ever 'View area'\n"
        if ui == "native":
            script += "expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'\nexpect-native-visible native.historyCard true\nexpect-native-layout\n"
        # #619 adds a content-sized Create row; retain the measurements caps
        # and account only for that row when it is visible.
        if ui == "qt":
            script += "expect-file '${MITCAD_MCP_TEST_WORK}/face' 60\n"
            script += "expect-selection-info 'Area: (200|300|600) mm²' 200 with-create-row\n"
            script += "command view.fit\nscreenshot '${MITCAD_MCP_TEST_OUT}/selection-face.png'\n"
            script += "expect-file '${MITCAD_MCP_TEST_WORK}/measure' 60\n"
            script += "screenshot '${MITCAD_MCP_TEST_OUT}/measure-panel.png'\nkey Escape\n"
            script += "expect-file '${MITCAD_MCP_TEST_WORK}/edge' 60\n"
            script += "expect-selection-info 'Length: (10|20|30) mm' 200 with-create-row\n"
            script += "screenshot '${MITCAD_MCP_TEST_OUT}/selection-edge.png'\n"
        script += "expect-file '${MITCAD_MCP_TEST_WORK}/create' 60\nexpect-model-bodies 1 6000\n"
        if ui == "qt":
            script += "expect-selection-info 'Volume: 6000 mm³' 240 with-create-row\n"
        if ui == "native":
            script += "expect-native-history\n"
        script += "command view.fit\nscreenshot-view '${MITCAD_MCP_TEST_OUT}/created.png'\n"
        if ui == "qt":
            script += "capture-view-height orbit_height\ncapture-view-orientation orbit_direction\n"
            script += "trackpad-scroll 55 45 45 30 alt\n"
            script += "expect-view-orientation ${orbit_direction} changed\n"
            script += "expect-view-zoom ${orbit_height} unchanged\nsleep 300\nexpect-view-buttons none\n"
            script += "capture-view-orientation pan_direction\ntrackpad-scroll 55 45 10 10 ''\n"
            script += "expect-view-orientation ${pan_direction} unchanged\nsleep 300\nexpect-view-buttons none\n"
        script += "expect-file '${MITCAD_MCP_TEST_WORK}/edit' 60\nexpect-model-bodies 1 9000\n"
        if ui == "qt":
            script += "expect-selection-info 'Volume: 9000 mm³' 240 with-create-row\n"
        if ui == "native":
            script += "expect-native-history\n"
        script += "command view.fit\nscreenshot-view '${MITCAD_MCP_TEST_OUT}/edited.png'\n"
        script += "expect-file '${MITCAD_MCP_TEST_WORK}/replace' 60\nmenu 'File/New Design'\n"
        script += "focus-dialog '^Save changes to .*'\nbutton \"^Don't Save$\"\nexpect-model-bodies 0 0\n"
        script += "screenshot-view '${MITCAD_MCP_TEST_OUT}/replaced.png'\n"
        if ui == "qt":
            script += "expect-selection-info '^$'\nexpect-file '${MITCAD_MCP_TEST_WORK}/circle' 60\n"
            script += "expect-selection-info 'Radius: 5 mm' 240 with-create-row\nexpect-selection-info 'Diameter: 10 mm'\n"
            script += "command view.fit\nscreenshot '${MITCAD_MCP_TEST_OUT}/selection-circle.png'\n"
        script += "expect-file '${MITCAD_MCP_TEST_WORK}/finish' 60\nquit\n"
        try:
            gui = Gui(app, env, out, "gui", script, ui, gui_options=gui_options)
            guis.append(gui)
            discovery = Path(env["MITCAD_MCP_DISCOVERY_DIR"])
            wait_for(lambda: discovery.exists() and bool(list(discovery.glob("*.json"))), "GUI endpoint", gui)
            records = list(discovery.glob("*.json"))
            check(len(records) == 1, "Expected exactly one GUI endpoint")
            record = records[0]
            endpoint = json.loads(record.read_text(encoding="utf-8"))
            check(endpoint["host"] == "127.0.0.1" and endpoint["pid"] == gui.process.pid,
                  "Endpoint did not identify the GUI loopback listener")
            check(stat.S_IMODE(discovery.stat().st_mode) == 0o700, "Discovery directory is not private")
            check(stat.S_IMODE(record.stat().st_mode) == 0o600, "Endpoint record is not private")
            listeners = subprocess.run(["/usr/sbin/lsof", "-nP", "-a", "-p", str(gui.process.pid),
                                        "-iTCP", "-sTCP:LISTEN"], capture_output=True, timeout=TIMEOUT)
            listener_lines = [line for line in listeners.stdout.decode("utf-8", "replace").splitlines()
                              if ":%d " % endpoint["port"] in line]
            check(listener_lines and all("127.0.0.1:%d " % endpoint["port"] in line for line in listener_lines),
                  "MCP listener is not restricted to IPv4 loopback")

            # Reject tokens and unauthenticated MCP before any model operation.
            for message in ({"mitcadAuth": "incorrect"}, {"jsonrpc": "2.0", "id": 1, "method": "tools/list"}):
                with raw_socket(endpoint) as sock, sock.makefile("rb") as reader:
                    sock.sendall(json.dumps(message).encode("utf-8") + b"\n")
                    check(socket_json(reader) == {"authenticated": False}, "Unauthenticated connection was accepted")

            listed = public_instances(cli, env)
            check(len(listed) == 1 and listed[0]["instance"] == endpoint["instance"], "CLI did not discover the GUI")
            check(endpoint["token"] not in json.dumps(listed), "Public discovery exposed authentication material")
            bridge = Bridge(cli, env, out, "bridge")
            bridges.append(bridge)
            check("error" in bridge.request("tools/list"), "Live tools did not require initialization")
            bridge.initialize(identifier=(1 << 64) - 1)
            connection = bridge.tool("ui_state", {})
            check(connection["agents"] == 1 and connection["agent_clients"] == ["mitcad-live-integration"],
                  "Initialized client name was not reported by the live UI")
            names = {tool["name"] for tool in bridge.request("tools/list")["result"]["tools"]}
            check(names == LIVE_TOOLS,
                  "Live no-folder catalog differs: missing " + repr(sorted(LIVE_TOOLS - names))
                  + "; unexpected " + repr(sorted(names - LIVE_TOOLS)))
            document = bridge.tool("document_list", {})["documents"][0]["document"]
            bridge.request("ping", identifier=(1 << 64) - 1)
            check(body_volume(bridge, document) == (0, 0), "Initial GUI document was not empty")
            bridge.notify("tools/call", {"name": "model_command", "arguments": {"document": document, "command": BOX}})
            check(body_volume(bridge, document) == (0, 0), "A notification mutated the GUI")
            # No folder for designs is allowed in this run (mitcad#402):
            # document_save's path is refused by the application.
            refused = bridge.request("tools/call", {"name": "document_save", "arguments": {"document": document, "path": "outside.mitcad"}})
            check(refused.get("result", {}).get("isError") is True
                  and "No folder for designs is allowed" in refused["result"]["structuredContent"]["error"],
                  "Live socket exposed file writes: " + repr(refused)[:2000])
            bridge.tool("model_command", {"document": document, "command": {"cmd": "load_design", "json": "{}"}}, error=True)
            check(body_volume(bridge, document) == (0, 0), "A refused command changed the model")

            bridge.tool("model_command", {"document": document, "command": BOX})
            check(body_volume(bridge, document) == (1, 6000), "CLI edited a separate model instead of the GUI")
            if ui == "qt":
                bodies = bridge.tool("model_query", {
                    "document": document, "query": {"query": "bodies"}})["result"]
                body = bodies[0]["uid"]
                faces = bridge.tool("model_query", {
                    "document": document, "query": {"query": "faces", "body": body}})["result"]
                bridge.tool("ui_select", {"document": document, "items": [
                    {"kind": "face", "owner": body, "name": faces[0]["names"][0]}]})
                (work / "face").touch()
                check_png(out / "selection-face.png", gui, "measure")
                bridge.tool("ui_command", {"document": document, "id": "inspect.measure",
                    "inputs": {"a": [{"kind": "face", "owner": body, "name": faces[0]["names"][0]}],
                               "b": [{"kind": "face", "owner": body, "name": faces[1]["names"][0]}]},
                    "finish": "keep_open"})
                (work / "measure").touch()
                check_png(out / "measure-panel.png", gui, "edge")
                edges = bridge.tool("model_query", {
                    "document": document, "query": {"query": "edges", "body": body}})["result"]
                bridge.tool("ui_select", {"document": document, "items": [
                    {"kind": "edge", "owner": body, "name": edges[0]["name"]}]})
                (work / "edge").touch()
                check_png(out / "selection-edge.png", gui, "create")
                bridge.tool("ui_select", {"document": document, "items": [
                    {"kind": "body", "owner": body}]})
            (work / "create").touch()
            check_png(out / "created.png", gui, "edit")
            bridge.finish()
            check(gui.process.poll() is None, "Bridge EOF closed the GUI")

            # An id-less parse-error reply must also drain the bridge before EOF.
            malformed = Bridge(cli, env, out, "malformed")
            bridges.append(malformed)
            malformed.send_raw(b"{bad json")
            check(malformed.receive()["error"]["code"] == -32700, "Malformed JSON was not refused")
            malformed.finish()
            invalid_utf8 = Bridge(cli, env, out, "invalid-utf8", endpoint["instance"])
            bridges.append(invalid_utf8)
            invalid_utf8.send_raw(b'{"jsonrpc":"2.0","method":"notifications/\xff"}')
            check(invalid_utf8.receive()["error"]["code"] == -32700, "Invalid UTF-8 was not refused")
            invalid_utf8.finish()
            ignored_response = Bridge(cli, env, out, "client-response", endpoint["instance"])
            bridges.append(ignored_response)
            ignored_response.initialize()
            ignored_response.send({"jsonrpc": "2.0", "id": "server-request", "result": {}})
            ignored_response.finish()
            reconnect = Bridge(cli, env, out, "reconnect", endpoint["instance"])
            bridges.append(reconnect)
            reconnect.initialize()
            check(reconnect.tool("document_list", {})["documents"][0]["document"] == document,
                  "Reconnecting replaced the GUI document handle")
            check(body_volume(reconnect, document) == (1, 6000), "Bridge disconnect discarded GUI work")
            timeline = reconnect.tool("model_query", {"document": document, "query": {"query": "timeline"}})["result"]
            feature = timeline["features"][0]["uid"]
            edit = copy.deepcopy(BOX)
            edit["cmd"], edit["uid"], edit["def"]["height"] = "edit_feature", feature, 15
            reconnect.tool("model_command", {"document": document, "command": edit})
            check(body_volume(reconnect, document) == (1, 9000), "GUI box was not edited")
            (work / "edit").touch()
            check_png(out / "edited.png", gui, "replace")

            # Valid definitions can commit despite an OCCT geometry failure.
            # The applied/error distinction and GUI undo must both survive RPC.
            cut = {"cmd": "add_feature", "def": {
                "type": "box", "plane": {"type": "origin", "plane": "xy"},
                "corner": [100, 100], "length": 5, "width": 5, "height": 5,
                "operation": "cut", "participants": [feature + ".b0"]}}
            failed = reconnect.tool("model_command", {"document": document, "command": cut}, error=True)
            check(failed.get("applied") is True, "Geometry failure lost its applied mutation")
            failed_timeline = reconnect.tool("model_query", {
                "document": document, "query": {"query": "timeline"}})["result"]
            check(len(failed_timeline["features"]) == 2 and failed_timeline["features"][-1]["status"] == "error",
                  "Geometry failure did not remain in the actual GUI timeline")
            reconnect.tool("model_command", {"document": document, "command": {"cmd": "undo"}})
            check(body_volume(reconnect, document) == (1, 9000), "GUI undo did not recover the failed command")

            # A bounded, authenticated raw request must preserve the full ID too.
            with raw_socket(endpoint) as sock, sock.makefile("rb") as reader:
                authenticate(sock, reader, endpoint)
                initialize = {"jsonrpc": "2.0", "id": (1 << 64) - 1, "method": "initialize", "params": {
                    "protocolVersion": PROTOCOL, "capabilities": {},
                    "clientInfo": {"name": "mitcad-raw-integration", "version": "1"}}}
                sock.sendall(json.dumps(initialize).encode("utf-8") + b"\n")
                check(socket_json(reader).get("id") == initialize["id"], "Socket changed an initialize u64 ID")
                before_ready = reconnect.tool("ui_state", {})
                check("mitcad-raw-integration" not in before_ready["agent_clients"],
                      "A client was marked connected before notifications/initialized")
                sock.sendall(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                huge = {"jsonrpc": "2.0", "id": (1 << 64) - 1, "method": "ping"}
                sock.sendall(json.dumps(huge).encode("utf-8") + b"\n")
                check(socket_json(reader).get("id") == huge["id"], "Socket changed a u64 request ID")
                live_names = reconnect.tool("ui_state", {})["agent_clients"]
                check("mitcad-raw-integration" in live_names and len(live_names) >= 2,
                      "Multiple initialized clients were not named")
            wait_for(lambda: "mitcad-raw-integration" not in reconnect.tool("ui_state", {})["agent_clients"],
                     "Disconnected client label to clear", gui)
            with raw_socket(endpoint) as sock, sock.makefile("rb") as reader:
                authenticate(sock, reader, endpoint)
                try:
                    sock.sendall(b" " * (FRAME_LIMIT + 1) + b"\n")
                    check(socket_json(reader)["error"]["code"] == -32600, "Oversized frame was not refused")
                except (BrokenPipeError, ConnectionResetError):
                    pass
            check(body_volume(reconnect, document) == (1, 9000), "Oversized input damaged the GUI document")

            # Valid-looking stale, remote-host and public records are not usable.
            stale = dict(endpoint, instance="f" * 32, token="a" * 64)
            with socket.socket() as reserved:
                reserved.bind(("127.0.0.1", 0))
                stale["port"] = reserved.getsockname()[1]
            for candidate, permissions in ((stale, 0o600),
                                           (dict(endpoint, instance="e" * 32, host="0.0.0.0"), 0o600),
                                           (dict(endpoint, instance="d" * 32), 0o644)):
                path = discovery / (candidate["instance"] + ".json")
                path.write_text(json.dumps(candidate), encoding="utf-8")
                path.chmod(permissions)
            check(len(public_instances(cli, env)) == 1, "CLI trusted stale or insecure discovery records")
            unknown = subprocess.run([cli, "mcp", "--connect", "--instance", stale["instance"]],
                                     env=env, capture_output=True, timeout=TIMEOUT)
            check(unknown.returncode != 0 and not unknown.stdout, "CLI connected to a stale endpoint")

            second_script = "expect-ever 'View area'\nexpect-file '${MITCAD_MCP_TEST_WORK}/second-finish' 60\nquit\n"
            second = Gui(app, env, out, "second", second_script, ui, gui_options=gui_options)
            guis.append(second)
            wait_for(lambda: len(public_instances(cli, env)) == 2, "second GUI discovery", second)
            automatic = subprocess.run([cli, "mcp", "--connect"], env=env, input=b"",
                                       capture_output=True, timeout=TIMEOUT)
            check(automatic.returncode == 0 and not automatic.stdout and b"the window active last" in automatic.stderr,
                  "CLI did not choose and name the window active last")
            selected = Bridge(cli, env, out, "selected", endpoint["instance"])
            bridges.append(selected)
            selected.initialize()
            check(body_volume(selected, document) == (1, 9000), "Explicit selection chose another GUI")
            selected.finish()
            (work / "second-finish").touch()
            second.process.wait(timeout=TIMEOUT)
            check(second.process.returncode == 0 and "TestDriver: FAIL" not in second.log_text(), second.log_text())

            (work / "replace").touch()
            check_png(out / "replaced.png", gui, "circle" if ui == "qt" else "finish")
            replacement = reconnect.tool("document_list", {})["documents"][0]["document"]
            check(replacement != document, "New reused a stale GUI document handle")
            reconnect.tool("model_command", {"document": document, "command": BOX}, error=True)
            check(body_volume(reconnect, replacement) == (0, 0), "A stale handle edited the new GUI document")
            if ui == "qt":
                reconnect.tool("model_command", {"document": replacement, "command": {
                    "cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
                    "diameter": 10, "height": 20, "operation": "new_body"}}})
                bodies = reconnect.tool("model_query", {
                    "document": replacement, "query": {"query": "bodies"}})["result"]
                body = bodies[0]["uid"]
                edges = reconnect.tool("model_query", {
                    "document": replacement, "query": {"query": "edges", "body": body}})["result"]
                circular = None
                for edge in edges:
                    measured = reconnect.tool("model_query", {"document": replacement, "query": {
                        "query": "measure", "a": {"body": body, "edge": edge["name"]}}})["result"]["a"]
                    if "circle" in measured:
                        circular = edge["name"]
                        break
                check(circular is not None, "Cylinder has no circular edge")
                reconnect.tool("ui_select", {"document": replacement, "items": [
                    {"kind": "edge", "owner": body, "name": circular}]})
                (work / "circle").touch()
                check_png(out / "selection-circle.png", gui, "finish")
            reconnect.finish()
            lingering = Bridge(cli, env, out, "lingering", endpoint["instance"])
            bridges.append(lingering)
            lingering.initialize()
            (work / "finish").touch()
            gui.process.wait(timeout=TIMEOUT)
            check(gui.process.returncode == 0 and "TestDriver: FAIL" not in gui.log_text(), gui.log_text())
            # The client still owns an open stdin pipe. A disconnected GUI
            # must nevertheless end the bridge and close stdout promptly.
            check(not lingering.process.stdin.closed, "Lingering client stdin closed unexpectedly")
            lingering.process.wait(timeout=TIMEOUT)
            check(lingering.process.returncode != 0, "GUI disconnect did not end the lingering bridge")
            check(lingering.messages.get(timeout=TIMEOUT) == b"", "GUI disconnect left bridge stdout open")
            check(not record.exists(), "Clean GUI exit left its discovery token behind")
            check(public_instances(cli, env) == [], "CLI reported a closed GUI instance")
            disabled_script = "expect-ever 'View area'\nexpect-file '${MITCAD_MCP_TEST_WORK}/disabled-finish' 60\nquit\n"
            disabled = Gui(app, env, out, "disabled", disabled_script, ui, ("--no-mcp",), gui_options=gui_options)
            guis.append(disabled)
            wait_for(lambda: "View area" in disabled.log_text(), "MCP-disabled GUI", disabled)
            check(public_instances(cli, env) == [], "--no-mcp still published a listener")
            (work / "disabled-finish").touch()
            disabled.process.wait(timeout=TIMEOUT)
            check(disabled.process.returncode == 0, disabled.log_text())

            # The live debugging flow (mitcad#265) on a design whose sketch F4
            # holds a projection that no longer follows its face.
            debug = work / "debug"
            debug.mkdir()
            design = LIVE.make_design(cli, debug, env)
            debug_script = "expect-ever 'View area'\nexpect-file '${MITCAD_MCP_TEST_WORK}/debug-finish' 300\nquit\n"
            debugger = Gui(app, env, out, "debug", debug_script, ui,
                           ("--mcp-folder", str(debug), "--open", str(design)), gui_options=gui_options)
            guis.append(debugger)
            wait_for(lambda: len(public_instances(cli, env)) == 1, "the debugging GUI's discovery", debugger)
            live = LIVE.Bridge(cli, env, out / "debug-bridge.stderr.log")
            bridges.append(live)
            live.initialize()
            LIVE.run_flow(live, design)
            live.finish()
            wait_for(lambda: "Agent disconnected" in debugger.log_text(), "the agent's disconnection", debugger)
            check("Agent connected" in debugger.log_text(), "The GUI did not say an agent was connected")
            # The agent indicator (mitcad#710) named the client, then none.
            check("Agent indicator: connected: mitcad-live-debug" in debugger.log_text(),
                  "The agent indicator did not name the connected client")
            wait_for(lambda: "Agent indicator: no agent" in debugger.log_text().split(
                         "Agent indicator: connected: mitcad-live-debug")[-1],
                     "the agent indicator's no agent after the disconnection", debugger)
            (work / "debug-finish").touch()
            debugger.process.wait(timeout=TIMEOUT)
            check(debugger.process.returncode == 0 and "TestDriver: FAIL" not in debugger.log_text(),
                  debugger.log_text())
            print("PASS: live GUI MCP discovery, authentication, bridge, model, UI, EOF and input limits, "
                  "and the live debugging flow")
        finally:
            for bridge in bridges:
                bridge.stop()
            for gui in guis:
                gui.stop()


def main():
    if len(sys.argv) not in (4, 5):
        print(__doc__, file=sys.stderr)
        return 2
    if sys.platform != "darwin":
        print("This integration test runs only in the isolated macOS VM", file=sys.stderr)
        return 2
    session = subprocess.run(["launchctl", "managername"], capture_output=True, text=True, timeout=5)
    if session.stdout.strip() == "System" and os.environ.get("MITCAD_UI_REQUIRE_GUI") != "1":
        print("SKIP: no macOS window server in the System session")
        return 77
    app, cli, out = str(Path(sys.argv[1]).resolve()), str(Path(sys.argv[2]).resolve()), Path(sys.argv[3]).resolve()
    ui = sys.argv[4] if len(sys.argv) == 5 else "native"
    check(ui in ("native", "qt"), "UI must be native or qt")
    try:
        run(app, cli, out, ui)
        return 0
    except Exception as error:
        print("FAIL: live GUI MCP: " + str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
