#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: core/model/src/features/boundary_surface.rs core/model/src/features/path.rs
# check-all sources: core/model/src/features/spatial_path.rs tools/cli
# Boundary Surface with curves of several sketches (mitcad#2198, GitHub
# mitcad/Mitcad#540) through the real UI (core/model-api/src/commands.md,
# "A loop through several sketches"):
#   - A design with only a 3D sketch: Boundary Surface is enabled (the
#     command search runs it).
#   - Sketch1 (F1) the line (0, 0)-(20, 0), Sketch2 (F2) the line
#     (20, 0)-(0, 20), 3DSketch1 (F3) the line (0, 20, 0)-(0, 0, 0) and a
#     box (F4) beside them: Fill a Loop of the three lines picked one by
#     one fills to one face (F5.b0, a surface). Tangent to Sketch Plane is
#     hidden until a planar sketch's line is picked; Edge Continuity stays
#     hidden with curves only, is shown once a box edge is picked into
#     Boundary Edges and hidden again when it is taken out.
#   - BoundarySurface1 edited: its three sketches' curves come back (the
#     preview fills the loop, which two of them would not close); saved
#     as one entry per sketch, the planar ones tangent (g1), the 3D
#     sketch's not.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-boundary-sketches-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-surface-boundary-sketches.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
ONLY=$WORK/only3d.mitcad
FILE=$WORK/loop.mitcad

cat > "$WORK/only3d.json" << 'EOF'
[
  {"cmd": "sketch3d.create"},
  {"cmd": "sketch3d.add_line", "sketch": "F1", "start": [0, 0, 0], "end": [10, 0, 5]}
]
EOF
"$CLI" run "$WORK/only3d.json" --save "$ONLY" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli only3d"; }

cat > "$WORK/loop.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 0], "end": [20, 0]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [20, 0], "end": [0, 20]},
  {"cmd": "sketch3d.create"},
  {"cmd": "sketch3d.add_line", "sketch": "F3", "start": [0, 20, 0], "end": [0, 0, 0]},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0], "length": 20, "width": 10,
   "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/loop.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli loop"; }

# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}
# expect_row INPUT shown|hidden description: Boundary Surface's panel row
# INPUT as the panel laid it out last.
expect_row() {
  local line=""
  for _ in $(seq 1 30); do
    line=$(grep -F "Panel Boundary Surface input $1 at " "$UI_LOG" | tail -1)
    if [ "$2" = hidden ]; then
      [[ $line == *"(hidden)" ]] && { echo "ok   $3"; return; }
    else
      [ -n "$line" ] && [[ $line != *"(hidden)" ]] && { echo "ok   $3"; return; }
    fi
    sleep 0.2
  done
  ui_fail "$3: Panel Boundary Surface input $1 is not $2 (${line:-never laid out})"
}
# The name of the last logged edge pick whose name holds both faces.
edge_of() {
  grep -F "Pick edge" "$UI_LOG" | grep -F -- "$1" | grep -F -- "$2" | tail -1 |
    sed -n 's/^Pick edge [^/]*\/\(.*\) at [0-9]*,[0-9]*$/\1/p'
}

ui_start_display

echo "--- A design with only a 3D sketch: Boundary Surface is enabled"
ui_start_app --open "$ONLY"
ui_step "fit (F6)"                         ui_key F6
ui_mark
# The command search runs only an enabled command.
ui_step "boundary surface (search)"        ui_command "Boundary Surface"
ui_expect_new "Panel Boundary Surface input curves at" "Boundary Surface runs with a 3D sketch alone"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Boundary Surface cancelled" "nothing added"
ui_crashed && ui_fail "the application crashed"
ui_stop_app

echo "--- A loop of three lines in two planar sketches and a 3D sketch"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "boundary surface (search)"        ui_command "Boundary Surface"
ui_expect_new "Panel Boundary Surface input curves at" "the panel laid out"
expect_row tangent hidden "Tangent to Sketch Plane hidden before any curve"
expect_row continuity hidden "Edge Continuity hidden before any edge"
ui_step "the boundary curves input"        ui_click_logged "Panel Boundary Surface input curves"
ui_step "Sketch1's line"                   ui_click_logged "Sketch entity F1/c1"
expect_row tangent shown "Tangent to Sketch Plane shown for a planar sketch's line"
ui_step "Sketch2's line"                   ui_click_logged "Sketch entity F2/c1"
ui_mark
ui_step "3DSketch1's line"                 ui_click_logged "Sketch entity F3/c3"
ui_expect_new "Preview Boundary Surface: ok" "the three sketches' lines fill the loop"
expect_row continuity hidden "Edge Continuity hidden with curves only"
ui_mark
ui_step "Tangent to Sketch Plane"          ui_click_logged "Panel Boundary Surface input tangent"
ui_expect_new "Preview Boundary Surface: ok" "previewed tangent to the planar sketches' plane"

echo "--- Edge Continuity follows the picked edges"
ui_step "the boundary edges input"         ui_click_logged "Panel Boundary Surface input edges"
edge=$(edge_of "F4:side0" "F4:top")
[ -n "$edge" ] || ui_fail "no pick place logged for the box's top edge"
ui_step "a box edge"                       ui_click_pick "$edge"
expect_row continuity shown "Edge Continuity shown once an edge is picked"
ui_mark
ui_step "the box edge again"               ui_click_pick "$edge"
expect_row continuity hidden "Edge Continuity hidden again with the edge taken out"
ui_expect_new "Preview Boundary Surface: ok" "the three lines alone again"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added boundary surface filling 3 curves and the open edges of 0 faces" "the loop filled"
ui_expect_log "(F5.b0): volume 0.000 mm3" "the filled loop, a surface"

echo "--- BoundarySurface1 edited: its three sketches' curves"
ui_double_click_expect "Timeline BoundarySurface1" "Editing F5 with Boundary Surface" "BoundarySurface1 is edited"
ui_expect_new "Preview Boundary Surface: ok" "the loaded curves close the loop"
expect_row tangent shown "Tangent to Sketch Plane shown for the loaded planar curves"
expect_row continuity hidden "Edge Continuity hidden: no edges in the loop"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F5" "BoundarySurface1 saved as it is"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file "$FILE" \
  '[(e["sketch"], e["curves"], e.get("continuity")) for f in doc["features"] if f["type"] == "boundary_surface" for e in f["boundary"]]' \
  "[('F1', ['c1'], 'g1'), ('F2', ['c1'], 'g1'), ('F3', ['c3'], None)]" \
  "one entry per sketch, the planar ones tangent to their plane"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "the application crashed"
ui_stop_app
ui_finish "surface boundary of several sketches"
