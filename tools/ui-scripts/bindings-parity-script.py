#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The Material and Appearance tools' fx and pickers (mitcad#1059,
mitcad#1066) and Configurations' Bindings tab in Qt and AppKit: the steps
of tools/ui-bindings-test.sh and tools/ui-material-tool-test.sh through the
macOS frontends."""
import sys
import json
import re

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 3 48000", "key F6"]

SWITCH_1 = "switch(Color; 'Black'; 'paint_black'; 'Red'; 'paint_red')"
MATERIAL_SWITCH = "switch(Color; 'Black'; 'abs'; 'Red'; 'pla'; 'steel')"


def add(*steps):
    lines.extend(steps)


def log(pattern):
    # An exact log line, quoted for the driver.
    return "expect-log " + json.dumps("^" + re.escape(pattern) + "$", ensure_ascii=False)


def press(label):
    add(f"native-pointer-button '^{label}$'" if native else f"button '^{label}$'")


def shot(state):
    add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{state}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'",
        f"screenshot-screen '${{MITCAD_TEST_WORK}}/{state}-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}-own-windows.png'")


def choice(identifier, text, keys="Down Return"):
    add(f"native-pointer-choice {identifier} '{text}' {keys}" if native else
        f"qt-choice {identifier} '{text}'")


def field(identifier, text):
    add(f"native-type {identifier} '{text}'" if native else f"focus-widget {identifier}")
    if not native:
        add("key Ctrl+a", f"type '{text}'" if text else "key Backspace")


def body_menu(number, entry):
    add("focus-main", "mark")
    if native:
        add(f"native-context-menu native.model.body:F{number}.b0@ '{entry}'")
    else:
        add(f"context-menu-logged '^Browser Root/Bodies/Body{number} at ' '{entry}'")
    add("expect-log '^Context menu: '", "expect-no-log 'Follow Option'")


def tab(name):
    add(f"native-choice configurationsTabs {name}" if native else
        f"dialog-click-logged 'Configurations tab {name}'",
        f"expect-log 'Configurations page {name}'")


# The tools' command panels: the AppKit inspector's form or the Qt panel.
def click_input(command, key):
    # The control's place as the panel logs it; the AppKit form's control
    # by its identifier input_<key>.
    add(f"expect-ever '^Panel {command} input {key} at '", f"click-logged '^Panel {command} input {key} at '")


def choose(command, key, text):
    add(f"native-choice input_{key} '{text}'" if native else f"qt-choice input_{key} '{text}'",
        "expect-log " + json.dumps(f"^{command}: .* = {re.escape(text)}$", ensure_ascii=False))


def label(command, key, text):
    add(log(f"Panel {command} label {key}: {text}"))


def ok():
    add("native-click commandOk" if native else "key Return")


def cancel():
    add("native-click commandCancel" if native else "key Escape")


def fx(command, on):
    add("mark")
    click_input(command, "value_fx" if command == "Appearance" else "material_fx")
    add(log(f"{command}: Varies with an option = {'on' if on else 'off'}"))


def picker(command, key, name, text=None):
    # The picker button opens the picker: the AppKit popover under the tile
    # (its Qt presenter never on the screen) or the Qt window. A name typed
    # in its search, Return chooses the first entry that contains it; Escape
    # closes it choosing nothing.
    title = f"{name} Picker"
    picker_log = f"{name} picker"
    add("mark")
    click_input(command, key)
    add(f"expect-log '^{picker_log} opened: [0-9]+ entries, current '")
    if native:
        add(f"expect-log '^Native picker libraryPicker at input_{key}: [0-9]+ rows$'",
            f"expect-native-tiles qt-hidden libraryPicker '^{title}$'")
    add(f"focus-dialog '^{title}$'")
    if text is None:
        add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/{name.lower()}-picker-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'",
            "key Escape", log(f"{picker_log} closed"), "focus-main")
        return
    add(f"type '{text}'", log(f"{picker_log} search: {text}, first {text}"), "key Return",
        log(f"{picker_log} chose: {text}"), "focus-main")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")

# Body1's Appearance… starts the Appearance tool; its fx on, Varies With
# Color: a row per value, nothing written yet; Paint - Black for Black and
# Paint - Red for Red; OK writes switch(Color; ...) in one undo step.
body_menu(1, "Appearance…")
add(log("Appearance tool for F1.b0"), "capture-undo-depth beforeAppearance")
if native:
    # The fx button on the tile's line, as the shared panel's.
    add("expect-native-visible input_value_fx true", "expect-native-visible input_value true")
shot("tool-appearance")
fx("Appearance", True)
# Logged as the panel was filled, before the fx.
add("expect-ever '^Panel Appearance options varies: None \\| Color'", "mark")
choose("Appearance", "varies", "Color")
add(log("Appearance tool varies with Color: 2 rows"))
label("Appearance", "row0", "Black (now)")
label("Appearance", "row1", "Red")
add("expect-no-log 'Appearance switch\\('", "expect-undo-depth ${beforeAppearance} 0", "mark")
choose("Appearance", "row0", "Paint - Black")
choose("Appearance", "row1", "Paint - Red")
add("expect-ever '^Panel Appearance option pictures row0: [1-9][0-9]* of '")
shot("appearance-bound")
add("mark")
ok()
add(log(f"Appearance {SWITCH_1} for 1 body(ies)"), "expect-undo-depth ${beforeAppearance} 1")
add("focus-main", "mark")
choice("configurationChooser_Color", "Red")
add("expect-log '^Configuration chosen: Color = Red'", "wait-idle", "expect-model-bodies 3 48000")

# Body2's Material ▸ More…: the Material tool binds the material to Color
# the same way (ABS Plastic, PLA Plastic, Steel for any other value). Its
# picker opens and Escape closes it, choosing nothing.
body_menu(2, "Material/More…")
add(log("Material tool for bodies F2.b0"), log("Panel Material bodies note: 1 body: Steel"),
    "capture-undo-depth beforeMaterial")
picker("Material", "material", "Material")
add("expect-no-log '^Material picker chose: '")
fx("Material", True)
add("mark")
choose("Material", "varies", "Color")
add(log("Material tool varies with Color: 2 rows"))
label("Material", "row1", "Red (now)")
choose("Material", "row0", "ABS Plastic")
choose("Material", "row1", "PLA Plastic")
shot("material-bound")
add("mark")
ok()
add(log(f"Material tool: varies with Color to 1 bodies: {MATERIAL_SWITCH}"),
    "expect-undo-depth ${beforeMaterial} 1")

# Body1's Appearance… again shows its binding (Red the value now); the fx
# off and OK keep Paint - Red set by hand in one undo step; undo brings the
# binding back.
body_menu(1, "Appearance…\tfx")
add(log(f"Appearance tool: the targets follow {SWITCH_1}"))
label("Appearance", "row1", "Red (now)")
add("expect-log '^Panel Appearance tile value: Paint - Red \\('", "capture-undo-depth beforeNone")
fx("Appearance", False)
shot("appearance-none-red")
add("mark")
ok()
add(log("Appearance paint_red for 1 body(ies)"), "expect-undo-depth ${beforeNone} 1",
    "focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeNone} 0")

# Body1, Body2 and Body3 selected: the tool shows Body1's table; a row
# changed and OK write the table to all three in one undo step, and undo
# restores each.
add("focus-main", "key Escape")
if native:
    add("native-select native.model.body:F1.b0@",
        "native-select native.model.body:F2.b0@ ctrl",
        "native-select native.model.body:F3.b0@ ctrl",
        "expect-selection 3 0")
else:
    add("click-logged 'Browser Root/Bodies/Body1'",
        "click-logged 'Browser Root/Bodies/Body2' left ctrl",
        "click-logged 'Browser Root/Bodies/Body3' left ctrl")
body_menu(1, "Appearance…\tfx")
add(log(f"Appearance tool: the targets follow {SWITCH_1}"), "capture-undo-depth beforeMixed", "mark")
choose("Appearance", "row0", "Paint - White")
shot("three-bodies")
add("mark")
ok()
add(log("Appearance switch(Color; 'Black'; 'paint_white'; 'Red'; 'paint_red') for 3 body(ies)"),
    "expect-undo-depth ${beforeMixed} 1",
    "focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeMixed} 0", "key Escape")

# The Appearance tool's picker: Paint - Blue chosen by its search shows on
# the picker button; Cancel writes nothing.
body_menu(2, "Appearance…")
add(log("Appearance tool for F2.b0"), "capture-undo-depth beforePicker")
picker("Appearance", "value", "Appearance", "Paint - Blue")
add(log("Panel Appearance tile value: Paint - Blue (Library)"))
if native:
    add("expect-native-visible input_value true")
shot("appearance-picked")
add("mark")
cancel()
add("expect-log '^Command Appearance cancelled'", "expect-undo-depth ${beforePicker} 0", "focus-main")

# Configurations' real fourth tab: actual populated rows, option/search
# filters, selected warning, the Material tool started on a binding,
# remove/undo and selection.
add("command solid.configurations", "focus-dialog '^Configurations$'")
tab("Bindings")
add("expect-log 'Configurations bindings status: 3 bindings, 1 warning'",
    "expect-log 'Configurations binding Body1 Appearance: Color:'",
    "expect-log 'Configurations binding Body2 Material: Color:'",
    "expect-log 'Configurations binding Body3 Appearance: .*warning .*paint_purple'",
    "capture-undo-depth beforeBindings")
choice("configurationsBindingsDrivenBy", "Color")
field("configurationsBindingsSearch", "Body3")
add("expect-log \"Configurations bindings driven by Color, search 'Body3': 1 of 3 bindings, 1 warning\"")
if native:
    add("native-pointer-click configurationsBindings/0/0",
        "expect-native-text configurationsBindingsStatus 'paint_purple'",
        "expect-native-bounds configurationsBindingsRemove 60")
else:
    add("dialog-click-logged 'Configurations binding Body3 Appearance at '")
shot("bindings-warning")
field("configurationsBindingsSearch", "Body2")
add("expect-log \"Configurations bindings driven by Color, search 'Body2': 1 of 3 bindings\"", "mark")
if native:
    add("native-pointer-dblclick configurationsBindings/0/0")
else:
    add("dialog-dblclick-logged 'Configurations binding Body2 Material at '")
add("expect-log 'Configurations binding edit: Body2 Material'", log("Material tool for bodies F2.b0"),
    log(f"Material tool: the targets follow {MATERIAL_SWITCH}"), "focus-main")
cancel()
add("expect-log '^Command Material cancelled'", "focus-dialog '^Configurations$'", "mark")
press("Select in Browser")
add("expect-log 'Configurations binding select: Body2 Material'", "focus-dialog '^Configurations$'", "mark")
press("Remove Binding")
add("expect-log 'Configurations binding removed: Body2 Material'",
    "expect-undo-depth ${beforeBindings} 1")
shot("bindings-removed")
press("Close")
add("focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeBindings} 0",
    "command solid.configurations", "focus-dialog '^Configurations$'")
tab("Options")
add("expect-log 'Configurations option Color used by 3 bindings'")
tab("Bindings")
field("configurationsBindingsSearch", "")
shot("bindings-restored")
press("Close")
add("focus-main", "key Ctrl+s", "expect-log 'Saved .*bindings[.]mitcad'",
    "expect-model-bodies 3 48000",
    "expect-never 'Native Mac canvas[:]|OCCT view event failed|Recompute failed'")
print("\n".join(lines))
