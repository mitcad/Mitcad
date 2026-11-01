#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework app/commands app/MainWindow.cpp app/MainWindowContextMenu.cpp
# The command framework through the real UI (app/COMMANDS.md): a panel
# generated from a definition, a group menu of the toolbar, a shortcut from
# the settings, Create Sketch by its default Alt+S with no default shortcut
# in conflict (mitcad#267), keys per context set in Keyboard Shortcuts by
# its keys and a conflict its OK refuses (mitcad#720: S draws a spline in a
# sketch, F3 searches there; mitcad#730: no Context column while one context
# is filtered, the selected row kept over a filter change, the reserved
# Ctrl+F as a read-only row under Search Timeline), selection with Ctrl and Shift and with windows,
# the context menu, an expression input (d1*2) whose preview fails and OK
# that refuses it, Cancel, editing a feature with its inputs filled in, one
# undo step per edit, and the command search refusing a command that cannot
# run.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-command-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

# Shortcuts of our own, as the Keyboard Shortcuts window saves them: Shift+C
# for Chamfer, F9 for Keyboard Shortcuts itself.
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[shortcuts]\nsolid.chamfer=Shift+C\ntools.shortcuts=F9\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

# expect_count "text" n description: the log has n lines with the text.
expect_count() {
  local count
  count=$(grep -cF -- "$1" "$UI_LOG")
  [ "$count" = "$2" ] || ui_fail "$3: $count lines with '$1', expected $2"
  echo "ok   $3"
}

# expect_lines "line" n description: the log has n lines that are exactly
# the line (not a longer one containing it, such as the message band's
# copy of a hint, mitcad#1042).
expect_lines() {
  local count
  count=$(grep -cxF -- "$1" "$UI_LOG")
  [ "$count" = "$2" ] || ui_fail "$3: $count lines '$1', expected $2"
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

echo "--- Create Sketch by its default shortcut (Alt+S, mitcad#267)"
# The registry's defaults with this test's own Shift+C: a conflict would
# have been logged at the start and the binding disabled.
grep -qF "Conflicting saved shortcuts" "$UI_LOG" && ui_fail "shortcuts conflict: $(grep -F -A3 "Conflicting saved shortcuts" "$UI_LOG")"
echo "ok   no default shortcut in conflict"
ui_step "Alt+S"                            ui_key alt+s
expect_lines "Create Sketch: select a plane" 1 "Alt+S starts Create Sketch"
ui_step "cancel (Esc)"                     ui_key Escape

echo "--- Keyboard Shortcuts (mitcad#720): Command Search F3 in Sketch, Fit Point Spline S"
ui_mark
ui_step "Keyboard Shortcuts (F9)"          ui_key F9
ui_expect_new "Keyboard Shortcuts opened" "the window opened"
ui_focus_dialog '^Keyboard Shortcuts$'
ui_step "find Command Search"              xdotool type --delay 40 "Command Search"
ui_step "to its row (Down)"                ui_key Down
ui_expect_new "Keyboard Shortcuts row: Command Search | All | S" "one row, S in all its contexts"
ui_step "expand it (Right)"                ui_key Right
ui_expect_new "Keyboard Shortcuts expanded: Command Search" "its contexts shown"
ui_step "its Sketch row (Down Down)"       ui_key Down Down
ui_expect_new "Keyboard Shortcuts row: Command Search | Sketch | S" "its Sketch row"
ui_step "record its key (Enter)"           ui_key Return
ui_expect_new "Keyboard Shortcuts recording: Command Search | Sketch" "recording the key"
ui_step "F3"                               ui_key F3
ui_expect_new "Keyboard Shortcuts key: Command Search | Sketch = F3" "F3 in Sketch"
ui_expect_new "Keyboard Shortcuts row: Command Search | All | Varies" "its keys vary by context"
ui_step "back to the search (Shift+Tab)"   ui_key shift+Tab shift+Tab
ui_step "find Fit Point Spline"            xdotool type --delay 40 "Fit Point Spline"
ui_step "to its row (Down)"                ui_key Down
ui_expect_new "Keyboard Shortcuts row: Fit Point Spline | Sketch | (none)" "the spline's row"
ui_step "record its key (Enter)"           ui_key Return
ui_step "S"                                ui_key s
ui_expect_new "Keyboard Shortcuts key: Fit Point Spline | Sketch = S" "S for the spline"
ui_step "OK (Enter in the search)"         ui_key shift+Tab shift+Tab Return
ui_expect_new "Keyboard Shortcuts saved" "OK passed"
ui_expect_new "Shortcut keys of tools.search by context: 3D View S, Sketch F3, Command Panel S" \
  "Command Search's keys per context"
grep -qF "Conflicting saved shortcuts" "$UI_LOG" && ui_fail "the saved shortcuts conflict"
echo "ok   no shortcut in conflict"
ui_focus_main

echo "--- Keyboard Shortcuts: OK refuses F3 for the spline too"
ui_mark
ui_step "Keyboard Shortcuts (F9)"          ui_key F9
ui_focus_dialog '^Keyboard Shortcuts$'
ui_step "find Fit Point Spline"            xdotool type --delay 40 "Fit Point Spline"
ui_step "to its row (Down)"                ui_key Down
ui_step "record its key (Enter)"           ui_key Return
ui_step "F3"                               ui_key F3
ui_expect_new "Keyboard Shortcuts key: Fit Point Spline | Sketch = F3" "F3 for the spline"
ui_step "OK (Enter in the search)"         ui_key shift+Tab shift+Tab Return
ui_expect_new " in a sketch. Change one of them." "OK names the conflict and its context"
ui_expect_new "Keyboard Shortcuts conflict at: Fit Point Spline | Sketch" "the conflicting row selected"
grep -qF "Keyboard Shortcuts saved" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a conflict was saved"
ui_focus_replace "Keyboard Shortcuts"
ui_step "close the message (Enter)"        ui_key Return
ui_focus_dialog '^Keyboard Shortcuts$'
echo "--- Keyboard Shortcuts filtered to Sketch: a key there changes that context alone"
ui_mark
ui_step "to the context list (Shift+Tab)"  ui_key shift+Tab
ui_step "3D View (Down)"                     ui_key Down
ui_expect_new "Keyboard Shortcuts context filter: 3D View" "the 3D View context filtered on the way"
# The Sketch filter's own lines from here on.
ui_mark
ui_step "Sketch (Down)"                    ui_key Down
ui_expect_new "Keyboard Shortcuts context filter: Sketch" "the Sketch context filtered"
# mitcad#730: no Context column repeating Sketch; the spline's row, not
# shown in 3D View on the way, selected again.
ui_expect_new "Keyboard Shortcuts columns: Command, Shortcut" "the Context column hidden"
ui_expect_new "Keyboard Shortcuts selection kept: Fit Point Spline | Sketch" "the spline's row still selected"
ui_step "back to the search (Shift+Tab)"   ui_key shift+Tab
ui_step "find Command Search"              xdotool type --delay 40 "Command Search"
ui_step "to its row (Down)"                ui_key Down
ui_expect_new "Keyboard Shortcuts row: Command Search | Sketch | F3" "one row: its Sketch key"
ui_step "record its key (Enter)"           ui_key Return
ui_step "F5"                               ui_key F5
ui_expect_new "Keyboard Shortcuts key: Command Search | Sketch = F5" "F5 in Sketch"
ui_expect_new "Keyboard Shortcuts keys of Command Search: 3D View S, Sketch F5, Command Panel S" \
  "3D View and Command Panel keep S"
echo "--- Keyboard Shortcuts: all contexts again, the selection kept (mitcad#730)"
ui_mark
ui_step "to the context list (Shift+Tab)"  ui_key shift+Tab
ui_step "All Contexts (Home)"              ui_key Home
ui_expect_new "Keyboard Shortcuts context filter: All Contexts" "all contexts shown"
ui_expect_new "Keyboard Shortcuts columns: Command, Context, Shortcut" "the Context column back"
ui_expect_new "Keyboard Shortcuts selection kept: Command Search | Sketch" "Command Search's Sketch row selected"
# The row the user chose stays the one kept: 3D View on the way selects the
# 3D View row, and All Contexts again the Sketch row.
ui_mark
ui_step "3D View (Down)"                     ui_key Down
ui_expect_new "Keyboard Shortcuts selection kept: Command Search | 3D View" "its 3D View row in 3D View"
ui_mark
ui_step "All Contexts (Up)"                ui_key Up
ui_expect_new "Keyboard Shortcuts context filter: All Contexts" "all contexts again"
ui_expect_new "Keyboard Shortcuts selection kept: Command Search | Sketch" "the Sketch row chosen first, again"
echo "--- Keyboard Shortcuts: the reserved Ctrl+F under Search Timeline, read-only (mitcad#730)"
ui_mark
ui_step "back to the search (Shift+Tab)"   ui_key shift+Tab
ui_step "find Search Timeline"             xdotool type --delay 40 "Search Timeline"
ui_step "to its row (Down)"                ui_key Down
ui_expect_new "Keyboard Shortcuts row: Search Timeline | All | (none)" "the command, no key of its own"
ui_step "to its last row (End)"            ui_key End
ui_expect_new "Keyboard Shortcuts row: Search Timeline | All | Ctrl+F (reserved)" "its reserved Ctrl+F"
ui_step "try to record a key (Enter)"      ui_key Return
ui_expect_new "Keyboard Shortcuts reserved: Search Timeline | Ctrl+F is reserved for Search Timeline." \
  "a reserved key cannot be changed, and says so"
grep -qF "Keyboard Shortcuts recording:" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a reserved key was recorded"
echo "ok   no recording on the reserved row"
# Under the command, not a row of its own: Left goes up to the command.
ui_mark
ui_step "to its parent (Left)"             ui_key Left
ui_expect_new "Keyboard Shortcuts row: Search Timeline | All | (none)" "Ctrl+F is a row under Search Timeline"
ui_step "cancel (Esc)"                     ui_key Escape
ui_focus_main

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
for entry in "Create Sketch" "Edit Extrude1"; do
  grep "Context menu: " "$UI_LOG" | tail -1 | grep -qF "$entry" ||
    ui_fail "no '$entry' in the context menu of a face"
  echo "ok   '$entry' offered for a face"
done
# The face completes Fillet and Chamfer: Solid's first entries (mitcad#494).
for entry in Fillet Chamfer; do
  grep "Context menu Solid: " "$UI_LOG" | tail -1 | grep -qF "$entry" ||
    ui_fail "no '$entry' in the Solid submenu of a face"
  echo "ok   '$entry' offered for a face"
done
# A planar face is a profile of Extrude (mitcad#535), which the face alone
# completes: in Solid. It is a profile of Revolve too (mitcad#543), which
# needs an axis besides: offered in Solid More.
grep "Context menu Solid: " "$UI_LOG" | tail -1 | grep -qE "[:|] Extrude( \||$)" ||
  ui_fail "no 'Extrude' in the Solid submenu of a planar face"
echo "ok   'Extrude' offered for a planar face"
grep "Context menu Solid More: " "$UI_LOG" | tail -1 | grep -qE "[:|] Revolve( \||$)" ||
  ui_fail "no 'Revolve' in the Solid More submenu of a planar face"
echo "ok   'Revolve' offered for a planar face"
ui_step "choose Solid > Fillet"            ui_menu_choose "Solid" "Fillet"
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

echo "--- Keys per context (mitcad#720): S and F3 in the sketch"
searches=$(grep -c "Command search opened" "$UI_LOG")
ui_step "spline (S in the sketch)"         ui_key s
ui_expect_log "Sketch tool Fit Point Spline" "S starts the spline in a sketch"
expect_count "Command search opened" "$searches" "S opens no command search in a sketch"
ui_step "end the tool (Esc)"               ui_key Escape
ui_step "search (F3 in the sketch)"        ui_key F3
expect_count "Command search opened" $((searches + 1)) "F3 opens the command search in a sketch"
ui_step "close the search (Esc)"           ui_key Escape
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
