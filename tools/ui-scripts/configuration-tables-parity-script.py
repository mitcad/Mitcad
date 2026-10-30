#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Exercise table-backed configuration UI through each frontend's controls."""
import sys

native = sys.argv[1] == "native"
frontend = "native" if native else "qt"
lines = ["# SPDX-License-Identifier: MIT", "wait-idle", "window-resize 1280 820",
         "expect-model-bodies 1 1600", "expect-model-parameter HeadDia 7", "capture-undo-depth baseline"]


def add(*steps):
    lines.extend(steps)


def press(label, identifier=None):
    plain = label.rstrip(".\u2026")
    add(f"native-pointer-click '{identifier}'" if native and identifier else
        f"native-pointer-button '^{label}$'" if native else f"button '^{plain}$'" + (f" {identifier}" if identifier else ""))


def page(title):
    add(f"native-choice configurationsTabs '{title}'" if native else
        f"dialog-click-logged 'Configurations tab {title}'",
        f"expect-log 'Configurations page {title}'")


def choice(identifier, title, index):
    if native:
        add(f"native-choice '{identifier}' '{title}'")
    else:
        add(f"focus-widget '{identifier}'", "key Space", "key Home")
        add(*(["key Down"] * index), "key Return")


def columns():
    add("native-pointer-click 'rulePiece_then.0 columns'" if native else
        "dialog-click-logged 'Rule canvas then.0 columns'",
        "focus-dialog '^Table Columns$'", "expect-log 'Rule canvas columns dialog for then.0 of Screws'")
    if native:
        add("expect-native-visible ruleTableColumnLength true", "expect-native-visible ruleTableColumnSize true")


if native:
    add("expect-ever 'Native UI: AppKit shell with embedded OCCT viewport'",
        "native-appearance '${MITCAD_TEST_COLOR_SCHEME}'")
add("command solid.configurations", "focus-dialog '^Configurations$'")

# Text and number domains choose real columns, with kinds filtered by the UI.
for option, title, index in [("Size", "Screws.Size [Text]", 0), ("Length", "Screws.Length [mm]", 1)]:
    add(f"native-pointer-click configurationOption.{option}" if native else
        f"dialog-click-logged 'Configurations option {option} option'")
    if native:
        add("expect-native-enabled configurationsEditDomain true")
    add("mark")
    press("Edit Domain…", "configurationsEditDomain")
    add("focus-dialog '^Option Domain$'", f"expect-log 'Option Domain dialog opened for {option}'")
    if native:
        add("native-pointer-click domainColumn")
    else:
        press("Table Column")
    choice("domainColumnChoice", title, index)
    press("OK", "domainOK")
    add(f"expect-log '^Option {option} domain: column Screws.{option}$'", "focus-dialog '^Configurations$'",
        "wait-idle", "expect-model-bodies 1 1600")
add("expect-undo-depth ${baseline} 2")

# Cancel leaves all model state intact; Generate creates three named rows.
page("Rows")
add("capture-undo-depth beforeRows", "mark")
press("Generate Rows from Table…", "configurationsGenerateRows")
add("focus-dialog '^Generate Rows from Table$'", "expect-log 'Generate Rows dialog opened: Screws.Size for Size'")
choice("generateRowsTable", "Screws", 0)
choice("generateRowsColumn", "Size [Text]", 0)
choice("generateRowsOption", "Size", 0)
press("Cancel")
add("focus-dialog '^Configurations$'", "expect-undo-depth ${beforeRows} 0", "mark")
press("Generate Rows from Table…", "configurationsGenerateRows")
add("focus-dialog '^Generate Rows from Table$'")
press("Generate", "generateRowsOK")
add("expect-log 'Configurations generated rows from Screws.Size for Size: 3 rows from Screws: added M3, M4, M5'",
    "expect-log 'M3: Size = M3, Length = ; M4: Size = M4, Length = ; M5: Size = M5, Length = '",
    "focus-dialog '^Configurations$'", "expect-undo-depth ${beforeRows} 1")

# An empty, valid rule is part of the fixture. Insert a real palette block,
# complete its table name, then test both cancel and an actual column edit.
page("Rules")
add("native-select 'rules.Made sizes'" if native else "focus-widget rulesList")
if not native:
    add("key Home")
add("native-pointer-click 'rulePiece_then.0 slot'" if native else
    "dialog-click-logged 'Rule canvas then.0 slot'", "key Return" if native else "focus-widget rulePalette",
    "key Home", "type combination", "key Return",
    "expect-log 'Rule edited Made sizes: Put combination in table in then.0'",
    "expect-log 'Rule canvas editing then.0 table:'")
if native:
    add("expect-native-visible ruleBlockEditor true")
else:
    add("focus-widget ruleBlockEditor")
# Completion follows keyboard edits, as in both real field editors.
add("type Scr")
if native:
    add("expect-native-text ruleBlockEditor '^Screws$'")
add("key Return", "expect-log 'Rule canvas columns of Screws: Length=Length, Size=Size'",
    "expect-log 'Rules saved: Set table of then.0 to Screws'", "capture-undo-depth beforeColumns", "mark")
columns()
choice("ruleTableColumnLength", "(none)", 0)
press("Cancel", "ruleTableColumnsCancel")
add("focus-dialog '^Configurations$'", "expect-undo-depth ${beforeColumns} 0",
    "expect-no-log '^Rules saved: Set columns'")
columns()
choice("ruleTableColumnLength", "(none)", 0)
press("OK", "ruleTableColumnsOK")
add("expect-log '^Rules saved: Set columns of then.0 to Size=Size$'", "focus-dialog '^Configurations$'",
    "expect-undo-depth ${beforeColumns} 1")
columns()
choice("ruleTableColumnLength", "Length [mm]", 3)
press("OK", "ruleTableColumnsOK")
add("expect-log '^Rules saved: Set columns of then.0 to Length=Length, Size=Size$'",
    "focus-dialog '^Configurations$'", "expect-undo-depth ${beforeColumns} 2")
# The rule's text is always shown under the blocks (mitcad#965).
add("expect-log 'Rule as text Made sizes: always / then combination in table Screws \\(Length = Length, Size = Size\\)'",
    f"screenshot-dialog '${{MITCAD_TEST_WORK}}/table-rule-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")

# Applying the generated M5 row drives the other option and the body.
page("Rows")
add("native-select configurationRow.M5" if native else "dialog-click-logged 'Configurations row M5 Name'",
    "capture-undo-depth beforeApply", "mark")
press("Apply", "configurationsApply")
add("expect-log '^Configurations applied M5'", "expect-log 'Configuration moves: Length 16 mm → 20 mm: Made sizes'",
    "wait-idle", "expect-model-bodies 1 2000", "expect-model-parameter HeadDia 8.5",
    "expect-undo-depth ${beforeApply} 1")
# A saved row referring to M5 must not survive removal of M5 from its
# column domain. Remove that row through the real UI first; its current
# parameter choice stays M5, so CSV import still has to move that choice.
# Undo this row removal separately after verifying the import's own undo.
add("capture-undo-depth beforeRowRepair",
    "native-select configurationRow.M5" if native else "dialog-click-logged 'Configurations row M5 Name'")
press("Delete Row", "configurationsDeleteRow")
add("expect-log '^Configurations deleted row: M5$'", "wait-idle",
    "expect-model-bodies 1 2000", "expect-undo-depth ${beforeRowRepair} 1")
press("Close")
add("focus-main", "command solid.parameters", "focus-dialog '^Parameters$'")
add("native-choice parametersTabs Tables" if native else "dialog-click-logged 'Parameters tab Tables'")
add("capture-undo-depth beforeImport", "mark")


def import_csv():
    press("Import CSV…", "tablesImport")
    add("focus-dialog '^Import Table from CSV$'")
    if native:
        add("native-file-directory '${MITCAD_TEST_WORK}'", "native-file-state directory-set", "native-system-key Ctrl+Shift+g", "sleep 400",
        "native-file-state goto-open",
            "native-system-key Ctrl+a", "native-system-type '${MITCAD_TEST_WORK}/Screws.csv'", "sleep 300", "native-file-state path-entered", "native-system-key Return", "sleep 500",
            "native-file-state goto-accepted", "native-system-key Return", "sleep 300", "native-file-state open-accepted")
    else:
        add("focus-widget fileNameEdit", "type '${MITCAD_TEST_WORK}/Screws.csv'", "key Return")
    add("focus-dialog '^Import Table$'", "expect-log 'Table import preview of Screws.csv as Screws:'",
        "expect-log 'Table import changes Size: M5 -> M3'",
        "expect-log 'Table import changes Length: 20 mm -> 16 mm'",
        "expect-undo-depth ${beforeImport} 0")
    if native:
        add("expect-native-visible tableImportChanges true", "expect-native-visible tableImportAccept true")
    press("Import", "tableImportAccept")
    add("focus-dialog '^Parameters$'", "wait-idle")


# Removed M5 falls back to M3, and the same import moves Length too.
import_csv()
add("expect-log 'Imported table Screws from Screws.csv:'",
    "expect-log 'Configuration moves: .*Size M5 → M3: its allowed values Screws.Size'",
    "expect-model-bodies 1 1600", "expect-model-parameter Length 16", "expect-model-parameter HeadDia 5.5", "expect-undo-depth ${beforeImport} 1",
    f"screenshot-dialog '${{MITCAD_TEST_WORK}}/table-csv-{frontend}-${{MITCAD_TEST_COLOR_SCHEME}}.png'")
press("OK")
add("focus-main", "command edit.undo", "wait-idle", "expect-model-bodies 1 2000",
    "expect-model-parameter HeadDia 8.5", "expect-undo-depth ${beforeImport} 0",
    "command edit.undo", "wait-idle", "expect-undo-depth ${beforeRowRepair} 0",
    "expect-model-bodies 1 2000", "expect-model-parameter HeadDia 8.5", "menu 'File/Save'",
    "expect-log '^Saved '", "expect-never 'Recompute failed|Native Mac canvas[:]|OCCT view event failed'")
print("\n".join(lines))
