#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/MainWindow.cpp
# Project files through the real UI: Save As with a typed path, reopen with
# --open, Save (Ctrl+S) after a parameter change, the title's "*" through
# undo and redo, the unsaved-changes prompt on New Design (an untitled
# design outside projects) and on exit, and Open through the file dialog.
# The File menu's entries in their groups (mitcad#175). Every new design
# starts with Component1 active (mitcad#898): the untitled design at the
# start, after New and after Close; a design read from a file opens with
# the component it was saved with active (mitcad#1678).
#
# Runs headless on Xvfb (see ui-test-lib.sh). The app uses Qt's own file
# dialog (--no-native-dialogs), so a path can be typed into it.
# Usage: tools/ui-file-test.sh
# check-all sources: core/vcs core/ffi/src/vcs.rs core/ffi/src/remote.rs core/ffi/src/diff.rs

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

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
    title=$(ui_window_title)
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

# type_path title path: the path typed into Qt's file dialog (ui_type_path), accepted.
type_path() {
  ui_type_path "$1" "$2"
  focus_main
}

# expect_exit description: waits for the app to quit with exit code 0.
expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  if command -v gdb > /dev/null && [ "${UI_GDB:-0}" = 1 ]; then
    grep -q "exited normally" "$UI_LOG" || ui_fail "$1: abnormal exit"
  else
    wait "$UI_RUNNER" || ui_fail "$1: exit code $?"
  fi
  UI_RUNNER=""
  echo "ok   $1"
}

ui_start_display
# Parts built over the origin, beside which an origin plane is clicked:
# the origin as 100 mm Centered squares (ui_origin_in_millimeters,
# mitcad#1748).
ui_origin_in_millimeters

echo "--- The File menu (mitcad#175)"
ui_start_app --demo --no-native-dialogs
expect_title "Untitled - MitCad"
# New and New Design are one entry; 3D Print, Insert Component and Render
# Image are on the ribbon only; the project commands in a submenu.
project_menu="New Project | Open Project | Open from Cloud | - | Sync | Check for Newer Versions | Open in Browser"
project_menu="$project_menu | - | Project Settings | - | Web Viewer | Update Web Viewer"
file_menu="New Design | Open | Open from Project | Open Recent > | Open Read-Only | Close | - | Save | Save As"
file_menu="$file_menu | Save to Project | Save with Description"
file_menu="$file_menu | Version History | Design Properties | - | Project > [$project_menu] | - | Import | Export | Create Drawing | Export Drawing | - | Print | - | Recover Designs"
file_menu="$file_menu | - | Exit"
ui_mark
ui_step "file menu (Alt+F)"              ui_key alt+f
ui_expect_new "File menu: $file_menu" "the File menu's groups and the Project submenu"
ui_step "close it (Escape)"              ui_key Escape
focus_main

echo "--- Save As"
ui_step "fit (F6)"                       ui_key F6
ui_step "fillet (F)"                     ui_key f
ui_step "pick a face of the body"        ui_view_click 50 50
ui_step "confirm (Enter)"                ui_key Return
ui_expect_log "Added fillet on 4 edge(s)" "fillet on the face's edges"
expect_title "Untitled* - MitCad"
ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
ui_step "type the path, Enter"           type_path "Save As" "$FILE"
ui_expect_log "Saved $FILE" "saved to the typed path"
expect_title "block.mitcad - MitCad"
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
expect_title "block.mitcad* - MitCad"
# The "*" follows the model's revisions: undo back to the state opened
# takes it away, redo brings it back.
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
expect_title "block.mitcad - MitCad"
ui_step "redo it (Ctrl+Y)"               ui_key ctrl+y
expect_title "block.mitcad* - MitCad"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_log "Saved $FILE" "saved without a dialog"
expect_title "block.mitcad - MitCad"
expect_file '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[35.0]" \
  "changed parameter saved"

echo "--- Unsaved changes: Cancel on New Design, Save on exit"
# Undo takes back the last command of this session: the change of d3.
ui_step "undo the change of d3 (Ctrl+Z)" ui_key ctrl+z
expect_title "block.mitcad* - MitCad"
ui_step "redo: the state saved (Ctrl+Y)" ui_key ctrl+y
expect_title "block.mitcad - MitCad"
ui_step "undo again (Ctrl+Z)"            ui_key ctrl+z
expect_title "block.mitcad* - MitCad"
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
focus_dialog '^MitCad$'
ui_step "cancel (Esc)"                   ui_key Escape
focus_main
expect_title "block.mitcad* - MitCad"
grep -q "New document" "$UI_LOG" && ui_fail "New Design went ahead after Cancel"
ui_step "exit (Ctrl+Q)"                  ui_key ctrl+q
focus_dialog '^MitCad$'
ui_key Return # Save, the default button
expect_exit "saved and exited"
grep -qF "Saved $FILE" "$UI_LOG" || ui_fail "not saved on exit"
expect_file '[p["value"] for p in doc["parameters"] if p["name"] == "d3"]' "[20.0]" \
  "undo saved on exit"

echo "--- Open dialog, Don't Save on New Design"
NEW_DESIGN="Opened the new design with Component1 active"
# no_new_design_since_mark description: a design read from a file opens
# with the component it was saved with active (mitcad#1678), not as a new
# design.
no_new_design_since_mark() {
  ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "$NEW_DESIGN" ||
    ui_fail "$1: logged '$NEW_DESIGN'"
  echo "ok   $1: opened as saved, not as a new design"
}
ui_start_app --no-native-dialogs
expect_title "Untitled - MitCad"
# Every new design starts with Component1 active (mitcad#898): at the
# start, after New and after Close.
ui_expect_log "$NEW_DESIGN" "the untitled design at the start has Component1 active"
ui_mark
ui_step "open (Ctrl+O)"                  ui_key ctrl+o
ui_step "type the path, Enter"           type_path "Open" "$FILE"
ui_expect_log "Opened $FILE" "opened through the dialog"
ui_expect_log "Recomputed 3 feature(s)" "sketch, extrude and fillet recomputed"
expect_title "block.mitcad - MitCad"
no_new_design_since_mark "the opened file"
checksum=$(sha256sum "$FILE")
ui_step "create a sketch on XY"          ui_create_sketch xy
expect_title "block.mitcad* - MitCad"
ui_mark
ui_step "new design (Ctrl+N)"            ui_key ctrl+n
focus_dialog '^MitCad$'
ui_step "don't save (D)"                 ui_key d
focus_main
ui_expect_new "New document" "new document after Don't Save"
ui_expect_new "New Design: no project, an untitled design" "outside projects New Design is New"
ui_expect_new "$NEW_DESIGN" "the new design after New has Component1 active"
expect_title "Untitled - MitCad"
[ "$(sha256sum "$FILE")" = "$checksum" ] || ui_fail "the file changed after Don't Save"
echo "ok   file unchanged"

echo "--- Close: the empty design after it has Component1 active (mitcad#898)"
ui_mark
ui_step "open (Ctrl+O)"                  ui_key ctrl+o
ui_step "type the path, Enter"           type_path "Open" "$FILE"
ui_expect_new "Opened $FILE" "opened through the dialog again"
expect_title "block.mitcad - MitCad"
no_new_design_since_mark "the file opened again"
ui_mark
ui_step "close (Ctrl+W)"                 ui_key ctrl+w
ui_expect_new "Document closed" "the design closed"
ui_expect_new "$NEW_DESIGN" "the new design after Close has Component1 active"
expect_title "Untitled - MitCad"
focus_main
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
expect_title "boss.mitcad - MitCad"
# Save is disabled while the design is unchanged (mitcad#1090): a change of
# the fillet's radius gives it something to save.
ui_save_state disabled "the opened design has nothing to save"
ui_stop_app
ui_start_app --open "$FILE" --set d9=2.5
ui_expect_log "Opened $FILE" "opened with a change of d9"
expect_title "boss.mitcad* - MitCad"
ui_save_state enabled "the changed design has something to save"
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
