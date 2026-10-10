#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/commands
# The command framework through the real UI (app/COMMANDS.md): a panel
# generated from a definition, a group menu of the toolbar, a shortcut from
# the settings, selection with Ctrl and Shift and with windows, the context
# menu, an expression input (d1*2) whose preview fails and OK that refuses
# it, Cancel, editing a feature with its inputs filled in, one undo step per
# edit, and the command search refusing a command that cannot run.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-command-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

# A shortcut of our own, as the Keyboard Shortcuts dialog saves it.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[shortcuts]\nsolid.chamfer=Shift+C\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

# expect_count "text" n description: the log has n lines with the text.
expect_count() {
  local count
  count=$(grep -cF -- "$1" "$UI_LOG")
  [ "$count" = "$2" ] || ui_fail "$3: $count lines with '$1', expected $2"
  echo "ok   $3"
}

ui_start_display
ui_start_app --demo
ui_step "fit (F6)"                         ui_key F6

echo "--- A panel from a definition, opened from the MODIFY menu"
ui_step "open the MODIFY menu"             ui_click_logged "Ribbon SOLID/MODIFY"
ui_step "choose Chamfer (third entry)"     ui_key Down Down Down Return
ui_expect_log "Command Chamfer started" "chamfer started from the menu"
for input in sets.0.edges sets.0.type sets.0.distance tangent_chain; do
  ui_expect_log "Panel Chamfer input $input at" "panel has the $input input"
done
for input in sets.0.distance2 sets.0.angle sets.0.flip; do
  grep -E "Panel Chamfer input $input at [0-9]+,[0-9]+ \(hidden\)" "$UI_LOG" > /dev/null ||
    ui_fail "the $input input is not hidden for an equal distance chamfer"
  echo "ok   $input hidden until its chamfer type is chosen"
done
ui_expect_log "Panel Chamfer OK at" "panel has OK"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_log "Command Chamfer cancelled" "cancelled"

echo "--- Shortcut from the settings"
ui_step "Shift+C"                          ui_key shift+c
expect_count "Command Chamfer started" 2 "Shift+C starts Chamfer"
ui_step "cancel (Esc)"                     ui_key Escape

echo "--- Selection outside commands: click, Ctrl, Shift"
ui_step "pick the top face"                ui_view_click 50 50
ui_expect_log "Selected: 1 face [face F2:end(" "top face selected"
xdotool keydown ctrl
ui_step "Ctrl+click a side face"           ui_view_click 34 70
xdotool keyup ctrl
ui_expect_log "Selected: 2 faces" "Ctrl adds a face"
xdotool keydown shift
ui_step "Shift+click it again"             ui_view_click 34 70
xdotool keyup shift
expect_count "Selected: 1 face [face F2:end(" 2 "Shift takes it away again"

echo "--- Context menu: commands that fit a face"
ui_step "right-click the top face"         ui_view_click 50 50 3
ui_expect_log "Context menu: " "context menu opened"
for entry in Fillet Chamfer "Create Sketch" "Edit Extrude1"; do
  grep "Context menu: " "$UI_LOG" | tail -1 | grep -qF "$entry" ||
    ui_fail "no '$entry' in the context menu of a face"
  echo "ok   '$entry' offered for a face"
done
grep "Context menu: " "$UI_LOG" | tail -1 | grep -qE "\| Extrude \||Revolve" &&
  ui_fail "profile commands offered for a face"
ui_step "choose Fillet"                    ui_menu_choose "Fillet"
ui_expect_log "Command Fillet started" "fillet started from the context menu"
ui_expect_log "Fillet Edges: 1 face" "the selected face went to the edges input"

echo "--- An expression input; a failing preview refuses OK"
ui_step "type d1*2 (120 mm)"               xdotool type --delay 40 "d1*2"
ui_expect_log "Fillet Radius: d1*2 = 120 mm" "expression evaluated with the parameters"
ui_expect_log "Preview Fillet: failed" "the preview shows the failure"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_step "OK (Enter) is refused"            ui_key Return
ui_expect_log "Fillet: OK refused" "OK refused while the preview fails"
grep -q "Added fillet" "$UI_LOG" && ui_fail "a failing fillet was added"
ui_step "select the text"                  ui_key ctrl+a
ui_step "type d9 (unknown)"                xdotool type --delay 40 "d9"
ui_expect_log "Fillet Radius: d9 is invalid" "an unknown name is shown as an error"
ui_step "select the text"                  ui_key ctrl+a
ui_step "type d1/30 (2 mm)"                xdotool type --delay 40 "d1/30"
ui_expect_log "Fillet Radius: d1/30 = 2 mm" "expression evaluated"
ui_expect_log "Preview Fillet: ok" "preview computed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added fillet on 4 edge(s)" "fillet added"
ui_expect_log "Body Body1 (F2.b0): volume 48000.000 ->" "the body lost volume"

echo "--- Edit from the context menu, inputs filled in from the feature"
ui_step "right-click the top face"         ui_view_click 50 50 3
ui_step "choose Edit Extrude1"             ui_menu_choose "Edit Extrude1"
ui_expect_log "Editing F2 with Extrude" "editing Extrude1"
ui_expect_log "Extrude Distance: 20" "the distance came from the feature"
ui_step "type 30"                          xdotool type --delay 40 "30"
ui_expect_log "Preview Extrude: ok" "preview of the edited extrusion"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited F2" "extrude edited"
volumes=$(sed -n 's/.*Body Body1 (F2.b0): volume \([0-9.]*\) -> \([0-9.]*\) mm3.*/\1 \2/p' \
  "$UI_LOG" | tail -1)
read -r before after <<< "$volumes"
awk -v b="$before" -v a="$after" 'BEGIN { exit !((a - b - 24000) ^ 2 < 1e-4) }' ||
  ui_fail "10 mm more of a 60 x 40 block should add 24000 mm3, volume went $before -> $after"
echo "ok   10 mm higher: volume $before -> $after mm3"
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_log "Undo: Edit Extrude1" "the edit is one undo step"
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_log "Undo: Add Fillet1" "the roll-back left no undo step"

echo "--- Window selection"
ui_step "clear the selection (Esc)"        ui_key Escape
ui_step "window from left to right"        ui_view_drag 1 3 3 97 97
grep -E "Selected: [0-9]+ edges" "$UI_LOG" > /dev/null || ui_fail "the window selected no edges"
echo "ok   $(grep -E 'Selected: [0-9]+ edges' "$UI_LOG" | tail -1 | cut -c1-60)"
ui_step "clear the selection (Esc)"        ui_key Escape

echo "--- Create Sketch on the selected planar face"
ui_step "pick the top face"                ui_view_click 50 50
ui_step "create sketch (search)"           ui_command "Create Sketch"
ui_expect_log 'Sketch started on {"body":"F2.b0","face":"F2:end(' "sketch on the top face"
ui_expect_log "origin (0, 0, 20)" "the sketch plane is the top face"
ui_step "finish the empty sketch"          ui_key ctrl+Return
ui_expect_log "Sketch finished" "empty sketch left"

echo "--- The command search offers only what can run"
ui_step "search (S)"                       ui_key s
ui_expect_log "Command search opened" "search opened"
ui_step "type finish sketch"               xdotool type --delay 40 "finish sketch"
ui_step "Enter"                            ui_key Return
grep -q "Command search chose sketch.finish" "$UI_LOG" && ui_fail "Finish Sketch ran outside a sketch"
echo "ok   Finish Sketch not run outside a sketch"
ui_step "close the search (Esc)"           ui_key Escape

ui_finish "UI command test"
