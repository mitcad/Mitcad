#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Edit Library… of the Materials and the Printers and Processes windows
# (mitcad#1019): a one-off visual check, not a recurring test (its name is
# not ui-*-test.sh, so no check picks it up). It opens a design of one
# box, Edit Materials (PLA Plastic selected) and Edit Printers and
# Processes, and saves pictures on the light
# and the dark theme (Qt's GTK platform theme, MITCAD_TEST_COLOR_SCHEME):
#   <dir>/edit-library-<theme>-materials.png   the Materials window
#   <dir>/edit-library-<theme>-tooltip.png     the screen, the mouse on
#                                              Edit Library… (its tooltip)
#   <dir>/edit-library-<theme>-profiles.png    Printers and Processes
# The app's log as <dir>/look-<theme>.log (the last dumps stay as
# <dir>/screen.xwd and <dir>/dialog.xwd).
#
# What to look for: at the bottom left New ▾, Duplicate, Delete, a gap,
# then Edit Library…; at the bottom right only Close (the window assigns
# nothing since mitcad#1059)
# (Printers and Processes: Use in this design when shown, and Close); the
# tooltip says it opens your own materials, printers and processes, kept
# apart from the design; the text readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-edit-library-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-edit-library-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
# My library of the look's own: empty.
export MITCAD_PRINT_LIBRARY_DIR=$DIR/print-library
FILE=$DIR/box.mitcad
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],' \
  '"length": 20, "width": 20, "height": 20, "operation": "new_body"}}]' > "$DIR/model.json"
"$CLI" run "$DIR/model.json" --save "$FILE" > "$DIR/cli.log" 2>&1 || { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }

# shot title name [screen]: the window titled so as
# <dir>/edit-library-<name>.png, or with "screen" the whole screen, the
# mouse left where it is.
shot() {
  local window
  window=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window $1"
  ui_wait_idle
  ui_sync "$window"
  sleep 2
  if [ "${3:-}" = screen ]; then
    xwd -root -silent > "$DIR/screen.xwd"
    python3 "$(dirname "$0")/xwd2png.py" "$DIR/screen.xwd" "$DIR/edit-library-$2.png"
  else
    xwd -id "$window" -silent > "$DIR/dialog.xwd"
    python3 "$(dirname "$0")/xwd2png.py" "$DIR/dialog.xwd" "$DIR/edit-library-$2.png"
  fi
  echo "ok   $DIR/edit-library-$2.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --open "$FILE" --no-native-dialogs
  ui_expect_log "Asking the platform for the $theme colour scheme" "the $theme colour scheme asked for"
  ui_expect_log "Window style: $theme, " "the window's style is $theme"
  ui_mark
  ui_step "Edit Materials (search)"          ui_command "Edit Materials"
  ui_focus_dialog '^Materials$'
  ui_expect_new "Materials item PLA Plastic at" "PLA is listed"
  ui_step "PLA Plastic"                      ui_click_logged "Materials item PLA Plastic"
  ui_expect_new "Material selected: PLA Plastic (" "PLA is selected"
  ui_expect_new "Materials edit_library at" "Edit Library… has its place"
  xdotool mousemove 5000 5000
  shot '^Materials$' "$theme-materials"
  ui_step "hover Edit Library…"              xdotool mousemove $(ui_logged_at "Materials edit_library")
  shot '^Materials$' "$theme-tooltip" screen
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  ui_mark
  ui_step "Edit Printers and Processes"      ui_command "Edit Printers and Processes"
  ui_focus_dialog '^Printers and Processes$'
  ui_expect_new "Profiles edit_library at" "Edit Library… has its place"
  xdotool mousemove 5000 5000
  shot '^Printers and Processes$' "$theme-profiles"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Edit Library look pictures in $DIR"
