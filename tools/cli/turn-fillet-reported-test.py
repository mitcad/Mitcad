# SPDX-License-Identifier: MIT
"""Turn Roughing along a concave R15 shoulder fillet, on the reported shaft
of GitHub mitcad/Mitcad#668 (mitcad#2420; never committed).

The report's archive is in `github-668/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, unpacked into the work folder. The shaft is revolved
about the design's X and steps up through an R15 fillet near Z -440 of
Setup 1's lathe frame; Setup 1 faces it and roughs the outside (Turn
Roughing 1: a turning tool of nose radius 0.8, 0.5 mm stock to leave in X,
0.2 in Z).

mitcad-cli mcp (read-only) opens the design and the test compares, in the
setup's work coordinates (X a radius, Z along the spindle axis):

- the profile Setup 1 turns (`cam_setups`) with the CAD fillet itself, the
  body's torus face of minor radius 15 (`faces` with geometry): the
  profile's arc on the fillet lies on that circle, concave (clockwise, G2,
  as the profile draws it), not mirrored out of the corner;
- Turn Roughing 1's arc on the fillet (`cam_toolpaths`, its `turn_path`):
  clockwise, 14.2 round (the fillet's 15 less the 0.8 nose radius: the
  nose's centre 0.8 from the fillet), about the fillet's centre moved by
  the nose to the imaginary tip and by the stock to leave;
- the Fanuc program (`cam_post`): that arc as G2, I 14.2 from its start;
- the simulated stock (`cam_simulate`): on the fillet the roughing leaves
  the fillet moved by the stock to leave, no gouge.

Before the fix the profile's arc bulged out of the corner, the roughing
stopped up to about 10 mm short of the fillet and its contour pass cut a
G3. Skipped when the folder or the archive is not there.

  python3 turn-fillet-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import math
import os
import queue
import re
import subprocess
import sys
import threading
import zipfile

ARCHIVE = "be037b92"  # the reporter's archive
DESIGN = "670822f3"  # the design in it
FOLDER = "github-668"
SKIP = "turn fillet reported: skipped"
TIMEOUT = 600
PROTOCOL = "2025-11-25"

FILLET = 15.0  # the CAD fillet's radius
NOSE = 0.8  # Turn Roughing 1's tool (number 3)
LEAVE_X = 0.5  # its stock to leave in X (a radius)
LEAVE_Z = 0.2  # and in Z


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


def sub(a, b):
    return [p - q for p, q in zip(a, b)]


def dot(a, b):
    return sum(p * q for p, q in zip(a, b))


def profile_arc_middle(a, b):
    """The middle of a profile's arc from vertex `a` to `b`, `[x, z]`, in the
    profile's own convention (Z right, X up, a positive bulge
    counter-clockwise: the arc bulges to the right of its chord)."""
    du, dv = b["z"] - a["z"], b["x"] - a["x"]
    d = math.hypot(du, dv)
    s = a["bulge"] * d / 2.0
    return [(a["x"] + b["x"]) / 2.0 - du / d * s, (a["z"] + b["z"]) / 2.0 + dv / d * s]


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
                                   "clientInfo": {"name": "mitcad-turn-fillet-test", "version": "1"}})
    session.notify("notifications/initialized")
    document =session.tool("document_open", {"path": "reported.mitcad"})["document"]

    def query(request):
        return session.tool("model_query", {"document": document, "query": request})["result"]

    setups = query({"query": "cam_setups"})["setups"]
    setup = next((s for s in setups if s.get("name") == "Setup 1"), None)
    check(setup is not None, f"no Setup 1 in {[s.get('name') for s in setups]}")
    work_frame = setup["work"]
    check("turning" in work_frame, f"Setup 1 does not turn: {work_frame}")
    origin, axis = work_frame["origin"], work_frame["z"]
    outside = work_frame["turning"]["part"]["outside"]

    # The CAD fillet: the shaft's torus face of minor radius 15, about the
    # spindle axis; its circle in the section, in work coordinates.
    faces = query({"query": "faces", "body": "F2.b0", "geometry": True, "surface": "torus"})
    tori = [f for f in faces["faces"] if abs(f.get("minor_radius", 0.0) - FILLET) < 1e-6]
    check(len(tori) == 1, f"{len(tori)} torus faces of radius {FILLET}: {faces}")
    torus = tori[0]
    check(abs(abs(dot(torus["axis"], axis)) - 1.0) < 1e-6,
          f"the fillet's axis {torus['axis']} is not the spindle's {axis}")
    off = sub(torus["center"], origin)
    along = dot(off, axis)
    across = math.sqrt(max(dot(off, off) - along * along, 0.0))
    check(across < 1e-6, f"the fillet's centre is {across} off the spindle axis")
    centre = [torus["major_radius"], along]
    print(f"     the CAD fillet: R{FILLET} about x {centre[0]:.4f}, z {centre[1]:.4f}")

    # The profile's arc on it: both ends on the circle, its middle too, in
    # the material's corner (clockwise as the profile draws it).
    def on_fillet(p):
        return abs(math.hypot(p[0] - centre[0], p[1] - centre[1]) - FILLET) < 1e-4

    arcs = [(a, b) for a, b in zip(outside, outside[1:])
            if a["bulge"] != 0.0 and on_fillet([a["x"], a["z"]]) and on_fillet([b["x"], b["z"]])]
    check(arcs, f"the profile has no arc on the fillet: {outside}")
    for a, b in arcs:
        middle = profile_arc_middle(a, b)
        print(f"     the profile's arc from x {a['x']:.4f}, z {a['z']:.4f} to x {b['x']:.4f}, "
              f"z {b['z']:.4f}, bulge {a['bulge']:.6f}, middle x {middle[0]:.4f}, z {middle[1]:.4f}")
        check(on_fillet(middle), f"the profile's arc bulges off the fillet: its middle at {middle}")
        check(a["bulge"] < 0.0, f"the profile's arc on the concave fillet is not clockwise: {a}")
    # One arc, from the fillet's start on the Ø100 diameter to where the
    # section's next face takes over (mitcad#2420's run: before the fix its
    # middle stood at the same distance on the other side of the chord).
    check(len(arcs) == 1, f"{len(arcs)} arcs on the fillet")
    a, b = arcs[0]
    for got, want in (([a["x"], a["z"]], [50.0, -425.0]), ([b["x"], b["z"]], [63.2353, -439.8958]),
                      (profile_arc_middle(a, b), [53.7868, -434.9632])):
        check(math.hypot(got[0] - want[0], got[1] - want[1]) < 1e-3, f"the profile's arc at {got}, not {want}")
    print("ok   the profile's arc lies on the CAD fillet, concave (G2)")

    # Turn Roughing 1's arc on the fillet: the nose's centre 0.8 from the
    # fillet (14.2 round), moved by the stock to leave; the tip's arc
    # about the fillet's centre moved by the nose and the stock to leave.
    operations = query({"query": "cam_toolpaths", "setup": "Setup 1"})["operations"]
    roughing = next((o for o in operations if o.get("name") == "Turn Roughing 1"), None)
    check(roughing is not None and "turn_path" in roughing,
          f"no Turn Roughing 1 toolpath: {[(o.get('name'), o.get('error')) for o in operations]}")
    tip_centre = [centre[0] - NOSE + LEAVE_X, centre[1] - NOSE + LEAVE_Z]
    at, found = None, []
    for move in roughing["turn_path"]["moves"]:
        if move["kind"] == "arc" and at is not None:
            radius = math.hypot(at[0] - move["center"][0], at[1] - move["center"][1])
            if abs(radius - (FILLET - NOSE)) < 1e-3:
                found.append((at, move))
        at = move["to"]
    check(found, f"Turn Roughing 1 has no arc {FILLET - NOSE} round: {roughing['turn_path']['moves']}")
    for start, move in found:
        print(f"     Turn Roughing 1's arc from {start} to {move['to']} about {move['center']}, "
              f"cw {move['cw']}")
        check(math.hypot(move["center"][0] - tip_centre[0], move["center"][1] - tip_centre[1]) < 1e-5,
              f"the arc's centre {move['center']} is not {tip_centre}")
        check(move["cw"], f"the arc on the concave fillet is not clockwise: {move}")
        end = math.hypot(move["to"][0] - tip_centre[0], move["to"][1] - tip_centre[1])
        check(abs(end - (FILLET - NOSE)) < 1e-5, f"the arc ends {end} from its centre")
    # One arc, from the fillet's start (0.5 out, 0.6 towards the chuck: the
    # stock to leave less the nose) to where the stock's radius ends it.
    check(len(found) == 1, f"{len(found)} arcs on the fillet")
    start, move = found[0]
    for got, want in ((start, [50.5, -425.6]), (move["to"], [63.0294, -439.7014])):
        check(math.hypot(got[0] - want[0], got[1] - want[1]) < 1e-3, f"the roughing's arc at {got}, not {want}")
    print(f"ok   Turn Roughing 1 rounds the fillet clockwise, {FILLET - NOSE} about its centre "
          f"(the nose's centre {NOSE} from it)")

    # The program: the arc as G2, I (a radius) 14.2 from its start.
    text = query({"query": "cam_post", "setup": "Setup 1", "dialect": "fanuc_turning"})["text"]
    arc_lines = [line for line in text.splitlines() if re.search(r"\bI-?[0-9.]+ K-?[0-9.]+", line)]
    for line in arc_lines:
        print(f"     {line}")
    fillet_lines = []
    for line in arc_lines:
        i = float(re.search(r"\bI(-?[0-9.]+)", line).group(1))
        k = float(re.search(r"\bK(-?[0-9.]+)", line).group(1))
        if abs(math.hypot(i, k) - (FILLET - NOSE)) < 2e-3:
            fillet_lines.append(line)
    check(fillet_lines, f"the program has no arc {FILLET - NOSE} round:\n{text}")
    for line in fillet_lines:
        check(line.startswith("G2 "), f"the fillet's arc is not written as G2: {line}")
    # X a diameter; the line the report gives for its fixed build.
    check([line.strip() for line in fillet_lines] == ["G2 X126.059 Z-439.701 I14.2 K0."],
          f"the fillet's arc is written as {fillet_lines}")
    print("ok   the program writes the fillet's arc as G2 X126.059 Z-439.701 I14.2 K0.")

    # The simulated stock on the fillet: the fillet moved by the stock to
    # leave (the roughing's contour pass), no gouge, between the fillet's
    # start and where it rises past the stock's radius.
    sim = query({"query": "cam_simulate", "setup": "Setup 1"})
    check(sim.get("turning") is True, f"not a turning simulation: {list(sim)}")
    check(not sim["gouges"], f"gouges: {sim['gouges']}")
    stock = sim["stock"]
    worst, checked = 0.0, 0
    for z, outer in zip(stock["z"], stock["outer"]):
        dz = z - centre[1]
        if not (-12.0 < dz < -0.5):
            continue
        left = centre[0] + LEAVE_X - math.sqrt(FILLET ** 2 - (dz - LEAVE_Z) ** 2)
        worst = max(worst, abs(outer - left))
        check(abs(outer - left) < 0.03,
              f"at z {z:.3f} the stock is turned to {outer:.4f}, the fillet with the stock to "
              f"leave at {left:.4f} (the fillet at {centre[0] - math.sqrt(FILLET ** 2 - dz ** 2):.4f})")
        checked += 1
    check(checked > 200, f"{checked} nodes on the fillet")
    print(f"ok   the simulated stock follows the fillet with the stock to leave ({checked} nodes, "
          f"at most {worst:.4f} off; left {sim.get('left')})")
    session.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
