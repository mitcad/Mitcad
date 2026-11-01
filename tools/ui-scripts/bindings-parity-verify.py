#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Validate retained binding UI results without opening a desktop window."""
import argparse
import json
from pathlib import Path
import re


def verify_theme(model, work, frontend, scheme):
    design = json.loads(model.read_text())
    bodies = design["bodies"]
    assert len(bodies) == 3, bodies
    assert bodies[0]["appearance"] == "paint_red", bodies[0]
    assert bodies[0]["appearance_expr"] == "switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_red')", bodies[0]
    assert bodies[1]["material"] == "pla", bodies[1]
    # The Material tool's tile, Default (Steel), for any other value.
    assert bodies[1]["material_expr"] == "switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')", bodies[1]
    assert bodies[2]["appearance_expr"] == "switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_purple'; 'paint_black')", bodies[2]
    parameters = {p["name"]: p for p in design["parameters"]}
    assert parameters["Color"]["expression"] == "'Red'", parameters
    for state in ["tool-appearance", "appearance-bound", "material-bound", "appearance-none-red",
                  "three-bodies", "appearance-picked", "bindings-warning", "bindings-removed", "bindings-restored"]:
        for suffix in ["", "-own-windows"]:
            image = work / f"{state}-{frontend}-{scheme}{suffix}.png"
            assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
    image = work / f"material-picker-{frontend}-{scheme}.png"
    assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
    print("Verified: the tools' saved appearance/material bindings and shown option value, "
          "fx off/three bodies/remove undo restoration, retained captures")


def verify_open_values(model, log, work, frontend):
    design = json.loads(model.read_text())
    assert len(design["bodies"]) == 1, design["bodies"]
    body = design["bodies"][0]
    assert not body.get("appearance_expr"), body
    parameters = {p["name"]: p for p in design["parameters"]}
    assert parameters["Length"]["expression"].startswith("20"), parameters
    assert design["configurations"]["options"] == ["Length"], design["configurations"]
    selected = re.findall(r"actual face selected after viewport click: face (.+) of F1[.]b0", log.read_text())
    assert len(selected) == 1, selected
    face = selected[0].strip()
    # The one value, 20 mm (the value now): Paint - Black.
    expression = body["face_appearance_exprs"][face]
    assert expression.startswith("switch(Length; 20") and "'paint_black'" in expression, (face, body)
    assert body["face_appearances"] == {face: "paint_black"}, (face, body)
    for state in ["tool-no-options", "add-option", "tool-face-bound"]:
        for suffix in ["", "-own-windows"]:
            image = work / f"{state}-{frontend}-light{suffix}.png"
            assert image.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"), image
    print("Verified: no options offered, the real Add Option output and an independently selected exact face "
          "bound by the Appearance tool")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    theme = commands.add_parser("theme", help="Check a completed tools/Bindings theme")
    theme.add_argument("model", type=Path)
    theme.add_argument("work", type=Path)
    theme.add_argument("frontend", choices=("qt", "native"))
    theme.add_argument("scheme", choices=("light", "dark"))
    opened = commands.add_parser("open-values", help="Check completed Add Option/face results")
    opened.add_argument("model", type=Path)
    opened.add_argument("log", type=Path)
    opened.add_argument("work", type=Path)
    opened.add_argument("frontend", choices=("qt", "native"))
    args = parser.parse_args()
    if args.command == "theme":
        verify_theme(args.model, args.work, args.frontend, args.scheme)
    else:
        verify_open_values(args.model, args.log, args.work, args.frontend)


if __name__ == "__main__":
    main()
