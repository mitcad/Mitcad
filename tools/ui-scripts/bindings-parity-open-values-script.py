#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The Appearance tool's fx without options, an option added in
Configurations with one value, and an actual face picked in the view bound
to it (mitcad#1059, mitcad#1066)."""
import json
import re
import sys

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 1 48000", "key F6"]


def add(*steps):
    lines.extend(steps)


def log(pattern):
    return "expect-log " + json.dumps("^" + re.escape(pattern) + "$", ensure_ascii=False)


def press(label):
    add(f"native-pointer-button '^{label}$'" if native else f"button '^{label}$'")


def shot(state):
    add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{state}-{frontend}-light.png'",
        f"screenshot-screen '${{MITCAD_TEST_WORK}}/{state}-{frontend}-light-own-windows.png'")


def field(identifier, value):
    add(f"native-pointer-click {identifier}" if native else f"focus-widget {identifier}",
        "key Ctrl+a", f"type '{value}'", "key Tab")


def click_input(command, key):
    add(f"expect-ever '^Panel {command} input {key} at '", f"click-logged '^Panel {command} input {key} at '")


def choose(command, key, text):
    add(f"native-choice input_{key} '{text}'" if native else f"qt-choice input_{key} '{text}'",
        "expect-log " + json.dumps(f"^{command}: .* = {re.escape(text)}$", ensure_ascii=False))


def ok():
    add("native-click commandOk" if native else "key Return")


def cancel():
    add("native-click commandCancel" if native else "key Escape")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'", "native-appearance light",
        "native-context-menu native.model.body:F1.b0@ 'Appearance…'")
else:
    add("context-menu-logged '^Browser Root/Bodies/Body1 at ' 'Appearance…'")
add(log("Appearance tool for F1.b0"), "capture-undo-depth beforeOption", "mark")

# Without options the fx offers None only; its note says how to add one.
click_input("Appearance", "value_fx")
# The options and the note are logged as the panel was filled, before the fx.
add(log("Appearance: Varies with an option = on"),
    "expect-ever '^Panel Appearance options varies: None$'",
    "expect-ever " + json.dumps("^" + re.escape("Panel Appearance varies note: The design has no options yet; "
                                                "Configurations… adds them.") + "$", ensure_ascii=False))
shot("tool-no-options")
cancel()
add("expect-log '^Command Appearance cancelled'", "expect-undo-depth ${beforeOption} 0", "focus-main")

# An option added in Configurations: Length with the value 20.
add("command solid.configurations", "focus-dialog '^Configurations$'")
if native:
    add("native-pointer-click configurationsAddOption")
else:
    add("focus-widget configurationsAddOption", "key Space")
add("focus-dialog '^Add Option$'")
field("addOptionName", "Length")
field("addOptionValue", "20")
shot("add-option")
press("OK")
add("expect-log 'Configurations added option Length'", "expect-undo-depth ${beforeOption} 1",
    "focus-dialog '^Configurations$'")
press("Close")
add("focus-main")

# The viewport helper probes a currently visible OCCT face and clicks it
# through the real input path. It also asserts the exact shared selection;
# the saved model is checked against that logged topological face name.
# The Appearance tool takes the face; its fx varies with Length, the
# option's list of values (a closed list: one row, 20 mm, the value now,
# and no Other row).
add("key Escape", "click-visible-face", "mark", "command solid.appearance",
    "expect-log '^Command Appearance started'", "capture-undo-depth beforeFace", "mark")
click_input("Appearance", "value_fx")
add(log("Appearance: Varies with an option = on"), "mark")
choose("Appearance", "varies", "Length")
add(log("Appearance tool varies with Length: 1 rows"),
    "expect-log '^Panel Appearance label row0: 20.* \\(now\\)$'",
    "expect-undo-depth ${beforeFace} 0", "mark")
choose("Appearance", "row0", "Paint - Black")
shot("tool-face-bound")
add("mark")
ok()
add("expect-log '^Appearance switch\\(Length; .* for 0 body\\(ies\\), 1 face\\(s\\)$'",
    "expect-undo-depth ${beforeFace} 1", "focus-main", "wait-idle")
add("key Ctrl+s", "expect-log 'Saved .*bindings-open[.]mitcad'",
    "expect-model-bodies 1 48000", "expect-never 'Native Mac canvas[:]|OCCT view event failed|Recompute failed'")
print("\n".join(lines))
