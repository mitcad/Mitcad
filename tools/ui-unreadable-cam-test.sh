#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindow.cpp app/MainWindow.hpp core/model/src/file core/model/src/cam
# check-all sources: core/model-api/src/lib.rs tools/cli
# A design whose CAM section this MitCad cannot read (mitcad#2581; here two
# setups with one id) opens without its CAM instead of being refused.
#   1. mitcad-cli: opening says why the CAM was left out; a save over the
#      file says on stderr that it drops the CAM, and writes without asking.
#   2. Floating: opening warns with the reason (the message band; "Status
#      warning" in the log). Save (Ctrl+S) asks first: Cancel leaves the
#      file as it is; Save As writes a new file without the CAM and leaves
#      the old one as it is.
#   3. Docked: the same warning; Save asks, and Save and Drop Them writes
#      over the file without the CAM. Opened again, the design warns no more.
#
# Runs headless on Xvfb (see ui-test-lib.sh). The app uses Qt's own file
# dialog (--no-native-dialogs), so a path can be typed into it.
# Usage: tools/ui-unreadable-cam-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-unreadable-cam.XXXXXX)
FILE=$WORK/cam.mitcad
COPY=$WORK/copy.mitcad
CLI_FILE=$WORK/cli.mitcad
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# file_value file expression: a Python expression on the file's JSON (doc).
file_value() {
  python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' "$1" "$2"
}

# expect_file file expression expected description
expect_file() {
  local actual
  actual=$(file_value "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

REASON="two setups or operations have the same id"
# The warning on opening, as the status line logs it.
WARNING="Status warning: This design's CAM could not be read and was left out (machines, setups and operations). Two setups or operations have the same id. Saving over the design drops it."
HAS_CAM='"cam" in doc'

ui_start_display

echo "--- A design, and a CAM section this MitCad cannot read"
ui_start_app --demo --set d3=30 --no-native-dialogs
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "type the path, Enter"           ui_type_path "Save As" "$FILE"
ui_focus_main
ui_expect_log "Saved $FILE" "saved"
ui_stop_app
python3 -c '
import json, sys
path = sys.argv[1]
doc = json.load(open(path))
doc["cam"] = {"version": 1, "setups": [{"id": 1, "name": "Setup 1"}, {"id": 1, "name": "Setup 2"}]}
json.dump(doc, open(path, "w"), indent=2)
' "$FILE" || ui_fail "cannot add the CAM section"
expect_file "$FILE" "$HAS_CAM" "True" "the file has the CAM section"
cp "$FILE" "$CLI_FILE"

echo "--- 1. mitcad-cli: opening says why, a save over the file what it drops"
"$CLI" convert "$CLI_FILE" "$CLI_FILE" > "$WORK/cli.out" 2> "$WORK/cli.err" ||
  { cat "$WORK/cli.out" "$WORK/cli.err"; ui_fail "mitcad-cli convert"; }
grep -qF "warning: the design's CAM could not be read, so it opened without its machines, setups and operations: $REASON" \
  "$WORK/cli.err" || { cat "$WORK/cli.err"; ui_fail "mitcad-cli did not warn on opening"; }
echo "ok   opening warns with the reason"
grep -qF "warning: $CLI_FILE: saving over this design drops its CAM (machines, setups and operations), which this MitCad could not read" \
  "$WORK/cli.err" || { cat "$WORK/cli.err"; ui_fail "mitcad-cli did not warn before saving"; }
grep -qF "newer MitCad" "$WORK/cli.err" && { cat "$WORK/cli.err"; ui_fail "mitcad-cli says a newer MitCad wrote it"; }
echo "ok   saving warns that it drops the CAM"
expect_file "$CLI_FILE" "$HAS_CAM" "False" "written without asking, without the CAM"

echo "--- 2. Floating: the warning; Save asks, Cancel, then Save As"
ui_start_app --open "$FILE" --set d3=35 --no-native-dialogs --chrome=floating
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "CAM of $FILE not read: $REASON" "the reason in the log"
ui_expect_log "$WARNING" "the warning on opening"
before=$(sha256sum "$FILE")
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^MitCad$'
ui_expect_new "Save drops unknown fields: $FILE: cam" "Save asks first"
ui_step "cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Save drops unknown fields: cancelled" "cancelled"
[ "$(sha256sum "$FILE")" = "$before" ] || ui_fail "Cancel wrote the file"
echo "ok   Cancel keeps the file as it is"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^MitCad$'
ui_step "Save As (A)"                    ui_key a
ui_expect_new "Save drops unknown fields: save as" "Save As chosen"
ui_step "type the new path, Enter"       ui_type_path "Save As" "$COPY"
ui_focus_main
ui_expect_new "Saved $COPY" "saved to the new file"
[ "$(sha256sum "$FILE")" = "$before" ] || ui_fail "Save As wrote the file it was read from"
echo "ok   the file read is as it was"
expect_file "$COPY" "$HAS_CAM" "False" "the new file has no CAM"
expect_file "$COPY" '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[35.0]" "with the change"
ui_stop_app

echo "--- 3. Docked: the warning; Save and Drop Them"
ui_start_app --open "$FILE" --set d3=36 --no-native-dialogs
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "$WARNING" "the warning on opening"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^MitCad$'
ui_expect_new "Save drops unknown fields: $FILE: cam" "Save asks first"
ui_step "Save and Drop Them (S)"         ui_key s
ui_focus_main
ui_expect_new "Save drops unknown fields: save" "the CAM dropped"
ui_expect_new "Saved $FILE" "saved over the file"
expect_file "$FILE" "$HAS_CAM" "False" "the file has no CAM"
expect_file "$FILE" '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[36.0]" "with the change"
ui_stop_app

ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "opened again"
ui_sync
grep -qF "CAM of" "$UI_LOG" && ui_fail "the saved design still warns"
echo "ok   opened again, no warning"
ui_stop_app

ui_finish "UI unreadable CAM test"
