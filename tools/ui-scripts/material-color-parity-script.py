#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The Materials window's colours: a filament's colour is its appearance
(mitcad#1055), shown by the list of the material's appearances, the
default marked in its row (mitcad#1083); Add…'s popover of the appearance
library opened and closed without a change. The AppKit frontend shows the
list as a native table and the popover as the tools' picker popover under
Add… (mitcad#1068); there Add… also chooses in the popover, then Make
Default and Remove."""
import sys

native = sys.argv[1] == "native"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle"]
if native:
    lines += ["expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
              "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'"]
lines += ["window-resize 1280 820", "expect-model-bodies 1 1000", "capture-undo-depth baseline",
          "command solid.materials", "focus-dialog '^Materials$'"]


def add(*steps):
    lines.extend(steps)


def select(identifier, name):
    if native:
        # The material's row in the AppKit outline (NativeControlIdentifierRole).
        add("mark", f"native-pointer-click 'material.design/{identifier}'",
            f"expect-log '^Material selected: {name} \\(design\\)'", "wait-idle")
    else:
        # Incremental search selects the actual visible tree row; then its
        # logged pointer target.
        add("mark", "focus-widget materialsList", "key Home", f"type '{name}'",
            f"expect-log '^Material selected: {name} \\(design\\)'", "wait-idle",
            f"dialog-click-logged 'Materials item {name} at '")


def open_add_popover(capture_name):
    add("mark", "native-pointer-click materialAppearanceAdd" if native else "button '^Add$' materialAppearanceAdd",
        "expect-log '^Materials add appearance picker opened: [0-9]+ appearances$'")
    if native:
        # Under Add…, its shared Qt presenter never on the screen.
        add("expect-log '^Native picker appearancePicker at materialAppearanceAdd: [0-9]+ rows$'",
            "focus-dialog '^appearancePicker$'",
            "expect-native-visible appearancePickerSearch true",
            "expect-native-visible 'appearancePickerList.item.Swatch source' true")
        # AppKit consumes the outside press that dismisses a transient
        # popover. Separate driver steps give AppKit an event-loop turn
        # before Add… receives the next press during the close animation.
        # The wrapper's 50 ms timer and zero settling delay keep this
        # genuine overlap; its retired-close assertion must prove it.
        add("mark", "native-pointer-click materialAppearanceAdd",
            "native-pointer-click materialAppearanceAdd",
            "wait-log '^Native picker appearancePicker at materialAppearanceAdd: [0-9]+ rows$'",
            "wait-log '^Native picker retired close: appearancePicker, successor shown$'",
            "sleep 600", "focus-dialog '^appearancePicker$'",
            "expect-native-visible appearancePickerSearch true",
            "expect-native-visible 'appearancePickerList.item.Swatch source' true",
            "screenshot-screen '${MITCAD_TEST_WORK}/" + capture_name +
            "-picker-reopened-${MITCAD_TEST_COLOR_SCHEME}.png'")


# The colour a command gave a material of its own became its appearance
# (Audit Explicit color); an appearance chosen stays; none leaves the list
# empty.
for identifier, name, offered, first in [
        ("explicit-color", "Audit Explicit", "Audit Explicit color \\(default\\)", "Audit Explicit color"),
        ("inherited-color", "Audit Inherited", "Swatch source \\(default\\)", "Swatch source"),
        ("unset-color", "Audit Slicer Default", "none", None)]:
    select(identifier, name)
    add(f"expect-ever '^Materials appearances: {offered}$'")
    if native:
        # The list as a native table: the swatch and name, Default beside
        # the default; a muted None row without any.
        if first is not None:
            add(f"expect-native-text materialAppearances/0/0 '^{first}$'",
                "expect-native-style materialAppearances/0/0 image",
                "expect-native-text materialAppearances/0/1 '^Default$'")
        else:
            add("expect-native-text materialAppearances/0/0 '^None$'")
    add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/{identifier}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
    # Add…'s popover of the appearance library, closed with Escape.
    open_add_popover(identifier)
    add(f"screenshot-screen '${{MITCAD_TEST_WORK}}/{identifier}-picker-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
    if not native:
        # The Qt popover is a window of its own (mitcad#1071): its search
        # field takes Escape, not the Materials window under it.
        add("focus-widget appearancePickerSearch")
    add("key Escape", "expect-log '^Materials add appearance picker closed$'",
        "focus-dialog '^Materials$'", "expect-undo-depth ${baseline} 0", "expect-model-bodies 1 1000")

changes = 0
if native:
    # Add… with Paint - Red chosen by the popover's search (the new row
    # selected), Make Default, then Remove of the default: each one
    # edit_material, one undo step.
    select("inherited-color", "Audit Inherited")
    open_add_popover('paint-choice')
    add("native-type appearancePickerSearch 'Paint - Red'",
        "expect-log '^Materials add appearance picker search: Paint - Red, [0-9]+ shown$'", "key Return",
        "expect-log '^Materials add appearance picker chosen: Paint - Red \\(paint_red\\)$'",
        "expect-log '^Material Audit Inherited \\(design\\) appearances = swatch-source, paint_red$'",
        "expect-log '^Materials appearances: Swatch source \\(default\\) \\| Paint - Red$'",
        "expect-undo-depth ${baseline} 1", "focus-dialog '^Materials$'", "wait-idle",
        "expect-native-text material.appearance.paint_red '^Paint - Red$'",
        "mark", "native-pointer-click material.appearance.paint_red",
        "native-pointer-click materialAppearanceDefault",
        "expect-log '^Material Audit Inherited \\(design\\) appearance = paint_red$'",
        "expect-log '^Materials appearances: Paint - Red \\(default\\) \\| Swatch source$'",
        "expect-undo-depth ${baseline} 2", "wait-idle",
        "expect-native-text materialAppearances/0/0 '^Paint - Red$'",
        "expect-native-text materialAppearances/0/1 '^Default$'",
        "screenshot-dialog '${MITCAD_TEST_WORK}/appearances-${MITCAD_TEST_COLOR_SCHEME}.png'",
        "mark", "native-pointer-click material.appearance.paint_red",
        "native-pointer-click materialAppearanceRemove",
        "expect-log '^Materials appearance default removed: Swatch source becomes the default$'",
        "expect-log '^Material Audit Inherited \\(design\\) appearances = swatch-source$'",
        "expect-log '^Materials appearances: Swatch source \\(default\\)$'",
        "expect-undo-depth ${baseline} 3", "wait-idle")
    changes = 3

add("native-pointer-button '^Close$'" if native else "button '^Close$'", "focus-main",
    f"expect-undo-depth ${{baseline}} {changes}", "expect-model-bodies 1 1000",
    "expect-never 'Native Mac canvas[:]|OCCT view event failed|Recompute failed'")
print("\n".join(lines))
