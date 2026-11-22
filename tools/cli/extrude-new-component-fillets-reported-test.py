# SPDX-License-Identifier: MIT
"""An extrude edited from Join to New Component takes its fillets along
(mitcad#2185), on the reported design (never committed).

The design is in `extrude-new-component-fillets/` under
MITCAD_MITCAD_REPORTED, found by its content id (the first 8 hex digits of
its SHA-256), copied into the work folder with the project file beside it.
Saved just before the reported edit: Extrude2 joins the body of a
component, Fillet1 and Fillet2 round its edges. Each case runs mitcad-cli
run on a copy with a script this test writes (the extrude's definition
from the file, its own parameters given by their expressions), checks the
features' statuses with the script's expectations and reads the saved
design:

- New Component: Fillet1 and Fillet2 compute, both in the component the
  extrude makes, on its body;
- undone: everything as it was;
- New Component and back to Join: as it was, and no component made;
- New Body: Fillet1 and Fillet2 compute on the extrude's body, in their
  own component.

Skipped when the folder or the design is not there.

  python3 extrude-new-component-fillets-reported-test.py --cli <mitcad-cli> --work <folder>
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys

DESIGN = "f2b7172a"
FOLDER = "extrude-new-component-fillets"
SKIP = "extrude new component fillets reported: skipped"


def content_id(path):
    with open(path, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()[:8]


def find_design():
    reported = os.environ.get("MITCAD_MITCAD_REPORTED", "")
    folder = os.path.join(reported, FOLDER)
    if not reported or not os.path.isdir(folder):
        print(f"{SKIP} (MITCAD_MITCAD_REPORTED/{FOLDER} is not there)")
        return None, None
    for name in sorted(os.listdir(folder)):
        path = os.path.join(folder, name)
        if name.endswith(".mitcad") and os.path.isfile(path) and content_id(path) == DESIGN:
            return folder, path
    print(f"{SKIP} (no design {DESIGN} in {folder})")
    return None, None


def by_name(design, name):
    for feature in design["features"]:
        if feature.get("name") == name:
            return feature
    sys.exit(f"FAIL: the design has no {name}")


def edited(design, feature, operation):
    """The feature's definition as an edit sends it, with `operation`: the
    file's entry without its timeline fields, its own parameters given by
    their expressions."""
    own = {
        p["name"]: p["expression"]
        for p in design.get("parameters", [])
        if p.get("owner") == feature["uid"] and "expression" in p
    }

    def values(value):
        if isinstance(value, dict):
            return {k: values(v) for k, v in value.items()}
        if isinstance(value, list):
            return [values(v) for v in value]
        if isinstance(value, str) and value in own:
            return own[value]
        return value

    timeline = ("uid", "name", "suppressed", "suppressed_when", "component", "visible",
                "display_size", "import_note", "acknowledged_warning", "split_pieces")
    definition = {k: values(v) for k, v in feature.items() if k not in timeline}
    definition["operation"] = operation
    if operation in ("new_body", "new_component"):
        definition.pop("participants", None)
    return definition


def run(cli, work, case, source, steps):
    folder = os.path.join(work, case)
    os.makedirs(folder, exist_ok=True)
    design = os.path.join(folder, "reported.mitcad")
    shutil.copyfile(source, design)
    script = os.path.join(folder, "script.json")
    with open(script, "w", encoding="utf-8") as f:
        json.dump(steps, f, indent=1)
    saved = os.path.join(folder, "saved.mitcad")
    done = subprocess.run([cli, "run", script, "--open", design, "--save", saved],
                          capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"FAIL: {case}: mitcad-cli run failed ({done.returncode}):\n{done.stdout}{done.stderr}")
    with open(saved, encoding="utf-8") as f:
        return json.load(f)


def computes(*names):
    return [{"expect": {"feature": name, "status": "ok"}} for name in names]


def made_by(design, uid):
    return [c["uid"] for c in design.get("components", []) if c.get("created_by") == uid]


def check(case, condition, message):
    if not condition:
        sys.exit(f"FAIL: {case}: {message}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    folder, source = find_design()
    if source is None:
        return 0
    os.makedirs(args.work, exist_ok=True)
    project = os.path.join(folder, ".mitcad", "project.json")
    if os.path.isfile(project):
        for case in ("new-component", "undo", "back", "new-body"):
            os.makedirs(os.path.join(args.work, case, ".mitcad"), exist_ok=True)
            shutil.copyfile(project, os.path.join(args.work, case, ".mitcad", "project.json"))
    with open(source, encoding="utf-8") as f:
        design = json.load(f)
    extrude = by_name(design, "Extrude2")
    fillets = [by_name(design, "Fillet1"), by_name(design, "Fillet2")]
    uid = extrude["uid"]
    own = extrude.get("component", "C0")
    check("as saved", extrude.get("operation") == "join", "Extrude2 does not join")
    check("as saved", not made_by(design, uid), "Extrude2 makes a component")
    names = [f["name"] for f in fillets]
    edit = {"cmd": "edit_feature", "uid": uid}

    # New Component: the fillets go into the component it makes, on its body.
    steps = computes(*names) + [{**edit, "def": edited(design, extrude, "new_component")}]
    saved = run(args.cli, args.work, "new-component", source, steps + computes("Extrude2", *names))
    made = made_by(saved, uid)
    check("new component", len(made) == 1, f"{len(made)} components made by Extrude2")
    for fillet in fillets:
        entry = by_name(saved, fillet["name"])
        check("new component", entry.get("component") == made[0],
              f"{fillet['name']} is in {entry.get('component')}, not in {made[0]}")
        check("new component", str(entry.get("body", "")).startswith(f"{uid}.b"),
              f"{fillet['name']} works on {entry.get('body')}, not on Extrude2's body")
    print("ok   New Component: Fillet1 and Fillet2 compute in Extrude2's component, on its body")

    # Undone: as it was.
    saved = run(args.cli, args.work, "undo", source, steps + [{"cmd": "undo"}] + computes(*names))
    check("undo", not made_by(saved, uid), "the component Extrude2 made is still there")
    for fillet in fillets:
        entry = by_name(saved, fillet["name"])
        check("undo", entry.get("component", "C0") == fillet.get("component", "C0")
              and entry.get("body") == fillet.get("body"), f"{fillet['name']} did not come back")
    print("ok   undone: as saved")

    # Back to Join: as it was.
    back = steps + [{**edit, "def": edited(design, extrude, "join")}] + computes("Extrude2", *names)
    saved = run(args.cli, args.work, "back", source, back)
    check("back", not made_by(saved, uid), "the component Extrude2 made is still there")
    for fillet in fillets:
        entry = by_name(saved, fillet["name"])
        check("back", entry.get("component", "C0") == fillet.get("component", "C0")
              and entry.get("body") == fillet.get("body"), f"{fillet['name']} did not come back")
    print("ok   New Component and back to Join: as saved")

    # New Body: the fillets on its body, in their own component.
    body = computes(*names) + [{**edit, "def": edited(design, extrude, "new_body")}]
    saved = run(args.cli, args.work, "new-body", source, body + computes("Extrude2", *names))
    check("new body", not made_by(saved, uid), "Extrude2 made a component")
    for fillet in fillets:
        entry = by_name(saved, fillet["name"])
        check("new body", entry.get("component", "C0") == own,
              f"{fillet['name']} moved to {entry.get('component')}")
        check("new body", str(entry.get("body", "")).startswith(f"{uid}.b"),
              f"{fillet['name']} works on {entry.get('body')}, not on Extrude2's body")
    print("ok   New Body: Fillet1 and Fillet2 compute on Extrude2's body")
    return 0


if __name__ == "__main__":
    sys.exit(main())
