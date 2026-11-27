#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings tools/cli
# A base view's Shows leaves the view's other settings usable in a low
# window (mitcad#2498, GitHub #703) through the real UI, on Plate (a 40 x 30
# x 5 body) placed 20 times in a grid, Plate:1 to Plate:20, with a drawing
# whose base view draws all of them, that mitcad-cli makes. In the Docked
# and then the Floating layout:
#   - The view selected, the window made 1358 x 720 (as reported): Shows is
#     a heading folded over its list ("Shows: 20 of 20 bodies"); no field or
#     label of the panel overlaps another, and each of the view's settings
#     (Orientation to Delete View) is in sight, or comes into sight when the
#     panel's page is scrolled.
#   - Shows opened: its list takes no more than the room the panel has
#     beside the other settings (only its least, 3 rows, when the page has
#     to scroll); no overlap, each setting reachable.
#   - The list scrolled to its end, Plate:20 unfolded and its body's box
#     cleared (an individual body): 19 bodies, one edit; the panel built
#     again keeps the list open, Plate:20 unfolded, the same row at the top.
#   - Plate:19's box cleared (an occurrence, all its bodies): 18 bodies,
#     the place kept again.
#   - Ctrl+Z: one undo step (Edit view 1 of Drawing1) takes back the second
#     edit alone: 19 bodies, the place kept.
#   - The window lower (1358 x 560), then higher (1358 x 900): the list
#     still open; no overlap, each setting reachable, the list within the
#     room.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-shows-short-window-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$HOME/mitcad-ui-drawing-shows-short-window.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/plates.mitcad"

{
  echo '['
  echo '  {"cmd": "create_component", "name": "Plate"},'
  echo '  {"cmd": "sketch.create"},'
  echo '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},'
  echo "  {\"cmd\": \"add_feature\", \"def\": {\"type\": \"extrude\", \"profiles\": [{\"sketch\": \"F1\", \"region\": \"$R\"}],"
  echo '    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},'
  echo '  {"cmd": "activate_component", "component": "C0"},'
  # Plate:2 to Plate:20: five in a row, four rows.
  for k in $(seq 1 19); do
    echo "  {\"cmd\": \"copy_occurrence\", \"occurrence\": \"Plate:1\", \"transform\": {\"translation\": [$((45 * (k % 5))), $((35 * (k / 5))), 0]}},"
  done
  echo '  {"cmd": "add_drawing", "size": "A3"},'
  echo '  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top", "at": [200, 150], "scale": "1:1"}'
  echo ']'
} > "$WORK/plates.json"
"$CLI" run "$WORK/plates.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
echo "ok   a design of 20 plates with a drawing made"

# A base view's settings in its panel, from the top (the plates' view has
# no model annotations to keep, no breaks).
SETTINGS=(drawingViewOrientation drawingViewShowsToggle drawingViewUpdate drawingViewExplode
  drawingViewScale drawingViewRotation drawingViewStyle_hidden_lines drawingViewStyle_tangent_edges
  drawingViewStyle_shaded drawingViewLabel drawingViewCenterMarks drawingViewRetrievedDimensions
  drawingViewModelAnnotations drawingViewDelete)
FIRST_ROW="Plate:1"

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# panel_settled: waits until the panel logs nothing new over a sync and a
# short wait (its places, its page, its list's height).
panel_settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -c "^Drawing " "$UI_LOG")
    ui_sync
    sleep 0.3
    after=$(grep -c "^Drawing " "$UI_LOG")
    [ "$before" = "$after" ] && return 0
  done
  ui_fail "the panel's places did not settle"
}

# last_place name: the panel's last line of the field's place, or that it
# is out of sight.
last_place() {
  grep -E "^Drawing panel $1 (at -?[0-9]+,-?[0-9]+|out of sight)$" "$UI_LOG" | tail -1
}

# no_overlap why: the panel's last overlap line says none.
no_overlap() {
  local line
  line=$(grep "^Drawing panel overlaps: " "$UI_LOG" | tail -1)
  [ -n "$line" ] || ui_fail "$1: the panel's overlaps were not logged"
  [ "$line" = "Drawing panel overlaps: none" ] || ui_fail "$1: ${line#Drawing panel }"
}

# reveal name: the field in sight on the panel's page: the wheel over the
# page's scroll bar scrolls it down, or up once the page is at its end.
reveal() {
  local last page bar button=5 height inSight scrolled
  for _ in $(seq 1 40); do
    panel_settled
    last=$(last_place "$1")
    [ -n "$last" ] || ui_fail "the panel logged no place of $1"
    [[ $last == *" out of sight" ]] || return 0
    page=$(grep -E "^Drawing panel page height " "$UI_LOG" | tail -1)
    [[ $page =~ height\ ([0-9]+),\ in\ sight\ ([0-9]+),\ scrolled\ ([0-9]+)$ ]] ||
      ui_fail "the panel's page was not logged"
    height=${BASH_REMATCH[1]} inSight=${BASH_REMATCH[2]} scrolled=${BASH_REMATCH[3]}
    [ "$scrolled" -ge $((height - inSight)) ] && button=4
    bar=$(grep -E "^Drawing panel page bar at [0-9]+,[0-9]+$" "$UI_LOG" | tail -1)
    [[ $bar =~ \ at\ ([0-9]+),([0-9]+)$ ]] || ui_fail "$1 is out of sight and the panel's page has no scroll bar"
    xdotool mousemove $((X + BASH_REMATCH[1] * UI_SCALE)) $((Y + BASH_REMATCH[2] * UI_SCALE)) sleep 0.08 \
      click "$button"
  done
  ui_fail "$1 did not come into sight on the panel's page"
}

# row_in_sight row: the Shows list's row (its text) in sight: its heading
# and the list scrolled into the page's sight, and the row among the
# places the panel logged last (after its last overlap line: the panel logs
# its places whole, and a row out of sight has no line).
row_in_sight() {
  local row overlaps
  reveal drawingViewShowsToggle
  reveal drawingViewShows
  settled "Drawing panel drawingViewShows row $1"
  row=$(grep -nF "Drawing panel drawingViewShows row $1 at " "$UI_LOG" | tail -1 | cut -d: -f1)
  overlaps=$(grep -n "^Drawing panel overlaps: " "$UI_LOG" | tail -1 | cut -d: -f1)
  [ -n "$row" ] && [ -n "$overlaps" ] && [ "$row" -gt "$overlaps" ] ||
    ui_fail "the Shows row $1 is not in sight in the panel"
}

# check_panel why: no field or label overlaps another, every setting is in
# sight or scrolled into it (then without overlap too); with the list open,
# it is in the room the panel has: the page scrolls only with the list at
# its least.
check_panel() {
  local name page list height inSight
  panel_settled
  no_overlap "$1"
  for name in "${SETTINGS[@]}"; do
    reveal "$name"
    no_overlap "$1, $name in sight"
  done
  if [[ $(last_place drawingViewShows) != *" out of sight" ]] || [ "${2:-}" = open ]; then
    page=$(grep -E "^Drawing panel page height " "$UI_LOG" | tail -1)
    [[ $page =~ height\ ([0-9]+),\ in\ sight\ ([0-9]+), ]] || ui_fail "$1: the panel's page was not logged"
    height=${BASH_REMATCH[1]} inSight=${BASH_REMATCH[2]}
    list=$(grep -E "^Drawing view 1 Shows list height " "$UI_LOG" | tail -1)
    [[ $list =~ height\ ([0-9]+)\ \(at\ least\ ([0-9]+), ]] || ui_fail "$1: the list's height was not logged"
    if [ "$height" -gt $((inSight + 4)) ] && [ "${BASH_REMATCH[1]}" -gt "${BASH_REMATCH[2]}" ]; then
      ui_fail "$1: the page ($height px) is higher than its view ($inSight px) with the list above its least ($list)"
    fi
    echo "ok   $1: the list within the room (${list#Drawing view 1 Shows }; page $height of $inSight px)"
  fi
  echo "ok   $1: no field overlaps another, each setting in sight or scrolled into it"
}

# view_draws view bodies why: since the mark, the log's line of the view's
# projection (mitcad#1033) with that many bodies.
view_draws() {
  local pattern="^Drawing projection of view $1: $2 bodies, [0-9]+\.[0-9] ms, (projected exactly|projected from a triangulation|from the cache)\$"
  for _ in $(seq 1 30); do
    if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "$pattern"; then
      echo "ok   view $1 draws $2 bodies: $3"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "view $1 does not draw $2 bodies ($3): no such line of its projection in the log"
}

# place_kept why: since the mark, the panel was built again with the Shows
# list open at the place it had (mitcad#2434, mitcad#2498).
place_kept() {
  local places
  ui_expect_new "Drawing view 1 Shows place after the rebuild: " "the panel built again"
  ui_sync
  sleep 0.5
  ui_sync
  places=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n 's/^Drawing view 1 Shows place \(before\|after\) the rebuild: //p' | sort -u)
  [ "$(wc -l <<< "$places")" = 1 ] || ui_fail "$1: the list's place changed with the rebuild: $(tr '\n' ';' <<< "$places")"
  case "$places" in
  "0 unfolded,"*) ui_fail "$1: the list lost its unfolded occurrences ($places)" ;;
  *"top row $FIRST_ROW,"* | *"top row none,"*) ui_fail "$1: the list is back at its first row ($places)" ;;
  *"current row none") ui_fail "$1: the list lost its current row ($places)" ;;
  esac
  [[ $(last_place drawingViewShows) != *" out of sight" ]] || ui_fail "$1: the list is folded or out of sight again"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Drawing view 1 Shows folded" &&
    ui_fail "$1: the list was folded by the rebuild"
  echo "ok   $1: the list open, at its place ($places)"
}

# clear_current bodies why: Space on the current row (a body's or an
# occurrence's) clears its box: one edit of the view, drawn with that many
# bodies, the heading's count and the list's place kept.
clear_current() {
  ui_mark
  ui_step "clear its box (Space)"          ui_key space
  ui_expect_new "Drawing view 1 Shows changed: $1 bodies" "the bodies left sent"
  ui_expect_new '"cmd":"edit_drawing_view"' "one edit of the view"
  view_draws 1 "$1" "$2"
  ui_expect_new "Drawing view 1 shows $1 of 20 bodies" "its Shows built again with the box cleared"
  place_kept "$2"
}

# window_size width height: the window made that size.
window_size() {
  ui_mark
  ui_step "the window $1 x $2"             xdotool windowsize "$UI_WINDOW" $(($1 * UI_SCALE)) $(($2 * UI_SCALE))
  ui_expect_new "Window layout: window $1x" "the window is $1 px wide"
  ui_wait_idle
  ui_sync
  eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
  echo "ok   $(grep -oE "^Window layout: window [0-9]+x[0-9]+" "$UI_LOG" | tail -1 | sed 's/^Window layout: //')"
}

drawings_in_sight() {
  ui_expect_log "Browser opened " "the browser fitted to the design"
  if grep -q "^Browser opened folded" "$UI_LOG" && ! ui_place_valid "Browser Root/Drawings/Drawing1"; then
    ui_mark
    ui_step "open the Drawings folder"     ui_click_logged "Browser arrow Root/Drawings"
    ui_expect_new "Browser Root/Drawings/Drawing1 at " "the drawing's row in sight"
  fi
}

# low_window layout: the scenario above in the window's layout.
low_window() {
  echo "--- $1: the view selected in a 1358 x 720 window"
  UI_MARK=0 # a new log
  ui_expect_log "plates.mitcad" "the design opened"
  drawings_in_sight
  ui_mark
  ui_step "open Drawing1 (double-click)"   ui_double_click_logged "Browser Root/Drawings/Drawing1"
  ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
  ui_expect_new "Drawing sheet drawn: " "its sheet drawn"
  settled "Drawing view 1"
  ui_mark
  ui_step "select the view"                ui_click_logged "Drawing view 1"
  ui_expect_new "Drawing view 1 selected" "the view selected"
  ui_expect_new "Drawing view 1 shows 20 of 20 bodies" "its Shows: the 20 bodies checked"
  window_size 1358 720
  panel_settled
  [[ $(last_place drawingViewShows) == *" out of sight" ]] || ui_fail "the Shows list is not folded at first"
  echo "ok   Shows folded under its heading"
  check_panel "$1, 1358 x 720, Shows folded"

  echo "--- $1: Shows opened"
  reveal drawingViewShowsToggle
  settled "Drawing panel drawingViewShowsToggle"
  ui_mark
  ui_step "open Shows (its heading)"       ui_click_logged "Drawing panel drawingViewShowsToggle"
  ui_expect_new "Drawing view 1 Shows opened" "the list opened under its heading"
  ui_expect_new "Drawing view 1 Shows list height " "the list fitted to the panel"
  check_panel "$1, 1358 x 720, Shows open" open

  echo "--- $1: Plate:20's body cleared, then Plate:19's occurrence"
  row_in_sight "$FIRST_ROW"
  ui_mark
  ui_step "Plate:1's row"                  ui_click_logged "Drawing panel drawingViewShows row $FIRST_ROW"
  ui_sync
  ui_step "to the last row (End)"          ui_key End
  ui_sync
  ui_step "unfold Plate:20 (Right)"        ui_key Right
  ui_expect_new "Drawing panel drawingViewShows row Plate:20 at" "the list at its end"
  ui_sync
  ui_step "to its body (Right)"            ui_key Right
  ui_sync
  clear_current 19 "Plate:20's body cleared"
  check_panel "$1, after the edit" open
  row_in_sight "Plate:19"
  ui_mark
  ui_step "Plate:19's row"                 ui_click_logged "Drawing panel drawingViewShows row Plate:19"
  ui_sync
  clear_current 18 "Plate:19's occurrence cleared"

  echo "--- $1: one undo step"
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: Edit view 1 of Drawing1" "one undo step"
  view_draws 1 19 "Plate:19's body back"
  ui_expect_new "Drawing view 1 shows 19 of 20 bodies" "its Shows built again with the box checked"
  place_kept "undone"

  echo "--- $1: a lower window, then a higher one"
  window_size 1358 560
  check_panel "$1, 1358 x 560" open
  window_size 1358 900
  check_panel "$1, 1358 x 900" open

  grep -q "Drawing command refused" "$UI_LOG" && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display

ui_start_app --open "$FILE" --no-native-dialogs
low_window "Docked"

ui_stop_app
ui_start_app --chrome=floating --open "$FILE" --no-native-dialogs
low_window "Floating"

ui_finish "UI drawing Shows short window test"
