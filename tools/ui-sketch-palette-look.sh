#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Sketch Palette's Create section and its constraints on two rows
# (mitcad#1027): a one-off visual check, not a recurring test (its name is
# not ui-*-test.sh, so no check picks it up). In an empty design a sketch is
# created on XY and the Line tool started (its Create button checked), and
# the screen is saved in the Floating layout (the default) and the Docked
# one, on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME), in a 1600 x 960 window:
#   <dir>/palette-floating-light.png  <dir>/palette-floating-dark.png
#   <dir>/palette-docked-light.png    <dir>/palette-docked-dark.png
#                                     the defaults (nothing counted yet)
#   <dir>/palette-floating-light-counts.png
#                                     with counts in the settings (Ellipse 9,
#                                     Text 5, 3-Point Circle 4) and Center
#                                     Rectangle pinned on the Sketch tab
# and the app's logs as <dir>/<name>.log, with the "Palette create tools",
# "Palette Create: … rows" and "Palette buttons clipped" lines.
#
# What to look for: Create above Constraints, each two rows of seven
# buttons whose columns line up; nothing cut off at the palette's right
# edge and no horizontal scrolling; the Line button checked; the icons and
# the section captions readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-palette-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-sketch-palette-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT

# settings counts: the settings before each start, without counts or with
# the counts and a pin of the "counts" picture.
settings() {
  mkdir -p "$XDG_CONFIG_HOME/Mitcad"
  if [ "$1" = counts ]; then
    printf '[sketchToolCounts]\nsketch.ellipse=9\nsketch.text=5\nsketch.circle_3pt=4\n\n[toolbarPins]\nSKETCH=sketch.rectangle_center\n' \
      > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
  else
    printf '' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
  fi
}

# picture layout theme [counts]: a start in the layout and theme, a sketch
# on XY with the Line tool on, the screen saved.
picture() {
  local layout=$1 theme=$2 name=palette-$1-$2${3:+-$3}
  settings "${3:-}"
  export MITCAD_TEST_COLOR_SCHEME=$theme
  ui_start_app --chrome="$layout"
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 960
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  ui_mark
  ui_step "create sketch on XY"            ui_create_sketch xy
  ui_expect_new "Palette create tools: " "$name: the Create tools chosen"
  ui_expect_new "Palette buttons clipped: " "$name: the buttons' places"
  ui_step "line tool (L)"                  ui_key l
  ui_expect_new "Palette create checked: line" "$name: the Line button checked"
  # The mouse off the window (no hover, no tooltip).
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$name.png"
  echo "ok   $DIR/$name.png"
  grep -E "^Palette (create tools|Create:|Constraints:|buttons clipped)" "$UI_LOG" | tail -n 4
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
picture floating light counts
ui_finish "Sketch Palette pictures in $DIR"
