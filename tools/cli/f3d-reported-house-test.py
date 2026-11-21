# SPDX-License-Identifier: MIT
"""The .f3d import of a large reported design (mitcad#1802; never committed).

The design is an archive in `github-329/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the
`.f3d` in it by its own, unpacked into the work folder. Beside the archive
the private `expect-1802.json` holds what the file itself says, read from
its bytes apart from the import's decoder: per body record (by its blob and
ASM record) the body's name, its body folder, whether it is in the design,
whether the file shows it and whether its display scene draws it; per component the placement of its
occurrence as the file's display scene keeps it (mm).

Imported with mitcad-cli import-f3d (saved, with its JSON report), then:

- every body of the file's stored design, and every body the file shows
  (its display scene draws it), has a body in the import (every
  unsuppressed item with bodies has them), and none of the file's bodies
  out of the design is shown;
- every such body is where the file has it: its centre of mass and its
  bounding box within 0.01 mm of the file's body in their component's
  coordinates, and its component's occurrence placed as the file places it
  (each entry within 0.01 mm): the two give its placement in the design;
- every such body has the file's name, is in the file's body folder (or
  in none), and is shown exactly when the file shows it;
- no item suppressed in the file is a feature that is not suppressed;
- every removal the file does not suppress is a remove feature of the
  timeline, and at most `placeholders_at_most` items (where the
  expectations give it) are placeholders (mitcad#1808);
- every sketch can be hidden from its light bulb: a script hides every
  sketch of the imported design and then none is shown.
- the design, modelled with Y up and in inches, comes in in inches and
  turned to stand on Z up (the report's `upright`, mitcad#1937): the turn
  takes the file's Front view (along -Z, +Y up) onto MitCad's (along +Y,
  +Z up), and the occurrences are compared turned with it.

With --worker the design is imported by the application's import worker
instead (`mitcad --import-worker`, the process the Import command starts,
with its import threads and trace; mitcad#2085, where it stopped in the
application while the CLI imported the design), and the same checks run on
the design and the report it writes.

The failures name the bodies by their ids in the import and in the file
(their names stay in the private files). Skipped when the folder, the
archive or the expectations are not there.

  python3 f3d-reported-house-test.py --cli <mitcad-cli> --work <folder> [--worker <mitcad>]
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import zipfile

ARCHIVE = "d8334ba7"
DESIGN = "43ba08a6"
TOLERANCE = 0.01  # mm


def skip(why):
    print(f"f3d reported house: skipped ({why})")
    sys.exit(0)


def content_id(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()[:8]


def find(folder, suffix, wanted):
    for root, _, files in os.walk(folder):
        for name in sorted(files):
            if name.lower().endswith(suffix):
                path = os.path.join(root, name)
                if content_id(path) == wanted:
                    return path
    return None


def run(args, env=None):
    done = subprocess.run(args, capture_output=True, text=True, env=env)
    return done.returncode, done.stdout + done.stderr


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    parser.add_argument("--worker")
    options = parser.parse_args()
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, "github-329")
    if not reported or not os.path.isdir(folder):
        skip("MITCAD_MITCAD_REPORTED has no github-329 folder")
    archive = find(folder, ".zip", ARCHIVE)
    expect_path = os.path.join(folder, "expect-1802.json")
    if archive is None or not os.path.isfile(expect_path):
        skip("the archive or its expectations are not there")
    with open(expect_path, encoding="utf-8") as f:
        expect = json.load(f)
    work = options.work
    os.makedirs(work, exist_ok=True)
    with zipfile.ZipFile(archive) as z:
        for info in z.infolist():
            if info.filename.lower().endswith(".f3d"):
                target = os.path.join(work, "design.f3d")
                with z.open(info) as src, open(target, "wb") as dst:
                    dst.write(src.read())
    design = os.path.join(work, "design.f3d")
    if not os.path.isfile(design) or content_id(design) != DESIGN:
        print(f"FAIL the archive holds no design {DESIGN}")
        return 1

    saved = os.path.join(work, "imported.mitcad")
    report_path = os.path.join(work, "report.json")
    if options.worker:
        # As the application starts it (app/files/F3dImport.cpp), its
        # trace on; the process's last lines when it stops.
        env = dict(os.environ, MITCAD_IMPORT_TRACE="1", QT_QPA_PLATFORM="offscreen")
        status, out = run([options.worker, "--import-worker", design, "--output", saved,
                           "--result", report_path,
                           "--hand-off-store", os.path.join(work, "results")], env)
        if status != 0 or "import-done" not in out:
            print(f"FAIL the import worker stopped ({status}):\n{out[-4000:]}")
            return 1
    else:
        status, out = run([options.cli, "import-f3d", design, "--save", saved,
                           "--report", report_path, "--threads", "4"])
        if status != 0:
            print(f"FAIL the import failed ({status}):\n{out[-4000:]}")
            return 1
    with open(report_path, encoding="utf-8") as f:
        report = json.load(f)
    # (The worker's result is the import command's answer, its report
    # under "report".)
    report = report.get("report", report)["designs"][0]
    with open(saved, encoding="utf-8") as f:
        doc = json.load(f)
    failures = []

    def fail(what, items):
        if items:
            shown = ", ".join(items[:20]) + (f" and {len(items) - 20} more" if len(items) > 20 else "")
            failures.append(f"{what}: {len(items)}: {shown}")

    bodies = {b["uid"]: b for b in doc.get("bodies", [])}
    components = {c["uid"]: c["name"] for c in doc.get("components", [])}
    occurrences = {o["component"]: o for o in doc.get("occurrences", [])}
    owners = {}
    for feature in doc.get("features", []):
        owners[feature["uid"]] = feature.get("component", "C0")

    # The stored design's bodies: each in the import, placed and named as
    # the file has it.
    missing, misplaced, unnamed, unfoldered, visibility, unknown = [], [], [], [], [], []
    stored = report.get("bodies", [])
    for b in stored:
        # `blob#record`, or `blob#record#k` for a lump of a stored body.
        key = "#".join(b.get("file", "").split("#")[:2])
        expected = expect["bodies"].get(key)
        uid = (b.get("mitcad") or "").split(" ")[0]
        if not uid:
            missing.append(key)
            continue
        centre, box = b.get("center_distance"), b.get("box_distance")
        if centre is None or centre > TOLERANCE or box is None or box > TOLERANCE:
            misplaced.append(f"{uid} ({key}: centre {centre}, box {box})")
        if expected is None:
            # No body record names it: no body of the design, never shown.
            if bodies.get(uid, {}).get("visible", True):
                unknown.append(f"{uid} ({key})")
            continue
        body = bodies.get(uid, {})
        if body.get("name") != expected["name"]:
            unnamed.append(f"{uid} ({key})")
        if body.get("folder") != expected["folder"]:
            unfoldered.append(f"{uid} ({key})")
        shown = body.get("visible", True)
        if shown != (expected["in_design"] and expected["shown"]):
            visibility.append(f"{uid} ({key}: shown {shown})")
    # Every body the file shows (its display scene draws it) is a body of
    # the import's stored design: none left out of it.
    compared = {"#".join(b.get("file", "").split("#")[:2]) for b in stored}
    for key, expected in sorted(expect["bodies"].items()):
        if expected.get("drawn") and expected["shown"] and key not in compared:
            missing.append(key)
    fail("bodies of the file's design missing in the import", missing)
    fail("bodies placed otherwise than the file's (component coordinates)", misplaced)
    fail("shown bodies no body record of the file names", unknown)
    fail("bodies without the file's name", unnamed)
    fail("bodies not in the file's body folder", unfoldered)
    fail("bodies shown otherwise than the file shows them", visibility)

    # The design was modelled with Y up: it comes in turned to stand on Z
    # up, so that its front faces MitCad's Front view (looking along +Y
    # with +Z up; the file's Front looks along -Z with +Y up), and in its
    # inch units (mitcad#1937).
    upright = report.get("upright") or {}
    turn = upright.get("matrix")
    if not upright.get("turned") or upright.get("up") != "+Y" or turn is None:
        failures.append(f"the Y-up design was not stood on Z up: {upright}")
        turn = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]

    def turned(v):
        return [sum(turn[i][k] * v[k] for k in range(3)) for i in range(3)]

    if turned([0, 0, -1]) != [0, 1, 0] or turned([0, 1, 0]) != [0, 0, 1]:
        failures.append(f"the file's Front view is not MitCad's after the turn {turn}")
    if doc.get("units", {}).get("length") != "in":
        failures.append(f"the design's length unit is not inches: {doc.get('units')}")

    # The occurrences placed as the file places them, turned with the
    # design: R · T · R⁻¹ (mitcad#1937).
    def turned_placement(t):
        linear = [[sum(turn[i][k] * t[k][m] * turn[j][m] for k in range(3) for m in range(3))
                   for j in range(3)] for i in range(3)]
        move = turned([t[k][3] for k in range(3)])
        return [linear[i] + [move[i]] for i in range(3)]

    placed = []
    for uid, name in components.items():
        wanted = expect["occurrences"].get(name)
        occurrence = occurrences.get(uid)
        if wanted is None or occurrence is None:
            continue
        wanted = turned_placement(wanted)
        have = occurrence.get("transform", [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]])
        worst = max(abs(have[i][j] - wanted[i][j]) * (1.0 if j == 3 else 1000.0)
                    for i in range(3) for j in range(4))
        if worst > TOLERANCE:
            placed.append(f"{uid} ({worst:.4f})")
    fail("occurrences placed otherwise than the file's", placed)

    # Items suppressed in the file come in suppressed.
    features = {f["uid"]: f for f in doc.get("features", [])}
    unsuppressed = []
    for item in report.get("items", []):
        if "uppressed in the file" not in (item.get("note") or ""):
            continue
        # (Its features: the sketches it uses copied into its component and
        # the base feature that brought the bodies before it are no part of
        # it.)
        for uid in item.get("features", []) or []:
            feature = features.get(uid)
            if feature is None or feature.get("type") in ("sketch", "base"):
                continue
            if not feature.get("suppressed", False):
                unsuppressed.append(f"{uid} (item {item.get('index')})")
    fail("items suppressed in the file that came in unsuppressed", unsuppressed)

    # Every removal the file does not suppress is a remove feature of the
    # timeline (mitcad#1808), and no more items than the expectations allow
    # are placeholders (their inputs do not decode; the import's report
    # says why in words).
    unremoved, placeholders = [], []
    for item in report.get("items", []):
        note = item.get("note") or ""
        types = [features.get(uid, {}).get("type") for uid in item.get("features", []) or []]
        if item.get("type") == "RemoveFeature" and "uppressed in the file" not in note \
                and "remove_body" not in types:
            unremoved.append(f"item {item.get('index')}")
        if "its definition did not come in" in note:
            placeholders.append(f"item {item.get('index')}")
    fail("removals not suppressed in the file without a remove feature", unremoved)
    most = expect.get("placeholders_at_most")
    if most is not None and len(placeholders) > most:
        fail(f"placeholders, more than {most}", placeholders)

    # Every sketch hidden from its light bulb: none shown after.
    sketches = [f["uid"] for f in doc.get("features", []) if f.get("type") == "sketch"]
    script = [{"comment": "Every sketch of the imported design hidden by its light bulb (mitcad#1802)."}]
    script += [{"cmd": "set_feature_visible", "uid": uid, "visible": False} for uid in sketches]
    script += [{"expect": {"query": {"query": "feature", "uid": uid}, "result": {"visible": False}}}
               for uid in sketches]
    script_path = os.path.join(work, "hide-sketches.json")
    with open(script_path, "w", encoding="utf-8") as f:
        json.dump(script, f)
    status, out = run([options.cli, "run", script_path, "--open", saved])
    if status != 0:
        failures.append(f"hiding every sketch from its light bulb failed ({status}):\n{out[-3000:]}")

    if failures:
        for line in failures:
            print(f"FAIL {line}")
        return 1
    print(f"ok   {len(stored)} bodies placed, named, in their folders and shown as the file has them; "
          f"{len(sketches)} sketches hidden from their light bulbs; "
          f"every removal a remove feature, {len(placeholders)} placeholders")
    return 0


if __name__ == "__main__":
    sys.exit(main())
