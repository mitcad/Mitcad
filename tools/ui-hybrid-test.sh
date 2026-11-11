#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/browser app/commands core/model
# Hybrid assemblies (mitcad#171) through the real UI, on a block modelled
# in the root:
#   - New Component asks first: Move into a New Component puts the block's
#     sketch and extrude into a component of their own, the block staying
#     where it was on screen, then makes the new component; undo takes both
#     back.
#   - Continue as Is leaves the root's features where they are.
#   - Create Components from Bodies asks too: moved, the body's new
#     component gets the features that made it.
#   - "Don't ask again for this design" (the browser's New Component) is
#     saved with the design: the next New Component does not ask.
#   - Move to Component on a feature in the timeline moves it with what
#     goes with it into an explicitly named new component.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hybrid-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-hybrid.XXXXXX)
FILE=$WORK/block.mitcad
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

save() {
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# The block's place in the window, from the last "Body <uid>[ in <path>] at
# x,y" line.
place() {
  grep -E "Body $1( in O[0-9]+)? at " "$UI_LOG" | tail -1 | sed -n 's/.* at \([0-9-]*,[0-9-]*\).*/\1/p'
}

# ui_answer "keys" description: answers the hybrid assembly question.
ui_answer() {
  ui_focus_dialog '^Hybrid Assembly$'
  ui_step "$2" ui_key $1
  ui_focus_main
}

# Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1 (F2.b0), in the root.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown in the root"
ui_sync
BEFORE=$(place F2.b0)
[ -n "$BEFORE" ] || ui_fail "no place logged for the block"

echo "--- New Component: Move into a New Component"
ui_mark
ui_step "new component (search)"         ui_command "New Component"
ui_step "name it Lid"                    ui_type_in "Panel New Component input name" "Lid"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Hybrid assembly question: Root has Body1" "the question"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_answer Return "move (Enter)"
ui_expect_new "Hybrid assembly: move" "moving"
ui_expect_new "Moved Sketch1, Extrude1 into Component1" "the block's features moved"
ui_expect_new "New component Lid" "then the new component"
ui_expect_new "Bodies shown: F2.b0 in O1" "the block shown in Component1:1"
ui_sync
AFTER=$(place F2.b0)
[ "$AFTER" = "$BEFORE" ] || ui_fail "the block moved on screen: $BEFORE -> $AFTER"
echo "ok   the block stays where it was ($AFTER)"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file '[f.get("component", "C0") for f in doc["features"]]' "['C1', 'C1']" \
  "Sketch1 and Extrude1 in Component1"
expect_file '[c["name"] for c in doc["components"]]' "['Component1', 'Lid']" "Component1 and Lid"
expect_file '[(o["component"], o["parent"], o.get("grounded", False)) for o in doc["occurrences"]]' \
  "[('C1', 'C0', True), ('C2', 'C0', False)]" "both in the root, Component1 grounded where it is"

echo "--- Undo, then Continue as Is"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: New Component Lid" "the new component undone"
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Move Features of Root into Component1" "the move undone"
ui_expect_new "Bodies shown: F2.b0" "the block in the root again"
ui_mark
ui_step "new component (search)"         ui_command "New Component"
ui_step "name it Lid"                    ui_type_in "Panel New Component input name" "Lid"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Hybrid assembly question: Root has Body1" "asked again"
ui_answer alt+o "continue as is (Alt+O)"
ui_expect_new "Hybrid assembly: continue as is" "continuing as is"
ui_expect_new "New component Lid" "the new component"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Moved " && ui_fail "features moved after Continue as Is"
save
expect_file '[f.get("component", "C0") for f in doc["features"]]' "['C0', 'C0']" \
  "Sketch1 and Extrude1 stay in the root"
expect_file '[c["name"] for c in doc["components"]]' "['Lid']" "only Lid"

echo "--- Create Components from Bodies: Move with Their Features"
ui_mark
ui_step "right-click Body1"              ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Create Components from Bodies" ui_menu_choose "Create Components from Bodies"
ui_expect_new "Hybrid assembly question: Root has Body1" "asked for the body"
ui_answer Return "move with its features (Enter)"
ui_expect_new "Moved Sketch1, Extrude1 into Body1" "the body's component has its history"
ui_expect_new "Bodies shown: F2.b0 in O2" "the block shown in Body1:1"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Component from Body1" &&
  ui_fail "a component from bodies was made as well"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Move Features of Root into Body1" "one undo step"

echo "--- Don't ask again for this design"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose New Component"           ui_menu_choose "New Component"
ui_expect_new "Hybrid assembly question: Root has Body1" "the browser's New Component asks"
ui_focus_dialog '^Hybrid Assembly$'
ui_step "don't ask again (Alt+D)"        ui_key alt+d
ui_step "continue as is (Alt+O)"         ui_key alt+o
ui_focus_main
ui_expect_new "Hybrid assembly: continue as is, don't ask again" "continuing, not to ask again"
ui_expect_new "Hybrid assembly allowed in this design" "allowed for the design"
ui_expect_new "New component in Root" "a new component"
ui_mark
ui_step "right-click Root"               ui_click_logged "Browser Root" 3
ui_step "choose New Component"           ui_menu_choose "New Component"
ui_expect_new "New component in Root" "another new component"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Hybrid assembly question" && ui_fail "asked again"
echo "ok   not asked again"
save
expect_file 'doc.get("hybrid_assembly")' "True" "the design allows a hybrid assembly"

echo "--- Move to Component on a feature in the timeline"
# The new component is active, and the timeline starts at Active Component
# (mitcad#1662): the root active again shows its Extrude1.
ui_mark
ui_step "activate Root (radio)"          ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root active again"
ui_expect_new "Timeline filter: Active Component (Root) with Subcomponents: " "every feature in the timeline"
ui_expect_new "Timeline Extrude1 at" "Extrude1 laid out in the timeline"
ui_mark
ui_step "right-click Extrude1"           ui_click_logged "Timeline Extrude1" 3
ui_step "choose Move to Component..."    ui_menu_choose "Move to Component..."
ui_focus_dialog '^Move to Component$'
ui_step "Create New (Alt+N)"             ui_key alt+n
ui_step "parent Root (Alt+P, Home)"      ui_key alt+p Home
ui_step "name field (Alt+A)"             ui_key alt+a
ui_step "name it Body1"                 ui_type_text "Body1"
ui_step "move (Alt+M)"                   ui_key alt+m
ui_focus_main
ui_expect_new "Moved Sketch1, Extrude1 into Body1" "moved into a component named after its body"
ui_expect_new "Bodies shown: F2.b0 in O4" "the block shown in Body1:1"
ui_sync
AFTER=$(place F2.b0)
[ "$AFTER" = "$BEFORE" ] || ui_fail "the block moved on screen: $BEFORE -> $AFTER"
echo "ok   the block stays where it was"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI hybrid assembly test"
