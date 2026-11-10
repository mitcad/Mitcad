#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The orientation cube's look (mitcad#1620: wider edges and corners): a
# one-off visual check, not a recurring test (its name is not ui-*-test.sh,
# so no check picks it up). Run it with a build before the change and one
# after it to compare. On a 60 x 40 x 20 mm block mitcad-cli makes, in the
# Floating layout, it saves the cube's corner of the view (200 x 200 window
# pixels around the cube's middle):
#   <dir>/cube-home.png        the home view (isometric)
#   <dir>/cube-home-edge.png   the same, the pointer on the front-top edge
#                              (its zone in the accent)
#   <dir>/cube-home-corner.png the same, the pointer on the front-right-top
#                              corner
#   <dir>/cube-front.png       face on, the arrows around the cube
# and the app's log as <dir>/look.log ("Orientation cube zones:" gives the
# parts' zones in square pixels, a build before the change logs none).
#
# What to look for: the edges' and corners' bevels about twice as wide as
# before, the face names as large and readable, the hover filling the whole
# zone of the edge or corner under the pointer; face on, the cube inside
# its arrows with room to spare; the Home capsule clear of the cube.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-cube-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-view-cube-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-view-cube-look.XXXXXX")
trap 'cp "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
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

# shot name: the cube's corner of the view, around the middle of the cube's
# outline logged last.
shot() {
  local line x0 y0 x1 y1 cx cy
  ui_sync
  sleep 0.5
  ui_sync
  line=$(grep -o "Orientation cube outline -\?[0-9]*,-\?[0-9]* to -\?[0-9]*,-\?[0-9]*" "$UI_LOG" | tail -1)
  read -r x0 y0 x1 y1 <<< "$(sed 's/Orientation cube outline \(-\?[0-9]*\),\(-\?[0-9]*\) to \(-\?[0-9]*\),\(-\?[0-9]*\)/\1 \2 \3 \4/' <<< "$line")"
  [ -n "${y1:-}" ] || ui_fail "no outline of the cube logged"
  cx=$(((x0 + x1) / 2))
  cy=$(((y0 + y1) / 2))
  xwd -root -silent > "$WORK/screen.xwd" || ui_fail "no screenshot"
  python3 "$(dirname "$0")/xwd2png.py" "$WORK/screen.xwd" "$DIR/cube-$1.png" \
    $((X + (cx - 100) * UI_SCALE)) $((Y + (cy - 100) * UI_SCALE)) $((200 * UI_SCALE)) $((200 * UI_SCALE))
  echo "saved $DIR/cube-$1.png"
}

ui_start_display
ui_start_app --chrome=floating --open "$WORK/block.mitcad"
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Orientation cube front-top at" "the cube's places logged"
shot home
ui_step "the pointer on the front-top edge" xdotool mousemove $(ui_logged_at "Orientation cube front-top")
shot home-edge
ui_step "the pointer on the corner"        xdotool mousemove $(ui_logged_at "Orientation cube front-right-top")
shot home-corner
ui_mark
ui_step "click the front face"             ui_click_logged "Orientation cube front"
ui_expect_new "Camera direction 0 1 0" "looking at the front" 8
ui_step "the pointer away from the cube"   xdotool mousemove $(ui_view_at 40 60)
shot front
echo "PASS: pictures in $DIR"
