# SPDX-License-Identifier: MIT
"""Natural Extend around the rounded outer rim of the reported offset shell
of GitHub mitcad/Mitcad#689 (mitcad#2525; never committed).

The report's archive is in `github-689/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, unpacked into the work folder. The design is a thin cover
(an imported solid, Base2): a flat top at z 777.413 rounded down at its
sides (radius 6) and its corners into walls drafted 15 degrees outwards
that end at z 769.413, a rounded rectangle in plan (straight sides 43 and
27.4 long, quarter-round corners of plan radius 17.144 about the sides'
ends). DeleteFace1 closes a hole in the top; OffsetSurface1 (F4) offsets the
top's tangent chain, the top, the rounds, the corners and the eight walls
(17 faces), 5 mm outwards into the surface body F4.b0. Its one open
boundary is the walls' lower rim, one free edge a wall: four straight
sides and four curved corners, at z 769.413 + 5 sin 15 degrees.

The timeline is rolled to the end of OffsetSurface1 and Extend is run as
the Extend panel sends it, Natural, 5 mm, each case by `mitcad-cli run`:
the panel's preview, then the same command committed (Extend1 ok), the
result saved into the work folder:

- Whole Opening on, the rim picked: the whole opening, its faces the eight
  walls;
- Whole Opening off, one curved corner edge alone (the corner at +x +y,
  picked by a point nearest it), the opening and the point;
- Whole Opening off, one straight side alone (the side at +y), the
  existing path of an edge along a side of its face.

mitcad-cli mcp (read-only) then opens the design and each result, and the
test checks F4.b0 against the design before the extension. The walls are
ruled, so Natural continues them along their straight rulings, square to
the rim: the extension's new edge lies 5 cos 15 degrees lower and 5 sin 15
degrees further out, a straight side's strip is a rectangle (its length by
5) and a corner's strip a quarter of a cone's (its rim length by 5, and pi
/ 4 times 5^2 sin 15 degrees more as it widens). Each case gives one
surface body (a sheet) of the same 17 faces, each keeping its name, whose
area grows by its strips. With Whole Opening on the strips are joined round
the rim: its free edges are again the eight, all on the new level, the
straight ones as long as before and the corners longer by pi / 2 times 5
sin 15 degrees. With one edge alone the other seven free edges stay where
they were, and the strip adds its new edge and its two sides, 5 long down
the rulings from the picked edge's ends. Before the fix the curved corners
were refused (`unsupported: extend of an edge, which does not run along a
side of its face's parameter range`). The API has no validity check of a
shape; the geometry test `test_a_rounded_rim_is_extended_past_its_curved_corners`
checks the extensions of a synthetic rim with OCCT's checker. Skipped when
the folder or the archive is not there.

  python3 extend-rounded-rim-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import math
import os
import queue
import subprocess
import sys
import threading
import zipfile

ARCHIVE = "c0abb757"  # the reporter's archive
DESIGN = "71767a89"  # the design in it
FOLDER = "github-689"
SKIP = "extend rounded rim reported: skipped"
TIMEOUT = 600
PROTOCOL = "2025-11-25"

BODY = "F4.b0"  # OffsetSurface1's surface body
FEATURES = 4  # Base1, Base2, DeleteFace1, OffsetSurface1
FACES = 17
CHAIN = range(25, 42)  # the offset faces' sources, F2:import(25) to (41)
WALLS = (26, 30, 31, 37, 38, 39, 40, 41)  # the faces along the rim
DISTANCE = 5.0  # the extension, and the offset
DRAFT = math.radians(15.0)  # the walls' draft (0.952001 out over 3.552914 down)
SIN, COS = math.sin(DRAFT), math.cos(DRAFT)
TOP_Z = 777.413 + DISTANCE  # the top, offset
RIM_Z = 769.413 + DISTANCE * SIN  # the rim, offset with the walls
NEW_Z = RIM_Z - DISTANCE * COS  # the extended rim
SIDES = (43.0, 27.4)  # the straight sides' lengths
CORNER = math.pi / 2 * (17.143594 + DISTANCE * COS)  # a corner's rim, offset
WIDENS = math.pi / 2 * DISTANCE * SIN  # how much longer a corner's new edge is

LEVEL = 0.01  # mm: an edge on a level, a box's side, a new edge's length
IN_PLACE = 1e-3  # mm: an edge that stays where it was
AREA = 1.0  # mm2: an extension's area


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


def offset_name(face):
    return f"F4:offset(F2:import({face}))"


class State:
    """F4.b0 in one opened design: its properties, faces and free edges."""

    def __init__(self, session, path):
        document = session.tool("document_open", {"path": path})["document"]

        def query(request):
            return session.tool("model_query", {"document": document, "query": request})["result"]

        bodies = query({"query": "bodies", "properties": True})
        body = next((b for b in bodies if b.get("uid") == BODY), None)
        check(body is not None, f"{path}: no body {BODY} in {[b.get('uid') for b in bodies]}")
        self.path = path
        self.body = body
        self.faces = sorted(n for f in query({"query": "faces", "body": BODY}) for n in f["names"])
        edges = query({"query": "edges", "body": BODY, "geometry": True})
        # A free edge (of one face only) has no name.
        self.free = [e for e in edges if e.get("name") is None]
        check(all(e.get("bbox") for e in self.free), f"{path}: a free edge without a box: {self.free}")
        session.tool("document_close", {"document": document})

    def box(self, side, axis):
        return self.body["bbox"][side][axis]


def extent(edge, axis):
    return edge["bbox"]["max"][axis] - edge["bbox"]["min"][axis]


def is_corner(edge):
    return extent(edge, 0) > 1.0 and extent(edge, 1) > 1.0


def at_level(edge, z):
    return abs(edge["bbox"]["min"][2] - z) <= LEVEL and abs(edge["bbox"]["max"][2] - z) <= LEVEL


def centre(edge, axis):
    return (edge["bbox"]["min"][axis] + edge["bbox"]["max"][axis]) / 2


def same_edge(a, b):
    return abs(a["length"] - b["length"]) <= IN_PLACE and all(
        abs(a["bbox"][side][k] - b["bbox"][side][k]) <= IN_PLACE
        for side in ("min", "max") for k in range(3))


def describe(edges):
    return "; ".join(f"{e['curve']} {e['length']:.4f} from {e['bbox']['min']} to {e['bbox']['max']}"
                     for e in edges)


def check_before(before):
    """The offset shell as the design describes it."""
    body = before.body
    check(body.get("kind") == "sheet", f"{BODY} is not a surface body: {body.get('kind')}")
    check(body.get("faces") == FACES, f"{BODY} has {body.get('faces')} faces, not {FACES}")
    wanted = sorted(offset_name(f) for f in CHAIN)
    check(before.faces == wanted, f"{BODY}'s faces are {before.faces}, not {wanted}")
    check(abs(before.box("max", 2) - TOP_Z) <= LEVEL and abs(before.box("min", 2) - RIM_Z) <= LEVEL,
          f"{BODY} runs from z {before.box('min', 2)} to {before.box('max', 2)}, not from the "
          f"rim at {RIM_Z:.4f} to the top at {TOP_Z:.4f}")
    free = before.free
    corners = [e for e in free if is_corner(e)]
    sides = [e for e in free if not is_corner(e)]
    check(len(corners) == 4 and len(sides) == 4,
          f"the rim is not four corners and four sides: {describe(free)}")
    check(all(at_level(e, RIM_Z) for e in free), f"the rim is not at z {RIM_Z:.4f}: {describe(free)}")
    check(all(abs(e["length"] - CORNER) <= LEVEL for e in corners),
          f"the corners' rims are not {CORNER:.4f} long: {describe(corners)}")
    check(all(min(abs(e["length"] - s) for s in SIDES) <= IN_PLACE for e in sides),
          f"the sides are not {SIDES[0]} and {SIDES[1]} long: {describe(sides)}")
    print(f"ok   {BODY}: {FACES} faces, area {body['area']:.4f}, its rim at z {RIM_Z:.4f} four "
          f"sides and four corners {sum(e['length'] for e in free):.4f} long")


def run_extend(cli, work, case, openings):
    """Extend as the panel sends it, previewed and committed, saved as
    `<case>.mitcad` in the work folder."""
    # Natural is the panel's default, which it leaves out.
    definition = {"type": "extend_surface", "body": BODY, "openings": openings,
                  "distance": f"{DISTANCE:g} mm"}
    script = [
        {"comment": f"Extend of the reported rim, {case} (extend-rounded-rim-reported-test.py): "
                    "the timeline rolled to the end of OffsetSurface1."},
        {"cmd": "set_marker", "position": FEATURES},
        {"preview": {"cmd": "add_feature", "def": definition}},
        {"cmd": "add_feature", "def": definition},
        {"expect": {"feature": "Extend1", "status": "ok"}},
    ]
    path = os.path.join(work, f"{case}.json")
    with open(path, "w", encoding="utf-8") as out:
        json.dump(script, out, indent=1)
    saved = f"{case}.mitcad"
    try:
        done = subprocess.run([cli, "run", path, "--open", os.path.join(work, "reported.mitcad"),
                               "--save", os.path.join(work, saved)],
                              capture_output=True, text=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        sys.exit(f"FAIL: Extend {case} timed out")
    check(done.returncode == 0,
          f"Extend {case} ({json.dumps(openings)}) failed ({done.returncode}):\n"
          f"{done.stdout[-3000:]}{done.stderr[-3000:]}")
    return saved


def check_extended(before, after, case, grown):
    """One surface body of the same faces, its area grown by `grown`."""
    body = after.body
    check(body.get("kind") == "sheet", f"{case}: {BODY} is not one surface body: {body.get('kind')}")
    check(body.get("faces") == FACES and after.faces == before.faces,
          f"{case}: {BODY}'s faces are {after.faces}, not the {FACES} it had")
    added = body["area"] - before.body["area"]
    check(abs(added - grown) <= AREA,
          f"{case}: {BODY}'s area grew by {added:.4f}, not by its strips' {grown:.4f}")
    check(abs(after.box("min", 2) - NEW_Z) <= LEVEL and abs(after.box("max", 2) - TOP_Z) <= LEVEL,
          f"{case}: {BODY} runs from z {after.box('min', 2)} to {after.box('max', 2)}, not from "
          f"{NEW_Z:.4f} to {TOP_Z:.4f}")
    return added


def check_box(before, after, case, moved):
    """The box's x and y sides: those in `moved` ((side, axis)) further out
    by 5 sin 15 degrees, the others where they were."""
    for side, sign in (("min", -1), ("max", 1)):
        for axis in (0, 1):
            wanted = before.box(side, axis) + (sign * DISTANCE * SIN if (side, axis) in moved else 0.0)
            check(abs(after.box(side, axis) - wanted) <= LEVEL,
                  f"{case}: {BODY}'s box {side} {'xy'[axis]} is {after.box(side, axis):.4f}, "
                  f"not {wanted:.4f}")


def check_one_edge(before, after, case, picked, new_length):
    """One free edge extended alone: the other seven in place, the strip's
    new edge `new_length` long on the new level and its two sides 5 long
    down from the picked edge's ends."""
    kept = [e for e in before.free if e is not picked]
    missing = [e for e in kept if not any(same_edge(e, a) for a in after.free)]
    check(not missing, f"{case}: unselected rim edges moved: {describe(missing)}; "
                       f"the free edges now: {describe(after.free)}")
    new = [a for a in after.free if not any(same_edge(e, a) for e in kept)]
    check(len(new) == 3, f"{case}: the strip's free edges are not its new edge and two sides: "
                         f"{describe(new)}")
    bottom = [e for e in new if at_level(e, NEW_Z)]
    check(len(bottom) == 1 and abs(bottom[0]["length"] - new_length) <= LEVEL,
          f"{case}: the new edge is not one {new_length:.4f} long at z {NEW_Z:.4f}: {describe(new)}")
    ends = [picked["start"], picked["end"]]
    for side in (e for e in new if e is not bottom[0]):
        top = max((side["start"], side["end"]), key=lambda p: p[2])
        check(abs(side["length"] - DISTANCE) <= LEVEL
              and abs(side["bbox"]["min"][2] - NEW_Z) <= LEVEL
              and min(math.dist(top, p) for p in ends) <= IN_PLACE,
              f"{case}: a side of the strip is not {DISTANCE} long down from the picked edge's "
              f"ends {ends}: {describe([side])}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    work = os.path.abspath(args.work)
    if find_design(work) is None:
        return 0
    rim = {"faces": [offset_name(f) for f in WALLS]}
    session = Session(args.cli, work)
    session.request("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                   "clientInfo": {"name": "mitcad-extend-rounded-rim-test",
                                                  "version": "1"}})
    session.notify("notifications/initialized")
    before = State(session, "reported.mitcad")
    check_before(before)
    rim_length = sum(e["length"] for e in before.free)

    # Whole Opening on: the rim, one joined surface round it.
    case = "whole rim"
    after = State(session, run_extend(args.cli, work, "whole-rim", [{"opening": rim}]))
    added = check_extended(before, after, case,
                           rim_length * DISTANCE + math.pi * DISTANCE ** 2 * SIN)
    check_box(before, after, case, {(s, a) for s in ("min", "max") for a in (0, 1)})
    free = after.free
    check(len(free) == 8 and all(at_level(e, NEW_Z) for e in free),
          f"{case}: the free edges are not one rim of eight on z {NEW_Z:.4f} (strips not joined?): "
          f"{describe(free)}")
    longer = sorted(e["length"] for e in free)
    wanted = sorted([e["length"] for e in before.free if not is_corner(e)]
                    + [CORNER + WIDENS] * 4)
    check(all(abs(a - b) <= LEVEL for a, b in zip(longer, wanted)),
          f"{case}: the new rim's edges are {longer}, not {wanted}")
    print(f"ok   {case}: one surface of {FACES} faces, {added:.4f} added, its rim of eight free "
          f"edges at z {NEW_Z:.4f}, {sum(longer):.4f} long")

    # Whole Opening off: the corner at +x +y alone, picked by a point
    # nearest it (its box's centre, inside the quarter round).
    case = "corner alone"
    corner = max((e for e in before.free if is_corner(e)), key=lambda e: centre(e, 0) + centre(e, 1))
    point = [centre(corner, k) for k in range(3)]
    after = State(session, run_extend(args.cli, work, "corner", [{"opening": rim, "at": [point]}]))
    added = check_extended(before, after, case,
                           corner["length"] * DISTANCE + math.pi / 4 * DISTANCE ** 2 * SIN)
    check_box(before, after, case, {("max", 0), ("max", 1)})
    check_one_edge(before, after, case, corner, corner["length"] + WIDENS)
    print(f"ok   {case}: {added:.4f} added, a quarter cone's strip down to z {NEW_Z:.4f}, "
          f"the other seven rim edges in place")

    # Whole Opening off: the straight side at +y alone.
    case = "side alone"
    side = max((e for e in before.free if not is_corner(e) and extent(e, 0) > 1.0),
               key=lambda e: centre(e, 1))
    point = [centre(side, k) for k in range(3)]
    after = State(session, run_extend(args.cli, work, "side", [{"opening": rim, "at": [point]}]))
    added = check_extended(before, after, case, side["length"] * DISTANCE)
    check_box(before, after, case, {("max", 1)})
    check_one_edge(before, after, case, side, side["length"])
    print(f"ok   {case}: {added:.4f} added, a rectangle's strip down to z {NEW_Z:.4f}, "
          f"the other seven rim edges in place")
    session.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
