#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/framework app/library app/OcctViewer.cpp
# check-all sources: core/model/src/build core/model-build core/model-api/src core/vcs/src/library tools/cli tools/ui-timber-lib.sh
# The TIMBER group's Timber tool, its loose members (mitcad#478, mitcad#520),
# on the members of a wall through the real UI, with a small library of
# synthetic parts (tools/ui-timber-lib.sh: a 48 x 98 timber member and a
# wood screw):
#   1. A wall from the origin placed with Wall (Enter); Timber's Mode has
#      no Frame button: On a member, Spacing and Two points.
#   2. Two points from a stud's side to the next stud's (mitcad#509): the
#      sides are not one plane, so the member is free between the points,
#      as long as the studs' distance, its end cut back to the end stud's
#      face (not its end moved onto the start's plane, a member a few
#      millimetres long).
#   3. On a member (mitcad#508): the hover over a member's side names only
#      the face a click picks, nothing previewed before the click; once
#      picked, the mouse along the side, off the parts and back previews
#      and inspects nothing, the preview's end A unchanged; a click on
#      another member's side moves it once. Closed without placing.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
source "$(dirname "$0")/ui-timber-lib.sh"

timber_library ui-timber

ui_start_display
# A new design (mitcad#394): Component1 active.
ui_start_app --no-native-dialogs

echo "--- 1. A wall to place members on; Timber's modes"
start_wall
expect_inspect Wall "frame wall Wall 1, wall 3600 × 2400 mm, 9 member(s)" "the wall from the origin along +X"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed frame Wall 1 (wall, lead C" "the wall frame placed" 60
ui_step "close Wall (Escape)"              ui_key Escape
ui_mark
ui_step "Timber (command search)"          ui_command "Timber"
ui_expect_new "Command Timber started" "Timber started"
ui_expect_new "Panel Timber input how=points at" "the Two points button"
ui_expect_log "Panel Timber input how=member at" "the On a member button"
ui_expect_log "Panel Timber input how=spacing at" "the Spacing button"
! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Panel Timber input how=frame " ||
  ui_fail "Timber still has a Frame mode (mitcad#520: walls, floors and roofs are tools of their own)"
echo "ok   no Frame mode"

echo "--- 2. Two points from one stud's side to the next one's (mitcad#509)"
ui_mark
ui_step "Mode: Two points"                 ui_click_logged "Panel Timber input how=points"
ui_expect_new "View hint Timber: Click where the member starts" "the view's hint for the start"
ui_step "fit (F6)"                         ui_key F6
ui_step "the start pick"                   ui_click_logged "Panel Timber input start"
# The studs and king studs (2304 mm, 98 x 48: 10838016 mm3) and, of each,
# its +X side: of its sides in sight, the one furthest right (the view from
# the front right: +X and +Y to the right).
studs=" $(sed -n -E 's/^New body .* \((F[0-9]+\.b[0-9]+)\): volume ([0-9.]+) mm3$/\2 \1/p' "$UI_LOG" |
  awk '$1 > 10838015 && $1 < 10838017 { print $2 }' | tr '\n' ' ')"
start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
sides=$(tail -n +"${start:-1}" "$UI_LOG" |
  sed -n -E 's/^Pick face (F[0-9]+\.b[0-9]+)\/[^ ]*:part\(F2:side\([^ ]* at ([0-9]+),([0-9]+)$/\1 \2 \3/p' |
  awk -v studs="$studs" 'index(studs, " " $1 " ") && (!($1 in x) || $2 > x[$1]) { x[$1] = $2; y[$1] = $3 }
    END { for (b in x) print x[b], y[b] }' | sort -n)
[ "$(wc -l <<< "$sides")" -ge 2 ] || ui_fail "fewer than two studs' sides in sight: $sides"
# The two rightmost: the stud at 3000 and the end stud. Their +X sides are
# not one plane. Before mitcad#509 the end was moved onto the start's
# plane, beside the start: a member a few millimetres long, refused.
read -r ax ay <<< "$(tail -2 <<< "$sides" | head -1)"
read -r bx by <<< "$(tail -1 <<< "$sides")"
ui_mark
ui_step "the start on a stud's side"       xdotool mousemove $((X + ax * UI_SCALE)) $((Y + ay * UI_SCALE)) click 1
ui_expect_new "View hint Timber: Click where it ends; Shift+click places at once" "the end next"
ui_mark
ui_step "the end on the next stud's side"  xdotool mousemove $((X + bx * UI_SCALE)) $((Y + by * UI_SCALE)) click 1
expect_inspect Timber "points, length " "; free" "the member free between the studs' sides"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Inspect Timber: points, length " | tail -1)
length=$(sed -n 's/^Inspect Timber: points, length \([0-9.]*\) mm.*/\1/p' <<< "$line")
awk -v l="${length:-0}" 'BEGIN { exit !(l > 450 && l < 700) }' ||
  ui_fail "the member between two studs 576 apart is ${length:-no} mm: $line"
cut_end=', end on [^;]* cut;'
[[ $line =~ $cut_end ]] || ui_fail "its end not cut back to the stud's face: $line"
echo "ok   $length mm between the studs, its end on the end stud's face"
ui_mark
ui_step "Place (Enter)"                    ui_key Return
ui_expect_new "Placed Timber 48x98 test as " "the member placed" 30
ui_expect_new "length by two points, " "a member by two points"
ui_step "close Timber (Escape)"            ui_key Escape

echo "--- 3. On a member: the preview stays where the click put it"
ui_mark
ui_step "Timber (command search)"          ui_command "Timber"
ui_expect_new "Command Timber started" "Timber again"
ui_step "Mode: On a member"                ui_click_logged "Panel Timber input how=member"
ui_step "fit the view (F6)"                ui_key F6
ui_mark
ui_step "Timber's Place On"                ui_click_logged "Panel Timber input at"
ui_expect_new "Pick places:" "the places a click picks"
# Two sides of the wall's members a click can reach, of two members.
start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
sides=$(tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick face " | grep -F ":part(F2:side(" |
  sed -n 's/^Pick face \(.*\) at [0-9]*,[0-9]*$/\1/p')
PICK1=$(printf '%s\n' "$sides" | sed -n 1p)
[ -n "$PICK1" ] || ui_fail "no side of the wall's members to pick"
BODY1=${PICK1%%/*}
FACE1=${PICK1#*/}
PICK2=$(printf '%s\n' "$sides" | grep -vF -- "$BODY1/" | head -1)
[ -n "$PICK2" ] || PICK2=$(printf '%s\n' "$sides" | grep -vxF -- "$PICK1" | head -1)
[ -n "$PICK2" ] || ui_fail "no second side of the wall's members to pick"
AT1=$(ui_pick_at "$PICK1 at")
# Before the click the hover names the face a click picks and previews
# nothing (mitcad#508).
ui_sync
ui_mark
ui_step "the mouse over a member's side"   xdotool mousemove $AT1
ui_expect_new "BUILD hover pick: face $FACE1 of $BODY1" "the hover names the face a click picks"
ui_step "the mouse a little further"       xdotool mousemove_relative -- 6 0
no_inspect Timber "no preview and no inspection while hovering before the click"
ui_mark
ui_step "a click on the side"              xdotool mousemove $AT1 click 1
expect_inspect Timber "place side" "; end A at " "the member on the side picked"
# Once picked the preview stays there: the mouse along the side, off the
# parts and back runs no placement and no inspection, its end A unchanged;
# a click on another member's side moves it once (mitcad#508).
ui_sync
END_A1=$(last_end_a Timber)
[ -n "$END_A1" ] || ui_fail "no end A in the side pick's inspection"
ui_mark
ui_step "the mouse along the side"         xdotool mousemove_relative -- 12 0
ui_step "the mouse off the parts"          off_view
ui_expect_new "BUILD hover pick: off the parts' faces" "the hover off the parts"
ui_step "the mouse on the side again"      xdotool mousemove $AT1
no_inspect Timber "no preview and no inspection while the mouse moves after the click"
[ "$(last_end_a Timber)" = "$END_A1" ] || ui_fail "the preview's end A moved from $END_A1"
echo "ok   the preview's end A still at $END_A1"
ui_mark
ui_step "a click on another member's side" ui_click_pick "$PICK2 at"
moved_once Timber "$END_A1" "a click on another member's side moves the preview once"
ui_step "close Timber (Escape)"            ui_key Escape

ui_finish "UI timber test"
