#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework core/model-api/src tools/cli
# check-all sources: app/MainWindowMeasureHistory.cpp
# check-all sources: app/MainWindowMeasurements.cpp app/MainWindowSelection.cpp app/PickContext.cpp app/PickContext.hpp
# Measure through the real UI, in the Docked layout, on two cylinders of
# radius 5, 20 high (Body1 about Z through the origin, Body2 through
# (30, 40)) and a block 10 x 10 x 20 at (20, -30) (Body3). Axes
# (mitcad#647):
#   - A round face: its axis through its centre in the panel, drawn as a
#     line ("axis lines 1").
#   - Two round faces: parallel, the surfaces 40 apart, the axes 50.
#   - A round face against the block's top: square to it, no axis
#     distance.
#   - Two top rims: the axes' and the centres' distance.
# The panel's layout (mitcad#1010), in the Docked and the Floating layout
# at the default width: top to bottom the picks, the result, Keep as
# Dimension and Annotate on one row, the history, Close; no control's text
# clipped, every result row on one line; the pair's values first (the
# distance), then a group per pick titled by its kind; a value copied from
# its context menu; Close and Esc close it.
# The history (mitcad#648): folded, unfolded from its header; the three
# measurements as three rows, newest first, in the next panel; the first
# shown again (its picks back, its lines drawn, no new row); one row copied
# from its context menu, all rows and Clear from the ⋯ menu. In the
# Floating layout, after the design is opened again: no rows, then one
# measurement's row shown again in the next panel.
# Tools/ui-windows-measure-test.ps1 does the same in the Windows VM.
# The picks' preference (mitcad#869), on a block extruded to both sides of
# its shown sketch, from the top: the two corners where the sketch's points
# lie give the body's vertices and Keep as Dimension is enabled; with
# Sketch Curves and Points the two sketch points, and Keep as Dimension
# greyed out with the reason.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-measure-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-measure.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/axes.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 40], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [20, -30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/axes.json" --save "$WORK/axes.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# logged_y "text": the y of the last "<text> at x,y".
logged_y() { grep -F "$1 at " "$UI_LOG" | tail -1 | sed -n 's/.* at [0-9]*,\([0-9]*\).*/\1/p'; }

# expect_layout: the panel in the order of the task (mitcad#1010), top to
# bottom the picks, the result, the actions, the history and Close; at
# most the default 300 wide; no control's text clipped; every result row
# on one line.
expect_layout() {
  ui_sync
  local pick result keep history close width
  pick=$(logged_y "Panel Measure input b")
  result=$(logged_y "Panel Measure result Distance")
  keep=$(logged_y "Measure Keep as Dimension")
  history=$(logged_y "Measure history")
  close=$(logged_y "Panel Measure Close")
  [ -n "$pick" ] && [ -n "$result" ] && [ -n "$keep" ] && [ -n "$history" ] && [ -n "$close" ] ||
    ui_fail "a place is not logged: picks $pick, result $result, actions $keep, history $history, Close $close"
  [ "$pick" -lt "$result" ] && [ "$result" -lt "$keep" ] && [ "$keep" -lt "$history" ] ||
    ui_fail "not in the task's order: picks $pick, result $result, actions $keep, history $history"
  # Close at the bottom, under the page that scrolls (mitcad#655): below
  # the panel's view when it is logged, else below the history.
  local bottom=$history view
  view=$(grep "^Panel Measure view y " "$UI_LOG" | tail -1 | sed -n 's/^Panel Measure view y -\{0,1\}[0-9]* to \(-\{0,1\}[0-9]*\),.*/\1/p')
  [ -n "$view" ] && bottom=$view
  [ "$close" -gt "$bottom" ] || ui_fail "Close is not at the bottom: Close $close, the panel's view or the history ending at $bottom"
  echo "ok   top to bottom: picks $pick, result $result, actions $keep, history $history, Close $close (under $bottom)"
  [ "$(logged_y "Measure Annotate")" = "$keep" ] || ui_fail "Annotate is not on Keep as Dimension's row"
  echo "ok   Keep as Dimension and Annotate on one row"
  width=$(grep -o "Panel Measure width [0-9]* of" "$UI_LOG" | tail -1 | sed 's/[^0-9]//g')
  [ -n "$width" ] && [ "$width" -le 300 ] || ui_fail "the panel is '$width' wide, not within the default 300"
  grep -F "Measure controls" "$UI_LOG" | tail -1 | grep -qF "Measure controls: none clipped" ||
    ui_fail "a control's text is clipped at $width: $(grep -F "Measure controls" "$UI_LOG" | tail -1)"
  grep -F "Panel Measure inputs" "$UI_LOG" | tail -1 | grep -qF "Panel Measure inputs: none clipped" ||
    ui_fail "a pick's label or the filter's text is clipped at $width: $(grep -F "Panel Measure inputs" "$UI_LOG" | tail -1)"
  echo "ok   no control's text clipped at $width, the picks' labels and the filter's text whole"
  grep -F "Panel Measure result: " "$UI_LOG" | tail -1 | grep -q "rows, one line each$" ||
    ui_fail "a result row does not fit its line: $(grep -F "Panel Measure result: " "$UI_LOG" | tail -1)"
  echo "ok   $(grep -F "Panel Measure result: " "$UI_LOG" | tail -1 | sed 's/.*: //') at $width"
}

ui_start_display
ui_start_app --open "$WORK/axes.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- A round face's axis"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_step "pick Body1's round face"          ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_new "Inspect Measure: area 628.319, radius 5, diameter 10, axis (0, 0, 10) along (0, 0, " \
  "its axis through its centre"
ui_expect_new "axis lines 1" "the axis drawn through it"

echo "--- Parallel axes"
ui_mark
ui_step "pick Body2's round face"          ui_click_pick "face F4.b0/F4:side(c1)"
ui_expect_new "axis (30, 40, 10) along (0, 0, " "the second axis"
ui_expect_new "distance 40, angle 0 deg, axis distance 50, axis lines 2" \
  "the surfaces 40 apart, the axes parallel and 50 apart, both drawn"

echo "--- The panel's layout (mitcad#1010)"
ui_expect_new "Panel Measure result values: Distance = 40 mm, Angle = 0°, Axis distance = 50 mm" \
  "the pair's values first, the distance first"
ui_expect_new "Panel Measure result Selection 1: cylindrical face: Diameter = 10 mm, Radius = 5 mm, Area = 628.319 mm²" \
  "then the first pick's group, titled by its kind"
ui_expect_new ", Axis through = (0, 0, 10), Axis direction = (0, 0, " "its axis as two rows"
ui_expect_new "Panel Measure result Selection 2: cylindrical face: Diameter = 10 mm" "and the second pick's"
ui_expect_new "Panel Measure result 2 Axis direction at" "its rows laid out"
ui_expect_new "Measure history at" "the history under the actions"
grep -F "Measure history at " "$UI_LOG" | tail -1 | grep -q "(folded)$" || ui_fail "the history is not folded"
echo "ok   the history folded"
expect_layout
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "right-click the distance"         ui_click_logged "Panel Measure result Distance" 3
ui_expect_new "Context menu: Copy" "its context menu"
ui_step "Copy"                             ui_menu_choose "Copy"
ui_expect_new "Panel Measure result copied Distance: 40 mm" "its number and unit copied"
ui_mark
ui_step "Close"                            ui_click_logged "Panel Measure Close"
ui_expect_new "Panel Measure Close clicked" "Close clicked"
ui_expect_new "Closed Measure" "measure closed"

echo "--- A round face against a plane"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_step "pick Body1's round face"          ui_click_pick "face F2.b0/F2:side(c1)"
ui_expect_new "axis lines 1" "its axis"
ui_step "pick the block's top"             ui_click_pick "face F6.b0/F6:end($R)"
ui_expect_new "angle 90 deg, axis lines 1" "square to the top, no axis distance"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

echo "--- Two rims"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_step "pick Body1's top rim"             ui_click_pick "edge F2.b0/E{F2:end(r{c1})|"
ui_expect_new "Inspect Measure: length 31.416, radius 5, diameter 10, axis (0, 0, 20) along" "the rim's axis"
ui_step "pick Body2's top rim"             ui_click_pick "edge F4.b0/E{F4:end(r{c1})|"
ui_expect_new "axis distance 50, center distance 50, axis lines 2" "the axes and the centres 50 apart"
ui_expect_new "Panel Measure result Selection 1: circular edge: Diameter = 10 mm" "a rim's group"
ui_step "close (Esc)"                      ui_key Escape
ui_expect_new "Command Measure cancelled" "Esc closes it"

echo "--- The history (mitcad#648)"
# The three measurements above, each a row (a pick's own row became the
# pair's when the second came), newest first, kept across the panels.
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Measure history: 3 rows" "three rows"
ui_expect_new "Measure history row 3: Edge of Body1" "the rims' row first"
ui_expect_new "Edge of Body2: Radius 5 mm, Diameter 10 mm, Radius 5 mm, Diameter 10 mm, Distance 40 mm, Angle 0" \
  "with their values"
ui_expect_new ", Axis distance 50 mm, Centre distance 50 mm" "and their axes' and centres' distances"
ui_expect_new "Measure history row 2: Face of Body1" "the round face against the top"
ui_expect_new "Face of Body3: Radius 5 mm, Diameter 10 mm, Distance" "with its distance"
ui_expect_new ", Angle 90" "and its angle"
ui_expect_new "Measure history row 1: Face of Body1" "the parallel faces last"
ui_expect_new "Face of Body2: Radius 5 mm, Diameter 10 mm, Radius 5 mm, Diameter 10 mm, Distance 40 mm, Angle 0" \
  "with their values"
grep "Measure history row 1: " "$UI_LOG" | tail -1 | grep -q ", Axis distance 50 mm" ||
  ui_fail "row 1 has no axis distance"
ui_expect_new "Measure history row 1 out of sight" "the history folded as the panel opens"
ui_expect_new "Measure history at" "its header laid out"
ui_mark
ui_step "unfold the history"               ui_click_logged "Measure history"
ui_expect_new "Measure history opened" "unfolded"
ui_expect_new "Measure history row 1 at" "its rows laid out"
# The rows' menus while the panel has no result yet, as the Windows test
# does in its low window.
ui_mark
ui_step "right-click the first row"        ui_click_logged "Measure history row 1" 3
ui_expect_new "Measure history row 1 menu" "its context menu"
ui_expect_new "Context menu: Show | Keep as Dimension | Copy" "Show, Keep as Dimension and Copy"
ui_step "Copy"                             ui_menu_choose "Copy"
ui_expect_new "Measure history copied row 1: 1 | Face of Body1" "the row copied as tab-separated text"
ui_mark
ui_step "the history's ⋯ menu"             ui_click_logged "Measure history menu"
ui_expect_new "Context menu: Copy All | Clear" "Copy All and Clear"
ui_step "Copy All"                         ui_menu_choose "Copy All"
ui_expect_new "Measure history copied 3 rows: 3 | Edge of Body1" "every row, newest first"
ui_expect_new " / 1 | Face of Body1" "down to the first"
ui_mark
ui_step "show the first again"             ui_click_logged "Measure history row 1"
ui_expect_new "Measure history showed row 1" "shown again"
ui_expect_new "Measure Selection 2: 1 face" "its picks back in the inputs"
ui_expect_new "distance 40, angle 0 deg, axis distance 50, axis lines 2" "measured again, its lines drawn"
grep -q "Measure history: 4 rows" "$UI_LOG" && ui_fail "showing a row again added one"
expect_layout
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-history.png"
# The picks' groups folded: the history comes up again.
ui_mark
ui_step "fold the first pick's group"      ui_click_logged "Panel Measure result group 1"
ui_expect_new "Panel Measure result group 1 folded" "folded"
ui_expect_new "Panel Measure result 1 Diameter out of sight" "its rows hidden"
ui_sync
ui_step "fold the second pick's group"     ui_click_logged "Panel Measure result group 2"
ui_expect_new "Panel Measure result group 2 folded" "folded"
ui_sync
ui_mark
ui_step "the history's ⋯ menu"             ui_click_logged "Measure history menu"
ui_expect_new "Context menu: Copy All | Clear" "its menu again"
ui_step "Clear"                            ui_menu_choose "Clear"
ui_expect_new "Measure history Clear clicked" "Clear chosen"
ui_expect_new "Measure history cleared" "cleared"
ui_expect_new "Measure history: 0 rows" "no rows"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

echo "--- The history in the Floating layout"
ui_stop_app
ui_start_app --open "$WORK/axes.mitcad" --chrome=floating
ui_step "fit (F6)"                         ui_key F6
# The Floating layout's command card (x from about 990) covers Body2 after
# the fit: zoomed out about Body1, a wheel notch at a time, until Body2
# lies left of the card (as the Windows test does, mitcad#648).
read -r b1x b1y <<< "$(ui_logged_at 'Body F2.b0')"
for _ in $(seq 1 12); do
  read -r b2x _ <<< "$(ui_logged_at 'Body F4.b0')"
  [ $((b2x - X)) -lt $((960 * UI_SCALE)) ] && break
  xdotool mousemove "$b1x" "$b1y" click 5
  ui_sync
  sleep 0.3
done
read -r b2x b2y <<< "$(ui_logged_at 'Body F4.b0')"
[ $((b2x - X)) -lt $((960 * UI_SCALE)) ] || ui_fail "Body2 stays under the command card at $b2x,$b2y"
echo "ok   Body2 left of the command card"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Measure history: 0 rows" "a design opened: no rows"
ui_step "pick Body1's round face"          ui_click_pick "face F2.b0/F2:side(c1)"
ui_step "pick Body2's round face"          ui_click_pick "face F4.b0/F4:side(c1)"
ui_expect_new "axis distance 50, axis lines 2" "measured"
ui_expect_new "Panel Measure result Distance at" "the result laid out"
expect_layout
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Measure history: 1 rows" "one row"
ui_expect_new "Measure history row 1: Face of Body1" "the parallel faces"
ui_expect_new "Measure history at" "its header laid out"
ui_step "unfold the history"               ui_click_logged "Measure history"
ui_expect_new "Measure history opened" "unfolded"
ui_expect_new "Measure history row 1 at" "its row laid out"
ui_step "show it again"                    ui_click_logged "Measure history row 1"
ui_expect_new "Measure history showed row 1" "shown again"
ui_expect_new "distance 40, angle 0 deg, axis distance 50, axis lines 2" "measured again"
expect_layout
ui_mark
ui_step "Close"                            ui_click_logged "Panel Measure Close"
ui_expect_new "Closed Measure" "measure closed"

echo "--- A shown sketch in the body's middle plane (mitcad#869)"
# A block 120 x 70 extruded 15 to each side of its sketch, which is shown:
# seen from the top, the sketch's corner points lie on the block's top
# corners and its lines on the top edges. Faces, Edges and Vertices takes
# the body's vertex there; Sketch Curves and Points the sketch's points,
# which Keep as Dimension cannot keep: its tooltip says why.
cat > "$WORK/middle.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-60, -35], "width": 120, "height": 70},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "symmetric", "distance": 15, "full_length": false}, "operation": "new_body"}},
  {"cmd": "set_feature_visible", "uid": "F1", "visible": true}
]
EOF
"$CLI" run "$WORK/middle.json" --save "$WORK/middle.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_stop_app
ui_start_app --open "$WORK/middle.mitcad"
ui_step "top view (search)"                ui_command "Top View"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_expect_new "Sketch entity F1/p5 at" "the sketch is shown, its corner points pickable"
ui_expect_new "Pick vertex F2.b0/" "the block's top corners have pick places over the sketch's points"
ui_sync
# The two top corners at the ends of the sketch's first line (c1, from
# (-60, -35) to (60, -35)), where its points p5 and p6 lie in the view.
start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
mapfile -t corners < <(tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick vertex F2.b0/" |
  grep -F "side(c1[c4,c2])" | sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
[ "${#corners[@]}" -eq 2 ] || ui_fail "the first line's two top corners: ${#corners[@]} pick places"
read -r c1x c1y <<< "${corners[0]}"
read -r c2x c2y <<< "${corners[1]}"
read -r s5x s5y <<< "$(ui_logged_at "Sketch entity F1/p5")"
read -r s6x s6y <<< "$(ui_logged_at "Sketch entity F1/p6")"
# Each corner within a sketch point's pick width (8 pixels) of p5 or p6.
for corner in "$c1x $c1y" "$c2x $c2y"; do
  read -r cx cy <<< "$corner"
  cx=$((X + cx * UI_SCALE))
  cy=$((Y + cy * UI_SCALE))
  near=0
  for point in "$s5x $s5y" "$s6x $s6y"; do
    read -r px py <<< "$point"
    [ $(((cx - px) * (cx - px) + (cy - py) * (cy - py))) -le $((64 * UI_SCALE * UI_SCALE)) ] && near=1
  done
  [ "$near" = 1 ] || ui_fail "the corner at $cx,$cy is not under a sketch point"
done
echo "ok   the corners lie under the sketch's points"
ui_mark
ui_step "pick a top corner"                xdotool mousemove $((X + c1x * UI_SCALE)) $((Y + c1y * UI_SCALE)) click 1
ui_expect_new "Measure Selection 1: 1 vertex" "the body's vertex, not the sketch point on it"
ui_step "pick the other top corner"        xdotool mousemove $((X + c2x * UI_SCALE)) $((Y + c2y * UI_SCALE)) click 1
ui_expect_new "Measure Selection 2: 1 vertex" "the other body vertex"
ui_expect_new "distance 120" "the corners 120 apart"
ui_expect_new "Measure Keep as Dimension enabled" "Keep as Dimension enabled for the body's corners"
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Measure Selection [12]: 1 sketch point" && ui_fail "a sketch point was picked"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "Measure Keep as Dimension (enabled|disabled)" | tail -1 |
  grep -q "Measure Keep as Dimension enabled" || ui_fail "Keep as Dimension is not enabled with both corners picked"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "measure started"
ui_step "Selection Filter: Sketch Curves and Points" ui_choose "Panel Measure input filter" 2
ui_expect_new "Measure: Selection Filter = Sketch Curves and Points" "sketch geometry asked for"
ui_step "pick the sketch's corner p5"      ui_click_logged "Sketch entity F1/p5"
ui_expect_new "Measure Selection 1: 1 sketch point" "the sketch point"
ui_step "pick its corner p6"               ui_click_logged "Sketch entity F1/p6"
ui_expect_new "Measure Selection 2: 1 sketch point" "the other sketch point"
ui_expect_new "distance 120" "the points 120 apart"
ui_expect_new "Measure Keep as Dimension disabled: Sketch geometry cannot be kept as an annotation" \
  "Keep as Dimension greyed out, its tooltip gives the reason"
ui_expect_new "Panel Measure result Selection 1: sketch point: Position = (" "the sketch points' groups"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "measure closed"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI measure test"
