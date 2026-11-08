# SPDX-License-Identifier: MIT
"""model_render's ray-traced picture on the headless MCP server (mitcad#1489).

A design saved in the workspace, then opened by a read-only server and
rendered by the render worker: a PNG of the asked size within the time
limit, with the camera the fast mode gives for the same arguments, and the
same camera passed back as the view. Without the worker, and with a program
that is no worker, the call is refused with its error code and no image.

Usage: python3 mcp-raytrace-test.py <mitcad-cli> <mitcad-render> <work folder>
"""

import base64
import json
import os
from pathlib import Path
import queue
import struct
import subprocess
import sys
import threading
import time

CLI = str(Path(sys.argv[1]).resolve())
WORKER = str(Path(sys.argv[2]).resolve())
WORK = Path(sys.argv[3]).resolve()
TIMEOUT = 240
TIME_LIMIT = 30
BOX = {"cmd": "add_feature", "def": {
    "type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 30, "width": 20, "height": 10,
    "operation": "new_body"}}


class Session:
    """One headless server over stdio; the worker as `worker` names it."""

    def __init__(self, worker, read_only=False):
        env = dict(os.environ)
        env["MITCAD_RENDER_WORKER"] = worker
        args = [CLI, "mcp", "--workspace", str(WORK)] + (["--read-only"] if read_only else [])
        self.stderr = open(WORK / "stderr.txt", "ab")
        self.process = subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr, env=env)
        self.lines = queue.Queue()
        threading.Thread(target=self.read, daemon=True).start()
        self.next_id = 0
        self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                    "clientInfo": {"name": "raytrace-test", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def read(self):
        for line in self.process.stdout:
            self.lines.put(line)
        self.lines.put(b"")

    def send(self, message):
        self.process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        line = self.lines.get(timeout=TIMEOUT)
        assert line, "the server ended"
        response = json.loads(line)
        assert response.get("id") == self.next_id, response
        assert "result" in response, response
        return response["result"]

    def call(self, name, arguments):
        return self.request("tools/call", {"name": name, "arguments": arguments})

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.stderr.close()


def png_size(data):
    assert data[:8] == b"\x89PNG\r\n\x1a\n", data[:8]
    return struct.unpack(">II", data[16:24])


def same_camera(a, b):
    """Equal up to 1e-6 mm (and degrees): taken back, the height of a
    perspective follows from its distance again."""
    if set(a) != set(b) or a["projection"] != b["projection"]:
        return False
    numbers = lambda camera: [camera[key] for key in ("height", "fov") if key in camera] + \
        camera["eye"] + camera["target"] + camera["up"]
    return all(abs(x - y) <= 1e-6 for x, y in zip(numbers(a), numbers(b)))


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def main():
    WORK.mkdir(parents=True, exist_ok=True)
    for stale in ("raytrace.png", "box.mitcad"):
        if (WORK / stale).exists():
            (WORK / stale).unlink()

    # The design, saved into the workspace.
    writer = Session(WORKER)
    handle = writer.call("document_create", {})["structuredContent"]["document"]
    result = writer.call("model_command", {"document": handle, "command": BOX})
    check(not result["isError"], result)
    result = writer.call("document_save", {"document": handle, "path": "box.mitcad"})
    check(not result["isError"], result)
    # Written into the workspace too, at the size asked for.
    arguments = {"document": handle, "quality": "raytrace", "size": [80, 60], "samples": 2,
                 "time_limit_s": TIME_LIMIT, "path": "raytrace.png"}
    result = writer.call("model_render", arguments)
    check(not result["isError"], result)
    check(png_size((WORK / "raytrace.png").read_bytes()) == (80, 60), "the written picture's size")
    check(result["structuredContent"]["path"].endswith("raytrace.png"), result["structuredContent"])
    writer.close()

    # A read-only server renders it.
    reader = Session(WORKER, read_only=True)
    handle = reader.call("document_open", {"path": "box.mitcad"})["structuredContent"]["document"]
    arguments = {"document": handle, "quality": "raytrace", "view": "iso", "size": [96, 64],
                 "samples": 4, "time_limit_s": TIME_LIMIT}
    started = time.monotonic()
    result = reader.call("model_render", arguments)
    took = time.monotonic() - started
    check(not result["isError"], result)
    content = result["content"]
    check([block["type"] for block in content] == ["text", "image"], content)
    check(content[1]["mimeType"] == "image/png", content[1])
    picture = base64.b64decode(content[1]["data"])
    check(png_size(picture) == (96, 64), "the picture's size")
    found = result["structuredContent"]
    check("image_base64" not in found, "the image left the JSON")
    check(found["quality"] == "raytrace" and found["width"] == 96 and found["height"] == 64, found)
    check(found["bytes"] == len(picture), found)
    check(len(found["bodies"]) == 1, found)
    # Within the time limit (and the worker's start).
    check(found["timing_ms"]["render"] <= TIME_LIMIT * 1000, found["timing_ms"])
    check(took < TIME_LIMIT + 60, f"the call took {took:.1f} s")
    camera = found["camera"]
    check(set(camera) >= {"eye", "target", "up", "projection", "height"}, camera)
    # The fast mode's camera for the same arguments (mitcad#1486).
    fast = dict(arguments)
    for key in ("quality", "samples", "time_limit_s"):
        fast.pop(key)
    answer = reader.call("model_render", fast)
    check(not answer["isError"], answer)
    check(answer["structuredContent"]["camera"] == camera,
          f"fast {answer['structuredContent']['camera']} != raytrace {camera}")
    check(answer["structuredContent"]["bodies"] == found["bodies"], answer["structuredContent"]["bodies"])
    # The camera passed back as the view gives the same camera.
    again = reader.call("model_render", dict(arguments, view=camera))
    check(not again["isError"], again)
    check(same_camera(again["structuredContent"]["camera"], camera), again["structuredContent"]["camera"])
    # path writes a file: not on a read-only server.
    refused = reader.call("model_render", dict(arguments, path="other.png"))
    check(refused["isError"] and not (WORK / "other.png").exists(), refused)
    reader.close()

    # No worker: refused with its error code, the fast mode suggested.
    for worker, code in ((str(WORK / "no-such-worker"), "render_worker_missing"),
                         (CLI, "render_worker_failed")):
        session = Session(worker, read_only=True)
        handle = session.call("document_open", {"path": "box.mitcad"})["structuredContent"]["document"]
        result = session.call("model_render", {"document": handle, "quality": "raytrace",
                                               "size": [32, 32], "samples": 1})
        check(result["isError"], result)
        check([block["type"] for block in result["content"]] == ["text"], result["content"])
        check(result["structuredContent"]["error_code"] == code, result["structuredContent"])
        check("fast" in result["structuredContent"]["suggestion"], result["structuredContent"])
        session.close()
    print("mcp raytrace test passed")


if __name__ == "__main__":
    main()
