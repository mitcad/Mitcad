#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A drawing takes the whole workspace (mitcad#1008): a one-off visual
# check, not a recurring test (its name is not ui-*-test.sh, so no check
# picks it up). A 40 x 30 x 20 block with a hole, an empty ISO drawing
# (Drawing1) and an empty cut sheet (CutSheet1) that mitcad-cli makes is
# opened, Drawing1 opened from the browser and a front view and a projected
# view placed on it (Base View on the ribbon), and the screen saved with the
# ribbon's Drawing tab and the sheet over the whole view, on the light and
# the dark theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME), in
# the Floating layout (the default) and the Docked one:
#   <dir>/drawing-floating-light.png  <dir>/drawing-floating-dark.png
#   <dir>/drawing-docked-light.png    <dir>/drawing-docked-dark.png
#                                     the drawing open, Base View's tool
#                                     ended, in the tests' 1280 x 800
#   <dir>/drawing-<layout>-<theme>-1600.png  the same in a 1600 x 960
#                                     window
#   <dir>/drawing-<layout>-light-closed.png  after Close Drawing: Solid and
#                                     Build back, the cards back
#   <dir>/cut-sheet-<layout>-light.png  a cut sheet open: its Cut Sheet tab
# and the app's logs as <dir>/<name>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-workspace-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-drawing-workspace-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$DIR/block.mitcad"
cat > "$DIR/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 15], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"cmd": "add_drawing", "template": "iso"},
  {"cmd": "add_cut_sheet", "width": 600, "height": 300}
]
EOF

# sheet_click x y: a click on a sheet point (mm), from the logged "Drawing
# sheet view x0,y0 x1,y1 x2,y2" (as tools/ui-drawings-test.sh).
sheet_click() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  ui_sync
  xdotool mousemove $(awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }') click 1
  ui_sync
}

# shot name: the mouse off the window (no hover, no tooltip), the screen
# saved.
shot() {
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$1.png"
  echo "ok   $DIR/$1.png"
}

# open_drawing name: the drawing opened from the browser (a double-click on
# its row), as tools/ui-drawings-test.sh opens one again. (Create Drawing's
# dialog did not take Return under the GTK platform theme this script
# uses, so mitcad-cli makes the drawings.)
open_drawing() {
  ui_mark
  ui_step "open $1 (double-click)"         ui_double_click_logged "Browser Root/Drawings/$1"
  ui_expect_new " $1, sheet 1 of 1" "$1 open"
}

# picture layout theme: a start in the layout and theme, a drawing with
# two views, the screens saved.
picture() {
  local layout=$1 theme=$2 name=drawing-$1-$2
  export MITCAD_TEST_COLOR_SCHEME=$theme
  cp -f "$FILE" "$DIR/block-$layout-$theme.mitcad"
  ui_start_app --open "$DIR/block-$layout-$theme.mitcad" --no-native-dialogs --chrome="$layout"
  ui_expect_log "block-$layout-$theme.mitcad" "$layout, $theme: the design opened"
  ui_expect_log "Browser Root/Drawings/Drawing1 at" "the browser lists Drawing1"
  open_drawing Drawing1
  ui_expect_new "Ribbon tabs: DRAWING (shown DRAWING)" "$layout, $theme: the Drawing tab alone"
  ui_expect_new "Drawing tool Base View at" "Base View on the ribbon"
  ui_expect_new "Drawing sheet view " "the sheet placed"
  ui_mark
  ui_step "Base View (the ribbon)"         ui_click_logged "Drawing tool Base View"
  ui_expect_new "Drawing tool: Base View" "Base View on"
  ui_mark
  ui_step "the front view"                 sheet_click 120 120
  ui_expect_new "Drawing view 1 at" "the front view drawn"
  ui_step "the right view"                 sheet_click 215 120
  ui_expect_new "Drawing view 2 at" "the right view drawn"
  ui_step "end the tool (Esc)"             ui_key Escape
  ui_expect_new "Drawing tool: selection" "the tool ended"
  shot "$name"
  ui_mark
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 960
  ui_sync
  ui_expect_new "Drawing area " "$layout, $theme: the drawing placed again"
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  shot "$name-1600"
  if [ "$theme" = light ]; then
    ui_mark
    ui_step "Close Drawing"                 ui_click_logged "Drawing Close Drawing"
    ui_expect_new "Ribbon tabs: SOLID, BUILD (shown SOLID)" "Solid and Build back"
    shot "$name-closed"
    open_drawing CutSheet1
    ui_expect_new "Ribbon tabs: CUT SHEET (shown CUT SHEET)" "the Cut Sheet tab"
    shot "cut-sheet-$layout-$theme"
  fi
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

"$CLI" run "$DIR/block.json" --save "$FILE" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }
ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Drawing workspace pictures in $DIR"
