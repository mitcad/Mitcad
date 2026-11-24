#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/view/Snapshot.cpp
# check-all sources: app/sketch/SketchOverlay.cpp app/sketch/SketchOverlay.hpp app/sketch/SketchController.cpp
# check-all sources: app/sketch/ShownDimensions.cpp app/sketch/ShownDimensions.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp tools/cli
# Sketch mode's overlay and the dimensions shown outside sketch mode are
# drawn by the view itself, in its overlay layer, not by widgets over the
# view (mitcad#2296, GitHub mitcad/Mitcad#590: sketch mode held the view to
# about 30 frames a second). Sketch1, a 30 x 20 rectangle with its width and
# height dimensioned:
#   - Show Dimensions on (its timeline menu): an orbit's "View navigation"
#     line counts the shown dimensions painted by the view with its frames
#     and names no widget over the view for them; their places ("Shown
#     dimension Sketch1 k…") are logged once the orbit ended, not during it.
#   - Sketch1 edited: an orbit and a burst of wheel steps each log their
#     navigation with the sketch overlay painted by the view and no repaint
#     of a sketch widget; no "Sketch view", "Sketch axes drawn" or
#     "Dimension … text at" line is written while the view is navigated,
#     and the new places once it ended.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-overlay-frames-test.sh [screenshot.png]

set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-overlay-frames.XXXXXX")
FILE=$WORK/plate.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_new_re "extended regex" "description": as ui_expect_new, a pattern.
expect_new_re() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line like '$1' since the mark"
}

# orbit: a Shift + middle drag across the view (an orbit in the default
# navigation) in twelve steps; the view logs the navigation once it ends.
orbit() {
  local i x y
  read -r x y <<< "$(ui_view_at 40 60)"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" keydown shift mousedown 2
  for i in $(seq 1 12); do
    xdotool mousemove $((x + i * 12)) $((y - i * 3))
    sleep 0.05
  done
  xdotool mouseup 2 keyup shift
}

# zoom: a burst of eight wheel steps over the view's middle.
zoom() {
  local i
  xdotool mousemove $(ui_view_at 50 50)
  for i in $(seq 1 8); do
    xdotool click 5
    sleep 0.03
  done
}

# navigated "what" "layer" "widget": the navigation's line since the mark:
# frames drawn, the layer (its name in the line) painted by the view with
# them, no repaint of the widget that drew it before (its object name);
# NAVIGATION keeps the line's number in the log.
navigated() {
  local line frames painted
  expect_new_re "^View navigation: [0-9]+ frame\(s\), [0-9]+ repaint\(s\) of widgets over the view" \
    "$1: the navigation ended"
  NAVIGATION=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -n "^View navigation: " | tail -1 | cut -d: -f1)
  NAVIGATION=$((UI_MARK + NAVIGATION))
  line=$(sed -n "${NAVIGATION}p" "$UI_LOG")
  frames=$(sed -E 's/^View navigation: ([0-9]+) frame.*/\1/' <<< "$line")
  [ "$frames" -ge 4 ] || ui_fail "$1: the navigation drew only $frames frame(s): $line"
  echo "ok   $1: $frames frames drawn"
  painted=$(sed -nE "s/.*, $2 painted ([0-9]+) time\(s\) by the view.*/\1/p" <<< "$line")
  [ -n "$painted" ] && [ "$painted" -ge 1 ] ||
    ui_fail "$1: the $2 was not painted by the view with its frames: $line"
  echo "ok   $1: the $2 painted $painted time(s) by the view"
  case "$line" in
  *"$3"*) ui_fail "$1: the $3 widget was painted again over the view: $line" ;;
  esac
  echo "ok   $1: no $3 widget painted over the view"
}

# quiet_during "what" "regex": no line like it from the mark to the
# navigation's line (none written per frame of the navigation).
quiet_during() {
  local count
  count=$(sed -n "$((UI_MARK + 1)),${NAVIGATION}p" "$UI_LOG" | grep -cE -- "$2")
  [ "$count" -eq 0 ] || ui_fail "$1: $count line(s) like '$2' written during the navigation"
  echo "ok   $1: no '$2' line during the navigation"
}

# logged_after "what" "text": the line logged after the navigation's line.
logged_after() {
  local i
  for i in $(seq 1 30); do
    tail -n +$((NAVIGATION + 1)) "$UI_LOG" | grep -qF -- "$2" && { echo "ok   $1"; return; }
    sleep 0.2
  done
  ui_fail "$1: no '$2' line after the navigation ended"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Sketch dimensions shown outside sketch mode: none" "no dimensions shown at first"

echo "--- The dimensions shown outside sketch mode"
ui_mark
ui_step "right-click Sketch1 (timeline)" ui_click_logged "Timeline Sketch1" 3
ui_expect_new "Show Dimensions" "the timeline's menu has Show Dimensions"
ui_step "choose Show Dimensions"         ui_menu_choose "Show Dimensions"
ui_expect_new "Sketch dimensions drawn outside sketch mode: Sketch1 (2)" "Sketch1's two dimensions are drawn"
ui_sync
ui_mark
ui_step "orbit with the dimensions shown" orbit
navigated "shown dimensions" "shown dimensions" "shownSketchDimensions"
quiet_during "shown dimensions" "^Shown dimension Sketch1 k"
logged_after "their places logged once the orbit ended" "Shown dimension Sketch1 k"

echo "--- Sketch mode"
ui_sync
ui_double_click_expect "Timeline Sketch1" "Editing sketch F1 (Sketch1)" "Sketch1 opened"
ui_expect_log "Sketch view " "the sketch's places logged"
ui_expect_log "Sketch axes drawn: " "its axes logged"
ui_sync
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "orbit in sketch mode"           orbit
navigated "orbit in sketch mode" "sketch overlay" "sketchOverlay"
quiet_during "orbit in sketch mode" "^(Sketch view |Sketch axes drawn: |Dimension k[0-9]+ text at )"
logged_after "the sketch's places logged once the orbit ended" "Sketch view "
logged_after "its axes logged once the orbit ended" "Sketch axes drawn: "
logged_after "its dimensions' texts logged once the orbit ended" " text at "
ui_sync
ui_mark
ui_step "zoom out (wheel)"               zoom
navigated "wheel zoom in sketch mode" "sketch overlay" "sketchOverlay"
quiet_during "wheel zoom in sketch mode" "^(Sketch view |Sketch axes drawn: |Dimension k[0-9]+ text at )"
logged_after "the sketch's places logged once the zoom ended" "Sketch view "
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"

ui_finish 'UI sketch overlay and shown dimensions drawn by the view'
