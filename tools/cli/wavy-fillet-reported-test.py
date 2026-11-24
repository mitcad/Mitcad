# SPDX-License-Identifier: MIT
"""A wave-shaped band's top edge rounded wider than its crests bend
(mitcad#2228), on the reported model and the other program's result of it
(never committed).

Both files are in `wavy-fillet/` under MITCAD_MITCAD_REPORTED, found by
their content ids (the first 8 hex digits of their SHA-256): the STEP model
(a band 24 wide between two vertical walls along two waves, 10 high, its
walls bending at the crests with a radius of 2.77 mm) and the `.f3d` design
whose fillet the other program built on it, its result stored in the file.

- The STEP model, imported with mitcad-cli import: each of its two top
  edges along the waves rounded alone at 2.5, 3, 5, 8.5 (the ticket's
  target) and 8.7 mm (the `.f3d` fillet's radius) gives one valid solid,
  the fillet's status ok; at 2.5 (below the crests' radius, the kernel's own
  rounding) the volume the corner's section (1 - pi / 4) r^2 swept along
  the edge (90.852 long) takes. Above 2.77 mm the contact line on the top
  face folds over itself at the crests: the kernel walks the twisted
  stretch across each crest as a piece of its own and fills it between the
  untwisted ones, cut where their contact lines cross (OCCT port patch
  2228).
- The `.f3d` design, imported with mitcad-cli import-f3d: its fillet (8.7
  mm, the design's parameter d2) built by MitCad at the file's radius.
  Parametric, each final body against the body stored in the file within
  0.1 % of its volume with as many faces. Where the import keeps the stored
  body instead (the item a fallback, "its result is not in the file's
  history": MitCad's rounding built, but not within the import's geometric
  tolerance of the file's), the import's deviation report is held to the
  difference named here, KNOWN_DIFFERENCE: across each crest the file's
  rounding has a three-sided lofted face between the neighbours' sections
  through the corner where the top contact lines cross (meeting the wall
  7.47 mm either side of the crest), tangent to them; the kernel fills the
  twisted stretch between its neighbours cut on the wall at the fold cuts
  (3.26 mm either side), with a filling that is not that loft (mitcad#2316,
  the port patch's doc). So the rounding takes up to 0.2 % less of the
  body's volume, its faces up to 0.1 mm off the file's (and the file's off
  it). A fallback for any other reason (the
  rounding not built) fails. (A fallback item leaves no fillet feature in
  the saved design: the radius is then read from the design's parameter
  in mm.)

Skipped when the folder or a file is not there.

  python3 wavy-fillet-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import math
import os
import subprocess
import sys

FOLDER = "wavy-fillet"
STEP = "190e0972"  # the reported model
DESIGN = "afa0aeba"  # the other program's result of it
SKIP = "wavy fillet reported: skipped"
RADII = (2.5, 3.0, 5.0, 8.5, 8.7)
DESIGN_RADIUS = 8.7  # the .f3d fillet's, its parameter d2
# The import's reason for keeping the stored body when its replay built
# the item but not to the file's result (core/import-replay NOT_IN_HISTORY).
NOT_IN_HISTORY = "its result is not in the file's history"
# The difference named for the .f3d's fillet (see the module's text): the
# body's volume relative to the stored one, and how far the new faces lie
# from the file's either way (mm). Measured (2026-11-22): 0.112 %, 0.082 and
# 0.091 mm.
KNOWN_DIFFERENCE = {"volume": 2.0e-3, "distance": 0.1}
# The top edges along the waves: the walls are the import's faces 0 and 2,
# the top face 5, as the model's shell lists them.
EDGES = ("E{F1:import(0)|F1:import(5)}", "E{F1:import(2)|F1:import(5)}")
BAND = 14400.0  # 24 x 60 x 10
EDGE_LENGTH = 90.85212
VOLUME_TOLERANCE = 1.0e-3  # 0.1 %


def content_id(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()[:8]


def find_files():
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None, None
    found = {}
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if os.path.isfile(path) and name.lower().endswith((".step", ".stp", ".f3d")):
            found.setdefault(content_id(path), path)
    if STEP not in found or DESIGN not in found:
        missing = " and ".join(i for i in (STEP, DESIGN) if i not in found)
        print(f"{SKIP} (no file {missing} in {folder})")
        return None, None
    return found[STEP], found[DESIGN]


def fail(message):
    sys.exit(f"FAIL: wavy fillet reported: {message}")


def cli_run(cli, args, what):
    done = subprocess.run([cli] + args, capture_output=True, text=True)
    if done.returncode != 0:
        fail(f"{what}: mitcad-cli {args[0]} failed ({done.returncode}):\n{done.stdout}{done.stderr}")
    return done


def step_model(cli, work, step):
    design = os.path.join(work, "model.mitcad")
    cli_run(cli, ["import", step, "--save", design], "the STEP import")
    corner = 1.0 - math.pi / 4.0
    for edge in EDGES:
        for radius in RADII:
            steps = [
                {"comment": "The band as imported: its volume and faces, the edge there."},
                {"expect": {"body": "F1.b0", "volume": BAND, "tolerance": 1e-5, "faces": 6, "has_edge": edge}},
                {"cmd": "add_feature",
                 "def": {"type": "fillet", "body": "F1.b0", "edges": [edge], "radius": radius}},
                {"expect": {"feature": "Fillet1", "status": "ok", "bodies": 1}},
                {"expect": {"query": {"query": "bodies", "properties": True},
                            "result": [{"uid": "F1.b0", "kind": "solid"}]}},
            ]
            if radius < 2.77:
                steps.append({"expect": {"body": "F1.b0", "tolerance": 3e-4,
                                         "volume": BAND - corner * radius * radius * EDGE_LENGTH}})
            name = f"edge{EDGES.index(edge)}-r{radius:g}"
            script = os.path.join(work, f"{name}.json")
            with open(script, "w", encoding="utf-8") as f:
                json.dump(steps, f, indent=1)
            cli_run(cli, ["run", script, "--open", design], f"{edge} rounded with {radius:g}")
            print(f"ok   {edge} rounded with {radius:g}: one valid solid")


def radius_of(design_path):
    """The radius of the design's fillet as the import made it (its value,
    or the parameter it names)."""
    with open(design_path, encoding="utf-8") as f:
        design = json.load(f)
    values = {p.get("name"): p.get("value") for p in design.get("parameters", [])}
    for feature in design.get("features", []):
        if feature.get("type") != "fillet":
            continue
        radius = feature.get("radius")
        if radius is None:
            for s in feature.get("sets", []):
                size = s.get("size", {})
                radius = size.get("radius", s.get("radius"))
                if radius is not None:
                    break
        if isinstance(radius, str):
            radius = values.get(radius, radius)
        return radius
    # No fillet feature (a fallback keeps the stored body): the design's
    # one parameter in mm, the radius the file's fillet names.
    lengths = [p.get("value") for p in design.get("parameters", []) if p.get("unit") == "mm"]
    return lengths[0] if len(lengths) == 1 else None


def other_program(cli, work, f3d):
    report = os.path.join(work, "f3d-report.json")
    saved = os.path.join(work, "f3d.mitcad")
    cli_run(cli, ["import-f3d", f3d, "--report", report, "--save", saved], "the .f3d import")
    with open(report, encoding="utf-8") as f:
        designs = json.load(f).get("designs", [])
    if not designs:
        fail("the .f3d import reports no design")
    design = designs[0]
    radius = radius_of(saved)
    if radius is None or not math.isclose(radius, DESIGN_RADIUS, abs_tol=1.0e-9):
        fail(f"the .f3d's fillet radius read as {radius}, not {DESIGN_RADIUS}")
    fillets = [i for i in design.get("items", []) if i.get("type") == "FilletFeature"]
    if not fillets:
        fail("the .f3d design has no fillet item")
    for item in fillets:
        if item.get("outcome") == "parametric":
            continue
        name = item.get("name")
        note = item.get("note_message") or item.get("note") or ""
        deviation = item.get("deviation") or {}
        if item.get("outcome") != "fallback" or not note.startswith(NOT_IN_HISTORY) or not deviation:
            fail(f"the .f3d's {name} (radius {radius}) came in as {item.get('outcome')}: {note}")
        # Built, not within the import's tolerance of the file's: held to
        # the difference named.
        stored = deviation.get("stored_body_volume") or 0.0
        relative = abs(deviation.get("volume_difference", math.inf)) / stored if stored > 0.0 else math.inf
        ours = deviation.get("largest_distance")
        theirs = deviation.get("file_largest_distance")
        beyond = [
            text
            for text, out in (
                (f"volume {relative:.3%} off the stored body's (named: {KNOWN_DIFFERENCE['volume']:.1%})",
                 relative > KNOWN_DIFFERENCE["volume"]),
                (f"its faces up to {ours} mm off the file's (named: {KNOWN_DIFFERENCE['distance']})",
                 ours is None or ours > KNOWN_DIFFERENCE["distance"]),
                (f"the file's faces up to {theirs} mm off its (named: {KNOWN_DIFFERENCE['distance']})",
                 theirs is None or theirs > KNOWN_DIFFERENCE["distance"]),
            )
            if out
        ]
        if beyond:
            fail(f"the .f3d's {name} at radius {radius} differs from the file's result beyond the difference "
                 f"named: " + "; ".join(beyond))
        print(f"ok   the .f3d's {name} at radius {radius}: built, the named difference from the file's result "
              f"(the crests' twisted stretches filled where the file has a loft): volume {relative:.3%} "
              f"off, faces up to {ours} mm off, the file's up to {theirs} mm; the stored body kept")
    differences = []
    compared = 0
    for body in design.get("bodies", []):
        if body.get("mitcad") is None:
            differences.append(f"the file's body {body.get('file')} has no MitCad body")
            continue
        compared += 1
        difference = body.get("volume_difference")
        if difference is None or abs(difference) > VOLUME_TOLERANCE:
            differences.append(f"{body.get('mitcad')}: volume {body.get('mitcad_volume')} against the file's "
                               f"{body.get('file_volume')} ({difference})")
        if body.get("file_faces") != body.get("mitcad_faces"):
            differences.append(f"{body.get('mitcad')}: {body.get('mitcad_faces')} faces against the file's "
                               f"{body.get('file_faces')}")
    if design.get("extra_bodies"):
        differences.append(f"bodies the file does not have: {design.get('extra_bodies')}")
    if compared == 0:
        differences.append("no body compared with the file's")
    if differences:
        fail(f"the .f3d's fillet at radius {radius} differs from the file's result:\n  " + "\n  ".join(differences))
    print(f"ok   the .f3d's fillet at radius {radius}: {compared} bodies as the file's "
          f"(volume within {VOLUME_TOLERANCE:.1%}, the same faces)")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    step, f3d = find_files()
    if step is None:
        return 0
    os.makedirs(args.work, exist_ok=True)
    step_model(args.cli, args.work, step)
    other_program(args.cli, args.work, f3d)
    return 0


if __name__ == "__main__":
    sys.exit(main())
