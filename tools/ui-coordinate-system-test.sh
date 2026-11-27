#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/files app/framework/ModelShapes.cpp
# check-all sources: core/model/src/features/coordinate_system.rs core/model/src/exchange.rs tools/cli
# Coordinate System (mitcad#1775) through the real UI, on the demo block
# (60 x 40 x 20 mm, Body1 F2.b0): made with three picks, the top front
# right corner for the origin, the top front edge for Z and the front right
# edge for X (each pointing away from the corner); drawn as a triad and
# listed as construction geometry; then File > Export writes the block in
# it to STEP (Coordinates, Alt+D), and the CLI reads the file back with the
# corner at the origin, the 20 mm edge along X and the 60 mm one along Z.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-coordinate-system-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-coordinate-system.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
TOP="F2:end($R)"
FRONT='F2:side(c1[c4,c2])'
RIGHT='F2:side(c2[c1,c3])'

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6

echo "--- Coordinate System: three picks"
ui_step "coordinate system (search)"       ui_command "Coordinate System"
ui_expect_log "Command Coordinate System started" "Coordinate System started"
ui_step "pick the top front right corner"  ui_click_pick "vertex F2.b0/V{$TOP|$FRONT|$RIGHT}"
ui_expect_log "Coordinate System Origin: 1 vertex" "the corner is the origin"
ui_step "pick the top front edge"          ui_click_pick "edge F2.b0/E{$TOP|$FRONT}"
ui_expect_log "Coordinate System Z Axis: 1 edge" "the top front edge is Z"
ui_step "pick the front right edge"        ui_click_pick "edge F2.b0/E{$FRONT|$RIGHT}"
ui_expect_log "Coordinate System X Axis: 1 edge" "the front right edge is X"
ui_expect_log "Preview Coordinate System: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added coordinate system CoordinateSystem1: origin (60, 0, 20), x (0, 0, -1), y (0, -1, 0), z (-1, 0, 0)" \
  "CoordinateSystem1 at the corner, its axes along the edges away from it"
ui_expect_log "Construction shown: CoordinateSystem1" "drawn as construction geometry"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- File > Export in CoordinateSystem1"
ui_mark
ui_key alt+f
ui_sync
ui_key e
ui_focus_dialog '^Export$'
ui_expect_new "coordinates design (design | CoordinateSystem1)" "Export offers the coordinate system"
ui_type_field "Export" "$WORK/frame.step"
ui_key alt+d
ui_key Down
ui_key Return
ui_focus_main
ui_expect_new "Exported $WORK/frame.step: step, 1 body, mm, in CoordinateSystem1" "written in the coordinate system"

cat > "$WORK/read.json" << EOF
[
  {"cmd": "import_file", "path": "$WORK/frame.step", "structure": "base"},
  {"expect": {"body": "F1.b0", "volume": 48000,
    "bbox": {"min": [0, -40, 0], "max": [20, 0, 60]}, "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/read.json" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "the STEP file is not in the coordinate system"; }
echo "ok   read back with the corner at the origin, the edges along X and Z"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI coordinate system test"
