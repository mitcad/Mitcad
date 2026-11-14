#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindowContextMenu.cpp app/MainWindow.cpp core/model/src/document/bindings.rs tools/cli app/files/FileCommands.cpp app/framework/QuickAccessGroup.cpp
# Show When and the browser's light bulbs (mitcad#863, after mitcad#809)
# through the real UI:
#   - A design (mitcad-cli): Box1 (Body1) and Box2 (Body2) in the root,
#     Part:1 (O1) with Box3 (its body Leg, F3.b0) and its copy Part:2 (O2);
#     the text parameter Holes ('No'). Body2 is shown when Holes == 'Yes'
#     (hidden now), Part:1 and Leg when Holes == 'No' (shown now); Body1 and
#     Part:2 have no condition.
#   - The conditioned rows' light bulbs are fx bulbs (the plain bulb with a
#     small fx corner badge) with tooltips that say what the condition
#     gives now and what a click does; the folders have the plain bulb
#     (their eyes are their own and change no body, mitcad#1771); Body2's
#     row, hidden by its condition, stays in the browser with Show When...
#     in its menu.
#   - The root's Bodies folder: Hide All and Show All (their tooltips name
#     Body2) change Body1 and leave Body2 following its condition (logged
#     with the count kept).
#   - Part:1's Bodies folder holds only a conditioned body: its Hide All is
#     disabled with the reason; its eye hides and shows the folder and
#     leaves Leg's condition.
#   - Body2's light bulb shows it and removes the condition, with the
#     status message; undo brings the condition back.
#   - The same for Part:1's light bulb (it hides Part:1).
#   - The view's Hide submenu: Hide 2 Bodies (Body1 and Leg) and Hide 2
#     Components (Part:1 and Part:2) are bulk changes that keep Leg and
#     Part:1 following their conditions (logged, status bar); Hide Body of
#     Leg alone removes its condition with the status message.
#   - Isolate is a view state of the design: it leaves the conditions.
#   - Find in Design's Hide of the found bodies and of the found components
#     keeps Body2 and Part:1 following their conditions (logged).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-show-when-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-show-when.XXXXXX)
FILE=$WORK/show-when.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

# expect_file filter expected description: a jq filter on the saved file.
expect_file() {
  local actual
  actual=$(jq -c "$1" "$FILE") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# (Nothing to save when a step changed nothing: the file holds the design.)
save() {
  ui_focus_main
  ui_save_if_changed "$FILE"
}

# expect_none "text" description: no line since the mark has the text.
expect_none() {
  ui_sync
  tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# choose_hide "entry": opens the Hide submenu of the view's context menu
# just shown and picks an entry of it, from the logged "Context menu Hide:
# a | b" (as tools/ui-hide-menu-test.sh does).
choose_hide() {
  local entries index=0 found="" item
  ui_menu_choose "Hide"
  ui_sync
  entries=$(grep "Context menu Hide: " "$UI_LOG" | tail -1 | sed 's/.*Context menu Hide: //')
  IFS='|' read -ra items <<< "$entries"
  for item in "${items[@]}"; do
    index=$((index + 1))
    item=${item# }
    item=${item% }
    if [ "$item" = "$1" ]; then
      found=$index
      break
    fi
  done
  [ -n "$found" ] || ui_fail "'$1' is not in the Hide submenu: $entries"
  [ "$found" -le 1 ] || xdotool key --delay 80 --repeat $((found - 1)) Down
  ui_key Return
}

# find_search "text": the Find in Design window's field, typed into.
find_search() {
  ui_focus_dialog '^Find in Design$'
  ui_click_logged "Find in Design field"
  ui_type_text "$1"
}

# The visibility and the condition of Body2 and of Part:1 in the saved file
# (a shown item has no "visible").
BODY2='.bodies[] | select(.uid == "F2.b0") | [(.visible != false), .visible_when]'
PART='.occurrences[] | select(.uid == "O1") | [(.visible != false), .visible_when]'
LEG='.bodies[] | select(.uid == "F3.b0") | [(.visible != false), .visible_when]'

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-45, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [5, -10],
    "length": 40, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "add_parameter", "name": "Holes", "expression": "'No'"},
  {"cmd": "create_component", "name": "Part", "transform": {"translation": [0, 40, 0]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F3.b0", "name": "Leg"},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "copy_occurrence", "occurrence": "Part:1", "transform": {"translation": [60, 40, 0]}},
  {"cmd": "set_body_visible", "uid": "F2.b0", "expression": "Holes == 'Yes'"},
  {"cmd": "set_body_visible", "uid": "F3.b0", "expression": "Holes == 'No'"},
  {"cmd": "set_occurrence_visible", "occurrence": "O1", "expression": "Holes == 'No'"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "the design: Body2 hidden by its condition"
expect_file "$PART" "[true,\"Holes == 'No'\"]" "the design: Part:1 shown by its condition"

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Body F1.b0 at" "the boxes are in the view"

echo "--- The fx light bulbs and their tooltips"
ui_expect_log "Browser fx bulb Root/Bodies/Body2 off: Shown when Holes == 'Yes'. Hidden now (Holes is 'No'). Clicking the light bulb shows Body2 and removes its condition." \
  "Body2's light bulb is an fx bulb, off, and says what a click does"
ui_expect_log "Browser fx bulb Root/Part:1 on: Shown when Holes == 'No'. Shown now (Holes is 'No'). Clicking the light bulb hides Part:1 and removes its condition." \
  "Part:1's light bulb is an fx bulb, on, and says what a click does"
grep -E "Browser (fx )?bulb Root/(Part:[12]/)?Bodies (on|off):" "$UI_LOG" &&
  ui_fail "a folder's bulb has a condition's tooltip"
echo "ok   the folders have the plain bulb: their eyes change no body (mitcad#1771)"
ui_expect_log "Browser fx Root/Bodies/Body2: Shown when Holes == 'Yes'" "Body2's row keeps its fx badge"
ui_expect_log "Browser eye Root/Bodies/Body2 at" "Body2's row, hidden, stays in the browser"
grep -F "Browser fx bulb Root/Bodies/Body1" "$UI_LOG" && ui_fail "Body1 has no condition but an fx bulb"
echo "ok   Body1's light bulb is a plain one"

echo "--- Body2's menu has Show When..."
ui_mark
ui_step "right-click Body2"                ui_click_logged "Browser Root/Bodies/Body2" 3
ui_expect_new "Context menu: " "the row's menu"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Context menu: " | tail -1 | grep -qF "Show When" ||
  ui_fail "Show When... is not in the hidden body's menu"
echo "ok   the hidden body's menu has Show When..."
ui_step "close the menu"                   ui_key Escape
ui_sync

echo "--- The root's Bodies folder: Hide All, Show All"
ui_mark
ui_step "right-click Bodies"               ui_click_logged "Browser Root/Bodies" 3
ui_expect_new "Hide All tooltip: Body2 follows its Show When and is left as it is." "Hide All's tooltip names Body2"
ui_expect_new "Context menu: Hide All" "the folder offers Hide All"
expect_none "Hide All disabled" "Hide All can be chosen"
ui_step "choose Hide All"                  ui_menu_choose "Hide All"
ui_expect_new "Hide Bodies Root/Bodies: kept 1 following Show When: Body2" "Hide All keeps Body2 (logged)"
ui_expect_new "Visibility Root/Bodies: hidden" "the folder hidden"
save
expect_file '.bodies[] | select(.uid == "F1.b0") | .visible' 'false' "Hide All hid Body1"
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "Body2 still follows its condition"
ui_mark
ui_step "right-click Bodies"               ui_click_logged "Browser Root/Bodies" 3
ui_expect_new "Show All tooltip: Body2 follows its Show When and is left as it is." "Show All's tooltip names Body2"
ui_expect_new "Context menu: Show All" "the folder offers Show All"
ui_step "choose Show All"                  ui_menu_choose "Show All"
ui_expect_new "Show Bodies Root/Bodies: kept 1 following Show When: Body2" "Show All keeps Body2 (logged)"
ui_expect_new "Visibility Root/Bodies: shown" "the folder shown"
save
expect_file '.bodies[] | select(.uid == "F1.b0") | (.visible != false)' 'true' "Show All showed Body1"
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "Body2 still hidden by its condition"

echo "--- Part:1's Bodies folder holds only a conditioned body"
ui_mark
ui_step "right-click Part:1's Bodies"      ui_click_logged "Browser Root/Part:1/Bodies" 3
ui_expect_new "Hide All disabled: Leg follows its Show When and is left as it is. Show When... on a body's row changes its condition." \
  "Hide All is disabled with the reason"
ui_step "close the menu"                   ui_key Escape
ui_sync
ui_mark
ui_step "Part:1's Bodies light bulb"       ui_click_logged "Browser eye Root/Part:1/Bodies"
ui_expect_new "Visibility Root/Part:1/Bodies: hidden" "the eye hides the folder (mitcad#1771)"
expect_none "following Show When" "the eye changes no body"
save
expect_file "$LEG" "[true,\"Holes == 'No'\"]" "Leg still follows its condition"
expect_file '.display.folders' '{"C1":{"bodies":true}}' "the folder hidden as display state"
ui_mark
ui_step "Part:1's Bodies light bulb"       ui_click_logged "Browser eye Root/Part:1/Bodies"
ui_expect_new "Visibility Root/Part:1/Bodies: shown" "the eye shows the folder again"
save
expect_file "$LEG" "[true,\"Holes == 'No'\"]" "Leg still follows its condition"
expect_file '.display.folders' 'null' "no folder hidden"

echo "--- Body2's light bulb removes the condition; undo brings it back"
ui_mark
ui_step "Body2's light bulb"               ui_click_logged "Browser eye Root/Bodies/Body2"
ui_expect_new "Visibility Root/Bodies/Body2: shown" "Body2 shown by hand"
ui_expect_new "Status: Show When of Body2 removed – Undo brings it back" "the status bar says the condition went"
save
expect_file "$BODY2" '[true,null]' "Body2 shown, its condition removed"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Show Body2" "one undo step"
ui_expect_new "Browser fx bulb Root/Bodies/Body2 off: Shown when Holes == 'Yes'." "the fx bulb is back"
save
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "undo brings the condition back"

echo "--- Part:1's light bulb removes the condition; undo brings it back"
ui_mark
ui_step "Part:1's light bulb"              ui_click_logged "Browser eye Root/Part:1"
ui_expect_new "Visibility Root/Part:1: hidden" "Part:1 hidden by hand"
ui_expect_new "Status: Show When of Part:1 removed – Undo brings it back" "the status bar says the condition went"
save
expect_file "$PART" '[false,null]' "Part:1 hidden, its condition removed"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Hide Part:1" "one undo step"
ui_expect_new "Browser fx bulb Root/Part:1 on: Shown when Holes == 'No'." "the fx bulb is back"
save
expect_file "$PART" "[true,\"Holes == 'No'\"]" "undo brings the condition back"

echo "--- The view's Hide: two bodies, one with a condition"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_expect_log "Body F3.b0 in O2 at" "Leg in Part:2 is in the view"
ui_mark
ui_step "click Body1"                      ui_click_logged "Body F1.b0"
ui_step "Ctrl+click Leg in Part:2"         xdotool mousemove $(ui_logged_at "Body F3.b0 in O2") keydown ctrl click 1 keyup ctrl
ui_step "right-click Leg in Part:2"        ui_click_logged "Body F3.b0 in O2" 3
ui_expect_new "Context menu Hide: Hide 2 Bodies" "the Hide submenu counts both bodies"
ui_step "Hide > Hide 2 Bodies"             choose_hide "Hide 2 Bodies"
ui_expect_new "Hide Bodies: kept 1 following Show When: Leg" "Leg keeps its condition (logged)"
ui_expect_new "Status: Hide Bodies left Show When in place for Leg" "the status bar says so"
ui_expect_new "Hid body Body1" "Body1 hidden"
save
expect_file '.bodies[] | select(.uid == "F1.b0") | .visible' 'false' "Body1 hidden"
expect_file "$LEG" "[true,\"Holes == 'No'\"]" "Leg still follows its condition"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Hide Body1" "one undo step"

echo "--- The view's Hide: two components, one with a condition"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "click Leg in Part:1"              ui_click_logged "Body F3.b0 in O1"
ui_step "Ctrl+click Leg in Part:2"         xdotool mousemove $(ui_logged_at "Body F3.b0 in O2") keydown ctrl click 1 keyup ctrl
ui_step "right-click Leg in Part:2"        ui_click_logged "Body F3.b0 in O2" 3
ui_expect_new "Context menu Hide: Hide Body | Hide 2 Components" "the Hide submenu counts both components"
ui_step "Hide > Hide 2 Components"         choose_hide "Hide 2 Components"
ui_expect_new "Hide Components: kept 1 following Show When: Part:1" "Part:1 keeps its condition (logged)"
ui_expect_new "Hid component Part:2" "Part:2 hidden"
save
expect_file '.occurrences[] | select(.uid == "O2") | .visible' 'false' "Part:2 hidden"
expect_file "$PART" "[true,\"Holes == 'No'\"]" "Part:1 still follows its condition"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Hide Part:2" "one undo step"

echo "--- The view's Hide Body of one body removes its condition"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "right-click Leg in Part:1"        ui_click_logged "Body F3.b0 in O1" 3
ui_expect_new "Context menu Hide: Hide Body | Hide Component" "Hide Body for the one body"
ui_step "Hide > Hide Body"                 choose_hide "Hide Body"
ui_expect_new "Hid body Leg" "Leg hidden by hand"
ui_expect_new "Status: Show When of Leg removed – Undo brings it back" "the status bar says the condition went"
save
expect_file "$LEG" '[false,null]' "Leg hidden, its condition removed"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Hide Leg" "one undo step"
save
expect_file "$LEG" "[true,\"Holes == 'No'\"]" "undo brings the condition back"

echo "--- Isolate leaves the conditions"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Isolate"                   ui_menu_choose "Isolate"
ui_expect_new "Isolated " "Body1 isolated"
save
expect_file '.bodies[] | select(.uid == "F1.b0") | (.visible != false)' 'true' "Body1's own visibility untouched"
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "Body2 still follows its condition"
expect_file "$PART" "[true,\"Holes == 'No'\"]" "Part:1 still follows its condition"
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Unisolate"                 ui_menu_choose "Unisolate"
ui_expect_new "Unisolated" "back to the whole design"

echo "--- Find in Design: Hide of the found bodies and components"
ui_mark
ui_step "click the magnifier"              ui_click_logged "Find in Design button"
ui_expect_new "Find in Design opened" "the window opened"
ui_mark
find_search "Body"
ui_expect_new "Find in Design found: " "Body1 and Body2 found"
ui_mark
ui_step "Hide"                             ui_click_logged "Find in Design hide"
ui_expect_new "Hide Found Items: kept 1 following Show When: Body2" "Body2 keeps its condition (logged)"
ui_expect_new "Find in Design: hid 1 body" "Body1 hidden"
save
expect_file '.bodies[] | select(.uid == "F1.b0") | .visible' 'false' "Body1 hidden"
expect_file "$BODY2" "[false,\"Holes == 'Yes'\"]" "Body2 still follows its condition"
ui_mark
ui_step "undo"                             ui_key ctrl+z
ui_expect_new "Undo: Hide Body1" "one undo step"
ui_mark
find_search "Part"
ui_expect_new "Find in Design found: " "Part:1 and Part:2 found"
ui_mark
ui_step "Hide"                             ui_click_logged "Find in Design hide"
ui_expect_new "Hide Found Items: kept 1 following Show When: Part:1" "Part:1 keeps its condition (logged)"
ui_expect_new "Find in Design: hid 1 component" "Part:2 hidden"
save
expect_file '.occurrences[] | select(.uid == "O2") | .visible' 'false' "Part:2 hidden"
expect_file "$PART" "[true,\"Holes == 'No'\"]" "Part:1 still follows its condition"

ui_finish "UI Show When"
