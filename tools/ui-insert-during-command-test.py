# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowArrivals.cpp app/files/MainWindowFiles.cpp app/webbrowser/MainWindowComponentWeb.cpp
# check-all sources: app/framework/CommandSessionVisibility.cpp app/framework/TestDriverFiles.cpp app/commands/InspectCommands.cpp tools/cli
"""Work that arrives while a command is open (mitcad#2281, GitHub
mitcad/Mitcad#574), through the real UI on a design of one block:

1. a Component from Web download (its test command, no web browser) with
   Measure open comes in at once, one undo step, and Measure stays open and
   checks its picks again; Move starts on it once Measure closes;
2. the same download with Extrude open waits: the browser's note says that it
   comes in when Extrude is finished or canceled, nothing is inserted, and
   after Cancel it comes in with Move started;
3. a STEP file dropped on the 3D view with Measure open comes in at once
   (Where to Import asked), Measure staying open;
4. the same drop with Extrude open waits, said, and comes in after Cancel.

Usage: ui-insert-during-command-test.py <app> <cli> <output folder> [<Qt's bin>]
       ui-insert-during-command-test.py prepare <cli> <output folder>
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

# The download: the component's name and its part number PA100.
WEB_FILE = "PA100_pin.step"
DROPPED = "drop_block.step"


def prepare(cli, directory, environment=None):
    """The design (a 20 mm cube), the download (a 10 mm cube) and the file
    to drop (5 x 10 x 10 mm), written by mitcad-cli; the driver's steps."""
    directory.mkdir(parents=True, exist_ok=True)
    box = lambda corner, length: {"cmd": "add_feature", "def": {
        "type": "box", "plane": {"type": "origin", "plane": "xy"}, "corner": corner,
        "length": length, "width": 10 if length < 20 else 20, "height": 10 if length < 20 else 20,
        "operation": "new_body"}}
    with (directory / "setup.log").open("wb") as log:
        for name, commands, save in (
                ("design", [box([0, 0], 20)], directory / "design.mitcad"),
                ("web", [box([0, 0], 10), {"cmd": "export", "path": str(directory / WEB_FILE),
                                           "bodies": ["F1.b0"], "format": "step"}], None),
                ("dropped", [box([0, 0], 5), {"cmd": "export", "path": str(directory / DROPPED),
                                              "bodies": ["F1.b0"], "format": "step"}], None)):
            fixture = directory / (name + ".json")
            fixture.write_text(json.dumps(commands), encoding="utf-8")
            command = [cli, "run", str(fixture)] + (["--save", str(save)] if save else [])
            subprocess.run(command, check=True, env=environment, stdout=log, stderr=subprocess.STDOUT,
                           timeout=120)
    (directory / "insert-during-command.mitcad-ui").write_text(workflow(directory), encoding="utf-8")


def workflow(directory):
    """The in-process test driver's steps (MITCAD_TEST_INPUT)."""
    web = WEB_FILE.replace(".", "\\.")
    dropped = DROPPED.replace(".", "\\.")
    drop_path = (directory / DROPPED).as_posix()
    inserted = ("expect-log '^Component from Web: inserted %s as component .* \\(C[0-9]+, occurrence O[0-9]+, "
                "1 body\\(ies\\)\\), part number PA100$' 60" % web)
    lines = [
        "wait-idle",
        "expect-model-bodies 1 8000",
        "capture-undo-depth before",
        # 1. A download with Measure open: in at once, Measure stays open
        # and checks its picks again; Move once it closes.
        "mark",
        "command inspect.measure",
        "expect-log '^Command Measure started' 20",
        "command test.component_web_file",
        "expect-log '^%s comes in beside Measure, which stays open$' 20" % web,
        inserted,
        "expect-log '^Measure checks its picks again after the insert of %s$' 20" % web,
        "expect-log '^Component from Web: Move on .* waits until Measure closes$' 20",
        "wait-idle",
        "expect-no-log '^Command Measure cancelled'",
        "expect-no-log '^Command Move/Copy started'",
        "expect-model-bodies 2 9000",
        "expect-undo-depth ${before} 1",
        "focus-main",
        "key Escape",
        "expect-log '^Command Measure cancelled' 20",
        "expect-log '^Command Move/Copy started$' 20",
        "focus-main",
        "key Escape",
        "expect-log '^Command Move/Copy cancelled$' 20",
        # 2. The download with Extrude open: it waits, said; nothing comes
        # in until Cancel, then it does, with Move.
        "mark",
        "command solid.extrude",
        "expect-log '^Command Extrude started' 20",
        "command test.component_web_file",
        "expect-log '^Component from Web: %s waits until Extrude is finished or canceled$' 20" % web,
        "expect-log '^Component from Web: the test file.s note: %s comes in when Extrude is finished or "
        "canceled\\.$' 20" % web,
        "wait-idle",
        "expect-no-log '^Component from Web: inserted'",
        "expect-model-bodies 2 9000",
        "expect-undo-depth ${before} 1",
        "focus-main",
        "key Escape",
        "expect-log '^Command Extrude cancelled$' 20",
        "expect-log '^%s comes in now that Extrude closed$' 20" % web,
        inserted,
        "expect-log '^Command Move/Copy started$' 20",
        "expect-model-bodies 3 10000",
        "expect-undo-depth ${before} 2",
        "focus-main",
        "key Escape",
        "expect-log '^Command Move/Copy cancelled$' 20",
        # 3. A STEP file dropped on the view with Measure open: in at once,
        # Measure stays open.
        "mark",
        "command inspect.measure",
        "expect-log '^Command Measure started' 20",
        "drop-files-on-widget modelView '%s'" % drop_path,
        "expect-log '^%s comes in beside Measure, which stays open$' 20" % dropped,
        "focus-dialog '^Where to Import$' 20",
        "button '^New Component$'",
        "focus-main",
        "expect-log '^Imported %s as ' 60" % dropped,
        "expect-log '^Measure checks its picks again after the insert of %s$' 20" % dropped,
        "wait-idle",
        "expect-no-log '^Command Measure cancelled'",
        "expect-model-bodies 4 10500",
        "expect-undo-depth ${before} 3",
        "key Escape",
        "expect-log '^Command Measure cancelled' 20",
        # 4. The same drop with Extrude open: it waits, said in the message
        # band; after Cancel it comes in.
        "mark",
        "command solid.extrude",
        "expect-log '^Command Extrude started' 20",
        "drop-files-on-widget modelView '%s'" % drop_path,
        "expect-log '^Dropped %s waits until Extrude is finished or canceled$' 20" % dropped,
        "wait-idle",
        "expect-no-log '^Imported %s'" % dropped,
        "expect-model-bodies 4 10500",
        "focus-main",
        "key Escape",
        "expect-log '^Command Extrude cancelled$' 20",
        "expect-log '^%s comes in now that Extrude closed$' 20" % dropped,
        "focus-dialog '^Where to Import$' 20",
        "button '^New Component$'",
        "focus-main",
        "expect-log '^Imported %s as ' 60" % dropped,
        "wait-idle",
        "expect-model-bodies 5 11000",
        "expect-undo-depth ${before} 4",
        "expect-no-log '^Component from Web: (Move did not start|could not import)'",
        "quit",
    ]
    return "\n".join(lines) + "\n"


def run(app, cli, directory, qt_bin=None):
    directory = Path(directory).resolve()
    environment = dict(os.environ)
    if qt_bin:
        environment["PATH"] = qt_bin + os.pathsep + environment.get("PATH", "")
    prepare(cli, directory, environment)
    with tempfile.TemporaryDirectory(prefix="settings-", dir=directory) as temporary:
        for variable in ("SETTINGS", "AUTOSAVE", "PROJECTS", "THUMBNAIL", "CRASH"):
            environment["MITCAD_" + variable + "_DIR"] = str(Path(temporary) / variable.lower())
        environment.update(MITCAD_TEST_INPUT=str(directory / "insert-during-command.mitcad-ui"),
                           MITCAD_RESULT_STORE="off", MITCAD_START_PAGE="0", MITCAD_NEW_DESIGN_VIEW="0",
                           MITCAD_LANGUAGE="en", MITCAD_NO_UPDATE_CHECK="1", QT_FORCE_STDERR_LOGGING="1",
                           MITCAD_TEST_STEP_MS="100", MITCAD_TEST_WEB_FILE=str(directory / WEB_FILE),
                           # The download's kept copy while it waits.
                           MITCAD_WEB_DOWNLOADS_DIR=str(directory / "downloads"))
        log_path = directory / "insert-during-command.log"
        with log_path.open("wb") as log:
            result = subprocess.run([app, "--ui=qt", "--no-native-dialogs", "--no-recovery", "--no-mcp",
                                     "--open", str(directory / "design.mitcad")],
                                    env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=300)
        text = log_path.read_text(encoding="utf-8", errors="replace")
        if (result.returncode or any(line.startswith("TestDriver: FAIL") for line in text.splitlines())
                or "TestDriver: quit" not in text):
            raise RuntimeError("insert during a command UI workflow failed: " + str(log_path))
    verify(directory)
    print("PASS: a download and a dropped STEP file in at once beside Measure, waiting while Extrude is open "
          "and in after its Cancel, Move started once the command closed")


def verify(directory):
    """The download's kept copy is gone once it came in."""
    downloads = Path(directory) / "downloads"
    left = [str(path) for path in downloads.rglob("*")] if downloads.exists() else []
    if left:
        raise RuntimeError("the kept copy of the download was left behind: " + ", ".join(left))


if __name__ == "__main__":
    if sys.argv[1] == "prepare":
        prepare(sys.argv[2], Path(sys.argv[3]).resolve())
    elif sys.argv[1] == "verify":
        verify(Path(sys.argv[2]))
    else:
        run(*sys.argv[1:])
