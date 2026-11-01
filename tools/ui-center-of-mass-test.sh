#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowCenterOfMass.cpp app/view/CenterOfMass.cpp app/view/CenterOfMassView.cpp
# check-all sources: app/browser app/OcctViewer.cpp core/model-api/src/center_of_mass.rs
# check-all sources: core/model-api/src/physical_extent.rs
# check-all sources: core/model/src/document/display.rs tools/cli
# The Center of Mass marker (mitcad#1094) through the real UI, in the
# Docked layout, on a design mitcad-cli makes: Body1 (F2.b0), a 30 x 20 x
# 10 block of aluminum in the root, and Body2 (F3.b0), the same outline 4
# deep below it in Component1:1 (O1), steel for lack of a material:
#   - Center of Mass (the command search) adds the marker: the view draws it
#     at the total center of mass of the `properties` query (mitcad-cli
#     info --properties, within 0.000001 mm), and the browser has the
#     Center of Mass row under Origin, its tooltip with the coordinates, the
#     mass and "1 body without a material counted as steel".
#   - Component1:1 moved 20 mm along X with Move/Copy from its row's menu:
#     the marker moves by 20 mm times Body2's share of the mass.
#   - The row's light bulb hides the marker and shows it again; hidden,
#     saved and opened again it stays hidden; Delete in the row's menu
#     removes it and its row, and undo brings it back.
# tools/ui-windows-center-of-mass-test.ps1 does the same in the Windows VM;
# tools/ui-center-of-mass-look.sh takes pictures of the marker on the light
# and the dark background.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-center-of-mass-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-center-of-mass.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/design.mitcad
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_body_material", "uid": "F2.b0", "material": "aluminum"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The `properties` query of the design as saved: the total's center of
# mass and each body's mass ("x y z m1 m2", Body1's mass first).
"$CLI" info "$FILE" --json --properties > "$WORK/properties.json" 2> "$WORK/cli.log" ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli info --properties"; }
read -r PX PY PZ M1 M2 <<< "$(python3 -c '
import json, sys
doc = json.load(open(sys.argv[1]))["properties"]
mass = {b["uid"]: b["mass"] for b in doc["bodies"]}
c = doc["total"]["center_of_mass"]
print(repr(c[0]), repr(c[1]), repr(c[2]), repr(mass["F2.b0"]), repr(mass["F3.b0"]))
' "$WORK/properties.json")"
[ -n "$M2" ] || ui_fail "no properties of both bodies in $WORK/properties.json"
echo "ok   the properties query: total center of mass $PX, $PY, $PZ; masses $M1 and $M2 kg"

# center_since: "x y z" of the newest "Center of Mass: at x, y, z mm" line
# since the mark.
center_since() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "^Center of Mass: at [-0-9., ]* mm" | tail -1 |
    sed 's/^Center of Mass: at \([-0-9.]*\), \([-0-9.]*\), \([-0-9.]*\) mm$/\1 \2 \3/'
}

# expect_center x y z description: the marker's last place within
# 0.000001 mm of (x, y, z).
expect_center() {
  local got
  got=$(center_since)
  [ -n "$got" ] || ui_fail "$4: no 'Center of Mass: at' line"
  awk -v g="$got" -v x="$1" -v y="$2" -v z="$3" 'BEGIN {
    split(g, c, " ")
    exit !((c[1] - x) ^ 2 <= 1e-12 && (c[2] - y) ^ 2 <= 1e-12 && (c[3] - z) ^ 2 <= 1e-12) }' ||
    ui_fail "$4: the marker is at $got, expected $1 $2 $3"
  echo "ok   $4: $got"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0, F3.b0 in O1" "both bodies shown"
ui_expect_log "Center of Mass: none" "no marker yet"

echo "--- Center of Mass"
ui_mark
ui_step "Center of Mass (search)"        ui_command "Center of Mass"
ui_expect_new "Center of Mass added" "the marker added"
ui_expect_new "Center of Mass: at " "its place worked out"
expect_center "$PX" "$PY" "$PZ" "the marker at the properties query's total"
ui_expect_new "Center of Mass drawn: " "drawn in the view"
ui_expect_new ", 2 bodies, 1 without a material (shown)" "two bodies, Body2 steel by default"
ui_expect_new "; 1 body without a material counted as steel" "the tooltip names the steel body"
tip=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Center of Mass: at " | tail -1 | sed 's/.*; tooltip: //')
[[ $tip =~ ^\(-?[0-9.]+,\ -?[0-9.]+,\ -?[0-9.]+\)\ mm,\ [0-9.]+\ kg\;\ 1\ body ]] ||
  ui_fail "the tooltip is not the coordinates, the mass and the steel note: '$tip'"
echo "ok   the tooltip: $tip"
ui_expect_log "Browser Root/Center of Mass at" "the Center of Mass row in the browser"
ui_expect_log "Browser eye Root/Center of Mass at" "with its light bulb"
origin=$(grep -o "^Browser Root/Origin at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.*,//')
row=$(grep -o "^Browser Root/Center of Mass at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.*,//')
[ -n "$origin" ] && [ -n "$row" ] && [ "$row" -gt "$origin" ] ||
  ui_fail "the Center of Mass row ($row) is not under Origin ($origin)"
echo "ok   the row is under Origin"
[ -z "$SHOT" ] || { ui_capture "$SHOT"; echo "ok   screenshot $SHOT"; }

echo "--- Moving a component moves the marker"
ui_mark
ui_step "click Component1:1"             ui_click_logged "Browser Root/Component1:1"
ui_expect_new "Selected: 1 component" "Component1:1 selected"
ui_step "right-click Component1:1"       ui_click_logged "Browser Root/Component1:1" 3
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Panel Move/Copy choices: move_type=components" "moving the component"
ui_expect_new "Panel Move/Copy input tx at" "the panel is laid out"
ui_step "20 mm along X"                  ui_type_in "Panel Move/Copy input tx" "20"
ui_expect_new "Preview Move/Copy: ok" "previewed" 30
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Moved 1 components (" "Component1:1 moved" 30
ui_expect_new "Center of Mass: at " "the marker's place worked out again"
MX=$(awk -v x="$PX" -v a="$M1" -v b="$M2" 'BEGIN { printf "%.12f", x + 20 * b / (a + b) }')
expect_center "$MX" "$PY" "$PZ" "the marker moved by Body2's share of 20 mm"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The light bulb, saved, Delete"
ui_mark
ui_step "hide the marker (light bulb)"   ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Visibility Root/Center of Mass: hidden" "hidden by its light bulb"
ui_expect_new "Center of Mass drawn: none" "not drawn"
ui_expect_new " without a material (hidden)" "the marker kept, hidden"
ui_mark
ui_step "show it (light bulb)"           ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Visibility Root/Center of Mass: shown" "shown again"
ui_expect_new "Center of Mass drawn: " "drawn again"
ui_mark
ui_step "hide it again (light bulb)"     ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Visibility Root/Center of Mass: hidden" "hidden"
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved " "saved"
visible=$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["display"]["center_of_mass"]["visible"])' \
  "$FILE") || ui_fail "the design file has no Center of Mass marker"
[ "$visible" = False ] || ui_fail "the design file keeps the marker shown: $visible"
echo "ok   the design file keeps the marker, hidden"
ui_stop_app

ui_start_app --open "$FILE"
ui_expect_log " without a material (hidden)" "opened again: the marker there, hidden"
ui_expect_log "Browser Root/Center of Mass at" "its row in the browser"
grep -q "^Center of Mass drawn: [-0-9]" "$UI_LOG" && ui_fail "the hidden marker is drawn"
echo "ok   not drawn"
ui_mark
ui_step "show it (light bulb)"           ui_click_logged "Browser eye Root/Center of Mass"
ui_expect_new "Center of Mass drawn: " "drawn"
ui_mark
ui_step "right-click the row"            ui_click_logged "Browser Root/Center of Mass" 3
ui_expect_new "Context menu: Hide | Delete" "its menu: Hide and Delete"
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Deleted the Center of Mass marker" "deleted"
ui_expect_new "Center of Mass drawn: none" "no longer drawn"
ui_expect_new "Center of Mass: none" "gone from the design"
ui_expect_new "Browser Root/Center of Mass out of sight" "its row gone"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Delete Center of Mass" "undo"
ui_expect_new " without a material (shown)" "the marker back, shown"
ui_expect_new "Center of Mass drawn: " "and drawn"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI Center of Mass test"
