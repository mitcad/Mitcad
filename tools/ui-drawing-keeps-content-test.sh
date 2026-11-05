#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/MainWindow.cpp core/model/src/drawing core/model/src/document/drawings.rs
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing.rs core/model-api/src/lib.rs tools/cli
# A drawing keeps what was put into it (mitcad#1293, GitHub #144) through
# the real UI, on Plate (a 40 x 30 x 5 body, F2.b0) placed twice, Plate:1
# (O1) and Plate:2 (O2) 50 mm apart, that mitcad-cli makes (as
# tools/ui-windows-drawing-keeps-content-test.ps1 does on Windows):
#   - Create Drawing and a base view of the design: it draws both plates
#     (its projection logged with 2 bodies).
#   - Close Drawing, Plate:2 hidden in the browser (its light bulb), the
#     drawing opened again: the view still draws both.
#   - Close Drawing, Plate:1 copied and pasted in the browser, 100 mm along
#     X (Plate:3, O3), the drawing opened again: the view still draws two.
#   - The view's Shows: Plate:3 checked (its row, Space): three bodies, one
#     undo step; Ctrl+Z takes it away.
#   - Plate:2 shown again; Update from Model: the three shown, one undo
#     step (Update view 1 of Drawing1); Ctrl+Z takes Plate:3 away again.
#   - A design file written before views recorded their bodies (mitcad-cli
#     makes it with Plate:2 hidden, its view's subject taken out): opened,
#     the view is pinned to what the file shows (one body) and the app says
#     so; Plate:2 shown, the view still draws one.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-keeps-content-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$HOME/mitcad-ui-drawing-keeps-content.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/plates.mitcad"
OLDER="$WORK/older.mitcad"

PLATES=$(cat << EOF
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "copy_occurrence", "occurrence": "Plate:1", "transform": {"translation": [50, 0, 0]}}
EOF
)
printf '[\n%s\n]\n' "$PLATES" > "$WORK/plates.json"
"$CLI" run "$WORK/plates.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# The older file: Plate:2 hidden, a drawing with a front view, the view's
# subject taken out of the file as mitcad-cli saved it.
cat > "$WORK/older.json" << EOF
[
$PLATES,
  {"cmd": "set_occurrence_visible", "occurrence": "Plate:2", "visible": false},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "front", "at": [150, 150], "scale": "1:1"}
]
EOF
"$CLI" run "$WORK/older.json" --save "$WORK/recorded.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the older file)"; }
python3 -I -c '
import json, sys
doc = json.load(open(sys.argv[1]))
view = doc["drawings"][0]["sheets"][0]["views"][0]
assert view["subject"]["type"] == "bodies", view
del view["subject"]
json.dump(doc, open(sys.argv[2], "w"))
' "$WORK/recorded.mitcad" "$OLDER" || ui_fail "the older file not made"
echo "ok   an older design file made: its view without a subject"

# sheet_at x y: the screen position of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (tools/ui-drawings-test.sh).
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
sheet_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") click 1; ui_sync; }

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# view_draws view bodies why: since the mark, the log's line of the view's
# projection (mitcad#1033) with that many bodies, the kernel's or the
# cache's.
view_draws() {
  local pattern="^Drawing projection of view $1: $2 bodies, [0-9]+\.[0-9] ms, (projected exactly|projected from a triangulation|from the cache)\$"
  for _ in $(seq 1 30); do
    if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "$pattern"; then
      echo "ok   view $1 draws $2 bodies: $3"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "view $1 does not draw $2 bodies ($3): no such line of its projection in the log"
}

# shown_bodies "list": the 3D view's last "Bodies shown:" line, waited for,
# is that list.
shown_bodies() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep "^Bodies shown: " "$UI_LOG" | tail -1)
    if [ "$last" = "Bodies shown: $1" ]; then
      echo "ok   the 3D view shows $1"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "the 3D view shows '$last', not '$1'"
}

close_drawing() {
  ui_mark
  settled "Drawing Close Drawing"
  ui_step "Close Drawing"                  ui_click_logged "Drawing Close Drawing"
  ui_expect_new "Drawing closed" "the 3D view again"
}

open_drawing() {
  ui_mark
  ui_step "open Drawing1 (double-click)"   ui_double_click_logged "Browser Root/Drawings/Drawing1"
  ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
  # The sheet's place is logged only when it changes: not again here.
  ui_expect_new "Drawing sheet drawn: " "its sheet drawn"
}

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "plates.mitcad" "the design opened"
shown_bodies "F2.b0 in O1, F2.b0 in O2"

echo "--- A base view of the design"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
ui_focus_dialog '^Create Drawing$'
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing tool: Base View" "Base View is on for the new drawing"
ui_mark
ui_step "click the sheet at 150, 150"      sheet_click 150 150
ui_expect_new '"type":"base"' "the base view asked for"
ui_expect_new "Drawing view 1 at" "the front view drawn"
view_draws 1 2 "both plates"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
no_refusal

echo "--- Plate:2 hidden in the browser"
close_drawing
ui_mark
ui_step "hide Plate:2 (light bulb)"        ui_click_logged "Browser eye Root/Plate:2"
shown_bodies "F2.b0 in O1"
open_drawing
view_draws 1 2 "hidden in the 3D view, Plate:2 still drawn"

echo "--- A placement added: Plate:1 copied and pasted"
close_drawing
ui_mark
ui_step "right-click Plate:1"              ui_click_logged "Browser Root/Plate:1" 3
ui_step "choose Copy"                      ui_menu_choose "Copy"
ui_expect_new "Copied Plate:1" "copied"
ui_mark
ui_step "right-click Root"                 ui_click_logged "Browser Root" 3
ui_step "choose Paste"                     ui_menu_choose "Paste"
ui_expect_new "Pasted Plate:1 as Plate:3" "a third placement"
ui_expect_new "Panel Move/Copy input tx at" "Move/Copy opens on it"
ui_step "100 mm along X"                   ui_type_in "Panel Move/Copy input tx" "100"
ui_expect_new "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Placed Plate:3 at (100, 0, 0)" "placed 100 mm along X"
shown_bodies "F2.b0 in O1, F2.b0 in O3"
open_drawing
view_draws 1 2 "the new placement not drawn"

echo "--- Shows adds it"
ui_mark
ui_step "select the front view"            ui_click_logged "Drawing view 1"
ui_expect_new "Drawing view 1 selected" "the front view selected"
ui_expect_new "Drawing view 1 shows 2 of 3 bodies" "its Shows: Plate:1 and Plate:2 of the three"
ui_expect_new "Drawing panel drawingViewShows row Plate:3 at" "Plate:3's row in its Shows"
ui_expect_new "Drawing panel drawingViewUpdate at" "Update from Model beside it"
settled "Drawing panel drawingViewShows row Plate:3"
ui_mark
ui_step "Plate:3's row"                    ui_click_logged "Drawing panel drawingViewShows row Plate:3"
ui_step "check it (Space)"                 ui_key space
ui_expect_new "Drawing view 1 Shows changed: 3 bodies" "the three plates sent"
ui_expect_new '"cmd":"edit_drawing_view"' "one edit of the view"
view_draws 1 3 "Plate:3 added by its Shows"
no_refusal
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Edit view 1 of Drawing1" "one undo step"
view_draws 1 2 "Plate:3 away again"

echo "--- Update from Model"
close_drawing
ui_mark
ui_step "show Plate:2 (light bulb)"        ui_click_logged "Browser eye Root/Plate:2"
shown_bodies "F2.b0 in O1, F2.b0 in O2, F2.b0 in O3"
open_drawing
view_draws 1 2 "shown again, nothing added"
ui_mark
ui_step "select the front view"            ui_click_logged "Drawing view 1"
ui_expect_new "Drawing view 1 selected" "the front view selected"
ui_expect_new "Drawing panel drawingViewUpdate at" "Update from Model in its panel"
settled "Drawing panel drawingViewUpdate"
ui_mark
ui_step "Update from Model"                ui_click_logged "Drawing panel drawingViewUpdate"
ui_expect_new "Drawing view 1 updated from the model" "the button pressed"
ui_expect_new '"subject":{"type":"design"}' "what the 3D view shows asked for"
view_draws 1 3 "the three plates shown in the 3D view"
ui_expect_new "Drawing view 1 shows 3 of 3 bodies" "its Shows has all three"
no_refusal
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Update view 1 of Drawing1" "one undo step"
view_draws 1 2 "Plate:3 away again"

echo "--- A design file written before views kept their bodies"
ui_stop_app
ui_start_app --open "$OLDER" --no-native-dialogs
ui_expect_log "older.mitcad" "the older design opened"
ui_expect_log "Drawing pinned: Drawing1 now keeps what its views showed when the design was opened (1 body)" \
  "the view pinned, and said so"
shown_bodies "F2.b0 in O1"
ui_mark
ui_step "show Plate:2 (light bulb)"        ui_click_logged "Browser eye Root/Plate:2"
shown_bodies "F2.b0 in O1, F2.b0 in O2"
open_drawing
view_draws 1 1 "what the file showed: Plate:1 alone"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI drawing keeps content test"
