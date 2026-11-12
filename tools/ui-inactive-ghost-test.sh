#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/PickContext.cpp app/PickContext.hpp app/MainWindow.cpp
# check-all sources: app/view/ViewController.cpp app/view/ViewController.hpp app/browser app/sketch tools/cli
# Everything outside the active component drawn as ghosts, and the active
# component first in picks (mitcad#420), through the real UI, on a design
# mitcad-cli makes: Part:1 (O1, C1) with a 30 x 20 x 10 block, Shield:1
# (O2, C2) with a 40 x 2 x 20 plate in front of it (y = -12 to -10), and
# Cover:1 (O3, C3) empty and active, saved so (the policies as in
# tools/ui-component-sketch-test.sh: Cover's sketches may lie on Part's
# faces):
#   - Opened with Cover active, as saved (mitcad#1678): Part's block and
#     the shield are ghosts.
#   - A ghost stays pickable: a click on Part's block from the top selects
#     its top face; a sketch of Cover on that face; Project's hover shows
#     the face under the pointer and a click projects it.
#   - Activating the root draws everything normally; undo of the
#     activation brings the ghosts back, redo takes them away.
#   - Part active: its block is drawn normally, the shield is a ghost.
#     Seen from the front, a click where the shield covers the block takes
#     the block's front face (the active component first); with Preferences
#     > Display, Pick the active component first off, the shield's face.
#   - Preferences > Display, Ghost inactive components off: no ghosts with
#     Cover active; on again: the ghosts are back. The settings are saved.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-inactive-ghost-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-inactive-ghost.XXXXXX)
FILE=$WORK/ghosts.mitcad
CONF=$UI_CONFIG/Mitcad/Mitcad.conf
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Part:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1
# (F2.b0). Shield:1 (O2, C2): Box1 (F3, F3.b0). Cover:1 (O3, C3), empty and
# active, beside them in the root.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Shield"},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-5, -12],
    "length": 40, "width": 2, "height": 20, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cover"},
  {"cmd": "set_policy", "component": "Part", "policy": {"attachments_from_others": true, "components": false}},
  {"cmd": "set_policy", "component": "Cover", "policy": {"attach_to_others": true}},
  {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# preferences "key" "logged line": Preferences > Display, a check box
# toggled by its key, OK.
preferences() {
  ui_mark
  ui_step "preferences, Display (search)" ui_command "Preferences: Display"
  ui_focus_dialog '^Preferences$'
  ui_step "toggle ($1)"                   ui_key "$1"
  ui_step "OK (Enter)"                    ui_key Return
  ui_focus_main
  ui_expect_new "$2" "${2#Preferences: }"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "F2.b0 in O1" "Part's block shown"
ui_expect_log "F3.b0 in O2" "the shield shown"
ui_expect_log "Opened with C3 active, as saved (unmodified)" "opened with Cover active, as saved (mitcad#1678)"
ui_expect_log "View inactive ghosts: 2 bodies (active Cover:1)" "the block and the shield are ghosts"

echo "--- A ghost stays pickable: a sketch on its face, a projection of it"
ui_mark
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_new "Camera direction 0 0 -1" "looking down"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "click the block's ghost"        ui_click_logged "Body F2.b0 in O1"
ui_expect_new "Selected: 1 face [face F2:end(" "the ghost's top face selected"
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new 'Sketch started on {"body":"F2.b0","face":"F2:end(' "a sketch on the ghost's face"
ui_expect_new "} in O1, origin (0, 0, 10)" "where Part:1 shows it, 10 mm up"
ui_expect_log "Sketch Sketch2 is in Cover" "the sketch is Cover's"
ui_mark
ui_step "project (P)"                    ui_key p
ui_step "pointer off the block"          xdotool mousemove $(ui_sketch_at 15 -18)
ui_step "pointer over the ghost's face"  xdotool mousemove $(ui_sketch_at 15 10)
ui_expect_new "Sketch hover: face F2:end(" "the hover shows the ghost's face"
ui_step "pick the ghost's face"          ui_sketch_click 15 10
ui_expect_new "Project Geometry: 1 face" "the ghost's face went to the projection"
ui_expect_new "Preview Project: ok" "projection previewed"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Projected 1 item(s)" "projected"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Activation, undo and redo"
ui_mark
ui_step "activate Root (radio)"          ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root active"
ui_expect_new "View inactive ghosts: none" "nothing is a ghost with the root active"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Activate Root" "the activation undone"
ui_expect_new "View inactive ghosts: 2 bodies (active Cover:1)" "Cover active again: the ghosts again"
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Activate Root" "the activation redone"
ui_expect_new "View inactive ghosts: none" "nothing is a ghost again"
ui_mark
ui_step "activate Part (radio)"          ui_click_logged "Browser radio Root/Part:1"
ui_expect_new "Activated Part" "Part active"
ui_expect_new "View inactive ghosts: 1 body (active Part:1)" "Part's own block normal, the shield a ghost"

echo "--- The active component first in picks"
ui_mark
ui_step "front view (search)"            ui_command "Front View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_new "Camera direction 0 1 0" "looking from the front: the shield covers the block"
ui_mark
ui_step "click where the shield covers the block" ui_click_logged "Body F2.b0 in O1"
ui_expect_new "Pick priority: active Part:1" "the active component's item taken first"
ui_expect_new "Selected: 1 face [face F2:side(" "the block's front face, behind the shield's ghost"
ui_step "clear the selection (Esc)"      ui_key Escape
preferences alt+p "Preferences: active component first in picks off"
grep -q '^activePickPriority=false' "$CONF" || ui_fail "the pick priority is not saved off"
echo "ok   saved off"
ui_mark
ui_step "click there again"              ui_click_logged "Body F2.b0 in O1"
ui_expect_new "of F3.b0 in O2]" "the nearest item, the shield's face, taken"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Pick priority:" && ui_fail "a pick priority with the setting off"
echo "ok   no pick priority"
ui_step "clear the selection (Esc)"      ui_key Escape
preferences alt+p "Preferences: active component first in picks on"
grep -q '^activePickPriority=true' "$CONF" || ui_fail "the pick priority is not saved on"
echo "ok   saved on"

echo "--- Preferences: Ghost inactive components"
ui_mark
ui_step "activate Cover (radio)"         ui_click_logged "Browser radio Root/Cover:1"
ui_expect_new "Activated Cover" "Cover active"
ui_expect_new "View inactive ghosts: 2 bodies (active Cover:1)" "the block and the shield ghosts"
preferences alt+g "Preferences: ghost inactive components off"
ui_expect_new "View inactive ghosts: none" "no ghosts with Cover active"
grep -q '^ghostInactive=false' "$CONF" || ui_fail "the setting is not saved off"
echo "ok   saved off"
preferences alt+g "Preferences: ghost inactive components on"
ui_expect_new "View inactive ghosts: 2 bodies (active Cover:1)" "the ghosts again"
grep -q '^ghostInactive=true' "$CONF" || ui_fail "the setting is not saved on"
echo "ok   saved on"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI inactive ghost test"
