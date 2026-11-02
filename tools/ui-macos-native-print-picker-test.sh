#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/platform/macos/NativeForms app/platform/macos/NativePicker app/browser/LibraryPicker app/print/ProfilePickerRows
# Native Printer/Process search, keyboard, pointer, current choice and undo
# (mitcad#1122), and Edit printers and processes… as the pickers' last row with no
# button beside Printer or Process (mitcad#1134), using only synthetic design profiles.
set -euo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
ui_init "$1" "$3"
cli=$2
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_PRINT_LIBRARY_DIR=$UI_OUT/library
mkdir -p "$MITCAD_TEST_WORK" "$MITCAD_PRINT_LIBRARY_DIR" "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '[print]\nslicerName=Fixture slicer\nslicerProgram=/usr/bin/true\nformat=3mf\n' \
  > "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
cat > "$MITCAD_TEST_WORK/setup.json" <<'JSON'
[
  {"cmd":"add_feature","def":{"type":"box","plane":"xy","corner":[0,0],"length":20,"width":20,"height":10,"operation":"new_body"}},
  {"cmd":"create_print_profile","kind":"printer","name":"Acme Lab X1 Carbon 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"printer","name":"Acme Lab P1S 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench Mini 0.4 nozzle","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Fine @ Acme Lab X1 Carbon 0.4 nozzle","values":{"layer_height":0.12}}
]
JSON
"$cli" run "$MITCAD_TEST_WORK/setup.json" --save "$MITCAD_TEST_WORK/model.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_INPUT=$here/ui-scripts/native-macos-print-picker.mitcad-ui \
  ui_run_app picker --ui=native --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/model.mitcad"
ui_expect_log 'TestDriver: done' 'native profile search and undo completed'
ui_expect_no_log 'TestDriver: FAIL' 'no failed native assertions'
[ "$UI_FAILURES" = 0 ] || { ui_log_tail; exit 1; }
echo 'PASS: native Printer and Process searchable popovers'
