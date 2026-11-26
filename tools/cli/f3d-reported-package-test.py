# SPDX-License-Identifier: MIT
"""The .f3z import of a reported assembly package (mitcad#2449; never
committed).

The package (83 designs: the top design and the documents it places,
several levels deep) and the STEP export of the same assembly are kept
outside the repository, in the reported `.f3d` designs' folder
(MITCAD_F3D_REPORTED, else the `f3d-reported` folder beside
MITCAD_F3D_CORPUS), found anywhere under it by their content ids (the first
8 hex digits of their SHA-256).

The package is imported with mitcad-cli import-f3d (saved, with its JSON
report) and the STEP file with mitcad-cli import (its assembly as
components); `mitcad-cli info --properties` gives each placed body of both
in design coordinates. Then:

- light bulbs: every document's sketches have the file's light bulb (none
  unknown), and every sketch and construction feature copied into the
  components of other documents is shown or hidden as in its document's
  own import (`light_bulbs.copies_differ`): before, the copies showed every
  sketch a document hides;
- placements: each solid of the STEP file has a body of the import of its
  volume (within 1 %) whose centre of mass lies within TOLERANCE of the
  solid's: the occurrences are where the file places them. The solids
  without one are counted (at most MISPLACED_AT_MOST, the state the fix
  left), the farthest named with their components and distances;
- fallbacks: the items of all documents that fall back to the file's
  bodies, each document counted once, at most FALLBACKS_AT_MOST;
- the joints the design and its documents kept as as-built joints, and
  the import's time per document, printed for the record.

With --compare-serial the package is imported again with one thread (its
documents one after another; the first import has four, its documents
side by side, mitcad#2449) and the two must give the same design:
components, occurrences and their placements, features with their ids,
names and light bulbs, and every document's outcome; both times printed.

With --diagnose nothing fails: every count is printed, the solids and
fallbacks in full (for the ticket's journal). Skipped when the folder or
the files are not there.

  python3 f3d-reported-package-test.py --cli <mitcad-cli> --work <folder> [--diagnose]
"""

import argparse
import hashlib
import json
import math
import os
import subprocess
import sys
import time

PACKAGE = "07f2d948"  # the .f3z package
STEP = "dd03690d"  # its STEP export
SKIP = "f3d reported package: skipped"
TOLERANCE = 0.1  # mm, a body's centre of mass from the STEP solid's
VOLUME = 0.01  # relative volume difference of a body taken for a solid
# The state the import reached (mitcad#2449); lower them as it improves.
# (25.11.2026: 889 of 3534 solids placed; the solids of the three
# assemblies of the older reference layout and of the printed circuit
# boards' inner levels are not, mitcad#2449's notes.)
MISPLACED_AT_MOST = 2645
FALLBACKS_AT_MOST = 33
# Sketches whose light bulb the decoder does not read (sketch class 12).
UNKNOWN_BULBS_AT_MOST = 0


def content_id(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()[:8]


def folder():
    reported = os.environ.get("MITCAD_F3D_REPORTED", "")
    if reported:
        return reported
    corpus = os.environ.get("MITCAD_F3D_CORPUS", "")
    if not corpus:
        return ""
    return os.path.join(os.path.dirname(os.path.abspath(corpus)), "f3d-reported")


def find(root, suffix, wanted):
    for here, _, files in os.walk(root):
        for name in sorted(files):
            if name.lower().endswith(suffix):
                path = os.path.join(here, name)
                if content_id(path) == wanted:
                    return path
    return None


def run(args):
    done = subprocess.run(args, capture_output=True, text=True, encoding="utf-8", errors="replace")
    return done.returncode, done.stdout or "", done.stderr or ""


def placed_bodies(cli, design):
    """Each placed body of a saved design: volume, centre of mass, names."""
    status, out, err = run([cli, "info", design, "--properties", "--json"])
    if status != 0:
        sys.exit(f"FAIL mitcad-cli info {design} failed ({status}):\n{(out + err)[-3000:]}")
    # (The store's line, if any, goes to stderr with --json.)
    data = json.loads(out)
    # Kept beside the design, to look into a failure.
    with open(design + ".bodies.json", "w", encoding="utf-8") as f:
        json.dump(data["properties"]["bodies"], f)
    return data["properties"]["bodies"]


def references(entries):
    """Every document the design refers to, each once (by its URN), with
    its report entry."""
    seen = {}
    todo = list(entries)
    while todo:
        entry = todo.pop(0)
        seen.setdefault(entry.get("document"), entry)
        todo.extend(entry.get("references", []))
    return seen


def compare_designs(path, reference_path):
    """What differs between two saved designs: components, occurrences
    (with their placements to 1e-6) and features (uid, name, type,
    component, suppression)."""
    with open(path, encoding="utf-8") as f:
        ours = json.load(f)
    with open(reference_path, encoding="utf-8") as f:
        theirs = json.load(f)
    out = []

    def rows(doc, key, fields):
        return [tuple(json.dumps(item.get(k), sort_keys=True) for k in fields)
                for item in doc.get(key, [])]

    for key, fields in (("components", ("uid", "name")),
                        ("features", ("uid", "name", "type", "component", "suppressed",
                                      "visible"))):
        a, b = rows(ours, key, fields), rows(theirs, key, fields)
        if a != b:
            only = [r for r in a if r not in b][:5]
            missing = [r for r in b if r not in a][:5]
            out.append(f"{key}: {len(a)} against {len(b)}; only here {only}; only serial {missing}")
    occurrences = {o["uid"]: o for o in theirs.get("occurrences", [])}
    for o in ours.get("occurrences", []):
        r = occurrences.pop(o["uid"], None)
        if r is None or (o["component"], o["parent"]) != (r["component"], r["parent"]):
            out.append(f"occurrence {o['uid']} differs: {o} / {r}")
            continue
        t = o.get("transform", [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]])
        u = r.get("transform", [[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0]])
        if max(abs(t[i][j] - u[i][j]) for i in range(3) for j in range(4)) > 1e-6:
            out.append(f"occurrence {o['uid']} placed otherwise: {t} / {u}")
    out += [f"occurrence {uid} only in the serial import" for uid in occurrences]
    return out


def compare_documents(ours, theirs):
    """What differs between two imports' documents (by URN): status, items
    by outcome, fallbacks, light bulbs, joints."""
    out = []

    def fallen(d):
        # The items only: why an item fell back may differ where its time
        # ran out (the item's time limit on a loaded machine), in either
        # import.
        return [f.split(":")[0] for f in d.get("fallbacks") or []]

    for urn in sorted(set(ours) | set(theirs)):
        a, b = ours.get(urn) or {}, theirs.get(urn) or {}
        for key in ("status", "items", "light_bulbs", "joints"):
            if a.get(key) != b.get(key):
                out.append(f"{a.get('name') or b.get('name')}: {key} {a.get(key)} / {b.get(key)}")
        if fallen(a) != fallen(b):
            out.append(f"{a.get('name') or b.get('name')}: fallbacks {fallen(a)} / {fallen(b)}")
    return out


def turned(turn, v):
    return [sum(turn[i][k] * v[k] for k in range(3)) for i in range(3)]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    parser.add_argument("--diagnose", action="store_true")
    parser.add_argument("--compare-serial", action="store_true")
    options = parser.parse_args()
    root = folder()
    if not root or not os.path.isdir(root):
        print(f"{SKIP} (MITCAD_F3D_REPORTED is not set and no f3d-reported folder is beside "
              f"MITCAD_F3D_CORPUS)")
        return 0
    package = find(root, ".f3z", PACKAGE)
    step = find(root, ".step", STEP) or find(root, ".stp", STEP)
    if package is None or step is None:
        print(f"{SKIP} (no package {PACKAGE} or STEP file {STEP} under {root})")
        return 0
    work = os.path.abspath(options.work)
    os.makedirs(work, exist_ok=True)
    failures = []

    def fail(what, items=None):
        if items is None:
            failures.append(what)
        elif items:
            shown = items if options.diagnose else items[:20]
            more = len(items) - len(shown)
            failures.append(f"{what}: {len(items)}:\n    " + "\n    ".join(shown)
                            + (f"\n    and {more} more" if more else ""))

    # The import, saved with its report.
    saved = os.path.join(work, "imported.mitcad")
    report_path = os.path.join(work, "report.json")
    clock = time.monotonic()
    status, out, err = run([options.cli, "import-f3d", package, "--save", saved,
                            "--report", report_path, "--threads", "4", "--hang-limit", "600"])
    seconds = time.monotonic() - clock
    # Its text report and, with MITCAD_IMPORT_TRACE, its trace, kept.
    with open(os.path.join(work, "import-output.txt"), "w", encoding="utf-8") as f:
        f.write(out)
        f.write(err)
    if status != 0:
        print(f"FAIL the import failed ({status}):\n{(out + err)[-4000:]}")
        return 1
    with open(report_path, encoding="utf-8") as f:
        report = json.load(f)["designs"][0]
    counts = {}
    for item in report.get("items", []):
        counts[item.get("outcome")] = counts.get(item.get("outcome"), 0) + 1
    print(f"note the import took {seconds:.0f} s; the top design's items: {counts}")
    documents = references(report.get("references", []))
    linked = [d for d in documents.values() if d.get("status") == "linked" or d.get("items")]
    print(f"note {len(documents)} documents referred to, {len(linked)} imported")
    timed = sorted(((d.get("seconds") or 0.0, d.get("name") or d.get("document"))
                    for d in documents.values()), reverse=True)
    print("note the documents' own imports, the longest first: "
          + ", ".join(f"{name} {s:.1f} s" for s, name in timed[:12]))
    not_linked = [f"{d.get('name') or d.get('document')}: {d.get('status')} {d.get('note') or ''}"
                  for d in documents.values() if d.get("status") not in ("linked", "found")]
    fail("documents not imported", not_linked)

    # Light bulbs.
    unknown = [f"{d.get('name')}: {d['light_bulbs']['sketch_bulbs_unknown']} of "
               f"{d['light_bulbs']['sketch_bulbs']} sketches"
               for d in documents.values()
               if (d.get("light_bulbs") or {}).get("sketch_bulbs_unknown")]
    unknown_sketches = sum((d.get("light_bulbs") or {}).get("sketch_bulbs_unknown", 0)
                           for d in documents.values())
    print(f"note {unknown_sketches} sketches whose light bulb the import did not read, in "
          f"{len(unknown)} documents")
    if options.diagnose:
        for line in unknown:
            print(f"     {line}")
    if unknown_sketches > UNKNOWN_BULBS_AT_MOST:
        fail(f"sketches whose light bulb the import did not read, more than "
             f"{UNKNOWN_BULBS_AT_MOST}", unknown)
    bulbs = report.get("light_bulbs", {})
    print(f"note light bulbs: {bulbs}")
    if bulbs.get("copies_differ", 0) > 0:
        fail(f"{bulbs['copies_differ']} of {bulbs.get('copied')} sketches and construction "
             f"features copied into components of other documents are shown or hidden "
             f"otherwise than in their documents")
    if not bulbs.get("copied"):
        fail("no sketch or construction feature was copied into a component of another document")

    # Fallbacks, each document once.
    fallbacks = [f"(top design) {i.get('name')}: {i.get('note')}" for i in report.get("items", [])
                 if i.get("outcome") == "fallback"]
    for d in documents.values():
        fallbacks += [f"{d.get('name')}: {f}" for f in d.get("fallbacks", [])]
    print(f"note {len(fallbacks)} items fall back to the file's bodies")
    if options.diagnose:
        for line in fallbacks:
            print(f"     {line}")
    if FALLBACKS_AT_MOST is not None and len(fallbacks) > FALLBACKS_AT_MOST:
        fail(f"fallbacks, more than {FALLBACKS_AT_MOST}", fallbacks)

    # Joints, for the record.
    print(f"note the top design's joints: {report.get('joints', {})}")
    for d in documents.values():
        if d.get("joints"):
            print(f"note {d.get('name')}'s joints: {d['joints']}")
    if options.diagnose:
        for item in report.get("items", []):
            note = item.get("note") or ""
            kind = item.get("type") or ""
            if note and ("Joint" in kind or "Snapshot" in kind or "Ground" in kind):
                print(f"     {item.get('index')} {item.get('name')} ({item.get('type')}, "
                      f"{item.get('outcome')}): {note}")

    # Placements against the STEP file's solids.
    reference = os.path.join(work, "reference.mitcad")
    status, out, err = run([options.cli, "import", step, "--save", reference])
    if status != 0:
        print(f"FAIL the STEP file's import failed ({status}):\n{(out + err)[-4000:]}")
        return 1
    theirs = placed_bodies(options.cli, reference)
    ours = placed_bodies(options.cli, saved)
    # The design is compared in MitCad's coordinates: a Y-up design came
    # in turned to stand on Z up (mitcad#1937), its STEP export is not.
    upright = report.get("upright") or {}
    turn = upright.get("matrix") if upright.get("turned") else None
    if turn is None:
        turn = [[1, 0, 0], [0, 1, 0], [0, 0, 1]]
    print(f"note {len(theirs)} solids in the STEP file, {len(ours)} placed bodies in the import"
          f"{'; the design turned ' + str(turn) if upright.get('turned') else ''}")

    def name(body):
        return f"{body.get('component_name') or '(root)'}/{body.get('name')} {body.get('path') or ''}"

    misplaced, missing, distances = [], [], []
    # Per top-level occurrence of the STEP file: its solids, those not at
    # their place, its component's name.
    by_top = {}
    for solid in theirs:
        top = (solid.get("path") or "(root)").split("/")[0]
        tally = by_top.setdefault(top, [0, 0, solid.get("component_name")])
        tally[0] += 1
        centre = turned(turn, solid["center_of_mass"])
        volume = solid["volume"]
        best = None
        for body in ours:
            if abs(body["volume"] - volume) > VOLUME * max(abs(volume), 1e-9):
                continue
            d = math.dist(centre, body["center_of_mass"])
            if best is None or d < best[0]:
                best = (d, body)
        if best is None:
            missing.append(f"{name(solid)} ({volume:.1f} mm3)")
            tally[1] += 1
            continue
        distances.append(best[0])
        if best[0] > TOLERANCE:
            misplaced.append((best[0], f"{name(solid)} {best[0]:.3f} mm from {name(best[1])}"))
            tally[1] += 1
    misplaced.sort(reverse=True)
    placed = sum(1 for d in distances if d <= TOLERANCE)
    print(f"note {placed} of {len(theirs)} STEP solids have a body of the import within "
          f"{TOLERANCE} mm; {len(misplaced)} farther, {len(missing)} without a body of their "
          f"volume")
    for top, (n, off, component) in sorted(by_top.items(), key=lambda x: -x[1][1]):
        if off:
            print(f"note   {off} of {n} solids not at their place under {top} (e.g. {component})")
    if options.diagnose or MISPLACED_AT_MOST is None:
        for _, line in misplaced:
            print(f"     misplaced: {line}")
        for line in missing:
            print(f"     no body of its volume: {line}")
    if MISPLACED_AT_MOST is not None and len(misplaced) + len(missing) > MISPLACED_AT_MOST:
        fail(f"STEP solids without a body of the import at their place, more than "
             f"{MISPLACED_AT_MOST}", [line for _, line in misplaced] + missing)

    # The serial import as the reference (mitcad#2449): with one thread the
    # documents are imported one after another; with four, side by side.
    # The design, its components, occurrences, features and their ids and
    # names, and every document's outcome must be the same.
    if options.compare_serial:
        serial_work = os.path.join(work, "serial")
        os.makedirs(serial_work, exist_ok=True)
        serial_saved = os.path.join(serial_work, "imported.mitcad")
        serial_report_path = os.path.join(serial_work, "report.json")
        clock = time.monotonic()
        status, out, err = run([options.cli, "import-f3d", package, "--save", serial_saved,
                                "--report", serial_report_path, "--threads", "1",
                                "--hang-limit", "600"])
        serial_seconds = time.monotonic() - clock
        if status != 0:
            fail(f"the serial import failed ({status}): {(out + err)[-2000:]}")
        else:
            print(f"note the serial import took {serial_seconds:.0f} s, the one with four "
                  f"threads {seconds:.0f} s")
            differences = compare_designs(saved, serial_saved)
            with open(serial_report_path, encoding="utf-8") as f:
                serial_report = json.load(f)["designs"][0]
            differences += compare_documents(documents,
                                             references(serial_report.get("references", [])))
            fail("differences from the serial import", differences)

    if failures and not options.diagnose:
        for line in failures:
            print(f"FAIL {line}")
        return 1
    for line in failures:
        print(f"(would fail) {line}")
    print(f"ok   light bulbs of {bulbs.get('copied')} copies as in their documents; "
          f"{placed} of {len(theirs)} STEP solids placed; {len(fallbacks)} fallbacks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
