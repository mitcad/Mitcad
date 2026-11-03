#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PlaceInputs.cpp app/commands/PrimitiveCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/CommandSession.cpp app/MainWindow.cpp tools/cli
# A box on a construction plane written with `plane` and `corner` (before
# the `at` placement), moved in its edit panel (mitcad#1140): saved as `at`
# on the same plane, it keeps the plane's own x axis and only moves. Before
# the fix the move was saved with `align` on Reference 1 (the origin axis
# its first distance is measured from), which turned the box onto that
# axis: by 180 degrees on a plane whose x axis runs along -X, by 90 on a
# plane offset from YZ (x along -Z, Reference 1 the Y axis).
# mitcad-cli makes Plane1 (F1, a fixed construction plane 36 above XY with
# x along -X and y along -Y), Plane2 (F2, 50 off YZ: x along -Z, y along Y,
# normal +X), Box1 (F3) on Plane1 and Box2 (F4) on Plane2, each at the
# corner (30, -30), 10 long, 8 wide and 4 high. In each box's panel both
# distances (from the two origin axes along its plane) are 30; typed 40,
# OK, saved:
#   1. Box1 saved as `at` on Plane1 without `align`, from x -50 to -40 and
#      y 32 to 40 (turned by 180 degrees it lay from x -40 to -30, y 40 to
#      48).
#   2. Box2 likewise on Plane2, from y -40 to -32 and z -50 to -40 (turned
#      by 90 degrees it lay from y -40 to -30, z -40 to -32).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-placed-plane-move-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-placed-plane-move.XXXXXX)
FILE=$WORK/planes.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "fixed", "origin": [0, 0, 36], "x_axis": [-1, 0, 0], "y_axis": [0, -1, 0]}}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "yz", "distance": 50}}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "F1", "corner": [30, -30],
    "length": 10, "width": 8, "height": 4, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "F2", "corner": [30, -30],
    "length": 10, "width": 8, "height": 4, "operation": "new_body"}},
  {"expect": {"body": "F3.b0", "bbox": {"min": [-40, 22, 36], "max": [-30, 30, 40]}, "tolerance": 1e-6}},
  {"expect": {"body": "F4.b0", "bbox": {"min": [50, -30, -40], "max": [54, -22, -30]}, "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

check() {
  printf '[%s]\n' "$2" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$1"; }
  echo "ok   $1"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F3.b0, F4.b0" "the boxes shown"

# move "Box1" F3 F1 "Plane1" '<expect>': opened for editing on its plane,
# both distances 30 typed as 40, OK, saved; the saved box's bounding box as
# expected (only moved, not turned) and the box saved as `at` on the plane
# without `align` (or `align_side`).
move() {
  ui_mark
  ui_step "double-click $1"                ui_double_click_logged "Timeline $1"
  ui_expect_new "Editing $2 with Box" "$1 opens for editing" 30
  ui_expect_new "Place Box: face " "$1 placed on $4"
  grep "^Place Box: face " "$UI_LOG" | tail -1 | grep -q ", 30 mm from .*, 30 mm from " ||
    ui_fail "$1 not 30 from both axes: $(grep "^Place Box: face " "$UI_LOG" | tail -1)"
  echo "ok   $1 30 from both axes"
  ui_mark
  ui_step "distance 1: 40"                 ui_type_in "Panel Box input at_offset1" "40"
  ui_step "distance 2: 40"                 ui_type_in "Panel Box input at_offset2" "40"
  ui_expect_new "Preview Box: ok" "$1 previewed 10 further from both" 30
  [ -n "$SHOT" ] && [ "$1" = Box1 ] && ui_capture "$SHOT"
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited $2" "$1 moved" 30
  ui_focus_main
  ui_mark
  ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
  check "$1 only moved, not turned" "$5"
  jq -e --arg uid "$2" --arg plane "$3" '.features[] | select(.uid == $uid) |
    .at.face == $plane and (.at | has("align") | not) and (.at | has("align_side") | not) and
    (has("plane") | not)' "$FILE" > /dev/null ||
    ui_fail "$1 not saved as at on $4 without align: $(jq -c --arg uid "$2" '.features[] | select(.uid == $uid)' "$FILE")"
  echo "ok   $1 saved as at on $4, without align"
}

echo "--- 1. Box1 on Plane1 (x along -X) moved is not turned 180 degrees"
move Box1 F3 F1 Plane1 \
  '{"expect": {"body": "F3.b0", "bbox": {"min": [-50, 32, 36], "max": [-40, 40, 40]}, "tolerance": 1e-6}}'

echo "--- 2. Box2 on Plane2 (50 off YZ) moved is not turned 90 degrees"
move Box2 F4 F2 Plane2 \
  '{"expect": {"body": "F4.b0", "bbox": {"min": [50, -40, -50], "max": [54, -32, -40]}, "tolerance": 1e-6}}'
check "Box1 still where it was moved" \
  '{"expect": {"body": "F3.b0", "bbox": {"min": [-50, 32, 36], "max": [-40, 40, 40]}, "tolerance": 1e-6}}'
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI placed plane move test"
