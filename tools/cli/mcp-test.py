# SPDX-License-Identifier: MIT
"""Exercise the actual MCP CLI over stdio; run only in an isolated build.

Usage: python3 mcp-test.py /absolute/path/mitcad-cli /build/test/directory
"""

import base64
import copy
import json
import os
from pathlib import Path
import queue
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import zipfile


CLI = str(Path(sys.argv[1]).resolve())
WORK = Path(sys.argv[2]).resolve()
TIMEOUT = 20
PROTOCOL = "2025-11-25"
TOOLS = {"document_create", "document_open", "document_list", "document_close",
         "model_command", "model_query", "document_save", "model_export", "model_reference",
         "model_render"}
CYLINDER = {"cmd": "add_feature", "def": {
    "type": "cylinder", "plane": {"type": "origin", "plane": "xy"},
    "center": [45, 10], "diameter": 16, "height": 25, "operation": "new_body"}}


def render(client, arguments, error=False):
    """model_render's structuredContent and PNG bytes (None for a refusal)."""
    response = client.request("tools/call", {"name": "model_render", "arguments": arguments})
    assert "error" not in response, response
    result = response["result"]
    assert bool(result.get("isError", False)) == error, result
    content = result["structuredContent"]
    assert json.loads(result["content"][0]["text"]) == content, result
    if error:
        assert len(result["content"]) == 1, result
        return content, None
    assert "image_base64" not in content, content
    image = result["content"][1]
    assert image["type"] == "image" and image["mimeType"] == "image/png", image
    png = base64.b64decode(image["data"])
    assert png.startswith(b"\x89PNG\r\n\x1a\n"), png[:8]
    assert len(png) == content["bytes"], content
    return content, png


def render_views(client, arguments):
    """model_render with views: structuredContent and each view's PNG."""
    response = client.request("tools/call", {"name": "model_render", "arguments": arguments})
    assert "error" not in response, response
    result = response["result"]
    assert not result.get("isError", False), result
    content = result["structuredContent"]
    assert json.loads(result["content"][0]["text"]) == content, result
    images = result["content"][1:]
    assert len(images) == len(content["views"]), result
    pngs = []
    for image, view in zip(images, content["views"]):
        assert image["type"] == "image" and image["mimeType"] == "image/png", image
        assert "image_base64" not in view, view
        png = base64.b64decode(image["data"])
        assert len(png) == view["bytes"], view
        pngs.append(png)
    return content, pngs


def count_pixels(png, test):
    """How many pixels of a PNG pass test((r, g, b))."""
    _, _, channels, rows = png_pixels(png)
    return sum(1 for row in rows for i in range(0, len(row), channels) if test(row[i:i + 3]))


def uniform(png, x0, y0, size):
    """Whether a size x size block of a PNG from (x0, y0) has one color."""
    _, _, channels, rows = png_pixels(png)
    colors = {rows[y][x * channels:x * channels + 3] for y in range(y0, y0 + size) for x in range(x0, x0 + size)}
    return len(colors) == 1


def png_pixels(png):
    """Width, height, bytes a pixel and rows of an 8-bit RGB(A) PNG."""
    import zlib
    width, height = int.from_bytes(png[16:20], "big"), int.from_bytes(png[20:24], "big")
    channels = {2: 3, 6: 4}[png[25]]
    at, data = 8, b""
    while at < len(png):
        length = int.from_bytes(png[at:at + 4], "big")
        if png[at + 4:at + 8] == b"IDAT":
            data += png[at + 8:at + 8 + length]
        at += 12 + length
    raw = zlib.decompress(data)
    stride = width * channels
    rows, above = [], bytearray(stride)
    for y in range(height):
        kind = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = above[i]
            c = above[i - channels] if i >= channels else 0
            if kind == 1:
                line[i] = (line[i] + a) & 255
            elif kind == 2:
                line[i] = (line[i] + b) & 255
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        above = line
    return width, height, channels, rows
BOX = {"cmd": "add_feature", "def": {
    "type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 30, "width": 20, "height": 10,
    "operation": "new_body"}}


def print_project(path):
    """A synthetic 3MF carrying printer, process and filament settings."""
    settings = {
        "printer_settings_id": "Fixture printer",
        "printer_model": "Fixture machine",
        "printable_area": ["0x0", "180x0", "180x180", "0x180"],
        "printable_height": "190", "nozzle_diameter": ["0.6"],
        "print_settings_id": "Fixture process", "layer_height": "0.28",
        "wall_loops": "4", "sparse_infill_density": "18%",
        "filament_settings_id": ["Fixture PLA"], "filament_type": ["PLA"],
        "filament_colour": ["#2080C0"], "nozzle_temperature": ["223"],
        "filament_density": ["1.24"]}
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as package:
        package.writestr("[Content_Types].xml", '<?xml version="1.0"?>'
                         '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                         '<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>'
                         '</Types>')
        package.writestr("3D/3dmodel.model", '<?xml version="1.0"?>'
                         '<model unit="millimeter" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">'
                         '<metadata name="Application">OrcaSlicer-2.3.0</metadata>'
                         '<resources/><build/></model>')
        package.writestr("Metadata/project_settings.config", json.dumps(settings))
    return settings


def slicer_fixture(root):
    """Only test-owned user presets and their two-level system inheritance."""
    files = {
        "system/Fixture/machine/Base.json": {
            "type": "machine", "name": "Fixture base", "instantiation": "false",
            "nozzle_diameter": ["0.6"], "printable_height": "200"},
        "system/Fixture/machine/Bench.json": {
            "type": "machine", "name": "Fixture bench", "inherits": "Fixture base",
            "printer_model": "Fixture machine",
            "printable_area": ["0x0", "180x0", "180x180", "0x180"]},
        "system/Fixture/filament/Base PLA.json": {
            "type": "filament", "name": "Fixture base PLA", "filament_type": ["PLA"],
            "filament_density": ["1.24"], "nozzle_temperature": ["210"]},
        "user/default/machine/Printer.json": {
            "name": "Fixture printer", "inherits": "Fixture bench", "from": "User",
            "printable_height": "190"},
        "user/default/process/Process.json": {
            "name": "Fixture process", "from": "User", "layer_height": "0.28"},
        "user/default/filament/PLA.json": {
            "name": "Fixture PLA", "inherits": "Fixture base PLA", "from": "User",
            "nozzle_temperature": ["228"]}}
    for relative, value in files.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value), encoding="utf-8")
    return [root / relative for relative in files if relative.startswith("user/")]


def make_private(path, directory):
    """Make a discovery path private as the app does (LocalEndpoint.cpp).

    On Unix the owner-only mode bits; on Windows the owner set to this user
    and a protected DACL granting only this user and SYSTEM, which mode bits
    cannot express (WindowsEndpointSecurity.cpp checks the ACL).
    """
    if os.name != "nt":
        path.chmod(0o700 if directory else 0o600)
        return
    user = subprocess.run(["whoami"], capture_output=True, text=True, check=True).stdout.strip()
    inherit = "(OI)(CI)" if directory else ""
    subprocess.run(["icacls", str(path), "/setowner", user], capture_output=True, check=True)
    subprocess.run(["icacls", str(path), "/inheritance:r", "/grant:r", f"{user}:{inherit}F",
                    "/grant:r", f"*S-1-5-18:{inherit}F"], capture_output=True, check=True)


class Session:
    """Reader thread gives pipe reads a timeout on Windows as well as Unix."""

    def __init__(self, workspace, read_only=False, connect=False, env=None):
        self.stderr = tempfile.TemporaryFile(dir=workspace)
        # connect: the bridge to a window (mcp --connect) in place of the
        # headless server.
        args = [CLI, "mcp", "--connect"] if connect else [CLI, "mcp", "--workspace", str(workspace)]
        if read_only:
            args.append("--read-only")
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, env=env,
                                        stdout=subprocess.PIPE, stderr=self.stderr)
        self.messages = queue.Queue()
        self.next_id = 0
        self.reader = threading.Thread(target=self.read_stdout, daemon=True)
        self.reader.start()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if self.process.poll() is None:
            self.process.kill()
        self.process.wait(timeout=TIMEOUT)
        self.reader.join(timeout=TIMEOUT)
        self.process.stdin.close()
        self.process.stdout.close()
        self.stderr.close()

    def read_stdout(self):
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
            raise AssertionError("MCP response timed out") from error
        if not line:
            self.stderr.seek(0)
            raise AssertionError("MCP stdout closed: " + self.stderr.read().decode("utf-8"))
        try:
            message = json.loads(line)
        except (ValueError, UnicodeDecodeError) as error:
            raise AssertionError("Non-MCP stdout: " + repr(line)) from error
        assert isinstance(message, dict) and message.get("jsonrpc") == "2.0", message
        return message

    def request(self, method, params=None):
        self.next_id += 1
        request = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            request["params"] = params
        self.send(request)
        response = self.receive()
        assert response.get("id") == self.next_id, response
        return response

    def notify(self, method, params=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)

    def initialize(self, protocol=PROTOCOL):
        response = self.request("initialize", {
            "protocolVersion": protocol, "capabilities": {},
            "clientInfo": {"name": "mitcad-integration-test", "version": "1"}})
        assert response["result"]["protocolVersion"] == PROTOCOL, response
        self.notify("notifications/initialized")
        return response["result"]

    def tool(self, name, arguments, error=False):
        response = self.request("tools/call", {"name": name, "arguments": arguments})
        assert "error" not in response, response
        result = response["result"]
        assert bool(result.get("isError", False)) == error, result
        content = result["structuredContent"]
        assert json.loads(result["content"][0]["text"]) == content, result
        if error:
            if content.get("applied") is True:
                # The whole timeline when asked for, else what the command
                # changed, failed features included (mitcad#689).
                features = (content["timeline"] if "timeline" in content else content["changes"])["features"]
                assert any(item["status"] == "error" for item in features), content
            else:
                assert isinstance(content["error"], str) and content["error"], content
        return content

    def finish(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.reader.join(timeout=TIMEOUT)
        self.stderr.seek(0)
        diagnostics = self.stderr.read().decode("utf-8")
        assert self.process.returncode == 0, diagnostics
        assert self.messages.get(timeout=TIMEOUT) == b"", "Unexpected output before EOF"
        assert self.messages.empty(), "Unexpected output after EOF"


def dead_pid():
    """The id of a process that has exited, as a killed app's would be."""
    process = subprocess.Popen([sys.executable, "-c", ""])
    process.wait(timeout=TIMEOUT)
    return process.pid


class FakeInstance:
    """A discovery record and a loopback listener standing in for the app.

    The listener accepts and counts every connection. With answer=False it
    never replies, like a hung app or another program on a reused port:
    probing it can only time out, so a test sees whether the CLI probed it.
    """

    def __init__(self, discovery, instance, pid, answer=True):
        self.instance = instance
        self.pid = pid
        self.token = instance[0] * 64
        self.answer = answer
        self.accepted = 0
        self.errors = []
        self.held = []
        self.lock = threading.Lock()
        self.stopping = threading.Event()
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(8)
        self.listener.settimeout(0.1)
        self.record = discovery / (instance + ".json")
        self.record.write_text(json.dumps({"protocol": 1, "host": "127.0.0.1",
                                           "port": self.listener.getsockname()[1], "pid": pid,
                                           "instance": instance, "token": self.token}), encoding="utf-8")
        make_private(self.record, False)
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.stopping.set()
        self.thread.join(timeout=TIMEOUT)
        for connection in self.held:
            connection.close()
        self.listener.close()

    def serve(self):
        while not self.stopping.is_set():
            try:
                connection, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            with self.lock:
                self.accepted += 1
            if not self.answer:
                self.held.append(connection)
                continue
            try:
                with connection:
                    connection.settimeout(TIMEOUT)
                    with connection.makefile("rb") as source:
                        if json.loads(source.readline()) != {"mitcadAuth": self.token}:
                            raise AssertionError("Unexpected authentication frame")
                        connection.sendall(b'{"authenticated":true}\n')
                        try:
                            source.read()
                        except ConnectionError:
                            pass
            except Exception as error:
                self.errors.append(error)

    def activate(self, timestamp, title):
        state = self.record.with_suffix(".window")
        state.write_text(json.dumps({"pid": self.pid, "activated_at": timestamp,
                                     "title": title, "agents": []}), encoding="utf-8")
        make_private(state, False)

    def connections(self):
        with self.lock:
            return self.accepted


class FakeWindow:
    """A window's MCP endpoint that speaks the protocol (mitcad#1288).

    It answers initialize, subscriptions and every tool call (with its own
    instance id), and records each message it got. With refuse, it answers
    initialize with that error and closes the connection, as a window does
    for an assistant disconnected there (mitcad#1289).
    """

    def __init__(self, discovery, instance, refuse=None):
        self.instance = instance
        self.token = instance[0] * 64
        self.refuse = refuse
        self.messages = []
        self.errors = []
        self.connections = []
        self.lock = threading.Lock()
        self.stopping = threading.Event()
        self.listener = socket.socket()
        self.listener.bind(("127.0.0.1", 0))
        self.listener.listen(8)
        self.listener.settimeout(0.1)
        self.record = discovery / (instance + ".json")
        self.record.write_text(json.dumps({"protocol": 1, "host": "127.0.0.1",
                                           "port": self.listener.getsockname()[1], "pid": os.getpid(),
                                           "instance": instance, "token": self.token}), encoding="utf-8")
        make_private(self.record, False)
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        """The window closes: its record, its listener and its connections go."""
        if self.stopping.is_set():
            return
        self.stopping.set()
        self.thread.join(timeout=TIMEOUT)
        self.record.unlink(missing_ok=True)
        self.listener.close()
        with self.lock:
            for connection in self.connections:
                try:
                    connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
                connection.close()

    def serve(self):
        while not self.stopping.is_set():
            try:
                connection, _ = self.listener.accept()
            except socket.timeout:
                continue
            except OSError:
                return
            with self.lock:
                self.connections.append(connection)
            threading.Thread(target=self.talk, args=(connection,), daemon=True).start()

    def talk(self, connection):
        try:
            connection.settimeout(None)
            source = connection.makefile("rb")
            if json.loads(source.readline()) != {"mitcadAuth": self.token}:
                raise AssertionError("Unexpected authentication frame")
            connection.sendall(b'{"authenticated":true}\n')
            for line in source:
                message = json.loads(line)
                with self.lock:
                    self.messages.append(message)
                if "id" not in message:
                    continue
                ident = message["id"]
                if isinstance(ident, bool) or not isinstance(ident, (str, int)):
                    # As the window's server (core/mcp/src/lib.rs valid_id):
                    # an id it cannot use is answered with a null id.
                    connection.sendall(json.dumps({"jsonrpc": "2.0", "id": None, "error": {
                        "code": -32600, "message": "id must be a string or integer"}}).encode("utf-8") + b"\n")
                    continue
                method = message.get("method")
                if method == "initialize" and self.refuse:
                    connection.sendall(json.dumps({"jsonrpc": "2.0", "id": message["id"], "error": {
                        "code": -32001, "message": self.refuse}}).encode("utf-8") + b"\n")
                    connection.shutdown(socket.SHUT_RDWR)
                    return
                if method == "initialize":
                    result = {"protocolVersion": PROTOCOL, "capabilities": {"tools": {}},
                              "serverInfo": {"name": "mitcad", "version": "test"}}
                elif method == "tools/call":
                    answer = {"window": self.instance}
                    result = {"content": [{"type": "text", "text": json.dumps(answer)}],
                              "structuredContent": answer, "isError": False}
                else:
                    result = {}
                connection.sendall(json.dumps({"jsonrpc": "2.0", "id": message["id"],
                                               "result": result}).encode("utf-8") + b"\n")
        except OSError:
            pass  # the bridge or the test closed the connection
        except (AssertionError, ValueError) as error:
            self.errors.append(error)

    def methods(self):
        with self.lock:
            return [message.get("method") for message in self.messages]


class McpTest(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="session-", dir=WORK)
        self.addCleanup(self.temp.cleanup)
        self.workspace = Path(self.temp.name)

    def private_discovery(self):
        discovery = self.workspace / "discovery"
        discovery.mkdir(mode=0o700)
        make_private(discovery, True)
        return discovery, dict(os.environ, MITCAD_MCP_DISCOVERY_DIR=str(discovery))

    def test_stale_records_are_removed_without_probing(self):
        # mitcad#437: a record whose process is gone is skipped by its pid and
        # removed; its port, which would only time out, is never connected.
        discovery, env = self.private_discovery()
        with FakeInstance(discovery, "1" * 32, os.getpid()) as live, \
                FakeInstance(discovery, "2" * 32, dead_pid(), answer=False) as stale:
            result = subprocess.run([CLI, "mcp", "--list-instances"], env=env,
                                    capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 0, result.stderr)
            listed = [item["instance"] for item in json.loads(result.stdout)["instances"]]
            self.assertEqual(listed, [live.instance])
            self.assertFalse(stale.record.exists(), "The stale record was not removed")
            self.assertTrue(live.record.exists(), "The live record was removed")
            self.assertEqual((live.connections(), stale.connections()), (1, 0))
            with FakeInstance(discovery, "3" * 32, dead_pid(), answer=False) as crashed:
                result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                        capture_output=True, timeout=TIMEOUT)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout, b"")
                self.assertFalse(crashed.record.exists(), "The stale record was not removed")
                self.assertEqual((live.connections(), crashed.connections()), (3, 0))
        self.assertFalse(live.errors, live.errors)

    def test_instance_probes_only_the_named_record(self):
        # mitcad#437: --instance connects to that record alone, once; another
        # instance that would only time out is not probed.
        discovery, env = self.private_discovery()
        with FakeInstance(discovery, "4" * 32, os.getpid()) as named, \
                FakeInstance(discovery, "5" * 32, os.getpid(), answer=False) as hung:
            result = subprocess.run([CLI, "mcp", "--connect", "--instance", named.instance], env=env,
                                    input=b"", capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, b"")
            self.assertEqual((named.connections(), hung.connections()), (1, 0))
            unknown = subprocess.run([CLI, "mcp", "--connect", "--instance", "6" * 32], env=env,
                                     input=b"", capture_output=True, timeout=TIMEOUT)
            self.assertEqual(unknown.returncode, 1, unknown.stderr)
            self.assertIn(b"No running MitCad instance", unknown.stderr)
            self.assertEqual((named.connections(), hung.connections()), (1, 0))
            self.assertTrue(hung.record.exists(), "A running process's record was removed")
        self.assertFalse(named.errors, named.errors)

    def test_connect_selects_the_window_active_last(self):
        discovery, env = self.private_discovery()
        with FakeInstance(discovery, "a" * 32, os.getpid()) as older, \
                FakeInstance(discovery, "b" * 32, os.getpid()) as newer:
            older.activate(1000, "Earlier design")
            newer.activate(2000, "Last design")
            result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                    capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, b"")
            self.assertEqual((older.connections(), newer.connections()), (1, 2))
            self.assertIn(("connected to " + newer.instance + " (Last design), the window active last").encode(), result.stderr)
            self.assertIn(("others: " + older.instance + " (Earlier design)").encode(), result.stderr)
            self.assertNotIn(older.token.encode(), result.stderr)
            self.assertNotIn(newer.token.encode(), result.stderr)
            # Explicit selection still wins over the activation time.
            selected = subprocess.run([CLI, "mcp", "--connect", "--instance", older.instance],
                                      env=env, input=b"", capture_output=True, timeout=TIMEOUT)
            self.assertEqual(selected.returncode, 0, selected.stderr)
            self.assertEqual((older.connections(), newer.connections()), (2, 2))
        self.assertFalse(older.errors + newer.errors, older.errors + newer.errors)

    def test_connect_refuses_windows_without_activation_times(self):
        discovery, env = self.private_discovery()
        with FakeInstance(discovery, "a" * 32, os.getpid()) as first, \
                FakeInstance(discovery, "b" * 32, os.getpid()) as second:
            result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                    capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, b"")
            self.assertIn(b"Several MitCad instances are running; use --list-instances and --instance ID", result.stderr)
            self.assertEqual((first.connections(), second.connections()), (1, 1))
            # A state for another process must not turn a refusal into a choice.
            first.activate(3000, "Wrong process")
            state = first.record.with_suffix(".window")
            wrong = json.loads(state.read_text(encoding="utf-8"))
            wrong["pid"] = dead_pid()
            state.write_text(json.dumps(wrong), encoding="utf-8")
            result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                    capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 1)
            self.assertIn(b"Several MitCad instances", result.stderr)

    def test_records_are_probed_in_parallel(self):
        # mitcad#437: two running but unanswering instances cost one connect
        # timeout (5 s) in all; probed one after the other they took 10 s.
        discovery, env = self.private_discovery()
        with FakeInstance(discovery, "7" * 32, os.getpid()) as live, \
                FakeInstance(discovery, "8" * 32, os.getpid(), answer=False) as first, \
                FakeInstance(discovery, "9" * 32, os.getpid(), answer=False) as second:
            started = time.monotonic()
            result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                    capture_output=True, timeout=TIMEOUT)
            elapsed = time.monotonic() - started
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, b"")
            self.assertLess(elapsed, 9, "The discovery records were probed one after the other")
            self.assertNotIn(b"QSocketNotifier", result.stderr)
            self.assertEqual((live.connections(), first.connections(), second.connections()), (2, 1, 1))
        self.assertFalse(live.errors, live.errors)

    def test_connect_waits_for_busy_live_discovery(self):
        discovery = self.workspace / "discovery"
        discovery.mkdir(mode=0o700)
        make_private(discovery, True)
        instance, token = "1" * 32, "a" * 64
        errors = []
        accepted = []
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            listener.settimeout(3)
            record = discovery / (instance + ".json")
            record.write_text(json.dumps({"protocol": 1, "host": "127.0.0.1",
                                         "port": listener.getsockname()[1], "pid": os.getpid(),
                                         "instance": instance, "token": token}), encoding="utf-8")
            make_private(record, False)

            def serve():
                try:
                    for index in range(2):
                        connection, _ = listener.accept()
                        with connection:
                            connection.settimeout(5)
                            with connection.makefile("rb") as source:
                                auth = json.loads(source.readline())
                                if auth != {"mitcadAuth": token}:
                                    raise AssertionError("Unexpected authentication frame")
                                if index == 0:
                                    # A live GUI can be busy longer than the listing probe's 500 ms.
                                    time.sleep(0.8)
                                connection.sendall(b'{"authenticated":true}\n')
                                accepted.append(index)
                                if index == 1:
                                    source.read()
                except Exception as error:
                    errors.append(error)

            server = threading.Thread(target=serve, daemon=True)
            server.start()
            env = dict(os.environ, MITCAD_MCP_DISCOVERY_DIR=str(discovery))
            result = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                                    capture_output=True, timeout=TIMEOUT)
            server.join(timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(server.is_alive(), "Discovery server did not finish")
        self.assertFalse(errors, errors)
        self.assertEqual(accepted, [0, 1])
        self.assertEqual(result.stdout, b"")

    def test_connect_outlives_its_window(self):
        # mitcad#1288: the bridge started before MitCad, a window coming,
        # closing and another coming: the bridge stays up, answers tool calls
        # "not running" while no window is there, and connects again, giving
        # the new window the client's initialize, initialized and
        # subscription first.
        discovery, env = self.private_discovery()
        quiet = subprocess.run([CLI, "mcp", "--connect"], env=env, input=b"",
                               capture_output=True, timeout=TIMEOUT)
        self.assertEqual(quiet.returncode, 0, quiet.stderr)
        self.assertEqual(quiet.stdout, b"")
        self.assertIn(b"MitCad is not running", quiet.stderr)

        def call(client):
            response = client.request("tools/call", {"name": "document_list", "arguments": {}})
            self.assertNotIn("error", response)
            return response["result"]

        def offline(client, expected="MitCad is not running"):
            # Until the expected answer comes: a call the bridge sent to a
            # window that went (or refused this client) during it is
            # answered as cut short; a call made while no window is
            # connected, also before the bridge's next look finds a new
            # (refusing) window, as not running.
            deadline = time.monotonic() + TIMEOUT
            while True:
                result = call(client)
                self.assertTrue(result["isError"], result)
                text = result["content"][0]["text"]
                if expected in text:
                    return json.loads(text)
                self.assertTrue("ended during this call" in text or "MitCad is not running" in text, text)
                self.assertLess(time.monotonic(), deadline, text)
                time.sleep(0.2)

        def online(client, window):
            deadline = time.monotonic() + TIMEOUT
            while True:
                result = call(client)
                if not result.get("isError"):
                    self.assertEqual(result["structuredContent"], {"window": window.instance})
                    return
                self.assertLess(time.monotonic(), deadline, "The bridge did not connect to the window")
                time.sleep(0.2)

        with Session(self.workspace, connect=True, env=env) as client:
            # The bridge's own answers: initialize, the tool list, the reference.
            info = client.initialize()
            self.assertEqual(info["serverInfo"]["name"], "mitcad")
            tools = {tool["name"] for tool in client.request("tools/list")["result"]["tools"]}
            self.assertTrue({"document_list", "model_command", "ui_state", "model_reference"} <= tools, tools)
            reference = client.request("tools/call", {"name": "model_reference", "arguments": {}})["result"]
            self.assertFalse(reference.get("isError"), reference)
            self.assertIn("error", offline(client))
            self.assertEqual(client.request("resources/subscribe", {"uri": "mitcad://comments"})["result"], {})
            self.assertEqual(client.process.poll(), None)
            with FakeWindow(discovery, "c" * 32) as first:
                online(client, first)
                methods = first.methods()
                self.assertEqual(methods[:3], ["initialize", "notifications/initialized", "resources/subscribe"], methods)
                self.assertTrue(str(first.messages[0]["id"]).startswith("mitcad-bridge-"), first.messages[0])
                self.assertEqual(first.messages[0]["params"]["clientInfo"]["name"], "mitcad-integration-test")
                self.assertEqual(first.messages[2]["params"], {"uri": "mitcad://comments"})
                first.close()
                offline(client)
            self.assertFalse(first.errors, first.errors)
            self.assertEqual(client.process.poll(), None, "The bridge ended with its window")
            # A window that refuses this client (its assistant disconnected
            # there, mitcad#1289): its reason answers the calls.
            with FakeWindow(discovery, "e" * 32, refuse="Disconnected in MitCad") as refusing:
                offline(client, "Disconnected in MitCad")
            with FakeWindow(discovery, "d" * 32) as second:
                online(client, second)
                self.assertEqual(second.methods()[:3],
                                 ["initialize", "notifications/initialized", "resources/subscribe"])
                client.finish()
            self.assertFalse(refusing.errors + second.errors, refusing.errors + second.errors)

    def test_connect_ends_after_a_request_with_an_unusable_id(self):
        # mitcad#1317: the window answers a request whose id is neither a
        # string nor an integer with a null id; at the end of its input the
        # bridge still ends, after that one error.
        discovery, env = self.private_discovery()
        with FakeWindow(discovery, "f" * 32) as window:
            result = subprocess.run([CLI, "mcp", "--connect"], env=env,
                                    input=b'{"jsonrpc":"2.0","id":true,"method":"ping"}\n',
                                    capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 0, result.stderr)
            answers = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
            self.assertEqual(len(answers), 1, answers)
            self.assertIsNone(answers[0]["id"])
            self.assertIn("error", answers[0])
            self.assertEqual(window.methods(), ["ping"])
        self.assertFalse(window.errors, window.errors)

    def test_cli_arguments(self):
        for args in ([], ["--workspace"], ["--workspace", str(self.workspace / "missing")],
                     ["--workspace", str(self.workspace), "--unknown"],
                     ["--workspace", str(self.workspace), "--workspace", str(self.workspace)]):
            result = subprocess.run([CLI, "mcp"] + args, capture_output=True, timeout=TIMEOUT)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(result.stdout, b"")
            self.assertTrue(result.stderr)
        result = subprocess.run([CLI, "mcp", "--help"], capture_output=True, timeout=TIMEOUT)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(b"--workspace", result.stdout)

    def test_model_save_open_export_and_eof(self):
        with Session(self.workspace) as client:
            capabilities = client.initialize()["capabilities"]
            self.assertIn("tools", capabilities)
            self.assertIn("resources", capabilities)
            tools = client.request("tools/list")["result"]["tools"]
            self.assertEqual({tool["name"] for tool in tools}, TOOLS)
            resources = client.request("resources/list")["result"]["resources"]
            self.assertIn("mitcad://reference/commands", {item["uri"] for item in resources})
            reference = client.request("resources/read", {
                "uri": "mitcad://reference/commands"})["result"]["contents"]
            self.assertIn("add_feature", reference[0]["text"])
            self.assertEqual(client.request("resources/templates/list")["result"]["resourceTemplates"], [])
            first = client.tool("document_create", {})["document"]
            second = client.tool("document_create", {})["document"]
            self.assertNotEqual(first, second)
            listed = client.tool("document_list", {})["documents"]
            self.assertEqual({item["document"] for item in listed}, {first, second})
            client.tool("model_command", {"document": first, "command": BOX})
            bodies = client.tool("model_query", {
                "document": first, "query": {"query": "bodies", "properties": True}})["result"]
            self.assertEqual(len(bodies), 1)
            self.assertAlmostEqual(bodies[0]["volume"], 6000, places=5)
            empty = client.tool("model_query", {
                "document": second, "query": {"query": "bodies"}})["result"]
            self.assertEqual(empty, [])
            client.tool("document_close", {"document": first}, error=True)
            self.assertEqual(len(client.tool("document_list", {})["documents"]), 2)
            client.tool("document_save", {"document": first, "path": "box.mitcad"})
            self.assertTrue((self.workspace / "box.mitcad").is_file())
            client.tool("document_save", {"document": first, "path": "box.mitcad"}, error=True)
            client.tool("document_save", {"document": first, "path": "box.mitcad", "overwrite": True})
            client.tool("model_export", {"document": first, "path": "box.step", "format": "step"})
            self.assertGreater((self.workspace / "box.step").stat().st_size, 0)
            # A drawing (mitcad#471): PDF with the export_drawing options.
            drawing = client.tool("model_export", {
                "document": first, "path": "box.pdf", "format": "pdf",
                "drawing": {"sheet": "A3", "views": ["front", "top", "right", "iso"]}})["result"]
            self.assertEqual((drawing["pages"], drawing["views"], drawing["sheet"]), (1, 4, "A3"))
            self.assertTrue((self.workspace / "box.pdf").read_bytes().startswith(b"%PDF-1.4"))
            client.tool("model_export", {"document": first, "path": "box.svg", "format": "svg",
                                         "drawing": {"path": "other.svg"}}, error=True)
            self.assertFalse((self.workspace / "box.svg").exists())
            (self.workspace / "box.mtl").write_text("existing material\n", encoding="utf-8")
            client.tool("model_export", {"document": first, "path": "box.obj", "format": "obj"}, error=True)
            self.assertFalse((self.workspace / "box.obj").exists())
            self.assertEqual((self.workspace / "box.mtl").read_text(encoding="utf-8"), "existing material\n")
            self.assertTrue(client.tool("document_close", {"document": first})["closed"])
            client.tool("model_query", {"document": first, "query": {"query": "document"}}, error=True)
            reopened = client.tool("document_open", {"path": "box.mitcad"})["document"]
            self.assertNotEqual(reopened, first)
            bodies = client.tool("model_query", {
                "document": reopened, "query": {"query": "bodies", "properties": True}})["result"]
            self.assertAlmostEqual(bodies[0]["volume"], 6000, places=5)
            client.tool("document_close", {"document": reopened})
            client.tool("document_close", {"document": second, "discard": True})
            self.assertEqual(client.tool("document_list", {})["documents"], [])
            client.finish()

    def test_protocol_errors_and_notifications_do_not_mutate(self):
        with Session(self.workspace) as client:
            self.assertIn("error", client.request("tools/list"))
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.notify("tools/call", {"name": "model_command", "arguments": {
                "document": document, "command": BOX}})
            client.notify("tools/call", {"name": "document_create", "arguments": {}})
            client.notify("notifications/cancelled", {"requestId": 999, "reason": "test"})
            bodies = client.tool("model_query", {
                "document": document, "query": {"query": "bodies"}})["result"]
            self.assertEqual(bodies, [])
            self.assertEqual(len(client.tool("document_list", {})["documents"]), 1)
            client.send_raw(b"{bad json")
            self.assertEqual(client.receive()["error"]["code"], -32700)
            client.send({"jsonrpc": "2.0", "id": 10000, "method": 4})
            self.assertEqual(client.receive()["error"]["code"], -32600)
            self.assertEqual(client.request("unknown/method")["error"]["code"], -32601)
            self.assertIn("error", client.request("tools/call", {"name": "model_command", "arguments": {
                "document": document, "command": "not an object"}}))
            client.tool("model_command", {"document": document, "command": {"cmd": "unknown"}}, error=True)
            state = client.tool("model_query", {
                "document": document, "query": {"query": "document"}})["result"]
            self.assertEqual(state["features"], 0)
            client.finish()

    def test_workspace_paths_and_read_only(self):
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})
            for path in ("../escape.mitcad", str(self.workspace.parent / "escape.mitcad")):
                client.tool("document_save", {"document": document, "path": path}, error=True)
            client.tool("document_open", {"path": "../escape.mitcad"}, error=True)
            client.tool("model_command", {"document": document, "command": {
                "cmd": "export", "path": "escaped.step"}}, error=True)
            client.tool("model_export", {"document": document, "path": "../escape.step", "format": "step"}, error=True)
            client.tool("document_save", {"document": document, "path": "box.mitcad"})
            client.finish()
        self.assertFalse((self.workspace.parent / "escape.mitcad").exists())
        with Session(self.workspace, read_only=True) as client:
            client.initialize()
            client.tool("document_create", {}, error=True)
            document = client.tool("document_open", {"path": "box.mitcad"})["document"]
            bodies = client.tool("model_query", {
                "document": document, "query": {"query": "bodies"}})["result"]
            self.assertEqual(len(bodies), 1)
            client.tool("model_command", {"document": document, "command": BOX}, error=True)
            client.tool("document_save", {"document": document, "path": "forbidden.mitcad"}, error=True)
            client.tool("model_export", {"document": document, "path": "forbidden.step", "format": "step"}, error=True)
            client.tool("document_close", {"document": document})
            client.finish()
        self.assertFalse((self.workspace / "forbidden.mitcad").exists())
        self.assertFalse((self.workspace / "forbidden.step").exists())

    def test_autoselect_profiles_are_available_in_read_only_documents(self):
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": {
                "cmd": "sketch.create"}})
            client.tool("model_command", {"document": document, "command": {
                "cmd": "sketch.add_circle", "sketch": "F1",
                "center": [0, 0], "diameter": 10}})
            before = client.tool("model_query", {"document": document,
                "query": {"query": "autoselect_profiles"}})["result"]
            self.assertTrue(before["eligible"])
            self.assertEqual(before["sketch"], "F1")
            self.assertEqual(len(before["drawn"]), 1)
            self.assertEqual(before["all"], before["drawn"])
            client.tool("document_save", {"document": document, "path": "sketch.mitcad"})
            client.finish()
        with Session(self.workspace, read_only=True) as client:
            client.initialize()
            document = client.tool("document_open", {"path": "sketch.mitcad"})["document"]
            after = client.tool("model_query", {"document": document,
                "query": {"query": "autoselect_profiles"}})["result"]
            self.assertEqual(after, before)
            client.finish()

    def test_geometry_failure_is_applied_and_can_be_undone(self):
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})
            # Positive sizes pass definition validation. The cut fails only
            # during evaluation because it does not touch its participant.
            cut = {"cmd": "add_feature", "def": {
                "type": "box", "plane": {"type": "origin", "plane": "xy"},
                "corner": [100, 100], "length": 5, "width": 5, "height": 5,
                "operation": "cut", "participants": ["F1.b0"]}}
            failed = client.tool("model_command", {"document": document, "command": cut}, error=True)
            self.assertTrue(failed["applied"])
            timeline = client.tool("model_query", {
                "document": document, "query": {"query": "timeline"}})["result"]
            self.assertEqual(len(timeline["features"]), 2)
            self.assertEqual(timeline["features"][1]["status"], "error")
            self.assertIn("cut into", timeline["features"][1]["error"])
            client.tool("model_export", {"document": document,
                        "path": "failed.step", "format": "step"}, error=True)
            self.assertFalse((self.workspace / "failed.step").exists())
            client.tool("model_command", {"document": document, "command": {"cmd": "undo"}})
            timeline = client.tool("model_query", {
                "document": document, "query": {"query": "timeline"}})["result"]
            self.assertEqual(len(timeline["features"]), 1)
            self.assertEqual(timeline["features"][0]["status"], "ok")
            client.tool("document_close", {"document": document, "discard": True})
            client.finish()

    def test_sheet_metal_and_flat_pattern_dxf(self):
        # Sheet-metal features, picks and the Flat Pattern DXF (mitcad#210).
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            for command in (
                    {"cmd": "sketch.create"},
                    {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 20},
                    {"cmd": "add_feature", "def": {
                        "type": "sheet_face", "profiles": [
                            {"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
                        "rules": {"thickness": 1, "bend_radius": 1, "allowance": {"type": "k_factor", "k": 0.44},
                                  "reference_side": "front", "bend_relief": {"type": "round", "width": 1, "depth": 3},
                                  "corner_relief": {"type": "round", "width": 1, "depth": 3}}}},
                    {"cmd": "set_flat_pattern", "body": "F3.b0"}):
                client.tool("model_command", {"document": document, "command": command})
            bodies = client.tool("model_query", {
                "document": document, "query": {"query": "sheet_bodies"}})["result"]
            self.assertEqual(bodies[0]["flat_pattern"]["status"], "valid")
            picked = client.tool("model_query", {"document": document, "query": {
                "query": "sheet_reference", "body": "F3.b0", "face": "F3:sheet_front(0)"}})["result"]
            self.assertEqual(picked["domain"]["kind"], "panel")
            client.tool("model_export", {"document": document, "path": "flat.dxf", "format": "dxf",
                                         "flat_pattern": "F3.b0"})
            self.assertIn("CUT_OUTER", (self.workspace / "flat.dxf").read_text(encoding="ascii"))
            client.tool("model_export", {"document": document, "path": "both.dxf", "format": "dxf",
                                         "flat_pattern": "F3.b0", "sketch": "F1"}, error=True)
            client.tool("document_close", {"document": document, "discard": True})
            client.finish()

    def test_symlink_cannot_leave_workspace(self):
        with tempfile.TemporaryDirectory(prefix="outside-", dir=WORK) as outside:
            link = self.workspace / "outside"
            try:
                os.symlink(outside, link, target_is_directory=True)
            except (OSError, NotImplementedError) as error:
                self.skipTest("directory symlinks unavailable: " + str(error))
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                client.tool("document_save", {"document": document,
                            "path": "outside/escape.mitcad"}, error=True)
                self.assertFalse((Path(outside) / "escape.mitcad").exists())
                outside_file = Path(outside) / "existing.mitcad"
                outside_file.write_text("untouched", encoding="utf-8")
                os.symlink(outside_file, self.workspace / "existing.mitcad")
                os.symlink(Path(outside) / "missing.mitcad", self.workspace / "dangling.mitcad")
                for path in ("existing.mitcad", "dangling.mitcad"):
                    client.tool("document_save", {"document": document, "path": path,
                                "overwrite": True}, error=True)
                    client.tool("document_open", {"path": path}, error=True)
                temporary = self.workspace / (".mitcad-mcp.%d.0.tmp" % client.process.pid)
                os.symlink(outside_file, temporary)
                client.tool("document_save", {"document": document, "path": "safe.mitcad"})
                self.assertTrue((self.workspace / "safe.mitcad").is_file())
                self.assertTrue(temporary.is_symlink())
                self.assertEqual(outside_file.read_text(encoding="utf-8"), "untouched")
                # Preflight must refuse the store path before parsing these
                # synthetic bytes as B-rep data; no third-party model needed.
                project = self.workspace / ".mitcad"
                project.mkdir()
                (project / "project.json").write_text("{}", encoding="utf-8")
                digest = "a" * 64
                (Path(outside) / "aa").mkdir()
                (Path(outside) / "aa" / (digest + ".brep.zlib")).write_bytes(b"synthetic")
                os.symlink(outside, project / "brep", target_is_directory=True)
                fixture = {"format": "mitcad", "version": 3, "features": [
                    {"type": "base", "bodies": [{"brep": {"format": "occt",
                     "compression": "zlib", "size": 9, "sha256": digest}}]}]}
                (self.workspace / "outside-store.mitcad").write_text(json.dumps(fixture), encoding="utf-8")
                error = client.tool("document_open", {"path": "outside-store.mitcad"}, error=True)
                self.assertIn("leaves the MCP workspace", error["error"])
                self.assertEqual(len(client.tool("document_list", {})["documents"]), 1)
                client.finish()

    def test_legacy_version_negotiation(self):
        with Session(self.workspace) as client:
            self.assertEqual(client.request("server/discover")["error"]["code"], -32601)
            client.initialize("2024-11-05")
            self.assertIn("result", client.request("ping"))
            client.finish()

    def test_font_files_in_definitions_and_opened_designs_are_refused(self):
        with tempfile.TemporaryDirectory(prefix="outside-font-", dir=WORK) as outside:
            font = Path(outside) / "font.ttf"
            font.write_bytes(b"must not be parsed as a font")
            text = {"id": "t1", "text": "A", "at": [0, 0], "height": 10,
                    "font_file": {"path": str(font), "index": 0, "licence": ""}}
            definition = {"type": "sketch", "plane": "xy", "texts": [text]}
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                client.tool("model_command", {"document": document,
                            "command": {"cmd": "sketch.create"}})
                for command in ("add_feature", "edit_feature"):
                    refused = client.tool("model_command", {"document": document,
                        "command": {"cmd": command, "uid": "F1", "def": definition}}, error=True)
                    self.assertNotIn("applied", refused)
                    self.assertIn("not available through MCP", refused["error"])
                timeline = client.tool("model_query", {"document": document,
                    "query": {"query": "timeline"}})["result"]
                self.assertEqual(len(timeline["features"]), 1)
                self.assertNotIn("texts", timeline["features"][0].get("def", {}))
                fixture = {"format": "mitcad", "version": 2, "parameters": [],
                           "features": [{**definition, "uid": "F1", "name": "Sketch1"}]}
                for stored in (False, True):
                    if stored:
                        text["glyphs"] = {"text": "A", "font": "", "glyphs": [{}]}
                    (self.workspace / "font.mitcad").write_text(json.dumps(fixture), encoding="utf-8")
                    refused = client.tool("document_open", {"path": "font.mitcad"}, error=True)
                    self.assertIn("not available through MCP", refused["error"])
                self.assertEqual(len(client.tool("document_list", {})["documents"]), 1)
                self.assertEqual(font.read_bytes(), b"must not be parsed as a font")
                client.finish()

    def test_usdz_never_reads_external_texture_files(self):
        image = base64.b64decode(
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l1sAAAAASUVORK5CYII=")
        with tempfile.TemporaryDirectory(prefix="outside-image-", dir=WORK) as outside:
            texture = Path(outside) / "texture.png"
            texture.write_bytes(image)
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                client.tool("model_command", {"document": document, "command": BOX})
                client.tool("document_save", {"document": document, "path": "plain.mitcad"})
                fixture = json.loads((self.workspace / "plain.mitcad").read_text(encoding="utf-8"))
                fixture["appearances"] = [{"id": "external", "name": "External texture",
                                           "texture": {"path": str(texture)}}]
                attrs = fixture.setdefault("bodies", [])
                existing = next((body for body in attrs if body["uid"] == "F1.b0"), None)
                if existing is None:
                    attrs.append({"uid": "F1.b0", "name": "Body1", "appearance": "external"})
                else:
                    existing["appearance"] = "external"
                for name, path in (("absolute", str(texture)), ("parent", "../" + texture.parent.name + "/texture.png")):
                    fixture["appearances"][0]["texture"]["path"] = path
                    (self.workspace / "texture.mitcad").write_text(json.dumps(fixture), encoding="utf-8")
                    opened = client.tool("document_open", {"path": "texture.mitcad"})["document"]
                    refused = client.tool("model_export", {"document": opened,
                        "path": name + ".usdz", "format": "usdz"}, error=True)
                    self.assertIn("embedded texture", refused["error"])
                    self.assertFalse((self.workspace / (name + ".usdz")).exists())
                    client.tool("document_close", {"document": opened})
                fixture["appearances"][0]["texture"]["data"] = base64.b64encode(image).decode("ascii")
                (self.workspace / "embedded.mitcad").write_text(json.dumps(fixture), encoding="utf-8")
                opened = client.tool("document_open", {"path": "embedded.mitcad"})["document"]
                client.tool("model_export", {"document": opened, "path": "embedded.usdz", "format": "usdz"})
                self.assertTrue((self.workspace / "embedded.usdz").is_file())
                self.assertEqual(texture.read_bytes(), image)
                client.finish()

    def test_every_drawing_output_is_checked_before_any_write(self):
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})
            second = copy.deepcopy(BOX)
            second["def"]["corner"] = [50, 0]
            client.tool("model_command", {"document": document, "command": second})
            for format in ("svg", "dxf"):
                following = self.workspace / ("drawing-2." + format)
                following.write_bytes(b"existing drawing")
                client.tool("model_export", {"document": document, "path": "drawing." + format,
                    "format": format, "drawing": {"per_body": True, "views": ["front"]}}, error=True)
                self.assertEqual(following.read_bytes(), b"existing drawing")
                self.assertFalse((self.workspace / ("drawing." + format)).exists())
                following.unlink()
            with tempfile.TemporaryDirectory(prefix="outside-drawing-", dir=WORK) as outside:
                target = Path(outside) / "untouched.svg"
                target.write_bytes(b"outside drawing")
                try:
                    os.symlink(target, self.workspace / "drawing-2.svg")
                except (OSError, NotImplementedError) as error:
                    self.skipTest("file symlinks unavailable: " + str(error))
                client.tool("model_export", {"document": document, "path": "drawing.svg",
                    "format": "svg", "drawing": {"per_body": True, "views": ["front"]}}, error=True)
                self.assertEqual(target.read_bytes(), b"outside drawing")
                self.assertFalse((self.workspace / "drawing.svg").exists())
                (self.workspace / "drawing-2.svg").unlink()
            result = client.tool("model_export", {"document": document, "path": "drawing.svg",
                "format": "svg", "drawing": {"per_body": True, "views": ["front"]}})["result"]
            self.assertEqual(result["sheets"], 2)
            self.assertEqual(len(result["files"]), 2)
            client.finish()

    def print_import_state(self, client, document):
        """Document and imported items: a refused preflight must change neither."""
        return {name: client.tool("model_query", {
            "document": document, "query": {"query": name}})["result"]
                for name in ("document", "print_profiles", "materials", "bodies")}

    def test_workspace_print_settings_are_used_by_plate_export(self):
        """mitcad#1570: all three imported layers reach the actual 3MF output."""
        source = self.workspace / "profiles" / "source.3mf"
        expected = print_project(source)
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            imported = client.tool("model_command", {"document": document, "command": {
                "cmd": "import_print_profile", "path": "profiles/source.3mf",
                "into": "design", "dialect": "orcaslicer"}})["result"]
            for key, name in (("printers", "Fixture printer"),
                              ("processes", "Fixture process"), ("materials", "Fixture PLA")):
                self.assertEqual([item["name"] for item in imported[key]], [name], imported)
            self.assertTrue(imported["chosen"], imported)
            client.tool("model_command", {"document": document, "command": BOX})
            client.tool("model_command", {"document": document, "command": [
                {"cmd": "set_body_material", "uid": "F1.b0", "material": imported["materials"][0]["id"]},
                {"cmd": "add_print_plate", "name": "Fixture plate"},
                {"cmd": "place_on_plate", "plate": "Fixture plate", "items": [{"body": "F1.b0"}]}]})
            exported = client.tool("model_export", {
                "document": document, "path": "imported.3mf", "format": "3mf",
                "plates": "one_file", "dialect": "orcaslicer"})["result"]
            self.assertEqual(exported["dialect"], "orcaslicer")
            self.assertEqual(exported["plates"][0]["pieces"], 1)
            with zipfile.ZipFile(self.workspace / "imported.3mf") as package:
                written = json.loads(package.read("Metadata/project_settings.config"))
            self.assertEqual(written["printer_settings_id"], expected["printer_settings_id"])
            self.assertEqual(written["print_settings_id"], expected["print_settings_id"])
            self.assertAlmostEqual(float(written["nozzle_diameter"][0]), 0.6)
            self.assertAlmostEqual(float(written["layer_height"]), 0.28)
            self.assertEqual(written["filament_settings_id"], expected["filament_settings_id"])
            self.assertAlmostEqual(float(written["nozzle_temperature"][0]), 223)
            client.finish()

    def test_workspace_slicer_presets_keep_confined_inherited_values(self):
        root = self.workspace / "presets"
        choices = slicer_fixture(root)
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            imported = client.tool("model_command", {"document": document, "command": {
                "cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                "presets": [{"path": str(path.relative_to(self.workspace))} for path in choices],
                "into": "design"}})["result"]
            self.assertEqual(len(imported["printers"]), 1, imported)
            self.assertEqual(len(imported["processes"]), 1, imported)
            self.assertEqual(len(imported["materials"]), 1, imported)
            profiles = client.tool("model_query", {"document": document,
                "query": {"query": "print_profiles"}})["result"]
            printer = next(item for item in profiles if item["name"] == "Fixture printer")
            process = next(item for item in profiles if item["name"] == "Fixture process")
            self.assertAlmostEqual(printer["values"]["nozzle_diameter"], 0.6)
            self.assertAlmostEqual(printer["values"]["max_print_height"], 190)
            self.assertEqual(printer["values"]["bed_shape"], [[0, 0], [180, 0], [180, 180], [0, 180]])
            self.assertAlmostEqual(process["values"]["layer_height"], 0.28)
            materials = client.tool("model_query", {"document": document,
                "query": {"query": "materials"}})["result"]
            material = next(item for item in materials if item["name"] == "Fixture PLA")
            self.assertAlmostEqual(material["density"], 1.24)
            self.assertAlmostEqual(material["filament"]["temperature"], 228)
            client.finish()

    def test_profile_import_inheritance_needs_a_confined_source_tree(self):
        root = self.workspace / "presets"
        choices = slicer_fixture(root)
        standalone = self.workspace / "standalone.json"
        standalone.write_text(json.dumps({"type": "machine", "name": "Standalone child",
                                         "inherits": "External base", "nozzle_diameter": ["0.6"]}),
                              encoding="utf-8")
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            before = self.print_import_state(client, document)
            client.tool("model_command", {"document": document, "command": {
                "cmd": "import_print_profile", "path": "standalone.json", "into": "design"}}, error=True)
            self.assertEqual(self.print_import_state(client, document), before)
            # The same import API may resolve a known tree inside the workspace.
            imported = client.tool("model_command", {"document": document, "command": {
                "cmd": "import_print_profile", "path": str(choices[0].relative_to(self.workspace)),
                "into": "design", "dialect": "orcaslicer"}})["result"]
            self.assertEqual([item["name"] for item in imported["printers"]], ["Fixture printer"])
            profiles = client.tool("model_query", {"document": document,
                "query": {"query": "print_profiles"}})["result"]
            self.assertAlmostEqual(profiles[0]["values"]["nozzle_diameter"], 0.6)
            # A missing parent must be refused, not accepted as a partial profile.
            child = json.loads(choices[0].read_text(encoding="utf-8"))
            child["inherits"] = "Missing system base"
            choices[0].write_text(json.dumps(child), encoding="utf-8")
            before = self.print_import_state(client, document)
            for command in (
                    {"cmd": "import_print_profile", "path": str(choices[0]), "into": "design"},
                    {"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                     "presets": [{"path": str(choices[1])}, {"path": str(choices[0])}], "into": "design"}):
                client.tool("model_command", {"document": document, "command": command}, error=True)
                self.assertEqual(self.print_import_state(client, document), before)
            client.finish()

    def test_profile_import_paths_and_arrays_are_preflighted(self):
        """A bad later source must not import earlier settings or create a body."""
        source = self.workspace / "profiles" / "source.3mf"
        print_project(source)
        with tempfile.TemporaryDirectory(prefix="outside-profile-", dir=WORK) as outside:
            outside_file = Path(outside) / "outside.3mf"
            print_project(outside_file)
            outside_before = outside_file.read_bytes()
            traversal = os.path.relpath(outside_file, self.workspace)
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                for path in (str(outside_file), traversal, "profiles", "missing.3mf"):
                    with self.subTest(path=path):
                        refused = client.tool("model_command", {"document": document, "command": [
                            {"cmd": "import_print_profile", "path": "profiles/source.3mf"},
                            BOX, {"cmd": "import_print_profile", "path": path}]}, error=True)
                        self.assertNotEqual(refused.get("applied"), True, refused)
                        self.assertEqual(self.print_import_state(client, document), before)
                client.finish()
            self.assertEqual(outside_file.read_bytes(), outside_before)

    def test_print_import_array_has_a_shared_source_budget(self):
        """Small valid files cannot bypass the call's budget through repetition."""
        source = self.workspace / "padded.json"
        source.write_text(json.dumps({
            "type": "process", "name": "Padded process", "layer_height": "0.28",
            "notes": "A" * (768 * 1024)}), encoding="utf-8")
        small = self.workspace / "small.json"
        small.write_text(json.dumps({"type": "process", "name": "Follow-up process",
                                    "layer_height": "0.2"}), encoding="utf-8")
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            before = self.print_import_state(client, document)
            command = {"cmd": "import_print_profile", "path": "padded.json",
                       "dialect": "orcaslicer", "into": "design", "choose": False}
            refused = client.tool("model_command", {"document": document,
                "command": [command] * 100}, error=True)
            self.assertIn("limit", refused["error"].lower(), refused)
            self.assertNotEqual(refused.get("applied"), True, refused)
            self.assertEqual(self.print_import_state(client, document), before)
            # Refusal releases the per-call budget and preserves writability.
            imported = client.tool("model_command", {"document": document, "command": {
                "cmd": "import_print_profile", "path": "small.json",
                "dialect": "orcaslicer", "into": "design"}})["result"]
            self.assertEqual([item["name"] for item in imported["processes"]], ["Follow-up process"])
            profiles = client.tool("model_query", {"document": document,
                "query": {"query": "print_profiles"}})["result"]
            self.assertEqual([item["name"] for item in profiles], ["Follow-up process"])
            self.assertEqual(client.tool("model_query", {"document": document,
                "query": {"query": "document"}})["result"]["undo_depth"],
                before["document"]["undo_depth"] + 1)
            client.finish()

    def test_print_imports_never_write_user_or_project_libraries(self):
        """Both native import commands are confined to the current design."""
        print_project(self.workspace / "source.3mf")
        choices = slicer_fixture(self.workspace / "presets")
        with tempfile.TemporaryDirectory(prefix="profile-libraries-", dir=WORK) as outside:
            user = Path(outside) / "user-library"
            project = Path(outside) / "project"
            for folder in (user, project / "print-library"):
                folder.mkdir(parents=True)
                (folder / "sentinel.txt").write_bytes(b"keep the existing library\n")
            def snapshot():
                return {str(path.relative_to(outside)): path.read_bytes()
                        for path in Path(outside).rglob("*") if path.is_file()}
            original = snapshot()
            env = dict(os.environ, MITCAD_PRINT_LIBRARY_DIR=str(user))
            with Session(self.workspace, env=env) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                commands = [
                    {"cmd": "import_print_profile", "path": "source.3mf"},
                    {"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                     "presets": [{"path": str(path)} for path in choices]}]
                for command in commands:
                    for changes in ({"into": "user"}, {"into": "project", "project": str(project)},
                                    {"into": "design", "project": str(project)}):
                        with self.subTest(command=command["cmd"], target=changes):
                            client.tool("model_command", {"document": document,
                                "command": dict(command, **changes)}, error=True)
                            self.assertEqual(self.print_import_state(client, document), before)
                            self.assertEqual(snapshot(), original)
                client.finish()

    def test_print_imports_respect_read_only_documents(self):
        print_project(self.workspace / "source.3mf")
        choices = slicer_fixture(self.workspace / "presets")
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("document_save", {"document": document, "path": "read-only.mitcad"})
            client.finish()
        with Session(self.workspace, read_only=True) as client:
            client.initialize()
            document = client.tool("document_open", {"path": "read-only.mitcad"})["document"]
            before = self.print_import_state(client, document)
            for command in (
                    {"cmd": "import_print_profile", "path": "source.3mf", "into": "design"},
                    {"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                     "presets": [{"path": str(path)} for path in choices], "into": "design"}):
                client.tool("model_command", {"document": document, "command": command}, error=True)
                self.assertEqual(self.print_import_state(client, document), before)
            client.finish()

    def test_profile_import_symlinks_cannot_escape_the_workspace(self):
        with tempfile.TemporaryDirectory(prefix="outside-profile-", dir=WORK) as outside:
            outside_file = Path(outside) / "source.3mf"
            print_project(outside_file)
            try:
                os.symlink(outside_file, self.workspace / "linked.3mf")
                os.symlink(outside, self.workspace / "linked-folder", target_is_directory=True)
            except (OSError, NotImplementedError) as error:
                self.skipTest("symlinks unavailable: " + str(error))
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                for path in ("linked.3mf", "linked-folder/source.3mf"):
                    client.tool("model_command", {"document": document, "command": {
                        "cmd": "import_print_profile", "path": path, "into": "design"}}, error=True)
                    self.assertEqual(self.print_import_state(client, document), before)
                client.finish()

    def test_slicer_presets_cannot_read_outside_choices_or_nested_sources(self):
        root = self.workspace / "presets"
        choices = slicer_fixture(root)
        with tempfile.TemporaryDirectory(prefix="outside-presets-", dir=WORK) as outside:
            outside_choices = slicer_fixture(Path(outside))
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                for folder, selected in ((str(outside), outside_choices),
                                         ("presets", [choices[1], outside_choices[0]]),
                                         ("presets", [Path(os.path.relpath(outside_choices[0], self.workspace))])):
                    client.tool("model_command", {"document": document, "command": {
                        "cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": folder,
                        "presets": [{"path": str(path)} for path in selected], "into": "design"}}, error=True)
                    self.assertEqual(self.print_import_state(client, document), before)
                client.finish()
            inherited = root / "system" / "Fixture" / "machine" / "Base.json"
            inherited.unlink()
            try:
                os.symlink(Path(outside) / "system" / "Fixture" / "machine" / "Base.json", inherited)
            except (OSError, NotImplementedError) as error:
                self.skipTest("symlinks unavailable: " + str(error))
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                for command in (
                        {"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                         "presets": [{"path": str(path)} for path in choices], "into": "design"},
                        {"cmd": "import_print_profile", "path": str(choices[0]), "into": "design"}):
                    client.tool("model_command", {"document": document, "command": command}, error=True)
                    self.assertEqual(self.print_import_state(client, document), before)
                client.finish()

    def test_slicer_preset_inherited_directories_cannot_leave_workspace(self):
        root = self.workspace / "presets"
        choices = slicer_fixture(root)
        with tempfile.TemporaryDirectory(prefix="outside-presets-", dir=WORK) as outside:
            slicer_fixture(Path(outside))
            machine = root / "system" / "Fixture" / "machine"
            for child in machine.iterdir():
                child.unlink()
            machine.rmdir()
            try:
                os.symlink(Path(outside) / "system" / "Fixture" / "machine", machine,
                           target_is_directory=True)
            except (OSError, NotImplementedError) as error:
                self.skipTest("directory symlinks unavailable: " + str(error))
            with Session(self.workspace) as client:
                client.initialize()
                document = client.tool("document_create", {})["document"]
                before = self.print_import_state(client, document)
                for command in (
                        {"cmd": "import_print_profile", "path": str(choices[0]), "into": "design"},
                        {"cmd": "import_slicer_presets", "dialect": "orcaslicer", "folder": "presets",
                         "presets": [{"path": str(path)} for path in choices], "into": "design"}):
                    client.tool("model_command", {"document": document, "command": command}, error=True)
                    self.assertEqual(self.print_import_state(client, document), before)
                client.finish()

    def test_place_on_plate_reports_the_plate_an_item_left(self):
        """mitcad#1595: a moved item names the plate it left; undo puts it back."""
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})
            second = copy.deepcopy(BOX)
            second["def"]["corner"] = [50, 0]
            client.tool("model_command", {"document": document, "command": second})
            placed = client.tool("model_command", {"document": document, "command": [
                {"cmd": "add_print_plate", "name": "one"},
                {"cmd": "add_print_plate", "name": "two"},
                {"cmd": "place_on_plate", "plate": "one",
                 "items": [{"body": "Body1", "count": 2}, {"body": "Body2"}]}]})
            # The fields the moves own; the result also carries the command
            # envelope's (error, recomputed), and its notices repeat the warnings.
            def moves_of(result):
                notices = [(notice["level"], notice["message"]) for notice in result.get("notices", [])]
                self.assertEqual(notices, [("warning", text) for text in result["warnings"]])
                return {"moved": result["moved"], "warnings": result["warnings"]}

            self.assertEqual(moves_of(placed["result"]), {"moved": [], "warnings": []})
            moved = client.tool("model_command", {"document": document, "command": {
                "cmd": "place_on_plate", "plate": "two", "items": [{"body": "Body1"}]}})
            self.assertTrue(moved["applied"])
            self.assertEqual(moves_of(moved["result"]), {
                "moved": [{"body": "F1.b0", "name": "Body1", "count": 2,
                           "from": {"id": "plate1", "name": "one", "count": 2},
                           "to": {"id": "plate2", "name": "two"}}],
                "warnings": ["Body1 moved from one to two (count 2)"]})

            def items():
                plates = client.tool("model_query", {
                    "document": document, "query": {"query": "print_plates"}})["result"]["plates"]
                return [[(item["name"], item["count"]) for item in plate["items"]] for plate in plates]

            self.assertEqual(items(), [[("Body2", 1)], [("Body1", 2)]])
            client.tool("model_command", {"document": document, "command": {"cmd": "undo"}})
            self.assertEqual(items(), [[("Body1", 2), ("Body2", 1)], []])
            client.tool("model_command", {"document": document, "command": {"cmd": "redo"}})
            self.assertEqual(items(), [[("Body2", 1)], [("Body1", 2)]])
            client.finish()

    def test_print_plates_are_exported_with_their_files_checked(self):
        """mitcad#1490: a plate per box, one file or a file per plate."""
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})
            second = copy.deepcopy(BOX)
            second["def"]["corner"] = [50, 0]
            client.tool("model_command", {"document": document, "command": second})
            client.tool("model_command", {"document": document, "command": [
                {"cmd": "add_print_plate", "name": "Red"},
                {"cmd": "add_print_plate", "name": "Blue"},
                {"cmd": "place_on_plate", "plate": "Red", "items": [{"body": "F1.b0"}]},
                {"cmd": "place_on_plate", "plate": "Blue", "items": [{"body": "F2.b0"}]}]})
            # Every plate in one file for a Bambu-family slicer.
            result = client.tool("model_export", {
                "document": document, "path": "kit.3mf", "format": "3mf",
                "plates": "one_file", "dialect": "bambu_studio"})["result"]
            self.assertEqual([plate["name"] for plate in result["plates"]], ["Red", "Blue"])
            for plate in result["plates"]:
                self.assertEqual(Path(plate["path"]).name, "kit.3mf")
                self.assertEqual(plate["pieces"], 1)
                self.assertIn("origin", plate)
                self.assertIn("bed", plate)
                self.assertIn("warnings", plate)
            self.assertEqual([Path(path).name for path in result["files"]], ["kit.3mf"])
            self.assertEqual(result["dialect"], "bambu_studio")
            self.assertTrue((self.workspace / "kit.3mf").is_file())
            # The plates go into 3MF files only; bodies are the plates' to name.
            refused = client.tool("model_export", {
                "document": document, "path": "kit.stl", "format": "stl", "plates": "one_file"},
                error=True)
            self.assertIn("3MF", refused["error"])
            self.assertFalse((self.workspace / "kit.stl").exists())
            client.tool("model_export", {
                "document": document, "path": "bodies.3mf", "format": "3mf",
                "plates": "one_file", "bodies": ["F1.b0"]}, error=True)
            self.assertFalse((self.workspace / "bodies.3mf").exists())
            # A file per plate: refused while one of them exists, nothing
            # written; then one file per plate beside the path.
            blue = self.workspace / "cura - Blue.3mf"
            blue.write_bytes(b"existing plate")
            refused = client.tool("model_export", {
                "document": document, "path": "cura.3mf", "format": "3mf",
                "plates": "file_per_plate", "dialect": "cura"}, error=True)
            self.assertIn("already exists", refused["error"])
            self.assertEqual(blue.read_bytes(), b"existing plate")
            self.assertFalse((self.workspace / "cura - Red.3mf").exists())
            blue.unlink()
            result = client.tool("model_export", {
                "document": document, "path": "cura.3mf", "format": "3mf",
                "plates": "file_per_plate", "dialect": "cura", "spacing": 8})["result"]
            self.assertEqual(sorted(Path(path).name for path in result["files"]),
                             ["cura - Blue.3mf", "cura - Red.3mf"])
            for name in ("cura - Red.3mf", "cura - Blue.3mf"):
                self.assertTrue((self.workspace / name).is_file())
            self.assertFalse((self.workspace / "cura.3mf").exists())
            # Again over the same plate files: only with overwrite (mitcad#1569).
            arguments = {"document": document, "path": "cura.3mf", "format": "3mf",
                         "plates": "file_per_plate", "dialect": "cura"}
            refused = client.tool("model_export", arguments, error=True)
            self.assertIn("overwrite", refused["error"])
            result = client.tool("model_export", dict(arguments, overwrite=True))["result"]
            self.assertEqual(sorted(Path(path).name for path in result["files"]),
                             ["cura - Blue.3mf", "cura - Red.3mf"])
            client.tool("document_close", {"document": document, "discard": True})
            client.finish()

    def test_export_refinement_and_overwrite(self):
        """mitcad#1569: the mesh's tolerances, and replacing existing files."""
        def triangles(name):
            # A binary STL: an 80-byte header, then the triangles' count.
            return struct.unpack_from("<I", (self.workspace / name).read_bytes(), 80)[0]

        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": CYLINDER})
            # A finer mesh than the default for a small printed part.
            client.tool("model_export", {"document": document, "path": "default.stl", "format": "stl"})
            client.tool("model_export", {"document": document, "path": "fine.stl", "format": "stl",
                                         "refinement": {"deviation": 0.03, "angle": 0.052}})
            self.assertGreater(triangles("fine.stl"), triangles("default.stl"))
            client.tool("model_export", {"document": document, "path": "high.obj", "format": "obj",
                                         "refinement": "high"})
            self.assertTrue((self.workspace / "high.obj").is_file())
            # export checks the values; the mesh formats only.
            client.tool("model_export", {"document": document, "path": "coarse.stl", "format": "stl",
                                         "refinement": "coarse"}, error=True)
            self.assertFalse((self.workspace / "coarse.stl").exists())
            refused = client.tool("model_export", {"document": document, "path": "mesh.step",
                                                   "format": "step", "refinement": "high"}, error=True)
            self.assertIn("refinement", refused["error"])
            self.assertFalse((self.workspace / "mesh.step").exists())
            # An existing file: refused without overwrite, replaced with it.
            existing = self.workspace / "part.stl"
            existing.write_bytes(b"existing part")
            refused = client.tool("model_export", {"document": document, "path": "part.stl",
                                                   "format": "stl"}, error=True)
            self.assertIn("model_export requires overwrite", refused["error"])
            self.assertEqual(existing.read_bytes(), b"existing part")
            client.tool("model_export", {"document": document, "path": "part.stl", "format": "stl",
                                         "overwrite": False}, error=True)
            self.assertEqual(existing.read_bytes(), b"existing part")
            exported = client.tool("model_export", {"document": document, "path": "part.stl",
                                                    "format": "stl", "overwrite": True})
            self.assertEqual(Path(exported["path"]).name, "part.stl")
            self.assertGreater(triangles("part.stl"), 0)
            # An OBJ's .mtl beside it likewise.
            (self.workspace / "part.mtl").write_text("existing material\n", encoding="utf-8")
            client.tool("model_export", {"document": document, "path": "part.obj", "format": "obj"},
                        error=True)
            self.assertFalse((self.workspace / "part.obj").exists())
            client.tool("model_export", {"document": document, "path": "part.obj", "format": "obj",
                                         "overwrite": True})
            self.assertTrue((self.workspace / "part.obj").is_file())
            # A drawing's sheets likewise.
            client.tool("model_export", {"document": document, "path": "part.svg", "format": "svg"})
            client.tool("model_export", {"document": document, "path": "part.svg", "format": "svg"},
                        error=True)
            client.tool("model_export", {"document": document, "path": "part.svg", "format": "svg",
                                         "overwrite": True})
            # Outside the workspace: refused with overwrite too.
            with tempfile.TemporaryDirectory(prefix="outside-export-", dir=WORK) as outside:
                target = Path(outside) / "outside.stl"
                target.write_bytes(b"outside part")
                for path in (str(target), "../" + Path(outside).name + "/outside.stl"):
                    client.tool("model_export", {"document": document, "path": path, "format": "stl",
                                                 "overwrite": True}, error=True)
                self.assertEqual(target.read_bytes(), b"outside part")
                try:
                    os.symlink(target, self.workspace / "linked.stl")
                except (OSError, NotImplementedError):
                    pass
                else:
                    client.tool("model_export", {"document": document, "path": "linked.stl",
                                                 "format": "stl", "overwrite": True}, error=True)
                    self.assertEqual(target.read_bytes(), b"outside part")
                    (self.workspace / "linked.stl").unlink()
            client.tool("document_close", {"document": document, "discard": True})
            client.finish()

    def test_modeling_hints_are_scoped_optional_and_work_with_every_timeline_mode(self):
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            def command(value, **options):
                return client.tool("model_command", {"document": document, "command": value, **options})
            command({"cmd": "sketch.create"})
            sketch = command({"cmd": "sketch.add_point", "sketch": "F1", "at": [8, 4]})
            self.assertGreater(sketch["result"]["status"]["dof"], 0)
            self.assertTrue(any(h["rule"] == "sketch-underconstrained" and h["target"] == "F1"
                                for h in sketch["hints"]), sketch)
            # The untouched, underconstrained F1 must not leak into a clean edit.
            clean = command(BOX)
            self.assertEqual(clean["hints"], [])
            fixed = copy.deepcopy(BOX)
            fixed["def"]["plane"] = {"origin": [0, 0, 0], "normal": [0, 0, 1]}
            made = command(fixed)
            self.assertEqual([h["rule"] for h in made["hints"]], ["fixed-placement"])
            self.assertEqual([h["target"] for h in made["hints"]], ["F3"])
            for mode in ("changed", "full", "none"):
                renamed = command({"cmd": "rename_feature", "uid": "F3", "name": "Fixed " + mode}, timeline=mode)
                self.assertEqual([h["target"] for h in renamed["hints"]], ["F3"])
                self.assertTrue(renamed["hints"][0]["guide"]["topic"])
            quiet = command({"cmd": "rename_feature", "uid": "F3", "name": "Quiet"}, hints=False, timeline="none")
            self.assertNotIn("hints", quiet)
            recomputed = command({"cmd": "recompute"})
            self.assertEqual(recomputed["hints"], [])
            invalid = client.request("tools/call", {"name": "model_command", "arguments": {
                "document": document, "command": {"cmd": "recompute"}, "hints": "false"}})
            self.assertEqual(invalid["error"]["code"], -32602)
            # A real picked cylindrical fillet face is a volatile parent even
            # when it is valid geometry for a construction axis.
            document = client.tool("document_create", {})["document"]
            region = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"
            command({"cmd": "sketch.create"})
            command({"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40})
            command({"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
                    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}})
            command({"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
                    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}})
            faces = client.tool("model_query", {"document": document, "query": {"query": "faces", "body": "F2.b0"}})["result"]
            face = next(name for row in faces for name in row["names"] if name.startswith("F3:fillet("))
            made = command({"cmd": "add_feature", "def": {"type": "construction_axis", "definition": {
                    "type": "normal_to_face_at_point", "face": {"body": "F2.b0", "face": face},
                    "point": {"point": [60, 0, 10]}}}})
            self.assertTrue(any(h["rule"] == "volatile-parent" and h["target"] == "F4" for h in made["hints"]), made)
            client.finish()

    def test_compact_results_and_command_arrays(self):
        """mitcad#689: what a command changed, arrays as one undo step."""
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            created = client.tool("model_query", {"document": document,
                "query": {"query": "document"}})["result"]["undo_depth"]
            # Parameters and a box placed by them, in one call.
            made = client.tool("model_command", {"document": document, "command": [
                {"cmd": "add_parameter", "name": "L", "expression": "40 mm"},
                {"cmd": "add_parameter", "name": "W", "expression": "20 mm"},
                {"cmd": "add_feature", "def": {
                    "type": "box", "plane": {"type": "origin", "plane": "xy"},
                    "corner": ["-L/2", "-W/2"], "length": "L", "width": "W", "height": 10,
                    "operation": "new_body"}}]})
            self.assertEqual(made["ran"], 3)
            self.assertEqual(len(made["results"]), 3)
            self.assertNotIn("timeline", made)
            added = [f for f in made["changes"]["features"] if f["change"] == "added"]
            self.assertEqual(len(added), 1)
            self.assertEqual([b["change"] for b in made["changes"]["bodies"]], ["added"])
            self.assertEqual(made["state"]["undo_depth"], created + 1)
            box = client.tool("model_query", {"document": document, "query": {
                "query": "bodies", "properties": True}})["result"][0]
            self.assertAlmostEqual(box["bbox"]["min"][0], -20, places=5)
            self.assertAlmostEqual(box["bbox"]["max"][1], 10, places=5)
            # A parameter change moves the box: its body is listed as changed.
            moved = client.tool("model_command", {"document": document, "command": {
                "cmd": "set_parameter", "name": "L", "expression": "60 mm"}})
            self.assertEqual([b["change"] for b in moved["changes"]["bodies"]], ["changed"])
            box = client.tool("model_query", {"document": document, "query": {
                "query": "bodies", "properties": True}})["result"][0]
            self.assertAlmostEqual(box["bbox"]["min"][0], -30, places=5)
            # A command that recomputes nothing answers only what it changed.
            hidden = client.tool("model_command", {"document": document, "command": {
                "cmd": "set_body_visible", "uid": box["uid"], "visible": False}})
            self.assertEqual(hidden["changes"]["features"], [])
            self.assertEqual([b["uid"] for b in hidden["changes"]["bodies"]], [box["uid"]])
            self.assertLess(len(json.dumps(hidden)), 2048)
            full = client.tool("model_command", {"document": document, "command": {
                "cmd": "set_body_visible", "uid": box["uid"], "visible": True}, "timeline": "full"})
            self.assertEqual(len(full["timeline"]["features"]), 1)
            self.assertEqual(len(full["bodies"]), 1)
            self.assertNotIn("changes", full)
            # A refused entry undoes the ones before it.
            depth = client.tool("model_query", {"document": document,
                "query": {"query": "document"}})["result"]["undo_depth"]
            # The second H is refused by the model when it runs.
            refused = client.tool("model_command", {"document": document, "command": [
                {"cmd": "add_parameter", "name": "H", "expression": "5 mm"},
                {"cmd": "add_parameter", "name": "H", "expression": "6 mm"}]}, error=True)
            self.assertEqual(refused["entry"], 1)
            self.assertEqual(refused["undone"], 1)
            self.assertFalse(refused["applied"])
            self.assertIn("commands[1]", refused["error"])
            # An unknown command is refused before any entry runs.
            unknown = client.tool("model_command", {"document": document, "command": [
                {"cmd": "add_parameter", "name": "H", "expression": "5 mm"},
                {"cmd": "no_such_command"}]}, error=True)
            self.assertEqual((unknown["entry"], unknown["undone"]), (1, 0))
            state = client.tool("model_query", {"document": document,
                "query": {"query": "document"}})["result"]
            self.assertEqual(state["undo_depth"], depth)
            names = [p["name"] for p in client.tool("model_query", {"document": document,
                "query": {"query": "parameters"}})["result"]]
            self.assertNotIn("H", names)
            client.tool("model_command", {"document": document, "command": [
                {"cmd": "undo"}, {"cmd": "undo"}]}, error=True)
            # print_thickness answers its summary over MCP.
            walls = client.tool("model_query", {"document": document, "query": {
                "query": "print_thickness", "minimum": 20}})["result"]
            self.assertNotIn("triangles", walls["bodies"][0])
            self.assertGreater(walls["thin_triangles"], 0)
            client.finish()

    def test_model_render_pictures(self):
        """mitcad#1486: a preview image, deterministic, its camera reproducible."""
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": [BOX, CYLINDER]})
            # The default: home, perspective, 1024 x 768, the theme's background.
            default, png = render(client, {"document": document})
            self.assertEqual((default["width"], default["height"]), (1024, 768))
            self.assertEqual(default["camera"]["projection"], "perspective")
            self.assertEqual(default["camera"]["fov"], 45)
            self.assertEqual(sorted(default["bodies"]), ["F1.b0", "F2.b0"])
            self.assertEqual(default["warnings"], [])
            self.assertFalse(default["downscaled_for_bytes"])
            self.assertGreater(default["triangles"], 12)
            self.assertGreater(default["edges"], 12)
            print("model_render default timing_ms:", default["timing_ms"], "bytes:", default["bytes"],
                  file=sys.stderr)
            self.assertLess(default["timing_ms"]["total"], 10000)
            width, height, channels, rows = png_pixels(png)
            self.assertEqual((width, height, channels), (1024, 768, 3))
            self.assertEqual(rows[0][0:3], bytes([247, 248, 250]))
            self.assertNotEqual(rows[384][512 * 3:512 * 3 + 3], rows[384][0:3])
            # The same arguments twice: the same bytes.
            arguments = {"document": document, "view": "iso", "size": [320, 240]}
            first, first_png = render(client, arguments)
            second, second_png = render(client, arguments)
            self.assertEqual(first_png, second_png)
            self.assertEqual(first["camera"], second["camera"])
            # Another view, another picture.
            top, top_png = render(client, dict(arguments, view="top"))
            self.assertNotEqual(top_png, first_png)
            self.assertEqual(top["camera"]["up"], [0, 1, 0])
            angles, angles_png = render(client, dict(arguments, view={"azimuth": 30, "elevation": 20}))
            self.assertNotEqual(angles_png, first_png)
            # The camera answered, given back as the view, gives the picture again.
            for camera, png in ((first["camera"], first_png), (angles["camera"], angles_png)):
                view = {key: camera[key] for key in ("eye", "target", "up")}
                again, again_png = render(client, dict(arguments, view=view))
                self.assertEqual(again["camera"], camera)
                self.assertEqual(again_png, png)
            ortho, ortho_png = render(client, dict(arguments, projection="orthographic"))
            again, again_png = render(client, {"document": document, "size": [320, 240],
                                               "view": ortho["camera"]})
            self.assertEqual(again_png, ortho_png)
            # Restricted, hidden, a box, a transparent background, no edges.
            only, only_png = render(client, dict(arguments, bodies=["F2.b0"]))
            self.assertEqual(only["bodies"], ["F2.b0"])
            self.assertNotEqual(only_png, first_png)
            hidden, _ = render(client, dict(arguments, hidden=["F2"]))
            self.assertEqual(hidden["bodies"], ["F1.b0"])
            detail, detail_png = render(client, dict(arguments, fit={"min": [0, 0, 0], "max": [5, 5, 5]}))
            self.assertLess(detail["camera"]["height"], first["camera"]["height"] / 3)
            clear, clear_png = render(client, dict(arguments, background="transparent", edges=False))
            self.assertEqual(png_pixels(clear_png)[2], 4)
            self.assertEqual(png_pixels(clear_png)[3][0][0:4], bytes(4))
            refused, _ = render(client, dict(arguments, bodies=["F9.b0"]), error=True)
            self.assertIn("F9.b0", refused["error"])
            refused, _ = render(client, dict(arguments, view="nowhere"), error=True)
            self.assertIn("top", refused["error"])
            # The byte budget draws a smaller picture with the same camera.
            small, small_png = render(client, dict(arguments, max_bytes=6000))
            self.assertTrue(small["downscaled_for_bytes"])
            self.assertLessEqual(len(small_png), 6000)
            self.assertLess(small["width"], 320)
            self.assertEqual(small["camera"], first["camera"])
            # path writes the full picture too.
            written, written_png = render(client, dict(arguments, path="iso.png"))
            self.assertEqual((self.workspace / "iso.png").read_bytes(), written_png)
            self.assertEqual(written_png, first_png)
            render(client, dict(arguments, path="iso.png"), error=True)
            render(client, dict(arguments, path="iso.png", overwrite=True))
            render(client, dict(arguments, path="iso.jpg"), error=True)
            # A named view of the design.
            client.tool("model_command", {"document": document, "command": {
                "cmd": "add_named_view", "name": "Detail", "eye": [100, -100, 60],
                "target": [20, 10, 5], "up": [0, 0, 1], "height": 60}})
            named, _ = render(client, dict(arguments, view="Detail"))
            self.assertEqual(named["camera"]["eye"], [100, -100, 60])
            self.assertEqual(named["camera"]["projection"], "orthographic")
            client.tool("document_save", {"document": document, "path": "render.mitcad"})
            client.finish()
        # Read-only, without a display and with no GPU driver to be found
        # (every OpenGL, EGL, GBM and Vulkan vendor path points nowhere): the
        # same picture, no file written.
        nowhere = str(self.workspace / "no-drivers")
        env = {key: value for key, value in os.environ.items()
               if key not in ("DISPLAY", "WAYLAND_DISPLAY", "XDG_SESSION_TYPE")}
        env.update({"__EGL_VENDOR_LIBRARY_DIRS": nowhere, "__GLX_VENDOR_LIBRARY_NAME": "none",
                    "LIBGL_DRIVERS_PATH": nowhere, "GBM_BACKENDS_PATH": nowhere,
                    "VK_ICD_FILENAMES": nowhere, "VK_DRIVER_FILES": nowhere,
                    "MESA_LOADER_DRIVER_OVERRIDE": "none", "CUDA_VISIBLE_DEVICES": ""})
        with Session(self.workspace, read_only=True, env=env) as client:
            client.initialize()
            document = client.tool("document_open", {"path": "render.mitcad"})["document"]
            # The named view is in the design now: home is still the iso view.
            again, again_png = render(client, dict(arguments, document=document))
            self.assertEqual(again_png, first_png)
            refused, _ = render(client, dict(arguments, document=document, path="ro.png"), error=True)
            self.assertIn("read-only", refused["error"])
            client.finish()
        self.assertFalse((self.workspace / "ro.png").exists())

    def test_model_render_inspection_options(self):
        """mitcad#1487: colors, highlight, section, explode and views."""
        def accent(p):
            return p[2] > 180 and p[2] > p[0] + 100

        def red(p):
            return p[0] > 150 and p[0] > p[1] + 90 and p[0] > p[2] + 90

        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": [BOX, CYLINDER]})
            plain_args = {"document": document, "view": "top", "projection": "orthographic",
                          "size": [240, 160], "edges": False}
            plain, plain_png = render(client, plain_args)
            self.assertEqual(count_pixels(plain_png, accent), 0)
            self.assertEqual(count_pixels(plain_png, red), 0)
            # colors: the box red, the cylinder as it was.
            colored, colored_png = render(client, dict(plain_args, colors={"F1.b0": "#e01010"}))
            self.assertGreater(count_pixels(colored_png, red), 2000)
            self.assertEqual(colored["warnings"], [])
            # The more specific key wins: F1 (all its bodies) and F1.b0.
            both, both_png = render(client, dict(plain_args, colors={"F1": "#1010e0", "F1.b0": "#e01010"}))
            self.assertEqual(both_png, colored_png)
            # highlight: the box's top face in the selection blue, by each form.
            top = client.tool("model_query", {"document": document, "query": {
                "query": "faces", "body": "F1.b0", "normal": [0, 0, 1]}})["result"]
            face = top["faces"][0]["names"][0]
            lit, lit_png = render(client, dict(plain_args, highlight=[{"body": "F1.b0", "face": face}]))
            self.assertGreater(count_pixels(lit_png, accent), 2000)
            self.assertEqual(lit["warnings"], [])
            for form in ("F1.b0/" + face, face, {"kind": "face", "owner": "F1.b0", "name": face}):
                again, again_png = render(client, dict(plain_args, highlight=[form]))
                self.assertEqual(again_png, lit_png, form)
            # A whole body; an edge drawn over the rest.
            body, body_png = render(client, dict(plain_args, highlight=["F2"]))
            self.assertGreater(count_pixels(body_png, accent), 1000)
            edges = client.tool("model_query", {"document": document, "query": {
                "query": "edges", "body": "F1.b0"}})["result"]
            # A 30 mm edge along X (a vertical one is a dot from the top).
            edge = max(edges, key=lambda e: e["length"])["name"]
            line, line_png = render(client, dict(plain_args, highlight=[edge]))
            self.assertGreater(count_pixels(line_png, accent), 20)
            self.assertLess(count_pixels(line_png, accent), 2000)
            # Unknown references warn; the picture is drawn without them.
            unknown, unknown_png = render(client, dict(plain_args, colors={"F9.b0": "#ff0000"},
                                                       highlight=["F9.b0", "F1.b0/F1:nowhere"]))
            self.assertEqual(unknown_png, plain_png)
            self.assertEqual(len(unknown["warnings"]), 3, unknown["warnings"])
            self.assertTrue(any("F9.b0" in w and "colors" in w for w in unknown["warnings"]))
            refused, _ = render(client, dict(plain_args, colors={"F1.b0": "red"}), error=True)
            self.assertIn("#rrggbb", refused["error"])
            # explode: 0 is no explode; more frames the bodies moved apart.
            _, still_png = render(client, dict(plain_args, explode=0))
            self.assertEqual(still_png, plain_png)
            apart, apart_png = render(client, dict(plain_args, explode=1))
            self.assertNotEqual(apart_png, plain_png)
            self.assertGreater(apart["camera"]["height"], plain["camera"]["height"] * 1.2)
            # views: two pictures equal two single calls, each with its camera.
            sheet, pngs = render_views(client, {
                "document": document, "size": [240, 160], "edges": False,
                "views": ["iso", {"view": "front", "size": [160, 120]}]})
            iso, iso_png = render(client, {"document": document, "size": [240, 160], "edges": False,
                                           "view": "iso"})
            front, front_png = render(client, {"document": document, "size": [160, 120], "edges": False,
                                               "view": "front"})
            self.assertEqual(pngs, [iso_png, front_png])
            self.assertEqual([v["camera"] for v in sheet["views"]], [iso["camera"], front["camera"]])
            self.assertEqual((sheet["views"][1]["width"], sheet["views"][1]["height"]), (160, 120))
            self.assertEqual(sheet["bytes"], len(iso_png) + len(front_png))
            # Within the budget together: each its share.
            tight, tight_pngs = render_views(client, {"document": document, "size": [400, 300],
                                                      "views": ["iso", "top"], "max_bytes": 12000})
            self.assertEqual(tight["share_bytes"], 6000)
            self.assertLessEqual(sum(len(p) for p in tight_pngs), 12000)
            self.assertTrue(any(v["downscaled_for_bytes"] for v in tight["views"]))
            render(client, {"document": document, "views": ["iso"], "path": "sheet.png"}, error=True)
            # A ray-traced picture refuses the fast mode's options.
            refused, _ = render(client, {"document": document, "quality": "raytrace",
                                         "section": {"origin": [0, 0, 0], "normal": [0, 0, 1]}}, error=True)
            self.assertIn("section", refused["error"])
            client.finish()

        # section: a hollow cube cut in the middle shows its cavity, the
        # walls around it capped (hatched); a solid one is capped all over.
        def cube(cavity):
            commands = [{"cmd": "add_feature", "def": {
                "type": "box", "plane": "xy", "corner": [0, 0], "length": 40, "width": 40, "height": 40,
                "symmetric": True, "operation": "new_body"}}]
            if cavity:
                commands.append({"cmd": "add_feature", "def": {
                    "type": "box", "plane": "xy", "corner": [10, 10], "length": 20, "width": 20, "height": 20,
                    "symmetric": True, "operation": "cut", "participants": ["F1.b0"]}})
            return commands

        with Session(self.workspace) as client:
            client.initialize()
            # From the front, the box framed: 40 mm on 200 * 0.85 pixels,
            # the middle (x 20, z 0) at pixel (100, 100).
            arguments = {"view": "front", "projection": "orthographic", "size": [200, 200],
                         "fit": {"min": [0, 0, -20], "max": [40, 40, 20]}}
            cut = {"origin": [20, 20, 0], "normal": [0, -1, 0]}
            pictures = {}
            for cavity in (False, True):
                document = client.tool("document_create", {})["document"]
                client.tool("model_command", {"document": document, "command": cube(cavity)})
                bodies = client.tool("model_query", {"document": document, "query": {"query": "bodies",
                                                                                     "properties": True}})["result"]
                self.assertEqual(len(bodies), 1)
                self.assertAlmostEqual(bodies[0]["volume"], 64000 - (8000 if cavity else 0), delta=1e-3)
                _, whole = render(client, dict(arguments, document=document))
                result, png = render(client, dict(arguments, document=document, section=cut))
                self.assertEqual(result["warnings"], [])
                pictures[cavity] = (whole, png)
            whole, solid = pictures[False]
            _, hollow = pictures[True]
            # Uncut: the front face, one color in the middle and in a wall.
            self.assertTrue(uniform(whole, 95, 95, 10))
            self.assertTrue(uniform(whole, 31, 95, 10))
            # Cut solid: hatched cap in the middle and in the wall.
            self.assertFalse(uniform(solid, 95, 95, 10))
            self.assertFalse(uniform(solid, 31, 95, 10))
            # Cut hollow: the cavity's far wall in the middle, hatched walls.
            self.assertTrue(uniform(hollow, 95, 95, 10))
            self.assertFalse(uniform(hollow, 31, 95, 10))
            self.assertNotEqual(hollow, solid)
            client.finish()

    def test_model_render_overlays(self):
        """mitcad#1488: overlays from Print Analysis' functions, with its numbers."""
        with Session(self.workspace) as client:
            client.initialize()
            document = client.tool("document_create", {})["document"]
            client.tool("model_command", {"document": document, "command": BOX})

            def middle(png):
                width, height, channels, rows = png_pixels(png)
                at = (width // 2) * channels
                return tuple(rows[height // 2][at:at + 3])

            # Printed with up tilted 30 degrees: the bottom, 60 degrees from
            # the vertical, overhangs; from below it takes the overlay's color.
            half = 3 ** 0.5 / 2
            arguments = {"document": document, "view": "bottom", "projection": "orthographic",
                         "size": [320, 240], "overlay": {"type": "overhang", "up": [-0.5, 0, half]}}
            below, below_png = render(client, arguments)
            overlay = below["overlay"]
            self.assertEqual(overlay["type"], "overhang")
            self.assertEqual(overlay["color"], "#7e3fd0")
            self.assertAlmostEqual(overlay["overhang_area"], 600, places=6)
            self.assertEqual(middle(below_png), (0x7e, 0x3f, 0xd0))
            self.assertIn("overlay", below["timing_ms"])
            analysis = client.tool("model_query", {"document": document, "query": {
                "query": "print_overhangs", "bodies": ["F1.b0"], "down": [0.5, 0, -half]}})["result"]
            self.assertAlmostEqual(overlay["bodies"][0]["overhang_area"],
                                   analysis["bodies"][0]["overhang_area"], places=6)
            self.assertAlmostEqual(overlay["bodies"][0]["area"], analysis["bodies"][0]["area"], places=6)
            top, top_png = render(client, dict(arguments, view="top"))
            self.assertNotEqual(middle(top_png), (0x7e, 0x3f, 0xd0))
            # Walls under 25 mm: all of the 10 mm thick box, from the front too.
            thin, thin_png = render(client, dict(arguments, view="front",
                                                 overlay={"type": "thickness", "minimum": 25}))
            self.assertAlmostEqual(thin["overlay"]["thinnest"], 10, places=6)
            self.assertEqual(middle(thin_png), (0xc2, 0x18, 0x8b))
            # Not with the ray tracer; an unknown type is refused.
            render(client, dict(arguments, quality="raytrace"), error=True)
            refused, _ = render(client, dict(arguments, overlay="curvature"), error=True)
            self.assertIn("curvature", refused["error"])
            client.finish()

    def test_reference_in_parts_and_errors_that_give_the_choices(self):
        """mitcad#714: model_reference, and errors that list the choices."""
        with Session(self.workspace) as client:
            client.initialize()
            contents = client.tool("model_reference", {})
            self.assertIn("Feature definitions", contents["contents"])
            self.assertIn("add_feature", contents["commands"])
            self.assertIn("sketch.add_line", contents["commands"])
            self.assertNotIn("import_file", contents["commands"])
            self.assertIn("bodies", contents["queries"])
            self.assertNotIn("dxf_info", contents["queries"])
            self.assertIn("construction_plane", contents["features"])
            values = client.tool("model_reference", {"section": "Values"})
            self.assertTrue(values["text"].startswith("## Values"))
            row = client.tool("model_reference", {"query": "faces"})
            self.assertIn("| `faces` |", row["text"])
            self.assertTrue(row["permitted"])
            plane = client.tool("model_reference", {"feature": "construction_plane"})
            self.assertIn('"type": "construction_plane"', plane["example"])
            found = client.tool("model_reference", {"search": "bbox"})
            self.assertGreater(found["total"], 0)
            # In brief by default, whole on request (mitcad#1587).
            brief = client.tool("model_reference", {"section": "Feature definitions"})
            self.assertTrue(brief["brief"])
            self.assertIn("extended: true", brief["note"])
            long = client.tool("model_reference", {"section": "Feature definitions",
                                                   "extended": True})
            self.assertTrue(long["truncated"])
            self.assertLessEqual(len(long["text"].encode("utf-8")), 64 * 1024)
            rest = client.tool("model_reference", {"section": "Feature definitions",
                                                   "offset": long["next_offset"],
                                                   "extended": True})
            self.assertTrue(rest["text"])
            client.tool("model_reference", {"section": "No such section"}, error=True)

            document = client.tool("document_create", {})["document"]
            # An unknown query lists the queries; a refused one keeps its text.
            unknown = client.tool("model_query", {"document": document,
                                                  "query": {"query": "bbox"}}, error=True)
            self.assertIn("unknown query bbox", unknown["error"])
            self.assertIn("bodies", unknown["error"])
            refused = client.tool("model_query", {"document": document,
                                                  "query": {"query": "dxf_info", "path": "x.dxf"}}, error=True)
            self.assertIn("query dxf_info is not available through MCP", refused["error"])
            # An unknown command lists the commands.
            unknown = client.tool("model_command", {"document": document,
                                                    "command": {"cmd": "list"}}, error=True)
            self.assertIn("unknown command list", unknown["error"])
            self.assertIn("add_feature", unknown["error"])
            # A command without cmd shows the form.
            form = client.tool("model_command", {"document": document,
                                                 "command": {"command": "list"}}, error=True)
            self.assertIn('{"cmd": "add_feature", "def": {...}}', form["error"])
            # A definition that misses fields gets its type's reference.
            for definition in ({"type": "construction_plane"},
                               {"type": "construction_plane", "definition": {"type": "offset"}}):
                missing = client.tool("model_command", {"document": document, "command": {
                    "cmd": "add_feature", "def": definition}}, error=True)
                self.assertIn("missing field", missing["error"])
                self.assertIn('model_reference {"feature": "construction_plane"}', missing["error"])
                self.assertEqual(missing["reference"]["feature"], "construction_plane")
                self.assertIn('"definition"', missing["reference"]["example"])
                self.assertIn("`offset`", missing["reference"]["text"])
            # The model's own list of choices stays as it is.
            variant = client.tool("model_command", {"document": document, "command": {
                "cmd": "add_feature", "def": {"type": "construction_plane", "definition": {
                    "type": "offst", "plane": "xy", "distance": 10}}}}, error=True)
            self.assertIn("unknown variant", variant["error"])
            self.assertIn("expected one of", variant["error"])
            self.assertNotIn("reference", variant)
            self.assertEqual(client.tool("model_query", {"document": document,
                             "query": {"query": "timeline"}})["result"]["features"], [])
            client.finish()


if __name__ == "__main__":
    WORK.mkdir(parents=True, exist_ok=True)
    unittest.main(argv=[sys.argv[0]])
