#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ConfigurationBar.cpp app/browser/MainWindowConfigurationPanel.cpp
# check-all sources: app/browser/BrowserController.cpp app/browser/BrowserPanel.cpp app/framework/FloatingLayout.cpp
# check-all sources: app/framework/GlassCard.cpp app/MainWindow.cpp app/projects/MainWindowProjectsPanel.cpp tools/cli
# check-all sources: app/projects/ProjectsPanel.cpp
# The Configuration panel and the left column (mitcad#1020) through the real
# UI, on two designs mitcad-cli makes: a 10 mm cube without options, and a
# design with three range options (SizeX, SizeY and Height, as a bin's
# sizes) whose cube is patterned 6 x 5 (30 bodies: a long Browser tree).
#   1. Floating, the design without options: no Configuration card.
#   2. Floating, the options: the Configuration card above the browser's,
#      at its left, with the bar's choosers in it (not in the browser's).
#   3. A tall window, Root/Bodies opened: the rows scroll, and the browser's
#      card reaches the timeline's card (less the gap) before it does.
#   4. The boundary between the two cards dragged 40 px up: the
#      Configuration card 40 px lower, the browser's 40 px higher; the split
#      is remembered and the same after a restart; a double-click on the
#      boundary gives the automatic split (the heights of the start) again.
#   5. The Configuration card collapsed by its chevron: its height goes to
#      the browser's card; remembered at the next start; expanded again.
#   6. Docked: the Configuration dock above the browser's, the bar in it;
#      opening the design without options hides it.
#   7. Floating with the Projects card shown (an empty projects folder):
#      its boundary with the Configuration card dragged 50 px down, past
#      the Projects rows, moves no card (mitcad#1152: the boundary below
#      jumped and the browser's card grew); the same after a restart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-configuration-panel-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-configuration-panel.XXXXXX)
PLAIN=$WORK/plain.mitcad
FILE=$WORK/bins.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/plain.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"expect": {"bodies": 1}}
]
EOF
cat > "$WORK/bins.json" << EOF
[
  {"cmd": "add_parameter", "name": "SizeX", "unit": "mm", "expression": "42 mm",
   "min": "42 mm", "max": "252 mm", "step": "42 mm"},
  {"cmd": "add_parameter", "name": "SizeY", "unit": "mm", "expression": "42 mm",
   "min": "42 mm", "max": "252 mm", "step": "42 mm"},
  {"cmd": "add_parameter", "name": "Height", "unit": "mm", "expression": "21 mm",
   "min": "7 mm", "max": "84 mm", "step": "7 mm"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": "Height"}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "rectangular_pattern",
    "objects": {"type": "bodies", "bodies": ["F2.b0"]},
    "direction1": {"axis": {"type": "origin", "axis": "x"}, "quantity": 6, "distance": 20},
    "direction2": {"axis": {"type": "origin", "axis": "y"}, "quantity": 5, "distance": 20},
    "distance_type": "spacing"}},
  {"cmd": "set_configurations", "configurations": {
    "options": ["SizeX", "SizeY", "Height"], "closed": false, "rows": [], "rules": []}},
  {"expect": {"bodies": 30}}
]
EOF
for design in plain bins; do
  "$CLI" run "$WORK/$design.json" --save "$WORK/$design.mitcad" > "$WORK/cli.log" 2>&1 ||
    { cat "$WORK/cli.log"; ui_fail "mitcad-cli: $design"; }
done

# card_rect name: "x y w h" of a card in the last "Floating cards:" line
# (main window coordinates); nothing when the card is not shown.
card_rect() {
  grep "^Floating cards:" "$UI_LOG" | tail -1 | grep -o "; $1 [0-9]*,[0-9]* [0-9]*x[0-9]*" |
    sed "s/^; $1 //; s/[,x]/ /g"
}

# dock_rect name: the same of a dock in the last "Window layout:" line.
dock_rect() {
  grep "^Window layout:" "$UI_LOG" | tail -1 | grep -o "; $1 [0-9]*,[0-9]* [0-9]*x[0-9]*" |
    sed "s/^; $1 //; s/[,x]/ /g"
}

# logged_place "text": "x y" of the last "<text> at x,y" line. (The text
# may hold a slash, as a Browser path does: not sed's delimiter.)
logged_place() { sed -n "s|^$1 at \([0-9]*\),\([0-9]*\)\$|\1 \2|p" "$UI_LOG" | tail -1; }

# inside "x y" "x y w h": the point is in the rectangle.
inside() {
  local px py rx ry rw rh
  read -r px py <<< "$1"
  read -r rx ry rw rh <<< "$2"
  [ -n "$px" ] && [ -n "$rx" ] && [ "$px" -ge "$rx" ] && [ "$px" -lt $((rx + rw)) ] &&
    [ "$py" -ge "$ry" ] && [ "$py" -lt $((ry + rh)) ]
}

# near a b: |a - b| <= 3 px.
near() { [ -n "$1" ] && [ -n "$2" ] && [ $(($1 - $2)) -le 3 ] && [ $(($2 - $1)) -le 3 ]; }

# wait_for description command...: the command succeeds within 10 s.
wait_for() {
  local what=$1
  shift
  for _ in $(seq 1 50); do
    if "$@"; then
      echo "ok   $what"
      return
    fi
    sleep 0.2
  done
  ui_fail "$what (cards: $(grep "^Floating cards:" "$UI_LOG" | tail -1))"
}

# The heights of the two cards: CH (Configuration) and BH (browser).
heights() {
  local rect
  rect=$(card_rect configurationDock)
  CH=$(cut -d' ' -f4 <<< "$rect")
  rect=$(card_rect browserDock)
  BH=$(cut -d' ' -f4 <<< "$rect")
}

# heights_are ch bh: the cards are that high (±3 px).
heights_are() {
  heights
  near "$CH" "$1" && near "$BH" "$2"
}

# config_above_browser: both cards shown, at the same left, the
# Configuration card above the browser's.
config_above_browser() {
  local cx cy cw ch bx by bw bh
  read -r cx cy cw ch <<< "$(card_rect configurationDock)"
  read -r bx by bw bh <<< "$(card_rect browserDock)"
  [ -n "$cx" ] && [ -n "$bx" ] && [ "$cx" = "$bx" ] && [ $((cy + ch)) -le "$by" ]
}

# choosers_in rect: the bar's three choosers are in the rectangle.
choosers_in() {
  local option
  for option in SizeX SizeY Height; do
    inside "$(logged_place "Configuration bar $option")" "$1" || return 1
  done
}

# bar_in_config: the choosers in the Configuration card, none in the
# browser's.
bar_in_config() {
  choosers_in "$(card_rect configurationDock)" || return 1
  ! inside "$(logged_place "Configuration bar SizeX")" "$(card_rect browserDock)"
}

# browser_reaches_timeline: the browser's card ends where the timeline's
# card starts, less the gap (8 px); above the message band's row while it
# has one (mitcad#1042).
browser_reaches_timeline() {
  local bx by bw bh tx ty tw th
  read -r bx by bw bh <<< "$(card_rect browserDock)"
  read -r tx ty tw th <<< "$(card_rect messageBandRow)"
  [ -n "$ty" ] || read -r tx ty tw th <<< "$(card_rect timelineDock)"
  [ -n "$by" ] && [ -n "$ty" ] && near $((by + bh)) $((ty - 8))
}

# scroll_range: the Browser rows' last logged vertical scroll range.
scroll_range() { grep -o '^Browser scroll range [0-9]*$' "$UI_LOG" | tail -1 | grep -o '[0-9]*$'; }
browser_scrolls() { [ "$(scroll_range)" -gt 0 ] 2> /dev/null; }

quit() {
  ui_focus_main
  ui_key ctrl+q
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "MitCad did not exit"
  UI_RUNNER=""
  echo "ok   exited"
}

# start_tall: the design with options in the Floating layout, in a tall
# window, Root/Bodies opened (the design opens folded when the 30 rows do
# not fit).
start_tall() {
  ui_start_app --chrome=floating --open "$FILE" --no-native-dialogs
  UI_MARK=0
  ui_expect_log "Opened $FILE" "the design with options opened"
  ui_expect_log "Configuration bar: SizeX = 42 mm" "the bar shows the options"
  ui_step "a tall window"                  xdotool windowsize "$UI_WINDOW" 1400 980
  ui_sync
  ui_expect_log "Browser arrow Root/Bodies at" "the Bodies folder's arrow"
  ui_wait_idle
  ui_sync
  # The row's last state: logged in sight before the design was folded,
  # then out of sight.
  if ! grep -E "^Browser Root/Bodies/Body1 (at |out of sight)" "$UI_LOG" | tail -1 | grep -q " at "; then
    ui_mark
    ui_step "open Bodies"                  ui_click_logged "Browser arrow Root/Bodies"
    ui_expect_new "Browser Root/Bodies/Body1 at" "Body1's row"
  fi
}

ui_start_display

echo "--- 1. Floating, a design without options: no Configuration card"
ui_start_app --chrome=floating --open "$PLAIN" --no-native-dialogs
ui_expect_log "Opened $PLAIN" "the design without options opened"
wait_for "the browser's card is placed" card_rect browserDock
ui_wait_idle
ui_sync
[ -z "$(card_rect configurationDock)" ] || ui_fail "a Configuration card for a design without options"
! grep -q "^Configuration panel: shown" "$UI_LOG" || ui_fail "the Configuration panel was shown"
echo "ok   no Configuration card"
quit

echo "--- 2. Floating, the options: the Configuration card above the browser's"
start_tall
ui_expect_log "Configuration panel: shown" "the panel shows with the options"
wait_for "the Configuration card is above the browser's, at its left" config_above_browser
wait_for "the bar's choosers are in the Configuration card, not in the browser's" bar_in_config

echo "--- 3. A long tree: the browser's card reaches the timeline before it scrolls"
wait_for "the 30 body rows scroll" browser_scrolls
wait_for "the browser's card reaches the timeline's (less the gap)" browser_reaches_timeline
ui_wait_idle
ui_sync
heights
CH0=$CH
BH0=$BH
echo "ok   at the start: Configuration $CH0 px, browser $BH0 px high"

echo "--- 4. The boundary dragged up: height from the Configuration card to the browser's"
ui_expect_log "Floating split configurationDock/browserDock at " "the boundary's place is logged"
ui_mark
ui_step "drag the boundary 40 px up"     ui_drag_from "Floating split configurationDock/browserDock" 0 -40
ui_expect_new "Floating split remembered: configurationDock " "the split is remembered"
wait_for "the Configuration card 40 px lower, the browser's 40 px higher" heights_are $((CH0 - 40)) $((BH0 + 40))
wait_for "the browser's card still reaches the timeline's" browser_reaches_timeline
SPLIT=$(sed -n 's/^Floating split remembered: //p' "$UI_LOG" | tail -1)
heights
CH1=$CH
BH1=$BH
echo "ok   after the drag: Configuration $CH1 px, browser $BH1 px high ($SPLIT)"
quit

echo "--- 4. After a restart: the same split"
start_tall
ui_expect_log "Floating split restored: $SPLIT" "the split is read at the start"
wait_for "the browser's card reaches the timeline's" browser_reaches_timeline
wait_for "the cards are as high as before the restart" heights_are "$CH1" "$BH1"

echo "--- 4. A double-click on the boundary: the automatic split again"
ui_mark
ui_step "double-click the boundary"      ui_double_click_logged "Floating split configurationDock/browserDock"
ui_expect_new "Floating split: automatic" "the user's split is forgotten"
wait_for "the heights of the start again" heights_are "$CH0" "$BH0"

echo "--- 5. The Configuration card collapsed: its height goes to the browser's"
ui_expect_log "Configuration panel chevron at " "the chevron's place is logged"
ui_mark
ui_step "collapse the Configuration card" ui_click_logged "Configuration panel chevron"
ui_expect_new "Configuration panel: collapsed, remembered" "collapsed, remembered"
collapsed() {
  heights
  [ -n "$CH" ] && [ "$CH" -lt 40 ] && [ -n "$BH" ] && [ "$BH" -ge $((BH0 + CH0 - CH - 3)) ]
}
wait_for "the Configuration card is its header, the browser's card took the rest" collapsed
wait_for "the browser's card reaches the timeline's" browser_reaches_timeline
quit
start_tall
ui_expect_log "Configuration panel: collapsed (remembered)" "collapsed at the next start"
wait_for "still collapsed after the restart" collapsed
ui_mark
ui_step "expand the Configuration card"  ui_click_logged "Configuration panel chevron"
ui_expect_new "Configuration panel: expanded, remembered" "expanded, remembered"
wait_for "the heights of the start again" heights_are "$CH0" "$BH0"
quit

echo "--- 6. Docked: the Configuration dock above the browser's; hidden without options"
ui_start_app --open "$FILE" --no-native-dialogs
UI_MARK=0
ui_expect_log "Opened $FILE" "the design with options opened"
ui_expect_log "Configuration panel: shown" "the dock shows with the options"
ui_expect_log "Configuration bar SizeX at " "the bar's choosers are placed"
ui_mark
ui_step "a tall window"                  xdotool windowsize "$UI_WINDOW" 1400 960
ui_expect_new "Window layout: " "the window logged its docks"
ui_wait_idle
ui_sync
docked_order() {
  local cx cy cw ch bx by bw bh
  read -r cx cy cw ch <<< "$(dock_rect configurationDock)"
  read -r bx by bw bh <<< "$(dock_rect browserDock)"
  [ -n "$cx" ] && [ -n "$bx" ] && [ "$cx" = "$bx" ] && [ $((cy + ch)) -le "$by" ] &&
    choosers_in "$(dock_rect configurationDock)" &&
    ! inside "$(logged_place "Configuration bar SizeX")" "$(dock_rect browserDock)"
}
for _ in $(seq 1 50); do
  docked_order && break
  sleep 0.2
done
docked_order || ui_fail "the Configuration dock is not above the browser's with the bar in it ($(grep "^Window layout:" "$UI_LOG" | tail -1))"
echo "ok   the Configuration dock is above the browser's, the bar in it"
ui_mark
ui_step "open (Ctrl+O)"                  ui_key ctrl+o
ui_type_path "Open" "$PLAIN"
ui_focus_main
ui_expect_new "Opened $PLAIN" "the design without options opened" 20
ui_expect_new "Configuration panel: hidden" "the dock is hidden without options"
ui_expect_new "Configuration bar: none" "the bar has nothing to show"
quit

echo "--- 7. The Projects card above them: its boundary dragged past the rows moves nothing (mitcad#1152)"
# The Projects panel shown, as for a user who turned it on, with an empty
# projects folder: a short card at its content's height, above the
# Configuration card, which is at its content's height too.
export MITCAD_PROJECTS_DIR=$WORK/projects
mkdir -p "$MITCAD_PROJECTS_DIR" "$UI_CONFIG/Mitcad"
CONF=$UI_CONFIG/Mitcad/Mitcad.conf
if grep -q '^\[projects\]$' "$CONF" 2> /dev/null; then
  sed -i '/^\[projects\]$/,/^\[/{/^panel=/d}' "$CONF"
  sed -i 's/^\[projects\]$/[projects]\npanel=true/' "$CONF"
else
  printf '[projects]\npanel=true\n' >> "$CONF"
fi
# The heights of the three cards: PH (Projects), CH and BH (heights).
three_heights() {
  heights
  PH=$(card_rect projectsDock | cut -d' ' -f4)
}
three_heights_are() {
  three_heights
  near "$PH" "$1" && near "$CH" "$2" && near "$BH" "$3"
}
projects_above_config() {
  local px py pw ph cx cy cw ch
  read -r px py pw ph <<< "$(card_rect projectsDock)"
  read -r cx cy cw ch <<< "$(card_rect configurationDock)"
  [ -n "$px" ] && [ -n "$cx" ] && [ "$px" = "$cx" ] && [ $((py + ph)) -le "$cy" ]
}
# The Bodies folder's arrow logged where the browser's card is now.
arrow_in_browser() { inside "$(logged_place "Browser arrow Root/Bodies")" "$(card_rect browserDock)"; }
# start_below_projects: start_tall with the Projects card shown. That card
# takes its rows' height when its folder is read, after the window shows:
# the cards below it move down then, and a click at the arrow's place
# logged before that misses it. The rows' scroll range tells whether the
# 30 body rows are shown, as Body1's row may be below a low browser card
# while they are. The tree scrolls by rows here as on Windows, not by
# pixels: the range counts the rows out of sight (23-31 open in a 1400x980
# window, 0 folded).
OPEN_RANGE=20
start_below_projects() {
  ui_start_app --chrome=floating --open "$FILE" --no-native-dialogs
  UI_MARK=0
  ui_expect_log "Opened $FILE" "the design with options opened"
  ui_expect_log "Configuration bar: SizeX = 42 mm" "the bar shows the options"
  ui_step "a tall window"                  xdotool windowsize "$UI_WINDOW" 1400 980
  ui_sync
  ui_expect_log "Projects panel: 0 projects" "the empty projects folder is read"
  ui_expect_log "Browser arrow Root/Bodies at" "the Bodies folder's arrow"
  ui_wait_idle
  ui_sync
  wait_for "the Bodies folder's arrow is logged in the browser's card" arrow_in_browser
  if [ "$(scroll_range)" -lt "$OPEN_RANGE" ] 2> /dev/null; then
    ui_mark
    ui_step "open Bodies"                  ui_click_logged "Browser arrow Root/Bodies"
    wait_for "the 30 body rows are shown" eval '[ "$(scroll_range)" -ge "$OPEN_RANGE" ] 2> /dev/null'
  fi
}
start_below_projects
ui_expect_log "Projects panel at the start: shown (as chosen)" "the Projects panel is shown"
wait_for "the Projects card is above the Configuration card" projects_above_config
wait_for "the browser's card reaches the timeline's" browser_reaches_timeline
ui_wait_idle
ui_sync
three_heights
PH0=$PH
CH0=$CH
BH0=$BH
echo "ok   at the start: Projects $PH0 px, Configuration $CH0 px, browser $BH0 px high"
ui_expect_log "Floating split projectsDock/configurationDock at " "the boundary's place is logged"
ui_mark
ui_step "drag the boundary 50 px down"   ui_drag_from "Floating split projectsDock/configurationDock" 0 50
ui_expect_new "Floating split remembered: projectsDock " "the split is remembered"
ui_wait_idle
ui_sync
# The Projects card is at its rows' height and the Configuration card at
# its content's: the boundary stays, and the other one with it.
three_heights_are "$PH0" "$CH0" "$BH0" ||
  ui_fail "the cards moved: Projects $PH, Configuration $CH, browser $BH (were $PH0, $CH0, $BH0)"
echo "ok   the three cards keep their heights"
SPLIT=$(sed -n 's/^Floating split remembered: //p' "$UI_LOG" | tail -1)
quit
start_below_projects
ui_expect_log "Floating split restored: $SPLIT" "the split is read at the start"
wait_for "the browser's card reaches the timeline's" browser_reaches_timeline
wait_for "the three cards are as high as before the drag" three_heights_are "$PH0" "$CH0" "$BH0"
quit

ui_finish "UI configuration panel test"
