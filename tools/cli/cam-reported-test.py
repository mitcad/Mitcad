# SPDX-License-Identifier: MIT
"""The CAM fixes of mitcad#2588 and mitcad#2591 on the reporters' own designs
(GitHub mitcad/Mitcad#745 and #749; mitcad#2592; never committed).

The reports' archives are in `github-745/` and `github-749/` under
MITCAD_MITCAD_REPORTED, each found by its content id (the first 8 hex digits
of its SHA-256), the design in it by its own, unpacked into the work folder.
mitcad-cli mcp opens each design (not read-only: the checks edit it in
memory, nothing is saved) and the test checks:

The sketch text (GitHub #745, mitcad#2588): a 90-degree engraving cutter, a
setup round the block, and a text of eight letters in a sketch on the
block's top.

- A Contour on the line whose outlines are the text itself is refused with
  what to do instead ("... Explode Text turns it into sketch curves to pick
  instead."), and the design keeps no operation. (In 0.74.0 it was refused
  with "A sketch curve is named like a curve.")
- Explode Text on the text; a Contour on the line along every curve it
  made, 0.5 mm deep, generates. Its toolpath cuts each closed outline of
  the letters once (one plunge per outline: as many as the curves make
  closed chains), and every point it cuts at the bottom lies within the box
  of the curves' control points, the cut reaching the box of their ends.

The bull nose pocket (GitHub #749, mitcad#2591): a 60 x 40 x 10 block with
a round pocket 34 mm across and 3 mm deep (at work X 30, Y 20), and four
setups pocketing it with a 6 mm flat end mill, a 6 mm bull nose with a 3 mm
corner radius and one with a 1 mm corner radius:

- A (flat, the default stepover) keeps its path: loops about the middle at
  radii 14, 11.6, 9.2, 6.8, 4.4, 2 and 0.5 (40 % of the diameter apart);
  its simulated floor is flat.
- B (bull nose r 3: no flat at its end) is refused like a ball end mill.
- C (bull nose r 1, stepover 5.5 mm) is refused: wider than the 4 mm flat
  of its end (the reporter's program of C was the flat end mill's, its
  loops 5.5 mm apart, leaving rings about 0.34 mm high). With 4 mm it
  generates loops at radii 14, 10, 6, 2 and 1 and its simulated floor has
  no ridge (every node within 15.5 mm of the middle at the floor's depth,
  within 5 µm).
- D (flat, stepover 5.5 mm) keeps its path: its posted moves are the
  reporter's program's, line for line (loops at radii 14, 8.5, 3 and 1.5),
  and its simulated floor is flat.

A design whose archive is not there is skipped with a line saying so; with
neither there the test is skipped.

  python3 cam-reported-test.py --cli <mitcad-cli> --work <folder>
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

SKIP = "cam reported: skipped"
TIMEOUT = 600
PROTOCOL = "2025-11-25"

# GitHub #745: the archive and the design in it.
TEXT_FOLDER = "github-745"
TEXT_ARCHIVE = "bbe4fd7e"
TEXT_DESIGN = "3c32d0ad"
TEXT_REFUSED = ("A CAM operation cannot follow a sketch text: Explode Text turns it into "
                "sketch curves to pick instead.")
ENGRAVE_DEPTH = 0.5

# GitHub #749: the archive, the design and setup D's program in it.
POCKET_FOLDER = "github-749"
POCKET_ARCHIVE = "17542f71"
POCKET_DESIGN = "2f45fd67"
POCKET_PROGRAM_D = "c11972ef"
MIDDLE = (30.0, 20.0)  # the pocket's middle in work X and Y
FLOOR = -3.0  # its floor in work Z
FLAT_REACH = 15.5  # the floor's nodes checked: this far from the middle
RIDGE = 0.005  # the most a floor node may stand above the floor (mm)
BULL_REFUSED = ("The operation needs an end mill with a flat end; this bull nose's corner "
                "radius is its whole radius, so its end is round like a ball end mill's.")
STEPOVER_REFUSED = ("The stepover must not be more than the flat of the tool's end, 4 mm (its "
                    "diameter less twice its corner radius): wider passes leave ridges on the "
                    "floor between them.")
LOOPS_A = [14.0, 11.6, 9.2, 6.8, 4.4, 2.0, 0.5]
LOOPS_C = [14.0, 10.0, 6.0, 2.0, 1.0]
LOOPS_D = [14.0, 8.5, 3.0, 1.5]


def content_id(data):
    return hashlib.sha256(data).hexdigest()[:8]


def unpack(work, folder_name, archive_id, wanted):
    """The entries of the archive `archive_id` in `folder_name` whose content
    ids are in `wanted` ({id: file name}), written into `work`; None when
    the archive is not there."""
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, folder_name)
    if not reported or not os.path.isdir(folder):
        print(f"skip {folder_name}: MITCAD_MITCAD_REPORTED/{folder_name} is not there")
        return None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if not (name.endswith(".zip") and os.path.isfile(path)):
            continue
        with open(path, "rb") as f:
            if content_id(f.read()) != archive_id:
                continue
        found = {}
        with zipfile.ZipFile(path) as archive:
            for entry in archive.infolist():
                data = archive.read(entry)
                key = content_id(data)
                if key in wanted:
                    os.makedirs(work, exist_ok=True)
                    out_path = os.path.join(work, wanted[key])
                    with open(out_path, "wb") as out:
                        out.write(data)
                    found[key] = out_path
        missing = [key for key in wanted if key not in found]
        if missing:
            sys.exit(f"FAIL: the archive {archive_id} holds nothing with the content id {missing}")
        return found
    print(f"skip {folder_name}: no archive {archive_id} in {folder}")
    return None


class Session:
    """mitcad-cli mcp over stdin and stdout, a reader thread for timeouts."""

    def __init__(self, cli, workspace):
        self.stderr = open(os.path.join(workspace, "mcp-stderr.txt"), "wb")
        self.process = subprocess.Popen([cli, "mcp", "--workspace", workspace],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=self.stderr)
        self.messages = queue.Queue()
        self.next_id = 0
        threading.Thread(target=self.read, daemon=True).start()
        self.request("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                    "clientInfo": {"name": "mitcad-cam-reported-test",
                                                   "version": "1"}})
        self.notify("notifications/initialized")

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

    def call(self, name, arguments):
        """The tool's answer and whether it was an error."""
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        content = result.get("structuredContent", {})
        return content, bool(result.get("isError"))

    def tool(self, name, arguments):
        content, error = self.call(name, arguments)
        if error:
            sys.exit(f"FAIL: {name} {json.dumps(arguments)[:400]}: {content}")
        return content

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=TIMEOUT)
        self.stderr.close()


class Design:
    """A design opened in a session."""

    def __init__(self, session, path):
        self.session = session
        self.document = session.tool("document_open", {"path": path})["document"]

    def query(self, request):
        return self.session.tool("model_query",
                                 {"document": self.document, "query": request})["result"]

    def command(self, command):
        result = self.session.tool("model_command",
                                   {"document": self.document, "command": command})["result"]
        return json.loads(result) if isinstance(result, str) else result

    def refused(self, command):
        """The message a refused command gives."""
        content, error = self.session.call("model_command",
                                           {"document": self.document, "command": command})
        check(error, f"{command['cmd']} was taken: {json.dumps(content)[:400]}")
        return str(content.get("error", content))


def check(condition, message):
    if not condition:
        sys.exit(f"FAIL: {message}")


def to_work(frame, p):
    """A model point in a setup's work coordinates (`work`: origin and
    axes in the model)."""
    d = [p[k] - frame["origin"][k] for k in range(3)]
    return [sum(d[k] * frame[axis][k] for k in range(3)) for axis in ("x", "y", "z")]


def feeds_at(path, z):
    """The ends of the feed moves (lines and arcs) at height `z`."""
    return [m["to"] for m in path["moves"]
            if m["kind"] != "rapid" and abs(m["to"][2] - z) < 1e-6]


def plunges(path, z):
    """How often the path goes down to `z` from above it."""
    count, at = 0, None
    for m in path["moves"]:
        if at is not None and at[2] > z + 1e-6 and abs(m["to"][2] - z) < 1e-6:
            count += 1
        at = m["to"]
    return count


def loop_radii(path, z):
    """The radii about the pocket's middle of the arcs cut at `z`, largest
    first, each once."""
    radii = set()
    at = None
    for m in path["moves"]:
        if m["kind"] == "arc" and at is not None and abs(at[2] - z) < 1e-6 \
                and abs(m["to"][2] - z) < 1e-6:
            c = m["center"]
            check(math.hypot(c[0] - MIDDLE[0], c[1] - MIDDLE[1]) < 1e-6,
                  f"an arc about ({c[0]}, {c[1]}), not about the pocket's middle")
            radii.add(round(math.hypot(m["to"][0] - c[0], m["to"][1] - c[1]), 6))
        at = m["to"]
    return sorted(radii, reverse=True)


def toolpath(design, setup, name):
    """The operation `name` of `setup` as cam_toolpaths computes it now."""
    answer = design.query({"query": "cam_toolpaths", "setup": setup})
    operation = next((o for o in answer["operations"] if o.get("name") == name), None)
    check(operation is not None,
          f"no {name} in {setup}: {[o.get('name') for o in answer['operations']]}")
    return operation


def generated(design, setup):
    """cam_generate's operations of `setup`."""
    result = design.command({"cmd": "cam_generate", "setup": setup})
    return result["setups"][0]["operations"]


def floor_ridge(design, setup):
    """The highest node within FLAT_REACH of the pocket's middle above the
    floor, by cam_simulate of the whole setup on a 0.1 mm grid (mm)."""
    simulated = design.query({"query": "cam_simulate", "setup": setup, "resolution": 0.1})
    grid = simulated["grid"]
    heights = simulated["heights"]
    nx, ny = grid["nx"], grid["ny"]
    origin, step = grid["origin"], grid["step"]
    highest, checked, k = -math.inf, 0, 0
    for i in range(0, len(heights), 2):
        z, count = heights[i] / 1000.0, heights[i + 1]
        for n in range(k, k + count):
            x = origin[0] + (n % nx) * step[0]
            y = origin[1] + (n // nx) * step[1]
            if math.hypot(x - MIDDLE[0], y - MIDDLE[1]) <= FLAT_REACH:
                checked += 1
                highest = max(highest, z)
        k += count
    check(k == nx * ny, f"{setup}: the heights cover {k} nodes of {nx} x {ny}")
    check(checked > 1000, f"{setup}: only {checked} nodes on the floor")
    return highest - FLOOR, checked


def motion_lines(text):
    """The moves (G0 to G3) of a program, line for line."""
    lines = [line.strip() for line in text.splitlines()]
    return [line for line in lines if line.split(" ")[0] in ("G0", "G1", "G2", "G3")]


def sketch_text(path):
    """The sketch holding a text in the design file, the text's id and the
    sketch's plane (origin and axes in the model)."""
    with open(path, encoding="utf-8") as f:
        document = json.load(f)
    for feature in document["features"]:
        texts = feature.get("texts") or []
        if feature.get("type") == "sketch" and texts:
            placed = feature["plane"]["placed"]
            return feature["uid"], texts[0]["id"], placed
    sys.exit("FAIL: no sketch with a text in the design")


def curve_ends_and_hull(entity):
    """A curve's two ends and the points whose box holds it (a spline's
    control points, the ends of a line)."""
    geometry = entity.get("geometry") or {}
    if "control" in geometry:
        control = geometry["control"]
        return (control[0], control[-1]), control
    check("start" in geometry and "end" in geometry,
          f"a made curve that is neither a line nor a spline: {entity}")
    return (geometry["start"], geometry["end"]), [geometry["start"], geometry["end"]]


def chains(ends):
    """How many connected chains the curves' ends make (ends meeting within
    a micrometre)."""
    parent = {}

    def key(p):
        return (round(p[0] * 1000.0), round(p[1] * 1000.0))

    def find(a):
        while parent.setdefault(a, a) != a:
            parent[a] = parent[parent[a]]
            a = parent[a]
        return a

    for a, b in ends:
        parent[find(key(a))] = find(key(b))
    return len({find(p) for p in list(parent)})


def check_text(session, design_path):
    design = Design(session, os.path.basename(design_path))
    sketch, text, placed = sketch_text(design_path)
    setups = design.query({"query": "cam_setups"})["setups"]
    check(len(setups) == 1, f"the design has {len(setups)} setups")
    setup = setups[0]
    before = len(setup.get("operations", []))

    def contour(curves):
        return {"cmd": "cam_operation_add", "setup": setup["id"], "operation": {
            "kind": {"type": "contour", "side": "on"}, "tool": 1,
            "depths": {"bottom": -ENGRAVE_DEPTH},
            "inputs": [{"kind": "curves", "sketch": sketch, "curves": curves}]}}

    # The text picked as an outline: refused with Explode Text named.
    message = design.refused(contour([text]))
    check(TEXT_REFUSED in message, f"the text as an outline: {message}")
    after = design.query({"query": "cam_setups"})["setups"][0]
    check(len(after.get("operations", [])) == before,
          f"the refused contour left operations: {after.get('operations')}")
    print(f"ok   the text as a Contour outline is refused: {message}")

    # Explode Text, then a contour on the line along every curve it made.
    exploded = design.command({"cmd": "sketch.explode_text", "sketch": sketch, "text": text})
    made = exploded["made"]
    check(len(made) > 8, f"Explode Text made {len(made)} curves: {exploded}")
    entities = {e["id"]: e for e in design.query({"query": "sketch", "uid": sketch})["entities"]}
    ends, hull = [], []
    for curve in made:
        check(curve in entities, f"no curve {curve} in the sketch")
        pair, points = curve_ends_and_hull(entities[curve])
        ends.append(pair)
        hull.extend(points)
    outlines = chains(ends)
    added = design.command(contour(made))
    operation_id = added["operation"]
    operations = generated(design, setup["name"])
    engraving = next(o for o in operations if o["id"] == operation_id)
    check(engraving["state"] == "up_to_date" and not engraving.get("error"),
          f"the contour along the letters: {engraving}")
    path = toolpath(design, setup["name"], engraving["name"])
    check(not path.get("error"), f"{engraving['name']}: {path.get('error')}")

    # In work coordinates: the sketch's points on its plane, then the setup's frame.
    frame = design.query({"query": "cam_setups"})["setups"][0]["work"]

    def work_xy(p):
        model = [placed["origin"][k] + p[0] * placed["x_axis"][k] + p[1] * placed["y_axis"][k]
                 for k in range(3)]
        return to_work(frame, model)[:2]

    def box(points):
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        return (min(xs), min(ys)), (max(xs), max(ys))

    end_box = box([work_xy(p) for pair in ends for p in pair])
    hull_box = box([work_xy(p) for p in hull])
    bottom = feeds_at(path["toolpath"], -ENGRAVE_DEPTH)
    check(bottom, f"{engraving['name']} cuts nothing at {-ENGRAVE_DEPTH}")
    cut_box = box(bottom)
    tolerance = 0.01
    for k in range(2):
        check(cut_box[0][k] >= hull_box[0][k] - tolerance
              and cut_box[1][k] <= hull_box[1][k] + tolerance,
              f"the cut {cut_box} leaves the letters' box {hull_box}")
        check(cut_box[0][k] <= end_box[0][k] + tolerance
              and cut_box[1][k] >= end_box[1][k] - tolerance,
              f"the cut {cut_box} does not reach the letters' ends {end_box}")
    entered = plunges(path["toolpath"], -ENGRAVE_DEPTH)
    check(entered == outlines,
          f"{engraving['name']} goes down {entered} times for {outlines} closed outlines")
    print(f"ok   after Explode Text ({len(made)} curves, {outlines} outlines) the contour on "
          f"the line generates: {entered} outlines cut {ENGRAVE_DEPTH} mm deep within "
          f"x {cut_box[0][0]:.3f} to {cut_box[1][0]:.3f}, y {cut_box[0][1]:.3f} to "
          f"{cut_box[1][1]:.3f}")


def check_pocket(session, design_path, program_d_path):
    design = Design(session, os.path.basename(design_path))
    setups = {s["name"]: s for s in design.query({"query": "cam_setups"})["setups"]}
    names = {letter: next((n for n in setups if n.startswith(letter + " ")), None)
             for letter in "ABCD"}
    check(all(names.values()), f"the setups A to D: {list(setups)}")

    def operation(letter):
        setup = setups[names[letter]]
        check(len(setup["operations"]) == 1, f"{names[letter]}: {setup['operations']}")
        return setup["operations"][0]

    def loops(letter, wanted):
        path = toolpath(design, names[letter], operation(letter)["name"])
        check(not path.get("error"), f"{names[letter]}: {path.get('error')}")
        radii = loop_radii(path["toolpath"], FLOOR)
        check(len(radii) == len(wanted)
              and all(abs(a - b) < 1e-6 for a, b in zip(radii, wanted)),
              f"{names[letter]}: loops at radii {radii}, not {wanted}")
        return radii

    def flat(letter):
        ridge, nodes = floor_ridge(design, names[letter])
        check(ridge <= RIDGE, f"{names[letter]}: the floor has a ridge {ridge:.4f} mm high")
        return ridge, nodes

    # A: the flat end mill at its default stepover keeps its loops.
    radii = loops("A", LOOPS_A)
    ridge, nodes = flat("A")
    print(f"ok   {names['A']}: loops at radii {radii}; the floor flat "
          f"({nodes} nodes, at most {ridge:.4f} mm above it)")

    # B and C (5.5 mm) refused, in Generate and in the toolpaths.
    for letter, wanted in (("B", BULL_REFUSED), ("C", STEPOVER_REFUSED)):
        listed = generated(design, names[letter])
        check(len(listed) == 1, f"{names[letter]}: {listed}")
        check(listed[0]["state"] == "failed" and listed[0].get("error") == wanted,
              f"{names[letter]}: {listed[0]}")
        path = toolpath(design, names[letter], operation(letter)["name"])
        check(path.get("error") == wanted, f"{names[letter]}'s toolpath: {path.get('error')}")
        print(f"ok   {names[letter]} is refused: {wanted}")

    # C with 4 mm: the bull nose's flat, loops 4 mm apart, a floor without ridges.
    design.command({"cmd": "cam_operation_edit", "operation": operation("C")["id"],
                    "changes": {"stepover": "4 mm"}})
    listed = generated(design, names["C"])
    check(listed[0]["state"] == "up_to_date" and not listed[0].get("error"),
          f"{names['C']} with 4 mm: {listed[0]}")
    radii = loops("C", LOOPS_C)
    ridge, nodes = flat("C")
    print(f"ok   {names['C']} with a 4 mm stepover: loops at radii {radii}; the floor has no "
          f"ridge ({nodes} nodes, at most {ridge:.4f} mm above it)")

    # D: the flat end mill at 5.5 mm keeps the reporter's program.
    radii = loops("D", LOOPS_D)
    ridge, nodes = flat("D")
    posted = design.query({"query": "cam_post", "setup": names["D"]})["text"]
    with open(program_d_path, encoding="utf-8") as f:
        reported = f.read()
    ours, theirs = motion_lines(posted), motion_lines(reported)
    check(theirs, "the reporter's program of D has no moves")
    check(ours == theirs,
          f"{names['D']}'s moves differ from the reporter's program:\n" + "\n".join(
              f"  {a!r} | {b!r}" for a, b in zip(ours, theirs) if a != b)
          + f"\n  ({len(ours)} moves, the reporter's {len(theirs)})")
    print(f"ok   {names['D']}: loops at radii {radii}, its {len(ours)} moves the reporter's "
          f"program's; the floor flat (at most {ridge:.4f} mm above it)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    work = os.path.abspath(args.work)
    text = unpack(work, TEXT_FOLDER, TEXT_ARCHIVE, {TEXT_DESIGN: "text.mitcad"})
    pocket = unpack(work, POCKET_FOLDER, POCKET_ARCHIVE,
                    {POCKET_DESIGN: "pocket.mitcad", POCKET_PROGRAM_D: "pocket-d.nc"})
    if text is None and pocket is None:
        print(f"{SKIP} (neither reported design is there)")
        return 0
    session = Session(args.cli, work)
    if text is not None:
        check_text(session, text[TEXT_DESIGN])
    if pocket is not None:
        check_pocket(session, pocket[POCKET_DESIGN], pocket[POCKET_PROGRAM_D])
    session.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
