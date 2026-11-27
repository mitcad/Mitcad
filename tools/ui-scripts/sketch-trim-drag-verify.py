#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Checks the sketches tools/ui-scripts/sketch-trim-drag.mitcad-ui
captured (mitcad#2524) in its work folder: the sketch intact after the
cancelled drag (five lines), the three vertical lines trimmed back to
their crossings with the horizontal ones after the drag (each in two
pieces, one ending at y = 10 and one at y = -10, and no line left across
y = 0 there), all five lines back after one Undo, and one piece trimmed
by the plain click (six lines).

Usage: sketch-trim-drag-verify.py <work folder>
"""
import json
import os
import sys

VERTICALS = (-16.0, -4.0, 12.0)
TOLERANCE = 1e-6


def lines(path):
    with open(path, encoding="utf-8") as f:
        sketch = json.load(f)
    points = {e["id"]: e["at"] for e in sketch["entities"] if e.get("type") == "point"}
    found = []
    for e in sketch["entities"]:
        if e.get("type") != "line":
            continue
        a = points.get(e["start"])
        b = points.get(e["end"])
        if a is None or b is None:
            raise SystemExit(f"FAIL: {os.path.basename(path)}: line {e['id']} has no end point")
        found.append((e["id"], a, b))
    return found


def near(a, b):
    return abs(a - b) <= TOLERANCE


def check_count(path, expected, what):
    count = len(lines(path))
    if count != expected:
        print(f"FAIL: {what}: {count} lines in {os.path.basename(path)}, expected {expected}")
        return False
    print(f"ok   {what}: {count} lines")
    return True


def check_trimmed(path):
    good = True
    found = lines(path)
    for x in VERTICALS:
        on = [(i, a, b) for i, a, b in found if near(a[0], x) and near(b[0], x)]
        ends = sorted(sorted((a[1], b[1])) for _, a, b in on)
        if len(on) != 2 or not (near(ends[0][0], -20) and near(ends[0][1], -10)
                                and near(ends[1][0], 10) and near(ends[1][1], 20)):
            print(f"FAIL: the vertical line at x = {x:g} is not trimmed back to its crossings: {ends}")
            good = False
    if good:
        print("ok   the three vertical lines trimmed back to their crossings at y = -10 and 10")
    return good


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    work = sys.argv[1]
    results = [
        check_count(os.path.join(work, "trim-drag-cancelled.json"), 5, "the sketch intact after Esc and the release"),
        check_count(os.path.join(work, "trim-drag-trimmed.json"), 8, "three pieces trimmed by the drag"),
        check_trimmed(os.path.join(work, "trim-drag-trimmed.json")),
        check_count(os.path.join(work, "trim-drag-undone.json"), 5, "all three back after one Undo"),
        check_count(os.path.join(work, "trim-click.json"), 6, "one piece trimmed by the click"),
    ]
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
