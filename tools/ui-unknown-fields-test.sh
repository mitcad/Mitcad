#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/MainWindow.cpp app/MainWindow.hpp core/model/src/file core/model/src/document.rs
# check-all sources: core/model-api/src/lib.rs tools/cli
# A design a newer MitCad wrote (mitcad#1132): its top-level fields this
# MitCad does not know (here `comments` and `next_comment`) are ignored.
#   1. mitcad-cli: opening says they were not read; a save over the file
#      says on stderr that it drops them, and writes without asking.
#   2. Floating: opening warns, naming them (the message band; "Status
#      warning" in the log). Save (Ctrl+S) asks first: Cancel leaves the
#      file as it is; Save As writes a new file without them and leaves
#      the old one as it is.
#   3. Docked: the same warning; Save asks, and Save and Drop Them writes
#      over the file without them. Opened again, the design warns no more.
#
# Runs headless on Xvfb (see ui-test-lib.sh). The app uses Qt's own file
# dialog (--no-native-dialogs), so a path can be typed into it.
# Usage: tools/ui-unknown-fields-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-unknown-fields.XXXXXX)
FILE=$WORK/newer.mitcad
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

# The warning on opening, as the status line logs it.
WARNING="Status warning: This design was written by a newer MitCad: comments, next_comment not read. Saving over it drops them."
FIELDS='"comments" in doc, "next_comment" in doc'

ui_start_display

echo "--- A design, and what a newer MitCad adds to it"
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
doc["comments"] = [{"id": "N1", "text": "Check the wall"}]
doc["next_comment"] = 2
json.dump(doc, open(path, "w"), indent=2)
' "$FILE" || ui_fail "cannot add the fields"
expect_file "$FILE" "$FIELDS" "(True, True)" "the file has the newer fields"
cp "$FILE" "$CLI_FILE"

echo "--- 1. mitcad-cli: a save over the file says what it drops"
"$CLI" convert "$CLI_FILE" "$CLI_FILE" > "$WORK/cli.out" 2> "$WORK/cli.err" ||
  { cat "$WORK/cli.out" "$WORK/cli.err"; ui_fail "mitcad-cli convert"; }
grep -qF "warning: this design was written by a newer MitCad: the fields \`comments\`, \`next_comment\` were not read" \
  "$WORK/cli.err" || { cat "$WORK/cli.err"; ui_fail "mitcad-cli did not warn on opening"; }
echo "ok   opening warns, naming the fields"
grep -qF "warning: $CLI_FILE: saving over this design drops the fields \`comments\`, \`next_comment\`" \
  "$WORK/cli.err" || { cat "$WORK/cli.err"; ui_fail "mitcad-cli did not warn before saving"; }
echo "ok   saving warns, naming the fields"
expect_file "$CLI_FILE" "$FIELDS" "(False, False)" "written without asking, without the fields"

echo "--- 2. Floating: the warning; Save asks, Cancel, then Save As"
ui_start_app --open "$FILE" --set d3=35 --no-native-dialogs --chrome=floating
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "Unknown fields of $FILE not read: comments, next_comment" "the fields named in the log"
ui_expect_log "$WARNING" "the warning on opening"
before=$(sha256sum "$FILE")
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^MitCad$'
ui_expect_new "Save drops unknown fields: $FILE: comments, next_comment" "Save asks first"
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
expect_file "$COPY" "$FIELDS" "(False, False)" "the new file has no fields this MitCad did not read"
expect_file "$COPY" '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[35.0]" "with the change"
ui_stop_app

echo "--- 3. Docked: the warning; Save and Drop Them"
ui_start_app --open "$FILE" --set d3=36 --no-native-dialogs
ui_expect_log "Opened $FILE" "opened"
ui_expect_log "$WARNING" "the warning on opening"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_focus_dialog '^MitCad$'
ui_expect_new "Save drops unknown fields: $FILE: comments, next_comment" "Save asks first"
ui_step "Save and Drop Them (S)"         ui_key s
ui_focus_main
ui_expect_new "Save drops unknown fields: save" "the fields dropped"
ui_expect_new "Saved $FILE" "saved over the file"
expect_file "$FILE" "$FIELDS" "(False, False)" "the file has no fields this MitCad did not read"
expect_file "$FILE" '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[36.0]" "with the change"
ui_stop_app

ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "Opened $FILE" "opened again"
ui_sync
grep -qF "Unknown fields of" "$UI_LOG" && ui_fail "the saved design still warns"
echo "ok   opened again, no warning"
ui_stop_app

ui_finish "UI unknown fields test"
