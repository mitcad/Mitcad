# SPDX-License-Identifier: MIT
"""Split Body with a face of another component's body as the Splitting
Tool, on the reporter's design of GitHub #588 (mitcad#2318, a follow-up of
mitcad#2299; never committed), built by the real kernel.

The design is in an archive in `github-588/` under MITCAD_MITCAD_REPORTED,
found by its content id (the first 8 hex digits of its SHA-256), the design
in it by its own, unpacked into the work folder. The reporter split a body
of one component (C20, placed in the root as O20) by a face of a body of
another (C21, placed as O21). C21's body F316.b0 has a plug that sits in
C20's top opening: its bottom face, F316:end(r{c23}), is a plane inside
C20's body F293.b0, a little below that body's top. The model test of
mitcad#2299 (`split_body_other_component_reported_2299_tests.rs`) builds
the split with the mock kernel, which cannot rebuild C20; this one builds
it with the real kernel through mitcad-cli.

mitcad-cli run opens the design twice, each time with its JSON report:

- as saved (an empty script);
- with the split as the Split Body panel writes it for that pick: C20
  activated (Bodies to Split activates the component of the bodies), and
  `split_body` of F293.b0 by the face, `tool_links` keyed by the tool's
  body with the occurrence it was picked in and the one the split's
  component is seen at (`{"source": "O21", "target": "O20"}`), saved.

Then:

- the split computes without an error or a warning, in C20, and the saved
  design keeps its definition with the link;
- C20 has one body more: F293.b0 and the split's new body, two solids
  (the kernel accepts only valid pieces) whose volumes add up to the body
  as saved; they meet at one height, the height of the tool face, which is
  the lowest point of the tool's body (its box's bottom);
- every body of the other components, C21's among them, is as saved (its
  volume, area, box, faces and edges), and every feature that computed as
  saved computes the same.

Skipped when the folder, the archive or the design is not there.

  python3 split-body-other-component-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import zipfile

ARCHIVE = "28f6817c"
DESIGN = "5e056248"
FOLDER = "github-588"
SKIP = "split body other component reported: skipped"

TARGET = "F293.b0"  # the body the reporter split, in C20
OWN = "C20"
TOOL = {"body": "F316.b0", "face": "F316:end(r{c23})"}  # in C21
OTHER = "C21"
LINKS = {"F316.b0": {"source": "O21", "target": "O20"}}

VOLUME = 1e-6  # relative, the pieces against the body
SAME = 1e-9  # relative, a body that must not change
HEIGHT = 0.01  # mm, the import's geometric tolerance (mitcad#224)


def fail(message):
    print(f"FAIL: {message}")
    sys.exit(1)


def check(condition, message):
    if not condition:
        fail(message)


def content_id(data):
    return hashlib.sha256(data).hexdigest()[:8]


def find_design(work):
    """The design unpacked into `work`, or None after the skip line."""
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if not name.lower().endswith(".zip") or not os.path.isfile(path):
            continue
        with open(path, "rb") as f:
            if content_id(f.read()) != ARCHIVE:
                continue
        with zipfile.ZipFile(path) as z:
            for info in z.infolist():
                if not info.filename.lower().endswith(".mitcad"):
                    continue
                data = z.read(info)
                if content_id(data) == DESIGN:
                    os.makedirs(work, exist_ok=True)
                    design = os.path.join(work, "reported.mitcad")
                    with open(design, "wb") as out:
                        out.write(data)
                    return design
        fail(f"the archive {ARCHIVE} holds no design {DESIGN}")
    print(f"{SKIP} (no archive {ARCHIVE} in {folder})")
    return None


def run(cli, work, case, design, steps, save=None):
    """mitcad-cli run of `steps` on the design; its JSON report."""
    script = os.path.join(work, f"{case}.json")
    with open(script, "w", encoding="utf-8") as f:
        json.dump(steps, f, indent=1)
    args = [cli, "run", script, "--open", design, "--json"]
    if save:
        args += ["--save", save]
    done = subprocess.run(args, capture_output=True, text=True, encoding="utf-8",
                          errors="replace")
    if done.returncode != 0:
        fail(f"{case}: mitcad-cli run failed ({done.returncode}):\n{done.stdout}{done.stderr}")
    # The report is the JSON object on stdout (notes and the store's line
    # go to stderr with --json).
    out = done.stdout
    start = out.find("{")
    if start < 0:
        fail(f"{case}: no report on stdout:\n{out}{done.stderr}")
    try:
        report, _ = json.JSONDecoder().raw_decode(out[start:])
    except ValueError as error:
        fail(f"{case}: the report is not JSON ({error}):\n{out}")
    return report


def bodies(report):
    return {b["uid"]: b for b in report["bodies"]}


def features(report):
    return {f["uid"]: f for f in report["timeline"]["features"]}


def close(a, b, relative):
    return abs(a - b) <= relative * max(abs(a), abs(b), 1.0)


def same_body(before, after):
    """Why `after` is not `before` as it was, or None."""
    for key in ("component", "kind", "faces", "edges"):
        if before.get(key) != after.get(key):
            return f"{key} {before.get(key)} -> {after.get(key)}"
    for key in ("volume", "area"):
        if not close(before[key], after[key], SAME):
            return f"{key} {before[key]} -> {after[key]}"
    # (An empty body has no box.)
    if (before.get("bbox") is None) != (after.get("bbox") is None):
        return f"box {before.get('bbox')} -> {after.get('bbox')}"
    for corner in ("min", "max") if before.get("bbox") else ():
        for b, a in zip(before["bbox"][corner], after["bbox"][corner]):
            if abs(a - b) > 1e-6:
                return f"box {corner} {before['bbox'][corner]} -> {after['bbox'][corner]}"
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    options = parser.parse_args()
    work = os.path.abspath(options.work)
    design = find_design(work)
    if design is None:
        return 0

    # As saved.
    before = run(options.cli, work, "as-saved", design, [])
    was = bodies(before)
    check(TARGET in was and was[TARGET].get("component") == OWN,
          f"as saved: {TARGET} is not a body of {OWN}")
    check(TOOL["body"] in was and was[TOOL["body"]].get("component") == OTHER,
          f"as saved: {TOOL['body']} is not a body of {OTHER}")
    check(was[TARGET].get("kind") == "solid", f"as saved: {TARGET} is {was[TARGET].get('kind')}")

    # The split as the panel writes it for a face picked in O21.
    split = {"type": "split_body", "bodies": [TARGET], "tool": TOOL, "tool_links": LINKS}
    saved = os.path.join(work, "split.mitcad")
    steps = [
        {"comment": "mitcad#2318: the body of C20 split by a face of C21's body, picked in O21."},
        {"cmd": "activate_component", "component": OWN},
        {"cmd": "add_feature", "def": split},
    ]
    after = run(options.cli, work, "split", design, steps, save=saved)

    # The split computes, in C20, and is saved with its link.
    added = [f for uid, f in features(after).items() if uid not in features(before)]
    check(len(added) == 1, f"{len(added)} features added, not one")
    feature = added[0]
    uid = feature["uid"]
    check(feature["type"] == "split_body", f"{uid} is a {feature['type']}")
    check(feature["status"] == "ok",
          f"the split {uid} is {feature['status']}: {feature.get('error') or feature.get('notices')}")
    check(feature.get("component") == OWN, f"the split is in {feature.get('component')}, not in {OWN}")
    with open(saved, encoding="utf-8") as f:
        stored = {entry["uid"]: entry for entry in json.load(f)["features"]}
    entry = stored.get(uid)
    check(entry is not None, f"the saved design has no {uid}")
    check(entry.get("component") == OWN, f"saved: {uid} is in {entry.get('component')}")
    check(entry.get("bodies") == [TARGET] and entry.get("tool") == TOOL,
          f"saved: {uid} splits {entry.get('bodies')} by {entry.get('tool')}")
    check(entry.get("tool_links") == LINKS, f"saved: {uid}'s tool_links are {entry.get('tool_links')}")
    print(f"ok   the split {uid} computes in {OWN} with the face of {OTHER}'s body by its link, saved so")

    # C20: two solids where the body was, of its volume.
    now = bodies(after)
    own_before = sorted(u for u, b in was.items() if b.get("component") == OWN)
    own_after = sorted(u for u, b in now.items() if b.get("component") == OWN)
    new = [u for u in own_after if u not in own_before]
    check(set(own_before) <= set(own_after) and len(new) == 1 and new[0].startswith(f"{uid}.b"),
          f"{OWN}'s bodies {own_before} -> {own_after}: not one new body of {uid}")
    pieces = [now[TARGET], now[new[0]]]
    for piece in pieces:
        check(piece.get("kind") == "solid", f"the piece {piece['uid']} is {piece.get('kind')}")
        check(piece["volume"] > 0, f"the piece {piece['uid']} has volume {piece['volume']}")
    total = sum(p["volume"] for p in pieces)
    check(close(total, was[TARGET]["volume"], VOLUME),
          f"the pieces' volumes add up to {total:.6f} mm3, the body's was {was[TARGET]['volume']:.6f}")
    lower, upper = sorted(pieces, key=lambda p: p["bbox"]["min"][2])
    cut = lower["bbox"]["max"][2]
    check(abs(upper["bbox"]["min"][2] - cut) <= HEIGHT,
          f"the pieces do not meet at one height: {lower['uid']} up to {cut:.4f}, "
          f"{upper['uid']} from {upper['bbox']['min'][2]:.4f}")
    face_height = was[TOOL["body"]]["bbox"]["min"][2]
    check(abs(cut - face_height) <= HEIGHT,
          f"the pieces meet at {cut:.4f}, the tool face is at {face_height:.4f}")
    print(f"ok   {TARGET} and {new[0]}: two solids of the body's volume ({total:.3f} mm3), "
          f"cut at the tool face's height {cut:.3f}")

    # The other components as saved.
    for body, old in sorted(was.items()):
        if old.get("component") == OWN:
            continue
        check(body in now, f"{body} of {old.get('component', 'the root')} is gone")
        why = same_body(old, now[body])
        check(why is None, f"{body} of {old.get('component', 'the root')} changed: {why}")
    for f_uid, old in features(before).items():
        status = features(after).get(f_uid, {}).get("status")
        check(status == old["status"], f"{f_uid} {old['name']} was {old['status']}, is {status}")
    print(f"ok   {OTHER}'s bodies and every other component's are as saved, every feature as it computed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
