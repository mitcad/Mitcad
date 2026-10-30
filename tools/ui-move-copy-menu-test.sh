#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowContextMenu.cpp app/browser app/commands/MoveCommands.cpp
# check-all sources: app/framework/Command.cpp app/framework/Command.hpp tools/cli
# check-all sources: core/model-api/src/analysis.rs
# Move/Copy in the context menus (mitcad#735) through the real UI, on a
# design with a body of the root (Body1, F2.b0) and a component placed
# twice, Component1:1 (O1) and Component1:2 (O2), each with Body2 (F3.b0):
#   - The browser: Move/Copy on a body's row starts with the body; on an
#     occurrence's row, two occurrences selected, it starts with both as
#     components, and OK moves them as one step. The occurrence's structure
#     move is Move to Component... (no Move to...); the root has no
#     Move/Copy.
#   - The view: a body selected in the browser, a right click on its face
#     keeps the body selected and offers Move/Copy at the menu's top (not
#     in Solid); it starts with the body, and OK moves it. An occurrence
#     selected, a right click on its body offers Move/Copy for the
#     component.
#   - A face alone has no Move/Copy.
#   - In a design of its own, a right click on a face of a large body (a
#     filleted bracket with modelled M6 tapped holes and a modelled M6
#     threaded rod, in a placed occurrence) selects it, opens the menu and
#     logs the click's timing (mitcad#981: the selection card measured a
#     moved copy of the whole body, seconds on the UI thread). No time is
#     asserted.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-copy-menu-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-move-copy-menu.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/design.mitcad

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Sketch1 (F1) 30 x 20 at the origin, Extrude1 (F2) 10 up: Body1 (F2.b0) in
# the root. Extrude2 (F3) 4 down as a new component: Component1:1 (O1,
# F3.b0) 40 mm along X, copied 50 mm along Y as Component1:2 (O2).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 50, 0]}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# A design of its own (mitcad#981): a box 60 x 30 x 20 (F1) as
# Component1:1 (O1, F1.b0), its edges filleted (F2), three modelled M6
# tapped holes through it (F3), a 6 mm cylinder 60 high joined to it (F4)
# with a modelled M6 thread (F5); the occurrence placed 50 mm along -Y.
BRACKET=$WORK/bracket.mitcad
cat > "$WORK/bracket.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 60, "width": 30, "height": 20,
    "operation": "new_component"}},
  {"cmd": "activate_component", "component": "C1"},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F1.b0",
    "sets": [{"all_edges": true, "size": {"type": "constant", "radius": 2}}]}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F1.b0", "face": "F1:top", "points": [[10, 15, 20], [22, 15, 20], [34, 15, 20]]},
    "diameter": 5, "extent": {"type": "through_all"}, "thread": {"designation": "M6x1", "modeled": true}}},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [50, 15], "diameter": 6, "height": 60,
    "operation": "join", "participants": ["F1.b0"]}},
  {"cmd": "add_feature", "def": {"type": "thread", "faces": [{"body": "F1.b0", "face": "F4:side0"}],
    "thread": {"designation": "M6x1"}, "modeled": true}},
  {"cmd": "activate_component", "component": "C0"},
  {"cmd": "set_occurrence_transform", "occurrence": "Component1:1", "transform": {"translation": [0, -50, 0]}}
]
EOF
"$CLI" run "$WORK/bracket.json" --save "$BRACKET" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli bracket"; }

# The entries of the last "Context menu<name>: ..." line since the mark,
# " | " around each.
menu_since() {
  printf '| %s |' "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Context menu$1: " | tail -1 |
    sed "s/.*Context menu$1: //" | sed "s/…/.../g")"
}
# has ENTRY / lacks ENTRY: whether the menu opened since the mark has it.
has() {
  [[ $(menu_since "") == *"| $1 |"* ]] || ui_fail "'$1' is not in the context menu: $(menu_since "")"
  echo "ok   the context menu has $1"
}
lacks() {
  [[ $(menu_since "") != *"| $1 |"* ]] || ui_fail "'$1' is in the context menu: $(menu_since "")"
  echo "ok   the context menu has no $1"
}
# no_new_selection: the right click kept the selection.
no_new_selection() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Selected: " &&
    ui_fail "the right click changed the selection: $(grep "^Selected: " "$UI_LOG" | tail -1)"
  echo "ok   the right click kept the selection"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Bodies/Body1 at" "the browser lists Body1"
ui_expect_log "Browser Root/Component1:1 at" "and Component1:1"
ui_expect_log "Browser Root/Component1:2 at" "and Component1:2"
ui_expect_log "Bodies shown: F2.b0, F3.b0 in O1, F3.b0 in O2" "Body1 and both occurrences shown"

echo "--- The browser: Move/Copy on a body's row"
ui_mark
ui_step "right-click Body1's row"        ui_click_logged "Browser Root/Bodies/Body1" 3
ui_expect_new "Context menu: " "the row's menu"
has "Move/Copy"
has "Move to Component..."
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Command Move/Copy started" "Move/Copy started"
ui_expect_new "Move/Copy Objects: 1 body [body F2.b0]" "with Body1"
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Move/Copy cancelled" "cancelled"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The browser: Move/Copy on two occurrences' rows"
ui_mark
ui_step "click Component1:1"             ui_click_logged "Browser Root/Component1:1"
ui_expect_new "Selected: 1 component" "Component1:1 selected"
ui_mark
ui_step "Ctrl+click Component1:2"        xdotool mousemove $(ui_logged_at "Browser Root/Component1:2") keydown ctrl click 1 keyup ctrl
ui_expect_new "Selected: 2 components" "both occurrences selected"
ui_mark
ui_step "right-click Component1:2"       ui_click_logged "Browser Root/Component1:2" 3
ui_expect_new "Context menu: " "the row's menu"
no_new_selection
has "Move/Copy"
has "Move to Component..."
lacks "Move to..."
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Command Move/Copy started" "Move/Copy started"
ui_expect_new "Panel Move/Copy choices: move_type=components" "moving components"
ui_expect_new "Move/Copy Objects: 2 components [component " "with both occurrences"
ui_expect_new "Panel Move/Copy input tx at" "the panel is laid out"
ui_step "20 mm along X"                  ui_type_in "Panel Move/Copy input tx" "20"
ui_expect_new "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Moved 2 components (" "both occurrences moved"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: " "one undo takes the move back"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The browser: the root has no Move/Copy"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_expect_new "Context menu: " "the root's menu"
lacks "Move/Copy"
ui_step "Escape (close the menu)"        ui_key Escape
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The view: a body selected in the browser"
ui_mark
ui_step "click Body1's row"              ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Selected: 1 body [body F2.b0]" "Body1 selected"
ui_mark
ui_step "right-click Body1 in the view"  ui_click_logged "Body F2.b0" 3
ui_expect_new "Context menu: " "the view's menu"
no_new_selection
has "Move/Copy"
[[ $(menu_since " Solid") != *"| Move/Copy |"* ]] || ui_fail "Move/Copy is in Solid too: $(menu_since " Solid")"
echo "ok   Move/Copy is not in Solid"
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Command Move/Copy started" "Move/Copy started"
ui_expect_new "Move/Copy Objects: 1 body [body F2.b0]" "with Body1"
ui_expect_new "Panel Move/Copy input tx at" "the panel is laid out"
ui_step "-20 mm along X"                 ui_type_in "Panel Move/Copy input tx" "-20"
ui_expect_new "Preview Move/Copy: ok" "previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Moved 1 bodies (" "Body1 moved"
ui_mark
ui_step "undo"                           ui_key ctrl+z
ui_expect_new "Undo: " "one undo takes the move back"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The view: an occurrence selected in the browser"
ui_mark
ui_step "click Component1:2"             ui_click_logged "Browser Root/Component1:2"
ui_expect_new "Selected: 1 component" "Component1:2 selected"
ui_mark
ui_step "right-click its body in the view" ui_click_logged "Body F3.b0 in O2" 3
ui_expect_new "Context menu: " "the view's menu"
no_new_selection
has "Move/Copy"
ui_step "choose Move/Copy"               ui_menu_choose "Move/Copy"
ui_expect_new "Command Move/Copy started" "Move/Copy started"
ui_expect_new "Panel Move/Copy choices: move_type=components" "moving the component"
ui_expect_new "Move/Copy Objects: 1 component [component " "with Component1:2"
grep "^Move/Copy Objects: 1 component " "$UI_LOG" | tail -1 | grep -qF " in O2]" ||
  ui_fail "not Component1:2: $(grep "^Move/Copy Objects: 1 component " "$UI_LOG" | tail -1)"
echo "ok   the occurrence O2"
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Move/Copy cancelled" "cancelled"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- The view: a face alone"
ui_mark
ui_step "right-click Body1 in the view"  ui_click_logged "Body F2.b0" 3
ui_expect_new "Selected: 1 face [face " "a face of Body1 selected"
ui_expect_new "Context menu: " "the view's menu"
lacks "Move/Copy"
ui_step "Escape (close the menu)"        ui_key Escape
ui_step "Escape (clear the selection)"   ui_key Escape

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The view: a face of a large body (modelled threads, placed)"
ui_stop_app
ui_start_app --open "$BRACKET"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F1.b0 in O1 at" "the bracket is in the view"
ui_mark
ui_step "offset face (search)"           ui_command "Offset Face"
ui_expect_new "Pick face F1.b0/F1:side0 " "where the bracket's side is picked"
at=$(ui_pick_at "face F1.b0/F1:side0 ")
ui_step "cancel it (Esc)"                ui_key Escape
ui_expect_new "Command Offset Face cancelled" "cancelled"
ui_mark
ui_step "right-click the bracket's side" xdotool mousemove $at click 3
ui_expect_new "Selected: 1 face [face F1:side0 of F1.b0 in O1]" "the bracket's side selected"
ui_expect_new "Selection timing: view " "the selection's parts timed"
ui_expect_new "Context menu timing: select " "the right click timed"
ui_expect_new "Context menu: " "the view's menu"
echo "     $(grep "^Context menu timing: " "$UI_LOG" | tail -1)"
ui_step "Escape (close the menu)"        ui_key Escape
ui_step "Escape (clear the selection)"   ui_key Escape

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI Move/Copy menus"
