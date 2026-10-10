#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/browser
# Combine with a tool of another component, through the real UI
# (mitcad#104, mitcad#105): a block in the root and a block of Part:1
# beside it, overlapping.
#   - Combine with the root's block as the target and Part's block, picked
#     where Part:1 shows it, as the tool: the tool comes in through a link
#     and joins the target.
#   - Edited to a cut: the panel takes the tool back where it was picked.
#   - The project file keeps the link.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-component-combine-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-component-combine.XXXXXX)
FILE=$WORK/overlap.mitcad
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

# Part:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: F2.b0. The
# root, active: Sketch2 (F3) 20 x 20 at x = 20, Extrude2 (F4) 15 up:
# F4.b0, overlapping Part's block by 10 x 20 x 10 mm.
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [20, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"

echo "--- Combine with Part's block as the tool"
ui_mark
ui_step "combine (search)"               ui_command "Combine"
ui_step "pick the root's block"          ui_click_logged "Body F4.b0"
ui_expect_new "Combine Target Body: 1 body [body F4.b0]" "the target picked"
ui_step "pick Part's block"              ui_click_logged "Body F2.b0 in O1"
ui_expect_new "Combine Tool Bodies: 1 body [body F2.b0 in O1]" "the tool picked where Part:1 shows it"
ui_expect_new "Preview Combine: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                     ui_key Return
# 6000 + 6000 - 2000 mm3.
ui_expect_volume "Body [A-Za-z0-9]* (F4.b0): volume 6000.000 -> \([0-9.]*\) mm3" 10000 "joined with Part's block"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][4]["type"], doc["features"][4]["tool_links"]' \
  "('combine', {'F2.b0': {'source': 'O1'}})" "the tool linked to Part:1"

echo "--- Edited to a cut"
ui_mark
ui_step "double-click Combine1"          ui_double_click_logged "Timeline Combine1"
ui_expect_new "Editing F5 with Combine" "editing the combine"
ui_step "Cut"                            ui_choose "Panel Combine input operation" 1
# The tool where it was picked again: the preview reads it through the link.
ui_expect_new "Preview Combine: ok" "previewed with the linked tool"
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F5" "edited"
ui_expect_volume "Body [A-Za-z0-9]* (F4.b0): volume [0-9.]* -> \([0-9.]*\) mm3" 4000 "Part's block cut away"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
save
expect_file 'doc["features"][4]["operation"], doc["features"][4]["tool_links"]' \
  "('cut', {'F2.b0': {'source': 'O1'}})" "a cut, still linked"

ui_finish "UI component combine test"
