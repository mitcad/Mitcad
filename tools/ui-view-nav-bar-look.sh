#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The navigation bar at the bottom of the view (mitcad#1086): a one-off
# visual check, not a recurring test (its name is not ui-*-test.sh, so no
# check picks it up). An empty new design, in the Floating and the
# Docked layout, on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME); it saves:
#   <dir>/nav-bar-<layout>-<theme>.png         the window, the bar shown
#   <dir>/nav-bar-<layout>-<theme>-styles.png  the screen with the bar's
#                                              Visual Style drop-down open
#   <dir>/nav-bar-<layout>-<theme>-sketch.png  the window in a sketch on XY
# The app's log as <dir>/look-<layout>-<theme>.log (its "Navigation bar"
# lines give the bar's place and the buttons'); the last dump stays as
# <dir>/window.xwd.
#
# What to look for: Floating: a small capsule card at the bottom centre of
# the view, above the Timeline's card, in the cards' material, its five
# icons evenly spaced, Layout Grid and Snap to Grid tinted and outlined in
# the accent colour (on), the status pill (if any) above it, nothing
# overlapping it; in a sketch beside the Sketch Palette's card. Docked: the
# five buttons in the status bar, left of the badges and the indicators,
# the two toggles shown checked. The drop-down lists the five styles (and
# Rendered with a render worker), the current one checked. Readable icons
# on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-nav-bar-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-view-nav-bar-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)

trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME

# An empty new design, as ui-view-nav-bar-test.sh uses: a body in the view
# takes the click on the XY plane that starts the sketch (picks prefer
# faces), and one beside the planes leaves them out of the fitted view.

# shot name [root]: the main window (with root: the whole screen, so that a
# popup shows) as <dir>/nav-bar-<layout>-<theme><name>.png.
shot() {
  local file="$DIR/nav-bar-$layout-$theme$1.png"
  ui_wait_idle
  ui_sync
  sleep 1.5
  if [ "${2:-}" = root ]; then
    xwd -root -silent > "$DIR/window.xwd"
  else
    xwd -id "$UI_WINDOW" -silent > "$DIR/window.xwd"
  fi
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/window.xwd" "$file"
  echo "ok   $file"
}

ui_start_display
for layout in floating docked; do
  if [ "$layout" = floating ]; then place=card; else place="status bar"; fi
  for theme in light dark; do
    echo "--- The $layout layout, the $theme theme"
    MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --chrome="$layout"
    # (Window style: is the Docked layout's line only.)
    ui_expect_log "Timeline colours: $theme, " "the palette is $theme"
    ui_step "fit (F6)"                     ui_key F6
    ui_expect_log "Navigation bar shown ($place) at " "the navigation bar is shown ($place)"
    xdotool mousemove 5000 5000
    shot ""
    ui_mark
    ui_step "Visual Style (bar)"           ui_click_logged "Navigation bar Visual Style"
    ui_expect_new "Context menu: Shaded" "the bar's drop-down open"
    shot "-styles" root
    ui_step "close the drop-down (Esc)"    ui_key Escape
    ui_mark
    ui_step "create sketch on XY"          ui_create_sketch xy
    ui_expect_log "Palette paletteGrid at" "the Sketch Palette is laid out"
    xdotool mousemove 5000 5000
    shot "-sketch"
    grep "^Navigation bar" "$UI_LOG" | tail -12
    cp -f "$UI_LOG" "$DIR/look-$layout-$theme.log"
    ui_crashed && ui_fail "the application crashed"
    ui_stop_app
  done
done

ui_finish "Navigation bar look pictures in $DIR"
