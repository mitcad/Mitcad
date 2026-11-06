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
import subprocess
import sys
import tempfile
import threading
import time
import unittest


CLI = str(Path(sys.argv[1]).resolve())
WORK = Path(sys.argv[2]).resolve()
TIMEOUT = 20
PROTOCOL = "2025-11-25"
TOOLS = {"document_create", "document_open", "document_list", "document_close",
         "model_command", "model_query", "document_save", "model_export", "model_reference"}
BOX = {"cmd": "add_feature", "def": {
    "type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 30, "width": 20, "height": 10,
    "operation": "new_body"}}


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
            long = client.tool("model_reference", {"section": "Feature definitions"})
            self.assertTrue(long["truncated"])
            self.assertLessEqual(len(long["text"].encode("utf-8")), 64 * 1024)
            rest = client.tool("model_reference", {"section": "Feature definitions",
                                                   "offset": long["next_offset"]})
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
