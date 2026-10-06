#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The browser (U3) through the real UI, on a part with a component placed
# 40 mm along X:
#   - The rows' layout (mitcad#10): a compact width; each light bulb just
#     before its row's icon and name, indented with the row; a component's
#     radio button right after its name.
#   - Light bulbs hide and show a body, a sketch, a plane and an occurrence;
#     the project file keeps them, undo takes them back.
#   - Rename in place (double-click, the context menu), activate a component
#     with its radio button.
#   - Selecting a row selects in the view; a face of the placed body is
#     picked in the view with its occurrence, and Find in Browser selects
#     its body's row.
#   - The context menu: Isolate, Copy and Paste (a new occurrence, placed
#     with Move/Copy: OK keeps it there, Cancel takes it back), Paste
#     New (a new component), Ground, Delete, New Component, Create Sketch on
#     an origin plane, Look At; the document's units.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-browser.XXXXXX)
FILE=$WORK/assembly.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# expect_file expression expected description: a Python expression on the
# saved project file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1; Extrude2 (F3) 4 down as
# a new component, Component1:1 (O1) moved 40 mm along X: Body2; Plane1 (F4);
# Plane2 (F5) in Component1, below Body2.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 15}}},
  {"cmd": "add_feature", "component": "Component1", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": -10}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Bodies/Body1 at" "the browser lists Body1"
ui_expect_log "Browser Root/Component1:1/Bodies/Body2 at" "and Body2 in Component1:1"
ui_expect_log "Bodies shown: F2.b0, F3.b0 in O1" "both bodies shown"
ui_expect_log "Browser sketch DOF: Sketch1 0" "Sketch1 is fully constrained: a lock on its icon (P9)"

echo "--- The rows' layout (mitcad#10)"
width=$(grep -o "^Browser width [0-9]*.*" "$UI_LOG" | tail -1)
[[ $width =~ ^Browser\ width\ ([0-9]+)$ ]] || ui_fail "no browser width logged, or it scrolls sideways: '$width'"
width=${BASH_REMATCH[1]}
[ "$width" -ge 200 ] && [ "$width" -le 260 ] || ui_fail "the browser opens $width px wide"
echo "ok   the browser opens $width px wide, without a horizontal scroll bar"
# Each light bulb is on its row, just before the icon and the name, and
# moves in with the row's depth.
last_eye=0
for path in Root/Bodies Root/Bodies/Body1 Root/Component1:1 Root/Component1:1/Bodies \
  Root/Component1:1/Bodies/Body2; do
  read -r ex ey <<< "$(ui_logged_at "Browser eye $path")"
  read -r nx ny <<< "$(ui_logged_at "Browser $path")"
  [ "$ey" = "$ny" ] && [ "$ex" -lt "$nx" ] && [ $((nx - ex)) -lt 90 ] ||
    ui_fail "the light bulb of $path ($ex,$ey) is not next to its name ($nx,$ny)"
  case $path in
    */Body1 | */Body2 | */Component1:1/Bodies)
      [ "$ex" -gt "$last_eye" ] || ui_fail "the light bulb of $path is not indented" ;;
  esac
  last_eye=$ex
done
echo "ok   the light bulbs are before their rows' names, indented with them"
# The radio buttons follow their names, well inside the panel.
for path in Root Root/Component1:1; do
  read -r rx ry <<< "$(ui_logged_at "Browser radio $path")"
  read -r nx ny <<< "$(ui_logged_at "Browser $path")"
  [ "$ry" = "$ny" ] && [ "$rx" -gt "$nx" ] && [ $((rx - nx)) -lt 80 ] && [ "$rx" -lt $((width - 20)) ] ||
    ui_fail "the radio button of $path ($rx,$ry) does not follow its name ($nx,$ny)"
done
echo "ok   the radio buttons follow the components' names"

echo "--- Light bulbs"
ui_mark
ui_step "hide Body1"                     ui_click_logged "Browser eye Root/Bodies/Body1"
ui_expect_new "Visibility Root/Bodies/Body1: hidden" "Body1 hidden"
ui_expect_new "Bodies shown: F3.b0 in O1" "only Body2 is shown"
ui_step "show Sketch1 (used, so hidden)" ui_click_logged "Browser eye Root/Sketches/Sketch1"
ui_expect_new "Visibility Root/Sketches/Sketch1: shown" "Sketch1 shown"
ui_step "hide Plane1"                    ui_click_logged "Browser eye Root/Construction/Plane1"
ui_expect_new "Visibility Root/Construction/Plane1: hidden" "Plane1 hidden"
ui_step "hide Component1:1"              ui_click_logged "Browser eye Root/Component1:1"
ui_expect_new "Bodies shown: none" "nothing shown"
save
expect_file '[b.get("visible") for b in doc["bodies"]]' "[False, None]" "Body1 hidden in the file"
expect_file '[f.get("visible") for f in doc["features"]]' "[True, None, None, False, None]" \
  "Sketch1 shown and Plane1 hidden in the file"
expect_file 'doc["occurrences"][0].get("visible")' "False" "Component1:1 hidden in the file"
for step in "Hide Component1:1" "Hide Plane1" "Show Sketch1" "Hide Body1"; do
  ui_mark
  ui_step "undo"                         ui_key ctrl+z
  ui_expect_new "Undo: $step" "undo: $step"
done
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1" "both bodies shown again"

echo "--- Rename and activate"
ui_mark
ui_step "double-click Body1"             ui_double_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Renaming Root/Bodies/Body1" "Body1 renamed in place"
ui_step "type Plate, Enter"              xdotool type --delay 40 "Plate"
ui_key Return
ui_expect_new "Renamed Body1 to Plate" "Body1 is Plate"
ui_expect_new "Browser Root/Bodies/Plate at" "the row shows the new name"
ui_mark
ui_step "right-click Component1:1"       ui_click_logged "Browser Root/Component1:1" 3
ui_step "choose Rename"                  ui_menu_choose "Rename"
ui_expect_new "Renaming Root/Component1:1" "the component renamed in place"
ui_step "type Bracket, Enter"            xdotool type --delay 40 "Bracket"
ui_key Return
ui_expect_new "Renamed Component1:1 to Bracket" "Component1 is Bracket"
ui_expect_new "Browser radio Root/Bracket:1 at" "its occurrence is Bracket:1"
ui_mark
ui_step "activate Bracket (radio)"       ui_click_logged "Browser radio Root/Bracket:1"
ui_expect_new "Activated Bracket" "Bracket active"
save
expect_file 'doc.get("active_component"), [b["name"] for b in doc["bodies"]]' "('C1', ['Plate', 'Body2'])" \
  "the active component and the new names in the file"
ui_mark
ui_step "activate Root (radio)"          ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root active again"

echo "--- Selection both ways"
ui_mark
ui_step "click the Plate row"            ui_click_logged "Browser Root/Bodies/Plate"
ui_expect_new "Selected: 1 body [body F2.b0]" "the row selected the body in the view"
ui_mark
ui_step "click Body2 in the view"        ui_click_logged "Body F3.b0 in O1"
ui_expect_new "Selected: 1 face [face F3:" "a face of the placed body picked"
ui_expect_new " of F3.b0 in O1]" "with its occurrence"
ui_expect_new "Browser selected: nothing" "a face has no row"
ui_mark
ui_step "right-click Body2 in the view"  ui_click_logged "Body F3.b0 in O1" 3
ui_step "choose Find in Browser"         ui_menu_choose "Find in Browser"
ui_expect_new "Browser selected: Root/Bracket:1/Bodies/Body2" "Find in Browser selected its row"
ui_mark
ui_step "click the component's plane"    ui_click_logged "Browser Root/Bracket:1/Construction/Plane2"
ui_expect_new "Selected: 1 plane [plane F5 in O1]" "a datum of the placed component, with its occurrence"

echo "--- Isolate"
ui_mark
ui_step "right-click Body2's row"        ui_click_logged "Browser Root/Bracket:1/Bodies/Body2" 3
ui_step "choose Isolate"                 ui_menu_choose "Isolate"
ui_expect_new "Bodies shown: F3.b0 in O1" "only Body2 shown"
# Saved in the document (P9), and back when it opens.
save
expect_file 'doc["display"]' "{'isolated': [{'body': 'F3.b0', 'occurrence': 'O1'}]}" "the isolation saved"
ui_stop_app
ui_start_app --open "$FILE"
ui_expect_log "Bodies shown: F3.b0 in O1" "still isolated after opening the file"
ui_mark
ui_step "right-click Body2's row"        ui_click_logged "Browser Root/Bracket:1/Bodies/Body2" 3
ui_step "choose Unisolate"               ui_menu_choose "Unisolate"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1" "all shown again"

echo "--- Copy, Paste, Paste New, Ground, Delete, New Component"
ui_mark
ui_step "right-click Bracket:1"          ui_click_logged "Browser Root/Bracket:1" 3
ui_step "choose Copy"                    ui_menu_choose "Copy"
ui_expect_new "Copied Bracket:1" "copied"
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose Paste"                   ui_menu_choose "Paste"
ui_expect_new "Pasted Bracket:1 as Bracket:2" "a second occurrence"
# Move/Copy opens on the copy (P9).
ui_expect_new "Command Move/Copy started" "Move/Copy opens on it"
ui_expect_new "Panel Move/Copy choices: move_type=components" "moving the component"
ui_expect_new "Panel Move/Copy input tx at" "the panel is laid out"
ui_step "50 mm along X"                  ui_type_in "Panel Move/Copy input tx" "50"
ui_expect_new "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Placed Bracket:2 at (90, 0, 0)" "placed where it was moved, 50 mm from Bracket:1"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "Body2 shown twice"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Timeline Move" && ui_fail "the paste left a Move in the timeline"
echo "ok   no Move in the timeline: the copy's own placement"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Paste Bracket:2" "paste and placement are one undo step"
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "pasted again"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose Paste"                   ui_menu_choose "Paste"
ui_expect_new "Command Move/Copy started" "Move/Copy opens again"
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Paste of Bracket:3 cancelled" "Cancel takes the paste back"
ui_expect_new "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "two occurrences again"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose Paste New"               ui_menu_choose "Paste New"
ui_expect_new "Pasted Bracket:1 (new component) as" "a new component with its own definition"
ui_expect_new "Command Move/Copy started" "Move/Copy opens on it too"
ui_step "keep it there (Enter)"          ui_key Return
ui_expect_new "Placed " "placed"
ui_mark
ui_step "right-click Bracket:2"          ui_click_logged "Browser Root/Bracket:2" 3
ui_step "choose Ground"                  ui_menu_choose "Ground"
ui_expect_new "Ground Bracket:2" "grounded"
save
expect_file '[o.get("grounded", False) for o in doc["occurrences"]]' "[False, True, False]" \
  "Bracket:2 grounded in the file"
ui_mark
ui_step "right-click Bracket:2"          ui_click_logged "Browser Root/Bracket:2" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Deleted Bracket:2" "the occurrence deleted"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose New Component"           ui_menu_choose "New Component"
ui_expect_new "New component in Root" "a new component"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Browser radio Root/Component[0-9]+:1 at" ||
  ui_fail "the new component has no row"
echo "ok   its row is there"

echo "--- The origin: Create Sketch, Look At"
ui_mark
ui_step "show the origin"                ui_click_logged "Browser eye Root/Origin"
ui_expect_new "Visibility Root/Origin: shown" "origin shown"
ui_expect_new "Datum xy at" "the origin planes are in the view"
save
expect_file 'doc["display"]["origin"]' "True" "the Origin's light bulb saved (P9)"
ui_step "select the Origin row"          ui_click_logged "Browser Root/Origin"
ui_step "expand it (Right)"              ui_key Right
ui_expect_new "Browser Root/Origin/XY at" "the origin's rows"
ui_mark
ui_step "right-click XY"                 ui_click_logged "Browser Root/Origin/XY" 3
ui_step "choose Create Sketch"           ui_menu_choose "Create Sketch"
ui_expect_new "Sketch started on xy" "a sketch on XY from the browser"
ui_step "finish the empty sketch"        ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch left"
ui_mark
ui_step "right-click Plane1"             ui_click_logged "Browser Root/Construction/Plane1" 3
ui_step "choose Look At"                 ui_menu_choose "Look At"
ui_expect_new "Look at plane F4" "looking at Plane1"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Units"
ui_step "select Document Settings"       ui_click_logged "Browser Root/Document Settings"
ui_step "expand it (Right)"              ui_key Right
ui_expect_log "Browser Root/Document Settings/Units: mm at" "the unit row"
ui_step "double-click the unit"          ui_double_click_logged "Browser Root/Document Settings/Units: mm"
ui_focus_dialog '^Units$'
ui_mark
ui_step "choose in (Down x3, Enter)"     ui_key Down Down Down Return
ui_focus_main
ui_expect_new "Units: in" "the document's unit is the inch"
ui_expect_new "Browser Root/Document Settings/Units: in at" "the row shows it"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI browser test"
