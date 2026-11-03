#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Checks the 3MF that Send to Slicer wrote of the design's print plates
(mitcad#1050, tools/ui-scripts/print-plates.mitcad-ui): a Bambu-family
project of two plates, Plate 1 with one object and no spiral mode, Plate 2
("Plate 2") with one object and its spiral mode on; no object override and
no project value of the spiral mode.

Usage: print-plates-verify.py <file.3mf>
"""
import json
import re
import sys
import zipfile


def fail(message):
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def main():
    if len(sys.argv) != 2:
        fail("usage: print-plates-verify.py <file.3mf>")
    with zipfile.ZipFile(sys.argv[1]) as package:
        model = package.read("Metadata/model_settings.config").decode("utf-8")
        project = json.loads(package.read("Metadata/project_settings.config").decode("utf-8"))
    plates = re.findall(r"<plate>(.*?)</plate>", model, re.S)
    if len(plates) != 2:
        fail(f"{len(plates)} plates in model_settings.config, not 2")
    if 'key="plater_name" value="Plate 2"' not in plates[1]:
        fail("the second plate is not Plate 2")
    if "spiral_mode" in plates[0]:
        fail("Plate 1 has a spiral mode")
    if '<metadata key="spiral_mode" value="true"/>' not in plates[1]:
        fail("Plate 2 has no spiral mode")
    for index, plate in enumerate(plates):
        if plate.count("<model_instance>") != 1:
            fail(f"plate {index + 1} has {plate.count('<model_instance>')} objects, not 1")
    if "spiral_mode" in model.split("<plate>")[0]:
        fail("an object overrides the spiral mode")
    if "spiral_mode" in project:
        fail("the project sets the spiral mode")
    print("ok   two plates, the spiral mode on Plate 2 only")


if __name__ == "__main__":
    main()
