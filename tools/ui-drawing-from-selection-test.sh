#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/files app/browser app/MainWindow.cpp core/model/src/drawing
# check-all sources: core/model/src/document/drawings.rs core/model-api/src/drawings.rs tools/cli
# Create Drawing from the selection (mitcad#2399, GitHub #621) through the
# real UI, on a design that mitcad-cli makes: Body1 (F2.b0) and Body2
# (F3.b0) of the root, and Component1 (its Body3, F4.b0) placed three
# times, Component1:1 (O1), Component1:2 (O2) and Component1:3 (O3):
#   - Component1:2 selected in the browser, Create Drawing: the drawing is
#     named for it (Component1:2 Drawing), its Base View shows it
#     (occurrence O2) and the view placed draws its one body only.
#   - Body1 and Body2 selected (a click, a Ctrl+click): Body1 and Body2
#     Drawing, its Base View shows the two bodies and the view draws two.
#   - Nothing selected (Esc): Drawing1, a view of the whole design, all
#     five bodies drawn.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-from-selection-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$HOME/mitcad-ui-drawing-from-selection.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/selection.mitcad"

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1; Box (F3) beside it:
# Body2; Extrude (F4) 4 down as a new component: Component1:1 (O1, Body3),
# moved 40 mm along X, copied 50 and 100 mm along Y (O2, O3).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, 0],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 50, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 100, 0]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

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

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# create_drawing: Create Drawing through the command search, its dialog's
# OK; the drawing opens with Base View on.
create_drawing() {
  ui_step "Create Drawing (search)"        ui_command "Create Drawing"
  ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
  ui_focus_dialog '^Create Drawing$'
  ui_step "OK"                             ui_key Return
  ui_focus_main
}

# place_base_view: a click on the sheet at 150, 150 with Base View on, then
# Esc out of the tool that follows.
place_base_view() {
  ui_expect_new "Drawing sheet view " "the sheet placed"
  ui_expect_new "Drawing tool: Base View" "Base View is on for the new drawing"
  ui_mark
  ui_step "click the sheet at 150, 150"    sheet_click 150 150
  ui_expect_new '"type":"base"' "the base view asked for"
  ui_expect_new "Drawing view 1 at" "the view drawn"
}

close_drawing() {
  ui_expect_new "Drawing tool: Projected View" "projected views follow"
  ui_step "end the tool (Esc)"             ui_key Escape
  ui_expect_new "Drawing tool: selection" "back to the selection"
  no_refusal
  ui_mark
  settled "Drawing Close Drawing"
  ui_step "Close Drawing"                  ui_click_logged "Drawing Close Drawing"
  ui_expect_new "Drawing closed" "the 3D view again"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "selection.mitcad" "the design opened"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Component1:2 at" "the browser lists Component1:2"
ui_expect_log "Browser Root/Bodies/Body2 at" "and the root's Body2"
ui_expect_log "Bodies shown: F2.b0, F3.b0, F4.b0 in O1, F4.b0 in O2, F4.b0 in O3" "the five bodies shown"

echo "--- One occurrence among three"
ui_mark
ui_step "select Component1:2"              ui_click_logged "Browser Root/Component1:2"
ui_expect_new "Browser selected: Root/Component1:2" "Component1:2 selected"
ui_mark
create_drawing
ui_expect_new "Create Drawing from the selection: occurrence:O2 (Component1:2)" "the subject taken from the selection"
ui_expect_new "Drawing created: D1 Component1:2 Drawing in C0" "named for what it shows"
ui_expect_new "Drawing opened: D1 Component1:2 Drawing, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing Base View shows the selection: occurrence:O2 (Component1:2)" "Base View's Shows is it"
place_base_view
ui_expect_new '"subject":{"occurrence":"O2","type":"occurrence"}' "the occurrence sent as the view's subject"
view_draws 1 1 "Component1:2's body alone"
close_drawing

echo "--- Two bodies"
ui_mark
ui_step "click Body1"                      ui_click_logged "Browser Root/Bodies/Body1"
ui_step "Ctrl+click Body2"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Body2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Bodies/Body2" "Body1 and Body2 selected"
ui_mark
create_drawing
ui_expect_new "Create Drawing from the selection: 2 bodies (Body1 and Body2)" "the two bodies taken from the selection"
ui_expect_new "Drawing created: D2 Body1 and Body2 Drawing in C0" "named for them"
ui_expect_new "Drawing Base View shows the selection: selection:" "Base View's Shows is the two bodies"
place_base_view
ui_expect_new '"subject":{"bodies":[{"body":"F2.b0"},{"body":"F3.b0"}],"type":"bodies"}' "the two bodies sent as the view's subject"
view_draws 1 2 "Body1 and Body2 alone"
close_drawing

echo "--- Nothing selected"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_sync
ui_mark
create_drawing
ui_expect_new "Drawing created: D3 Drawing1 in C0" "the default name"
ui_expect_new "Drawing opened: D3 Drawing1, sheet 1 of 1" "its sheet over the view"
grep -q "Create Drawing from the selection" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "nothing selected, yet a subject was taken from the selection"
grep -q "Drawing Base View shows the selection" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "nothing selected, yet Base View showed a selection"
echo "ok   nothing taken from a selection"
place_base_view
view_draws 1 5 "the whole design"
close_drawing

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI drawing from selection test"
