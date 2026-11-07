#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/browser
# check-all sources: core/model/src/features/intersection_curve.rs
# check-all sources: geometry/src/intersection_curve.cpp
# Intersection Curve (mitcad#1405) through the real UI:
#   - the curve of a half circle of radius 20 on XZ and a line on XY at
#     y = 10, picked as the first and the second curves: the half circle
#     lifted to y = 10, drawn in the view and listed under Construction in
#     the Browser, its light bulb hiding and showing it;
#   - a 4 mm circle on YZ swept along it, the path picked on the timeline:
#     a half ring, pi 2^2 times the half circle's 20 pi (Pappus);
#   - the file keeps the sweep's path as the curve's feature; the sweep and
#     the curve open again with their picks.
#   - Project and Intersect take the curve (mitcad#1417): in Sketch4 on XY
#     Project takes IntersectionCurve1 on its Browser row and Sketch1
#     clicked on the timeline as a whole sketch (a timeline click on a
#     sketch picks the sketch as before), the curve's ends fixed points at
#     (20, 10) and (-20, 10); in Sketch5 on YZ Intersect takes the curve on
#     the timeline and Sketch1 on its Browser row, fixed points where the
#     plane crosses each: the curve's top (0, 10, -20) at (10, -20), the
#     arc's (0, 0, -20) at (0, -20) (Create Sketch stands a sketch on YZ
#     upright: its x along +Y, its y along +Z, unlike Sketch3's).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-intersection-curve-test.sh [screenshot.png]
# check-all sources: tools/cli
# check-all sources: app/commands/SketchCommands.cpp app/framework/SelectionScope.cpp app/framework/Selection.cpp
# check-all sources: app/framework/CommandSession.cpp core/model/src/features/sketch.rs
# check-all sources: core/model/src/document/intersection_curves.rs app/sketch

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-intersection-curve.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
PI=3.14159265358979
# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

# Sketch1 (F1) on XZ: the arc c1 about the origin from (20, 0) round to
# (-20, 0), the half circle below XY (the sketch's y is -Z); Sketch2 (F2)
# on XY: the line c1 from (-30, 10) to (30, 10); Sketch3 (F3) on YZ: a 4 mm
# circle at (20, 10), the curve's top (0, 10, -20) (the sketch's x is -Z).
cat > "$WORK/curve.json" << EOF
[
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.arc", "sketch": "F1", "mode": "center", "center": [0, 0], "start": [20, 0],
   "end": [-20, 0]},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [-30, 10], "end": [30, 10]},
  {"cmd": "sketch.create", "plane": "yz"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 10], "diameter": 4}
]
EOF
"$CLI" run "$WORK/curve.json" --save "$WORK/curve.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/curve.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Intersection Curve of the arc and the line"
ui_mark
ui_step "intersection curve (search)"      ui_command "Intersection Curve"
ui_expect_new "Command Intersection Curve started" "the command started"
ui_step "the first curves input"           ui_click_logged "Panel Intersection Curve input first"
ui_step "pick the arc"                     ui_click_logged "Sketch entity F1/c1"
ui_expect_new "Intersection Curve First Curves: 1 sketch curve [sketch curve c1 of F1]" "the arc picked"
ui_step "the second curves input"          ui_click_logged "Panel Intersection Curve input second"
ui_step "pick the line"                    ui_click_logged "Sketch entity F2/c1"
ui_expect_new "Intersection Curve Second Curves: 1 sketch curve [sketch curve c1 of F2]" "the line picked"
ui_expect_new "Preview Intersection Curve: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added intersection curve of 1 and 1 curve(s)" "the curve added"
ui_expect_new "Construction shown: IntersectionCurve1" "the curve drawn in the view"

echo "--- Its light bulb in the Browser's Construction folder"
ui_mark
ui_step "hide IntersectionCurve1"          ui_click_logged "Browser eye Root/Construction/IntersectionCurve1"
ui_expect_new "Visibility Root/Construction/IntersectionCurve1: hidden" "hidden"
ui_expect_new "Construction shown: none" "no longer drawn"
ui_mark
ui_step "show IntersectionCurve1"          ui_click_logged "Browser eye Root/Construction/IntersectionCurve1"
ui_expect_new "Visibility Root/Construction/IntersectionCurve1: shown" "shown"
ui_expect_new "Construction shown: IntersectionCurve1" "drawn again"

echo "--- Sweep: the circle along the curve, picked on the timeline"
ui_mark
ui_step "sweep (search)"                   ui_command "Sweep"
ui_expect_new "Sweep Profiles: 1 profile [profile r{c1} of F3]" "the newest profile, the circle"
ui_step "the path input"                   ui_click_logged "Panel Sweep input path"
ui_step "pick IntersectionCurve1"          ui_click_logged "Timeline IntersectionCurve1"
ui_expect_new "Sweep Path: 1 feature [feature F4]" "the curve picked as the path"
ui_expect_new "Preview Sweep: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added sweep (New Body) along 1 curve(s)" "the sweep added"
ui_expect_volume "New body Body1 (F5.b0): volume \([0-9.]*\) mm3" \
  "$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 4 * 20 * pi }')" "a half ring along the curve" 1e-3
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $WORK/curve.mitcad" "saved"
expect_file "$WORK/curve.mitcad" \
  '[(f["type"], f.get("path")) for f in doc["features"] if f["type"] in ("intersection_curve", "sweep")]' \
  "[('intersection_curve', None), ('sweep', {'feature': 'F4'})]" "the sweep's path is the curve"
expect_file "$WORK/curve.mitcad" \
  '[(f["first"]["sketch"], f["second"]["sketch"]) for f in doc["features"] if f["type"] == "intersection_curve"]' \
  "[('F1', 'F2')]" "the curve of Sketch1 and Sketch2"

echo "--- Both open again with their picks"
ui_mark
ui_step "double-click Sweep1"              ui_double_click_logged "Timeline Sweep1"
ui_expect_new "Editing F5 with Sweep" "editing Sweep1"
ui_expect_new "Sweep Path: 1 feature [feature F4]" "its path is the curve"
ui_expect_new "Preview Sweep: ok" "previewed as it is"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Sweep cancelled" "nothing changed"
ui_mark
ui_step "double-click IntersectionCurve1"  ui_double_click_logged "Timeline IntersectionCurve1"
ui_expect_new "Editing F4 with Intersection Curve" "editing IntersectionCurve1"
ui_expect_new "Intersection Curve First Curves: 1 sketch curve [sketch curve c1 of F1]" "the arc"
ui_expect_new "Intersection Curve Second Curves: 1 sketch curve [sketch curve c1 of F2]" "the line"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Intersection Curve cancelled" "nothing changed"

# fixed_at x y what: a drag of the edited sketch at (x, y) finds a fixed
# point there (it does not move, or it is put back where it was).
fixed_at() {
  ui_mark
  ui_step "drag at ($1, $2)"               ui_sketch_drag "$1" "$2" $(($1 + 3)) $(($2 - 6))
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    grep -qE "Drag of p[0-9]+ did not move it|Dragged p[0-9]+ to \($1\.000, $2\.000\)" ||
    ui_fail "no fixed point at ($1, $2): $3"
  echo "ok   a fixed point at ($1, $2): $3"
}

echo "--- Project: the curve and its ends into a later sketch on XY (mitcad#1417)"
ui_step "create sketch on XY"              ui_create_sketch xy
ui_step "fit (F6)"                         ui_key F6
ui_sync
ui_mark
ui_step "project (P)"                      ui_key p
ui_expect_new "Command Project started" "Project started"
ui_expect_new "other-sketch-geometry=cross" "Project's filter has the row for other sketches' and intersection curves"
ui_expect_log "Browser Root/Construction/IntersectionCurve1 at" "the curve's row in the Browser"
ui_step "pick IntersectionCurve1's row"    ui_click_logged "Browser Root/Construction/IntersectionCurve1"
ui_expect_new "Project Geometry: 1 feature [feature F4]" "the curve went to Project"
ui_step "pick Sketch1 on the timeline"     ui_click_logged "Timeline Sketch1"
ui_expect_new "Project Geometry: 1 sketch, 1 feature [feature F4; sketch F1]" \
  "a sketch clicked on the timeline goes in whole, as before"
ui_expect_new "Preview Project: ok" "the projection previewed"
ui_wait_idle
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Projected 2 item(s): intersection curve F4, sketch F1" "the log names the curve and the sketch"
ui_step "fit (F6)"                         ui_key F6
ui_sync
fixed_at 20 10 "the curve's end"
fixed_at -20 10 "the curve's other end"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch4 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Intersect: where a later sketch on YZ crosses the curve (mitcad#1417)"
# Sketch5 (F7) on YZ, made upright by Create Sketch (mitcad#692): x along
# +Y, y along +Z (Sketch3 from the CLI has x along -Z, y along +Y).
ui_step "create sketch on YZ"              ui_create_sketch yz
ui_step "fit (F6)"                         ui_key F6
ui_sync
ui_mark
ui_step "intersect (search)"               ui_command "Intersect"
ui_expect_new "Command search chose sketch.intersect" "Intersect started"
ui_step "pick IntersectionCurve1 on the timeline" ui_click_logged "Timeline IntersectionCurve1"
ui_expect_new "Intersect Geometry: 1 feature [feature F4]" "the curve went to Intersect"
ui_expect_log "Browser Root/Sketches/Sketch1 at" "Sketch1 in the browser"
ui_step "pick Sketch1 in the browser"      ui_click_logged "Browser Root/Sketches/Sketch1"
ui_expect_new "Intersect Geometry: 1 sketch, 1 feature [feature F4; sketch F1]" \
  "a sketch picked in the browser goes in whole, as before"
ui_expect_new "Preview Intersect: ok" "the crossings previewed"
ui_wait_idle
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Intersected 2 item(s): intersection curve F4, sketch F1" "the log names the curve and the sketch"
ui_step "fit (F6)"                         ui_key F6
ui_sync
fixed_at 10 -20 "where the plane crosses the curve, its top (0, 10, -20)"
fixed_at 0 -20 "where the plane crosses Sketch1's arc, at (0, 0, -20)"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch5 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $WORK/curve.mitcad" "saved"
expect_file "$WORK/curve.mitcad" \
  '[bool(p.get("section")) for f in doc["features"] if f["type"] == "sketch" for p in f.get("projections", []) if p["source"] == {"feature": "F4"}]' \
  "[False, True]" "a linked projection of the curve and a linked section of it"
[ -n "$SHOT" ] && ui_capture "$SHOT"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI intersection curve test"
