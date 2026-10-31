#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# One label column in the library browser's side panel and in Library
# Parts (mitcad#1007): a one-off visual check, not a recurring test (its
# name is not ui-*-test.sh, so no check picks it up). It makes the parts
# library's socket head cap screws (M5 and M6, their rows tagged with
# their standards and suppliers' articles) with the generator, fetches the
# library, opens Insert from Library on the screw, inserts it and opens
# Library Parts, and saves pictures on the light and the dark theme (Qt's
# GTK platform theme, MITCAD_TEST_COLOR_SCHEME):
#   <dir>/library-fields-<theme>.png            the whole screen, the browser
#   <dir>/library-fields-<theme>-browser.png    the browser's window alone
#   <dir>/library-fields-<theme>-parts.png      Library Parts' window alone
# The app's log as <dir>/look-<theme>.log, with the "Library fields at x:"
# lines (the last dumps stay as <dir>/screen.xwd and <dir>/dialog.xwd).
#
# What to look for: in the browser's side panel the fields of Version,
# Standard, Supplier, Size and Length start at one x (their labels one
# column), and so do Version, the filters and Size and Length under the
# parts table in Library Parts; the text readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-library-fields-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-library-fields-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
export MITCAD_LIBRARIES_DIR=$DIR/cache
LIB=$DIR/fasteners

# The socket head cap screws of sizes M5 and M6, fetched into the look's
# own cache.
if [ ! -f "$LIB/mitcad-library.json" ]; then
  python3 "$ROOT/tools/libraries/make-fastener-library.py" --cli "$CLI" --out "$LIB" --sizes M5,M6 \
    --standards iso4762 --version 1.0.0 --no-render --commit > "$DIR/generator.log" 2>&1 ||
    { cat "$DIR/generator.log"; ui_fail "the generator failed"; }
fi
"$CLI" library fetch "$LIB" --libraries "$MITCAD_LIBRARIES_DIR" --json > "$DIR/fetch.log" 2>&1 ||
  { cat "$DIR/fetch.log"; ui_fail "the library fetch failed"; }

# shot title name [screen]: the window titled so cut out of the screen as
# <dir>/library-fields-<name>.png (with "screen": the whole screen too, as
# <dir>/library-fields-<theme>.png), the mouse away (no hover, no tooltip).
shot() {
  local window
  window=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window $1"
  xdotool mousemove 5000 5000
  ui_wait_idle
  ui_sync "$window"
  sleep 2
  if [ "${3:-}" = screen ]; then
    xwd -root -silent > "$DIR/screen.xwd"
    python3 "$(dirname "$0")/xwd2png.py" "$DIR/screen.xwd" "$DIR/library-fields-$theme.png"
    echo "ok   $DIR/library-fields-$theme.png"
  fi
  xwd -id "$window" -silent > "$DIR/dialog.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/dialog.xwd" "$DIR/library-fields-$2.png"
  echo "ok   $DIR/library-fields-$2.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --demo
  ui_expect_log "Asking the platform for the $theme colour scheme" "the $theme colour scheme asked for"
  ui_expect_log "Window style: $theme, " "the window's style is $theme"
  ui_mark
  ui_step "Insert from Library (command search)" ui_command "Insert from Library"
  ui_focus_dialog '^Insert from Library$'
  ui_step "search 4762"                      xdotool type --delay 40 "4762"
  ui_expect_new "Library browser selected mitcad-parts/iso4762: version " "the socket head cap screw"
  ui_expect_new "Library fields at x: Version " "the browser's fields logged"
  grep "^Library fields at x: " "$UI_LOG" | tail -1
  shot '^Insert from Library$' "$theme-browser" screen
  ui_mark
  ui_step "Insert (Alt+I)"                   ui_key alt+i
  ui_focus_main
  ui_expect_new "Inserted component " "the screw inserted"
  ui_mark
  ui_step "Library Parts (command search)"   ui_command "Library Parts"
  ui_focus_dialog '^Library Parts$'
  ui_expect_new "Library fields at x: Version " "Library Parts' fields logged"
  grep "^Library fields at x: " "$UI_LOG" | tail -1
  shot '^Library Parts$' "$theme-parts"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Library fields look pictures in $DIR"
