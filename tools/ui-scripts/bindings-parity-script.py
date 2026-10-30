#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise shared assignment and Bindings widgets in Qt and AppKit."""
import sys
import json
import re

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 3 48000", "key F6"]


def add(*steps):
    lines.extend(steps)


def press(label):
    # Assignment actions can be below the editor's fold. The stable native
    # control resolves that exact button before scrolling and hit-testing
    # its actual pointer target; caption and enabled state remain required.
    assignment_actions = {"Formula Editor…": "assignmentFormula", "Replace with a table": "assignmentReplace"}
    if native and label in assignment_actions:
        identifier = assignment_actions[label]
        add(f"expect-native-text {identifier} '^{label}$'",
            f"expect-native-enabled {identifier} true", f"native-pointer-click {identifier}")
    elif label in assignment_actions:
        identifier = assignment_actions[label]
        add(f"expect-widget-property {identifier} text '{label}'",
            f"expect-widget-property {identifier} enabled true",
            f"focus-widget {identifier}", "key Space")
    else:
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


def appearance(number, bound=False):
    body_menu(number, "Appearance…\tfx" if bound else "Appearance…")
    add("focus-dialog '^Appearances$'")


def appearance_fill(identifier):
    if native:
        # The narrow form intentionally leaves a scrolling library. Use its
        # real search before targeting a row, including after another fill.
        name = {"paint_black": "Paint - Black", "paint_red": "Paint - Red",
                "paint_white": "Paint - White"}[identifier]
        add("native-pointer-click appearanceSearch", "native-key-event Ctrl+a",
            f"type '{name}'",
            f"expect-log '^Appearances filtered \"{name}\" .*: 1 shown$'",
            f"expect-native-visible appearance.{identifier} true",
            f"native-pointer-dblclick appearance.{identifier}")
    else:
        add(f"dialog-dblclick-logged 'Appearances item {identifier} at '")


def material_fill(identifier, name):
    add(f"native-pointer-dblclick material.mitcad/{identifier}" if native else
        f"dialog-dblclick-logged 'Materials item {name} at '")


def tab(name):
    add(f"native-choice configurationsTabs {name}" if native else
        f"dialog-click-logged 'Configurations tab {name}'",
        f"expect-log 'Configurations page {name}'")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")

# Varies with is a pending presentation choice until the actual library
# double-click fills a row. Each fill is one command/undo step.
appearance(1)
add("expect-log 'Appearances panel: Assign to Body1; varies with None'",
    "capture-undo-depth beforeAppearance")
if native:
    add("expect-native-text assignmentTitle '^Assign to Body1$'")
shot("assignment-none")
add("mark")
choice("assignmentVaries", "Color")
add("expect-log 'Appearances varies with Color: 2 rows, not written yet'",
    "expect-no-log 'Appearances binding of'", "expect-undo-depth ${beforeAppearance} 0")
if native:
    add("expect-native-text assignment.value.0 '^Black \\(now\\)$'",
        "expect-native-selection assignmentRows 1")
    # The long appearance form scrolls above the populated assignment panel
    # and footer, as in Qt. A small supported window must not clamp to the
    # complete form's height or lose the table/Formula Editor actions.
    add("native-dialog-resize 720 440",
        "expect-log '^Native dialog size: requested 720x440, actual 720x440$'",
        "expect-native-bounds appearanceBody 0",
        "expect-native-bounds assignmentPanel 0",
        "expect-native-bounds assignmentRows 0",
        "expect-native-bounds assignmentFormula 80",
        "expect-native-bounds appearanceAssign 40",
        "expect-native-bounds appearanceClose 30",
        "expect-native-bounds appearanceTextureEmbed 100",
        "expect-native-bounds appearanceSearch 100",
        "expect-native-form-layout appearanceBody assignmentTitle assignmentVaries assignmentRows assignmentFormula appearanceAssign appearanceClose")
    shot("assignment-narrow")
appearance_fill("paint_black")
add("expect-log \"Appearances filled Black with 'paint_black'\"",
    "expect-log 'Appearances assign label: Assign to row Red'",
    "expect-undo-depth ${beforeAppearance} 1")
appearance_fill("paint_red")
add("expect-log \"Appearances filled Red with 'paint_red'\"",
    "expect-log 'Appearances active row: none'", "expect-undo-depth ${beforeAppearance} 2")
if native:
    add("expect-native-selection assignmentRows 0", "expect-native-visible assignmentFx true",
        "expect-native-text assignment.result.0 '^Paint - Black$'",
        "expect-native-text assignment.result.1 '^Paint - Red$'")
shot("appearance-bound")
press("Close")
add("focus-main", "mark")
choice("configurationChooser_Color", "Red")
add("expect-log '^Configuration chosen: Color = Red'", "wait-idle", "expect-model-bodies 3 48000")

# The same panel in Materials drives the shared material binding, using
# actual library double-clicks, including automatic advance to the next row.
body_menu(2, "Material…")
add("focus-dialog '^Materials$'", "capture-undo-depth beforeMaterial", "mark")
choice("assignmentVaries", "Color")
add("expect-log 'Materials varies with Color: 2 rows, not written yet'",
    "expect-undo-depth ${beforeMaterial} 0")
material_fill("abs", "ABS Plastic")
add("expect-log \"Materials filled Black with 'abs'\"",
    "expect-log 'Materials assign label: Assign to row Red'",
    "expect-undo-depth ${beforeMaterial} 1")
material_fill("pla", "PLA Plastic")
add("expect-log \"Materials filled Red with 'pla'\"",
    "expect-log 'Materials active row: none'", "expect-undo-depth ${beforeMaterial} 2")
shot("material-bound")
press("Close")

# Back to None keeps the currently displayed Red result. Undo restores the
# complete binding rather than rewriting it from the current option value.
appearance(1, bound=True)
add("expect-log 'Appearances panel: Assign to Body1; varies with Color \\(bound\\)'",
    "capture-undo-depth beforeNone", "mark")
choice("assignmentVaries", "None", "Up Return")
add("expect-log 'Appearances back to None: paint_red for Body1'",
    "expect-undo-depth ${beforeNone} 1")
shot("appearance-none-red")
press("Close")
add("focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeNone} 0")

# A custom formula stays custom until an explicit replacement. Opening
# the real Formula Editor from this panel uses the same shared write path;
# replacing its presentation with a table alone writes nothing.
appearance(1, bound=True)
add("capture-undo-depth beforeCustom")
press("Formula Editor…")
add("focus-dialog '^Formula Editor$'")
custom_formula = "if(Holes == 'Yes'; 'paint_red'; 'paint_black')"
add("native-pointer-click formulaText" if native else "focus-widget formulaText",
    "key Ctrl+a", "type " + json.dumps(custom_formula))
add("expect-native-text formulaText " + json.dumps('^' + re.escape(custom_formula) + '$') if native else
    "expect-widget-property formulaText plainText " + json.dumps(custom_formula))
if native:
    # AppKit can defer smart substitutions until focus leaves the editor.
    # The actual formula must retain its literal quotes at that point too.
    add("native-pointer-click formulaFunctionSearch",
        "expect-native-text formulaText " + json.dumps('^' + re.escape(custom_formula) + '$'))
press("OK")
add("expect-native-dialog-gone '^Formula Editor$'" if native else "expect-dialog-gone '^Formula Editor$'",
    "focus-dialog '^Appearances$'",
    "expect-log 'Appearances from the Formula Editor: if\\(Holes'",
    "expect-undo-depth ${beforeCustom} 1")
if native:
    add("expect-native-text assignmentCustom '^Custom: if\\(Holes'",
        "expect-native-visible assignmentReplace true")
shot("assignment-custom")
add("mark")
press("Replace with a table")
add("expect-log 'Appearances: the expression replaced with a table of Color \\(not written yet\\)'",
    "expect-no-log 'Appearances binding of'", "expect-undo-depth ${beforeCustom} 1")
press("Close")
add("focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeCustom} 0")

# Existing bindings and a plain assignment produce the shared Mixed notice.
# Shift-select both value rows; one real library double-click fills all
# selected rows for all three targets, as a single undo step.
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
appearance(1, bound=True)
add("expect-log 'Appearances panel: Assign to 3 Bodies'",
    "expect-log 'Appearances mixed: Mixed:'",
    "expect-log 'Filling a row gives all 3 bodies this table.'",
    "capture-undo-depth beforeMixed")
if native:
    add("expect-native-text assignmentMixedText '^Mixed: .*Filling a row gives all 3 bodies this table[.]$'",
        "native-pointer-click assignmentRows/0/0", "native-key-event Shift+Down",
        "expect-native-selection assignmentRows 2")
else:
    # Logged pointer events reach the row viewport directly; establish its
    # keyboard focus before extending that real click's selection.
    add("dialog-click-logged 'Appearances value Black at '",
        "focus-widget assignmentRows", "expect-widget-property assignmentRows focus true",
        "key Shift+Down",
        "expect-widget-property appearanceAssign text 'Assign to 2 Rows (3 bodies)'")
shot("assignment-mixed")
add("mark")
appearance_fill("paint_white")
add("expect-log \"Appearances filled Black, Red with 'paint_white'\"",
    "expect-log 'Appearances binding of Body1, Body2 and Body3:'",
    "expect-undo-depth ${beforeMixed} 1")
press("Close")
add("focus-main", "command edit.undo", "wait-idle", "expect-undo-depth ${beforeMixed} 0", "key Escape")

# Configurations' real fourth tab: actual populated rows, option/search
# filters, selected warning, editor opening, remove/undo and selection.
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
add("expect-log 'Configurations binding edit: Body2 Material'", "focus-dialog '^Materials$'",
    "expect-log 'Materials panel: Assign to Body2; varies with Color \\(bound\\)'")
press("Close")
add("focus-dialog '^Configurations$'", "mark")
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
