#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/ChromeStyle.cpp app/framework/ChromeStyle.hpp app/framework/WindowStyle.cpp app/framework/WindowStyle.hpp app/framework/Theme.cpp app/framework/TestDriver.cpp
# The timeline in a light and a dark theme (mitcad#14): the history marker
# and the playback buttons' glyphs follow the palette, also when the theme
# changes while Mitcad runs; so does the Docked window's style (mitcad#251),
# whose one accent colour is the marker's handle.
#   1. A light theme: the marker is dark (#2b3a4a), so are the buttons'
#      glyphs; the window's style is light, the handle #1a6ee0.
#   2. The desktop switches to a dark colour scheme after the window shows
#      (MITCAD_TEST_COLOR_SCHEME=dark, framework/Diagnostics.hpp): the
#      marker turns light (#c8d0da) at once, the glyphs too; the window's
#      style turns dark, the handle #5aa2ff.
#   3. A hidden body's row follows a switch of the theme (mitcad#1104): the
#      demo in the Floating layout, Body1 hidden by its light bulb, its name
#      in the light theme's TextHidden (#4c525c); the test driver switches
#      to the dark scheme (color-scheme dark) after the row was styled: its
#      name in the dark theme's (#b4b8be), and the light one's again
#      after a switch back ("Browser row colours: ...").
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

# expect_line regex description: waits for a log line that matches the
# extended regular expression.
expect_line() {
  for _ in $(seq 1 30); do
    grep -qE -- "$1" "$UI_LOG" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line matching '$1': $(grep -E 'Window style|Timeline colours' "$UI_LOG" | tail -4)"
}

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
ui_expect_log "Window style: light, " "the window's style is light (mitcad#251)"
expect_line "Timeline colours: light, .*, handle #1a6ee0" "light: the marker's handle in the accent colour"
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
ui_expect_log "Window style: dark, " "the window's style turns dark (mitcad#251)"
expect_line "Timeline colours: dark, marker #c8d0da, .*, handle #5aa2ff" "dark: the handle in the dark theme's accent colour"
shot dark
check dark
more "the marker's bar is light" "$MARKER_light" 20
more "the button's glyph is light" "$BUTTON_light" 5
ui_step "hover over the timeline"        xdotool mousemove $(ui_logged_at "Timeline marker")
ui_crashed && ui_fail "crashed"
ui_stop_app

echo "--- A hidden body's row when the theme switches (mitcad#1104)"
cat > "$WORK/hidden-row.mitcad-ui" << 'EOF'
# Body1 hidden by its light bulb in the light theme; the scheme switched to
# dark and back after its row was styled.
expect-ever 'Browser eye Root/Bodies/Body1 at' 30
expect-ever 'Browser row colours: light, ' 30
mark
click-logged 'Browser eye Root/Bodies/Body1 at'
expect-log 'Browser row colours: light, hidden #4c525c: [^;]*Root/Bodies/Body1' 10
mark
color-scheme dark
expect-log 'Browser row colours: dark, hidden #b4b8be: [^;]*Root/Bodies/Body1' 10
mark
color-scheme light
expect-log 'Browser row colours: light, hidden #4c525c: [^;]*Root/Bodies/Body1' 10
EOF
MITCAD_TEST_INPUT="$WORK/hidden-row.mitcad-ui" ui_start_app --chrome=floating --demo
for _ in $(seq 1 300); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the hidden row's script timed out"
wait "$UI_RUNNER" ||
  ui_fail "the hidden row's script failed: $(grep -E 'TestDriver|Browser row colours' "$UI_LOG" | tail -6)"
ui_expect_log "TestDriver: done" "the hidden row's script ran"
grep -q "Browser row colours: dark, hidden #b4b8be: [^;]*Root/Bodies/Body1" "$UI_LOG" ||
  ui_fail "the hidden row did not take the dark theme's colour"
echo "ok   the hidden row's name in the dark theme's TextHidden after the switch"

ui_finish "UI theme test"
