#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp core/model-api/src/analysis.rs core/cpp/bridge/analysis.cpp geometry/analysis/src/section.cpp tools/cli
# Section Analysis with a plane past every body (mitcad#824). A Distance
# that puts the plane beyond the bodies removes them all from the view, or
# leaves them whole, and the panel showed only zero totals. It now says
# which way the plane misses them.
# A 20 mm block (Body1, x = 0 to 20), Section Analysis at the YZ plane:
#   - Distance 10 cuts the block: the area and the outline as before;
#   - Distance 30 leaves it behind the plane: "it cuts nothing";
#   - Distance -30 puts it on the plane's normal side: "it cuts them all
#     away";
#   - Distance 10 again: the area again.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-beyond-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-section-beyond.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the block shown"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
# YZ from the browser: the hidden origin stays hidden (mitcad#1030).
ui_pick_origin_row YZ
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_mark
ui_step "10 along X"                       ui_type_in "Panel Section Analysis input offset" "10"
ui_expect_new "Inspect Section Analysis: area 400, length 80" "the cut through the block at x = 10"

ui_mark
ui_step "30 along X"                       ui_type_in "Panel Section Analysis input offset" "30"
ui_expect_new "Inspect Section Analysis: The plane is beyond the bodies: it cuts nothing." \
  "past the block, the block behind the plane: the panel says it cuts nothing"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect Section Analysis: area 0, length 0" &&
  ui_fail "zero totals shown instead of the warning"
echo "ok   no zero totals"

ui_mark
ui_step "-30 along X"                      ui_type_in "Panel Section Analysis input offset" "-30"
ui_expect_new "Inspect Section Analysis: The plane is beyond the bodies: it cuts them all away. Change Distance or turn on Flip." \
  "past the block on the normal's side: the panel says it cuts it all away"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Inspect Section Analysis: area 0, length 0" &&
  ui_fail "zero totals shown instead of the warning"
echo "ok   no zero totals"

ui_mark
ui_step "10 along X again"                 ui_type_in "Panel Section Analysis input offset" "10"
ui_expect_new "Inspect Section Analysis: area 400, length 80" "the area again once the plane cuts the block"
ui_step "Cancel (Escape)"                  ui_key Escape
ui_expect_new "Command Section Analysis cancelled" "closed"

ui_finish "UI section beyond test"
