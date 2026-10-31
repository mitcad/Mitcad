#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Pinned group on the Solid tab (mitcad#1001): a one-off visual check,
# not a recurring test (its name is not ui-*-test.sh, so no check picks it
# up). Sweep, Loft and Shell are pinned on the Solid tab in the settings
# (toolbarPins/SOLID, as Pin to Toolbar leaves them), and the screen is
# saved with the Pinned group at the end of the tab, on the light and the
# dark theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME), in the
# Floating layout (the default) and the Docked one:
#   <dir>/pinned-floating-light.png  <dir>/pinned-floating-dark.png
#   <dir>/pinned-docked-light.png    <dir>/pinned-docked-dark.png
#                                    in a 1600 x 960 window, where every
#                                    group shows its buttons
#   <dir>/pinned-<layout>-<theme>-1280.png  the same at the tests' 1280 x
#                                    800, where the groups may fold to their
#                                    names (the Pinned group before Create
#                                    and Modify)
#   <dir>/pinned-menu-<layout>-light.png  the Create menu open, the ⋮ of
#                                    Sweep's row (disabled in the empty
#                                    design) under the pointer
# and the app's logs as <dir>/<name>.log.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-pinned-toolbar-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-pinned-toolbar-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT

# pinned: the settings with the three pins, before each start.
pinned() {
  mkdir -p "$XDG_CONFIG_HOME/Mitcad"
  printf '[toolbarPins]\nSOLID=solid.sweep,solid.loft,solid.shell\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"
}

# picture layout theme: a start in the layout and theme, the Solid tab's
# Pinned group checked in the log, the screen saved.
picture() {
  local layout=$1 theme=$2 name=pinned-$1-$2
  pinned
  export MITCAD_TEST_COLOR_SCHEME=$theme
  ui_start_app --chrome="$layout"
  ui_expect_log "Ribbon group SOLID/PINNED: solid.sweep, solid.loft, solid.shell" "$layout, $theme: the Pinned group"
  # At the tests' 1280 x 800 the groups may fold to their names, the Pinned
  # group before Create and Modify.
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$name-1280.png"
  echo "ok   $DIR/$name-1280.png"
  # Wide enough for every group's buttons.
  ui_mark
  ui_step "a wide window"                  xdotool windowsize "$UI_WINDOW" 1600 960
  ui_sync
  ui_expect_new "Ribbon pinned SOLID/solid.shell at " "$layout, $theme: the Pinned group's buttons in a wide window"
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  # The mouse off the window (no hover, no tooltip).
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$name.png"
  echo "ok   $DIR/$name.png"
  if [ "$theme" = light ]; then
    ui_mark
    ui_step "open the Create menu"         ui_click_logged "Ribbon SOLID/CREATE"
    ui_expect_new "Ribbon menu dots SOLID/CREATE solid.sweep at" "$layout: the ⋮ places"
    xdotool mousemove $(ui_logged_at "Ribbon menu dots SOLID/CREATE solid.sweep")
    ui_sync
    sleep 1
    ui_capture "$DIR/pinned-menu-$layout-$theme.png"
    echo "ok   $DIR/pinned-menu-$layout-$theme.png"
    ui_key Escape
  fi
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Pinned group pictures in $DIR"
