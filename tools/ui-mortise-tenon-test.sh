#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MortiseTenonCommands.cpp core/model/src/features/mortise_tenon.rs
# check-all sources: app/framework/CommandSession.cpp tools/cli
# Mortise and Tenon (mitcad#1012) through the real UI, with the volumes the
# model reports, on two boards mitcad-cli makes: a rail 100 long, 20 across
# and 60 high (Box1 F1, Body1, 120000 mm3) whose end face F1:side1 stands
# at x = -10, and a post 40 x 12 x 40 (Box2 F2, Body2, 19200 mm3) from
# x = 0 on, in front of the middle of the rail's end, small enough that the
# end face shows round it.
#   1. Mortise and Tenon: the end face clicked, the post picked, Width 12
#      (along the rail's height) and Length 30, Thickness and Clearance as
#      they come (6 mm, 0.1 mm). OK: the rail gains the tenon, 12 6 30 =
#      2160 mm3, and the post loses the mortise 12.2 x 6.2 deep to 0.1 past
#      the tenon's tip, 12.2 6.2 20.1 = 1520.364 mm3. Both bodies changed by
#      the one feature in the timeline.
#   2. MortiseTenon1 edited to 25 long: both bodies built again (12 6 25 =
#      1800 mm3 of tenon, 12.2 6.2 15.1 = 1142.164 mm3 of mortise).
# The Windows counterpart is tools/ui-windows-mortise-tenon-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mortise-tenon-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-mortise-tenon.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [-110, 0], "length": 100, "width": 20, "height": 60, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 4], "length": 40, "width": 12, "height": 40, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/boards.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli model"; }

ui_start_display
ui_start_app --open "$WORK/boards.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the boards shown"

echo "--- 1. A tenon on the rail's end, its mortise in the post"
ui_mark
ui_step "Mortise and Tenon"                ui_command "Mortise and Tenon"
ui_expect_new "Command Mortise and Tenon started" "the panel opened"
ui_expect_new "Pick places:" "the picks are logged"
ui_step "click the rail's end face"        ui_click_pick "face F1.b0/F1:side1"
ui_expect_new "Mortise and Tenon End Face: 1 face" "the end face picked"
ui_step "pick the post"                    ui_click_logged "Body F2.b0"
ui_expect_new "Mortise and Tenon Mortise Body: 1 body [body F2.b0]" "the post picked"
ui_mark
ui_step "width 12"                         ui_type_in "Panel Mortise and Tenon input width" "12"
ui_step "length 30"                        ui_type_in "Panel Mortise and Tenon input length" "30"
ui_expect_new "Preview Mortise and Tenon: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added mortise and tenon (blind)" "the joint added"
ui_expect_volume "Body Body1 (F1.b0): volume 120000.000 -> \([0-9.]*\) mm3" 122160 "the tenon on the rail"
ui_expect_volume "Body Body2 (F2.b0): volume 19200.000 -> \([0-9.]*\) mm3" 17679.636 "the mortise in the post"
ui_expect_log "Timeline MortiseTenon1 at " "the joint in the timeline"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 2. Edited to 25 long: both bodies again"
ui_mark
ui_step "edit MortiseTenon1"               ui_double_click_logged "Timeline MortiseTenon1"
ui_expect_new "Editing F3 with Mortise and Tenon" "the joint opens for editing"
ui_mark
ui_step "length 25"                        ui_type_in "Panel Mortise and Tenon input length" "25"
ui_expect_new "Preview Mortise and Tenon: ok" "the edit previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "the joint edited"
ui_expect_volume "Body Body1 (F1.b0): volume 122160.000 -> \([0-9.]*\) mm3" 121800 "the shorter tenon"
ui_expect_volume "Body Body2 (F2.b0): volume 17679.636 -> \([0-9.]*\) mm3" 18057.836 "the shallower mortise"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI mortise and tenon test"
