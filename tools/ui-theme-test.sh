#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/ChromeStyle.cpp app/framework/ChromeStyle.hpp
# The timeline in a light and a dark theme (mitcad#14): the history marker
# and the playback buttons' glyphs follow the palette, also when the theme
# changes while Mitcad runs.
#   1. A light theme: the marker is dark (#2b3a4a), so are the buttons'
#      glyphs.
#   2. The desktop switches to a dark colour scheme after the window shows
#      (MITCAD_TEST_COLOR_SCHEME=dark, framework/Diagnostics.hpp): the
#      marker turns light (#c8d0da) at once, the glyphs too.
# Qt's GTK platform theme gives the palettes (QT_QPA_PLATFORMTHEME=gtk3):
# the generic one has no dark scheme.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-theme-test.sh [screenshot-dir]

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-theme.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME

# shot name: a screenshot (and a PNG of it in the screenshot directory).
shot() {
  sleep 1
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$(dirname "$0")/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/theme-$1.png"
  fi
}

# stats shot "logged place" dx dy width height: pixel statistics of a
# region by a logged place, in STAT_<key> variables.
stats() {
  local px py json
  read -r px py <<< "$(ui_logged_at "$2")"
  json=$(python3 "$STATS" "$WORK/$1.xwd" $((px + $3)) $((py + $4)) "$5" "$6")
  echo "     $1, $2: $json"
  eval "$(python3 -c 'import json, sys; d = json.loads(sys.argv[1]); print(" ".join(f"STAT_{k}={v}" for k, v in d.items()))' "$json")"
}

# more name a b: a > b, or fail.
more() { [ "$2" -gt "$3" ] || ui_fail "$1: $2 is not more than $3"; echo "ok   $1 ($2 > $3)"; }

# check shot: the marker's bar (2 pixels wide, below its handle) and the
# Step Back button's glyph.
check() {
  stats "$1" "Timeline marker" 0 0 2 12
  MARKER_dark=$STAT_dark MARKER_light=$STAT_light
  stats "$1" "Timeline button back" -6 -6 12 12
  BUTTON_dark=$STAT_dark BUTTON_light=$STAT_light
}

ui_start_display

echo "--- A light theme"
ui_start_app --demo
ui_expect_log "Timeline marker at" "the timeline is logged"
ui_expect_log "Timeline colours: light, marker #2b3a4a, buttons the style's" "light: the marker is dark"
shot light
check light
more "the marker's bar is dark" "$MARKER_dark" 20
more "the button's glyph is dark" "$BUTTON_dark" 5
ui_stop_app

echo "--- The theme turns dark while Mitcad runs"
MITCAD_TEST_COLOR_SCHEME=dark ui_start_app --demo
ui_expect_log "Timeline colours: light, marker #2b3a4a" "light at first"
ui_expect_log "Asking the platform for the dark colour scheme" "the desktop switches to dark"
ui_expect_log "Timeline colours: dark, marker #c8d0da" "dark: the marker is light"
grep -q "Timeline colours: dark, marker #c8d0da, buttons #" "$UI_LOG" ||
  ui_fail "the buttons' glyphs are not in the text colour: $(grep 'Timeline colours: dark' "$UI_LOG")"
echo "ok   the buttons' glyphs take the text colour"
shot dark
check dark
more "the marker's bar is light" "$MARKER_light" 20
more "the button's glyph is light" "$BUTTON_light" 5
ui_step "hover over the timeline"        xdotool mousemove $(ui_logged_at "Timeline marker")
ui_crashed && ui_fail "crashed"

ui_finish "UI theme test"
