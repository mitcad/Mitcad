#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/HoleCommands.cpp app/commands/PlaceInputs.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/MainWindowActivate.cpp tools/cli app/OcctViewer.cpp
# Hole By Pick moved by new clicks (mitcad#1743, GitHub #302 and #303)
# through the real UI, on a design mitcad-cli makes: a 60 x 40 x 20 block
# in the root (Sketch1 F1, Extrude1 F2: F2.b0, its top face at z = 20) and
# Bar:1 (O1), a 30 x 20 x 10 block of its own (Sketch2 F3, Extrude2 F4:
# F4.b0) turned 90 degrees about Z and moved by (100, 0, 0): a point
# (x, y, z) of Bar is (100 - y, x, z) in the design, Bar's top face at
# z = 10.
#   1. Hole, a click on the root block's top face: placed there, measured
#      from two of that face's edges; the distances' arrows end and
#      Depth's arrow starts at the hole's point (within 0.001 mm); the
#      preview draws the hole's tool as a cut (see-through red).
#   2. A second click on the same face, away from the first: the hole
#      moves there, still measured from that face's edges, and the
#      handles move with it in the same update.
#   3. A click on Bar's top face: Bar is activated (the hole goes into
#      the picked body's component), the hole is measured from that
#      face's own edges (none of the root block's kept), and the handles
#      are where the hole is in the design, not where Bar's own
#      coordinates would put them; the tool still drawn as a cut. OK adds
#      the hole.
# The pictures before and after the change: tools/ui-hole-preview-look.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hole-pick-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-hole-pick.XXXXXX")
FILE=$WORK/holes.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "create_component", "name": "Bar", "transform": [[0, -1, 0, 100], [1, 0, 0, 0], [0, 0, 1, 0]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# coords "line" what: "x,y,z" of "(what x, y, z)" in a line.
coords() { sed -n "s/.*($2 \([^)]*\)).*/\1/p" <<< "$1" | tr -d ' '; }

# close "x,y,z" "x,y,z" tolerance: whether each coordinate is within it.
close() {
  awk -v a="$1" -v b="$2" -v tol="$3" 'BEGIN {
    if (split(a, p, ",") != 3 || split(b, q, ",") != 3) exit 1
    for (i = 1; i <= 3; i++) { d = p[i] - q[i]; if (d < 0) d = -d; if (d > tol) exit 1 }
  }'
}

# last_handle input field: the "x,y,z" of the handle's `knob` (a distance's
# arrow ends at the point) or `origin` (Depth's starts there), as logged
# last ("Handle world Hole <input>: origin x,y,z, knob x,y,z, color ...").
last_handle() {
  local line
  line=$(grep "^Handle world Hole $1: " "$UI_LOG" | tail -1)
  case $2 in
    knob) sed -n 's/^Handle world Hole [^:]*: origin [^ ]* knob \([^ ]*\), color .*/\1/p' <<< "$line" ;;
    *) sed -n 's/^Handle world Hole [^:]*: origin \([^ ]*\), knob .*/\1/p' <<< "$line" ;;
  esac
}

# handles_at "x,y,z" description: Distance 1's and Distance 2's arrows end
# and Depth's arrow starts at the point in the design, within 0.001 mm;
# a few syncs for a refresh that comes when the model is idle.
handles_at() {
  local i k1 k2 depth
  for i in 1 2 3 4 5 6 7 8 9 10; do
    k1=$(last_handle at_offset1 knob)
    k2=$(last_handle at_offset2 knob)
    depth=$(last_handle depth origin)
    if close "$k1" "$1" 0.001 && close "$k2" "$1" 0.001 && close "$depth" "$1" 0.001; then
      echo "ok   $2: the handles at ($1)"
      return
    fi
    ui_sync
    sleep 0.3
  done
  ui_fail "$2: the handles are not at the hole ($1): Distance 1 ends at ($k1), Distance 2 at ($k2), Depth starts at ($depth)"
}

# cut_tool description: the preview draws the hole's tool as a cut.
cut_tool() {
  local look
  look=$(grep "^Preview Hole tool drawn as: " "$UI_LOG" | tail -1 | sed 's/^Preview Hole tool drawn as: //')
  [ "$look" = "cut" ] || ui_fail "$1: the hole's tool drawn as '${look:-nothing logged}', not as a cut"
  echo "ok   $1: the tool drawn as a cut"
}

# references_of "line" face-feature: both references are edges of that
# face (E{F2:end(...)|...} of the body it is on).
references_of() {
  local count
  count=$(grep -o "mm from edge E{$2:end([^ ]*} of $2\.b0" <<< "$1" | wc -l)
  [ "$count" = 2 ] || ui_fail "the references are not the edges of $2's top face: $1"
  echo "ok   both references are edges of $2's top face"
}

# The newest pick places (after the last "Pick places:").
newest_places() {
  local start
  start=$(grep -n "^Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG"
}
# corner_places: the window places ("x y side side") of the root block's
# top corners, each with the two sides it is on (c1 .. c4).
corner_places() {
  newest_places | grep "^Pick vertex F2\.b0/V{F2:end(" |
    sed -n 's/.*F2:side(\(c[1-4]\)\[[^|]*|F2:side(\(c[1-4]\)\[.* at \([0-9]*\),\([0-9]*\)$/\3 \4 \1 \2/p'
}
# The sides (c1 .. c4) a Place line measures from.
reference_sides() { grep -o "mm from edge E{F2:end([^ ]*|F2:side(c[1-4]" <<< "$1" | sed 's/.*F2:side(//'; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the design shown"

echo "--- 1. A click on the root block's top face"
ui_mark
ui_step "hole (H)"                         ui_key h
ui_expect_new "Command Hole started" "hole started"
ui_expect_new "Pick places:" "the picks are logged"
ui_step "click the top face"             ui_click_pick "face F2.b0/F2:end($R)"
ui_expect_new "Hole Place On: 1 face" "the face picked"
ui_expect_new "Place Hole: face face F2:end(" "measured from two of its edges"
ui_expect_new "Preview Hole: ok" "previewed" 30
ui_expect_new "Pick edge F2.b0/E{F2:end(" "the face's own edges take picks now (references)"
grep -q "^Pick edge F4\.b0/" < <(newest_places) && ui_fail "Bar's edges taken as references of the root block's face"
echo "ok   another body's edges are no references of the face"
place=$(grep "^Place Hole: " "$UI_LOG" | tail -1)
references_of "$place" F2
point1=$(coords "$place" point)
[ -n "$point1" ] || ui_fail "no point logged: $place"
close "$point1" "$(cut -d, -f1,2 <<< "$point1"),20" 0.001 || ui_fail "the point is not on the top face: ($point1)"
handles_at "$point1" "the first click"
cut_tool "the first click"

echo "--- 2. A second click on the same face"
# Toward the corner on the sides neither reference is on: the distances'
# arrows run from the references to the hole, so that quarter of the face
# is clear of them; 60 % of the way from the face's middle (the mean of
# its corners) to that corner, clear of the snaps at its edges' middles
# and corners.
ui_sync
corners=$(corner_places)
[ "$(wc -l <<< "$corners")" = 4 ] || ui_fail "the top face's four corners have no pick places: $corners"
refs=$(reference_sides "$(grep "^Place Hole: " "$UI_LOG" | tail -1)" | tr '\n' ' ')
second=$(awk -v refs="$refs" '{
    sx += $1; sy += $2
    if (index(refs, $3) == 0 && index(refs, $4) == 0) { cx = $1; cy = $2; found = 1 }
  } END {
    if (!found) exit 1
    mx = sx / NR; my = sy / NR
    printf "%d %d", mx + 0.6 * (cx - mx) + 0.5, my + 0.6 * (cy - my) + 0.5
  }' <<< "$corners") || ui_fail "no corner of the top face away from the references ($refs): $corners"
read -r sx sy <<< "$second"
echo "     references on $refs: the second click at $second (window)"
ui_mark
ui_step "click the face again elsewhere"   xdotool mousemove $((X + sx * UI_SCALE)) $((Y + sy * UI_SCALE)) click 1
ui_expect_new "Place Hole: face face F2:end(" "placed again on the same face"
ui_expect_new "Preview Hole: ok" "previewed at the new point" 30
place=$(grep "^Place Hole: " "$UI_LOG" | tail -1)
references_of "$place" F2
point2=$(coords "$place" point)
close "$point2" "$point1" 0.5 && ui_fail "the hole did not move: ($point2), was ($point1)"
close "$point2" "$(cut -d, -f1,2 <<< "$point2"),20" 0.001 || ui_fail "the point is not on the top face: ($point2)"
echo "ok   the hole moved from ($point1) to ($point2)"
handles_at "$point2" "the second click"
cut_tool "the second click"

echo "--- 3. A click on another body's face, in a component placed elsewhere"
ui_sync
ui_mark
ui_step "click Bar's top face"             ui_click_pick "face F4.b0/F4:end($R)"
ui_expect_new "Activated Bar" "Bar activated for the hole"
ui_expect_new "Place Hole: face face F4:end(" "placed on Bar's top face"
ui_expect_new "Preview Hole: ok" "previewed on Bar" 30
place=$(grep "^Place Hole: " "$UI_LOG" | tail -1)
references_of "$place" F4
grep -qF "F2:" <<< "$place" && ui_fail "a reference of the root block's face kept: $place"
point3=$(coords "$place" point)
design=$(coords "$place" "in the design")
[ -n "$point3" ] && [ -n "$design" ] || ui_fail "no point in the design logged: $place"
# Bar's (x, y, z) is (100 - y, x, z) in the design.
expected=$(awk -v p="$point3" 'BEGIN { split(p, c, ","); printf "%.6f,%.6f,%.6f", 100 - c[2], c[1], c[3] }')
close "$design" "$expected" 0.001 || ui_fail "the point in the design is ($design), Bar's ($point3) gives ($expected)"
close "$design" "$(cut -d, -f1,2 <<< "$design"),10" 0.001 || ui_fail "the point is not on Bar's top face: ($design)"
echo "ok   the hole at ($point3) in Bar, ($design) in the design"
handles_at "$design" "the click on Bar"
cut_tool "the click on Bar"
[ -n "$SHOT" ] && ui_capture "$SHOT"

ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added 1 hole(s)" "the hole added" 30
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

ui_finish "UI Hole by pick test"
