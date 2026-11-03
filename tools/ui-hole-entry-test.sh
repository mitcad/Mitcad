#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/commands/HoleCommands.cpp app/commands/PlaceInputs.cpp
# Hole's keyboard and its dimensions in the view (mitcad#1181), through the
# real UI on the demo block (60 x 40 x 20, Body1, F2.b0); the Windows
# counterpart is tools/ui-windows-hole-entry-test.ps1 (ctest
# app.ui-windows-hole-entry):
#   1. In the Floating and the Docked layout: Hole on the top face,
#      Distance 1 and Distance 2 typed (15, 20). Tab in Distance 2 moves
#      the keyboard on to Diameter, which takes what is typed next (10);
#      Shift+Tab goes back to Distance 2, and again to Distance 1.
#   2. Floating: the dimensions in the view take typed values. Distance
#      2's value clicked, 25 typed and Esc: the box closes and 20 is back.
#      Depth's value clicked, 12 and Enter: Depth is 12 and the preview
#      follows. Depth's again, "abc": the box turns to the error colour,
#      Enter is refused (the box stays, the command too), Esc puts 12 back.
#      Depth's again, 10 and Enter. OK makes the hole 15 and 20 from the
#      edges, 10 across and 10 deep (the volume of ui-create-test's hole).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hole-entry-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# hole_on_top layout [app arguments]: the demo in that layout, Hole on the
# block's top face with its distances 15 and 20 typed, the keyboard in
# Distance 2.
hole_on_top() {
  echo "=== Hole, $1"
  shift
  ui_start_app --demo "$@"
  ui_step "fit (F6)"                       ui_key F6
  ui_mark
  ui_step "hole (H)"                       ui_key h
  ui_expect_new "Command Hole started" "hole started"
  ui_expect_new "Pick places:" "the picks are logged"
  ui_step "click the top face"             ui_click_pick "face F2.b0/F2:end($RECT)"
  ui_expect_new "Hole Place On: 1 face" "the face picked"
  ui_expect_new "Place Hole: face face F2:end(" "measured from two of its edges"
  ui_step "distance 1: 15"                 ui_type_in "Panel Hole input at_offset1" "15"
  ui_step "distance 2: 20"                 ui_type_in "Panel Hole input at_offset2" "20"
}

# tab_steps: Tab from Distance 2 to Diameter, which takes the typed 10;
# Shift+Tab back to Distance 2 and on to Distance 1.
tab_steps() {
  local from typed
  echo "--- Tab and Shift+Tab between the fields"
  ui_mark
  from=$(wc -l < "$UI_LOG")
  ui_step "Tab"                            ui_key Tab
  ui_expect_new "Panel Hole Tab: at_offset2 -> diameter" "Tab goes on from Distance 2 to Diameter"
  ui_step "type 10"                        xdotool type --delay 40 -- "10"
  ui_sync
  typed=$(ui_typed_since "$from" "Panel field diameter: ")
  [ "$typed" = "10" ] || ui_fail "Diameter has '$typed' after Tab and 10 typed"
  echo "ok   Diameter took what was typed"
  ui_expect_new "Hole Diameter: 10" "the diameter evaluated"
  grep -q "^Panel field at_offset2: 2010$\|^Panel field at_offset2: 2010 mm$" <(tail -n +"$from" "$UI_LOG") &&
    ui_fail "the typed 10 went into Distance 2"
  ui_mark
  ui_step "Shift+Tab"                      ui_key shift+Tab
  ui_expect_new "Panel Hole Shift+Tab: diameter -> at_offset2" "Shift+Tab back to Distance 2"
  ui_mark
  ui_step "Shift+Tab again"                ui_key shift+Tab
  ui_expect_new "Panel Hole Shift+Tab: at_offset2 -> at_offset1" "and on to Distance 1"
  grep -q "Command Hole cancelled\|Done Hole" <(tail -n +"$from" "$UI_LOG") && ui_fail "Tab ended the command"
  echo "ok   the command is still open"
}

ui_start_display

hole_on_top "Floating" --chrome=floating
tab_steps

echo "--- The dimensions in the view take typed values"
ui_expect_log "Value box Hole at_offset2 at " "Distance 2 is a dimension in the view"
ui_expect_log "Value box Hole depth at " "and Depth"
ui_mark
ui_step "Distance 2's value"               ui_click_logged "Value box Hole at_offset2"
ui_expect_new "Value box Hole at_offset2 opened: 20" "a field over the view edits it"
ui_mark
ui_step "type 25"                          xdotool type --delay 40 -- "25"
ui_sync
ui_step "Esc puts it back"                 ui_key Escape
ui_expect_new "Value box Hole at_offset2 closed" "the box closed"
ui_expect_new "Hole Distance 2: 20" "Distance 2 is 20 again"
grep -q "Command Hole cancelled" <(tail -n +"$UI_MARK" "$UI_LOG") && ui_fail "Esc in the box cancelled the command"

ui_mark
ui_step "Depth's value"                    ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 10" "a field over the view edits the depth"
ui_step "type 12"                          xdotool type --delay 40 -- "12"
ui_sync
ui_step "Enter keeps it"                   ui_key Return
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 12 mm = 12 mm" "Depth is 12"
ui_expect_new "Preview Hole: ok" "the preview followed" 30
grep -q "^Done Hole\|^Added 1 hole" <(tail -n +"$UI_MARK" "$UI_LOG") && ui_fail "Enter in the box ended the command"

ui_mark
ui_step "Depth's value again"              ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 12" "the box opens with 12"
ui_step "type abc"                         xdotool type --delay 40 -- "abc"
ui_sync
ui_expect_new "Value box Hole depth invalid" "the box shows the error"
ui_mark
ui_step "Enter"                            ui_key Return
ui_expect_new "Value box Hole depth refused: abc" "Enter refused"
ui_sync
grep -q "Value box Hole depth closed\|^Done Hole\|^Added 1 hole" <(tail -n +"$UI_MARK" "$UI_LOG") &&
  ui_fail "Enter closed the box or ended the command with abc"
echo "ok   the box stays open"
ui_mark
ui_step "Esc puts 12 back"                 ui_key Escape
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 12" "Depth is 12 again"

ui_mark
ui_step "Depth's value once more"          ui_click_logged "Value box Hole depth"
ui_expect_new "Value box Hole depth opened: 12" "the box opens with 12"
ui_step "type 10"                          xdotool type --delay 40 -- "10"
ui_sync
ui_step "Enter keeps it"                   ui_key Return
ui_expect_new "Value box Hole depth closed" "the box closed"
ui_expect_new "Hole Depth: 10 mm = 10 mm" "Depth is 10"
ui_expect_new "Preview Hole: ok" "previewed" 30
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added 1 hole(s) (simple), diameter 10" "hole added"
# The hole of ui-create-test.sh (hole_simple_distance): 864.0506 mm3.
ui_expect_volume "Body Body1 (F2.b0): volume 48000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 47135.94936549554 }')" "one hole, 10 deep"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

hole_on_top "Docked" --chrome=docked
tab_steps
ui_step "Esc (cancel)"                     ui_key Escape
ui_stop_app

ui_finish "UI Hole entry test"
