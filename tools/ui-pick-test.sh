#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/PickContext.cpp app/PickContext.hpp app/OcctViewer.cpp app/OcctViewer.hpp app/view
# Picking edges next to faces (mitcad#29) through the real UI, on an L
# bracket: a 60 x 40 x 5 mm floor (Box1, F1) and a 60 x 5 x 40 mm wall at
# its back (Box2, F2, joined), seen from the front and above (the
# orientation cube's front-top edge), where its edges along X run across
# the view and both faces beside each edge are in sight:
#   - In Fillet's Edges input (edges and faces), a click 4 logical pixels
#     above and below an edge picks the edge, not the face there: on the
#     inside (concave) corner between the floor and the wall, where the
#     faces are nearer the eye than the edge, and on two outside (convex)
#     edges. The hover there highlights the edge, as the click picks it.
#   - A click in a face's middle still picks the face.
# Runs at device pixel ratio 1 and again at 2 (UI_SCALE, QT_SCALE_FACTOR).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pick-test.sh
# check-all sources: tools/cli

if [ -z "${UI_SCALE:-}" ]; then
  UI_SCALE=1 bash "$0" "$@" || exit 1
  UI_SCALE=2 bash "$0" "$@"
  exit
fi

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-pick.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_LOG_ORIENTATION_CUBE=1
# How far off the edges the clicks are (logical pixels): within the edges'
# pick width (6 on each side), outside the faces' (3).
OFF=4
INSIDE='E{F1:top|F2:side0}'
WALL_TOP='E{F2:side0|F2:top}'
FLOOR_FRONT='E{F1:side0|F1:top}'

cat > "$WORK/bracket.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 60, "width": 40, "height": 5, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 35],
    "length": 60, "width": 5, "height": 40, "operation": "join", "participants": ["F1.b0"]}}
]
EOF
"$CLI" run "$WORK/bracket.json" --save "$WORK/bracket.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# view_dump name: a dump of the screen and the view's place in it.
view_dump() {
  local area
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ VX VY VW VH <<< "$area"
}

# changed_in_view name: how many of the view's pixels differ from the dump
# "rest" (nothing highlighted).
changed_in_view() {
  python3 "$STATS" "$WORK/$1.xwd" $((X + VX * UI_SCALE)) $((Y + VY * UI_SCALE)) $((VW * UI_SCALE)) \
    $((VH * UI_SCALE)) "$WORK/rest.xwd" | sed -n 's/.*"changed": \([0-9]*\).*/\1/p'
}

# edge_off name dy: the screen position dy logical pixels below the edge's
# pick place (above when negative).
edge_off() {
  local ex ey
  read -r ex ey <<< "$(ui_pick_at "edge F1.b0/$1")"
  echo "$ex" $((ey + $2 * UI_SCALE))
}

# pick_off name dy: in a new Fillet (the edges picked before would show
# their preview), the highlight under the pointer dy pixels off the edge is
# a line, not a face, and a click there picks the edge.
pick_off() {
  local changed
  ui_mark
  ui_step "fillet (F)" ui_key f
  ui_expect_new "Pick edge F1.b0/$1 at" "the edge can be picked"
  ui_step "hover $2 px off $1" xdotool mousemove $(edge_off "$1" "$2")
  view_dump hover
  changed=$(changed_in_view hover)
  [ -n "$changed" ] && [ "$changed" -gt 50 ] || ui_fail "nothing highlighted off $1 (${changed:-?} pixels)"
  [ $((changed * 20)) -lt $((VW * VH * UI_SCALE * UI_SCALE)) ] ||
    ui_fail "a face highlighted off $1 ($changed pixels changed)"
  echo "ok   the edge highlighted ($changed pixels changed)"
  ui_step "click there" xdotool click 1
  ui_expect_new "Fillet Edges: 1 edge [edge $1 of F1.b0]" "it picked the edge"
  ui_step "cancel (Esc)" ui_key Escape
  ui_expect_new "Command Fillet cancelled" "no fillet"
}

echo "=== Device pixel ratio $UI_SCALE"
ui_start_display
ui_start_app --open "$WORK/bracket.mitcad"
ui_step "front-top (the cube's edge)"      ui_click_logged "Orientation cube front-top"
ui_expect_log "Camera direction 0 0.707 -0.707" "seen from the front and above"
ui_step "fit (F6)"                         ui_key F6
# Over the background on the view's left, where nothing is highlighted.
ui_step "pointer off the bracket"          xdotool mousemove $(ui_view_at 3 50)
ui_step "fillet (F)"                       ui_key f
ui_expect_log "Pick edge F1.b0/$INSIDE at" "the inside edge can be picked"
view_dump rest
ui_step "cancel (Esc)"                     ui_key Escape

echo "--- The inside (concave) edge"
# Above it the wall, below it the floor, both nearer the eye than the edge.
# (The floor's back bottom edge lies right behind it in this view.)
pick_off "$INSIDE" -$OFF
pick_off "$INSIDE" $OFF

echo "--- Outside (convex) edges"
for edge in "$WALL_TOP" "$FLOOR_FRONT"; do
  pick_off "$edge" -$OFF
  pick_off "$edge" $OFF
done

echo "--- Faces from their middle"
ui_step "fillet (F)"                       ui_key f
for face in F1:top F2:side0; do
  ui_mark
  ui_step "click the middle of $face"      ui_click_pick "face F1.b0/$face"
  ui_expect_new "Fillet Edges: 1 face [face $face of F1.b0]" "it picked the face"
  ui_step "click it again"                 ui_click_pick "face F1.b0/$face"
  ui_expect_new "Fillet Edges: nothing" "the face taken out again"
done

ui_step "cancel (Esc)"                     ui_key Escape
ui_finish "picking edges next to faces at device pixel ratio $UI_SCALE"
