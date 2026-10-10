#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser
# Sketches and the timeline in a design of two components side by side,
# through the real UI (mitcad#98, mitcad#99, mitcad#100):
#   - With Cover active, a sketch on the top face of Part's block: linked
#     to the face, at its height; Project takes the face into it, linked.
#   - The timeline's filter: the active component's features, the playback
#     buttons stepping over the others; a component chosen in the
#     browser; Find in Timeline on a hidden feature shows all again.
#   - Cover deleted: the root is active, and a sketch there on Part's face
#     with a projection of it works as well (what failed before, leaving
#     Project with nothing to project).
#   - The project file keeps the links.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-component-sketch-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-component-sketch.XXXXXX)
FILE=$WORK/siblings.mitcad
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

# sketch_on_top_face: Create Sketch on the top face of Part's block, seen
# from above, and the frame it gets (the face is 10 mm up).
sketch_on_top_face() {
  ui_mark
  ui_step "top view (search)"            ui_command "Top View"
  ui_step "fit (F6)"                     ui_key F6
  ui_step "pick the top face"            ui_click_logged "Body F2.b0 in O1"
  ui_expect_new "Selected: 1 face [face F2:end(" "the top face of Part's block"
  ui_step "create sketch (search)"       ui_command "Create Sketch"
  ui_expect_new 'Sketch started on {"body":"F2.b0","face":"F2:end(' "a sketch on the face"
  ui_expect_new "} in O1, origin (0, 0, 10)" "where Part:1 shows it, 10 mm up"
}

# project_top_face: Project (P) on the top face, linked, and OK.
project_top_face() {
  ui_mark
  ui_step "project (P)"                  ui_key p
  ui_step "pick the top face"            ui_sketch_click 15 10
  ui_expect_new "Project Geometry: 1 face" "the face went to the projection"
  ui_expect_new "Preview Project: ok" "projection previewed"
  ui_step "OK (Enter)"                   ui_key Return
  ui_expect_new "Projected 1 item(s)" "projected"
}

# Part:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1
# (F2.b0). Cover:1 (O2, C2), empty and active, beside it in the root.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Cover"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0 in O1" "Part's block shown"
ui_expect_log "Browser radio Root/Cover:1 at" "Cover in the browser"
ui_expect_log "Timeline filter: all components" "the timeline shows every component"

echo "--- A sketch in Cover on a face of Part (mitcad#100)"
sketch_on_top_face
project_top_face
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-sketch.png"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
ui_expect_log "Timeline Sketch2 at" "Sketch2 in the timeline"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
grep -q "belongs to Part" "$UI_LOG" && ui_fail "the sketch could not use Part's face"
echo "ok   the sketch evaluates"
save
expect_file 'doc["features"][2]["component"], doc["features"][2]["plane_link"]' \
  "('C2', {'source': 'O1', 'target': 'O2'})" "Sketch2 is Cover's, its plane linked to Part:1"
expect_file 'doc["features"][2]["projections"][0]["link"]' "{'source': 'O1', 'target': 'O2'}" \
  "the projection linked too"

echo "--- The timeline's filter (mitcad#98)"
ui_mark
ui_step "open the filter"                ui_click_logged "Timeline button filter"
ui_expect_new "Context menu: All Components | Active Component | Root | Part | Cover | Include Subcomponents" \
  "the filter's choices"
ui_step "choose Active Component"        ui_menu_choose "Active Component"
ui_expect_new "Timeline filter: Active Component (Cover): 1 of 3 features" "only Cover's sketch shown"
ui_expect_new "Timeline Sketch2 at" "Sketch2 laid out"
ui_mark
ui_step "step back"                      ui_click_logged "Timeline button back"
ui_expect_new "Marker at 2 of 3" "one step back: before Sketch2"
ui_mark
ui_step "step back"                      ui_click_logged "Timeline button back"
ui_expect_new "Marker at 0 of 3" "the next step passes over Part's features"
ui_mark
ui_step "step forward"                   ui_click_logged "Timeline button forward"
ui_expect_new "Marker at 2 of 3" "forward over them in one step"
ui_mark
ui_step "move to the end"                ui_click_logged "Timeline button end"
ui_expect_new "Marker at 3 of 3" "at the end"
ui_mark
ui_step "right-click Part:1"             ui_click_logged "Browser Root/Part:1" 3
ui_step "choose Show in Timeline Only"   ui_menu_choose "Show in Timeline Only"
ui_expect_new "Timeline filter: Part: 2 of 3 features" "Part's sketch and extrude shown"
ui_mark
ui_step "right-click Cover's sketch"     ui_click_logged "Browser Root/Cover:1/Sketches/Sketch2" 3
ui_step "choose Find in Timeline"        ui_menu_choose "Find in Timeline"
ui_expect_new "Timeline filter cleared to show Sketch2" "a hidden feature clears the filter"
ui_expect_new "Timeline filter: all components" "every feature shown again"

echo "--- Cover deleted; a sketch in the root on Part's face (mitcad#99)"
ui_mark
ui_step "right-click Cover:1"            ui_click_logged "Browser Root/Cover:1" 3
ui_step "choose Delete"                  ui_menu_choose "Delete"
ui_expect_new "Deleted Cover:1" "Cover deleted with its sketch"
ui_step "Escape"                         ui_key Escape
sketch_on_top_face
project_top_face
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[f.get("component") for f in doc["features"]], doc["features"][2]["plane_link"]' \
  "(['C1', 'C1', None], {'source': 'O1'})" "the root's sketch linked to Part:1"

ui_finish "UI component sketch test"
