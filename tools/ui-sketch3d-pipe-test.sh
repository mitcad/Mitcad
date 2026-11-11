#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework app/browser app/MainWindow.cpp app/MainWindowSketch3d.cpp
# check-all sources: app/sketch app/OcctViewer.cpp app/framework/ModelShapes.cpp
# check-all sources: core/model-api/src/sketch3d.rs core/model/src/features/spatial_path.rs
# check-all sources: core/model/src/features/path.rs core/model/src/features/geom_ref.rs
# check-all sources: core/model/src/document/browser.rs core/model/src/features/pipe.rs
# check-all sources: core/model/src/features/sweep.rs core/sketch/src/spatial.rs tools/cli
# check-all sources: core/model/src/features/construction core/model/src/features/pattern.rs
# Pipe and Sweep along 3D sketches in the shared Qt UI (mitcad#1527), the
# route workflow end to end on a design that mitcad-cli makes:
# 3DSketch1 (F1) the route A-B-C-D from the origin, 40 along +X, 30 along
# +Y and 20 along +Z, fully constrained (AB c3, BC c5, CD c7; drawing it is
# tools/ui-sketch3d-test.sh's); 3DSketch2 (F2) three lines from one point
# (a branch) and a closed fit-point spline; Sketch1 (F3) an 8 mm circle on
# YZ at the origin; 3DSketch3 (F4) a closed triangle of lines (c3, c5, c6).
#   - The corners B and C filleted in the UI (Sketch Fillet, F7 the test's
#     key), radius 10 typed for each: 50 + 10 pi of path.
#   - Pipe: one pick of BC with Chain Selection takes the whole route; the
#     panel lists it from A to D (`c3 ... c7, 5 curves (p1 to p6)`), the
#     fillets make it smooth (no sharp corners), Reverse is offered; an
#     8 mm pipe: pi 4^2 (50 + 10 pi) = 4092.410827 mm3.
#   - The route edited (W2): BC's length 30 -> 50 in 3DSketch1's palette
#     list; Finish moves the pipe with it, pi 4^2 (70 + 10 pi) =
#     5097.720476; Undo and Redo of the edit move it back and again.
#   - Pipe1 edited: its route as picked (BC, chained), Reverse runs it from
#     D (`c7 ... c3, 5 curves (p6 to p1)`); saved as the typed path, BC
#     chained and reversed, its volume unchanged.
#   - Sweep: the circle along 3DSketch1 picked whole from the Browser (its
#     one open route), listed from A to D (its volume along a 3D route is
#     CLI sketch3d_pipe's; here it lies in the pipe, whatever operation it
#     takes).
#   - Plane Along Path (mitcad#1550): 3DSketch1 picked whole from the
#     Browser, listed from A to D, Plane1 added across it.
#   - Closed paths (mitcad#1550): 3DSketch3's row takes its triangle whole,
#     listed `c3, c5, c6 (closed, from p1)`, Pipe2 round it; 3DSketch2's
#     closed spline is a closed path by itself, listed `(closed)`.
#   - Refusals: 3DSketch2's row (a branch and the closed spline: two
#     chains, said so), a line of its branch with Chain Selection (listed
#     `Not one path`, the preview fails saying where it branches) and the
#     same line alone once Chain Selection is off.
#   - Saved and opened again (W8): Pipe1's edit lists its route from D.
# The Windows step is in tools/ui-windows-sketch3d-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-pipe-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch3d-pipe.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/route.mitcad
PI=3.14159265358979

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch3d.create"},
  {"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0], "end": [38, 1, 0], "axis": "x",
    "length": 40, "start_source": "origin"},
  {"cmd": "sketch3d.add_line", "sketch": "F1", "start": "p2", "end": [41, 28, 0], "axis": "y", "length": 30},
  {"cmd": "sketch3d.add_line", "sketch": "F1", "start": "p4", "end": [40, 31, 18], "axis": "z", "length": 20},
  {"expect": {"query": {"query": "sketch3d", "uid": "F1"}, "result": {
    "status": {"dof": 0, "fully_constrained": true},
    "chains": [{"curves": ["c3", "c5", "c7"], "closed": false}]}}},
  {"cmd": "sketch3d.create"},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": [0, -40, 0], "end": [20, -40, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p1", "end": [0, -60, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F2", "start": "p1", "end": [0, -40, 20]},
  {"cmd": "sketch3d.add_spline", "sketch": "F2", "closed": true, "points": [[60, -40, 0], [80, -40, 0], [70, -55, 5]]},
  {"cmd": "sketch.create", "plane": "yz"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": 8},
  {"expect": {"feature": "3DSketch2", "status": "ok"}},
  {"cmd": "sketch3d.create"},
  {"cmd": "sketch3d.add_line", "sketch": "F4", "start": [100, -40, 0], "end": [160, -40, 0]},
  {"cmd": "sketch3d.add_line", "sketch": "F4", "start": "p2", "end": [130, 0, 10]},
  {"cmd": "sketch3d.add_line", "sketch": "F4", "start": "p4", "end": "p1"},
  {"expect": {"query": {"query": "sketch3d", "uid": "F4"}, "result": {
    "chains": [{"curves": ["c3", "c5", "c6"], "closed": true,
                "path": {"spatial_sketch": "F4", "curves": ["c3", "c5", "c6"]}}]}}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The closed spline's id, from the saved design.
RING=$(python3 - "$FILE" << 'EOF'
import json, sys
doc = json.load(open(sys.argv[1]))
sketch = [f for f in doc["features"] if f.get("type") == "spatial_sketch"][1]
print(next(c["id"] for c in sketch["curves"] if c["type"] == "fitted_spline"))
EOF
)
[ -n "$RING" ] || ui_fail "no closed spline in 3DSketch2"
echo "ok   3DSketch2's closed spline is $RING"

# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}
# expect_row COMMAND INPUT shown|hidden description: the panel row INPUT as
# the panel laid it out last.
expect_row() {
  local line=""
  for _ in $(seq 1 30); do
    line=$(grep -F "Panel $1 input $2 at " "$UI_LOG" | tail -1)
    if [ "$3" = hidden ]; then
      [[ $line == *"(hidden)" ]] && { echo "ok   $4"; return; }
    else
      [ -n "$line" ] && [[ $line != *"(hidden)" ]] && { echo "ok   $4"; return; }
    fi
    sleep 0.2
  done
  ui_fail "$4: Panel $1 input $2 is not $3 (${line:-never laid out})"
}
SHORT=$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 16 * (50 + 10 * pi) }')
LONG=$(awk -v pi=$PI 'BEGIN { printf "%.6f", pi * 16 * (70 + 10 * pi) }')

ui_start_display
# A key of the test's own for Sketch Fillet.
mkdir -p "$UI_CONFIG/Mitcad"
printf '[shortcuts]\nsketch.fillet=F7\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Sketches/3DSketch1 at" "the Browser lists the route's sketch"

echo "--- The route's corners B and C filleted, radius 10 (W11's bends)"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F1 (3DSketch1)" "3DSketch1 is edited"
ui_expect_new "3D sketch entity F1/p2 at" "the corner B is drawn"
ui_expect_new "3D sketch entity F1/p4 at" "and the corner C"
ui_mark
ui_step "Sketch Fillet (F7)"             ui_key F7
ui_expect_new "3D sketch tool Fillet: started" "Fillet runs in the 3D sketch"
ui_expect_new "3D sketch field radius at" "its Radius field is shown"
ui_mark
ui_step "the Radius field"               ui_click_logged "3D sketch field radius"
ui_sync
ui_step "type 10"                        xdotool type --delay 40 -- "10"
ui_expect_new "3D sketch field radius: 10" "Radius 10 typed: each corner clicked is rounded at once"
# Each corner by its point, one sketch3d.fillet each at the typed radius.
for corner in "p2 of c3 and c5" "p4 of c5 and c7"; do
  point=${corner%% *}
  ui_mark
  ui_step "click the corner $point"      ui_click_logged "3D sketch entity F1/$point"
  ui_expect_new "3D sketch Fillet: corner $corner picked" "the corner $point picked by its point"
  ui_expect_new " at $point, radius 10" "$point rounded with radius 10, one step"
  ui_expect_new "3D sketch entity F1/p4 at" "the sketch drawn again"
done
ui_mark
ui_step "Esc (the field)"                ui_key Escape
ui_expect_new "3D sketch Esc: " "Esc steps back"
ui_sync
if ! tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "3D sketch tool Fillet: stopped"; then
  ui_step "Esc (the tool)"               ui_key Escape
  ui_expect_new "3D sketch tool Fillet: stopped" "the tool ends"
fi
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"

echo "--- Pipe along the route by one pick of BC (W11, chain selection)"
ui_mark
ui_step "pipe (search)"                  ui_command "Pipe"
ui_expect_new "Sketch entity F1/c5 at" "the route's curves are where a click picks them"
ui_mark
ui_step "pick BC"                        ui_click_logged "Sketch entity F1/c5"
ui_expect_new "Pipe Path listed: c3 ... c7, 5 curves (p1 to p6)" "one pick takes the whole route, from A to D"
ui_expect_new "Preview Pipe: ok" "previewed"
ui_expect_new "Pipe: the path has 0 sharp corners; Corners hidden" "the fillets make the route smooth"
expect_row Pipe path_reverse shown "Reverse offered for a 3D sketch's route"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-pipe.png"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added pipe" "Pipe1 added"
ui_expect_volume "New body Body1 (F5.b0): volume \([0-9.]*\) mm3" "$SHORT" "an 8 mm pipe along the route" 1e-4

echo "--- The route edited: BC to 50 (W2); the pipe follows"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F1 (3DSketch1)" "3DSketch1 is edited"
ui_expect_new "3D sketch item k5 field " "BC's length is in the palette's list"
ui_mark
ui_step "BC's length field"              ui_click_logged "3D sketch item k5 field"
ui_sync
ui_step "select its text"                ui_key ctrl+a
ui_sync
ui_step "type 50"                        xdotool type --delay 40 -- "50"
ui_sync
ui_step "Enter"                          ui_key Return
ui_expect_new "3D sketch edit value k5 = 50: done" "the value set, one step"
ui_mark
ui_step "finish (Ctrl+Enter)"            ui_key ctrl+Return
ui_expect_new "Sketch finished" "finished"
ui_expect_new "Body Body1 (F5.b0): volume" "the pipe recomputed"
# Finish Sketch logs each body's volume after an edit as it is now (the
# history's "a -> b" form is Undo's and Redo's).
ui_expect_volume "Body Body1 (F5.b0): volume \([0-9.]*\) mm3" "$LONG" "the pipe follows the longer route" 1e-4
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-edited.png"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Edit 3DSketch1" "the route edit is one step"
ui_expect_new "Body Body1 (F5.b0): volume" "the pipe recomputed"
ui_expect_volume "Body Body1 (F5.b0): volume [0-9.]* -> \([0-9.]*\) mm3" "$SHORT" "the pipe back on the route as it was" 1e-4
ui_mark
ui_step "redo (Ctrl+Y)"                  ui_key ctrl+y
ui_expect_new "Redo: Edit 3DSketch1" "redone"
ui_expect_new "Body Body1 (F5.b0): volume" "the pipe recomputed"
ui_expect_volume "Body Body1 (F5.b0): volume [0-9.]* -> \([0-9.]*\) mm3" "$LONG" "the pipe on the longer route again" 1e-4

echo "--- Pipe1 edited: the route as picked, run from D (Reverse)"
ui_double_click_expect "Timeline Pipe1" "Editing F5 with Pipe" "Pipe1 is edited"
ui_expect_new "Pipe Path listed: c3 ... c7, 5 curves (p1 to p6)" "its route as picked, through BC"
ui_expect_new "Preview Pipe: ok" "previewed as it is"
expect_row Pipe path_reverse shown "Reverse offered"
ui_mark
ui_step "Reverse"                        ui_click_logged "Panel Pipe input path_reverse"
ui_expect_new "Pipe: Reverse = on" "reversed"
ui_expect_new "Path path listed again: c7 ... c3, 5 curves (p6 to p1)" "the route listed from D"
ui_expect_new "Preview Pipe: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F5" "Pipe1 edited"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file "$FILE" \
  '[(f["path"]["spatial_sketch"], f["path"]["curves"], f["path"].get("chain"), f["path"].get("reversed")) for f in doc["features"] if f["type"] == "pipe"]' \
  "[('F1', ['c5'], True, True)]" "saved as the typed path: BC, chained, reversed"

echo "--- Sweep: the circle along 3DSketch1 taken whole from the Browser"
ui_mark
ui_step "sweep (search)"                 ui_command "Sweep"
ui_expect_new "Panel Sweep input path at" "the panel laid out"
ui_step "the path input"                 ui_click_logged "Panel Sweep input path"
ui_sync
ui_mark
ui_step "3DSketch1's row"                ui_click_logged "Browser Root/Sketches/3DSketch1"
ui_expect_new "Path path: 3DSketch1 taken whole: c3, " "the sketch's one open route"
grep -F "Path path: 3DSketch1 taken whole: " "$UI_LOG" | tail -1 | grep -qE "taken whole: c3, c[0-9]+, c5, c[0-9]+, c7$" ||
  ui_fail "not the route through both arcs: $(grep -F "Path path: 3DSketch1 taken whole: " "$UI_LOG" | tail -1)"
echo "ok   AB, the arc at B, BC, the arc at C and CD in order"
ui_expect_new "Sweep Path listed: c3 ... c7, 5 curves (p1 to p6)" "listed from A to D"
ui_expect_new "Preview Sweep: ok" "previewed"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-sweep.png"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added sweep" "Sweep1 added along the route"

echo "--- Plane Along Path: 3DSketch1's route whole from the Browser (mitcad#1550)"
ui_mark
ui_step "plane along path (search)"      ui_command "Plane Along Path"
ui_expect_new "Panel Plane Along Path input path at" "the panel laid out"
ui_step "the path input"                 ui_click_logged "Panel Plane Along Path input path"
ui_sync
ui_mark
ui_step "3DSketch1's row"                ui_click_logged "Browser Root/Sketches/3DSketch1"
ui_expect_new "Path path: 3DSketch1 taken whole: c3, " "the sketch's route"
ui_expect_new "Plane Along Path Path listed: c3 ... c7, 5 curves (p1 to p6)" "listed from A to D"
ui_expect_new "Preview Plane Along Path: ok" "previewed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added plane Plane1: origin (" "Plane1 across the route, half way along it"

echo "--- Closed paths (mitcad#1550): 3DSketch3's triangle whole, 3DSketch2's closed spline"
ui_mark
ui_step "pipe (search)"                  ui_command "Pipe"
ui_expect_new "Sketch entity F4/c5 at" "3DSketch3's curves are where a click picks them"
ui_mark
ui_step "3DSketch3's row"                ui_click_logged "Browser Root/Sketches/3DSketch3"
ui_expect_new "Path path: 3DSketch3 taken whole: c3, c5, c6" "the closed route whole, from its first line"
ui_expect_new "Pipe Path listed: c3, c5, c6 (closed, from p1)" "listed as closed"
ui_expect_new "Preview Pipe: ok" "a pipe round the triangle"
[ -n "$SHOT" ] && ui_capture "${SHOT%.png}-closed.png"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added pipe" "Pipe2 added round the triangle"
ui_mark
ui_step "pipe (search)"                  ui_command "Pipe"
ui_expect_new "Sketch entity F2/$RING at" "3DSketch2's closed spline is where a click picks it"
ui_mark
ui_step "the closed spline"              ui_click_logged "Sketch entity F2/$RING"
ui_expect_new "Pipe Path listed: $RING (closed)" "a closed spline is a closed path by itself"
ui_expect_new "Preview Pipe: ok" "previewed round it"
ui_mark
ui_step "the closed spline again"        ui_click_logged "Sketch entity F2/$RING"
ui_expect_new "Pipe Path: nothing" "taken out again"

echo "--- Refused paths: 3DSketch2's branch"
ui_mark
ui_step "3DSketch2's row"                ui_click_logged "Browser Root/Sketches/3DSketch2"
ui_expect_new "not taken: 3DSketch2 has 2 separate chains: pick a curve of the one to follow" \
  "a whole sketch of a branch and a closed spline is refused, saying why"
ui_mark
ui_step "a line of the branch"           ui_click_logged "Sketch entity F2/c5"
ui_expect_new "Pipe Path listed: Not one path" "chained, the branch is no path"
ui_expect_new "Preview Pipe: failed: " "the preview fails"
grep -F "Preview Pipe: failed: " "$UI_LOG" | tail -1 | grep -qF "branches at p1" ||
  ui_fail "the preview does not say where the path branches: $(grep -F "Preview Pipe: failed: " "$UI_LOG" | tail -1)"
echo "ok   the preview says where the path branches"
ui_mark
ui_step "Chain Selection off"            ui_click_logged "Panel Pipe input chain"
ui_expect_new "Pipe: Chain Selection = off" "only the picked line"
ui_expect_new "Path path listed again: c5 (p1 to p4)" "the line alone, from its start"
ui_expect_new "Preview Pipe: ok" "a line alone is a path"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Pipe cancelled" "nothing added"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "the application crashed"
ui_stop_app

echo "--- Opened again (W8): Pipe1 keeps its route, chained from BC and reversed"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Pipe1 at" "Pipe1 on the timeline"
ui_double_click_expect "Timeline Pipe1" "Editing F5 with Pipe" "Pipe1 is edited"
ui_expect_new "Pipe Path listed: c7 ... c3, 5 curves (p6 to p1)" "its route from D, as saved"
ui_expect_new "Preview Pipe: ok" "previewed as it is"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Pipe cancelled" "nothing changed"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "the application crashed"
ui_finish "UI 3D sketch pipe and sweep"
