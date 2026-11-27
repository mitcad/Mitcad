# SPDX-License-Identifier: MIT
"""The whole outside turned on the reported lathe design of GitHub
mitcad/Mitcad#704 (mitcad#2531, a follow-up of mitcad#2499; never
committed).

The report's archive is in `github-704/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, unpacked into the work folder. The part is a revolved
spear, 255 mm long and 32 mm across at its base, whose section goes through
a spline that the section samples into pieces shorter than the CAM
outlines' 0.001 mm join tolerance. Setup 1 turns it about its saved
spindle axis (an edge at the tip), work Z 0 at the tip, and roughs the
outside (Turn Roughing 1).

The design's lathe names a controller of the reporter's own build that
MitCad does not post for; the design opens with it kept (mitcad#2531
round 2: it was refused before). mitcad-cli mcp (read-only) opens the
design and the test checks, in the setup's work coordinates (X a radius,
Z along the spindle axis):

- the profile Setup 1 turns (`cam_setups`, `work.turning.part.outside`):
  it runs from the tip at Z 0 to the base at Z -255 (32 mm across there),
  one chain from the face to the back that never turns back towards the
  face by more than 0.05 mm (no piece taken the wrong way round);
- Turn Roughing 1 (`cam_toolpaths`, the toolpath Generate makes): it has
  no error, and its feed moves cover the whole span, from 1 mm off the tip
  to 1 mm off the base, without a gap.

With the old first-match rule of `take_from()`
(`core/model/src/cam/outline.rs`) the section fell apart at the spline into
several chains and the widest one, the base, was kept: the profile ran only
from Z -212.8 to -255 and the roughing cut only there. Skipped when the
folder or the archive is not there.

  python3 turn-outline-chain-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import os
import queue
import subprocess
import sys
import threading
import zipfile

ARCHIVE = "363f6417"  # the reporter's archive
DESIGN = "9ed135a3"  # the design in it
FOLDER = "github-704"
SKIP = "turn outline chain reported: skipped"
TIMEOUT = 600
PROTOCOL = "2025-11-25"

FACE_Z = 0.0  # the tip, the setup's work Z 0
BACK_Z = -255.0  # the base
BASE_RADIUS = 16.0  # the base's 32 mm diameter
BACKWARDS = 0.05  # the most the profile may step back towards the face
END = 1.0  # how close to the tip and the base the roughing must cut


def content_id(data):
    return hashlib.sha256(data).hexdigest()[:8]


def find_design(work):
    """The design unpacked into `work` from the reporter's archive."""
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if not (name.endswith(".zip") and os.path.isfile(path)):
            continue
        with open(path, "rb") as f:
            if content_id(f.read()) != ARCHIVE:
                continue
        with zipfile.ZipFile(path) as archive:
            for entry in archive.infolist():
                if not entry.filename.endswith(".mitcad"):
                    continue
                data = archive.read(entry)
                if content_id(data) == DESIGN:
                    os.makedirs(work, exist_ok=True)
                    design = os.path.join(work, "reported.mitcad")
                    with open(design, "wb") as out:
                        out.write(data)
                    return design
        sys.exit(f"FAIL: the archive {ARCHIVE} holds no design {DESIGN}")
    print(f"{SKIP} (no archive {ARCHIVE} in {folder})")
    return None


class Session:
    """mitcad-cli mcp over stdin and stdout, a reader thread for timeouts."""

    def __init__(self, cli, workspace):
        self.stderr = open(os.path.join(workspace, "mcp-stderr.txt"), "wb")
        self.process = subprocess.Popen([cli, "mcp", "--workspace", workspace, "--read-only"],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr)
        self.messages = queue.Queue()
        self.next_id = 0
        threading.Thread(target=self.read, daemon=True).start()

    def read(self):
        while True:
            line = self.process.stdout.readline()
            self.messages.put(line)
            if not line:
                return

    def request(self, method, params):
        self.next_id += 1
        message = {"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}
        self.process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
        self.process.stdin.flush()
        try:
            line = self.messages.get(timeout=TIMEOUT)
        except queue.Empty:
            sys.exit(f"FAIL: {method} timed out")
        if not line:
            sys.exit(f"FAIL: mitcad-cli mcp ended at {method}")
        response = json.loads(line)
        if response.get("id") != self.next_id or "error" in response:
            sys.exit(f"FAIL: {method}: {response}")
        return response["result"]

    def notify(self, method):
        message = {"jsonrpc": "2.0", "method": method}
        self.process.stdin.write(json.dumps(message).encode("utf-8") + b"\n")
        self.process.stdin.flush()

    def tool(self, name, arguments):
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent", {})
        if result.get("isError"):
            sys.exit(f"FAIL: {name} {json.dumps(arguments)}: {content}")
        return content

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.stderr.close()


def check(condition, message):
    if not condition:
        sys.exit(f"FAIL: {message}")


def covered(intervals, low, high):
    """The first gap of `intervals` ((z, z) pairs) in [low, high], or None
    when they cover it."""
    at = low
    for a, b in sorted((min(a, b), max(a, b)) for a, b in intervals):
        if a > at + 1e-9:
            break
        at = max(at, b)
        if at >= high:
            return None
    return (at, high) if at < high else None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    work = os.path.abspath(args.work)
    if find_design(work) is None:
        return 0
    session = Session(args.cli, work)
    session.request("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                   "clientInfo": {"name": "mitcad-turn-outline-chain-test",
                                                  "version": "1"}})
    session.notify("notifications/initialized")
    document = session.tool("document_open", {"path": "reported.mitcad"})["document"]

    def query(request):
        return session.tool("model_query", {"document": document, "query": request})["result"]

    # The lathe names a controller of the reporter's own build: the design
    # opens with it kept (only the post refuses it).
    listed = query({"query": "cam_setups"})
    controllers = [m.get("controller") for m in listed.get("machines", [])]
    check(controllers and all(controllers),
          f"the design's machines lost their controllers: {listed.get('machines')}")
    print(f"ok   the design opens with its lathe's controller kept: {controllers}")

    # The profile Setup 1 turns, about its saved spindle axis.
    setups = listed["setups"]
    setup = next((s for s in setups if s.get("name") == "Setup 1"), None)
    check(setup is not None, f"no Setup 1 in {[s.get('name') for s in setups]}")
    work_frame = setup["work"]
    check("turning" in work_frame, f"Setup 1 does not turn: {work_frame}")
    outside = work_frame["turning"]["part"]["outside"]
    check(len(outside) >= 2, f"the outside profile has {len(outside)} points")
    first, last = outside[0], outside[-1]
    zs = [v["z"] for v in outside]
    radius = max(v["x"] for v in outside)
    print(f"     the outside profile: {len(outside)} points from x {first['x']:.4f}, "
          f"z {first['z']:.4f} to x {last['x']:.4f}, z {last['z']:.4f}; "
          f"z {min(zs):.4f} to {max(zs):.4f}, radius at most {radius:.4f}")
    check(abs(first["z"] - FACE_Z) < 1e-3,
          f"the outside profile starts at z {first['z']}, not at the tip (z {FACE_Z})")
    check(abs(last["z"] - BACK_Z) < 1e-3,
          f"the outside profile ends at z {last['z']}, not at the base (z {BACK_Z})")
    check(max(zs) < FACE_Z + 1e-3 and min(zs) > BACK_Z - 1e-3,
          f"the outside profile leaves the part: z {min(zs)} to {max(zs)}")
    check(abs(radius - BASE_RADIUS) < 1e-3,
          f"the outside profile is {radius} at its widest, not the base's {BASE_RADIUS}")
    # One chain from the face to the back: never back towards the face by
    # more than a spline's own wiggle.
    furthest, worst = zs[0], 0.0
    for i, z in enumerate(zs):
        worst = max(worst, z - furthest)
        check(z - furthest <= BACKWARDS,
              f"the outside profile turns back towards the face at point {i}: z {z}, "
              f"after z {furthest}")
        furthest = min(furthest, z)
    print(f"ok   the outside profile runs from the tip at z {FACE_Z} to the base at z {BACK_Z} "
          f"in one chain (at most {worst:.4f} back towards the face)")

    # Turn Roughing 1 over the whole span: its feed moves (lines, arcs and
    # threads; not rapids) from 1 mm off the tip to 1 mm off the base.
    operations = query({"query": "cam_toolpaths", "setup": "Setup 1"})["operations"]
    roughing = next((o for o in operations if o.get("name") == "Turn Roughing 1"), None)
    check(roughing is not None and "turn_path" in roughing,
          f"no Turn Roughing 1 toolpath: {[(o.get('name'), o.get('error')) for o in operations]}")
    check("error" not in roughing, f"Turn Roughing 1: {roughing.get('error')}")
    at, feeds = None, []
    for move in roughing["turn_path"]["moves"]:
        if move["kind"] != "rapid" and at is not None:
            feeds.append((at[1], move["to"][1]))
        at = move["to"]
    check(feeds, f"Turn Roughing 1 has no feed moves: {roughing['turn_path']['moves']}")
    low = min(min(a, b) for a, b in feeds)
    high = max(max(a, b) for a, b in feeds)
    print(f"     Turn Roughing 1: {len(roughing['turn_path']['moves'])} moves, "
          f"{len(feeds)} feeds from z {high:.4f} to {low:.4f}")
    gap = covered(feeds, BACK_Z + END, FACE_Z - END)
    check(gap is None,
          f"Turn Roughing 1 does not cut from z {gap[0] if gap else 0:.4f} to "
          f"{gap[1] if gap else 0:.4f} (its feeds run z {low:.4f} to {high:.4f})")
    print(f"ok   Turn Roughing 1 cuts the whole outside, z {FACE_Z - END} to {BACK_Z + END}")
    session.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
