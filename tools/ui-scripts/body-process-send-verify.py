#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Checks the 3MF that Send to Slicer wrote after Fixture Fine was chosen
in its Process row for two bodies with processes of their own
(mitcad#1799, tools/ui-scripts/body-process-send.mitcad-ui): a
Bambu-family project with the objects Body1 and Body2, its process
Fixture Fine with 0.12 mm layers, and no part with layers of its own
(Fixture Draft's 0.28 mm).

Usage: body-process-send-verify.py <file.3mf>
"""
import json
import re
import sys
import zipfile


def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def number(value):
    # A value as the project writes it: text, a number or a list of one.
    if isinstance(value, list):
        value = value[0] if value else None
    try:
        return float(value)
    except (TypeError, ValueError):
        fail(f"layer_height {value!r} is no number")


def main():
    if len(sys.argv) != 2:
        fail("usage: body-process-send-verify.py <file.3mf>")
    with zipfile.ZipFile(sys.argv[1]) as package:
        model = package.read("Metadata/model_settings.config").decode("utf-8")
        project = json.loads(package.read("Metadata/project_settings.config").decode("utf-8"))
    for body in ("Body1", "Body2"):
        if f'<metadata key="name" value="{body}"/>' not in model:
            fail(f"no object {body} in model_settings.config")
    process = project.get("print_settings_id")
    if process != "Fixture Fine":
        fail(f"the project's process is {process!r}, not Fixture Fine")
    if abs(number(project.get("layer_height")) - 0.12) > 1e-9:
        fail(f"the project's layer height is {project.get('layer_height')!r}, not 0.12")
    for value in re.findall(r'key="layer_height" value="([^"]*)"', model):
        if abs(number(value) - 0.12) > 1e-9:
            fail(f"a part has layers of its own: {value}")
    print("ok   Fixture Fine for Body1 and Body2, 0.12 mm layers")


if __name__ == "__main__":
    main()
