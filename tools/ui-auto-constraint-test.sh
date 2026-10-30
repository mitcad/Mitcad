#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/Dialogs.cpp core/solver
# Auto constraints while drawing (mitcad#189) through the real UI: the
# sketch palette's Auto constraint section, which links each new entity of
# a drawing tool to the first or the last one the tool drew:
#   - The section appears after the tool's first circle, under its exact
#     heading, Link to last chosen and nothing checked; before that, and
#     after the tool ends, it is hidden.
#   - Equal with Link to last: circles 1-2 and 2-3; with Link to first the
#     next ones are linked to circle 1. Changing the choices adds, changes
#     and removes nothing. The pointer over the section highlights the
#     reference.
#   - A new tool session starts over (Link to last, nothing checked).
#     Equal with Horizontal/Vertical alignment: equal circles whose centres
#     are level or plumb with the last one's, by which is nearer.
#     Concentric: the new circle on the last one's centre, through the
#     point clicked.
#   - Lines: Equal and Perpendicular with Link to last turn square corners
#     of equal sides; Parallel and Equal with Link to first follow the
#     first line, also when the cursor passed another line on the way.
#   - A constraint the sketch refuses (Equal to a circle of another
#     dimensioned size): a dialog lists it; Cancel takes the circle back,
#     Accept keeps it without the constraint. Successful drawing asks
#     nothing. One undo takes a circle back with its auto constraint, and
#     the next circle is linked to the one before it, not to the undone
#     one.
#
# Clicks are given in sketch millimetres (ui_sketch_click), from the
# placement the app logs; points snap to the grid's 10 mm lines.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-auto-constraint-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}

# zoom_to_show x1 y1 x2 y2: zooms out (the wheel at the view's middle)
# until the sketch rectangle from (x1, y1) to (x2, y2) mm is in the view.
zoom_to_show() {
  local area vx vy vw vh ax ay bx by
  for _ in $(seq 1 20); do
    area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
    read -r _ _ vx vy vw vh <<< "$area"
    read -r ax ay <<< "$(ui_sketch_at "$1" "$2")"
    read -r bx by <<< "$(ui_sketch_at "$3" "$4")"
    if [ "$ax" -gt $((X + vx + 20)) ] && [ "$bx" -lt $((X + vx + vw - 20)) ] &&
      [ "$by" -gt $((Y + vy + 20)) ] && [ "$ay" -lt $((Y + vy + vh - 20)) ]; then
      return 0
    fi
    xdotool mousemove $(ui_view_at 50 50) click 5
    ui_sync
  done
  ui_fail "the view did not zoom out to show ($1, $2) to ($3, $4)"
}

# The curve the circle and line tools logged last.
last_circle() { sed -n 's/.*Added circle, .* \[\(c[0-9]*\)\]$/\1/p' "$UI_LOG" | tail -1; }
last_line() { sed -n 's/.*Added line \(c[0-9]*\) from .*/\1/p' "$UI_LOG" | tail -1; }
# The sketch's degrees of freedom the app logged last.
sketch_dof() { sed -n 's/^Sketch Sketch1: \([0-9]*\) DOF.*/\1/p' "$UI_LOG" | tail -1; }

# expect_auto type reference made description: an auto constraint of that
# type from the reference to the new curve, since the mark.
expect_auto() {
  for _ in $(seq 1 30); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Auto constraint $1 k[0-9]+ on $2, $3\$" &&
      { echo "ok   $4"; return; }
    sleep 0.2
  done
  ui_fail "$4: no 'Auto constraint $1 k… on $2, $3' since the mark"
}

# no_new "regex" description: nothing matching since the mark.
no_new() {
  ui_sync
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE -- "$1"; then
    ui_fail "$2: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E -- "$1" | head -1)"
  fi
  echo "ok   $2"
}

# check name: a click on a palette check box or radio button where it was
# logged last ("Palette paletteAuto_equal at x,y").
check() { ui_click_logged "Palette $1"; }

CIRCLES="Equal ( ), Horizontal/Vertical alignment ( ), Concentric ( )"

ui_start_display
ui_start_app

ui_step "create sketch on XY"            ui_create_sketch xy
ui_step "zoom out"                       zoom_to_show -80 -65 100 60

echo "--- The section appears after the first circle"
ui_mark
ui_step "circle tool (C)"                ui_key c
ui_expect_new "Sketch tool Center Diameter Circle" "the circle tool"
no_new "Palette Auto constraint: Link" "no Auto constraint section before the first circle"
ui_step "circle 1: centre"               ui_sketch_click -60 30
ui_step "circle 1: rim"                  ui_sketch_click -50 30
ui_expect_new "Added circle, diameter 20 mm at (-60, 30)" "circle 1"
A=$(last_circle)
ui_expect_new "Palette Auto constraint: Link to first ( ), Link to last (x); $CIRCLES" \
  "the section under its heading: Link to last, nothing checked"
ui_expect_new "Palette paletteAuto_equal at" "its check boxes are placed"

echo "--- Equal, Link to last: circles 1-2 and 2-3"
ui_mark
ui_step "check Equal"                    check paletteAuto_equal
ui_expect_new "Auto constraint equal: on (checked: equal)" "Equal checked"
dof1=$(sketch_dof)
ui_mark
ui_step "circle 2: centre"               ui_sketch_click -30 30
ui_step "circle 2: a larger rim"         ui_sketch_click -10 30
ui_expect_new "Added circle, diameter 20 mm at (-30, 30)" "circle 2 as large as circle 1"
B=$(last_circle)
expect_auto equal "$A" "$B" "circle 2 equal to circle 1"
ui_step "circle 3: centre"               ui_sketch_click 0 30
ui_step "circle 3: rim"                  ui_sketch_click 20 30
ui_expect_new "Added circle, diameter 20 mm at (0, 30)" "circle 3 as large"
C=$(last_circle)
expect_auto equal "$B" "$C" "Link to last: circle 3 equal to circle 2"
ui_sync
# Circle 3's centre snapped onto the y axis and is held there (mitcad#948).
[ "$(sketch_dof)" = "$((dof1 + 3))" ] ||
  ui_fail "DOF $(sketch_dof), expected $((dof1 + 3)): one equal each, circle 3's centre on the y axis"
echo "ok   one constraint for each new circle, circle 3's centre on the y axis"
no_new "Auto constraint dialog" "no dialog when the constraints are added"

echo "--- The reference highlighted while the pointer is over the section"
ui_mark
ui_step "point at the section"           xdotool mousemove $(ui_logged_at "Palette paletteAutoConstraint")
ui_expect_new "Auto constraint reference highlighted: $C" "the last circle highlighted"
ui_step "back to the view"               xdotool mousemove $(ui_sketch_at 40 -10)
ui_expect_new "Auto constraint reference highlight off" "no highlight outside it"

echo "--- New choices change nothing drawn"
ui_mark
ui_step "Link to first"                  check paletteAutoFirst
ui_expect_new "Auto constraint link: first" "Link to first chosen"
ui_step "uncheck Equal"                  check paletteAuto_equal
ui_expect_new "Auto constraint equal: off (checked: none)" "Equal unchecked"
ui_step "check Equal again"              check paletteAuto_equal
ui_expect_new "Auto constraint equal: on (checked: equal)" "Equal checked again"
no_new "Sketch Sketch1: |Deleted |Undo: |Auto constraint [a-z_]+ k[0-9]+ on" \
  "no constraint added or removed, nothing moved"
ui_expect_new "Palette Auto constraint: Link to first (x), Link to last ( ); Equal (x), " \
  "the section shows the choices"

echo "--- Equal, Link to first: circle 4 linked to circle 1"
ui_mark
ui_step "circle 4: centre"               ui_sketch_click 30 30
ui_step "circle 4: rim"                  ui_sketch_click 50 30
ui_expect_new "Added circle, diameter 20 mm at (30, 30)" "circle 4 as large"
D=$(last_circle)
expect_auto equal "$A" "$D" "Link to first: circle 4 equal to circle 1"
ui_expect_log "Auto constraint target $A" "circle 1 was the target"

echo "--- Leaving the tool ends its session"
ui_mark
ui_step "end the tool (Esc)"             ui_key Escape
ui_expect_new "Palette Auto constraint: hidden" "the section hidden without a tool"
ui_step "circle tool (C)"                ui_key c
ui_step "circle 5: centre"               ui_sketch_click -60 -10
ui_step "circle 5: rim"                  ui_sketch_click -50 -10
ui_expect_new "Added circle, diameter 20 mm at (-60, -10)" "circle 5"
E=$(last_circle)
ui_expect_new "Palette Auto constraint: Link to first ( ), Link to last (x); $CIRCLES" \
  "a new session: Link to last, nothing checked"
no_new "Auto constraint [a-z_]+ k[0-9]+ on" "circle 5 is linked to nothing drawn before"

echo "--- Equal and Horizontal/Vertical alignment"
ui_mark
ui_step "check Equal"                    check paletteAuto_equal
ui_step "check the alignment"            check paletteAuto_horizontal_vertical
ui_expect_new "(checked: equal, horizontal_vertical)" "both checked"
ui_mark
ui_step "circle 6: centre, 10 above"     ui_sketch_click -30 0
ui_step "circle 6: rim"                  ui_sketch_click -20 0
ui_expect_new "Added circle, diameter 20 mm at (-30, -10)" "circle 6 level with circle 5, as large"
F=$(last_circle)
expect_auto equal "$E" "$F" "circle 6 equal to circle 5"
expect_auto horizontal_points "$E" "$F" "circle 6's centre level with circle 5's (further apart along x)"
ui_step "circle 7: centre, 10 aside"     ui_sketch_click -40 -40
ui_step "circle 7: rim"                  ui_sketch_click -20 -40
ui_expect_new "Added circle, diameter 20 mm at (-30, -40)" "circle 7 plumb with circle 6, as large"
G=$(last_circle)
expect_auto vertical_points "$F" "$G" "circle 7's centre plumb with circle 6's (further apart along y)"

echo "--- Concentric"
ui_mark
ui_step "check Concentric"               check paletteAuto_concentric
ui_expect_new "Auto constraint concentric: on (checked: equal, concentric)" \
  "Concentric checked, the alignment unchecked"
ui_step "uncheck Equal"                  check paletteAuto_equal
ui_expect_new "Auto constraint equal: off (checked: concentric)" "only Concentric"
ui_mark
ui_step "circle 8: centre elsewhere"     ui_sketch_click 10 -40
ui_step "circle 8: through a point"      ui_sketch_click -10 -40
ui_expect_new "Added circle, diameter 40 mm at (-30, -40)" "circle 8 on circle 7's centre, through the point"
H=$(last_circle)
expect_auto concentric "$G" "$H" "circle 8 concentric with circle 7"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- Lines: Equal and Perpendicular to the last turn square corners"
ui_mark
ui_step "line tool (L)"                  ui_key l
ui_step "line 1: start"                  ui_sketch_click 60 0
ui_step "line 1: end"                    ui_sketch_click 80 0
ui_expect_new "from (60, 0) to (80, 0) (horizontal)" "line 1"
L1=$(last_line)
ui_expect_new "Palette Auto constraint: Link to first ( ), Link to last (x); Equal ( ), Parallel ( ), Perpendicular ( )" \
  "the line tool offers Equal, Parallel and Perpendicular"
ui_expect_new "Palette paletteAuto_perpendicular at" "its check boxes are placed"
ui_step "check Equal"                    check paletteAuto_equal
ui_step "check Perpendicular"            check paletteAuto_perpendicular
ui_expect_new "(checked: equal, perpendicular)" "both checked"
ui_mark
ui_step "line 2: towards (80, 30)"       ui_sketch_click 80 30
ui_expect_new "Added line" "line 2"
L2=$(last_line)
ui_expect_new "Added line $L2 from (80, 0) to (80, 20)" "square to line 1, as long"
expect_auto perpendicular "$L1" "$L2" "line 2 perpendicular to line 1"
expect_auto equal "$L1" "$L2" "line 2 equal to line 1"
ui_step "line 3: towards (60, 30)"       ui_sketch_click 60 30
ui_expect_new "Added line" "line 3"
L3=$(last_line)
ui_expect_new "Added line $L3 from (80, 20) to (60, 20)" "square to line 2, as long"
expect_auto perpendicular "$L2" "$L3" "Link to last: line 3 perpendicular to line 2"

echo "--- Parallel and Equal to the first; a line passed does not take its place"
ui_mark
ui_step "Link to first"                  check paletteAutoFirst
ui_step "check Parallel"                 check paletteAuto_parallel
ui_expect_new "Auto constraint parallel: on (checked: equal, parallel)" "Parallel checked, Perpendicular unchecked"
ui_mark
ui_step "pass over line 2"               xdotool mousemove $(ui_sketch_at 80 10)
ui_sync
ui_step "line 4: towards (40, 10)"       ui_sketch_click 40 10
ui_expect_new "Added line" "line 4"
L4=$(last_line)
ui_expect_new "Added line $L4 from (60, 20) to (40, 20)" "along line 1, as long"
expect_auto parallel "$L1" "$L4" "line 4 parallel to line 1, not to the line passed"
expect_auto equal "$L1" "$L4" "line 4 equal to line 1"
no_new "Auto constraint [a-z_]+ k[0-9]+ on $L2," "nothing linked to the line passed"
ui_mark
ui_step "point at the section"           xdotool mousemove $(ui_logged_at "Palette paletteAutoConstraint")
ui_expect_new "Auto constraint reference highlighted: $L1" "line 1 highlighted as the reference"
ui_step "back to the view"               xdotool mousemove $(ui_sketch_at 40 -10)
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "end the chain and the tool"     ui_key Escape Escape
ui_expect_new "Palette Auto constraint: hidden" "hidden again"

echo "--- A refused constraint: Cancel, Accept; undo"
ui_mark
ui_step "circle tool (C)"                ui_key c
ui_step "circle 9: centre"               ui_sketch_click 20 -40
ui_step "type its diameter 20"           xdotool type --delay 40 "20"
ui_step "Enter"                          ui_key Return
ui_expect_new "Added circle, diameter 20 mm at (20, -40)" "circle 9, dimensioned"
P=$(last_circle)
ui_expect_new "Palette paletteAuto_equal at" "the section"
ui_step "check Equal"                    check paletteAuto_equal
ui_expect_new "(checked: equal)" "Equal checked"
dof9=$(sketch_dof)
ui_mark
ui_step "circle 10: centre"              ui_sketch_click 60 -40
ui_step "type another diameter, 30"      xdotool type --delay 40 "30"
ui_step "Enter"                          ui_key Return
ui_expect_new "Auto constraint dialog: Equal: " "the dialog lists the refused Equal"
ui_focus_dialog '^Auto Constraint$'
ui_step "Cancel (Esc)"                   ui_key Escape
ui_focus_main
ui_expect_new "Auto constraint cancelled: " "cancelled"
no_new "Added circle, diameter 30 mm" "the circle taken back"
[ "$(sketch_dof)" = "$dof9" ] || ui_fail "DOF $(sketch_dof) after Cancel, expected $dof9"
echo "ok   the sketch as before"
ui_mark
ui_step "circle 10 again: centre"        ui_sketch_click 60 -40
ui_step "type 30"                        xdotool type --delay 40 "30"
ui_step "Enter"                          ui_key Return
ui_expect_new "Auto constraint dialog: Equal: " "the dialog again"
ui_focus_dialog '^Auto Constraint$'
ui_step "Accept (Enter)"                 ui_key Return
ui_focus_main
ui_expect_new "Auto constraint accepted: " "accepted"
ui_expect_new "Added circle, diameter 30 mm at (60, -40)" "circle 10 kept, without Equal"
no_new "Auto constraint equal k[0-9]+ on" "no Equal added"
dof10=$(sketch_dof)
[ "$dof10" = "$((dof9 + 2))" ] || ui_fail "DOF $dof10, expected $((dof9 + 2)): its centre free, no Equal"
echo "ok   circle 10 with its own size"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "circle 10 undone"
ui_sync
[ "$(sketch_dof)" = "$dof9" ] || ui_fail "DOF $(sketch_dof) after the undo, expected $dof9"
echo "ok   one undo took circle 10 back"
ui_mark
ui_step "circle 11: centre"              ui_sketch_click 60 -40
ui_step "circle 11: rim"                 ui_sketch_click 80 -40
ui_expect_new "Added circle, diameter 20 mm at (60, -40)" "circle 11 as large as circle 9"
R=$(last_circle)
expect_auto equal "$P" "$R" "Link to last: circle 11 linked to circle 9, not to the undone one"
ui_sync
[ "$(sketch_dof)" = "$((dof9 + 2))" ] || ui_fail "DOF $(sketch_dof), expected $((dof9 + 2))"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: " "circle 11 undone"
ui_sync
[ "$(sketch_dof)" = "$dof9" ] || ui_fail "DOF $(sketch_dof) after the undo, expected $dof9"
echo "ok   one undo took circle 11 and its Equal back together"
count=$(grep -c "Auto constraint dialog" "$UI_LOG")
[ "$count" = 2 ] || ui_fail "$count dialogs, expected 2 (only the refused constraints ask)"
echo "ok   only the refused constraints asked"

ui_step "end the tool (Esc)"             ui_key Escape
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI auto constraint test"
