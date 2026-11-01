#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework core/model/src/document core/model-api/src/components.rs
# Moving sketches and components from the browser, through the real UI:
#   - Move to Component... on a sketch row of the root (mitcad#218) opens
#     the timeline's destination picker: the sketch goes into an existing
#     component in one undo step.
#   - Move to Component... (Move to... before mitcad#735) on a component
#     nested in a turned assembly (mitcad#219): to the root, its body
#     staying where it was on screen and its row selected at the new
#     place; one undo step puts it back; then into another component. A
#     component cannot go into one placed in it: the picker says why, and
#     Cancel changes nothing.
#   - Move to Component... on a component a joint of its parent places
#     (mitcad#301): the picker names the joint that goes along, the move
#     takes it into the root with the component, which stays where it was
#     on screen; one undo step puts both back.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-test.sh [screenshot.png]
# check-all sources: tools/cli app/files/FileCommands.cpp

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-move.XXXXXX)
FILE=$WORK/move.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file expression expected description: a Python expression on the
# saved design file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# (Nothing to save when a step changed nothing: the file holds the design.)
save() {
  ui_save_if_changed "$FILE"
}

# The block's place in the window, from the last "Body <uid>[ in <path>] at
# x,y" line.
place() {
  grep -E "Body $1( in [O0-9/]+)? at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*,[0-9-]*\).*/\1/p'
}

# Assembly:1 (O1) turned a quarter about Z, holding Part:1 (O2): Sketch1
# (F1) 30 x 20 and Extrude1 (F2) 10 up, Body1 (F2.b0). Holder:1 (O3), empty,
# beside it. Sketch2 (F3) in the root. Frame:1 (O4) with a 40 x 40 x 10 block
# (F5.b0) holding Slider:1 (O5) with a 10 x 10 x 20 block (F7.b0), which
# Joint1 (F8) of Frame stands on Frame's block.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Assembly",
   "transform": {"translation": [40, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": 1.5707963267948966}}},
  {"cmd": "create_component", "name": "Part", "transform": {"translation": [0, 0, 5]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Holder", "transform": {"translation": [0, 60, 0]}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-50, -50], "width": 10, "height": 10},
  {"cmd": "create_component", "name": "Frame", "transform": {"translation": [-80, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F4", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "create_component", "name": "Slider", "transform": {"translation": [50, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F6", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Frame"},
  {"cmd": "add_joint", "kind": "rigid", "flip": true,
   "a": {"occurrence": "Slider:1", "geometry": {"body": "F7.b0", "face": "F7:start(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}},
   "b": {"geometry": {"body": "F5.b0", "face": "F5:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
# A tree longer than the browser opens folded below the root (mitcad#426):
# Frame:1, Assembly:1 and Sketches are opened, the lowest first (the rows
# above keep their places).
ui_expect_log "Browser opened " "the browser fitted the design"
ui_sync
if grep -q "Browser opened folded" "$UI_LOG"; then
  ui_step "open Frame:1"                 ui_click_logged "Browser arrow Root/Frame:1"
  ui_step "open Assembly:1"              ui_click_logged "Browser arrow Root/Assembly:1"
  ui_step "open Sketches"                ui_click_logged "Browser arrow Root/Sketches"
fi
ui_expect_log "Bodies shown: F2.b0 in O1/O2" "the block shown in Assembly:1/Part:1"
ui_expect_log "Browser Root/Sketches/Sketch2 at" "the root's sketch in the browser"
ui_expect_log "Browser Root/Assembly:1/Part:1 at" "Part:1 in Assembly:1"
ui_expect_log "Browser Root/Holder:1 at" "Holder:1 in the root"
ui_sync
BEFORE=$(place F2.b0)
[ -n "$BEFORE" ] || ui_fail "no place logged for the block"

echo "--- Move to Component... on a sketch row (mitcad#218)"
ui_mark
ui_step "right-click Sketch2"            ui_click_logged "Browser Root/Sketches/Sketch2" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_step "destinations (Alt+D, Home)"     ui_key alt+d Home
ui_step "Holder:1 (Down x3)"             ui_key Down Down Down
ui_step "move (Alt+M)"                   ui_key alt+m
ui_focus_main
ui_expect_new "Moved Sketch2 into Holder" "the sketch moved into Holder"
ui_expect_new "Move to Component completed: Root/Holder:1" "into the chosen occurrence"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[f.get("component", "C0") for f in doc["features"]][:3]' "['C2', 'C2', 'C3']" \
  "Sketch2 is Holder's"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "the move undone"
save
expect_file '[f.get("component", "C0") for f in doc["features"]][:3]' "['C2', 'C2', 'C0']" \
  "one undo step puts Sketch2 back in the root"

echo "--- Move to Component... on a component: to the root (mitcad#219)"
ui_mark
ui_step "right-click Part:1"             ui_click_logged "Browser Root/Assembly:1/Part:1" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_expect_new "Move Component parents of Part:1: Root | Root/Assembly:1 (unavailable)" \
  "the parents with their full paths, the current one unavailable"
ui_step "parents (Alt+P, Home)"          ui_key alt+p Home
ui_expect_new "Move Component parent: Root, ok" "the root can be its parent"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "move (Alt+M)"                   ui_key alt+m
ui_focus_main
ui_expect_new "Moved Part:1 to Root" "moved"
ui_expect_new "Bodies shown: F2.b0 in O2" "the block shown in Part:1 in the root"
ui_expect_new "Browser Root/Part:1 at" "Part:1 is a row of the root"
ui_expect_new "Selected: " "its row selected again"
ui_sync
AFTER=$(place F2.b0)
[ "$AFTER" = "$BEFORE" ] || ui_fail "the block moved on screen: $BEFORE -> $AFTER"
echo "ok   the block stays where it was ($AFTER)"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[(o["uid"], o["parent"]) for o in doc["occurrences"]][:3]' \
  "[('O1', 'C0'), ('O2', 'C0'), ('O3', 'C0')]" "Part:1 placed in the root"

echo "--- Undo and redo: one step"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Move Part:1 to Root" "the move undone in one step"
ui_expect_new "Bodies shown: F2.b0 in O1/O2" "back in Assembly:1"
save
expect_file '[(o["uid"], o["parent"]) for o in doc["occurrences"]][:3]' \
  "[('O1', 'C0'), ('O2', 'C1'), ('O3', 'C0')]" "Part:1 placed in Assembly again"

echo "--- Move to Component... into another component"
ui_mark
ui_step "right-click Part:1"             ui_click_logged "Browser Root/Assembly:1/Part:1" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_step "parents (Alt+P, Home)"          ui_key alt+p Home
ui_step "Holder:1 (Down x3)"             ui_key Down Down Down
ui_expect_new "Move Component parent: Root/Holder:1, ok" "Holder:1 can be its parent"
ui_step "move (Alt+M)"                   ui_key alt+m
ui_focus_main
ui_expect_new "Moved Part:1 to Root/Holder:1" "moved into Holder"
ui_expect_new "Bodies shown: F2.b0 in O3/O2" "the block shown in Holder:1/Part:1"
ui_sync
AFTER=$(place F2.b0)
[ "$AFTER" = "$BEFORE" ] || ui_fail "the block moved on screen: $BEFORE -> $AFTER"
echo "ok   the block stays where it was"
save
expect_file '[(o["uid"], o["parent"]) for o in doc["occurrences"]][:3]' \
  "[('O1', 'C0'), ('O2', 'C3'), ('O3', 'C0')]" "Part:1 placed in Holder"

echo "--- A component cannot go into one placed in it; Cancel changes nothing"
ui_mark
ui_step "right-click Holder:1"           ui_click_logged "Browser Root/Holder:1" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_step "parents (Alt+P, Home)"          ui_key alt+p Home
ui_step "Holder:1/Part:1 (Down x3)"      ui_key Down Down Down
ui_expect_new "Move Component parent: Root/Holder:1/Part:1, unavailable: Holder:1 cannot go into Part, which is placed in it" \
  "the reason is shown"
ui_step "cancel (Escape)"                ui_key Escape
ui_focus_main
ui_expect_new "Move of Holder:1 cancelled" "cancelled"
save
expect_file '[(o["uid"], o["parent"]) for o in doc["occurrences"]][:3]' \
  "[('O1', 'C0'), ('O2', 'C3'), ('O3', 'C0')]" "nothing changed"

echo "--- Move to Component... on a component a joint places: the joint goes along (mitcad#301)"
ui_expect_log "F7.b0 in O4/O5" "the slider shown in Frame:1"
SLIDER=$(place F7.b0)
[ -n "$SLIDER" ] || ui_fail "no place logged for the slider"
ui_mark
ui_step "right-click Slider:1"           ui_click_logged "Browser Root/Frame:1/Slider:1" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_step "parents (Alt+P, Home)"          ui_key alt+p Home
ui_expect_new "Move Component features: Joint1 -> Root" "the preview names the joint that moves along"
ui_expect_new "Move Component parent: Root, ok" "the root can be its parent"
ui_step "move (Alt+M)"                   ui_key alt+m
ui_focus_main
ui_expect_new "Moved Slider:1 to Root" "moved with its joint"
ui_expect_new "Browser Root/Slider:1 at" "Slider:1 is a row of the root"
ui_sync
AFTER=$(place F7.b0)
[ "$AFTER" = "$SLIDER" ] || ui_fail "the slider moved on screen: $SLIDER -> $AFTER"
echo "ok   the slider stays where it was"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[(f.get("component", "C0"), f["b"].get("occurrence")) for f in doc["features"] if f["name"] == "Joint1"]' \
  "[('C0', 'O4')]" "Joint1 is a root joint to Frame:1's block"
expect_file '[(o["uid"], o["parent"]) for o in doc["occurrences"]][3:]' \
  "[('O4', 'C0'), ('O5', 'C0')]" "Slider:1 placed in the root"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Move Slider:1 to Root" "the move and the joint undone in one step"
save
expect_file '[(f.get("component", "C0"), f["b"].get("occurrence")) for f in doc["features"] if f["name"] == "Joint1"]' \
  "[('C4', None)]" "Joint1 is Frame's again"

ui_finish "UI move test"
