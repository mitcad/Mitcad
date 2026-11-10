#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# One symbol per coincident constraint at a point (mitcad#1578): a point
# with two coincidences showed one symbol, and the second constraint could
# not be seen, selected or deleted in the view. On a design mitcad-cli
# makes, the constraints as an import leaves them (point-to-point
# coincidences between curve ends, not merged points):
#   Sketch1 (F1) on XY: a line c3 from (0, 0) to p2 at (20, 10), a line c6
#   from p4 at (20, 10) to (40, 0), and a vertical line c9 at x = 20;
#   k10 holds p2 on p4 (the other line's end), k11 holds p2 on the
#   vertical line, k12 makes c9 vertical.
#   Sketch2 (F2) on XY, the same 60 mm to the right, but k21 holds the
#   other line's end p14 on the vertical line: the two symbols belong to
#   two points at one place.
# In each sketch:
#   1. Both coincidences have a symbol at the point (logged "Constraint k10
#      glyph at x,y (coincident by p2)"), side by side, not over each other.
#   2. A click on each symbol selects its own constraint.
#   3. Delete (the symbol's context menu) removes that one; the other's
#      symbol stays.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-coincident-glyphs-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-coincident-glyphs.XXXXXX)
FILE=$WORK/coincident.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p1", "type": "point", "at": [0, 0]},
      {"id": "p2", "type": "point", "at": [20, 10]},
      {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
      {"id": "p4", "type": "point", "at": [20, 10]},
      {"id": "p5", "type": "point", "at": [40, 0]},
      {"id": "c6", "type": "line", "start": "p4", "end": "p5"},
      {"id": "p7", "type": "point", "at": [20, -10]},
      {"id": "p8", "type": "point", "at": [20, 30]},
      {"id": "c9", "type": "line", "start": "p7", "end": "p8"}],
    "constraints": [
      {"id": "k10", "type": "coincident", "point": "p2", "entity": "p4"},
      {"id": "k11", "type": "coincident", "point": "p2", "entity": "c9"},
      {"id": "k12", "type": "vertical", "line": "c9"}]}},
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy",
    "entities": [
      {"id": "p11", "type": "point", "at": [60, 0]},
      {"id": "p12", "type": "point", "at": [80, 10]},
      {"id": "c13", "type": "line", "start": "p11", "end": "p12"},
      {"id": "p14", "type": "point", "at": [80, 10]},
      {"id": "p15", "type": "point", "at": [100, 0]},
      {"id": "c16", "type": "line", "start": "p14", "end": "p15"},
      {"id": "p17", "type": "point", "at": [80, -10]},
      {"id": "p18", "type": "point", "at": [80, 30]},
      {"id": "c19", "type": "line", "start": "p17", "end": "p18"}],
    "constraints": [
      {"id": "k20", "type": "coincident", "point": "p12", "entity": "p14"},
      {"id": "k21", "type": "coincident", "point": "p14", "entity": "c19"},
      {"id": "k22", "type": "vertical", "line": "c19"}]}},
  {"expect": {"query": {"query": "sketch", "uid": "F1"}, "result": {
    "constraints": [{"id": "k10", "type": "coincident", "point": "p2", "entity": "p4"},
                    {"id": "k11", "type": "coincident", "point": "p2", "entity": "c9"},
                    {"id": "k12", "type": "vertical", "line": "c9"}]}}},
  {"expect": {"query": {"query": "sketch", "uid": "F2"}, "result": {
    "constraints": [{"id": "k20", "type": "coincident", "point": "p12", "entity": "p14"},
                    {"id": "k21", "type": "coincident", "point": "p14", "entity": "c19"},
                    {"id": "k22", "type": "vertical", "line": "c19"}]}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# glyph_line "k": the last logged place of a constraint's symbol.
glyph_line() { grep -F "Constraint $1 glyph at " "$UI_LOG" | tail -1; }

# expect_glyph "k" "point": its symbol is placed, a coincidence drawn by
# that point.
expect_glyph() {
  ui_expect_log "Constraint $1 glyph at" "$1's symbol is placed"
  local line
  line=$(glyph_line "$1")
  [[ $line == *" (coincident by $2)" ]] || ui_fail "$1's symbol is not a coincidence by $2: $line"
  echo "ok   $1's symbol is a coincidence by $2"
}

# side_by_side "k" "k": the two symbols lie apart in one row, as far as a
# symbol is wide at least (15 px), not over each other.
side_by_side() {
  local ax ay bx by
  read -r ax ay <<< "$(ui_logged_at "Constraint $1 glyph")"
  read -r bx by <<< "$(ui_logged_at "Constraint $2 glyph")"
  local dx=$((ax > bx ? ax - bx : bx - ax)) dy=$((ay > by ? ay - by : by - ay))
  [ "$dx" -ge 15 ] || [ "$dy" -ge 15 ] ||
    ui_fail "$1 and $2 are drawn over each other ($ax,$ay and $bx,$by)"
  echo "ok   $1 at $ax,$ay and $2 at $bx,$by lie side by side"
}

# select_constraint "k" "sketch": a click on its symbol selects it alone.
select_constraint() {
  ui_mark
  ui_step "select $1 (its symbol)"        ui_click_logged "Constraint $1 glyph"
  ui_expect_new "Selected: 1 constraint [" "one constraint selected"
  local line
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Selected: " | tail -1)
  [[ $line == *" $1 of $2"* ]] || ui_fail "a click on $1's symbol selected another: $line"
  echo "ok   a click on $1's symbol selects $1"
}

# delete_constraint "k" "other": Delete in the context menu of its symbol
# (selected last) removes it alone; the other's symbol is placed again
# after it, and its own is not.
delete_constraint() {
  ui_mark
  ui_step "right-click $1's symbol"       ui_click_logged "Constraint $1 glyph" 3
  ui_expect_new "Context menu: " "the context menu opened"
  ui_step "Delete"                        ui_menu_choose "Delete"
  ui_expect_new "Deleted $1" "$1 deleted"
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qxF "Deleted $1" ||
    ui_fail "more than $1 deleted: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Deleted " | tail -1)"
  ui_expect_new "Constraint $2 glyph at" "$2's symbol placed again"
  local after
  after=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | awk -v gone="Deleted $1" '$0 == gone { n = 0; next } { a[n++] = $0 }
    END { for (i = 0; i < n; i++) print a[i] }')
  grep -qF "Constraint $1 glyph at " <<< "$after" && ui_fail "$1's symbol is still placed"
  echo "ok   $1 deleted from the view, $2 stays"
}

# edit_sketch "name" "uid": a double-click on it in the timeline.
edit_sketch() {
  ui_expect_log "Timeline $1 at" "$1 in the timeline"
  ui_mark
  ui_step "edit $1 (double-click)"        ui_double_click_logged "Timeline $1"
  ui_expect_new "Editing sketch $2 ($1)" "$1 edited"
  ui_step "fit (F6)"                      ui_key F6
  ui_sync
}

ui_start_display
ui_start_app --open "$FILE"

echo "--- 1. Two coincidences at one point"
edit_sketch Sketch1 F1
expect_glyph k10 p2
expect_glyph k11 p2
[ -n "$SHOT" ] && ui_capture "$SHOT"
side_by_side k10 k11
select_constraint k10 F1
select_constraint k11 F1
delete_constraint k11 k10
ui_mark
ui_step "finish sketch (Ctrl+Enter)"      ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch1 finished"

echo "--- 2. Two points at one place, a coincidence by each"
edit_sketch Sketch2 F2
expect_glyph k20 p12
expect_glyph k21 p14
side_by_side k20 k21
select_constraint k21 F2
select_constraint k20 F2
delete_constraint k20 k21
ui_mark
ui_step "finish sketch (Ctrl+Enter)"      ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

ui_finish "UI sketch coincident glyphs test"
