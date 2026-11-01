#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp app/framework/GlassCard.cpp app/commands/MaterialCommands.cpp tools/cli
# Find in Design (mitcad#350) through the real UI, on a design with nested
# components and bodies of the same name in several places: "Plate" in the
# root, "Plate Frame" and "Plate Leg" in the component Frame, which is placed
# twice (Frame:1, Frame:2) and holds the component Leg (one in each), and "Rod":
#   - The magnifier on the browser's title row opens the window, which
#     lists everything: 7 bodies and 4 components, each body once per place.
#   - A search for "plate" lists the matching bodies with their places (5
#     rows, 3 bodies); the wildcard text "pl?te*leg" lists two rows.
#   - Hide hides exactly those 3 bodies and no other, one undo step
#     restores them; with the filter "Hidden only" the hidden ones are found.
#   - Appearance... starts the Appearance tool with the 3 bodies, which
#     gives them one appearance in one undo step (mitcad#1059).
#   - A component search ("leg", Bodies off) finds the two occurrences;
#     Hide hides them and Show shows them again; Select selects them.
#   - Selecting a row selects it in the view; a double-click on a row reveals
#     it in the browser.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-find-in-design-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-find-in-design.XXXXXX)
FILE=$WORK/design.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Sketch1 (F1) 20 x 20 and Extrude1 (F2) 10 up: Plate (F2.b0) in the root.
# Frame (O1, C1): Sketch2 (F3), Extrude2 (F4): Plate Frame (F4.b0); Sketch3
# (F5), Extrude3 (F6): Rod (F6.b0). Leg (C2, in Frame): Sketch4 (F7), Extrude4
# (F8): Plate Leg (F8.b0). Frame is copied, so Frame:2 holds its own Leg.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Plate"},
  {"cmd": "create_component", "name": "Frame", "transform": {"translation": [0, 60, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F4.b0", "name": "Plate Frame"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [30, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F6.b0", "name": "Rod"},
  {"cmd": "create_component", "name": "Leg", "transform": {"translation": [0, 40, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F7", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F8.b0", "name": "Plate Leg"},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Frame:1", "transform": {"translation": [60, 60, 0]}},
  {"cmd": "create_appearance", "id": "red_copy", "name": "Red Copy", "based_on": "paint_red"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_file filter expected description: a jq filter on the saved project file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

save() {
  ui_focus_main
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# search "text": clicks the window's field and types into it.
search() {
  ui_focus_dialog '^Find in Design$'
  ui_click_logged "Find in Design field"
  ui_type_text "$1"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                       ui_key F6
# A tree longer than the browser opens folded below the root (mitcad#426).
ui_expect_log "Browser opened " "the browser fitted the design"
ui_sync
if grep -q "Browser opened folded" "$UI_LOG"; then
  ui_step "open Bodies"                  ui_click_logged "Browser arrow Root/Bodies"
fi
ui_expect_log "Browser Root/Bodies/Plate at" "the browser lists the root's body"
ui_expect_log "Find in Design button at" "the magnifier is on the browser's title row"

echo "--- The window lists everything, each body once per place"
ui_mark
ui_step "click the magnifier"            ui_click_logged "Find in Design button"
ui_expect_new "Find in Design opened" "the window opened"
ui_expect_new "Find in Design found: 7 bodies, 4 components" "no text: every body in each place and every occurrence"

echo "--- A name search with the bodies' places"
ui_mark
search "plate"
ui_expect_new "Find in Design found: 5 bodies, 0 components" "Plate, Plate Frame and Plate Leg in their places"
ui_mark
search "pl?te*leg"
ui_expect_new "Find in Design found: 2 bodies, 0 components" "the wildcards find the two Plate Leg bodies"
search "plate"
ui_expect_new "Find in Design found: 5 bodies, 0 components" "back to five"

echo "--- Hide hides exactly those bodies; one undo restores them"
ui_mark
ui_step "Hide"                           ui_click_logged "Find in Design hide"
ui_expect_new "Find in Design: hid 3 bodies" "the three bodies are hidden, once each"
save
expect_file '[.bodies[] | {n: .name, v: .visible}]' \
  '[{"n":"Plate","v":false},{"n":"Plate Frame","v":false},{"n":"Rod","v":null},{"n":"Plate Leg","v":false}]' \
  "only the bodies named Plate are hidden"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Hide Found Items" "one undo step brings them back"
save
expect_file '[.bodies[] | .visible]' '[null,null,null,null]' "all bodies are shown again"

echo "--- Hidden only"
ui_focus_dialog '^Find in Design$'
ui_mark
ui_step "Hide"                           ui_click_logged "Find in Design hide"
ui_expect_new "Find in Design: hid 3 bodies" "hidden again"
ui_mark
ui_step "Hidden only"                    ui_choose "Find in Design visibility" 2
ui_expect_new "Find in Design found: 5 bodies, 0 components" "the hidden bodies are found"
ui_mark
ui_step "Show"                           ui_click_logged "Find in Design show"
ui_expect_new "Find in Design: showed 3 bodies" "Show shows them"
ui_expect_new "Find in Design found: 0 bodies, 0 components" "nothing is hidden any more"
ui_step "Visible and hidden"             ui_choose "Find in Design visibility" -2
ui_expect_new "Find in Design found: 5 bodies, 0 components" "the list is whole again"

echo "--- Appearance..."
ui_mark
ui_step "Appearance..."                  ui_click_logged "Find in Design appearance"
ui_expect_new "Find in Design: appearance for 3 bodies" "the three bodies are the targets"
ui_expect_new "Appearance tool for F2.b0, F4.b0, F8.b0" "the Appearance tool for them"
ui_step "the picker: Red Copy"           ui_picker_choose "Panel Appearance input value" "Appearance picker" "Red Copy"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Appearance red_copy for 3 body(ies)" "all three got it"
save
expect_file '[.bodies[] | .appearance]' '["red_copy","red_copy",null,"red_copy"]' "one appearance for the three bodies, Rod untouched"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: Appearance" "one undo step"
save
expect_file '[.bodies[] | .appearance]' '[null,null,null,null]' "the default look is back"

echo "--- Components"
ui_focus_dialog '^Find in Design$'
ui_mark
search "leg"
ui_expect_new "Find in Design found: 2 bodies, 2 components" "the two Plate Leg bodies and the two Leg occurrences"
ui_mark
ui_step "Bodies off"                     ui_click_logged "Find in Design bodies"
ui_expect_new "Find in Design found: 0 bodies, 2 components" "only the two Leg occurrences are listed"
ui_mark
ui_step "Hide"                           ui_click_logged "Find in Design hide"
ui_expect_new "Find in Design: hid 2 components" "both occurrences are hidden"
save
expect_file '[.occurrences[] | .visible]' '[null,false,null]' \
  "the Leg occurrence is hidden in the file (one definition places it in both frames)"
ui_focus_dialog '^Find in Design$'
ui_mark
ui_step "Show"                           ui_click_logged "Find in Design show"
ui_expect_new "Find in Design: showed 2 components" "Show shows them again"
ui_mark
ui_step "Select"                         ui_click_logged "Find in Design select"
ui_expect_new "Find in Design: selected 2 components" "the occurrences are the selection"
ui_expect_new "Selected: 2 components" "the main window has them selected"
ui_step "Select None"                   ui_click_logged "Find in Design none"
ui_mark
ui_step "click the first row"           ui_click_logged "Find in Design row 1"
ui_expect_new "Selected: 1 component" "selecting a row selects it in the view"

echo "--- A double-click reveals the row in the browser"
ui_mark
ui_step "double-click the first row"     ui_double_click_logged "Find in Design row 1"
ui_expect_new "in the browser" "the row is revealed in the browser"

ui_finish "UI Find in Design"
