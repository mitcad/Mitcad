#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The Materials, Printers and Processes managers (mitcad#1135): a one-off
# visual check, not a recurring test (its name is not ui-*-test.sh, so no
# check picks it up). It opens a design of one box in a project, whose
# processes live in each place (Mine in this design, Draft in the
# project's library, Bench in My library, the built-in ones), and saves
# pictures on the light and the dark theme (Qt's GTK platform theme,
# MITCAD_TEST_COLOR_SCHEME):
#   <dir>/print-managers-<theme>-materials.png   Materials, PLA Plastic
#                                                (built in) selected
#   <dir>/print-managers-<theme>-printers.png    Printers
#   <dir>/print-managers-<theme>-processes.png   Processes, Draft (This
#                                                project) selected
# The app's log as <dir>/look-<theme>.log (the last dump stays as
# <dir>/dialog.xwd).
#
# What to look for: the search above the list; the groups This design,
# This project, My library and Built-in, each with its count; the design's
# process bold with its dot; the line above the settings that says where
# the item is edited; Copy to ▾ beside the name (Built-in and Duplicate for
# a built-in one); the Import into box before Import from Slicer; New,
# Duplicate, Delete under the list and Close on the right; the text
# readable on both themes.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-managers-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-print-managers-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'ui_cleanup' EXIT
# The palettes of Qt's GTK platform theme: the generic one has no dark
# scheme (tools/ui-theme-test.sh).
export QT_QPA_PLATFORMTHEME=gtk3
unset GTK_THEME MITCAD_TEST_COLOR_SCHEME
# My library of the look's own.
export MITCAD_PRINT_LIBRARY_DIR=$DIR/print-library
PROJ=$DIR/project
FILE=$PROJ/box.mitcad
if [ ! -f "$FILE" ]; then
  "$CLI" project init "$PROJ" --no-history > "$DIR/cli.log" 2>&1 || { cat "$DIR/cli.log"; ui_fail "mitcad-cli project init"; }
  cat > "$DIR/model.json" << EOF
[
  {"cmd": "create_print_profile", "library": "user", "kind": "process", "name": "Bench",
   "values": {"layer_height": 0.28}},
  {"cmd": "create_print_profile", "library": "project", "project": "$PROJ", "kind": "process", "name": "Draft",
   "values": {"layer_height": 0.2}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -10],
    "length": 20, "width": 20, "height": 20, "operation": "new_body"}},
  {"cmd": "create_print_profile", "kind": "process", "name": "Mine", "choose": true}
]
EOF
  "$CLI" run "$DIR/model.json" --save "$FILE" > "$DIR/cli.log" 2>&1 || { cat "$DIR/cli.log"; ui_fail "mitcad-cli"; }
fi

# shot title name: the window titled so as <dir>/print-managers-<name>.png.
shot() {
  local window
  window=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window $1"
  ui_wait_idle
  ui_sync "$window"
  sleep 2
  xwd -id "$window" -silent > "$DIR/dialog.xwd"
  python3 "$(dirname "$0")/xwd2png.py" "$DIR/dialog.xwd" "$DIR/print-managers-$2.png"
  echo "ok   $DIR/print-managers-$2.png"
}

ui_start_display
for theme in light dark; do
  echo "--- The $theme theme"
  MITCAD_TEST_COLOR_SCHEME=$theme ui_start_app --open "$FILE" --no-native-dialogs
  ui_expect_log "Asking the platform for the $theme colour scheme" "the $theme colour scheme asked for"
  ui_mark
  ui_step "Edit Materials (search)"          ui_command "Edit Materials"
  ui_focus_dialog '^Materials$'
  ui_expect_new "Materials search at" "the search has its place"
  ui_step "search 'PLA Plastic'"             ui_click_logged "Materials search"
  xdotool type --delay 40 "PLA Plastic"
  ui_expect_new "Materials mitcad item PLA Plastic at" "PLA is listed"
  ui_step "PLA Plastic"                      ui_click_logged "Materials mitcad item PLA Plastic"
  ui_expect_new "Material selected: PLA Plastic (" "PLA is selected"
  ui_step "clear the search"                 ui_click_logged "Materials search"
  ui_key ctrl+a BackSpace
  xdotool mousemove 5000 5000
  shot '^Materials$' "$theme-materials"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  ui_mark
  ui_step "Printers (search)"                ui_command "Printers"
  ui_focus_dialog '^Printers$'
  ui_expect_new "Printers groups: " "the groups listed"
  xdotool mousemove 5000 5000
  shot '^Printers$' "$theme-printers"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  ui_mark
  ui_step "Processes (search)"               ui_command "Processes"
  ui_focus_dialog '^Processes$'
  ui_expect_new "Processes project item Draft at" "Draft is listed"
  ui_step "Draft"                            ui_click_logged "Processes project item Draft"
  ui_expect_new "Profile selected: process Draft (project)" "Draft is selected"
  xdotool mousemove 5000 5000
  shot '^Processes$' "$theme-processes"
  ui_step "close (Escape)"                   ui_key Escape
  ui_focus_main
  cp -f "$UI_LOG" "$DIR/look-$theme.log"
  ui_crashed && ui_fail "the application crashed"
  ui_stop_app
done

ui_finish "Print managers look pictures in $DIR"
