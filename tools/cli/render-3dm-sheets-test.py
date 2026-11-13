# SPDX-License-Identifier: MIT
"""Plane surfaces in the ground's plane of a render (mitcad#1738): the
.3dm test writer's three plane surfaces drawn facing up at z 0, each kept
in another of the file's forms (`mitcad-cli import-3dm --write-test
--case plane-surfaces`), are imported (every body built and valid), each
face's outward normal is +z (the `faces` query), and rendered from above
on the CPU with a transparent background they are lit: the ground under
the lowest bodies (Cycles' shadow catcher) lies a hair under their plane,
not in it, where it would leave them black. Skipped (after the import)
where no render worker is given or found.

    python3 render-3dm-sheets-test.py --cli <mitcad-cli> [--worker <mitcad-render>]
        --libraries <tools/libraries> --work <folder>
"""

import argparse
import os
import shutil
import subprocess
import sys


def fail(message):
    sys.exit(f"3dm plane surfaces: FAILED: {message}")


def run(command, what):
    result = subprocess.run([str(c) for c in command], capture_output=True, text=True, encoding="utf-8",
                            errors="replace")
    output = result.stdout + result.stderr
    if result.returncode != 0:
        fail(f"{what} failed with exit status {result.returncode}:\n{output}")
    print(output)
    return output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", required=True)
    parser.add_argument("--worker", default="")
    parser.add_argument("--libraries", required=True, help="tools/libraries (its PNG reader)")
    parser.add_argument("--work", required=True)
    args = parser.parse_args()
    sys.path.insert(0, os.path.abspath(args.libraries))
    import library_previews as previews

    work = os.path.abspath(args.work)
    shutil.rmtree(work, ignore_errors=True)
    os.makedirs(work)
    model = os.path.join(work, "sheets.3dm")
    design = os.path.join(work, "sheets.mitcad")
    run([args.cli, "import-3dm", "--write-test", model, "--case", "plane-surfaces"], "the test writer")
    out = run([args.cli, "import-3dm", model, "--save", design, "--check"], "the import")
    for text in ("3 bodies (0 solids, 0 meshes; 3 valid)", "Checks: 3 of 3 pass"):
        if text not in out:
            fail(f"the import: no '{text}' in:\n{out}")

    # Each sheet's one face faces up, as the file means.
    check = os.path.join(work, "normals.json")
    with open(check, "w", encoding="utf-8", newline="\n") as f:
        f.write("[\n" + ",\n".join(
            f'  {{"expect": {{"query": {{"query": "faces", "body": "F1.b{i}", "geometry": true}}, '
            f'"result": [{{"surface": "plane", "normal": [0, 0, 1]}}]}}}}' for i in range(3)) + "\n]\n")
    run([args.cli, "run", check, "--open", design], "the faces' normals")

    worker = os.path.abspath(args.worker) if args.worker else ""
    available, reason = previews.renderer_available(args.cli, worker or None)
    if not worker or not os.path.isfile(worker) or not available:
        print(f"3dm plane surfaces: skipped (no render worker: {reason or worker or 'none given'})")
        return

    picture = os.path.join(work, "top.png")
    run([args.cli, "render", design, "-o", picture, "--view", "top", "--size", "160x100", "--samples", "8",
         "--no-denoise", "--device", "cpu", "--transparent", "--worker", worker], "the render from above")
    with open(picture, "rb") as f:
        width, height, rows = previews.read_png(f.read())
    covered = lit = dark = 0
    for row in rows:
        for x in range(0, len(row), 4):
            r, g, b, a = row[x:x + 4]
            if a < 128:
                continue
            covered += 1
            luma = 0.299 * r + 0.587 * g + 0.114 * b
            if luma >= 128:
                lit += 1
            elif luma < 64:
                dark += 1
    pixels = width * height
    print(f"top.png: {width} x {height}; the sheets cover {covered} pixels, {lit} lit, {dark} dark")
    # Three 20 mm squares 10 mm apart fitted to the view: about a fifth of it.
    if lit < 0.05 * pixels:
        fail(f"the sheets are lit on {lit} of {pixels} pixels ({covered} covered, {dark} dark): "
             "they render dark from above")
    if dark > 0.02 * covered:
        fail(f"{dark} of the {covered} pixels the sheets cover are dark")
    print("3dm plane surfaces render test passed")


if __name__ == "__main__":
    main()
