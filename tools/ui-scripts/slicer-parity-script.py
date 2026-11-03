#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Create paired driver input without replacing real UI interactions."""
import sys

native = sys.argv[1] == "native"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820", "focus-main",
         "expect-model-bodies 1 4000", "capture-undo-depth baseline"]
if native:
    lines += ["expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
              "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'"]


def add(*steps):
    lines.extend(steps)


def press(button, widget=None):
    if native:
        add(f"native-pointer-button '^{button}$'")
    else:
        # TestDriver normalizes Qt captions by stripping their ellipsis.
        caption = button.rstrip(".\u2026")
        add(f"button '^{caption}$'" + (f" {widget}" if widget else ""))


def import_presets(count):
    # Both frontends show the selected count in the actual button caption.
    caption = f"Import {count} Preset" + ("s" if count != 1 else "")
    add(f"expect-log '^Slicer presets ticked: {count}, {caption}$'")
    press(caption)


def field(name, text):
    add(f"native-pointer-click {name}" if native else f"focus-widget {name}",
        "key Ctrl+a", f"type '{text}'", "key Return")


def menu(choice):
    if native:
        add(f"native-popup slicerImportButton '{choice}'")
    else:
        add("button '^Import from Slicer'", f"popup '{choice}'")


def tick(kind, name):
    add(f"native-pointer-click 'slicerPreset.{kind}.{name}'" if native else
        f"dialog-click-logged 'Slicer presets item {name} at '")


def row_choice(kind, name, choice):
    add(f"{'native-choice' if native else 'qt-choice'} 'slicerPresetChoice.{kind}.{name}' '{choice}'")


def capture(surface):
    add("screenshot-dialog '${MITCAD_TEST_WORK}/slicer-" + surface + "-" +
        ("native" if native else "qt") + "-${MITCAD_TEST_COLOR_SCHEME}.png'")


# The Printers and the Processes managers' titles (mitcad#1135).
MANAGERS = {"printer": "Printers", "process": "Processes"}

# Both action entries create a design profile and its choice in one step.
# The application panel has no preview: its independently committed
# profile changes persist after Cancel, in both frontends.
add("command make.print3d", "expect-log '^Command Send to Slicer started$'", "wait-idle",
    "capture-undo-depth profilesBaseline")
for depth, (kind, label) in enumerate([("process", "Process"), ("printer", "Printer")], start=1):
    add("mark")
    if native:
        add(f"native-pointer-click input_{kind}")
    else:
        # The command field opens the searchable picker, not a combo popup.
        add(f"click-logged '^Panel Send to Slicer input {kind} at '")
    add(f"focus-dialog '^{label} Picker$'", f"type 'New {kind}'", "key Return")
    # The manager of its kind (mitcad#1135).
    add(f"expect-log '3D Print {kind} new: {label} 1'", f"focus-dialog '^{MANAGERS[kind]}$'",
        f"expect-log 'Profile selected: {kind} {label} 1'", "expect-model-bodies 1 4000",
        f"expect-undo-depth ${{profilesBaseline}} {depth}")
    if native and kind == "process":
        # A new process leaves its choices unset: the popup shows the shared
        # placeholder without a guessed item (mitcad#976). This runs before
        # the file chooser, which needs event-posting access.
        add("native-choice settingsTabs Quality", "expect-native-visible printSetting_seam_position true",
            "expect-widget-property printSetting_seam_position currentIndex -1",
            "expect-native-text printSetting_seam_position '^Slicer default$'")
    press("Close")
    add("focus-main")
add("key Escape", "expect-log '^Command Send to Slicer cancelled$'", "wait-idle",
    "expect-undo-depth ${baseline} 2", "expect-model-bodies 1 4000", "mark",
    "command make.printers", "focus-dialog '^Printers$'",
    "expect-log 'Printers row: printer/printer1 Printer 1 \\(design\\), the design.s'")
press("Close")
add("focus-main", "mark", "command make.processes", "focus-dialog '^Processes$'",
    "expect-log 'Processes row: process/process1 Process 1 \\(design\\), the design.s'")
press("Close")
add("focus-main", "command edit.undo", "command edit.undo", "expect-undo-depth ${baseline} 0",
    "expect-model-bodies 1 4000")

# Import from Slicer reaches the production file chooser from the converted
# dialog's real popup button; the selected 3MF brings no geometry with it.
add("command make.processes", "focus-dialog '^Processes$'")
menu("From a file…")
add("focus-dialog '^Import from Slicer$'")
if native:
    add("native-file-directory '${MITCAD_TEST_WORK}'", "native-file-state directory-set", "native-system-key Ctrl+Shift+g", "sleep 400",
        "native-file-state goto-open",
        "native-system-key Ctrl+a", "native-system-type '${MITCAD_TEST_WORK}/plate.3mf'", "sleep 300", "native-file-state path-entered", "native-system-key Return", "sleep 500",
        "native-file-state goto-accepted", "native-system-key Return", "sleep 300", "native-file-state open-accepted")
else:
    add("focus-widget fileNameEdit", "type '${MITCAD_TEST_WORK}/plate.3mf'", "key Return")
add("expect-log 'Imported print settings from plate.3mf: printers: Bench X; processes: Standard'",
    "focus-dialog '^Processes$'", "expect-log 'Profile selected: process Standard \\(design\\)'",
    "expect-model-bodies 1 4000", "expect-undo-depth ${baseline} 1")
press("Details…")
add("focus-dialog '^Import Details$'", "expect-log 'Import details: Printer: Bench X'")
press("Close")
add("focus-dialog '^Processes$'")

# Category selection must actually expose its field. Search then narrows
# another category, whose invalid value is refused and whose edit is one step.
if native:
    add("native-choice settingsTabs Quality", "expect-native-visible printSetting_layer_height true")
else:
    add("dialog-click-logged 'Processes settings tab quality'")
field("printSetting_layer_height", "0.25")
add("expect-log 'Profile Standard \\(design\\) layer_height = 0\\.25 mm'", "expect-undo-depth ${baseline} 2")
field("settingsSearch", "infill density")
field("printSetting_fill_density", "250")
add("expect-log 'Setting fill_density refused:'", "expect-undo-depth ${baseline} 2")
field("printSetting_fill_density", "25")
add("expect-log 'Profile Standard \\(design\\) fill_density = 25 %'", "expect-undo-depth ${baseline} 3",
    "screenshot-dialog '${MITCAD_TEST_WORK}/slicer-file-" + ("native" if native else "qt") + "-${MITCAD_TEST_COLOR_SCHEME}.png'")
# Clearing a cross-category search must restore the actual row subtrees.
# Also expand and contract Advanced in the selected category before editing
# the restored field; this catches retained or missing hidden constraints.
add("native-pointer-click settingsSearch" if native else "focus-widget settingsSearch",
    "key Ctrl+a", "key Backspace", "key Tab")
if native:
    add("native-choice settingsTabs Quality", "expect-native-visible printSetting_layer_height true",
        "expect-native-visible printSetting_fill_density false")
else:
    add("dialog-click-logged 'Processes settings tab quality'")
press("Advanced")
press("Advanced")
field("printSetting_layer_height", "0.25")
add("expect-undo-depth ${baseline} 3", "expect-model-bodies 1 4000")
if native:
    # An unset choice shows the shared placeholder without a guessed item
    # (mitcad#976); a real choice replaces it and its undo restores it.
    add("expect-widget-property printSetting_seam_position currentIndex -1",
        "expect-native-text printSetting_seam_position '^Slicer default$'",
        "mark", "native-choice printSetting_seam_position Aligned",
        "expect-log 'Profile Standard \\(design\\) seam_position = Aligned'",
        "expect-undo-depth ${baseline} 4", "expect-native-text printSetting_seam_position '^Aligned$'",
        "native-action edit.undo", "expect-undo-depth ${baseline} 3",
        "expect-native-text printSetting_seam_position '^Slicer default$'",
        "expect-widget-property printSetting_seam_position currentIndex -1")
press("Close")
add("focus-main", "command edit.undo", "command edit.undo", "expect-undo-depth ${baseline} 1",
    "command make.processes", "focus-dialog '^Processes$'")

# Review starts on the same-named printer without selecting its warning row.
# Keyboard changes remain provisional until Import; Cancel changes no model.
menu("Presets of Bambu Studio…")
add("focus-dialog '^Import Presets$'", "expect-log 'Slicer presets opened: Bambu Studio, 3 presets'",
    "expect-log 'Slicer presets row: process Fine, ticked'",
    "expect-log 'Slicer presets row: material Amber, ticked'",
    "expect-log 'Slicer presets row: printer Bench X, unticked, same name:'",
    "expect-undo-depth ${baseline} 1")
if native:
    add("expect-native-focus slicerPresetsList",
        "expect-native-style 'slicerPreset.printer.Bench X' current-row,unchecked,warning")
add("mark", "key Space", "expect-log 'Slicer presets tick: Bench X ticked'",
    "mark", "key Space", "expect-log 'Slicer presets tick: Bench X unticked'", "key Tab",
    "expect-log 'Slicer presets focus: Bench X choice'")
if native:
    add("expect-native-text 'slicerPresetSameName.printer.Bench X' '^Same name:$'",
        "expect-native-focus 'slicerPresetChoice.printer.Bench X'",
        "key Shift+Tab", "expect-native-focus slicerPresetsList",
        "expect-native-style 'slicerPreset.printer.Bench X' current-row,unchecked,warning",
        "mark", "key Tab", "expect-log 'Slicer presets focus: Bench X choice'",
        "expect-native-focus 'slicerPresetChoice.printer.Bench X'")
row_choice("printer", "Bench X", "Override")
add("expect-log 'Slicer presets choice: Bench X override'", "expect-log 'Slicer presets tick: Bench X ticked'")
capture("review")
press("Cancel")
add("expect-log 'Slicer presets cancelled'", "focus-dialog '^Processes$'",
    "expect-undo-depth ${baseline} 1", "expect-model-bodies 1 4000", "mark")
menu("Presets of Bambu Studio…")
add("focus-dialog '^Import Presets$'", "expect-log 'Slicer presets row: printer Bench X, unticked, same name:'")
tick("material", "Amber")
import_presets(1)
add("expect-log 'Processes import: Fine imported'", "focus-dialog '^Processes$'",
    "expect-log 'Profile selected: process Fine \\(design\\)'", "expect-model-bodies 1 4000",
    "expect-undo-depth ${baseline} 2",
    "screenshot-dialog '${MITCAD_TEST_WORK}/slicer-preset-" + ("native" if native else "qt") + "-${MITCAD_TEST_COLOR_SCHEME}.png'")
press("Close")
# Save the imported profiles first, so undoing them changes the file on disk.
# Returning to the original saved baseline alone leaves native Save disabled.
add("focus-main", "mark", "menu 'File/Save'", "expect-log '^Saved '",
    "command edit.undo", "command edit.undo", "expect-undo-depth ${baseline} 0",
    "expect-model-bodies 1 4000", "mark", "menu 'File/Save'", "expect-log '^Saved '")

# The same production chooser into My library, the place chosen in the
# Processes manager's Import into box (mitcad#1135: My Library went).
add("command make.processes", "focus-dialog '^Processes$'")
if native:
    add("native-choice slicerImportInto 'My library'")
else:
    add("qt-choice slicerImportInto 'My library'")
add("expect-log 'Processes import into: My library'")
menu("Presets of Bambu Studio…")
add("focus-dialog '^Import Presets$'", "expect-log 'Slicer presets row: printer Bench X, ticked'")
tick("printer", "Bench X")
tick("material", "Amber")
import_presets(1)
add("expect-log 'Processes import: Fine imported into My library'", "focus-dialog '^Processes$'",
    "expect-log 'Profile selected: process Fine \\(user\\)'",
    "expect-undo-depth ${baseline} 1", "expect-model-bodies 1 4000",
    "screenshot-dialog '${MITCAD_TEST_WORK}/slicer-library-" + ("native" if native else "qt") + "-${MITCAD_TEST_COLOR_SCHEME}.png'")

# The filament preset is classified as a material: imported in the
# Processes manager, it is no process to show there (the Materials
# manager lists it).
add("mark")
menu("Presets of Bambu Studio…")
add("focus-dialog '^Import Presets$'", "expect-log 'Slicer presets row: process Fine, unticked, same name:'")
tick("printer", "Bench X")
import_presets(1)
add("focus-dialog '^Processes$'", "expect-log 'Processes import: Amber imported into My library'",
    "expect-undo-depth ${baseline} 2", "expect-model-bodies 1 4000")
capture("material")

# Two same-name presets make All same names genuinely mixed. Choosing a
# row's Override automatically ticks it; Copy imports Fine as Fine 2.
add("mark")
menu("Presets of Bambu Studio…")
add("focus-dialog '^Import Presets$'", "expect-log 'Slicer presets row: material Amber, unticked, same name:'",
    "expect-log 'Slicer presets all same names: copy'")
if not native:
    add("expect-widget-property slicerPresetsList focus true")
add("key Space",
    "expect-log 'Slicer presets ticked: 0'")
if native:
    add("expect-native-focus slicerPresetsList")
else:
    add("expect-widget-property slicerPresetsList focus true")
add("key Down", "key Down", "expect-log 'Slicer presets current: Fine'")
if native:
    add("expect-native-style slicerPreset.process.Fine current-row,unchecked,warning")
add("mark", "key Space",
    "expect-log 'Slicer presets tick: Fine ticked'", "mark", "key Space",
    "expect-log 'Slicer presets tick: Fine unticked'", "key Tab",
    "expect-log 'Slicer presets focus: Fine choice'")
if native:
    add("expect-native-text slicerPresetSameName.process.Fine '^Same name:$'",
        "expect-native-focus slicerPresetChoice.process.Fine",
        "expect-native-style slicerPreset.process.Fine current-row,unchecked,warning")
row_choice("process", "Fine", "Override")
add("expect-log 'Slicer presets choice: Fine override'", "expect-log 'Slicer presets tick: Fine ticked'",
    "expect-log 'Slicer presets all same names: none'",
    "expect-widget-property slicerPresetsSameName currentIndex -1")
if native:
    add("expect-native-text slicerPresetsSameName '^$'")
row_choice("process", "Fine", "Copy")
add("expect-log 'Slicer presets choice: Fine copy'", "expect-log 'Slicer presets all same names: copy'")
if native:
    add("expect-native-focus slicerPresetChoice.process.Fine")
add("key Shift+Tab")
if native:
    add("expect-native-focus slicerPresetsList",
        "expect-native-style slicerPreset.process.Fine current-row,checked,warning")
capture("copy-review")
add("key Tab", "key Tab")
if native:
    add("expect-native-focus slicerPresetsImport")
import_presets(1)
add("focus-dialog '^Processes$'", "expect-log 'Processes import: Fine imported into My library as Fine 2'",
    "expect-log 'Profile selected: process Fine 2 \\(user\\)'",
    "expect-undo-depth ${baseline} 3", "expect-model-bodies 1 4000")
press("Close")
add("focus-main")
# A manager in an open design records library file transactions on that
# document's undo stack, even when its geometry and profiles are unchanged.
# Undo each import, including its file, then redo all three so the wrapper
# can independently verify the imported values and the saved design.
for action, depth, counts in [
        ("undo", 2, (1, 1, 0)),
        ("undo", 1, (1, 0, 0)),
        ("undo", 0, (0, 0, 0)),
        ("redo", 1, (1, 0, 0)),
        ("redo", 2, (1, 1, 0)),
        ("redo", 3, (1, 1, 1))]:
    add("mark", f"command edit.{action}",
        f"expect-log '^{action.title()}: Import Presets of Bambu Studio$'",
        f"expect-undo-depth ${{baseline}} {depth}", "expect-model-bodies 1 4000")
    for path, count in zip(["print/processes/fine.json", "materials/amber.json",
                            "print/processes/fine-2.json"], counts):
        directory, name = path.rsplit("/", 1)
        add(f"expect-files '${{MITCAD_PRINT_LIBRARY_DIR}}/{directory}' '{name}' {count}")
add("expect-undo-depth ${baseline} 3", "expect-model-bodies 1 4000",
    "mark", "menu 'File/Save'", "expect-log '^Saved '",
    "expect-never 'The document was read while a job computes it|OCCT view event failed|Native Mac canvas[:]'")
print("\n".join(lines))
