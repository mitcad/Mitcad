#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Dimension tool's hover and pick marks as wide as the edge they mark
# (mitcad#1618): a one-off visual check, not a recurring test (its name is
# not ui-*-test.sh, so no check picks it up). A 90 x 50 x 5 plate with a
# hole of 16 that mitcad-cli makes, its top view at 2:1 on an A3 drawing
# (the plate's point (x, y) at (120 + 2x, 100 + 2y) on the sheet), the
# plate's front edge (y = 0) hovered and then picked with Dimension, with
# the sheet fitted and zoomed in about the edge (16 notches of the wheel),
# the screen saved each time:
#   <dir>/hover-fit.png   <dir>/picked-fit.png
#   <dir>/hover-zoom.png  <dir>/picked-zoom.png
# and the app's log as <dir>/mark-width.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-mark-width-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-drawing-mark-width-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$DIR/plate.mitcad

cat > "$DIR/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 90, "height": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 25], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "2:1"}
]
EOF
"$CLI" run "$DIR/plate.json" --save "$FILE" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)); as tools/ui-drawing-corners-test.sh.
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
# On the plate's front edge, between its corners and away from the hole:
# the plate's point (45, 0); 6 mm of paper above it, out of the pick's
# reach: nothing.
EDGE="210 100"
OFF="210 106"
# edge_hover: the pointer off the edge first, so that the hover is asked
# again when it comes onto it.
edge_hover() {
  ui_sync
  xdotool mousemove $(sheet_at $OFF)
  hover_is "nothing at " "nothing beside the edge"
  xdotool mousemove $(sheet_at $EDGE)
  ui_sync
}
edge_click() { ui_sync; xdotool mousemove $(sheet_at $EDGE) click 1; ui_sync; }

# hover_is "text" "description": the newest "Drawing hover:" line starts so.
hover_is() {
  local last
  for _ in $(seq 1 30); do
    ui_wait_idle
    last=$(grep -F "Drawing hover: " "$UI_LOG" | tail -n 1)
    case "$last" in
      *"Drawing hover: $1"*) echo "ok   $2"; return 0 ;;
    esac
    sleep 0.1
  done
  ui_fail "$2: the newest hover line is '$last'"
}

# shot name: the screen saved, the pointer where it is (the hover is what
# is pictured).
shot() {
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

# hover_and_pick suffix: the edge hovered (pictured), clicked (pictured),
# the pick dropped (Esc).
hover_and_pick() {
  ui_mark
  ui_step "point at the plate's front edge" edge_hover
  hover_is "line at " "the edge under the pointer"
  shot "hover-$1"
  ui_mark
  ui_step "pick the edge"                  edge_click
  ui_expect_new "Drawing picked line (1)" "the edge picked"
  shot "picked-$1"
  ui_step "drop the pick (Esc)"            ui_key Escape
  ui_sync
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "plate.mitcad" "the design opened"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"

echo "--- The sheet fitted"
hover_and_pick fit

echo "--- Zoomed in about the edge"
ui_mark
ui_step "zoom in (16 notches)"             xdotool mousemove $(sheet_at $EDGE) click --repeat 16 --delay 30 4
ui_expect_new "Drawing sheet view " "the sheet zoomed in"
hover_and_pick zoom

cp -f "$UI_LOG" "$DIR/mark-width.log"
ui_finish "Drawing mark width pictures in $DIR"
