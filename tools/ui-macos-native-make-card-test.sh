#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeDialogs.mm app/platform/macos/NativePicker.mm app/framework/Dialogs.cpp
# check-all sources: app/framework/PickerComboBox.cpp app/browser/LibraryPicker.cpp app/print/MakeBodyCard.cpp
# check-all sources: app/MainWindowMake.cpp app/print/PrintValuePicker.cpp app/browser/PropertySetsDialog.cpp
# check-all sources: app/print/PrintLibrary.cpp app/print/ProfilePickerRows.cpp app/MainWindowNative.cpp
# check-all sources: app/framework/TestDriver.cpp app/platform/macos/NativeTestInput.mm
# check-all sources: tools/ui-scripts/native-macos-make-card.mitcad-ui tools/cli
# The Make card and Property Sets' process list in the AppKit frontend
# (mitcad#1353), on synthetic design profiles: Property Sets' process
# pop-up shows its printer headings as bold items that cannot be chosen,
# and a process is still chosen from it; on the Make tab the selected
# body's card opens the searchable Printer picker (the native popover with
# its vendor headings) and chooses from it, and Add Print Value… opens as
# an AppKit sheet on the window and adds Walls to the body
# (tools/ui-scripts/native-macos-make-card.mitcad-ui). Input is the
# in-process native input only, no system events.
# Usage: tools/ui-macos-native-make-card-test.sh <app> <mitcad-cli> <output directory>
# Requires MITCAD_MACOS_NATIVE_UI=ON and a logged-in macOS window server.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" -ne 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> <output directory>" >&2
  exit 2
fi
cli=$2
ui_init "$1" "$3"
UI_TIMEOUT=240
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
# A box; printers of one vendor (Acme Lab) and another; a process for one
# of them and two for any printer, the design's process Standard (Walls 2).
cat > "$MITCAD_TEST_WORK/setup.json" <<'JSON'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":20,"width":20,"height":10,"operation":"new_body"}},
  {"cmd":"create_print_profile","kind":"printer","name":"Acme Lab X1 Carbon 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"printer","name":"Acme Lab P1S 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench Mini 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Fine @ Acme Lab X1 Carbon 0.4 nozzle","values":{"layer_height":0.12}},
  {"cmd":"create_print_profile","kind":"process","name":"Standard","values":{"perimeters":2}},
  {"cmd":"create_print_profile","kind":"process","name":"Draft","values":{"layer_height":0.28}},
  {"cmd":"set_property","owner":"C0","set":"Print Process","key":"profile","value":"standard"}
]
JSON
"$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/make-card.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-make-card.mitcad-ui \
  ui_run_app native-make-card --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/make-card.mitcad"
ui_expect_log '^Native choice native\.properties\.C0\.Print Process\.profile\.value: .*\(heading\)' \
  "Property Sets' process pop-up shows its printer headings as items that cannot be chosen"
ui_expect_log '^Native picker libraryPicker at makeCardPrinter: ' \
  "the Make card's Printer opens the searchable picker under its field"
ui_expect_log '^Native sheet shown: Add Print Value \(printValuePicker\)$' 'Add Print Value… is an AppKit sheet'
ui_expect_log '^Make card Walls added: 2$' 'Add Print Value… added Walls to the body'
ui_expect_log '^TestDriver: done' 'the Make card and Property Sets in the AppKit frontend'
ui_expect_no_log 'TestDriver: FAIL' 'no failed native assertions'
if [ "$UI_FAILURES" -gt 0 ]; then ui_log_tail; exit 1; fi
echo 'PASS: AppKit Make card pickers, Add Print Value sheet and Property Sets headings'
