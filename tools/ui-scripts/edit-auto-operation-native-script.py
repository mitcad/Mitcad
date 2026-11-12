#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Keep the stored-operation assertions, replacing only Qt input gestures."""
from pathlib import Path
import sys

phase = sys.argv[1]
source_names = {
    "automatic": "edit-auto-operation",
    "new-body": "edit-auto-operation-retained",
    "participants": "edit-auto-operation-retained",
    "overrides": "edit-auto-operation-overrides",
    "scope": "edit-auto-operation-scope",
    "early": "edit-auto-operation-early",
}
source = Path(__file__).with_name(source_names[phase] + ".mitcad-ui")
operation = {"new-body": "New Body", "early": "New Body", "scope": "Cut"}.get(phase, "Join")
lines = []
skip_next = None
cancel = 0
initialized = False


def add(*steps):
    lines.extend(steps)


def capture(state):
    add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/edit-{phase}-${{MITCAD_TEST_COLOR_SCHEME}}-{state}.png'")


for line in source.read_text(encoding="utf-8").splitlines():
    if skip_next is not None:
        if line != skip_next:
            raise ValueError(f"changed Qt gesture after native replacement: {line}")
        skip_next = None
        continue
    if line == "wait-idle" and not initialized:
        add(line, "expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
            "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")
        initialized = True
    elif line.startswith("window-resize "):
        add(line.replace("window-resize", "native-resize", 1))
    elif line.startswith("command "):
        add(line.replace("command", "native-action", 1))
    elif line == "dblclick-logged '^Timeline Extrude1 at '":
        add("native-pointer-dblclick native.history.F4")
    elif line == "focus-widget input_distance":
        add("native-pointer-click input_distance")
    elif line.startswith("qt-type-now "):
        # Keep kPlain immediate input while the first preview is busy.
        add(line.replace("qt-type-now", "native-type-now", 1))
    elif line == "click-logged '^Panel Extrude input operation at '":
        add("expect-native-text input_operation '^Join$'",
            "native-pointer-choice input_operation 'New Body' Up Return",
            "expect-native-text input_operation '^New Body$'")
        skip_next = "key Home Return"
    elif line == "button '^(✕|Clear the selection)$' clear_objects":
        add("expect-native-enabled native.action.clear|objects true",
            "native-pointer-click native.action.clear|objects")
    elif line == "focus-widget cut_targets_list":
        # The original keyboard gesture excludes the second body's actual
        # checkbox. Keep the component included and the first body checked.
        add("native-expand cut_targets_list",
            "expect-native-visible cut_body_F1.b0@ true",
            "expect-native-visible cut_body_F2.b0@ true",
            r"expect-native-text cut_targets_status '^1 component selected \(2 bodies\)$'",
            "native-pointer-click cut_body_F2.b0@",
            r"expect-native-text cut_targets_status '^1 component selected \(1 body\)$'")
        skip_next = "key Home Right Down Down Space"
    elif line == "button '^OK$'":
        add(f"expect-native-text input_operation '^{operation}$'")
        capture("before-ok")
        add("native-pointer-click commandOk")
    elif line == "key Escape":
        cancel += 1
        capture(f"before-cancel-{cancel}")
        add("focus-main", line)
    elif line.startswith("screenshot "):
        add(line.replace("edit-keeps-join.png", "edit-keeps-join-native-${MITCAD_TEST_COLOR_SCHEME}.png"))
    elif "Extrude: editing keeps the stored operation" in line and phase != "early":
        add(line, f"expect-native-text input_operation '^{operation}$'")
    elif line == "wait-idle 30" and phase == "early":
        # A kIdle assertion before native-type-now would lose the race.
        add(line, "expect-native-text input_operation '^New Body$'")
    else:
        if line.startswith(("focus-widget ", "click-logged ", "button ", "qt-")):
            raise ValueError(f"unmapped Qt input gesture: {line}")
        add(line)

if skip_next is not None or not initialized:
    raise ValueError("incomplete native stored-operation script")
print("\n".join(lines))
