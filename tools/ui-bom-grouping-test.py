# SPDX-License-Identifier: MIT
# check-all sources: app/build core/model/src/document/bom.rs core/model-api/src/libraries.rs tools/cli
"""Synthetic BOM fixture, actual Qt controls and exact exported CSV rows."""
import csv
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile


def prepare(cli, directory, environment=None):
    directory.mkdir(parents=True, exist_ok=True)
    # A repeated CTest run must exercise writing, not an old export or
    # an unexpected overwrite question. Remove only this fixture's outputs.
    for name in ("grouped.csv", "separate.csv", "grouped.pdf"):
        (directory / name).unlink(missing_ok=True)
    commands = []
    for name, length in (("First", 10), ("Second", 10), ("Third", 10), ("Longer", 20)):
        commands += [{"cmd": "create_component", "name": name},
                     {"cmd": "add_feature", "def": {
                         "type": "box", "plane": {"type": "origin", "plane": "xy"},
                         "corner": [0, 0], "length": length, "width": 10, "height": 10,
                         "operation": "new_body"}},
                     {"cmd": "activate_component", "component": "C0"}]
    fixture = directory / "bom.json"
    fixture.write_text(json.dumps(commands), encoding="utf-8")
    with (directory / "setup.log").open("wb") as log:
        subprocess.run([cli, "run", str(fixture), "--save", str(directory / "bom.mitcad")],
                       check=True, env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=90)
    (directory / "bom.mitcad-ui").write_text(workflow(directory), encoding="utf-8")
    (directory / "remembered.mitcad-ui").write_text(
        "wait-idle\nexpect-model-bodies 4 5000\ncommand build.parts_list\n"
        "focus-dialog '^Bill of Materials$'\n"
        "expect-widget-property buildPartsGroupIdentical checked true\n"
        "expect-widget-property buildPartsList rowCount 2\n"
        "key Escape\nfocus-main\nquit\n", encoding="utf-8")


def workflow(directory):
    quote = lambda name: shlex.quote((directory / name).as_posix())
    lines = ["wait-idle", "expect-model-bodies 4 5000", "capture-undo-depth beforeBom",
             "command build.parts_list", "focus-dialog '^Bill of Materials$'",
             "expect-widget-property buildPartsGroupIdentical checked false",
             "expect-widget-property buildPartsList rowCount 4",
             "focus-widget buildPartsColumns", "key Ctrl+a", "type Material", "key Tab",
             "expect-log '^Build parts list columns: Material$'", "mark",
             "button '^Group identical parts$'", "expect-widget-property buildPartsList rowCount 2",
             "expect-log '^Build BOM row: 3 x First$'", "expect-log '^Build BOM row: 1 x Longer$'",
             "expect-setting build/groupIdenticalParts true",
             "screenshot-dialog " + quote("bom-grouped.png")]

    def save_csv(name):
        return ["button '^Save as CSV$'", "focus-dialog '^Save List as CSV$'",
                "focus-widget fileNameEdit", "key Ctrl+a", "type " + quote(name), "key Return",
                "expect-dialog-gone '^Save List as CSV$'", "focus-dialog '^Bill of Materials$'",
                "expect-file " + quote(name)]

    lines += save_csv("grouped.csv")
    lines += ["mark", "button '^Group identical parts$'", "expect-widget-property buildPartsList rowCount 4",
              "expect-log '^Build BOM row: 1 x Second$'", "expect-log '^Build BOM row: 1 x Third$'",
              "button '^Copy as CSV$'", "expect-log '^Build list CSV: 1,First,'",
              "screenshot-dialog " + quote("bom-separate.png")]
    lines += save_csv("separate.csv")
    lines += ["button '^Group identical parts$'", "expect-widget-property buildPartsList rowCount 2",
              "button '^Save as PDF$'", "focus-dialog '^Save Bill of Materials as PDF$'",
              "focus-widget fileNameEdit", "key Ctrl+a", "type " + quote("grouped.pdf"), "key Return",
              "expect-dialog-gone '^Save Bill of Materials as PDF$'", "focus-dialog '^Bill of Materials$'",
              "expect-file " + quote("grouped.pdf"), "key Escape", "expect-dialog-gone '^Bill of Materials$'",
              "focus-main", "expect-undo-depth ${beforeBom} 0", "expect-model-bodies 4 5000",
              "command build.parts_list", "focus-dialog '^Bill of Materials$'",
              "expect-widget-property buildPartsGroupIdentical checked true",
              "expect-widget-property buildPartsList rowCount 2", "key Escape", "focus-main", "quit"]
    return "\n".join(lines) + "\n"


def verify(directory):
    for name, expected in (("grouped.csv", [("3", "First"), ("1", "Longer")]),
                           ("separate.csv", [("1", "First"), ("1", "Second"),
                                             ("1", "Third"), ("1", "Longer")])):
        with (directory / name).open(encoding="utf-8", newline="") as stream:
            rows = list(csv.reader(stream))
        if rows[0][:2] != ["Quantity", "Component"] or [(r[0], r[1]) for r in rows[1:]] != expected:
            raise RuntimeError(name + " does not contain exactly the displayed BOM rows: " + repr(rows))
        material = rows[0].index("Material")
        if any(row[material] != "Steel" for row in rows[1:]):
            raise RuntimeError(name + " does not contain the displayed physical materials: " + repr(rows))
    if not (directory / "grouped.pdf").read_bytes().startswith(b"%PDF-"):
        raise RuntimeError("BOM PDF export was not preserved")


def run(app, cli, directory, qt_bin=None):
    directory = Path(directory).resolve()
    environment = dict(os.environ)
    if qt_bin:
        environment["PATH"] = qt_bin + os.pathsep + environment.get("PATH", "")
    prepare(cli, directory, environment)
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
            environment["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
        environment.update(MITCAD_TEST_INPUT=str(directory / "bom.mitcad-ui"), MITCAD_RESULT_STORE="off",
                           MITCAD_START_PAGE="0", MITCAD_LANGUAGE="en", MITCAD_NO_UPDATE_CHECK="1",
                           QT_FORCE_STDERR_LOGGING="1")
        for name, script in (("bom", "bom.mitcad-ui"), ("remembered", "remembered.mitcad-ui")):
            environment["MITCAD_TEST_INPUT"] = str(directory / script)
            log_path = directory / (name + ".log")
            with log_path.open("wb") as log:
                result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp",
                                         "--open", str(directory / "bom.mitcad")], env=environment,
                                        stdout=log, stderr=subprocess.STDOUT, timeout=180)
            text = log_path.read_text(encoding="utf-8", errors="replace")
            if result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines()) or "TestDriver: quit" not in text:
                raise RuntimeError("BOM UI workflow failed: " + str(log_path))
    verify(directory)
    print("PASS: BOM grouping, remembered view, clipboard, displayed CSV/PDF and unchanged model")


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(sys.argv[2], Path(sys.argv[3]).resolve())
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]))
    else:
        run(*sys.argv[1:])
