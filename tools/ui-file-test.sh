#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Project files through the real UI: Save As with a typed path, reopen with
# --open, Save (Ctrl+S) after a parameter change, the title's "*" through
# undo and redo, the unsaved-changes prompt on New and on exit, and Open
# through the file dialog.
#
# Runs headless on Xvfb (see ui-test-lib.sh). The app uses Qt's own file
# dialog (--no-native-dialogs), so a path can be typed into it.
# Usage: tools/ui-file-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-file.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# file_value expression: evaluates a Python expression on the saved JSON (doc).
file_value() {
  python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1"
}

# expect_file expression expected description
expect_file() {
  local actual
  actual=$(file_value "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(xdotool getwindowname "$UI_WINDOW" 2> /dev/null)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

# focus_dialog name-regex: waits for a dialog and gives it the keyboard.
focus_dialog() {
  local dialog=""
  for _ in $(seq 1 50); do
    dialog=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
    [ -n "$dialog" ] && break
    sleep 0.2
  done
  [ -n "$dialog" ] || ui_fail "no dialog matching '$1'"
  ui_sync "$dialog"
  xdotool windowfocus --sync "$dialog" 2> /dev/null
  echo "ok   dialog '$1' open"
}

# Xvfb has no window manager to give the keyboard back after a dialog.
focus_main() {
  ui_sync
  xdotool windowfocus --sync "$UI_WINDOW" 2> /dev/null
}

# type_path path: replaces the file name in Qt's file dialog and accepts.
type_path() {
  ui_key ctrl+a
  xdotool type --delay 20 "$1"
  sleep 0.5
  ui_key Return
  focus_main
}

# expect_exit description: waits for the app to quit with exit code 0.
expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  if command -v gdb > /dev/null; then
    grep -q "exited normally" "$UI_LOG" || ui_fail "$1: abnormal exit"
  else
    wait "$UI_RUNNER" || ui_fail "$1: exit code $?"
  fi
  UI_RUNNER=""
  echo "ok   $1"
}

ui_start_display

echo "--- Save As"
ui_start_app --demo --no-native-dialogs
expect_title "Untitled - Mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_step "fillet (F)"                     ui_key f
ui_step "pick a face of the body"        ui_view_click 50 50
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added fillet on 4 edge(s)" "fillet on the face's edges"
expect_title "Untitled* - Mitcad"
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
focus_dialog '^Save As$'
ui_step "type the path, Enter"           type_path "$FILE"
ui_expect_log "Saved $FILE" "saved to the typed path"
expect_title "block.mitcad - Mitcad"
expect_file 'doc["format"], doc["version"]' "('mitcad', 2)" "format marker and version"
expect_file '[f["type"] for f in doc["features"]]' "['sketch', 'extrude', 'fillet']" \
  "history in the file"
# A picked face stays a face in the definition (face selection).
expect_file 'doc["features"][2]["body"], doc["features"][2]["sets"][0]["faces"]' \
  "('F2.b0', ['F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})'])" \
  "fillet refers to its body and the top face"

echo "--- Reopen with --open, change a parameter, Save"
ui_stop_app
ui_start_app --open "$FILE" --set d3=35 --no-native-dialogs
ui_expect_log "Opened $FILE" "opened with --open"
ui_expect_log "Recomputed 3 feature(s)" "sketch, extrude and fillet recomputed"
ui_expect_log "Recomputed 2 feature(s)" "d3 change recomputed extrude and fillet"
expect_title "block.mitcad* - Mitcad"
# The "*" follows the model's revisions: undo back to the state opened
# takes it away, redo brings it back.
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
expect_title "block.mitcad - Mitcad"
ui_step "redo it (Ctrl+Y)"               ui_key ctrl+y
expect_title "block.mitcad* - Mitcad"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_log "Saved $FILE" "saved without a dialog"
expect_title "block.mitcad - Mitcad"
expect_file '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[35.0]" \
  "changed parameter saved"

echo "--- Unsaved changes: Cancel on New, Save on exit"
# Undo takes back the last command of this session: the change of d3.
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
expect_title "block.mitcad* - Mitcad"
ui_step "redo: the state saved (Ctrl+Y)" ui_key ctrl+y
expect_title "block.mitcad - Mitcad"
ui_step "undo again (Ctrl+Z)"            ui_key ctrl+z
expect_title "block.mitcad* - Mitcad"
ui_step "new (Ctrl+N)"                   ui_key ctrl+n
focus_dialog '^Mitcad$'
ui_step "cancel (Esc)"                   ui_key Escape
focus_main
expect_title "block.mitcad* - Mitcad"
grep -q "New document" "$UI_LOG" && ui_fail "New went ahead after Cancel"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^Mitcad$'
ui_key Return # Save, the default button
expect_exit "saved and exited"
grep -qF "Saved $FILE" "$UI_LOG" || ui_fail "not saved on exit"
expect_file '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[20.0]" \
  "undo saved on exit"

echo "--- Open dialog, Don't Save on New"
ui_start_app --no-native-dialogs
expect_title "Untitled - Mitcad"
ui_step "open (Ctrl+O)"                  ui_key ctrl+o
focus_dialog '^Open$'
ui_step "type the path, Enter"           type_path "$FILE"
ui_expect_log "Opened $FILE" "opened through the dialog"
ui_expect_log "Recomputed 3 feature(s)" "sketch, extrude and fillet recomputed"
expect_title "block.mitcad - Mitcad"
checksum=$(sha256sum "$FILE")
ui_step "create a sketch on XY"          ui_create_sketch xy
expect_title "block.mitcad* - Mitcad"
ui_step "new (Ctrl+N)"                   ui_key ctrl+n
focus_dialog '^Mitcad$'
ui_step "don't save (D)"                 ui_key d
focus_main
ui_expect_log "New document" "new document after Don't Save"
expect_title "Untitled - Mitcad"
[ "$(sha256sum "$FILE")" = "$checksum" ] || ui_fail "the file changed after Don't Save"
echo "ok   file unchanged"
ui_key ctrl+q
expect_exit "unmodified document exits without a prompt"

echo "--- Join and Cut: open a version 1 file, recompute, save as version 2"
FILE=$WORK/boss.mitcad
cat > "$FILE" << 'EOF'
{
  "format": "mitcad",
  "version": 1,
  "parameters": [
    { "name": "d1", "value": 60.0 }, { "name": "d2", "value": 40.0 },
    { "name": "d3", "value": 20.0 }, { "name": "d4", "value": 20.0 },
    { "name": "d5", "value": 10.0 }, { "name": "d6", "value": 10.0 },
    { "name": "d7", "value": 30.0 }, { "name": "d8", "value": 30.0 },
    { "name": "d9", "value": 2.0 }
  ],
  "features": [
    { "type": "sketch", "name": "Sketch1", "shapes": [
      { "type": "rectangle", "corner": [0.0, 0.0], "width": "d1", "height": "d2" } ] },
    { "type": "extrude", "name": "Extrude1", "sketch": "Sketch1", "profile": 0,
      "distance": "d3", "operation": "new_body" },
    { "type": "sketch", "name": "Sketch2", "shapes": [
      { "type": "rectangle", "corner": [10.0, 10.0], "width": "d4", "height": "d5" },
      { "type": "circle", "center": [45.0, 20.0], "diameter": "d6" } ] },
    { "type": "extrude", "name": "Extrude2", "sketch": "Sketch2", "profile": 0,
      "distance": "d7", "operation": "join", "body": "Extrude1" },
    { "type": "extrude", "name": "Extrude3", "sketch": "Sketch2", "profile": 1,
      "distance": "d8", "operation": "cut", "body": "Extrude1" },
    { "type": "fillet", "name": "Fillet1", "body": "Extrude1", "edges": [
      { "extrude": "Extrude1", "role": "side", "index": 1 },
      { "extrude": "Extrude2", "role": "side", "index": 1 } ], "radius": "d9" }
  ]
}
EOF
ui_start_app --open "$FILE"
ui_expect_log "Opened $FILE" "opened a part with a join and a cut"
ui_expect_log "Recomputed 6 feature(s)" "join, cut and fillet recomputed"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
expect_title "boss.mitcad - Mitcad"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_log "Saved $FILE" "saved again"
expect_file 'doc["version"]' "2" "saved as version 2"
expect_file '[(f.get("operation"), f.get("participants")) for f in doc["features"] if f["type"] == "extrude"]' \
  "[('new_body', None), ('join', ['F2.b0']), ('cut', ['F2.b0'])]" "operations saved"
expect_file 'doc["features"][5]["edges"]' \
  "['E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}', 'E{F4:side(c1[c4,c2])|F4:side(c2[c1,c3])}']" \
  "fillet edges converted to edge names"
ui_stop_app

echo "--- Degenerate dimension"
# A hand-edited sketch profile too small to build must not crash the app.
python3 - "$FILE" "$WORK/tiny.mitcad" << 'EOF'
import json, sys
doc = json.load(open(sys.argv[1]))
doc["features"] = doc["features"][:1]
doc["parameters"] = doc["parameters"][:2]
doc["parameters"][0]["expression"] = "1e-9 mm"
doc.pop("bodies", None)
json.dump(doc, open(sys.argv[2], "w"))
EOF
ui_start_app --open "$WORK/tiny.mitcad"
ui_expect_log "Opened $WORK/tiny.mitcad" "opened a sketch 1e-9 mm wide"
ui_step "fit (F6)"                       ui_key F6
ui_stop_app

echo "--- Invalid file"
printf '{"format": "mitcad", "version": 1,' > "$WORK/broken.mitcad"
"$UI_APP" --open "$WORK/broken.mitcad" > "$WORK/broken.log" 2>&1
status=$?
[ "$status" = 2 ] || { cat "$WORK/broken.log"; ui_fail "--open of a broken file exited with $status"; }
grep -q "Could not open.*not valid JSON" "$WORK/broken.log" ||
  { cat "$WORK/broken.log"; ui_fail "no clear error for a broken file"; }
echo "ok   broken file rejected with a clear message"

ui_finish "UI file test"
