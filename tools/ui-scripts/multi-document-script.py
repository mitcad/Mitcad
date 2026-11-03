#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Shared synthetic multi-window workflows; the platform harness owns launch.

first/second.mitcad are 6000/120 mm3 boxes. Recovery uses the exact same
paths and real autosave records from the earlier kill, never copied models.
"""
import json
from pathlib import Path
import shlex
import sys


def scenario(work, mode="basic", frontend="qt"):
    work = Path(work)
    path = lambda name: shlex.quote(str(work / name))
    parameter = lambda name, value: "model-command " + shlex.quote(json.dumps({
        "cmd": "add_parameter", "name": name, "unit": "mm", "expression": str(value) + " mm"}))
    question = lambda name: "focus-dialog '^Save changes to " + name + "[.]mitcad[?]$'"
    discard = 'button "^Don\'t Save$"'
    start = ["wait-idle", "capture-document-window first", "expect-document-windows 1", "expect-model-bodies 1 6000"]
    if mode == "current-window":
        steps = start + ["focus-main", "command tools.preferences.general", "focus-dialog '^Settings$'"]
        if frontend == "native":
            # The real AppKit checkbox's exposed bounds and pointer route;
            # the setting assertion below reads the ordinary UI commit.
            steps += ["expect-native-visible openInCurrentWindow true",
                      "expect-native-bounds openInCurrentWindow 100",
                      "native-pointer-click openInCurrentWindow"]
        elif frontend == "qt":
            steps += ["expect-widget-property openInCurrentWindow checked false",
                      "focus-widget openInCurrentWindow", "key Space",
                      "expect-widget-property openInCurrentWindow checked true"]
        else:
            raise ValueError("frontend must be qt or native")
        steps += ["expect-setting documents/openInCurrentWindow true",
            "screenshot-dialog " + path("documents-current-window-setting.png"), "key ctrl+w",
            "expect-native-dialog-gone '^Settings$'" if frontend == "native" else "expect-dialog-gone '^Settings$'",
            "focus-main", "expect-model-bodies 1 6000", "open-file " + path("second.mitcad"), "wait-idle",
            "expect-document-windows 1", "focus-document-window first", "expect-model-bodies 1 120",
            "key ctrl+n", "wait-idle", "expect-document-windows 1", "focus-document-window first",
            "expect-model-bodies 0 0", "quit"]
    elif mode == "basic":
        steps = start + [
            "capture-undo-depth firstUndo", "capture-model-number totalCache '{\"query\":\"cache\"}' memory.budget",
            parameter("firstOnly", 1), "expect-model-parameter firstOnly 1 user", "expect-undo-depth ${firstUndo} 1",
            # Force separate windows before testing the real Merge All action,
            # independently of the system preference for automatic tabbing.
            "command window.new", "wait-idle", "expect-model-bodies 0 0", "expect-document-tabs 1",
            "open-file " + path("second.mitcad"), "wait-idle", "capture-document-window second",
            "expect-document-windows 2", "expect-model-bodies 1 120", "expect-viewport", "capture-undo-depth secondUndo",
            "expect-model-parameter firstOnly absent", parameter("secondOnly", 2),
            "expect-undo-depth ${secondUndo} 1", "expect-document-cache-sharing ${totalCache}",
            "focus-document-window first", "expect-model-bodies 1 6000", "expect-model-parameter secondOnly absent",
            "command edit.undo", "expect-model-parameter firstOnly absent", "expect-undo-depth ${firstUndo} 0",
            "focus-document-window second", "expect-model-parameter secondOnly 2 user", "expect-undo-depth ${secondUndo} 1",
            "focus-document-window first", "command edit.redo", "expect-model-parameter firstOnly 1 user",
            "expect-undo-depth ${firstUndo} 1",
            # Opening an already open path activates its retained owner.
            "focus-document-window second", "open-file " + path("first.mitcad"), "wait-idle",
            "expect-document-windows 2", "expect-model-bodies 1 6000", "expect-model-parameter firstOnly 1 user",
            "focus-document-window first", "expect-undo-depth ${firstUndo} 1",
            "expect-document-tabs 1",
            # Exercise the actual OS command, not a tab-count setter.
            "system-window-menu 'Merge All Windows'", "expect-document-tabs 2",
            # Merge All may choose another tab. Explicitly activate the
            # retained first owner and prove AppKit selected that same tab.
            "focus-document-window first", "expect-document-tab-selected first", "expect-document-tabs 2",
            "expect-model-bodies 1 6000", "expect-model-parameter firstOnly 1 user",
            "screenshot " + path("documents-merged.png"),
            "command file.close", question("first"), "screenshot-dialog " + path("documents-close-question.png"),
            "button '^Cancel$'", "focus-main", "expect-document-windows 2",
            "focus-document-window first", "expect-model-parameter firstOnly 1 user", "expect-undo-depth ${firstUndo} 1",
            "command file.close", question("first"), discard, "focus-main", "expect-document-window-gone first",
            "expect-document-windows 1", "focus-document-window second", "expect-model-bodies 1 120",
            "expect-model-parameter secondOnly 2 user", "expect-undo-depth ${secondUndo} 1",
            "expect-document-cache-sharing ${totalCache}", "command edit.undo",
            "expect-model-parameter secondOnly absent", "expect-undo-depth ${secondUndo} 0",
            "open-file " + path("first.mitcad"), "wait-idle", "expect-document-windows 2",
            "expect-model-bodies 1 6000", "expect-model-parameter firstOnly absent",
            "key ctrl+n", "wait-idle", "capture-document-window empty", "expect-model-bodies 0 0",
            "expect-document-windows 3", "key ctrl+n", "wait-idle", "expect-document-windows 3",
            "focus-document-window empty", "expect-model-bodies 0 0", "command file.close",
            "expect-document-window-gone empty", "expect-document-windows 2",
            "command window.new", "wait-idle", "capture-document-window separate", "expect-document-windows 3",
            "expect-model-bodies 0 0", "expect-document-tabs 1", "expect-viewport", "screenshot " + path("documents-separate.png"),
            "command file.close", "expect-document-window-gone separate", "expect-document-windows 2",
            "focus-document-window second", "expect-model-bodies 1 120", "expect-model-parameter secondOnly absent", "quit",
        ]
    elif mode == "quit":
        steps = start + [parameter("firstQuit", 3), "open-file " + path("second.mitcad"), "wait-idle",
            "capture-document-window second", "expect-model-bodies 1 120", parameter("secondQuit", 4),
            "expect-document-windows 2", "command file.exit", question("first"), "button '^Cancel$'", "focus-main",
            "expect-document-windows 2", "focus-document-window first", "expect-model-parameter firstQuit 3 user",
            "focus-document-window second", "expect-model-parameter secondQuit 4 user",
            "command file.exit", question("first"), discard, question("second"),
            "screenshot-dialog " + path("documents-quit-second-question.png"), discard, "stay"]
        # No TestDriver quit: the real final close must empty the registry.
    elif mode == "crash":
        steps = start + [parameter("firstCrash", 11), "open-file " + path("second.mitcad"), "wait-idle",
            "capture-document-window second", "expect-model-bodies 1 120", parameter("secondCrash", 22),
            "expect-document-windows 2", "expect-files '${MITCAD_AUTOSAVE_DIR}' '*.json' 2 20",
            "focus-document-window first", "expect-model-parameter firstCrash 11 user",
            "focus-document-window second", "expect-model-parameter secondCrash 22 user", "kill"]
    elif mode == "recovery":
        steps = ["expect-ever 'Recovery: 2 document\\(s\\) found'", "focus-dialog '^Recover Unsaved Work$'",
            "screenshot-dialog " + path("documents-recovery-question.png"), "key Return", "focus-main",
            "expect-document-windows 2", "focus-document-path " + path("first.mitcad"),
            "capture-document-window restoredFirst", "expect-model-bodies 1 6000",
            "expect-model-parameter firstCrash 11 user", "expect-model-parameter secondCrash absent", "expect-undo-depth 0 0",
            "focus-document-path " + path("second.mitcad"), "capture-document-window restoredSecond",
            "expect-model-bodies 1 120", "expect-model-parameter secondCrash 22 user",
            "expect-model-parameter firstCrash absent", "expect-undo-depth 0 0",
            "expect-files '${MITCAD_AUTOSAVE_DIR}' '*.json' 2 20"]
        for session in json.loads((work / "crashed-sessions.json").read_text()).values():
            steps.append("expect-files '${MITCAD_AUTOSAVE_DIR}' " + shlex.quote(session + ".*") + " 0 20")
        steps += ["screenshot " + path("documents-recovered.png"), "command file.save", "wait-idle",
            "focus-document-window restoredFirst", "command file.save", "wait-idle",
            "expect-files '${MITCAD_AUTOSAVE_DIR}' '*.json' 0 20", "expect-document-windows 2", "quit"]
    else:
        raise ValueError("mode must be current-window, basic, quit, crash or recovery")
    return "\n".join(steps + [""])


if __name__ == "__main__":
    print(scenario(*sys.argv[1:4]), end="")
