#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Commit appearance fields with real keys and retain their editor; the new
appearance assigned with the Appearance tool and its picker (mitcad#1059,
mitcad#1066)."""
import sys

native = sys.argv[1] == "native"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 1 48000", "mark", "command solid.appearances",
         "focus-dialog '^Appearances$'", "expect-log 'Appearances dialog opened'",
         "capture-undo-depth baseline"]


def add(*steps):
    lines.extend(steps)


def button(title):
    add(f"{'native-pointer-button' if native else 'button'} '^{title}$'")


def focus(identifier):
    add(f"{'native-pointer-click' if native else 'focus-widget'} {identifier}")


def edit(identifier, text, key="Return"):
    focus(identifier)
    add("key Ctrl+a", f"type '{text}'", f"key {key}", "wait-idle",
        "focus-dialog '^Appearances$'")


def capture(state):
    add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{state}-${{MITCAD_TEST_COLOR_SCHEME}}-content.png'",
        f"screenshot-screen '${{MITCAD_TEST_WORK}}/{state}-${{MITCAD_TEST_COLOR_SCHEME}}-own-windows.png'")


focus("appearanceSearch")
add("key Ctrl+a", "type Chrome", "sleep 500", "wait-idle")
add("native-pointer-click appearanceLibrary/1/0" if native else
    "dialog-click-logged 'Appearances item chrome at'")
add("expect-log 'Appearance selected: chrome'")
focus("appearanceNew")
if not native:
    add("key Space")
add("expect-log 'Created appearance custom1 .* from chrome'")
edit("appearanceName", "Key test appearance")
add("expect-log 'Appearance custom1 name = Key test appearance'")
edit("appearance_roughness", "1", "Enter")
add("expect-log 'Appearance custom1 roughness = 1'")
edit("appearance_base_color", "#d02020")
add("expect-log 'Appearance custom1 base_color = #d02020'",
    "expect-undo-depth ${baseline} 4", "expect-model-bodies 1 48000")
# Refused colour input must not add an undo step or close the editor.
edit("appearance_base_color", "invalid-colour")
add("expect-undo-depth ${baseline} 4")
add("expect-native-text appearance_base_color '^#d02020$'" if native else
    "expect-widget-property appearance_base_color text '#d02020'")
capture("edited")
add("native-dialog-resize 720 440" if native else "dialog-resize 720 440")
capture("narrow")
# Explicit pointer Close in AppKit, focused-button Space in Qt.
if native:
    button("Close")
else:
    focus("appearanceClose")
    add("key Space")
add(f"{'expect-native-dialog-gone' if native else 'expect-dialog-gone'} '^Appearances$'", "focus-main")

# Find in Design's Appearance… starts the Appearance tool on the body; its
# picker chooses the new appearance by its search; OK assigns it.
add("command view.find_in_design", "focus-dialog '^Find in Design$'", "type Body?", "wait-idle")
button("Select All")
button("Select")
add("expect-log 'Find in Design: selected 1 body'", "mark")
button("Appearance.*")
add("expect-log '^Appearance tool for F1[.]b0$'", "focus-dialog '^Find in Design$'")
button("Close")
add("focus-main", "mark", "expect-ever '^Panel Appearance input value at '",
    "click-logged '^Panel Appearance input value at '",
    "expect-log '^Appearance picker opened: [0-9]+ entries, current '")
if native:
    add("expect-log '^Native picker libraryPicker at input_value: [0-9]+ rows$'",
        "expect-native-tiles qt-hidden libraryPicker '^Appearance Picker$'")
add("focus-dialog '^Appearance Picker$'", "type 'Key test appearance'",
    "expect-log '^Appearance picker search: Key test appearance, first Key test appearance$'")
capture("picker")
add("key Return", "expect-log '^Appearance picker chose: Key test appearance$'", "focus-main",
    "expect-log '^Panel Appearance tile value: Key test appearance [(]This design[)]$'", "mark",
    "native-click commandOk" if native else "key Return",
    "expect-log '^Appearance custom1 for 1 body[(]ies[)]$'",
    "focus-main", "expect-undo-depth ${baseline} 5", "expect-model-bodies 1 48000",
    "command solid.appearances", "focus-dialog '^Appearances$'")
capture("reopened")
# Escape still closes the rejecting editor, without another model command.
add("key Escape",
    f"{'expect-native-dialog-gone' if native else 'expect-dialog-gone'} '^Appearances$'",
    "focus-main", "expect-undo-depth ${baseline} 5", "key Ctrl+s",
    "expect-log '^Saved '", "expect-model-bodies 1 48000",
    "expect-never 'Recompute failed|Native Mac canvas[:]|OCCT view event failed'")
print("\n".join(lines))
