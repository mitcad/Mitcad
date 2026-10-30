# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/OcctViewer.cpp app/framework tools/cli
"""Timeline inspection through Qt input, visible tooltips and the OCCT framebuffer."""
import os
from pathlib import Path
import json
import subprocess
import sys
import tempfile


def main():
    app, cli, output = sys.argv[1:4]
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=True)
    commands = []
    for name, size in (("Inner", 10), ("Inner detail", 6), ("Outer", 40)):
        commands.append({"cmd": "add_feature", "name": name, "def": {
            "type": "box", "plane": "xy", "corner": [-size / 2, -size / 2],
            "length": size, "width": size, "height": size, "symmetric": True,
            "operation": "new_body"}})
    commands.extend([
        {"cmd": "add_feature", "name": "Failed round", "def": {"type": "fillet",
            "body": "F3.b0", "edges": ["E{F3:side0|F3:side1}"], "radius": 100}},
        # Outer's four vertical edges rounded: its picked edges are drawn
        # where they were when it is selected (mitcad#957).
        {"cmd": "add_feature", "name": "Fillet1", "def": {"type": "fillet",
            "body": "F3.b0", "edges": ["E{F3:side0|F3:side1}", "E{F3:side1|F3:side2}",
                                       "E{F3:side2|F3:side3}", "E{F3:side0|F3:side3}"],
            "radius": 2}},
        {"cmd": "add_feature", "name": "Suppressed detail", "def": {
            "type": "box", "plane": "xy", "corner": [0, 0], "length": 8,
            "width": 8, "height": 8, "operation": "new_body"}},
        {"cmd": "suppress_feature", "uid": "F6", "suppressed": True},
        {"cmd": "add_feature", "name": "Later plane", "def": {"type": "construction_plane",
            "definition": {"type": "offset", "plane": "xy", "distance": 50}}},
        {"cmd": "group_features", "features": ["F1", "F2"], "name": "Inner details"},
        {"cmd": "set_marker", "position": 6},
    ])
    model = out / "model.json"
    model.write_text(json.dumps(commands), encoding="utf-8")
    design = out / "inspection.mitcad"
    env = dict(os.environ)
    if len(sys.argv) > 4:
        env["PATH"] = sys.argv[4] + os.pathsep + env.get("PATH", "")
    with (out / "setup.log").open("wb") as log:
        subprocess.run([cli, "run", str(model), "--save", str(design)], env=env,
                       stdout=log, stderr=subprocess.STDOUT, check=True, timeout=90)
    fixture = Path(__file__).with_name("ui-scripts") / "timeline-inspection.mitcad-ui"
    for chrome in ("floating", "docked"):
        shots = out / chrome
        shots.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=chrome + "-", dir=out) as folder:
            work = Path(folder)
            case_env = dict(env)
            for variable, name in (("MITCAD_SETTINGS_DIR", "settings"),
                    ("MITCAD_AUTOSAVE_DIR", "autosave"), ("MITCAD_PROJECTS_DIR", "projects"),
                    ("MITCAD_THUMBNAIL_DIR", "thumbnails"), ("MITCAD_CRASH_DIR", "crashes")):
                case_env[variable] = str(work / name)
            case_env.update(MITCAD_TEST_INPUT=str(fixture), MITCAD_TEST_SHOTS=str(shots),
                MITCAD_RESULT_STORE="off", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1")
            with (out / (chrome + ".log")).open("wb") as log:
                result = subprocess.run([app, "--ui=qt", "--chrome=" + chrome,
                    "--no-recovery", "--no-mcp", "--open", str(design)], env=case_env,
                    stdout=log, stderr=subprocess.STDOUT, timeout=120)
            text = (out / (chrome + ".log")).read_text(encoding="utf-8", errors="replace")
            errors = [line for line in text.splitlines() if line.startswith("TestDriver: FAIL")]
            if result.returncode or "TestDriver: done" not in text or errors:
                raise RuntimeError(chrome + " timeline inspection failed (exit " + str(result.returncode)
                    + (", " + errors[0] if errors else "") + "); see " + str(out / (chrome + ".log")))
    print("PASS: hidden-feature hover, selection, groups, tooltip, card and a fillet's picked edges"
          " in Floating/Docked")


if __name__ == "__main__":
    main()
