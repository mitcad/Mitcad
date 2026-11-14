#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/ShownDimensions.cpp app/sketch/ShownDimensions.hpp app/sketch/SketchOverlay.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentSnapshot.cpp app/MainWindow.cpp
# check-all sources: core/model/src/document/display.rs core/model-api/src/lib.rs tools/cli
# Show Dimensions of a sketch's row (mitcad#1772, GitHub mitcad/Mitcad#306)
# through the real UI: Part (Sketch1) and Cover (Sketch2), each a rectangle
# with its two sides dimensioned, side by side.
#   - Both sketches are opened and finished; then Show Dimensions in
#     Sketch1's browser menu draws its two dimensions in the view outside
#     sketch mode, and not Sketch2's.
#   - Part hidden by its eye's menu: they are gone; shown again: back.
#   - Saved and opened again: the setting is kept (display state in the
#     file), the timeline's menu has it checked.
#   - A double-click on one of the dimensions opens Sketch1 and the
#     dimension's value editor.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-show-dimensions-test.sh [screenshot.png]

set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-show-dimensions.XXXXXX")
FILE=$WORK/sides.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file expression expected description: a Python expression on the
# saved design file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# expect_last prefix expected description: the last line logged that starts
# with the prefix is the prefix and the expected text.
expect_last() {
  local last
  ui_sync
  last=$(grep -F -- "$1" "$UI_LOG" | tail -1)
  [ "$last" = "$1$2" ] || ui_fail "$3: the last '$1' line is '$last', expected '$1$2'"
  echo "ok   $3"
}

# edit_and_finish name uid: the sketch opened from the timeline and finished.
edit_and_finish() {
  ui_double_click_expect "Timeline $1" "Editing sketch $2 ($1)" "$1 opened"
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"   ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$1 finished"
}

# Part:1 (O1, C1): Sketch1 (F1), a 30 x 20 rectangle with its width and
# height dimensioned. Cover:1 (O2, C2): Sketch2 (F2), the same 50 mm
# to the right. Neither sketch is used, so both are shown.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cover"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [50, 0], "width": 30, "height": 20},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Sketch dimensions shown outside sketch mode: none" "no sketch shows its dimensions at first"

echo "--- Both sketches opened and finished"
edit_and_finish Sketch1 F1
edit_and_finish Sketch2 F2
expect_last "Sketch dimensions shown outside sketch mode: " "none" "finished sketches show no dimensions"

echo "--- Show Dimensions on Sketch1's row"
ui_mark
ui_step "right-click Sketch1"            ui_click_logged "Browser Root/Part:1/Sketches/Sketch1" 3
ui_expect_new "Show Dimensions" "the sketch row's menu has Show Dimensions"
ui_step "choose Show Dimensions"         ui_menu_choose "Show Dimensions"
ui_expect_new "Show Dimensions of Sketch1: on" "turned on"
ui_expect_new "Sketch dimensions shown outside sketch mode: Sketch1 (2)" "Sketch1's two dimensions are shown"
ui_expect_new "Sketch dimensions drawn outside sketch mode: Sketch1 (2)" "and drawn in the view"
expect_last "Sketch dimensions drawn outside sketch mode: " "Sketch1 (2)" "Sketch2's are not drawn"
grep -q "^Shown dimension Sketch2 " "$UI_LOG" && ui_fail "a dimension of Sketch2 was drawn"
echo "ok   no place of Sketch2's dimensions logged"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Hidden with its component"
ui_mark
ui_step "right-click Part:1"             ui_click_logged "Browser Root/Part:1" 3
ui_step "choose Hide"                    ui_menu_choose "Hide"
ui_expect_new "Sketch dimensions shown outside sketch mode: none" "Part hidden: its sketch's dimensions are gone"
ui_mark
ui_step "right-click Part:1"             ui_click_logged "Browser Root/Part:1" 3
ui_step "choose Show"                    ui_menu_choose "Show"
ui_expect_new "Sketch dimensions shown outside sketch mode: Sketch1 (2)" "Part shown: they are back"

echo "--- Kept in the design"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file 'doc["display"]["sketch_dimensions"]' "['F1']" "the file keeps Sketch1's Show Dimensions"
ui_stop_app
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Sketch dimensions shown outside sketch mode: Sketch1 (2)" "opened again: Sketch1's dimensions shown"
ui_expect_log "Sketch dimensions drawn outside sketch mode: Sketch1 (2)" "and drawn"
ui_mark
ui_step "right-click Sketch1 (timeline)" ui_click_logged "Timeline Sketch1" 3
ui_expect_new "Context menu checked: Show Dimensions" "the timeline's menu has it checked"
ui_step "close the menu"                 ui_key Escape

echo "--- A double-click edits the dimension"
ui_sync
DIMENSION=$(grep -o "^Shown dimension Sketch1 k[0-9]* at " "$UI_LOG" | tail -1 | awk '{print $4}')
[ -n "$DIMENSION" ] || ui_fail "no place of Sketch1's dimensions logged"
ui_double_click_expect "Shown dimension Sketch1 $DIMENSION" "Editing dimension $DIMENSION (" \
  "the double-click opens Sketch1 and the dimension's value editor"
ui_expect_log "Editing sketch F1 (Sketch1)" "in Sketch1"
ui_mark
ui_step "close the editor (Esc)"         ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch dimensions shown outside sketch mode: Sketch1 (2)" \
  "left: the row's setting draws them again"

ui_finish 'UI Show Dimensions of a sketch outside sketch mode'
