#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The live MCP debugging flow (mitcad#265) against a running Mitcad.

Usage: ui-mcp-live.py <mitcad-cli> <design.mitcad>

The application must have opened the design (tools/ui-mcp-test.sh makes it:
a block, a fillet, and a sketch F4 whose projection of the block's bottom
face no longer matches it) and publish its listener in
MITCAD_MCP_DISCOVERY_DIR. Through `mitcad-cli mcp --connect`, the actual
bridge:
  - the sketch F4 opened for editing (ui_command edit);
  - queries with the sketch open (sketch, timeline);
  - ui_state: the sketch, its Sketch Palette's warning with the entities it
    is about, the agent connected;
  - sketch.add_line into the open sketch through its session, labelled
    "(agent)" in the undo text; a command on another feature refused; undo;
  - ui_select of a curve and a point, then ui_state's selection;
  - ui_screenshot of the view and of the window: PNGs under the limit;
  - ui_command Clean Up Stale Projections (its preview and warning), then
    Finish Sketch;
  - document_save to the design's own path;
  - ui_log: the commands' lines, and since a sequence number.
Before the flow (mitcad#935): the bridge authenticates while eight idle
connections that never authenticate are held open (each opened again when
the application closes it), and 200 ui_select calls with a 7 MiB owner and
50 model_command calls with a 7 MiB command name leave the application's
memory under 50 MB more (Linux) and every ui_log line under 4 KiB.
run_flow() is shared with the macOS test (tools/ui-macos-mcp-test.py).
Only the standard library is used.
"""

import base64
import json
import os
from pathlib import Path
import queue
import select
import socket
import struct
import subprocess
import sys
import threading
import time


PROTOCOL = "2025-11-25"
TIMEOUT = 60
FRAME_LIMIT = 8 * 1024 * 1024
LIVE_TOOLS = {"document_list", "model_query", "model_command", "document_save", "document_open", "ui_state", "ui_log",
              "ui_screenshot", "ui_select", "ui_command", "ui_camera", "model_export", "model_reference"}
# The projection of the block's bottom face in the sketch F4, as the
# design's definition lists it.
PROJECTED = ["p1", "p2", "c3", "p4", "c5", "p6", "c7", "c8"]
RECT = "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def design_script():
    """mitcad-cli run's commands for the design: a 60 x 40 block 20 high
    with a fillet of 5 on its vertical edge at (60, 0); a sketch on XY holds
    the block's bottom face as an earlier version recorded it (no
    by_geometry), 70 wide from x = -10 with sharp corners, and a line of
    the user's at x = 20; an extrude uses the region right of it. The face
    has an arc now: the projection keeps its geometry and warns."""
    return [
        {"cmd": "sketch.create"},
        {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
        {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": RECT}],
                                       "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
        {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
                                       "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 5}},
        {"cmd": "add_feature", "def": {
            "type": "sketch", "plane": "xy",
            "entities": [
                {"id": "p1", "type": "point", "at": [-10, 0], "fixed": True, "reference": True},
                {"id": "p2", "type": "point", "at": [60, 0], "fixed": True, "reference": True},
                {"id": "c3", "type": "line", "start": "p1", "end": "p2", "fixed": True, "reference": True},
                {"id": "p4", "type": "point", "at": [60, 40], "fixed": True, "reference": True},
                {"id": "c5", "type": "line", "start": "p2", "end": "p4", "fixed": True, "reference": True},
                {"id": "p6", "type": "point", "at": [-10, 40], "fixed": True, "reference": True},
                {"id": "c7", "type": "line", "start": "p4", "end": "p6", "fixed": True, "reference": True},
                {"id": "c8", "type": "line", "start": "p6", "end": "p1", "fixed": True, "reference": True},
                {"id": "p9", "type": "point", "at": [20, 0]},
                {"id": "p10", "type": "point", "at": [20, 40]},
                {"id": "c11", "type": "line", "start": "p9", "end": "p10"}],
            "constraints": [
                {"id": "k12", "type": "coincident", "point": "p9", "entity": "c3"},
                {"id": "k13", "type": "coincident", "point": "p10", "entity": "c7"}],
            "projections": [{"source": "F2:start(" + RECT + ")", "entities": PROJECTED}],
            # Valid stored outlines need no file. A remote text/style edit
            # must not silently read the metadata's missing font path.
            "texts": [{"id": "t14", "text": "A", "at": [0, -10], "height": 5,
                       "font": "MCP Synthetic",
                       "font_file": {"path": "mcp-missing-font.ttf"},
                       "glyphs": {"text": "A", "font": "MCP Synthetic", "glyphs": [{}]}}]}},
        {"cmd": "add_feature", "def": {
            "type": "extrude", "profiles": [{"sketch": "F4", "region": "r{c3[c11,c5],c5[c3,c7],c7[c5,c11],c11[c3,c7]}"}],
            "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
    ]


def make_design(cli, directory, env=None):
    """Writes the design with mitcad-cli; returns its path."""
    directory = Path(directory)
    script = directory / "stale.json"
    design = directory / "stale.mitcad"
    script.write_text(json.dumps(design_script()), encoding="utf-8")
    made = subprocess.run([cli, "run", str(script), "--save", str(design)], env=env, capture_output=True,
                          timeout=TIMEOUT)
    check(made.returncode == 0 and design.exists(),
          "mitcad-cli: " + made.stdout.decode("utf-8", "replace") + made.stderr.decode("utf-8", "replace"))
    make_linked_font_fixture(cli, directory, env)
    return design


def make_linked_font_fixture(cli, directory, env=None):
    """A safe saved root with embedded linked bodies; its child then changes
    both geometry and a font path. Opening the root through MCP must use the
    saved bodies without reading/recomputing that child."""
    child = directory / "mcp-linked-child.mitcad"
    root = directory / "mcp-linked-root.mitcad"
    fixtures = (
        (child, [{"cmd": "add_feature", "def": {"type": "box",
            "plane": {"type": "origin", "plane": "xy"}, "corner": [0, 0],
            "length": 10, "width": 10, "height": 10, "operation": "new_body"}}]),
        (root, [{"cmd": "insert_component", "path": str(child), "link": True,
                 "name": "Linked cube"}]),
    )
    for destination, commands in fixtures:
        script = destination.with_suffix(".json")
        script.write_text(json.dumps(commands), encoding="utf-8")
        made = subprocess.run([cli, "run", str(script), "--save", str(destination)],
                              env=env, capture_output=True, timeout=TIMEOUT)
        check(made.returncode == 0 and destination.exists(),
              "linked fixture: " + made.stdout.decode("utf-8", "replace")
              + made.stderr.decode("utf-8", "replace"))
    body = json.loads(child.read_text(encoding="utf-8"))
    height = body["features"][0]["height"]
    parameter = next(p for p in body["parameters"] if p["name"] == height)
    parameter["expression"], parameter["value"] = "20 mm", 20
    font = directory / "mcp-linked-blocked-font.ttf"
    font.write_bytes(b"this file must not be read as a font by an MCP open")
    body["features"].append({"uid": "F2", "name": "File font", "type": "sketch", "plane": "xy",
        "texts": [{"id": "t1", "text": "A", "at": [0, 0], "height": 5,
                   "font_file": {"path": str(font)}}]})
    child.write_text(json.dumps(body), encoding="utf-8")


class Bridge:
    """`mitcad-cli mcp --connect` with JSON-RPC over its stdin and stdout."""

    def __init__(self, cli, env, err_path):
        self.err = open(err_path, "w+b")
        self.process = subprocess.Popen([cli, "mcp", "--connect"], env=env, stdin=subprocess.PIPE,
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

    def stderr(self):
        self.err.seek(0)
        return self.err.read().decode("utf-8", "replace")

    def send(self, message):
        self.process.stdin.write(json.dumps(message, separators=(",", ":")).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def receive(self):
        try:
            line = self.messages.get(timeout=TIMEOUT)
        except queue.Empty as error:
            raise AssertionError("Bridge response timed out") from error
        check(line, "Bridge stdout closed: " + self.stderr())
        check(len(line) <= FRAME_LIMIT + 1, "Bridge frame over the limit")
        try:
            message = json.loads(line) if line.strip() else None
        except ValueError as error:
            raise AssertionError("Bridge stdout is not JSON: " + repr(line)) from error
        check(message is None or (isinstance(message, dict) and message.get("jsonrpc") == "2.0"),
              "Invalid bridge stdout: " + repr(line))
        return message

    def request(self, method, params=None):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method}
        if params is not None:
            message["params"] = params
        self.send(message)
        return self.answer()

    def request_encoded(self, method, params):
        """request() with its params already encoded as JSON bytes (a large
        input sent many times)."""
        self.next_id += 1
        self.process.stdin.write(b'{"jsonrpc":"2.0","id":%d,"method":"%s","params":'
                                 % (self.next_id, method.encode("ascii")) + params + b"}\n")
        self.process.stdin.flush()
        return self.answer()

    def answer(self):
        """The answer to the request sent last."""
        # Empty lines and notifications (a method, no id) are skipped;
        # anything else must be the answer with this id.
        for _ in range(1000):
            response = self.receive()
            if response is None:
                print("note: an empty line from the bridge", file=sys.stderr)
                continue
            if "id" not in response and "method" in response:
                print("note: a notification from the bridge: " + json.dumps(response), file=sys.stderr)
                continue
            check(response.get("id") == self.next_id, "JSON-RPC request ID was changed: " + repr(response))
            return response
        raise AssertionError("No answer to request " + str(self.next_id))

    def initialize(self):
        response = self.request("initialize", {
            "protocolVersion": PROTOCOL, "capabilities": {},
            "clientInfo": {"name": "mitcad-live-debug", "version": "1"}})
        check(response["result"]["protocolVersion"] == PROTOCOL, "Incorrect negotiated protocol")
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def tool(self, name, arguments, error=False):
        response = self.request("tools/call", {"name": name, "arguments": arguments})
        check("error" not in response, name + ": protocol failure: " + repr(response))
        result = response["result"]
        check(bool(result.get("isError", False)) == error, name + ": unexpected result: " + repr(result)[:2000])
        value = result["structuredContent"]
        check(json.loads(result["content"][0]["text"]) == value, "Tool text and JSON disagree")
        # A screenshot's image block after the text (mitcad#689).
        images = [item for item in result["content"][1:] if item.get("type") == "image"]
        if images:
            value = dict(value, image=images[0])
        return value

    def finish(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.reader.join(timeout=TIMEOUT)
        check(self.process.returncode == 0, self.stderr())

    def stop(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait(timeout=5)
        if not self.process.stdin.closed:
            self.process.stdin.close()
        self.reader.join(timeout=5)
        self.process.stdout.close()
        self.err.close()


class IdleConnections:
    """Connections to the listener that never authenticate, as another local
    account could hold them (mitcad#935): each is opened again as soon as the
    application closes it."""

    def __init__(self, port, count):
        self.port = port
        self.reopened = 0
        self.stopping = threading.Event()
        self.sockets = [self.open() for _ in range(count)]
        self.thread = threading.Thread(target=self.hold, daemon=True)
        self.thread.start()

    def open(self):
        return socket.create_connection(("127.0.0.1", self.port), timeout=5)

    def hold(self):
        while not self.stopping.is_set():
            readable = select.select(self.sockets, [], [], 0.05)[0] if self.sockets else []
            for closed in readable:
                try:
                    data = closed.recv(1)
                except OSError:
                    data = b""
                if data:
                    continue
                closed.close()
                self.sockets.remove(closed)
                try:
                    self.sockets.append(self.open())
                    self.reopened += 1
                except OSError:
                    pass
            if not self.sockets:
                time.sleep(0.05)

    def stop(self):
        self.stopping.set()
        self.thread.join(timeout=5)
        for held in self.sockets:
            held.close()


def resident_kb(pid):
    """The process's resident memory in kB (Linux), or None."""
    try:
        with open("/proc/%d/status" % pid, encoding="ascii", errors="replace") as status:
            for line in status:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except OSError:
        return None
    return None


def check_hostile_input(bridge, pid, say=print):
    """mitcad#935: a client's huge strings neither fill the application's
    memory nor its log, through the actual endpoint."""
    document = bridge.tool("document_list", {})["documents"][0]["document"]
    huge = 7 * 1024 * 1024
    select_params = json.dumps({"name": "ui_select", "arguments": {"document": document, "items": [
        {"kind": "face", "owner": "o" * huge, "name": "x"}]}}, separators=(",", ":")).encode("utf-8")
    command_params = json.dumps({"name": "model_command", "arguments": {"document": document, "command": {
        "cmd": "c" * huge}}}, separators=(",", ":")).encode("utf-8")
    check(len(select_params) < FRAME_LIMIT - 1024 and len(command_params) < FRAME_LIMIT - 1024,
          "The hostile frames do not fit the frame limit")

    def select_once():
        response = bridge.request_encoded("tools/call", select_params)
        check(response.get("error", {}).get("code") == -32602, "A 7 MiB owner was not refused: " + repr(response)[:500])
        message = response["error"]["message"]
        check("owner must have at most" in message and len(message) < 1024,
              "The refusal of a 7 MiB owner: " + message[:500])

    def command_once():
        response = bridge.request_encoded("tools/call", command_params)
        check("error" not in response, "model_command: protocol failure: " + repr(response)[:500])
        result = response["result"]
        error = result["structuredContent"].get("error", "")
        check(result.get("isError") is True and error, "A 7 MiB command name was not refused: " + repr(result)[:500])
        return error

    # The design may still be computing: until model_command answers.
    deadline = time.monotonic() + TIMEOUT
    while True:
        error = command_once()
        if "busy" not in error or time.monotonic() > deadline:
            break
        time.sleep(0.5)
    check("unknown command 'ccc" in error and "c" * 1000 not in error and len(error) < 1024 * 1024,
          "The refusal of a 7 MiB command name: " + error[:500])
    for _ in range(4):
        select_once()
        command_once()
    before = resident_kb(pid)
    for _ in range(200):
        select_once()
    for _ in range(50):
        error = command_once()
        check("c" * 1000 not in error, "A refusal repeated the 7 MiB command name")
    after = resident_kb(pid)
    if before is not None and after is not None:
        check(after - before < 50 * 1024, "The application's memory grew by %d kB" % (after - before))
        say("ok   200 ui_select calls with a 7 MiB owner, 50 model_command calls with a 7 MiB name: "
            "memory grew %d kB" % (after - before))
    else:
        say("ok   200 ui_select calls with a 7 MiB owner, 50 model_command calls with a 7 MiB name")
    log = bridge.tool("ui_log", {"limit": 2000})
    sizes = [len(line["text"].encode("utf-8")) for line in log["lines"]]
    check(max(sizes) < 4096, "A ui_log line of %d bytes" % max(sizes))
    check(any("MCP refused: unknown command 'ccc" in line["text"] for line in log["lines"]),
          "ui_log lacks the refusals of the long command name")
    say("ok   every ui_log line is under 4 KiB")


def wait_for_listener(cli, env, seconds=30):
    """Until the application answers an authenticated probe, as the bridge
    will (under load it may take a while)."""
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        listed = subprocess.run([cli, "mcp", "--list-instances"], env=env, capture_output=True, timeout=TIMEOUT)
        if listed.returncode == 0 and len(json.loads(listed.stdout)["instances"]) == 1:
            return
        time.sleep(0.2)
    raise AssertionError("mitcad-cli mcp --list-instances did not list the application")


def check_png(shot, limit):
    png = base64.b64decode(shot["image"]["data"])
    check(shot["image"]["mimeType"] == "image/png", "ui_screenshot's image is not a PNG block")
    check(png[:8] == b"\x89PNG\r\n\x1a\n", "ui_screenshot did not give a PNG")
    width, height = struct.unpack(">II", png[16:24])
    check((width, height) == (shot["width"], shot["height"]), "ui_screenshot's size disagrees with its PNG")
    check(len(png) == shot["bytes"] and len(png) < FRAME_LIMIT, "ui_screenshot's PNG is not under the limit")
    check(max(width, height) <= limit and min(width, height) >= 32, "ui_screenshot's PNG is not scaled to max_size")


def run_flow(bridge, design, say=print):
    """The live debugging flow on the open design (see the module's text)."""
    names = {tool["name"] for tool in bridge.request("tools/list")["result"]["tools"]}
    check(names == LIVE_TOOLS, "Live catalog: " + repr(sorted(names)))
    # The reference in parts, with the names the live server permits
    # (mitcad#714).
    contents = bridge.tool("model_reference", {})
    check("sketch.add_line" in contents["commands"] and "faces" in contents["queries"]
          and "extrude" in contents["features"], "model_reference lacks the permitted names")
    check("export" not in contents["commands"], "model_reference lists a file command")
    row = bridge.tool("model_reference", {"query": "faces"})
    check("| `faces` |" in row["text"] and row["permitted"], "model_reference's row of faces: " + repr(row))
    document = bridge.tool("document_list", {})["documents"][0]["document"]
    unknown = bridge.tool("model_query", {"document": document, "query": {"query": "bbox"}}, error=True)
    check("unknown query 'bbox'" in unknown["error"] and "bodies" in unknown["error"],
          "An unknown query does not list the queries: " + repr(unknown))

    # Refusals must preserve the complete live camera, including FOV. The
    # new eye/FOV would previously be applied before framing was checked.
    bridge.tool("ui_select", {"document": document, "items": []})
    before_camera = bridge.tool("ui_camera", {})["camera"]
    for framing in ({"frame": "selection"},
                    {"frame": "shown", "box": {"min": [0, 0, 0], "max": [10, 10, 10]}}):
        refused = bridge.tool("ui_camera", {"eye": [100, 100, 100], "target": [0, 0, 0],
                                            "fov": 90, **framing}, error=True)
        check(refused.get("error"), "Invalid framing was not refused: " + repr(refused))
        after_camera = bridge.tool("ui_camera", {})["camera"]
        check(after_camera == before_camera,
              "A refused ui_camera changed the camera: " + repr((before_camera, after_camera)))
    say("ok   refused camera framing preserves eye, target, up, height, projection and FOV")

    def sketch_query():
        return bridge.tool("model_query", {"document": document,
                                           "query": {"query": "sketch", "uid": "F4"}})["result"]

    unsafe = json.loads(Path(design).read_text(encoding="utf-8"))
    for feature in unsafe["features"]:
        for text in feature.get("texts", []):
            text.pop("glyphs", None)
    unsafe_path = Path(design).with_name("mcp-file-font.mitcad")
    unsafe_path.write_text(json.dumps(unsafe), encoding="utf-8")
    refused = bridge.tool("document_open", {"path": str(unsafe_path), "discard": True}, error=True)
    check("not available through MCP" in refused["error"],
          "Opening a design with a font path was not refused: " + repr(refused))
    check(bridge.tool("document_list", {})["documents"][0]["document"] == document,
          "A refused file-font open replaced the live document")
    for command in ("add_feature", "edit_feature"):
        refused = bridge.tool("model_command", {"document": document, "command": {
            "cmd": command, "uid": "F4", "def": {"type": "sketch", "texts": [{
                "id": "t1", "text": "A", "at": [0, 0], "height": 5,
                "font_file": {"path": "mcp-missing-font.ttf"}}]}}}, error=True)
        check(not refused.get("applied") and "not available through MCP" in refused["error"],
              "A file-backed feature definition was not refused: " + repr(refused))
    for fields in ({"text": "B"}, {"bold": True}, {"italic": True}):
        refused = bridge.tool("model_command", {"document": document, "command": {
            "cmd": "sketch.edit_text", "sketch": "F4", "id": "t14", **fields}}, error=True)
        check(not refused.get("applied") and "stored font file" in refused["error"],
              "An implicit font file read was not refused: " + repr(refused))
    say("ok   file-backed definitions and implicit live text font reads refused")

    edited = bridge.tool("ui_command", {"document": document, "edit": "F4"})
    check(edited["mode"] == "sketch" and edited["sketch"] == "F4", "Sketch F4 was not opened: " + repr(edited))
    say("ok   ui_command edit opened the sketch F4")

    # Queries with the sketch open.
    sketch = sketch_query()
    check(sketch["warning_entities"] == [PROJECTED], "The sketch query's warning entities: " + repr(sketch))
    timeline = bridge.tool("model_query", {"document": document, "query": {"query": "timeline"}})["result"]
    check(len(timeline["features"]) == 5, "The timeline with the sketch open: " + repr(timeline))
    say("ok   queries with the sketch open")

    state = bridge.tool("ui_state", {})
    check(state["mode"] == "sketch" and state["sketch"]["uid"] == "F4", "ui_state's sketch: " + repr(state))
    warnings = state["sketch"]["palette"]["warnings"]
    check(len(warnings) == 1 and warnings[0]["entities"] == PROJECTED and "does not follow" in warnings[0]["text"],
          "ui_state's palette warning: " + repr(warnings))
    check(state["sketch"]["palette"]["clean_up_offered"] is True, "ui_state: no clean-up offered")
    check(state["agents"] >= 1 and state["app"]["version"], "ui_state's agents and version: " + repr(state))
    check("mitcad-live-debug" in state["agent_clients"], "ui_state's initialized client labels: " + repr(state))
    say("ok   ui_state reports the sketch and its palette's warning with its entities")

    # A line into the open sketch through its session, then undo.
    before = len(sketch["entities"])
    added = bridge.tool("model_command", {"document": document, "command": {
        "cmd": "sketch.add_line", "sketch": "F4", "start": [100, 100], "end": [130, 100]}})
    check(added["applied"] is True and added["sketch"] == "F4", "sketch.add_line: " + repr(added)[:2000])
    check(len(sketch_query()["entities"]) == before + 3, "sketch.add_line did not add a line and its points")
    state = bridge.tool("ui_state", {})
    check(state["undo"] and state["undo"].endswith(" (agent)"), "The agent's change in the undo text: " + repr(state["undo"]))
    refused = bridge.tool("model_command", {"document": document, "command": {
        "cmd": "add_parameter", "name": "agent_width", "value": 10}}, error=True)
    check("being edited" in refused["error"], "Another feature's command in a sketch: " + repr(refused))
    bridge.tool("model_command", {"document": document, "command": {"cmd": "undo"}})
    check(len(sketch_query()["entities"]) == before, "Undo did not take the line back")
    say("ok   sketch.add_line through the sketch session, labelled (agent), then undo")

    # The selection, as an agent points at something.
    selected = bridge.tool("ui_select", {"document": document, "items": [
        {"kind": "sketch_curve", "owner": "F4", "name": "c3"},
        {"kind": "sketch_point", "owner": "F4", "name": "p1"}]})
    check(selected["selected"] == 2, "ui_select: " + repr(selected))
    state = bridge.tool("ui_state", {})
    shown = sorted((item["kind"], item["owner"], item["name"]) for item in state["selection"])
    check(shown == [("sketch_curve", "F4", "c3"), ("sketch_point", "F4", "p1")], "ui_state's selection: " + repr(shown))
    say("ok   ui_select, then ui_state shows the selection")

    for window in (False, True):
        shot = bridge.tool("ui_screenshot", {"window": window, "max_size": 640})
        check_png(shot, 640)
    say("ok   ui_screenshot of the view and the window: PNGs under the limit")

    cleaned = bridge.tool("ui_command", {"document": document, "id": "sketch.clean_up_stale_projections"})
    check(cleaned["committed"] is True, "Clean Up Stale Projections: " + repr(cleaned)[:3000])
    check(any(line.startswith("Preview Clean Up Stale Projections: ok") for line in cleaned["previews"]),
          "Clean Up Stale Projections' preview: " + repr(cleaned["previews"]))
    check(any("Cleaned up stale projections" in line for line in cleaned["log"]), "Clean Up's log: " + repr(cleaned["log"]))
    check(cleaned["undo"].endswith(" (agent)"), "Clean Up's undo text: " + repr(cleaned["undo"]))
    say("ok   ui_command ran Clean Up Stale Projections")
    finished = bridge.tool("ui_command", {"document": document, "id": "sketch.finish"})
    check(finished["mode"] == "idle", "Finish Sketch: " + repr(finished))
    say("ok   ui_command finished the sketch")

    # An appearance that reads no file is allowed, a texture by path is not
    # (mitcad#383).
    made = bridge.tool("model_command", {"document": document, "command": {
        "cmd": "create_appearance", "id": "led_red", "name": "LED red", "based_on": "emissive_white",
        "base_color": [1, 0.1, 0.08], "emission_color": [1, 0.1, 0.05], "emission": 5}})
    check(made["applied"] is True, "create_appearance: " + repr(made)[:2000])
    refused = bridge.tool("model_command", {"document": document, "command": {
        "cmd": "create_appearance", "id": "wood", "texture": {"path": "/etc/wood.png"}}}, error=True)
    check("/etc/wood.png" in refused["error"], "create_appearance by path: " + repr(refused))
    say("ok   create_appearance without a file ran, a texture by path was refused")

    written = os.stat(design).st_mtime_ns
    time.sleep(0.05)
    saved = bridge.tool("document_save", {"document": document})
    check(saved["saved"] is True and os.path.samefile(saved["path"], design), "document_save: " + repr(saved))
    check(os.stat(design).st_mtime_ns != written, "document_save did not write the file")
    check(bridge.tool("ui_state", {})["modified"] is False, "The document is still modified after document_save")
    say("ok   document_save wrote the file")

    log = bridge.tool("ui_log", {"limit": 2000})
    texts = [line["text"] for line in log["lines"]]
    for expected in ("MCP: an agent ran sketch.add_line in sketch F4", "Cleaned up stale projections",
                     "MCP: an agent saved"):
        check(any(expected in text for text in texts), "ui_log lacks " + expected)
    last = log["lines"][-1]["seq"]
    later = bridge.tool("ui_log", {"since": last})
    check(all(line["seq"] > last for line in later["lines"]), "ui_log's since")
    say("ok   ui_log shows the commands' lines")

    # A linked child is not covered by a root-only feature audit. The root
    # embeds a 1000 mm3 cube; the changed child would produce 2000 mm3 and
    # read an unembedded font if an ordinary update_links ran on MCP open.
    root = Path(design).parent / "mcp-linked-root.mitcad"
    original = json.loads(root.read_text(encoding="utf-8"))
    digests = {component["uid"]: component["link"]["digest"]
               for component in original["components"] if component.get("link")}
    check(bool(digests), "The linked regression root has no external component")
    opened = bridge.tool("document_open", {"path": str(root), "discard": True})
    linked = opened["document"]
    bodies = bridge.tool("model_query", {"document": linked,
        "query": {"query": "bodies", "properties": True}})["result"]
    check(len(bodies) == 1 and abs(bodies[0]["volume"] - 1000) <= 1e-6,
          "MCP open refreshed the changed linked child: " + repr(bodies))
    bridge.tool("document_save", {"document": linked})
    saved = json.loads(root.read_text(encoding="utf-8"))
    check({component["uid"]: component["link"]["digest"]
           for component in saved["components"] if component.get("link")} == digests,
          "MCP open followed a linked child instead of retaining its saved bodies")
    later = bridge.tool("ui_log", {"since": last, "limit": 2000})
    check(not any("mcp-linked-blocked-font.ttf" in line["text"] for line in later["lines"]),
          "Opening the root evaluated the linked child's font path")
    say("ok   MCP open retains embedded linked bodies without reading a changed child's font")


def main():
    if len(sys.argv) != 3:
        print(__doc__, file=sys.stderr)
        return 2
    cli, design = sys.argv[1], Path(sys.argv[2]).resolve()
    wait_for_listener(cli, dict(os.environ))
    listed = subprocess.run([cli, "mcp", "--list-instances"], env=dict(os.environ), capture_output=True,
                            timeout=TIMEOUT, check=True)
    instance = json.loads(listed.stdout)["instances"][0]
    # The bridge authenticates while eight idle connections are held open
    # (mitcad#935).
    idle = IdleConnections(instance["port"], 8)
    bridge = None
    try:
        bridge = Bridge(cli, dict(os.environ), design.parent / "bridge.stderr.log")
        bridge.initialize()
        check(bridge.tool("ui_state", {}).get("mode") is not None, "ui_state through the bridge")
        idle.stop()
        print("ok   the bridge authenticated with eight idle connections held open (%d opened again)"
              % idle.reopened)
        check_hostile_input(bridge, instance["pid"])
        run_flow(bridge, design)
        bridge.finish()
        print("PASS: live MCP debugging flow")
        return 0
    except Exception as error:  # noqa: BLE001 - the test reports any failure
        print("FAIL: live MCP: " + str(error), file=sys.stderr)
        return 1
    finally:
        idle.stop()
        if bridge is not None:
            bridge.stop()


if __name__ == "__main__":
    sys.exit(main())
