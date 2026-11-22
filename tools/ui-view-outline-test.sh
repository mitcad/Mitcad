#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ViewController.cpp
# check-all sources: tools/ui-image-stats.py tools/cli
# Wireframe with Visible Edges Only (mitcad#2155) through the real UI, on a
# cylinder 30 mm across and 60 mm high seen from the front, from its side:
#   - Wireframe draws its end circles, seen edge on as lines across, and
#     no line down its side (its seam is a smooth one, never drawn).
#   - Wireframe with Visible Edges Only draws its two outlines down the
#     view too, where its side turns away; Wireframe with Hidden Edges,
#     whose faces are the same, likewise.
#   - On a dark background the faces of Wireframe with Visible Edges Only
#     take its middle colour: nothing of the light background's is left.
# The box's edges in sight drawn solid and no dashed ones: the visual styles
# of tools/ui-view-test.sh.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-outline-test.sh [directory for screenshots]

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-view-outline.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/cylinder.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 60}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/cylinder.json" --save "$WORK/cylinder.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# shot name: a screenshot of the view below the orientation cube; its pixel
# statistics in STAT_<key> variables and its lines down the view at least
# 100 pixels long in LINES (ui-image-stats.py --vertical-lines).
shot() {
  local area vx vy vw vh dump region json lines
  ui_sync
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  dump="$WORK/$1.xwd"
  xwd -root -silent > "$dump" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$(dirname "$0")/xwd2png.py" "$dump" "$SHOTS/outline-$1.png"
  fi
  region=($((X + vx)) $((Y + vy + 200)) $((vw - 20)) $((vh - 220)))
  json=$(python3 "$STATS" "$dump" "${region[@]}")
  lines=$(python3 "$STATS" --vertical-lines "$dump" "${region[@]}" 100)
  echo "     $1: $json"
  echo "     $1 lines: $lines"
  eval "$(python3 -c 'import json, sys; d = json.loads(sys.argv[1]); print(" ".join(f"STAT_{k}={v}" for k, v in d.items()))' "$json")"
  LINES=$(python3 -c 'import json, sys; print(json.loads(sys.argv[1])["lines"])' "$lines")
}

# more name a b: a > b, or fail.
more() { [ "$2" -gt "$3" ] || ui_fail "$1: $2 is not more than $3"; echo "ok   $1 ($2 > $3)"; }

ui_start_display
ui_start_app --open "$WORK/cylinder.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/cylinder.mitcad" "the cylinder opened"

echo "--- From the side"
ui_mark
ui_step "front view (search)"              ui_command "Front View"
ui_expect_new "Camera direction 0 1 0 up 0 0 1" "looking at the cylinder's side" 8
ui_mark
ui_step "fit (F6)"                         ui_key F6
ui_expect_new "Fit: everything shown" "the cylinder fills the view"

echo "--- Wireframe: no outlines"
ui_mark
ui_step "wireframe (search)"               ui_command "Wireframe"
ui_expect_new "Visual style Wireframe" "wireframe"
shot wireframe
WIRE_LINES=$LINES
more "the end circles are drawn" "$STAT_object" 50

echo "--- Wireframe with Visible Edges Only: the two outlines"
ui_mark
ui_step "visible edges only (search)"      ui_command "Wireframe with Visible Edges Only"
ui_expect_new "Visual style Wireframe with Visible Edges Only" "wireframe with visible edges only"
shot visible-only
more "the side's two outlines are drawn" "$LINES" $((WIRE_LINES + 1))

echo "--- Wireframe with Hidden Edges: the same outlines"
ui_mark
ui_step "hidden edges (search)"            ui_command "Wireframe with Hidden Edges"
ui_expect_new "Visual style Wireframe with Hidden Edges" "wireframe with hidden edges"
shot hidden
more "the side's two outlines are drawn" "$LINES" $((WIRE_LINES + 1))

echo "--- A dark background"
ui_mark
ui_step "visible edges only (search)"      ui_command "Wireframe with Visible Edges Only"
ui_expect_new "Visual style Wireframe with Visible Edges Only" "wireframe with visible edges only again"
ui_mark
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_new "Background #" "the background changed"
shot dark
more "the background is dark" "$STAT_dark" $((STAT_pixels / 3))
more "the faces take the dark background's colour" $((STAT_pixels / 50)) "$STAT_light"

ui_crashed && ui_fail "the application crashed"
ui_finish "UI view outline test"
