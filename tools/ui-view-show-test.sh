#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/MainWindowVisibility.cpp app/view/ViewController.cpp
# check-all sources: app/files/MainWindowLocks.cpp core/model/src/document/display.rs
# check-all sources: core/model-api/src/lib.rs tools/cli
# A body's Browser row and View › Show (mitcad#1287, GitHub mitcad/Mitcad#142)
# through the real UI, on designs mitcad-cli makes:
#   1. Docked: three boxes written to STEP and read back as one base
#      feature, its bodies renamed Left (F1.b0), Middle (F1.b1) and Right
#      (F1.b2). Delete Body in Middle's row menu removes Middle only (a
#      Remove Body, the base feature kept); one undo brings it back. Left
#      and Right selected, the row menu's Delete Bodies removes both in one
#      undo step.
#   2. Floating: components Frame (O1) and Motor (O2), each with a
#      construction plane (FramePlane, MotorPlane) and a sketch (F2, F4),
#      the Origin folder's light bulb on. View › Show › Construction off
#      (its command by a key of our own, F8) hides both planes, greys their
#      rows and keeps the sketches and the origin; the design saved keeps
#      it as display state and no feature's light bulb; on again shows both
#      planes. Sketches (F9) and Origins (F7) the same way, the Origin
#      folder's light bulb staying on.
# tools/ui-windows-view-show-test.ps1 does the same in the Windows VM.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-show-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-view-show.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/boxes.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0], "length": 20,
    "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "export", "path": "$WORK/boxes.step"}
]
EOF
cat > "$WORK/imported.json" << EOF
[
  {"cmd": "import_file", "path": "$WORK/boxes.step", "structure": "base"},
  {"expect": {"bodies": 3}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Left"},
  {"cmd": "rename_body", "uid": "F1.b1", "name": "Middle"},
  {"cmd": "rename_body", "uid": "F1.b2", "name": "Right"}
]
EOF
cat > "$WORK/components.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Frame"},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 10}}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Motor", "transform": [[1, 0, 0, 60], [0, 1, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "rename_feature", "uid": "F1", "name": "FramePlane"},
  {"cmd": "rename_feature", "uid": "F3", "name": "MotorPlane"}
]
EOF
"$CLI" run "$WORK/boxes.json" --save "$WORK/boxes.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli boxes"; }
"$CLI" run "$WORK/imported.json" --save "$WORK/imported.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli imported"; }
"$CLI" run "$WORK/components.json" --save "$WORK/components.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli components"; }

# View › Show's commands by keys of our own, as Tools > Keyboard Shortcuts
# saves them.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[shortcuts]\nview.show_origins=F7\nview.show_construction=F8\nview.show_sketches=F9\n' \
  > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

# last_since "prefix": the last line since the mark that begins with it.
last_since() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^$1" | tail -1; }

# expect_planes description: the last "Construction shown:" line has both
# components' planes.
expect_planes() {
  local line
  line=$(grep "^Construction shown: " "$UI_LOG" | tail -1)
  [[ $line == *"FramePlane in O"* && $line == *"MotorPlane in O"* ]] || ui_fail "$1: $line"
  echo "ok   $1: $line"
}

ui_start_display

echo "=== 1. Delete on a body's row (Docked)"
ui_start_app --open "$WORK/imported.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0, F1.b1, F1.b2" "the three imported bodies shown"
ui_expect_log "View Show menu: Origins on | Construction on | Sketches on" "View › Show's entries, all on"
ui_expect_log "Browser Root/Bodies/Middle at" "the browser lists Middle"

ui_mark
ui_step "right-click Middle's row"        ui_click_logged "Browser Root/Bodies/Middle" 3
ui_expect_new "Context menu: " "the row's menu"
menu=$(last_since "Context menu: ")
[[ $menu == *"| Delete Body |"* ]] || ui_fail "the row's menu has no Delete Body: $menu"
echo "ok   the row's menu says Delete Body"
ui_step "choose Delete Body"               ui_menu_choose "Delete Body"
ui_expect_new "Deleted body Middle" "Middle deleted"
ui_expect_new "Bodies shown: F1.b0, F1.b2" "Left and Right stay: the base feature is kept"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Deleted Base" && ui_fail "the base feature was deleted"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Delete Middle" "one undo step, named as Delete Body names it"
ui_expect_new "Bodies shown: F1.b0, F1.b1, F1.b2" "Middle back"

ui_mark
ui_step "click Left"                       ui_click_logged "Browser Root/Bodies/Left"
ui_step "Ctrl+click Right"                 xdotool mousemove $(ui_logged_at "Browser Root/Bodies/Right") keydown ctrl click 1 keyup ctrl
ui_expect_new "Browser selected: Root/Bodies/Left, Root/Bodies/Right" "Left and Right selected"
ui_step "right-click Left"                 ui_click_logged "Browser Root/Bodies/Left" 3
ui_expect_new "Context menu: " "the row's menu"
ui_step "choose Delete Bodies"             ui_menu_choose "Delete Bodies"
ui_expect_new "Deleted body " "the bodies deleted"
deleted=$(last_since "Deleted body ")
[[ $deleted == *Left* && $deleted == *Right* && $deleted != *Middle* ]] || ui_fail "not Left and Right: $deleted"
echo "ok   $deleted"
ui_expect_new "Bodies shown: F1.b1" "Middle stays"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Delete Bodies" "one undo step for both"
ui_expect_new "Bodies shown: F1.b0, F1.b1, F1.b2" "both back"
ui_stop_app

echo "=== 2. View › Show of the whole design (Floating)"
ui_start_app --chrome=floating --open "$WORK/components.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "View Show menu: Origins on | Construction on | Sketches on" "View › Show's entries, all on"
ui_expect_log "Construction shown: " "the planes drawn"
expect_planes "both components' planes drawn"
ui_expect_log "Sketches shown: F2, F4" "both components' sketches drawn"
ui_show_origin

echo "--- Construction"
ui_mark
ui_step "View › Show › Construction off (F8)" ui_key F8
ui_expect_new "View show construction: hidden" "Construction turned off"
ui_expect_new "Construction shown: none" "no plane drawn in either component"
ui_expect_new "View Show menu: Origins on | Construction off | Sketches on" "its check mark off"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Sketches shown: " && ui_fail "the sketches changed"
origin=$(grep -o "^Design origin .*" "$UI_LOG" | tail -1)
[ "$origin" = "Design origin shown in the view; the Origin folder's light bulb on" ] ||
  ui_fail "the origin changed: $origin"
echo "ok   the sketches and the origin stay"
colours=$(grep "^Browser row colours: " "$UI_LOG" | tail -1)
[[ $colours == *"Frame:1/Construction/FramePlane"* && $colours == *"Motor:1/Construction/MotorPlane"* ]] ||
  ui_fail "the planes' rows are not greyed: $colours"
echo "ok   the planes' rows greyed"
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved " "saved"
python3 - "$WORK/components.mitcad" << 'PYEOF' || ui_fail "the design file"
import json, sys
doc = json.load(open(sys.argv[1]))
hidden = doc.get("display", {}).get("hidden")
assert hidden == {"construction": True}, f"display.hidden is {hidden}"
lit = [f.get("name") for f in doc["features"] if "visible" in f]
assert not lit, f"light bulbs set: {lit}"
PYEOF
echo "ok   the design keeps Construction hidden as display state, no light bulb set"
ui_mark
ui_step "View › Show › Construction on (F8)" ui_key F8
ui_expect_new "View show construction: shown" "Construction turned on"
ui_expect_new "Construction shown: " "the planes drawn again"
expect_planes "both components' planes drawn again"
ui_expect_new "View Show menu: Origins on | Construction on | Sketches on" "its check mark on"
colours=$(grep "^Browser row colours: " "$UI_LOG" | tail -1)
[[ $colours != *"/Construction/"* ]] || ui_fail "a plane's row is still greyed: $colours"
echo "ok   the planes' rows no longer greyed"

echo "--- Sketches and Origins"
ui_mark
ui_step "View › Show › Sketches off (F9)"  ui_key F9
ui_expect_new "Sketches shown: none" "no sketch drawn"
ui_mark
ui_step "View › Show › Sketches on (F9)"   ui_key F9
ui_expect_new "Sketches shown: F2, F4" "both sketches drawn again"
ui_mark
ui_step "View › Show › Origins off (F7)"   ui_key F7
ui_expect_new "Design origin hidden in the view; the Origin folder's light bulb on" \
  "the origin hidden, its light bulb still on"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Hide All Origins" "an undo step of its own"
ui_expect_new "Design origin shown in the view; the Origin folder's light bulb on" "the origin back"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI View Show test"
