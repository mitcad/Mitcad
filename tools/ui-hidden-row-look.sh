#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A hidden Browser row's name in the Floating layout (mitcad#1101): a
# one-off visual check, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). An empty new design, whose
# Origin folder is hidden (its light bulb off), on the dark theme and, for
# comparison, the light one (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME); it saves:
#   <dir>/hidden-row-light.png        the window, the light theme
#   <dir>/hidden-row-dark.png         the window, the dark theme
#   <dir>/hidden-row-dark-white.png   the same on View › White Background,
#                                     the lightest view under the cards
# The app's log as <dir>/look-<theme>.log (its "Browser row colours:" line
# gives the hidden rows' colour: TextHidden, #d0d4da on the dark theme);
# the last dump stays as <dir>/window.xwd.
#
# What to look for: Origin's name reads in a dimmed grey on its row's
# card on both themes and both views, clearly dimmer than the shown rows'
# names (Design Settings, Named Views, Component1:1), as the light theme's
# grey does.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hidden-row-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-hidden-row-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)

trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME

# shot name: the main window as <dir>/hidden-row-<name>.png.
shot() {
  local file="$DIR/hidden-row-$1.png"
  ui_wait_idle
  ui_sync
  sleep 1.5
  xwd -id "$UI_WINDOW" -silent > "$DIR/window.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/window.xwd" "$file"
  echo "ok   $file"
}

ui_start_display
for theme in light dark; do
  echo "--- The Floating layout, the $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --chrome=floating
  ui_expect_log "Timeline colours: $theme, " "the palette is $theme"
  ui_expect_log "Browser Root/Origin at" "the browser lists Origin"
  ui_expect_log "Browser row colours: $theme, hidden #" "the hidden rows' colour logged"
  grep "^Browser row colours: $theme, hidden #[0-9a-f]*: [^;]*Root/Origin" "$UI_LOG" | tail -1 |
    grep -q . || ui_fail "Origin's row is not logged as hidden"
  grep "^Browser row colours: " "$UI_LOG" | tail -1
  xdotool mousemove 5000 5000
  shot "$theme"
  if [ "$theme" = dark ]; then
    ui_step "White Background"           ui_command "White Background"
    xdotool mousemove 5000 5000
    shot "$theme-white"
  fi
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Hidden row look pictures in $DIR"
