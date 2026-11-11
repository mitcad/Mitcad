#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/framework/DialogFolders.cpp app/framework/DialogFolders.hpp
# The file dialogs' folders per group (mitcad#498), through the File menu:
#   - Save As into one folder (the Open group), Export into another, Import
#     from a third: each dialog's first start is the design's folder.
#   - Import again starts in the import folder, Export in the export folder,
#     Open in the design's.
#   - The app killed and started again with no design: Import, Export and
#     Open each start in their own remembered folder, not in the default.
#
# Runs headless on Xvfb (see ui-test-lib.sh) with Qt's own file dialog
# (--no-native-dialogs). The app logs where a group's dialog starts
# ("Dialog folder import: <folder>") and the folder it remembers.
# Usage: tools/ui-dialog-folders-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-dialog-folders.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
DESIGNS=$WORK/designs
EXPORTS=$WORK/exports
IMPORTS=$WORK/imports
mkdir -p "$DESIGNS" "$EXPORTS" "$IMPORTS"

# file_menu key: the File menu (Alt+F), then an entry's underlined letter.
file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}

# cancel_dialog title: the dialog closed with Escape, the keyboard back.
cancel_dialog() {
  ui_focus_dialog "$(ui_title_regex "$1")"
  ui_key Escape
  ui_focus_main
}

ui_start_display

echo "--- Save As, Export and Import into folders of their own"
ui_start_app --demo --no-native-dialogs
ui_mark
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
ui_step "type the path, Enter"             ui_type_path "Save As" "$DESIGNS/block.mitcad"
ui_focus_main
ui_expect_new "Saved $DESIGNS/block.mitcad" "saved through the dialog"
ui_expect_new "Dialog folder open remembered: $DESIGNS" "the Open group remembers the design's folder"

ui_mark
ui_step "File > Export"                    file_menu e
ui_expect_new "Dialog folder export: $DESIGNS" "Export starts in the design's folder at first"
ui_step "type the path, Enter"             ui_type_path "Export" "$EXPORTS/block.step"
ui_focus_main
ui_expect_new "Exported $EXPORTS/block.step: step, 1 body(ies)" "exported"
ui_expect_new "Dialog folder export remembered: $EXPORTS" "the Export group remembers its folder"

cp "$EXPORTS/block.step" "$IMPORTS/part.step"
ui_mark
ui_step "File > Import"                    file_menu i
ui_expect_new "Dialog folder import: $DESIGNS" "Import starts in the design's folder at first"
ui_step "type the path, Enter"             ui_type_path "Import" "$IMPORTS/part.step"
# The block is content: asked where the file goes (mitcad#1629).
ui_expect_new "Import place dialog opened: part.step" "asked where the file goes"
ui_focus_dialog '^Where to Import$'
ui_step "into a new component (Enter)"     ui_key Return
ui_focus_main
ui_expect_new "Imported part.step as" "imported"
ui_expect_new "Dialog folder import remembered: $IMPORTS" "the Import group remembers its folder"
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Import part.step" "undone: the design as saved"

echo "--- Each group starts in its own folder"
ui_mark
ui_step "File > Import again"              file_menu i
ui_expect_new "Dialog folder import: $IMPORTS" "Import starts in the import folder"
ui_step "cancel it (Escape)"               cancel_dialog "Import"
ui_mark
ui_step "File > Export again"              file_menu e
ui_expect_new "Dialog folder export: $EXPORTS" "Export starts in the export folder, with a design open"
ui_step "cancel it (Escape)"               cancel_dialog "Export"
ui_mark
ui_step "open (Ctrl+O)"                    ui_key ctrl+o
ui_expect_new "Dialog folder open: $DESIGNS" "Open keeps its own folder"
ui_step "cancel it (Escape)"               cancel_dialog "Open"
ui_mark
ui_step "File > Import, cancelled"         file_menu i
ui_step "cancel it (Escape)"               cancel_dialog "Import"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Dialog folder import remembered"; then
  ui_fail "a cancelled dialog remembered a folder"
fi
echo "ok   a cancelled dialog remembers nothing"

echo "--- After a restart, with no design"
ui_stop_app
ui_start_app --no-native-dialogs
ui_mark
ui_step "File > Import"                    file_menu i
ui_expect_new "Dialog folder import: $IMPORTS" "Import still starts in the import folder"
ui_step "cancel it (Escape)"               cancel_dialog "Import"
ui_mark
ui_step "File > Export"                    file_menu e
ui_expect_new "Dialog folder export: $EXPORTS" "Export still starts in the export folder"
ui_step "cancel it (Escape)"               cancel_dialog "Export"
ui_mark
ui_step "open (Ctrl+O)"                    ui_key ctrl+o
ui_expect_new "Dialog folder open: $DESIGNS" "Open still starts in the design's folder"
ui_step "cancel it (Escape)"               cancel_dialog "Open"

echo "--- A folder that is gone"
rm -rf "$IMPORTS"
ui_mark
ui_step "File > Import"                    file_menu i
ui_expect_new "Dialog folder import: " "Import logs where it starts"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Dialog folder import: $IMPORTS"; then
  ui_fail "Import starts in a folder that is gone"
fi
echo "ok   Import starts in the default folder"
ui_step "cancel it (Escape)"               cancel_dialog "Import"

ui_finish "UI dialog folders test"
