# SPDX-License-Identifier: MIT
"""The shafts' outlines of a reported subassembly drawing at four scales
(mitcad#2539, GitHub mitcad/Mitcad#729 and #765; never committed).

The report's archive is in `github-765/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, unpacked into the work folder. The design is a component
the reporter isolated and saved as a design of its own: a frame of two
upright square tubes, two shafts through holes in them, and between the
uprights two rollers (a tube on two bearings, each held by a clamping collar
on the shaft). Its Drawing1 has one front view (toward the eye -y, x to the
right) on an A3 sheet at 1:5, without hidden and tangent edges; it takes the
quick path (the collars and bearings are free-form), so its triangulation
follows the view's scale.

For each of 1:1, 1:2, 1:5 and 1:10, with the view's own edge settings and
with hidden edges on, a copy of the design whose view has that scale is
written as SVG by export-drawing with `MITCAD_DRAWING_TRACE=strokes`, and
the test reads the lines the sheet draws (`stroke sheet`) and checks:

- the view projects the same 102 bodies at every scale (the component's,
  nothing else);
- both outline lines of each shaft are drawn seen without a gap where the
  shafts stick out to the left of the left upright (nothing is in front of
  them there), and with hidden edges on no hidden line runs there;
- with the view's settings no hidden and no tangent edge is drawn; with
  hidden edges on hidden lines are drawn (the shafts inside the uprights
  and the rollers).

Before the fix of the polygonal hidden-line removal (OCCT port patch 2539),
at 1:2 and 1:5 a sliver of a collar's bore lying along a shaft's outline hid
that outline from the collar to the shaft's end: the left stubs lost one or
both lines. Skipped when the folder or the archive is not there.

  python3 drawing-shaft-outlines-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import math
import os
import re
import subprocess
import sys
import zipfile

ARCHIVE = "da2e372a"  # the reporter's archive
DESIGN = "9f43a933"  # the design in it
FOLDER = "github-765"
SKIP = "drawing shaft outlines reported: skipped"
TIMEOUT = 600
BODIES = 102

# The shafts' outline lines in the view's millimetres (x the model's x, y
# its z), from end to end, and the stretch left of the left upright (whose
# outer face is at x = -16.5) where nothing stands in front of them.
OUTLINES = [
    ("the lower shaft's lower outline", (-133.0, -0.27), (627.0, 0.33), -130.0),
    ("the lower shaft's upper outline", (-133.1, 12.43), (626.9, 13.03), -130.0),
    ("the upper shaft's lower outline", (-72.8, 279.78), (535.5, 280.26), -70.0),
    ("the upper shaft's upper outline", (-72.8, 292.48), (535.5, 292.96), -70.0),
]
STUB_END = -20.0
STEP = 0.5
# How near a drawn line must pass, view mm: the quick path's lines lie within
# about 0.02 mm of the edges on the sheet (0.2 mm of the view at 1:10).
NEAR = 0.3

STROKE = re.compile(r"drawing-trace: stroke (\w+) view=\d+ seen=(\d) kind=(\w+) source=-?\d+ points=(\S*)")
PROJECT = re.compile(r"drawing-trace: project (\d+) bodies")


def content_id(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()[:8]


def find_design(work):
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None
    archives = [os.path.join(folder, n) for n in sorted(os.listdir(folder)) if n.endswith(".zip")]
    archive = next((a for a in archives if content_id(a) == ARCHIVE), None)
    if archive is None:
        print(f"{SKIP} (no archive {ARCHIVE} in {folder})")
        return None
    unpacked = os.path.join(work, "unpacked")
    os.makedirs(unpacked, exist_ok=True)
    with zipfile.ZipFile(archive) as z:
        z.extractall(unpacked)
    for root, _, names in os.walk(unpacked):
        for n in names:
            path = os.path.join(root, n)
            if n.endswith(".mitcad") and content_id(path) == DESIGN:
                return path
    sys.exit(f"the archive {ARCHIVE} holds no design {DESIGN}")


def segment_distance(p, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    length = dx * dx + dy * dy
    t = 0.0 if length == 0.0 else max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length))
    return math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy)


def sheet_lines(trace):
    """The sheet's lines: (seen, kind, points)."""
    lines = []
    for line in trace.splitlines():
        m = STROKE.match(line.strip())
        if not m or m.group(1) != "sheet":
            continue
        points = [tuple(float(v) for v in p.split(",")) for p in m.group(4).split(";") if p]
        lines.append((m.group(2) == "1", m.group(3), points))
    return lines


def gaps(lines, seen, a, b, x0, x1):
    """The stretches of the line a-b between x0 and x1 that no line (seen or
    hidden) passes within NEAR of, sampled every STEP mm."""
    segments = []
    for s, _, points in lines:
        if s != seen:
            continue
        for p, q in zip(points, points[1:]):
            if max(p[0], q[0]) < x0 - 1.0 or min(p[0], q[0]) > x1 + 1.0:
                continue
            segments.append((p, q))
    out = []
    x = x0
    start = None
    while x <= x1:
        t = (x - a[0]) / (b[0] - a[0])
        p = (x, a[1] + (b[1] - a[1]) * t)
        hit = any(segment_distance(p, s, e) <= NEAR for s, e in segments)
        if not hit and start is None:
            start = x
        if hit and start is not None:
            out.append((start, x))
            start = None
        x += STEP
    if start is not None:
        out.append((start, x1))
    return out


def run_scale(cli, design, work, denominator, hidden):
    with open(design, encoding="utf-8") as f:
        doc = json.load(f)
    drawing = next(d for d in doc["drawings"] if d["name"] == "Drawing1")
    for sheet in drawing["sheets"]:
        for view in sheet["views"]:
            view["scale"] = 1.0 / denominator
            if hidden:
                view.setdefault("style", {})["hidden_lines"] = True
    name = f"scale-{denominator}{'-hidden' if hidden else ''}"
    copy = os.path.join(work, name + ".mitcad")
    with open(copy, "w", encoding="utf-8") as f:
        json.dump(doc, f)
    env = dict(os.environ, MITCAD_DRAWING_TRACE="strokes")
    r = subprocess.run(
        [cli, "export-drawing", copy, os.path.join(work, name + ".svg"), "--drawing", "Drawing1"],
        capture_output=True, text=True, env=env, timeout=TIMEOUT)
    if r.returncode != 0:
        sys.exit(f"export-drawing of {name} failed ({r.returncode}):\n{r.stdout}{r.stderr[-4000:]}")
    return name, r.stderr


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--cli", required=True)
    ap.add_argument("--work", required=True)
    args = ap.parse_args()
    os.makedirs(args.work, exist_ok=True)
    design = find_design(args.work)
    if design is None:
        return 0
    failures = []
    for denominator in (1, 2, 5, 10):
        for hidden in (False, True):
            name, trace = run_scale(args.cli, design, args.work, denominator, hidden)
            projected = [int(n) for n in PROJECT.findall(trace)]
            if projected != [BODIES]:
                failures.append(f"{name}: the view projected {projected} bodies, not {BODIES}")
            lines = sheet_lines(trace)
            if not lines:
                failures.append(f"{name}: no lines traced")
                continue
            for what, a, b, x0 in OUTLINES:
                missing = gaps(lines, True, a, b, x0, STUB_END)
                if missing:
                    text = ", ".join(f"{s:.1f} to {e:.1f}" for s, e in missing)
                    failures.append(f"{name}: {what} is missing left of the upright from x {text}")
                if hidden:
                    behind = gaps(lines, False, a, b, x0, STUB_END)
                    covered = (STUB_END - x0) - sum(e - s for s, e in behind)
                    if covered > 1.0:
                        failures.append(f"{name}: {what} is drawn hidden for {covered:.1f} mm left of the upright")
            seen_hidden = sum(1 for s, _, _ in lines if not s)
            smooth = sum(1 for _, k, _ in lines if k == "Smooth")
            if smooth:
                failures.append(f"{name}: {smooth} tangent edges drawn without Tangent Edges")
            if hidden and seen_hidden == 0:
                failures.append(f"{name}: no hidden lines with Hidden Edges on")
            if not hidden and seen_hidden:
                failures.append(f"{name}: {seen_hidden} hidden lines without Hidden Edges")
            print(f"ok   {name}: {len(lines)} lines on the sheet, {seen_hidden} hidden")
    if failures:
        for f in failures:
            print("FAIL " + f)
        return 1
    print("drawing shaft outlines reported: every outline continuous at 1:1, 1:2, 1:5 and 1:10")
    return 0


if __name__ == "__main__":
    sys.exit(main())
