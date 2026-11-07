#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/MainWindow.cpp app/MainWindow.hpp app/MainWindowSelection.cpp
# check-all sources: app/view/ViewController.cpp app/OcctViewer.cpp app/OcctViewer.hpp app/framework/Selection.cpp
# check-all sources: tools/cli
# Click adds to the selection in sketches (mitcad#1403, GitHub
# mitcad/Mitcad#170), Preferences > Sketch, through the real UI, on a
# sketch mitcad-cli makes: three slanted lines A from (0, 0) to (40, 10),
# B from (0, 25) to (40, 35) and C from (0, 50) to (40, 60), clicked at
# x = 20 (A at y = 5, B at 30, C at 55); (20, 17.5) is empty space.
#   1. Off (the default): a click on A selects A, one on B replaces it
#      with B, one on empty space clears.
#   2. Turned on in Preferences > Sketch (Alt+A, OK): logged and saved.
#      Clicks on A, B and C select all three; B clicked again is removed;
#      Esc clears. A and B, then a click on empty space clears.
#   3. A and B selected, a right click on C adds C and opens the context
#      menu for the three (Delete offered); the menu's Esc keeps them.
#   4. Off again: a click on A, then one on B, leaves only B, as in 1.
#
# Clicks are given in sketch millimetres (ui_sketch_click), from the
# placement the app logs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-click-adds-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-click-adds.XXXXXX)
FILE=$WORK/lines.mitcad
SETTINGS=$XDG_CONFIG_HOME/Mitcad/Mitcad.conf
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/sketch.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [40, 10]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 25], "end": [40, 35]},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 50], "end": [40, 60]}
]
EOF
"$CLI" run "$WORK/sketch.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# selected: the last "Selected:" line since the mark.
selected() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Selected: " | tail -1; }

# curves_of line: the sketch curves a "Selected:" line names, sorted.
curves_of() { grep -oE 'sketch curve c[0-9]+ of F1' <<< "$1" | sed 's/sketch curve \(c[0-9]*\) of F1/\1/' | sort -u | tr '\n' ' '; }

# click_line A|B|C [button]: a click on the line's point at x = 20.
click_line() {
  case $1 in
  A) ui_sketch_click 20 5 "${2:-1}" ;;
  B) ui_sketch_click 20 30 "${2:-1}" ;;
  C) ui_sketch_click 20 55 "${2:-1}" ;;
  esac
}

# click_empty: a click on empty space between A and B.
click_empty() { ui_sketch_click 20 17.5; }

# expect_selected "count" "description": the last selection since the mark
# is that many sketch curves ("Selected: 2 sketch curves"; "nothing" for
# none). CURVES gets the curves selected (not a command substitution: a
# failure must end the test, not a subshell).
CURVES=""
expect_selected() {
  local want last
  case $1 in
  nothing) want="Selected: nothing" ;;
  1) want="Selected: 1 sketch curve [" ;;
  *) want="Selected: $1 sketch curves [" ;;
  esac
  ui_expect_new "$want" "$2"
  ui_sync
  last=$(selected)
  case "$last" in
  *"$want"*) ;;
  *) ui_fail "$2: the last selection is '$last'" ;;
  esac
  CURVES=$(curves_of "$last")
}

# sorted curve...: the curves as curves_of gives them.
sorted() { printf '%s\n' "$@" | sort -u | tr '\n' ' '; }

# preference on|off: Click adds to the selection in sketches switched in
# Preferences > Sketch (Alt+A), then OK.
preference() {
  ui_mark
  ui_step "Preferences: Sketch (search)"     ui_command "Preferences: Sketch"
  ui_focus_dialog '^Preferences$'
  ui_step "Click adds to the selection (Alt+A)" ui_key alt+a
  ui_step "OK (Enter)"                       ui_key Return
  ui_focus_main
  ui_expect_new "Preferences: click adds to the selection in sketches $1" "the preference turned $1"
  if [ "$1" = on ]; then
    grep -q '^clickAddsToSelection=true$' "$SETTINGS" || ui_fail "sketch/clickAddsToSelection is not true in the settings"
  else
    grep -q '^clickAddsToSelection=false$' "$SETTINGS" || ui_fail "sketch/clickAddsToSelection is not false in the settings"
  fi
  echo "ok   the setting is saved $1"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "Sketch1 in the timeline"
ui_mark
ui_step "double-click Sketch1"               ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited"
ui_sync
ui_step "fit (F6)"                           ui_key F6
ui_sync
ui_expect_log "Sketch view " "the sketch view is logged"

echo "--- 1. Off (the default): a click replaces the selection"
ui_mark
ui_step "click A"                            click_line A
expect_selected 1 "A selected"
A=$CURVES
ui_mark
ui_step "click B"                            click_line B
expect_selected 1 "B selected"
B=$CURVES
[ -n "$A" ] && [ -n "$B" ] && [ "$A" != "$B" ] || ui_fail "a click on B did not replace A ($A, $B)"
echo "ok   B replaced A ($A-> $B)"
ui_mark
ui_step "click empty space"                  click_empty
expect_selected nothing "a click on empty space clears"
ui_mark
ui_step "click C"                            click_line C
expect_selected 1 "C selected"
C=$CURVES
[ -n "$C" ] && [ "$C" != "$A" ] && [ "$C" != "$B" ] || ui_fail "C is not a third curve ($A, $B, $C)"
ui_mark
ui_step "clear (Esc)"                        ui_key Escape
expect_selected nothing "Esc clears"

echo "--- 2. On: a click adds or removes"
preference on
ui_mark
ui_step "click A"                            click_line A
expect_selected 1 "A selected"
ui_mark
ui_step "click B"                            click_line B
expect_selected 2 "B added"
ui_mark
ui_step "click C"                            click_line C
expect_selected 3 "C added: all three selected"
[ "$CURVES" = "$(sorted $A $B $C)" ] || ui_fail "the three selected are not A, B and C: $CURVES"
ui_mark
ui_step "click B again"                      click_line B
expect_selected 2 "B clicked again is removed"
[ "$CURVES" = "$(sorted $A $C)" ] || ui_fail "A and C are not what is left: $CURVES"
ui_mark
ui_step "clear (Esc)"                        ui_key Escape
expect_selected nothing "Esc clears"
ui_mark
ui_step "click A"                            click_line A
expect_selected 1 "A selected"
ui_mark
ui_step "click B"                            click_line B
expect_selected 2 "B added"
ui_mark
ui_step "click empty space"                  click_empty
expect_selected nothing "a click on empty space clears"

echo "--- 3. On: a right click adds and opens the menu for all"
ui_mark
ui_step "click A"                            click_line A
expect_selected 1 "A selected"
ui_mark
ui_step "click B"                            click_line B
expect_selected 2 "B added"
ui_mark
ui_step "right-click C"                      click_line C 3
ui_expect_new "Context menu: " "the context menu opened"
expect_selected 3 "the right click added C"
[ "$CURVES" = "$(sorted $A $B $C)" ] || ui_fail "the right click did not keep A and B with C: $CURVES"
grep "Context menu: " "$UI_LOG" | tail -1 | grep -qE "(:|\|) Delete( \||$)" ||
  ui_fail "no Delete in the context menu: $(grep "Context menu: " "$UI_LOG" | tail -1)"
echo "ok   the menu offers Delete for the three"
ui_mark
ui_step "close the menu (Esc)"               ui_key Escape
ui_sync
since=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Selected: " | tail -1)
[ -z "$since" ] || ui_fail "closing the menu changed the selection: $since"
echo "ok   closing the menu keeps the selection"
ui_mark
ui_step "clear (Esc)"                        ui_key Escape
expect_selected nothing "Esc clears"

echo "--- 4. Off again: a click replaces the selection"
preference off
ui_mark
ui_step "click A"                            click_line A
expect_selected 1 "A selected"
ui_mark
ui_step "click B"                            click_line B
expect_selected 1 "B replaced A"
[ "$CURVES" = "$B" ] || ui_fail "only B should be selected: $CURVES"

ui_finish "sketch click adds to the selection"
