#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowBrowserHover.cpp app/browser/BrowserPanel.cpp app/browser/BrowserController.cpp
# check-all sources: tools/cli
# The browser's hover on a large assembly (mitcad#1022): a row under the
# pointer highlights its bodies in the view without meshing or computing
# anything, so that moving the pointer quickly down the rows stays smooth.
# A synthetic design made with mitcad-cli: Assembly:1 in the root holds
# Row:1 .. Row:60, each Row two occurrences of Cell, each Cell five bodies:
# 600 bodies in nested components (Root > Assembly > Row > Cell). With
# MITCAD_MESH_TRACE=1:
#   - the pointer on Assembly:1's row (its arrow opens it) highlights its
#     600 bodies, then goes down 50 rows in a row, Row:1 .. Row:50, each
#     highlighting its 10 bodies; the rows below those in sight scroll
#     into sight with the wheel as it goes;
#   - every hover's logged time ("Browser hover highlight: <row> (<n>
#     bodies), <ms> ms, <k> faces meshed") is under 50 ms, no hover meshes
#     a face, and nothing is meshed meanwhile (no "mesh-trace:
#     presentation" or "Display meshes of" line);
#   - off the rows the highlight goes; the selection stays as it was.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-hover-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-browser-hover.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
export MITCAD_MESH_TRACE=1
LIMIT_MS=50
ROWS=50

# Assembly (C1, Assembly:1) in the root, Row (C2) in it, Cell (C3) in Row
# with Sketch1's rectangle extruded five times as new bodies; Cell:2 beside
# Cell:1 in Row, then Row:2 .. Row:60 beside Row:1 in Assembly.
{
  echo '['
  echo '  {"cmd": "create_component", "name": "Assembly"},'
  echo '  {"cmd": "create_component", "name": "Row"},'
  echo '  {"cmd": "create_component", "name": "Cell"},'
  echo '  {"cmd": "sketch.create"},'
  echo '  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 16, "height": 12},'
  for i in 1 2 3 4 5; do
    echo "  {\"cmd\": \"add_feature\", \"def\": {\"type\": \"extrude\", \"profiles\": [{\"sketch\": \"F1\", \"region\": \"$R\"}],"
    echo "    \"extent\": {\"type\": \"distance\", \"distance\": $((2 * i))}, \"operation\": \"new_body\"}},"
  done
  echo '  {"cmd": "activate_component", "component": "Row"},'
  echo '  {"cmd": "copy_occurrence", "occurrence": "Assembly:1/Row:1/Cell:1", "transform": {"translation": [20, 0, 0]}},'
  echo '  {"cmd": "activate_component", "component": "Assembly"},'
  for i in $(seq 1 59); do
    echo "  {\"cmd\": \"copy_occurrence\", \"occurrence\": \"Assembly:1/Row:1\", \"transform\": {\"translation\": [0, $((16 * i)), 0]}},"
  done
  echo '  {"cmd": "activate_component", "component": "C0"}'
  echo ']'
} > "$WORK/assembly.json"
"$CLI" run "$WORK/assembly.json" --save "$WORK/assembly.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
UI_WINDOW_WAIT=90 ui_start_app --open "$WORK/assembly.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_wait_idle
ui_expect_log "Browser Root/Assembly:1 at" "the browser lists Assembly:1" 60
ui_sync
shown=$(grep "^Bodies shown: " "$UI_LOG" | tail -1 | sed 's/^Bodies shown: //' | tr ',' '\n' | grep -c .)
[ "$shown" -ge 500 ] || ui_fail "only $shown bodies shown, not 600"
echo "ok   $shown bodies shown"
selected_before=$(grep "^Browser selected: " "$UI_LOG" | tail -1)

# expect_hover "row" bodies: the row's hover logged since the mark (up to
# 5 s), polled without the sync key, so that the pointer goes on as soon
# as the app has taken the move.
expect_hover() {
  local i
  for i in $(seq 1 250); do
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Browser hover highlight: $1 ($2 bodies), " && return 0
    sleep 0.02
  done
  ui_fail "no hover highlight of $1 with $2 bodies since the mark"
}

echo "--- Down $ROWS rows in a row"
ui_mark
# A tree longer than the browser opens folded below the root (mitcad#426).
if ! ui_place_valid "Browser Root/Assembly:1/Row:1"; then
  ui_step "open Assembly:1 (its arrow)"  ui_click_logged "Browser arrow Root/Assembly:1"
  ui_expect_new "Browser Root/Assembly:1/Row:1 at" "its rows"
fi
ui_step "point at Assembly:1's row"      xdotool mousemove $(ui_logged_at "Browser Root/Assembly:1")
expect_hover "Root/Assembly:1" 600
echo "ok   Assembly:1's row highlights its 600 bodies"
for i in $(seq 1 "$ROWS"); do
  # A row below those in sight scrolls into sight first (ui_logged_at).
  xdotool mousemove $(ui_logged_at "Browser Root/Assembly:1/Row:$i")
  expect_hover "Root/Assembly:1/Row:$i" 10
done
echo "ok   Row:1 .. Row:$ROWS each highlighted their 10 bodies"
ui_step "point at the view"              xdotool mousemove $(ui_view_at 3 50)
ui_expect_new "Browser hover highlight: none (0 bodies), " "the highlight goes with the pointer"
ui_sync

hovers=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "^Browser hover highlight: ")
rows=$(grep -cE "^Browser hover highlight: Root/Assembly:1/Row:[0-9]+ \(10 bodies\), " <<< "$hovers")
[ "$rows" -ge "$ROWS" ] || ui_fail "$rows rows hovered, not $ROWS"
# Each line: "Browser hover highlight: <row> (<n> bodies), <ms> ms, <k> faces meshed".
slow=$(awk -F', ' -v limit="$LIMIT_MS" '{ ms = $2; sub(/ ms$/, "", ms); if (ms + 0 >= limit) print }' <<< "$hovers")
[ -z "$slow" ] || ui_fail "hovers of $LIMIT_MS ms or more: $slow"
stats=$(awk -F', ' '{ ms = $2; sub(/ ms$/, "", ms); n++; sum += ms; if (ms + 0 > max) max = ms + 0 }
  END { printf "%d hovers, mean %.2f ms, longest %.2f ms", n, sum / n, max }' <<< "$hovers")
echo "ok   every hover under $LIMIT_MS ms ($stats)"
grep -E ", [1-9][0-9]* faces meshed$" <<< "$hovers" && ui_fail "a hover meshed faces"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^mesh-trace: presentation |^Display meshes of " | head -5 |
  grep . && ui_fail "something was meshed during the hovers"
echo "ok   nothing meshed during the hovers"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "^(Selected: |View body highlights: |Visibility )" &&
  ui_fail "the hovers changed the selection or the visibility"
[ "$(grep "^Browser selected: " "$UI_LOG" | tail -1)" = "$selected_before" ] ||
  ui_fail "the selection changed with the hovers"
echo "ok   the selection and the visibility unchanged"
ui_crashed && ui_fail "the application crashed"

ui_finish "UI browser hover test"
