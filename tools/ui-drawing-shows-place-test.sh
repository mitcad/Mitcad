#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings tools/cli
# A base view's Shows list keeps its place through visibility edits
# (mitcad#2434, GitHub #683) through the real UI, on Plate (a 40 x 30 x 5
# body) placed 20 times in a grid, Plate:1 to Plate:20, with a drawing
# whose base view draws all of them, that mitcad-cli makes. In the Docked
# and then the Floating layout:
#   - The view selected and its Shows opened under its heading (folded at
#     first, mitcad#2498); the list (20 folded occurrences, 14 rows in
#     sight) scrolled to its end (End), Plate:20 unfolded and its body's
#     box cleared (Right, Right, Space): the view draws 19 bodies, and the
#     panel built again keeps the list's place (the same occurrences
#     unfolded, the same row at the top, the same current row; Plate:1's
#     row stays out of sight).
#   - Plate:19 unfolded too and its body's box cleared: 18 bodies, the
#     place kept again.
#   - Ctrl+Z: one undo step (Edit view 1 of Drawing1) takes back the second
#     edit alone: 19 bodies, the place kept.
# Then, when the private test files have it (MITCAD_MITCAD_REPORTED,
# github-683, the archive and the design in it found by their content
# ids), the reported design, Docked: one occurrence (Component1:1) of 100
# bodies, its drawing's front view drawing 98 of them. Component1:1
# unfolded (Right), the list scrolled to its end (End) and Body100 cleared
# (Space): 97 bodies, the place kept (Component1:1 still unfolded, its row
# out of sight); Body99 cleared next: 96, kept; Ctrl+Z: 97, kept.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-shows-place-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$HOME/mitcad-ui-drawing-shows-place.XXXXXX")
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

# The design's Shows list: its first row and how many bodies it lists.
FIRST_ROW="Plate:1"
TOTAL=20

# content_id file: the first 8 hex digits of its SHA-256.
content_id() {
  sha256sum "$1" | cut -c1-8
}

# place_kept why: since the mark, the panel was built again with the Shows
# list at the place it had (mitcad#2434): every "place before the rebuild"
# and "place after the rebuild" line says the same, the list scrolled away
# from its first row, and that row was not logged in sight again.
place_kept() {
  local places
  ui_expect_new "Drawing view 1 Shows place after the rebuild: " "the panel built again"
  ui_sync
  sleep 0.5
  ui_sync
  places=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n 's/^Drawing view 1 Shows place \(before\|after\) the rebuild: //p' | sort -u)
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "^Drawing view 1 Shows place before the rebuild: " ||
    ui_fail "$1: the list's place before the rebuild was not logged"
  [ "$(wc -l <<< "$places")" = 1 ] || ui_fail "$1: the list's place changed with the rebuild: $(tr '\n' ';' <<< "$places")"
  case "$places" in
  *"top row $FIRST_ROW,"* | *"top row none,"*) ui_fail "$1: the list is back at its first row ($places)" ;;
  *"current row none") ui_fail "$1: the list lost its current row ($places)" ;;
  esac
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Drawing panel drawingViewShows row $FIRST_ROW at " &&
    ui_fail "$1: $FIRST_ROW's row came into sight again (the list scrolled back to its top)"
  echo "ok   $1: the list kept its place ($places)"
}

# clear_current bodies why: Space on the current row (a body's) clears its
# box: one edit of the view, drawn with that many bodies, the place kept.
clear_current() {
  ui_mark
  ui_step "clear its box (Space)"          ui_key space
  ui_expect_new "Drawing view 1 Shows changed: $1 bodies" "the bodies left sent"
  ui_expect_new '"cmd":"edit_drawing_view"' "one edit of the view"
  view_draws 1 "$1" "$2"
  ui_expect_new "Drawing view 1 shows $1 of $TOTAL bodies" "its Shows built again with the box cleared"
  place_kept "$2"
}

drawings_in_sight() {
  ui_expect_log "Browser opened " "the browser fitted to the design"
  if grep -q "^Browser opened folded" "$UI_LOG" && ! ui_place_valid "Browser Root/Drawings/Drawing1"; then
    ui_mark
    ui_step "open the Drawings folder"     ui_click_logged "Browser arrow Root/Drawings"
    ui_expect_new "Browser Root/Drawings/Drawing1 at " "the drawing's row in sight"
  fi
}

# open_view file shown: in the app just started on the design file, its
# Drawing1 opened and its view 1 selected, whose Shows checks that many of
# its TOTAL bodies; the list's first row settled in sight.
open_view() {
  UI_MARK=0 # a new log
  ui_expect_log "$1" "the design opened"
  drawings_in_sight
  ui_mark
  ui_step "open Drawing1 (double-click)"   ui_double_click_logged "Browser Root/Drawings/Drawing1"
  ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
  ui_expect_new "Drawing sheet drawn: " "its sheet drawn"
  settled "Drawing view 1"
  ui_mark
  ui_step "select the view"                ui_click_logged "Drawing view 1"
  ui_expect_new "Drawing view 1 selected" "the view selected"
  ui_expect_new "Drawing view 1 shows $2 of $TOTAL bodies" "its Shows: $2 of the $TOTAL bodies checked"
  # The list folded under its heading at first (mitcad#2498): opened.
  ui_expect_new "Drawing panel drawingViewShowsToggle at" "its Shows heading in the panel"
  settled "Drawing panel drawingViewShowsToggle"
  ui_mark
  ui_step "open its Shows"                 ui_click_logged "Drawing panel drawingViewShowsToggle"
  ui_expect_new "Drawing view 1 Shows opened" "the list opened under its heading"
  ui_expect_new "Drawing panel drawingViewShows row $FIRST_ROW at" "$FIRST_ROW's row in sight"
  settled "Drawing panel drawingViewShows row $FIRST_ROW"
}

# consecutive_edits layout: the scenario above in the window's layout.
consecutive_edits() {
  echo "--- $1: the view's Shows"
  open_view "plates.mitcad" 20

  echo "--- $1: scrolled to the end, Plate:20's body cleared"
  ui_mark
  ui_step "Plate:1's row"                  ui_click_logged "Drawing panel drawingViewShows row Plate:1"
  ui_sync
  ui_step "to the last row (End)"          ui_key End
  ui_sync
  ui_step "unfold Plate:20 (Right)"        ui_key Right
  ui_expect_new "Drawing panel drawingViewShows row Plate:20 at" "the list at its end"
  ui_sync
  ui_step "to its body (Right)"            ui_key Right
  ui_sync
  clear_current 19 "Plate:20's body cleared"

  echo "--- $1: Plate:19's body cleared next"
  settled "Drawing panel drawingViewShows row Plate:19"
  ui_mark
  ui_step "Plate:19's row"                 ui_click_logged "Drawing panel drawingViewShows row Plate:19"
  ui_sync
  ui_step "unfold Plate:19 (Right)"        ui_key Right
  ui_sync
  ui_step "to its body (Right)"            ui_key Right
  ui_sync
  clear_current 18 "Plate:19's body cleared"

  echo "--- $1: one undo step"
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: Edit view 1 of Drawing1" "one undo step"
  view_draws 1 19 "Plate:19's body back"
  ui_expect_new "Drawing view 1 shows 19 of 20 bodies" "its Shows built again with the box checked"
  place_kept "undone"

  grep -q "Drawing command refused" "$UI_LOG" && ui_fail "a drawing command was refused"
  return 0
}

ui_start_display

ui_start_app --open "$FILE" --no-native-dialogs
consecutive_edits "Docked"

ui_stop_app
ui_start_app --chrome=floating --open "$FILE" --no-native-dialogs
consecutive_edits "Floating"

archive=""
for candidate in "${MITCAD_MITCAD_REPORTED:-/nonexistent}"/github-683/*.zip; do
  [ -f "$candidate" ] && [ "$(content_id "$candidate")" = 2afcdc2c ] && { archive=$candidate; break; }
done
if [ -n "$archive" ]; then
  echo "--- The reported design (Docked): 100 bodies under one occurrence"
  mkdir -p "$WORK/reported"
  (cd "$WORK/reported" && cmake -E tar xf "$archive") || ui_fail "the reported archive was not unpacked"
  reported=""
  while IFS= read -r candidate; do
    [ "$(content_id "$candidate")" = e0171a6f ] && { reported=$candidate; break; }
  done < <(find "$WORK/reported" -name '*.mitcad' | sort)
  [ -n "$reported" ] || ui_fail "the reported archive holds no design e0171a6f"
  FIRST_ROW="Component1:1"
  TOTAL=100
  ui_stop_app
  ui_start_app --open "$reported" --no-native-dialogs
  open_view "$(basename "$reported")" 98
  ui_mark
  ui_step "Component1:1's row"             ui_click_logged "Drawing panel drawingViewShows row Component1:1"
  ui_sync
  ui_step "unfold it (Right)"              ui_key Right
  ui_expect_new "Drawing panel drawingViewShows row Body1 at" "its bodies in sight"
  ui_sync
  ui_step "to the last row (End)"          ui_key End
  ui_sync
  clear_current 97 "the reported design: Body100 cleared"
  settled "Drawing panel drawingViewShows row Body99"
  ui_step "Body99's row"                   ui_click_logged "Drawing panel drawingViewShows row Body99"
  ui_sync
  clear_current 96 "the reported design: Body99 cleared next"
  ui_mark
  ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
  ui_expect_new "Undo: Edit view 1 of Drawing1" "one undo step"
  view_draws 1 97 "Body99 back"
  ui_expect_new "Drawing view 1 shows 97 of 100 bodies" "its Shows built again with Body99 checked"
  place_kept "the reported design: undone"
  grep -q "Drawing command refused" "$UI_LOG" && ui_fail "a drawing command was refused"
else
  echo "skip the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-683)"
fi

ui_finish "UI drawing Shows place test"
