#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewCube.cpp app/view/ViewCube.hpp app/OcctViewer.cpp app/OcctViewer.hpp tools/cli
# The orientation cube's wider edges and corners (mitcad#1620) through the
# real UI, on a 60 x 40 x 20 mm block mitcad-cli makes:
#   1. From the home view, a click near the middle of the front-top edge's
#      zone turns the view to that edge, and one near the middle of the
#      front-left-top corner's zone to that diagonal view.
#      In that diagonal view the corner's zone, in the middle of the cube,
#      picks at least 0.4 of the front face's area (by the geometry about
#      0.6; with AIS_ViewCube's own bevels, half as wide, about 0.15).
#   2. Face on (a click on the front face), the cube's zones as the view
#      picks them ("Orientation cube zones: ...", square pixels): each edge
#      beside the front face picks at least 0.25 of the face's area (about
#      0.31; AIS_ViewCube's own about 0.18) and each corner, which shows
#      only its tip face on, at least 0.05 of it (about 0.068; about 0.02).
# Pictures of the cube: tools/ui-view-cube-look.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-cube-zones-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-view-cube-zones.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Where the cube's faces, edges and corners can be clicked, and their zones.
export MITCAD_LOG_ORIENTATION_CUBE=1

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_camera "direction ..." description: the camera at rest after the
# action since the mark.
expect_camera() { ui_expect_new "Camera direction $1" "$2" 8; }

# zone name: the area the last "Orientation cube zones:" line since the
# mark gives the part (0 when it is not in sight).
zone() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Orientation cube zones: .*" | tail -1 |
    sed 's/^Orientation cube zones: //' | tr ',' '\n' | sed 's/^ *//' |
    awk -v name="$1" '$1 == name { print $2; found = 1 } END { if (!found) print 0 }'
}

# at_least part face share description: the part's zone is at least that
# share of the face's.
at_least() {
  local part face
  part=$(zone "$1")
  face=$(zone "$2")
  [ "$face" -gt 0 ] || ui_fail "no zone of the $2 face logged"
  awk -v p="$part" -v f="$face" -v s="$3" 'BEGIN { exit !(p >= s * f) }' ||
    ui_fail "$4: the $1 zone is $part px2, less than $3 of the $2 face's $face px2"
  echo "ok   $4: $1 $part px2 of the $2 face's $face px2 (at least $3)"
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Camera direction -0.577 0.577 -0.577" "the camera starts at home (isometric)"
ui_expect_log "Orientation cube front-top at" "the cube's places logged"

echo "--- 1. An edge and a corner near their middle"
ui_mark
ui_step "click the front-top edge"         ui_click_logged "Orientation cube front-top"
expect_camera "0 0.707 -0.707" "the edge view: looking down at the front-top edge"
ui_expect_new "Orientation cube front-left-top at" "the front-left-top corner can be clicked"
ui_mark
ui_step "click the front-left-top corner"  ui_click_logged "Orientation cube front-left-top"
expect_camera "0.577 0.577 -0.577" "the diagonal view from the front-left-top corner"
ui_expect_new "Orientation cube zones: " "the zones logged in the diagonal view" 8
at_least front-left-top front 0.4 "the front-left-top corner seen along its diagonal"

echo "--- 2. Face on: the edges' and corners' zones against the face's"
ui_expect_new "Orientation cube front at" "the front face can be clicked"
ui_mark
ui_step "click the front face"             ui_click_logged "Orientation cube front"
expect_camera "0 1 0 up 0 0 1" "looking at the front"
ui_expect_new "Orientation cube zones: " "the zones logged face on" 8
for edge in front-top front-bottom front-left front-right; do
  at_least "$edge" front 0.25 "the $edge edge"
done
for corner in front-left-top front-right-top front-left-bottom front-right-bottom; do
  at_least "$corner" front 0.05 "the $corner corner"
done

ui_finish "the orientation cube's edges and corners"
