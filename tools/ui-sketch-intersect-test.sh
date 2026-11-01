#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandSession.cpp app/sketch
# check-all sources: core/sketch core/model-api/src/sketch.rs core/model/src/document.rs tools/cli
# check-all sources: app/framework/SelectionScope.cpp core/model/src/features/sketch.rs
# Sketch Intersect (mitcad#736) through the real UI:
#   - A 40 x 25 block 20 high, from (-20, -10), and a sketch on XZ, which
#     cuts it through (mitcad-cli builds them).
#   - Sketch2 edited; Intersect (the command search) takes Body1 picked in
#     the browser: the preview shows the section, OK keeps it, linked
#     (Projection Link is on by default).
#   - Extrude1 edited to 30: the section follows the block. Sketch2 has
#     no warning about it, its corner is at the new top (a drag there finds
#     the fixed, projected point), and nothing is left at the old top.
#   - Project of another sketch's curve (mitcad#904): in a second design,
#     Sketch1 (a line and a circle on XY) and Sketch2 on XY after it;
#     Sketch2 edited, Project's selection filter has the row for other
#     sketches' curves and points, a click takes Sketch1's line, and OK
#     logs it by name.
#   - Intersect of another sketch's curve (mitcad#1031): in the same design,
#     Sketch1 also has a circle round (30, 0) of radius 5 and Sketch3 lies
#     on XZ. Sketch3 edited; Intersect's selection filter has the row for
#     other sketches' curves and points, Sketch1 picked in the browser goes
#     in whole (in the view the circle is seen edge on, and Sketch1 is
#     hidden once Sketch2 projects it), and OK gives two fixed points where
#     its circle crosses XZ, at x = 25 and 35 (its line and other circle
#     do not cross it). The saved design has them as one linked section of
#     the circle.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-intersect-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-intersect.XXXXXX)
FILE=$WORK/intersect.mitcad
SKETCHES=$WORK/sketches.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Sketch1 (F1): the rectangle; Extrude1 (F2): Body1 (F2.b0), z from 0 to
# 20; Sketch2 (F3) on XZ: x along X, y along -Z.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-20, -10], "width": 40, "height": 25},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# no_new "text" description: no log line with the text since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# edit_sketch2: Sketch2 edited from the timeline.
edit_sketch2() {
  ui_mark
  ui_step "edit Sketch2 (double-click)"  ui_double_click_logged "Timeline Sketch2"
  ui_expect_new "Editing sketch F3 (Sketch2)" "Sketch2 edited"
  ui_step "fit (F6)"                     ui_key F6
  ui_sync
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"

echo "--- Intersect: the section of Body1"
edit_sketch2
ui_mark
ui_step "intersect (search)"             ui_command "Intersect"
ui_expect_new "Command search chose sketch.intersect" "Intersect started"
ui_sync
ui_expect_log "Browser Root/Bodies/Body1 at" "Body1 in the browser"
ui_step "pick Body1 in the browser"      ui_click_logged "Browser Root/Bodies/Body1"
ui_expect_new "Intersect Geometry: 1 body" "the body went to Intersect"
ui_expect_new "Preview Intersect: ok" "the section previewed"
ui_wait_idle
ui_focus_main
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Intersected 1 item(s)" "the section kept"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"

echo "--- The block grows to 30: the section follows"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new " with Extrude" "editing Extrude1"
ui_step "distance 30"                    ui_type_in "Panel Extrude input distance" "30"
ui_expect_new "Preview Extrude: ok" "the edit previewed"
ui_wait_idle
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F2" "Extrude1 edited"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
echo "ok   the design recomputes"

edit_sketch2
no_new "Palette warning" "Sketch2 has no warning: the section followed"
# The section's top right corner is at x = 20, z = 30 now: (20, -30).
ui_mark
ui_step "drag the section's new corner"  ui_sketch_drag 20 -30 26 -36
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  grep -qE "Drag of p[0-9]+ did not move it|Dragged p[0-9]+ to \(20\.000, -30\.000\)" ||
  ui_fail "no section point at the block's new top corner"
echo "ok   the section's corner is where the block is now"
# The old top line ran along z = 20: nothing of the section is there now.
ui_mark
ui_step "drag at the old top"            ui_sketch_drag 0 -20 6 -26
ui_sync
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Drag of |Dragged " &&
  ui_fail "a section curve stayed at the old top"
echo "ok   nothing of the section is left at the old top"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_stop_app

echo "--- Project: a curve of an earlier sketch (mitcad#904)"
# Sketch1 (F1) on XY: a line c3 from (-20, 10) to (20, 10), a circle c5
# round (0, -10) and a circle c7 round (30, 0) of radius 5, which crosses
# XZ; Sketch2 (F2) on XY after it and Sketch3 (F3) on XZ, empty.
cat > "$WORK/sketches.json" << 'JSON'
[
  {"cmd": "add_feature", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [-20, 10]},
    {"id": "p2", "type": "point", "at": [20, 10]},
    {"id": "c3", "type": "line", "start": "p1", "end": "p2"},
    {"id": "p4", "type": "point", "at": [0, -10]},
    {"id": "c5", "type": "circle", "center": "p4", "radius": 8},
    {"id": "p6", "type": "point", "at": [30, 0]},
    {"id": "c7", "type": "circle", "center": "p6", "radius": 5}]}},
  {"cmd": "sketch.create", "plane": "xy"},
  {"cmd": "sketch.create", "plane": "xz"}
]
JSON
"$CLI" run "$WORK/sketches.json" --save "$SKETCHES" > "$WORK/cli2.log" 2>&1 ||
  { cat "$WORK/cli2.log"; ui_fail "mitcad-cli"; }
ui_start_app --open "$SKETCHES"
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "edit Sketch2 (double-click)"    ui_double_click_logged "Timeline Sketch2"
ui_expect_new "Editing sketch F2 (Sketch2)" "Sketch2 edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_mark
ui_step "project (P)"                    ui_key p
ui_expect_new "Command Project started" "Project started"
ui_expect_new "other-sketch-geometry=cross" "Project's filter has the row for other sketches' geometry"
ui_expect_new "Sketch entity F1/c3 at" "Sketch1's line can be picked"
ui_step "pick Sketch1's line"            ui_click_logged "Sketch entity F1/c3"
ui_expect_new "Project Geometry: 1 sketch curve [sketch curve c3 of F1]" "the line went to Project"
ui_expect_new "Preview Project: ok" "the projection previewed"
ui_wait_idle
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Projected 1 item(s): sketch curve c3 of F1" "the log names the projected curve"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Intersect: where another sketch's circle crosses the plane (mitcad#1031)"
# Sketch3 (F3) on XZ: x along X, y along -Z. The circle c7 crosses it at
# (25, 0, 0) and (35, 0, 0), seen edge on as a line from x = 25 to 35.
ui_mark
ui_step "edit Sketch3 (double-click)"    ui_double_click_logged "Timeline Sketch3"
ui_expect_new "Editing sketch F3 (Sketch3)" "Sketch3 edited"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_mark
ui_step "intersect (search)"             ui_command "Intersect"
ui_expect_new "Command search chose sketch.intersect" "Intersect started"
ui_expect_new "other-sketch-geometry=cross" "Intersect's filter has the row for other sketches' geometry"
ui_expect_log "Browser Root/Sketches/Sketch1 at" "Sketch1 in the browser"
ui_step "pick Sketch1 in the browser"    ui_click_logged "Browser Root/Sketches/Sketch1"
ui_expect_new "Intersect Geometry: 1 sketch" "Sketch1 went to Intersect"
ui_expect_new "Preview Intersect: ok" "the crossings previewed"
ui_wait_idle
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Intersected 1 item(s): sketch F1" "the log names the intersected sketch"
# The fit before Intersect saw nothing of Sketch3: the points come in view.
ui_step "fit (F6)"                       ui_key F6
ui_sync
for x in 25 35; do
  ui_mark
  ui_step "drag the crossing at x = $x"  ui_sketch_drag "$x" 0 $((x + 3)) -6
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    grep -qE "Drag of p[0-9]+ did not move it|Dragged p[0-9]+ to \($x\.000, 0\.000\)" ||
    ui_fail "no fixed point where the circle crosses XZ at x = $x"
  echo "ok   a fixed point where the circle crosses XZ at x = $x"
done
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $SKETCHES" "saved"
# Two points only, one linked section of the circle, no warning.
cat > "$WORK/check.json" << 'JSON'
[
  {"expect": {"query": {"query": "sketch", "uid": "F3"}, "result": {
    "entities": [{"type": "point"}, {"type": "point"}],
    "projections": [{"section": true, "source": {"sketch": "F1", "curve": "c7"}}],
    "warnings": []}}}
]
JSON
"$CLI" run "$WORK/check.json" --open "$SKETCHES" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved crossings"; }
echo "ok   two linked points where the circle crosses XZ"

ui_finish "UI sketch intersect test"
