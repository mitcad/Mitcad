# SPDX-License-Identifier: MIT
"""Lofts along two rails with the real kernel (mitcad#2373, a follow-up of
mitcad#2320): the reporters' designs of GitHub #607 and #610 and the
four-section bottle of a forum post.

`--case forum` runs `tests/loft_rails_four_sections.json` (the forum's
bottle built from scratch: rounded rectangles at z = 0 and 100, circles at
200 and 230, two mirrored spline rails; Loft1 with Tangent Edges Merge,
Loft2 with Keep), saves the design and checks each loft on it:

- it is its own mirror image in x within 1e-6 mm: the body is mirrored
  across the YZ plane as a body of its own (Mirror), that body is exported
  to STEP and `compare_step` measures the largest distance between the
  two bodies' surfaces (sampled both ways, exact distances; the volume
  booleans of the two coincident bodies, which took five minutes each, are
  left out with `booleans_above`);
- both rails lie on it: six points along each of the rails' curves are
  within 1e-4 mm of the body and not inside it (`measure`: `inside` is
  false for a point within 1e-3 mm of the boundary). A construction point
  along a path takes only lines and arcs of a sketch, so the points are
  worked out here: the rails' sketch is exported to DXF (`mitcad-cli
  export-sketch`), each spline evaluated from its control points and
  knots (a line from its ends as the design stores them), placed by the
  sketch's frame, and added as fixed construction points.

`--case reported` opens the reporters' designs, each in an archive under
MITCAD_MITCAD_REPORTED found by its content id (the first 8 hex digits of
its SHA-256), the design in it by its own, unpacked into the work folder
(skipped when neither is there):

- GitHub #607 (`github-607/`): the design's own Loft1 (two rounded
  rectangles to a circle, two mirrored spline rails, Merge) builds, is its
  own mirror image in x and both rails lie on it, as above;
- GitHub #610 (`github-610/`): the design has the sections and the rails
  but no loft. Each rail is a chain of a spline and a line (c1 then c27,
  c15 then c28); the first spline meets its line with one tangent, the
  second one at an angle (the sketch has no tangency there). A loft of
  the three sections with Merge along both chains, along each chain alone,
  and the same with the splines' second fit points from the top moved
  from 16 mm to 15 mm off the centre line (the reporter's variant), each
  builds and its rails lie on it. The rails are not mirror images of each
  other, so the body is not checked against its mirror.

  python3 loft-rails-reported-test.py --cli <mitcad-cli> --work <folder> --case forum|reported
"""

import argparse
import copy
import hashlib
import json
import os
import re
import subprocess
import sys
import time
import zipfile

SKIP = "loft rails reported: skipped"
# --verbose: each run's time and its slow steps (mitcad#2373).
VERBOSE = False

MIRROR = 1e-6  # mm, the body against its mirror image
ON_RAIL = 1e-4  # mm, a rail's point off the body
SAME_POINT = 1e-3  # mm, a spline's end against its stored fit point (which spline is which)
SAMPLES = 5  # pieces along each rail curve (a measure of a point against a lofted body takes about a second)
COMPARED = 400  # points sampled on each side of the mirror comparison

DESIGNS = {
    "607": {"folder": "github-607", "archive": "805aa7e5", "design": "e7c53b7a"},
    "610": {"folder": "github-610", "archive": "d1de5480", "design": "e66abec2"},
}

# The sections of #607's and #610's designs (the same sketches): the
# rounded rectangles of Sketch1 and Sketch2 and the circle of Sketch3.
RECTANGLE = "r{c1[c11,c15],c2[c15,c19],c3[c19,c23],c4[c23,c11],c11[c4,c1],c15[c1,c2],c19[c2,c3],c23[c3,c4]}"
SECTIONS_610 = [
    {"type": "profile", "sketch": "F1", "region": RECTANGLE},
    {"type": "profile", "sketch": "F3", "region": RECTANGLE},
    {"type": "profile", "sketch": "F5", "region": "r{c2}"},
]
CHAINS_610 = {"first": ["c1", "c27"], "second": ["c15", "c28"]}
# The splines' second fit points from the top, mirror images of each other.
VARIANT_POINTS = {"p5": 15.0, "p19": -15.0}

# The origin planes' own frames (x, y); a sketch's `frame` is given in them.
PLANES = {
    "xy": ((1.0, 0.0, 0.0), (0.0, 1.0, 0.0)),
    "xz": ((1.0, 0.0, 0.0), (0.0, 0.0, -1.0)),
    "yz": ((0.0, 0.0, -1.0), (0.0, 1.0, 0.0)),
}


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def check(condition, message):
    if not condition:
        fail(message)


def content_id(data):
    return hashlib.sha256(data).hexdigest()[:8]


def find_design(work, which):
    """The design of report `which` unpacked into `work` and its contents,
    or None after a note."""
    spec = DESIGNS[which]
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, spec["folder"])
    if not reported or not os.path.isdir(folder):
        print(f"not here: #{which} (MITCAD_MITCAD_REPORTED/{spec['folder']} is not there)")
        return None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if not name.lower().endswith(".zip") or not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            if content_id(f.read()) != spec["archive"]:
                continue
        with zipfile.ZipFile(path) as z:
            for info in z.infolist():
                if not info.filename.lower().endswith(".mitcad"):
                    continue
                data = z.read(info)
                if content_id(data) == spec["design"]:
                    os.makedirs(work, exist_ok=True)
                    design = os.path.join(work, f"reported-{which}.mitcad")
                    with open(design, "wb") as out:
                        out.write(data)
                    return design, json.loads(data)
        fail(f"the archive {spec['archive']} holds no design {spec['design']}")
    print(f"not here: #{which} (no archive {spec['archive']} in {folder})")
    return None


def run(cli, work, case, steps, design=None, save=None):
    """mitcad-cli run of `steps` (on the design, else a new one); its JSON
    report. A failed expectation fails the test with the CLI's words."""
    script = os.path.join(work, f"{case}.json")
    with open(script, "w", encoding="utf-8") as f:
        json.dump(steps, f, indent=1)
    args = [cli, "run", script, "--json"]
    if design:
        args += ["--open", design]
    if save:
        args += ["--save", save]
    if VERBOSE:
        args.append("--timings")
    started = time.monotonic()
    done = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    if VERBOSE:
        # The run's time, its steps of a second or more and the lines
        # MITCAD_TRACE_LOFT_BENDS writes.
        print(f"time {case}: {time.monotonic() - started:.1f} s")
        for line in (done.stdout + done.stderr).splitlines():
            slow = re.fullmatch(r"Timing steps\[\d+\] .*: ([0-9.]+) ms", line)
            if (slow and float(slow.group(1)) >= 1000.0) or line.startswith("loft bends:"):
                print(f"  {line}")
    if done.returncode != 0:
        fail(f"{case}: mitcad-cli run failed ({done.returncode}):\n{done.stdout}{done.stderr}")
    out = done.stdout
    start = out.find("{")
    if start < 0:
        fail(f"{case}: no report on stdout:\n{out}{done.stderr}")
    try:
        report, _ = json.JSONDecoder().raw_decode(out[start:])
    except ValueError as error:
        fail(f"{case}: the report is not JSON ({error}):\n{out}")
    return report


class Uids:
    """The uids the features added by a script get: one past the design's
    largest, or its `next_feature` when that is further (as a design is
    read, mitcad#1320)."""

    def __init__(self, stored):
        numbers = [int(m.group(1)) for f in stored.get("features", [])
                   if (m := re.fullmatch(r"F(\d+)", str(f.get("uid", ""))))]
        self.next = max([1] + [n + 1 for n in numbers] + [int(stored.get("next_feature") or 0)])

    def take(self):
        uid = f"F{self.next}"
        self.next += 1
        return uid


# --- The rails' points

def dxf_splines(path):
    """The SPLINE entities of a DXF file: degree, knots, control points
    (x, y) and weights (empty when not rational)."""
    with open(path, encoding="utf-8", errors="replace") as f:
        lines = [line.strip() for line in f]
    splines = []
    current = None
    for i in range(0, len(lines) - 1, 2):
        try:
            code = int(lines[i])
        except ValueError:
            continue
        value = lines[i + 1]
        if code == 0:
            current = {"degree": 3, "knots": [], "points": [], "weights": []} if value == "SPLINE" else None
            if current is not None:
                splines.append(current)
            continue
        if current is None:
            continue
        if code == 71:
            current["degree"] = int(value)
        elif code == 40:
            current["knots"].append(float(value))
        elif code == 41:
            current["weights"].append(float(value))
        elif code == 10:
            current["points"].append([float(value), 0.0])
        elif code == 20:
            current["points"][-1][1] = float(value)
    return splines


def spline_point(spline, t):
    """De Boor's point of the spline at t (homogeneous when rational)."""
    p = spline["degree"]
    knots = spline["knots"]
    weights = spline["weights"] or [1.0] * len(spline["points"])
    n = len(spline["points"]) - 1
    k = p
    while k < n and t >= knots[k + 1]:
        k += 1
    d = [[spline["points"][j][0] * weights[j], spline["points"][j][1] * weights[j], weights[j]]
         for j in range(k - p, k + 1)]
    for r in range(1, p + 1):
        for j in range(p, r - 1, -1):
            i = j + k - p
            denominator = knots[i + p - r + 1] - knots[i]
            a = 0.0 if denominator == 0 else (t - knots[i]) / denominator
            d[j] = [(1 - a) * d[j - 1][c] + a * d[j][c] for c in range(3)]
    return (d[p][0] / d[p][2], d[p][1] / d[p][2])


def along(spline):
    p = spline["degree"]
    t0 = spline["knots"][p]
    t1 = spline["knots"][-p - 1]
    return [spline_point(spline, t0 + (t1 - t0) * j / SAMPLES) for j in range(SAMPLES + 1)]


def sketch_frame(sketch):
    """The sketch's origin and axes in the component (x, y)."""
    if sketch["plane"] not in PLANES:
        fail(f"{sketch['uid']} is on {sketch['plane']}: only sketches on origin planes are read here")
    x_axis, y_axis = PLANES[sketch["plane"]]
    normal = (x_axis[1] * y_axis[2] - x_axis[2] * y_axis[1],
              x_axis[2] * y_axis[0] - x_axis[0] * y_axis[2],
              x_axis[0] * y_axis[1] - x_axis[1] * y_axis[0])
    frame = sketch.get("frame") or {"origin": [0, 0, 0], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0]}

    def placed(v):
        return tuple(v[0] * x_axis[c] + v[1] * y_axis[c] + v[2] * normal[c] for c in range(3))

    return placed(frame["origin"]), placed(frame["x_axis"]), placed(frame["y_axis"])


def rail_points(cli, work, case, design, stored, sketch_uid, curves):
    """Points along the sketch's curves in the component's coordinates."""
    sketch = next(f for f in stored["features"] if f["uid"] == sketch_uid)
    entities = {e["id"]: e for e in sketch["entities"]}
    dxf = os.path.join(work, f"{case}-{sketch_uid}.dxf")
    done = subprocess.run([cli, "export-sketch", design, sketch_uid, dxf, "--unit", "mm"],
                          capture_output=True, text=True, encoding="utf-8", errors="replace")
    if done.returncode != 0:
        fail(f"{case}: mitcad-cli export-sketch failed ({done.returncode}):\n{done.stdout}{done.stderr}")
    splines = dxf_splines(dxf)
    origin, x_axis, y_axis = sketch_frame(sketch)

    def at(name):
        return tuple(entities[name]["at"])

    def near(a, b):
        return ((a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2) ** 0.5 <= SAME_POINT

    points = []
    for curve in curves:
        entity = entities[curve]
        if entity["type"] == "line":
            a, b = at(entity["start"]), at(entity["end"])
            local = [(a[0] + (b[0] - a[0]) * j / SAMPLES, a[1] + (b[1] - a[1]) * j / SAMPLES)
                     for j in range(SAMPLES + 1)]
        elif entity["type"] == "fitted_spline":
            first, last = at(entity["points"][0]), at(entity["points"][-1])
            local = None
            for spline in splines:
                ends = along(spline)
                if (near(ends[0], first) and near(ends[-1], last)) or \
                        (near(ends[0], last) and near(ends[-1], first)):
                    local = ends
                    break
            check(local is not None, f"{case}: no spline of the DXF of {sketch_uid} runs as {curve} does")
        else:
            fail(f"{case}: {sketch_uid}'s {curve} is a {entity['type']}, not a line or a fitted spline")
        for u, v in local:
            points.append([origin[c] + u * x_axis[c] + v * y_axis[c] for c in range(3)])
    return points


# --- Checks

def mirror_steps(uids, work, case, body):
    """Steps that check `body` against its mirror image in x."""
    mirror = uids.take()
    step = os.path.join(work, f"{case}-{body}-mirrored.step")
    return [
        {"comment": f"{body} mirrored across YZ as a body of its own, exported and compared with {body}."},
        {"cmd": "add_feature", "def": {"type": "mirror", "objects": {"type": "bodies", "bodies": [body]},
                                       "plane": {"type": "origin", "plane": "yz"}}},
        {"expect": {"feature": mirror, "status": "ok"}},
        {"cmd": "export", "path": step, "bodies": [f"{mirror}.b0"]},
        {"expect": {"query": {"query": "compare_step", "file": step, "bodies": [body], "samples": COMPARED,
                              "booleans_above": 1e-3},
                    "result": {"max_deviation": 0}, "tolerance": MIRROR}},
    ]


def on_body_steps(uids, body, points, what):
    """Steps that check that the points lie on `body`."""
    steps = [{"comment": f"{what} on {body}."}]
    for point in points:
        uid = uids.take()
        steps.append({"cmd": "add_feature", "def": {
            "type": "construction_point", "definition": {"type": "fixed", "point": point}}})
        steps.append({"expect": {
            "query": {"query": "measure", "a": {"datum": uid}, "b": {"body": body}},
            "result": {"between": {"distance": 0, "inside": False}}, "tolerance": ON_RAIL}})
    return steps


def loft_status(report, uid):
    for feature in report["timeline"]["features"]:
        if feature["uid"] == uid:
            return feature
    fail(f"no feature {uid} in the report")
    return None


def forum(cli, work, scripts):
    """The four-section bottle built from scratch."""
    with open(os.path.join(scripts, "loft_rails_four_sections.json"), encoding="utf-8") as f:
        steps = json.load(f)
    design = os.path.join(work, "forum.mitcad")
    run(cli, work, "forum-build", steps, save=design)
    with open(design, encoding="utf-8") as f:
        stored = json.load(f)
    rails = {curve: rail_points(cli, work, "forum", design, stored, "F8", [curve]) for curve in ("c13", "c14")}
    uids = Uids(stored)
    checks = []
    for body in ("F9.b0", "F10.b0"):
        checks += mirror_steps(uids, work, "forum", body)
        for curve, points in rails.items():
            checks += on_body_steps(uids, body, points, f"the rail {curve}")
    run(cli, work, "forum-checks", checks, design)
    print("ok   the four-section bottle builds with Merge and with Keep, each its own mirror image in x "
          f"within {MIRROR} mm, both rails on it within {ON_RAIL} mm")


def reported_607(cli, work):
    found = find_design(work, "607")
    if found is None:
        return False
    design, stored = found
    loft = next((f for f in stored["features"] if f.get("type") == "loft"), None)
    check(loft is not None, "#607's design has no loft")
    body = f"{loft['uid']}.b0"
    uids = Uids(stored)
    steps = [{"comment": "GitHub #607 (mitcad#2373): the design's own loft."},
             {"expect": {"feature": loft["uid"], "status": "ok"}},
             {"expect": {"bodies": 1}}]
    steps += mirror_steps(uids, work, "607", body)
    for rail in loft["rails"]:
        points = rail_points(cli, work, "607", design, stored, rail["sketch"], rail["curves"])
        steps += on_body_steps(uids, body, points, f"the rail {', '.join(rail['curves'])}")
    run(cli, work, "607", steps, design)
    print(f"ok   #607's {loft['name']} builds, its own mirror image in x within {MIRROR} mm, "
          f"both rails on it within {ON_RAIL} mm")
    return True


def reported_610(cli, work):
    found = find_design(work, "610")
    if found is None:
        return False
    design, stored = found
    # The reporter's variant: the splines' second fit points from the top
    # 15 mm off the centre line, as the first spline's top point and line.
    variant = copy.deepcopy(stored)
    sketch = next(f for f in variant["features"] if f["uid"] == "F6")
    moved = 0
    for entity in sketch["entities"]:
        if entity["id"] in VARIANT_POINTS:
            entity["at"][0] = VARIANT_POINTS[entity["id"]]
            moved += 1
    check(moved == len(VARIANT_POINTS), f"#610's Sketch4 has {moved} of the points to move")
    variant_design = os.path.join(work, "reported-610-15mm.mitcad")
    with open(variant_design, "w", encoding="utf-8") as f:
        json.dump(variant, f)

    for label, path, contents in (("as saved", design, stored), ("15 mm", variant_design, variant)):
        tag = label.replace(" ", "")
        rails = {c: rail_points(cli, work, f"610-{tag}-{c}", path, contents, "F6", CHAINS_610[c])
                 for c in CHAINS_610}
        for chains in (["first", "second"], ["first"], ["second"]):
            uids = Uids(contents)
            loft = uids.take()
            body = f"{loft}.b0"
            case = f"610-{tag}-{'-'.join(chains)}"
            steps = [{"comment": f"GitHub #610 (mitcad#2373), {label}: a loft along the "
                                 f"{' and '.join(chains)} chain."},
                     {"cmd": "add_feature", "def": {
                         "type": "loft", "sections": SECTIONS_610,
                         "rails": [{"sketch": "F6", "curves": CHAINS_610[c]} for c in chains],
                         "tangent_edges": "merge", "operation": "new_body"}},
                     {"expect": {"feature": loft, "status": "ok"}},
                     {"expect": {"bodies": 1}}]
            for c in chains:
                steps += on_body_steps(uids, body, rails[c], f"the {c} chain")
            report = run(cli, work, case, steps, path)
            feature = loft_status(report, loft)
            check(feature["status"] == "ok", f"{case}: the loft is {feature['status']}: {feature.get('error')}")
            print(f"ok   #610 {label}: the loft along the {' and '.join(chains)} chain builds, "
                  f"its rails on it within {ON_RAIL} mm")
    return True


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    parser.add_argument("--case", choices=["forum", "reported"], required=True)
    parser.add_argument("--verbose", action="store_true",
                        help="print each run's time, its steps of a second or more and the loft bends' trace")
    options = parser.parse_args()
    global VERBOSE
    VERBOSE = options.verbose
    work = os.path.abspath(options.work)
    os.makedirs(work, exist_ok=True)
    if options.case == "forum":
        forum(options.cli, work, os.path.join(os.path.dirname(os.path.abspath(__file__)), "tests"))
        return 0
    ran = [reported_607(options.cli, work), reported_610(options.cli, work)]
    if not any(ran):
        # The skip line only when neither design is here (ctest's skip).
        print(f"{SKIP} (neither design is under MITCAD_MITCAD_REPORTED)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
