#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ChromeStyle.cpp app/framework/ChromeStyle.hpp app/framework/Restart.cpp
# check-all sources: app/framework/Restart.hpp app/framework/FloatingLayout.cpp app/framework/FloatingLayout.hpp
# check-all sources: app/framework/GlassCard.cpp app/framework/TitleBar.cpp app/framework/Ribbon.cpp
# check-all sources: app/main.cpp app/view/ViewController.cpp app/MainWindow.cpp app/OcctViewer.cpp tools/cli
# check-all sources: app/browser/BrowserBackground.cpp
# The window's layout chosen in Preferences (mitcad#419) and the defaults
# (mitcad#429) through the real UI:
#   1. Without a choice (a plate open) Floating with the Browser's Rows: only
#      the menu bar is above the view (mitcad#425); the title row (no card:
#      its controls and the title's box, each whole within it) and the group
#      cards float over its top, every other card starts below them; with
#      the plate zoomed in to fill the view, a click between two group cards
#      selects its face and a click on the title's box selects nothing.
#      Preferences > Display > Layout Docked asks Restart MitCad, Later
#      keeps it in the settings for the next start.
#   2. Floating again is the window's layout: saved, no question.
#   3. Docked, Restart Now with unsaved changes: the window asks about them
#      as quitting does; Cancel keeps Mitcad open and nothing restarts.
#   4. Saved, Docked, Restart Now: Mitcad quits and would start again
#      (MITCAD_TEST_NO_RESTART=1 logs the command) with the run's settings
#      options and the design, without --chrome.
#   5. The next start is Docked by the preference: no cards; Preferences
#      shows the Browser background there too (mitcad#429); Floating and
#      the Card background chosen together, Later.
#   6. The next start is Floating by the preference, with the Card
#      background: the cards at the default size, in a narrow and a wide
#      window; a click on the title row beside its controls reaches the
#      view.
#   7. --chrome=docked overrides the preference.
# The Windows counterpart is tools/ui-windows-chrome-layout-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-chrome-layout-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

WORK=$(mktemp -d /tmp/mitcad-ui-chrome-layout.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/part.mitcad
# A 200 x 200 x 10 mm plate centred on the origin: Body1 (F2.b0), for the
# click between the group cards.
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
PLATE=$WORK/plate.mitcad
cat > "$WORK/plate.json" << 'JSON'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-100, -100], "width": 200, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
JSON
"$CLI" run "$WORK/plate.json" --save "$PLATE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
CONF=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
# The restart is logged, not started: the test starts the next run itself.
export MITCAD_TEST_NO_RESTART=1
# The layout is the preference's and the default's (ui_start_app would
# start Docked), unless --chrome chooses it.
export UI_DEFAULT_CHROME=1

# choose_layout key [background]: Preferences on its Display page, the
# Layout (Alt+L) set with the key (Home: Docked, End: Floating), the Browser
# background (Alt+B) with the other key if given (Home: Card, End: Rows),
# OK.
choose_layout() {
  ui_mark
  ui_step "Preferences: Display (search)"  ui_command "Preferences: Display"
  ui_expect_new "Preferences opened: Display, " "Preferences on its Display page"
  ui_focus_dialog '^Preferences$'
  ui_step "Layout (Alt+L, $1)"             ui_key alt+l "$1"
  if [ -n "${2:-}" ]; then
    ui_expect_new "Preferences browser background at " "the Browser background shown on the Display page"
    ui_step "Browser background (Alt+B, $2)" ui_key alt+b "$2"
  fi
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
}

# expect_exit description: the app has ended on its own, normally.
expect_exit() {
  for _ in $(seq 1 50); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "$1: Mitcad did not exit"
  if command -v gdb > /dev/null && [ "${UI_GDB:-0}" = 1 ]; then
    grep -q "exited normally" "$UI_LOG" || ui_fail "$1: abnormal exit"
  else
    wait "$UI_RUNNER" || ui_fail "$1: abnormal exit"
  fi
  UI_RUNNER=""
  echo "ok   $1"
}

# cards_below_bar description min max: the last "Floating cards:" line, of
# a window min to max wide (waited for): only the menu bar is above the view
# (the view's top is the bar's bottom, mitcad#425); the title row with its
# controls and the title's box, each whole within it (mitcad#429), and at
# least two group cards float over the view below the bar; the Browser's
# card and every other card start below the floating rows' bottom. CARDS
# holds the line's entries by name: "x y width height".
declare -A CARDS
cards_below_bar() {
  local line="" width="" bar view rows rest name xy size y count=0 groups=0 part
  local rx ry rw rh px py pw ph
  for _ in $(seq 1 25); do
    line=$(grep '^Floating cards: ' "$UI_LOG" | tail -1)
    width=$(sed -n 's/^Floating cards: window \([0-9]*\)x.*/\1/p' <<< "$line")
    [ -n "$width" ] && [ "$width" -ge "$2" ] && [ "$width" -le "$3" ] && break
    sleep 0.2
  done
  [ -n "$width" ] && [ "$width" -ge "$2" ] && [ "$width" -le "$3" ] ||
    ui_fail "$1: no Floating cards line of a window $2 to $3 wide (last: '$line')"
  read -r bar view rows <<< "$(sed -n 's/^Floating cards: window [0-9]*x[0-9]*, top bar bottom \([0-9]*\), view top \([0-9]*\), floating rows bottom \([0-9]*\);.*/\1 \2 \3/p' <<< "$line")"
  [ -n "$bar" ] && [ "$bar" -gt 0 ] || ui_fail "$1: no top bar in '$line'"
  [ "$view" = "$bar" ] || ui_fail "$1: the view's top $view is not the menu bar's bottom $bar ($line)"
  [ "$rows" -gt "$bar" ] || ui_fail "$1: no floating rows below the bar ($line)"
  rest=${line#*; }
  CARDS=()
  while read -r name xy size; do
    [ -n "$name" ] || continue
    y=${xy#*,}
    CARDS[$name]="${xy%,*} $y ${size%x*} ${size#*x}"
    [ "$y" -ge "$bar" ] || ui_fail "$1: $name's top $y is above the top bar's bottom $bar ($line)"
    case $name in
      ribbon | noticeCard | title*) ;;
      ribbonGroup_*) groups=$((groups + 1)) ;;
      *) [ "$y" -ge "$rows" ] || ui_fail "$1: $name's top $y is above the floating rows' bottom $rows ($line)" ;;
    esac
    count=$((count + 1))
  done < <(tr ';' '\n' <<< "$rest" | sed 's/^ *//')
  [ -n "${CARDS[titleRow]:-}" ] || ui_fail "$1: no title row in '$line'"
  [ -z "${CARDS[titleCard]:-}" ] || ui_fail "$1: a title card in '$line'"
  [ -z "${CARDS[titleFinishSketch]:-}" ] || ui_fail "$1: duplicate Finish Sketch in the title row: '$line'"
  for part in titleHistory titleTabs titleSearch; do
    [ -n "${CARDS[$part]:-}" ] || ui_fail "$1: no $part in '$line'"
  done
  # Every control of the row and the title's box whole within the row.
  read -r rx ry rw rh <<< "${CARDS[titleRow]}"
  for part in "${!CARDS[@]}"; do
    case $part in
      titleRow) ;;
      title*)
        read -r px py pw ph <<< "${CARDS[$part]}"
        [ "$px" -ge "$rx" ] && [ "$py" -ge "$ry" ] && [ $((px + pw)) -le $((rx + rw)) ] &&
          [ $((py + ph)) -le $((ry + rh)) ] ||
          ui_fail "$1: $part ($px,$py ${pw}x$ph) is not within the title row ($rx,$ry ${rw}x$rh): '$line'"
        ;;
    esac
  done
  [ "$groups" -ge 2 ] || ui_fail "$1: $groups group cards in '$line'"
  case $rest in
    *browserDock*) ;;
    *) ui_fail "$1: no Browser card in '$line'" ;;
  esac
  echo "ok   $1: the view below the menu bar ($bar), $count cards and parts, $groups group cards, the title row's parts within it, the others below the rows' bottom $rows (window ${width} wide)"
}

# count_new "text": how many lines logged since the mark contain the text.
count_new() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -- "$1"; }

# zoom_plate: the plate seen from the top, zoomed in until it fills the view
# (also behind the floating rows).
zoom_plate() {
  ui_step "top view (search)"              ui_command "Top View"
  ui_step "fit (F6)"                       ui_key F6
  ui_sync
  ui_step "zoom in at the plate's middle"  xdotool mousemove $(ui_logged_at "Body F2.b0") click --repeat 14 --delay 60 4
  ui_wait_idle
}

# click_between_cards: with the plate filling the view, a click between two
# group cards selects its face, a click on the middle of the title's box
# (CARDS, from cards_below_bar) selects nothing.
click_between_cards() {
  local bx by bw bh cx cy
  zoom_plate
  ui_expect_log "Floating view between the cards at" "a place between two group cards is logged"
  ui_mark
  ui_step "click between two group cards"  ui_click_logged "Floating view between the cards"
  ui_expect_new "Selected: 1 face [face F2:" "the click between the cards reached the view: the plate's face"
  [ -n "${CARDS[titleBox]:-}" ] || ui_fail "no title box in the Floating cards line"
  read -r bx by bw bh <<< "${CARDS[titleBox]}"
  cx=$((bx + bw / 2))
  cy=$((by + bh / 2))
  ui_mark
  ui_step "click the title box's middle"   xdotool mousemove $((X + cx * UI_SCALE)) $((Y + cy * UI_SCALE)) click 1
  ui_sync
  sleep 0.5
  [ "$(count_new "Selected: ")" = 0 ] || ui_fail "a click on the title box reached the view"
  echo "ok   the title box takes its click"
}

# click_title_row: with the plate filling the view and nothing selected, a
# click on the title row between the tabs and the next part (no card there,
# mitcad#429) selects the plate's face.
click_title_row() {
  local ry rh tx tw px next="" part cx cy
  zoom_plate
  read -r _ ry _ rh <<< "${CARDS[titleRow]}"
  read -r tx _ tw _ <<< "${CARDS[titleTabs]}"
  for part in "${!CARDS[@]}"; do
    case $part in
      titleRow | titleTabs) ;;
      title*)
        read -r px _ <<< "${CARDS[$part]}"
        [ "$px" -gt "$tx" ] && { [ -z "$next" ] || [ "$px" -lt "$next" ]; } && next=$px
        ;;
    esac
  done
  [ -n "$next" ] || ui_fail "nothing right of the title row's tabs"
  cx=$(((tx + tw + next) / 2))
  cy=$((ry + rh / 2))
  ui_mark
  ui_step "click the title row beside its controls ($cx,$cy)" \
    xdotool mousemove $((X + cx * UI_SCALE)) $((Y + cy * UI_SCALE)) click 1
  ui_expect_new "Selected: 1 face [face F2:" "the click on the title row reached the view: the plate's face"
}

ui_start_display

echo "--- 1. Without a choice Floating with Rows; the cards; Docked chosen, Later"
ui_start_app --no-native-dialogs --open "$PLATE"
ui_expect_log "Chrome: floating (the platform's default)" "Floating without a choice"
ui_expect_log "Browser background: rows" "the Browser's Rows without a choice"
ui_expect_log "Floating cards: window " "the cards placed"
cards_below_bar "at the default size" 1280 1280
click_between_cards
ui_stop_app
# Without the plate for the rest (a sketch on XY).
ui_start_app --no-native-dialogs
ui_expect_log "Chrome: floating (the platform's default)" "Floating without a choice"
choose_layout Home
ui_expect_new "Layout preference: docked (restart asked)" "Docked chosen, a restart asked"
ui_focus_dialog '^Restart MitCad$'
ui_step "later (Escape)"                  ui_key Escape
ui_focus_main
ui_expect_new "Layout restart: later (docked at the next start)" "kept for the next start"
grep -q '^layout=docked$' "$CONF" || ui_fail "the layout is not saved"
echo "ok   saved as appearance/layout"
ui_crashed && ui_fail "Mitcad ended after Later"

echo "--- 2. Floating again: the window's layout, no question"
choose_layout End
ui_expect_new "Layout preference: floating (in use)" "Floating is the window's layout"
grep -q '^layout=floating$' "$CONF" || ui_fail "Floating is not saved"
echo "ok   saved"

echo "--- 3. Docked, Restart Now with unsaved changes: Cancel keeps Mitcad open"
ui_step "create a sketch"                 ui_create_sketch xy
ui_step "rectangle tool (R)"              ui_key r
ui_step "first corner at the origin"      ui_sketch_click 0 0
ui_step "opposite corner"                 ui_sketch_click 40 20
ui_expect_log "Added rectangle" "a rectangle: a change to save"
cards_below_bar "while sketching" 1 100000
ui_mark
ui_step "finish it from the ribbon"      ui_click_logged "Ribbon button sketch.finish"
ui_expect_log "Sketch finished" "a change to save"
ui_step "create another sketch"          ui_create_sketch xy
ui_expect_new "Palette Finish Sketch at" "palette Finish Sketch is laid out"
cards_below_bar "in the next sketch" 1 100000
ui_mark
ui_step "finish it from the palette"     ui_click_logged "Palette Finish Sketch"
ui_expect_new "Sketch finished" "the palette still finishes a sketch"
choose_layout Home
ui_expect_new "Layout preference: docked (restart asked)" "a restart asked"
ui_focus_dialog '^Restart MitCad$'
ui_step "restart now (Enter)"             ui_key Return
ui_expect_new "Layout restart: now" "Restart Now"
ui_expect_new "Restart: closing the window" "the window closes first"
ui_focus_dialog '^MitCad$'
ui_step "cancel (Escape)"                 ui_key Escape
ui_expect_new "Restart cancelled: MitCad stayed open" "cancelled as quitting would be"
ui_crashed && ui_fail "Mitcad ended although the close was cancelled"
grep -q '^Restart (test, not started)' "$UI_LOG" && ui_fail "a restart after the cancel"
echo "ok   Mitcad still runs, nothing restarted"
ui_focus_main

echo "--- 4. Saved; Docked, Restart Now: Mitcad quits and starts again"
ui_step "save as (Ctrl+Shift+S)"          ui_key ctrl+shift+s
ui_step "type the path, Enter"            ui_type_path "Save As" "$FILE"
ui_expect_log "Saved $FILE" "saved"
ui_focus_main
choose_layout End
ui_expect_new "Layout preference: floating (in use)" "Floating again"
choose_layout Home
ui_expect_new "Layout preference: docked (restart asked)" "a restart asked"
ui_focus_dialog '^Restart MitCad$'
ui_key Return
ui_expect_new "Restart (test, not started): " "the restart's command" 20
COMMAND=$(sed -n 's/^Restart (test, not started): //p' "$UI_LOG" | tail -1)
case " $COMMAND " in
  *" --chrome"*) ui_fail "the restart keeps --chrome: $COMMAND" ;;
esac
for part in "--no-native-dialogs" "--no-recovery" "--open $FILE"; do
  case " $COMMAND " in
    *" $part "*) ;;
    *) ui_fail "the restart has no '$part': $COMMAND" ;;
  esac
done
echo "ok   the run's settings options and the design, no --chrome: $COMMAND"
expect_exit "Mitcad quit to restart"

echo "--- 5. The next start: Docked by the preference; Floating and the Card background chosen there"
ui_start_app --no-native-dialogs --open "$FILE"
ui_expect_log "Chrome: docked (the preference)" "Docked by the preference"
grep -q '^Floating cards: ' "$UI_LOG" && ui_fail "Floating cards in the Docked layout"
ui_step "create a Docked sketch"          ui_create_sketch xy
ui_expect_log "Palette Finish Sketch at" "Docked palette finish is laid out"
ui_mark
ui_step "finish Docked sketch from palette" ui_click_logged "Palette Finish Sketch"
ui_expect_new "Sketch finished" "Docked palette still finishes a sketch"
ui_step "create another Docked sketch"    ui_create_sketch xy
ui_mark
ui_step "finish Docked sketch from ribbon" ui_click_logged "Ribbon button sketch.finish"
ui_expect_new "Sketch finished" "Docked ribbon still finishes a sketch"
choose_layout End Home
ui_expect_new "Preferences: browser background card" "the Card background chosen while Docked"
ui_expect_new "Layout preference: floating (restart asked)" "Floating chosen, a restart asked"
ui_focus_dialog '^Restart MitCad$'
ui_step "later (Escape)"                  ui_key Escape
ui_focus_main
ui_expect_new "Layout restart: later (floating at the next start)" "kept for the next start"
grep -q '^layout=floating$' "$CONF" || ui_fail "the layout is not saved"
grep -q '^browserBackground=card$' "$CONF" || ui_fail "the Browser background is not saved"
echo "ok   both saved for the next start"
ui_stop_app

echo "--- 6. The next start: Floating with the Card by the preferences; the cards"
ui_start_app --no-native-dialogs --open "$PLATE"
ui_expect_log "Chrome: floating (the preference)" "Floating by the preference"
ui_expect_log "Browser background: card" "the Browser's card by the preference"
ui_expect_log "Floating cards: window " "the cards placed"
cards_below_bar "at the default size" 1280 1280
click_title_row
# As narrow as the window gets: the ribbon's groups collapse.
ui_step "a narrow window"                 xdotool windowsize "$UI_WINDOW" 800 640
cards_below_bar "in a narrow window" 1 1279
ui_step "a wide window again"             xdotool windowsize "$UI_WINDOW" 1400 900
cards_below_bar "in a wide window" 1400 1400
ui_stop_app

echo "--- 7. --chrome=docked overrides the preference"
ui_start_app --no-native-dialogs --chrome=docked
ui_expect_log "Chrome: docked (--chrome; the preference floating)" "--chrome decides, the preference named"
grep -q '^Floating cards: ' "$UI_LOG" && ui_fail "Floating cards in the Docked layout"
echo "ok   no cards"

ui_finish "UI chrome layout test"
