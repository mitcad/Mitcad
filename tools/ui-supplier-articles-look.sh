#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The suppliers' articles of a library part's size in their table
# (mitcad#979): a one-off visual check, not a recurring test (its name is
# not ui-*-test.sh, so no check picks it up). It makes the parts library's
# socket head cap screws of size M5 with the generator (their rows carry
# Würth's and Etra's articles), fetches the library, opens Insert from
# Library on the screw (its default size M5x16, several articles), presses
# Supplier Articles (Alt+A) and saves pictures of the table on the light
# and the dark theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME):
#   <dir>/supplier-articles-light.png   the whole screen
#   <dir>/supplier-articles-light-dialog.png   the table's window alone
#   <dir>/supplier-articles-dark.png
#   <dir>/supplier-articles-dark-dialog.png
# The app's log as <dir>/look-<theme>.log (the last dumps stay as
# <dir>/screen.xwd and <dir>/dialog.xwd).
#
# What to look for: one line per article; the columns Supplier (the
# supplier's name as a link), Part number, Material, Finish and Checked,
# none of them empty (Product page is left out: these rows have none);
# the text readable on both themes; Copy as TSV, Copy Part Number, Open
# Link and Close below.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-supplier-articles-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-supplier-articles-look.sh <dir>}
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

# The socket head cap screws of size M5 with their suppliers' articles,
# fetched into the look's own cache.
if [ ! -f "$LIB/mitcad-library.json" ]; then
  python3 "$ROOT/tools/libraries/make-fastener-library.py" --cli "$CLI" --out "$LIB" --sizes M5 \
    --standards iso4762 --version 1.0.0 --no-render --commit > "$DIR/generator.log" 2>&1 ||
    { cat "$DIR/generator.log"; ui_fail "the generator failed"; }
fi
"$CLI" library fetch "$LIB" --libraries "$MITCAD_LIBRARIES_DIR" --json > "$DIR/fetch.log" 2>&1 ||
  { cat "$DIR/fetch.log"; ui_fail "the library fetch failed"; }

# shot theme: the whole screen and the table's window cut out of it, the
# mouse away (no hover, no tooltip).
shot() {
  local dump="$DIR/screen.xwd" window
  window=$(xdotool search --onlyvisible --name '^Supplier Articles - ' 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no Supplier Articles window"
  xdotool mousemove 5000 5000
  ui_wait_idle
  ui_sync "$window"
  sleep 2
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/supplier-articles-$1.png"
  xwd -id "$window" -silent > "$DIR/dialog.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/dialog.xwd" "$DIR/supplier-articles-$1-dialog.png"
  echo "ok   $DIR/supplier-articles-$1.png $DIR/supplier-articles-$1-dialog.png"
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
  ui_expect_new ", size M5x16, " "its default size M5x16"
  ui_mark
  ui_step "Supplier Articles (Alt+A)"        ui_key alt+a
  ui_focus_dialog '^Supplier Articles - '
  ui_expect_new "Supplier parts of M5x16: " "the table of M5x16's articles"
  grep "^Supplier parts of M5x16: " "$UI_LOG" | tail -1
  shot "$theme"
  ui_step "close the table (Escape)"         ui_key Escape
  ui_focus_dialog '^Insert from Library$'
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Supplier articles look pictures in $DIR"
