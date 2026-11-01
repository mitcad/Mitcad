#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise Cut Sheet widgets, shared native menus and file panels on macOS."""
import sys

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 2 21769.91118430775", "capture-undo-depth baseline"]


def add(*steps):
    lines.extend(steps)


def press(label):
    add(f"native-pointer-button '^{label}$'" if native else f"button '^{label}$'")


def field(identifier, value):
    add(f"native-pointer-click {identifier}" if native else f"focus-widget {identifier}",
        "key Ctrl+a", f"type '{value}'", "key Tab")


def choice(identifier, title, index):
    if native:
        add(f"native-choice {identifier} '{title}'")
    else:
        add(f"focus-widget {identifier}", "key Space", "key Home",
            *(["key Down"] * index), "key Return")


def visible(identifier, shown):
    add(f"expect-native-visible {identifier} {shown}" if native else
        f"expect-widget-property {identifier} visible {shown}")


def create():
    add("command file.create_drawing", "focus-dialog '^Create Drawing$'",
        "expect-log 'Create Drawing dialog opened'")


def material():
    add("click-logged '^Drawing Sheet at '", "focus-dialog '^Cut Sheet$'",
        "expect-log 'Cut sheet material dialog opened'")


def export_menu(entry):
    # The CUT SHEET tab's Export group (mitcad#1008): its caption's menu,
    # in AppKit the native ribbon's (mitcad#1073).
    add(f"native-popup native.ribbon.group.export '{entry}'" if native else
        "click-logged '^Drawing Export at '")
    if not native:
        add(f"popup '{entry}'")


def export_open():
    if native:
        # The Export menu's DXF… item by its action: chosen inside the
        # popup's tracking loop (native-popup), its file panel would run
        # modally in that loop and the step would never return.
        add("native-action drawing.export_dxf")
    else:
        export_menu("DXF…")
    add("focus-dialog '^Export Cut Sheet$'")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")

# A real kind change hides whole form rows. Reversing it must restore the
# ordinary drawing controls without losing the cut-sheet dimensions.
create()
visible("drawingTemplate", "true")
visible("drawingCutWidth", "false")
choice("drawingKind", "Cut Sheet", 1)
visible("drawingTemplate", "false")
visible("drawingSize", "false")
visible("drawingProjection", "false")
visible("drawingCutWidth", "true")
visible("drawingCutHeight", "true")
field("drawingCutWidth", "800")
field("drawingCutHeight", "400")
choice("drawingKind", "Drawing", 0)
visible("drawingTemplate", "true")
visible("drawingCutWidth", "false")
choice("drawingKind", "Cut Sheet", 1)
visible("drawingCutWidth", "true")
add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/cut-create-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
press("Cancel")
add("focus-main", "expect-undo-depth ${baseline} 0", "mark")

# Cancel did not create a drawing or retain a previous dialog's edits.
create()
choice("drawingKind", "Cut Sheet", 1)
if native:
    add("expect-native-text drawingCutWidth '^600([.,]0)? mm$'",
        "expect-native-text drawingCutHeight '^300([.,]0)? mm$'")
else:
    add("expect-widget-property drawingCutWidth value 600",
        "expect-widget-property drawingCutHeight value 300")
press("OK")
add("focus-main", "expect-log 'Create Drawing dialog: cut sheet 600 x 300'",
    "expect-log 'Drawing created: D1 CutSheet1 in C0'", "expect-log 'Drawing sheet drawn: 0 views'",
    "expect-log 'Ribbon tabs: CUT SHEET \\(shown CUT SHEET\\)'")
if native:
    # The CUT SHEET tab alone in the AppKit ribbon, of the cards only the
    # Browser (mitcad#1073).
    add("expect-log 'Native workspaces: Cut Sheet \\(shown Cut Sheet\\)'",
        "expect-log 'Native cards: Browser only \\(a drawing is open\\)'",
        "expect-native-visible native.ribbon.drawing.close true",
        "expect-native-visible native.ribbon.drawing.add_parts true",
        "expect-native-visible native.ribbon.group.export true",
        "expect-native-present native.ribbon.group.dimensions false",
        "expect-native-visible native.sidebar true",
        "expect-native-visible native.historyCard false")
add("expect-undo-depth ${baseline} 1", "capture-undo-depth beforeParts", "mark",
    "native-click native.ribbon.drawing.add_parts" if native else "click-logged '^Drawing Add Parts at '",
    "focus-dialog '^Add Components$'", "expect-log 'Cut sheet Add Parts dialog opened: 2 bodies'")
if native:
    # addParts explicitly gives its list the initial keyboard focus. The
    # native dialog must honour that before any corrective pointer input.
    add("expect-native-focus cutSheetBodies", "key Ctrl+a", "expect-native-selection cutSheetBodies 2")
    # Hit the actual first cell, not empty space in the list's scroll view.
    # Then Command-A must publish both rows through the normal table bridge.
    add("native-pointer-click cutSheetBodies/0/0", "expect-native-selection cutSheetBodies 1")
else:
    add("expect-widget-property cutSheetBodies focus true")
add("key Ctrl+a")
if native:
    add("expect-native-selection cutSheetBodies 2")
field("cutSheetCount", "2")
if native:
    add("expect-native-selection cutSheetBodies 2")
add(f"screenshot-dialog '${{MITCAD_TEST_WORK}}/cut-parts-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
press("OK")
add("focus-main", "expect-log '\"cmd\":\"add_cut_sheet_parts\"'",
    "expect-log 'Drawing sheet drawn: 4 views'", "expect-undo-depth ${beforeParts} 1",
    "capture-undo-depth beforeMaterial", "mark")

# Material edits remain local until OK and are one undoable model action.
material()
field("cutSheetWidth", "800")
field("cutSheetHeight", "400")
press("Cancel")
add("focus-main", "expect-undo-depth ${beforeMaterial} 0", "mark")
material()
if native:
    add("expect-native-text cutSheetWidth '^600([.,]0)? mm$'",
        "expect-native-text cutSheetHeight '^300([.,]0)? mm$'")
else:
    add("expect-widget-property cutSheetWidth value 600", "expect-widget-property cutSheetHeight value 300")
field("cutSheetWidth", "800")
field("cutSheetHeight", "400")
press("OK")
add("focus-main", "expect-log '\"cmd\":\"edit_cut_sheet\"'",
    "expect-undo-depth ${beforeMaterial} 1", "command edit.undo", "wait-idle",
    "expect-undo-depth ${beforeMaterial} 0", "mark")
material()
if native:
    add("expect-native-text cutSheetWidth '^600([.,]0)? mm$'",
        "expect-native-text cutSheetHeight '^300([.,]0)? mm$'")
else:
    add("expect-widget-property cutSheetWidth value 600", "expect-widget-property cutSheetHeight value 300")
press("Cancel")
add("focus-main", "command edit.redo", "wait-idle", "expect-undo-depth ${beforeMaterial} 1",
    "capture-undo-depth beforeRotate", "mark")

# Select the actual plotted part, then open its menu at that same visible
# hit point. Native mode must track and choose the genuine AppKit menu.
add("click-logged '^Drawing view 1 at '", "expect-log 'Drawing view 1 selected'",
    "context-menu-logged '^Drawing view 1 at ' 'Rotate 90° Left'",
    "expect-log '\"cmd\":\"edit_cut_sheet_part\"'", "expect-undo-depth ${beforeRotate} 1",
    "capture-undo-depth beforeCopy", "mark",
    "context-menu-logged '^Drawing view 1 at ' 'Add Copy'",
    "expect-log 'Drawing sheet drawn: 5 views'", "expect-undo-depth ${beforeCopy} 1",
    "capture-undo-depth beforeDelete", "mark",
    "context-menu-logged '^Drawing view 5 at ' 'Delete Component'",
    "expect-log 'Drawing sheet drawn: 4 views'", "expect-undo-depth ${beforeDelete} 1",
    "command edit.undo", "wait-idle", "expect-log 'Drawing sheet drawn: 5 views'",
    "expect-undo-depth ${beforeDelete} 0", "command edit.redo", "wait-idle",
    "expect-log 'Drawing sheet drawn: 4 views'", "expect-undo-depth ${beforeDelete} 1",
    "capture-undo-depth beforeExport", "mark")

# Cancel and accept the real file chooser opened by the actual format
# popup (AppKit: its Export menu's action). The saved DXF and model are
# checked independently by the harness.
export_menu("Component Names on the LABEL Layer")
add("expect-log '^Cut sheet labels: on$'")
export_open()
if native:
    add("native-file-cancel", "expect-native-dialog-gone '^Export Cut Sheet$'")
else:
    press("Cancel")
add("focus-main", "expect-undo-depth ${beforeExport} 0", "expect-no-log '^Exported '", "mark")
export_open()
if native:
    # The Save panel's filename/directory editor lives in the XPC service.
    # Use its real WindowServer input; local NSSavePanel property setters
    # can appear to succeed while that remote editor keeps its old values.
    # Save is permitted only after both destination fields match exactly.
    add("native-file-state save-before-entry", "native-system-key Ctrl+a",
        f"native-system-type 'cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.dxf'",
        "native-system-key Tab", "sleep 400", "native-file-state save-name-entered",
        f"expect-native-text native.file.name '^cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME:re}}\\.dxf$'",
        "native-system-key Ctrl+Shift+g", "sleep 400", "native-file-state save-goto-open",
        "native-system-key Ctrl+a", "native-system-type '${MITCAD_TEST_WORK}'", "sleep 300",
        "native-system-key Return", "sleep 500", "native-file-state save-goto-accepted",
        "expect-native-text native.file.directory '^${MITCAD_TEST_WORK:re}$'",
        f"expect-native-text native.file.name '^cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME:re}}\\.dxf$'",
        "native-file-state save-before-return", "native-system-key Return", "sleep 500",
        "native-file-state save-return-delivered", "expect-native-dialog-gone '^Export Cut Sheet$'",
        "native-file-state save-panel-closed")
else:
    add("focus-widget fileNameEdit", "key Ctrl+a",
        f"type '${{MITCAD_TEST_WORK}}/cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.dxf'", "key Return",
        "expect-dialog-gone '^Export Cut Sheet$'")
# WindowServer/XPC acceptance is asynchronous. Keep the real chooser's
# focus until it has closed; focusing the canvas can steal the Return.
add("focus-main",
    f"expect-log '^Exported ${{MITCAD_TEST_WORK:re}}/cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME:re}}\\.dxf: cut sheet CutSheet1, 4 parts$' 30",
    f"expect-file '${{MITCAD_TEST_WORK}}/cut-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.dxf'",
    "expect-undo-depth ${beforeExport} 0", "expect-model-bodies 2 21769.91118430775",
    f"screenshot-screen '${{MITCAD_TEST_WORK}}/cut-workspace-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'",
    "menu 'File/Save'", "expect-log '^Saved '",
    "expect-never 'Recompute failed|Drawing command refused|Native Mac canvas[:]|OCCT view event failed'")
print("\n".join(lines))
