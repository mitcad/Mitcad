#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The pictures of the user guide's Configurations chapter (mitcad#969): a
# one-off run that makes them, not a recurring test (its name is not
# ui-*-test.sh, so no check picks it up). It builds the chapter's example,
# the shelf, step by step and saves one small picture per step, each cut
# out of the screen and checked to be under 200 kB:
#   <dir>/shelf-1-parameters.png  the Parameters window (Change Parameters)
#                                 with the user parameters Color and Length
#   <dir>/shelf-2-options.png     the Configurations window's Options tab
#                                 after Add Option… for Color and Length
#   <dir>/shelf-3-bar.png         the browser's Configuration bar after
#                                 Length 1200 typed in its field
#   <dir>/shelf-4-rows.png        the Rows tab with the row Standard (White,
#                                 1200 mm), made with Add Row, renamed and set
#                                 as the default
#   <dir>/shelf-5-rule.png        the Rules tab with the rule Black is short
#                                 (Add › Empty rule, named, then typed as two
#                                 lines in its Text and saved with Enter), as
#                                 text and as blocks
#   <dir>/shelf-6-moved.png       the Configuration bar after choosing Black:
#                                 Length moved to 1000 mm, with the notice
# The pictures go into docs/images/configurations/, where the guide
# expects them: run it into a folder under build/ and copy the six PNG
# files. The design, the commands and the app's log stay in <dir>/work.
#
# The window is made 420 pixels high for the Options and Rows pictures
# (their short lists leave no screen of empty sheet) and 864 for the Rules
# picture.
#
# What to look for: each window whole and nothing else of the screen; the
# bar's pictures show the whole width of the browser's top with the
# Configuration bar, and in the last one the notice "Length 1200 mm →
# 1000 mm: Black is short" under the choosers.
#
# Runs headless on Xvfb (see ui-test-lib.sh), in the Floating layout (the
# default).
# Usage: tools/ui-configurations-guide-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-configurations-guide-look.sh <dir>}
mkdir -p "$DIR/work"
DIR=$(cd "$DIR" && pwd)
WORK=$DIR/work
FILE=$WORK/shelf.mitcad
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$WORK/look.log" 2> /dev/null; ui_cleanup' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# The largest picture the guide takes (bytes).
MAX_BYTES=200000
TOO_LARGE=""

# The shelf: a board 250 x 18 mm whose extrusion distance is Length, and a
# text parameter Color; both with the limits the chapter's step 2 gives
# them (the domains), no configuration yet.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "add_parameter", "name": "Color", "unit": "Text", "expression": "'White'",
   "values": ["'Black'", "'White'"]},
  {"cmd": "add_parameter", "name": "Length", "unit": "mm", "expression": "1500 mm",
   "min": "300 mm", "max": "2000 mm", "step": "10 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 250, "height": 18},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "Length"}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the shelf)"; }

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name x y width height: that part of the screen once everything has
# settled, as <dir>/<name>.png; noted when it is too large for the guide.
shot() {
  local dump="$WORK/screen.xwd" bytes
  away
  ui_wait_idle
  ui_sync
  sleep 1
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png" "$2" "$3" "$4" "$5"
  bytes=$(stat -c %s "$DIR/$1.png")
  if [ "$bytes" -gt "$MAX_BYTES" ]; then
    TOO_LARGE="$TOO_LARGE $1.png ($bytes bytes)"
    echo "LARGE $DIR/$1.png: $bytes bytes"
  else
    echo "ok   $DIR/$1.png ($bytes bytes)"
  fi
}

# window_height "title regex" height: that window made so high (its
# width kept), so a short list leaves no screen of empty sheet in the
# picture.
window_height() {
  local window geometry wx width screen
  window=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window matching '$1'"
  geometry=$(xdotool getwindowgeometry --shell "$window")
  wx=$(sed -n 's/^X=//p' <<< "$geometry")
  width=$(sed -n 's/^WIDTH=//p' <<< "$geometry")
  read -r _ screen <<< "$(xdotool getdisplaygeometry)"
  xdotool windowsize "$window" "$width" $(($2 * UI_SCALE))
  # Centred again from top to bottom: a window made higher in place would
  # reach past the screen's foot, and its foot would be cut.
  xdotool windowmove "$window" "$wx" $(((screen - $2 * UI_SCALE) / 2))
  ui_sync
}

# window_shot "title regex" name: the window of that title, whole.
window_shot() {
  local window geometry wx wy ww wh
  window=$(xdotool search --onlyvisible --name "$1" 2> /dev/null | head -1)
  [ -n "$window" ] || ui_fail "no window matching '$1'"
  geometry=$(xdotool getwindowgeometry --shell "$window")
  wx=$(sed -n 's/^X=//p' <<< "$geometry")
  wy=$(sed -n 's/^Y=//p' <<< "$geometry")
  ww=$(sed -n 's/^WIDTH=//p' <<< "$geometry")
  wh=$(sed -n 's/^HEIGHT=//p' <<< "$geometry")
  [ "$wx" -lt 0 ] && wx=0
  [ "$wy" -lt 0 ] && wy=0
  shot "$2" "$wx" "$wy" "$ww" "$wh"
}

# bar_shot name below: the top of the browser with the Configuration bar,
# the browser's width across, from above the row box to <below> pixels
# under the last chooser (room for the notice).
bar_shot() {
  local left top bottom width card
  local -a anchor row last
  # The Floating layout logs the browser's card ("browserDock x,y wxh"):
  # the picture takes the card's whole width.
  card=$(grep "^Floating cards:" "$UI_LOG" | tail -1 | grep -o "browserDock [0-9]*,[0-9]* [0-9]*x" | tr ',x' '  ')
  if [ -n "$card" ]; then
    read -r _ left _ width <<< "$card"
    left=$((X + (left - 2) * UI_SCALE))
  else
    if grep -q "^Browser arrow Root at " "$UI_LOG"; then
      # The disclosure arrow sits in the indentation at the tree's left.
      read -r -a anchor <<< "$(ui_logged_at "Browser arrow Root")"
      left=$((anchor[0] - 18 * UI_SCALE))
    else
      read -r -a anchor <<< "$(ui_logged_at "Browser Root")"
      left=$((anchor[0] - 60 * UI_SCALE))
    fi
    width=$(sed -n 's/^Browser width \([0-9]*\).*/\1/p' "$UI_LOG" | tail -1)
    [ -n "$width" ] || ui_fail "no browser width logged"
  fi
  [ "$left" -lt 0 ] && left=0
  read -r -a row <<< "$(ui_logged_at "Configuration bar row")"
  if grep -q "^Configuration bar Length slider at " "$UI_LOG"; then
    read -r -a last <<< "$(ui_logged_at "Configuration bar Length slider")"
  else
    read -r -a last <<< "$(ui_logged_at "Configuration bar Length")"
  fi
  top=$((row[1] - 22 * UI_SCALE))
  [ "$top" -lt 0 ] && top=0
  bottom=$((last[1] + $2 * UI_SCALE))
  shot "$1" "$left" "$top" $(((width + 4) * UI_SCALE)) $((bottom - top))
}

# add_option: Add Option... and OK on the first user parameter offered.
add_option() {
  ui_focus_dialog '^Configurations$'
  ui_click_logged "Configurations add option"
  ui_focus_dialog '^Add Option$'
  ui_key Return
}

# edit_row_cell "row column" text: double-click a cell of the Rows sheet,
# replace its text, Enter.
edit_row_cell() {
  ui_focus_dialog '^Configurations$'
  ui_double_click_logged "Configurations row $1"
  ui_sync
  ui_key ctrl+a
  xdotool type --delay 40 "$2"
  ui_key Return
}

# open_window: the Configurations window through the command search.
open_window() {
  ui_focus_main
  ui_mark
  ui_step "Configurations (search)"      ui_command "Configurations"
  ui_expect_new "Configurations window opened" "the window opened"
  ui_focus_dialog '^Configurations$'
}

# close_window: Esc in the Configurations window.
close_window() {
  ui_focus_dialog '^Configurations$'
  ui_step "close the window (Esc)"       ui_key Escape
  ui_focus_main
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs --chrome=floating
ui_expect_log "Browser Root at" "the design is open"
grep -q "Recompute failed\|Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- 1. The user parameters"
ui_mark
ui_step "Change Parameters (search)"     ui_command "Change Parameters"
ui_expect_new "Parameters dialog opened" "the Parameters window opened"
ui_focus_dialog '^Parameters$'
window_shot '^Parameters$' shelf-1-parameters
ui_focus_dialog '^Parameters$'
ui_step "OK (close it)"                  ui_click_logged "Parameters OK"
ui_focus_main

echo "--- 2. The options"
open_window
ui_expect_new "Configurations add option at" "Add Option's place is logged"
ui_mark
ui_step "Add Option: Color"              add_option
ui_expect_new "Configurations added option Color" "Color is an option"
ui_mark
ui_step "Add Option: Length"             add_option
ui_expect_new "Configurations added option Length" "Length is an option"
ui_expect_new "Configurations options: Color (Black, White) = White; Length (300 mm" \
  "two options in their order, with their domains"
ui_focus_dialog '^Configurations$'
window_height '^Configurations$' 420
window_shot '^Configurations$' shelf-2-options
close_window

echo "--- 3. The Configuration bar"
ui_expect_log "Configuration bar Length at" "the Length field's place is logged"
ui_mark
ui_step "the bar's Length field"         ui_click_logged "Configuration bar Length"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "1200"
ui_key Return
ui_expect_new "Configuration chosen: Length = 1200" "the field's value is chosen"
ui_expect_new "Configuration bar: Color = White (Black | White); Length = 1200 mm" "the bar shows 1200 mm"
# The field keeps the keyboard after Enter: a click on an empty place of
# the view takes it, so the picture shows the field without its cursor and
# the command search's S is not typed into the field.
ui_step "click the view (the field lets go)" ui_view_click 92 70
ui_sync
bar_shot shelf-3-bar 22

echo "--- 4. The row Standard"
open_window
ui_expect_log "Configurations tab Rows at" "the Rows tab's place is logged"
ui_mark
ui_step "the Rows tab"                   ui_click_logged "Configurations tab Rows"
ui_expect_new "Configurations page Rows" "the Rows page"
ui_expect_new "Configurations add row at" "Add Row's place is logged"
ui_mark
ui_step "Add Row"                        ui_click_logged "Configurations add row"
ui_expect_new "Configurations added row: Row1" "Row1 added"
ui_expect_new "Configurations row Row1 Name at" "its cells' places are logged"
ui_mark
ui_step "Row1's Name = Standard"         edit_row_cell "Row1 Name" "Standard"
ui_expect_new "Configurations row Row1 Name: Standard" "the row is renamed"
ui_expect_new "Configurations row Standard Name at" "the renamed row's place is logged"
ui_mark
ui_focus_dialog '^Configurations$'
ui_step "select Standard"                ui_click_logged "Configurations row Standard Name"
ui_step "Set as Default"                 ui_click_logged "Configurations set as default"
ui_expect_new "Configurations default row: Standard" "Standard is the default"
ui_focus_dialog '^Configurations$'
window_height '^Configurations$' 420
window_shot '^Configurations$' shelf-4-rows
window_height '^Configurations$' 864

echo "--- 5. The rule Black is short, typed as text"
ui_focus_dialog '^Configurations$'
ui_expect_log "Configurations tab Rules at" "the Rules tab's place is logged"
ui_mark
ui_step "the Rules tab"                  ui_click_logged "Configurations tab Rules"
ui_expect_new "Configurations page Rules" "the Rules page"
ui_expect_new "Rules add at" "the page's buttons are logged"
ui_mark
ui_step "Add"                            ui_click_logged "Rules add"
ui_expect_new "Rules add menu: Limit a range when… | " "Add offers its templates"
ui_sync
ui_step "Empty rule (E)"                 ui_key e
ui_expect_new "Rules saved: Add rule Rule 1" "a new rule"
ui_expect_new "Rules naming Rule 1" "its name to be typed"
ui_step "type its name"                  xdotool type --delay 40 "Black is short"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Rename rule Rule 1 to Black is short" "renamed"
ui_mark
ui_step "Alt+T (the text)"               ui_key alt+t
ui_step "type the when"                  xdotool type --delay 40 "when Color == 'Black'"
ui_step "Shift+Enter (a new line)"       ui_key shift+Return
ui_step "type the then"                  xdotool type --delay 40 "then Length max 1000 mm"
ui_step "Enter"                          ui_key Return
ui_expect_new "Rules saved: Edit Black is short as text" "the two lines are saved"
ui_expect_new "Rule as text Black is short: when Color == 'Black' / then Length max 1000 mm" \
  "the rule as text"
ui_expect_log "Rules row Black is short: enabled, ok" "no mark in the list"
ui_focus_dialog '^Configurations$'
window_shot '^Configurations$' shelf-5-rule
close_window

echo "--- 6. Black moves Length"
ui_expect_log "Configuration bar Color at" "the Color chooser's place is logged"
ui_mark
ui_step "the bar's Color: Black"         ui_choose "Configuration bar Color" -1
ui_expect_new "Configuration chosen: Color = Black" "Black chosen"
ui_expect_new "Configuration moves: Length 1200 mm → 1000 mm: Black is short" \
  "the move is a notice that names the rule"
ui_expect_new "Configuration bar: Color = Black (Black | White); Length = 1000 mm (300 mm … 1000 mm" \
  "the bar offers only what the rule allows"
bar_shot shelf-6-moved 70
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

[ -z "$TOO_LARGE" ] || ui_fail "pictures over $MAX_BYTES bytes:$TOO_LARGE"
ui_finish "Configurations guide pictures in $DIR"
