#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Solid tab's Create and Modify groups (mitcad#1660, GitHub
# mitcad/Mitcad#252: Extrude's and Press Pull's icons apart at ribbon
# size): a one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). MitCad starts with the demo block,
# the Solid tab is shown and the screen saved, the mouse off the window, on
# the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME), in the Floating layout (the default) and the
# Docked one:
#   <dir>/solid-<layout>-<theme>.png   the Solid tab
# and the app's logs as <dir>/solid-<layout>-<theme>.log. Before and after
# an icon change: run it once with UI_APP set to the older build's app.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-solid-tab-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-solid-tab-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
trap 'ui_cleanup' EXIT

# picture layout theme: a start in the layout and theme, the Solid tab's
# screen saved.
picture() {
  local layout=$1 theme=$2 name=solid-$1-$2
  export MITCAD_TEST_COLOR_SCHEME=$theme
  ui_start_app --no-mcp --demo --no-native-dialogs --chrome="$layout"
  ui_step "the Solid tab"                 ui_click_logged "Ribbon tab SOLID"
  ui_expect_log "Ribbon SOLID/CREATE at" "$layout, $theme: the Create group"
  ui_expect_log "Ribbon SOLID/MODIFY at" "$layout, $theme: the Modify group"
  xdotool mousemove 5000 5000
  ui_sync
  sleep 1
  ui_capture "$DIR/$name.png"
  echo "ok   $DIR/$name.png"
  cp -f "$UI_LOG" "$DIR/$name.log"
  ui_stop_app
}

ui_start_display
for layout in floating docked; do
  for theme in light dark; do
    picture "$layout" "$theme"
  done
done
ui_finish "Solid tab pictures in $DIR"
