#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model-api/src/sketch.rs
# Body edges and vertices in the sketch plane through the real UI
# (mitcad#256, mitcad#258), with sketches on the top face of a 60 x 40 x 20
# block. Sketch2 has two free points:
#   - Sketch Dimension: a click on a block edge, a click on a sketch point
#     and a value: the edge is projected (linked) and the dimension added
#     in one undo step, and the point moves; one undo takes both back.
#   - Esc after the first pick leaves the sketch unchanged.
#   - Dimensioning again from the same edge uses its projection again,
#     and the body geometry read before the dimension added (mitcad#1646).
#   - With Hide Above Sketch on, a block corner is still picked.
#   - A dimension between two block edges is driven.
#   - The saved design has the linked projections and the dimensions.
# Sketch3 is empty (mitcad#258):
#   - Point tool, a click near a block corner: the corner projected
#     (linked), a point and a coincident constraint; one undo takes all
#     three back. Again, and a click near the middle of a top edge: the
#     edge projected, a point and a midpoint constraint.
#   - The block made 80 wide (Sketch1's width): both points follow.
#   - A click at the same corner again adds no projection and no point.
# Sketch4 is 10 above the block's top (mitcad#866, GitHub #50), with a
# free point:
#   - A block edge out of its plane selected, then Sketch Dimension (D): the
#     selection is used, the edge projected (linked) and the distance from
#     the point added.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-body-picks-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-body-picks.XXXXXX)
FILE=$WORK/block.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT

TOP='F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})'

# Sketch1 (F1) 60 x 40 at the origin (its width k5, d1, shown above its bottom line, in the view),
# Extrude1 (F2) 20 up: Body1 (F2.b0). Sketch2 (F3) on its top face (the
# sketch's x and y are the design's), with two free points: (20, 10) and
# (45, 25). Sketch3 (F4) on the same face, empty. Sketch4 (F5) on the XY
# plane 30 up, 10 above the block's top, with a free point at (20, 10).
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [30, 6]},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "$TOP", "body": "F2.b0"}},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [20, 10]},
  {"cmd": "sketch.add_point", "sketch": "F3", "at": [45, 25]},
  {"cmd": "sketch.create", "plane": {"face": "$TOP", "body": "F2.b0"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.redefine_plane", "sketch": "F5", "offset": 30},
  {"cmd": "sketch.add_point", "sketch": "F5", "at": [20, 10]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# double_click_at x y: a double click at a screen position.
double_click_at() { xdotool mousemove "$1" "$2" click --repeat 2 --delay 90 1; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# edit_sketch name uid: double-clicks the sketch in the timeline and fits the
# view.
edit_sketch() {
  local tx ty
  read -r tx ty <<< "$(ui_logged_at "Timeline $1")"
  ui_mark
  ui_step "double-click $1"              double_click_at "$tx" "$ty"
  ui_expect_new "Editing sketch $2 ($1)" "$1 edited"
  ui_step "fit (F6)"                     ui_key F6
  ui_sync
}

# dimension_value value: types the dimension's value into the editor the
# placing click opened and enters it.
dimension_value() {
  ui_expect_new "Sketch value editor at" "the value editor opened"
  ui_step "type $1"                      xdotool type --delay 40 "$1"
  ui_step "Enter"                        ui_key Return
}

# expect_file python-expression description: a Python expression on the
# saved design (doc; s is Sketch2's definition) is true.
expect_file() {
  python3 - "$FILE" "$1" << 'PY' || ui_fail "$2: $1"
import json, math, sys
doc = json.load(open(sys.argv[1]))
s = doc["features"][2]
points = {e["id"]: e["at"] for e in s.get("entities", []) if e.get("type") == "point"}
projections = s.get("projections", [])
dimensions = s.get("dimensions", [])
sys.exit(0 if eval(sys.argv[2]) else 1)
PY
  echo "ok   $2"
}

save() {
  ui_mark
  ui_step "save (Ctrl+S)"                ui_key ctrl+s
  ui_expect_new "Saved $FILE" "saved"
}

# expect_new_line "line" description: a line logged since the mark is exactly
# that (a status line that is the start of another).
expect_new_line() {
  for _ in $(seq 1 30); do
    tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qxF -- "$1" && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: no line '$1' since the mark"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the block shown"
ui_expect_log "Timeline Sketch2 at" "Sketch2 in the timeline"

echo "--- Sketch2 on the block's top face"
edit_sketch Sketch2 F3
ui_expect_log "Sketch Sketch2: 4 DOF" "two free points"
ui_expect_log "Sketch Sketch2: 0 linked projection(s)" "nothing projected yet"

echo "--- Esc after the first pick changes nothing"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the block's front edge"    ui_sketch_click 30 0
ui_expect_new "Sketch plane geometry: 4 edge(s), 4 vertex(es)" "the top face's edges and corners are pickable"
ui_expect_new "Sketch Dimension picked the body edge E{" "the edge picked"
ui_step "Esc"                            ui_key Escape
ui_sync
since_mark "Projected the body" && ui_fail "a pick projected the edge"
since_mark "Sketch Sketch2: 1 linked projection(s)" && ui_fail "the sketch changed"
echo "ok   nothing projected"

echo "--- A dimension from the edge to a point: one undo step"
# Elsewhere on the edge: a press where the last one was, that soon, would
# be a double-click.
ui_mark
ui_step "pick the block's front edge"    ui_sketch_click 38 0
ui_step "pick the point (20, 10)"        ui_sketch_click 20 10
ui_step "place the value above it"       ui_sketch_click 30 5
dimension_value 15
ui_expect_new "Projected the body edge E{" "the edge projected"
ui_expect_new "Added dimension k" "a driving dimension"
ui_expect_new "point_line_distance" "from the point to the projected edge"
ui_expect_new "Sketch Sketch2: 3 DOF" "the point is 15 mm from the edge"
ui_expect_new "Sketch Sketch2: 1 linked projection(s)" "one linked projection"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Dimension to Sketch2" "one undo step, named after the dimension"
ui_expect_new "Sketch Sketch2: 4 DOF" "the dimension gone"
ui_expect_new "Sketch Sketch2: 0 linked projection(s)" "the projection gone with it"

echo "--- Again, and again from the same edge: its projection is used"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the block's front edge"    ui_sketch_click 30 0
ui_step "pick the point (20, 10)"        ui_sketch_click 20 10
ui_step "place the value above it"       ui_sketch_click 30 5
dimension_value 15
ui_expect_new "Projected the body edge E{" "the edge projected"
ui_expect_new "Sketch Sketch2: 3 DOF" "the point is 15 mm from the edge"
ui_mark
ui_step "pick the front edge again"      ui_sketch_click 24 0
ui_step "pick the point (45, 25)"        ui_sketch_click 45 25
ui_step "place the value beside it"      ui_sketch_click 52 12
dimension_value 30
ui_expect_new "Added dimension k" "a second dimension"
ui_expect_new "Sketch Sketch2: 2 DOF" "the second point is 30 mm from the edge"
since_mark "Projected the body" && ui_fail "the edge was projected a second time"
since_mark "Sketch Sketch2: 2 linked projection(s)" && ui_fail "a second projection"
echo "ok   the existing projection used"
# The dimension added before changed the sketch, not the bodies before it:
# the body geometry out of the plane is not read again (mitcad#1646, the
# model's plane_geometry_key; seconds in an assembly of hundreds of bodies).
since_mark "Sketch body geometry out of the plane:" && ui_fail "the body geometry was read again after a dimension"
echo "ok   the body geometry kept after a dimension added"

# The top right of the view is the orientation cube's: the clicks stay
# away from it.
echo "--- Hide Above Sketch: a corner of the block is still picked"
ui_mark
ui_step "check Hide Above Sketch"        ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch on" "on"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the corner (0, 40)"        ui_sketch_click 0 40
ui_expect_new "Sketch Dimension picked the body vertex V{" "the corner picked while the block is cut"
ui_step "pick the point (45, 30)"        ui_sketch_click 45 30
ui_step "place the value between them"  ui_sketch_click 20 35
dimension_value 20
ui_expect_new "Projected the body vertex V{" "the corner projected"
ui_expect_new "Added dimension k" "a distance to the corner"
ui_expect_new "Sketch Sketch2: 2 linked projection(s)" "two linked projections"
ui_mark
ui_step "uncheck Hide Above Sketch"      ui_click_logged "Palette paletteHideAbove"
ui_expect_new "Hide Above Sketch off" "off"

echo "--- Between two block edges: driven"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the left edge"             ui_sketch_click 0 20
ui_step "pick the right edge"            ui_sketch_click 60 20
ui_step "place the value between them"  ui_sketch_click 30 22
ui_expect_new "Sketch value editor at" "the value editor opened"
ui_step "accept the value (Enter)"       ui_key Return
ui_expect_new "Added driven dimension k" "driven: both edges are fixed"
ui_expect_new "line_distance" "the distance between the edges"
ui_expect_new "Sketch Sketch2: 4 linked projection(s)" "both edges projected"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch2 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The saved design"
save
expect_file 'len(projections) == 4 and all(p.get("by_geometry") and p.get("body") == "F2.b0" for p in projections)' \
  "four linked projections of Body1"
expect_file 'sum(p["source"].startswith("E{") for p in projections) == 3 and sum(p["source"].startswith("V{") for p in projections) == 1' \
  "three edges and a corner"
expect_file 'len(dimensions) == 4 and sum("value" in d for d in dimensions) == 3' \
  "three driving dimensions and a driven one"
expect_file 'any(abs(p[1] - 15) < 1e-4 and abs(p[0] - 20) < 1e-4 for p in points.values())' \
  "the first point moved to 15 mm from the edge"
expect_file 'any(abs(p[1] - 30) < 1e-4 and abs(math.hypot(p[0], 40 - p[1]) - 20) < 1e-4 for p in points.values())' \
  "the second point 30 mm from the edge and 20 mm from the corner"

echo "--- Point tool on a block corner (mitcad#258)"
edit_sketch Sketch3 F4
ui_expect_log "Sketch Sketch3: 0 linked projection(s)" "nothing projected yet"
ui_mark
ui_step "point (search)"                 ui_command "Point"
ui_expect_new "Sketch tool Point" "the Point tool"
ui_mark
ui_step "click near the corner (60, 0)" ui_sketch_click 59.7 0.3
ui_expect_new "Sketch plane geometry: 4 edge(s), 4 vertex(es)" "the top face's corners and edges to snap to"
ui_expect_new "Projected the body vertex V{" "the corner projected"
ui_expect_new "Added point p" "a point"
ui_expect_new "coincident with p" "on the projected corner"
ui_expect_new "Sketch Sketch3: 1 linked projection(s)" "one linked projection"
expect_new_line "Sketch Sketch3: 0 DOF, fully constrained" "the point is held at the corner"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Point to Sketch3" "one undo step, named after the point"
ui_expect_new "Sketch Sketch3: 0 linked projection(s)" "the projection gone"
expect_new_line "Sketch Sketch3: 0 DOF" "the point and its constraint gone with it"
ui_mark
# Not a double-click on the first one (Qt takes a press that near and soon
# as one).
sleep 0.5
ui_step "click near the corner again"    ui_sketch_click 59.7 0.3
ui_expect_new "Projected the body vertex V{" "the corner projected"
ui_expect_new "coincident with p" "the point on it"

echo "--- Point tool at the middle of a block edge"
ui_mark
ui_step "click near the front edge's middle" ui_sketch_click 30.3 0.2
ui_expect_new "Projected the body edge E{" "the edge projected"
ui_expect_new "Added point p" "a point"
ui_expect_new "at the midpoint of c" "at the projected edge's midpoint"
ui_expect_new "Sketch Sketch3: 2 linked projection(s)" "two linked projections"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"

echo "--- The block 80 wide: the points follow"
edit_sketch Sketch1 F1
ui_expect_new "Dimension k5 text at" "the width's value placed"
ui_mark
ui_step "double-click the width"         ui_double_click_logged "Dimension k5 text"
ui_expect_new "Editing dimension k5" "the width's value editor"
ui_step "type 80"                        xdotool type --delay 40 "80"
ui_step "Enter"                          ui_key Return
ui_expect_new "Dimension k5 = 80 mm (d1)" "the width changed"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Body Body1 (F2.b0): volume 64000.000 mm3" "the block is 80 x 40 x 20"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- The same corner again: nothing added"
edit_sketch Sketch3 F4
ui_expect_log "Sketch Sketch3: 2 linked projection(s)" "the two projections"
ui_mark
ui_step "point (search)"                 ui_command "Point"
ui_expect_new "Sketch tool Point" "the Point tool"
ui_mark
ui_step "click the corner (80, 0)"       ui_sketch_click 79.7 0.3
ui_sync
since_mark "Projected the body" && ui_fail "the corner projected again"
since_mark "Added point" && ui_fail "a second point at the corner"
since_mark "linked projection(s)" && ui_fail "the projections changed"
echo "ok   no second projection and no second point"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch3 finished"
save
# The design as the model evaluates it: Sketch3's projected corner and
# edge where the block is now, the points held on them. The edge ends at
# the corner: its end is the corner's projected point (mitcad#741).
cat > "$WORK/check.json" << 'EOF2'
[
  {"expect": {"query": {"query": "sketch", "uid": "F4"}, "result": {
    "entities": [{"type": "point", "at": [80, 0]}, {"type": "point", "at": [80, 0]},
                 {"type": "point"}, {"type": "line"},
                 {"type": "point", "at": [40, 0]}],
    "constraints": [{"type": "coincident"}, {"type": "midpoint"}],
    "projections": [{"body": "F2.b0", "by_geometry": true}, {"body": "F2.b0", "by_geometry": true}],
    "warnings": []}}}
]
EOF2
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "Sketch3 does not follow the block"; }
echo "ok   both points follow the block: the corner (80, 0), the edge's middle (40, 0)"

echo "--- Sketch4 10 above the block: a selected edge out of its plane, then D (mitcad#866)"
edit_sketch Sketch4 F5
ui_expect_log "Sketch Sketch4: 2 DOF" "a free point"
ui_mark
# Seen from above, the block's top front edge lies under the sketch's
# y = 0: a click there selects it.
ui_step "select the block's front edge"  ui_sketch_click 38 0
ui_expect_new "Selected: 1 edge [" "the edge 10 below the sketch selected"
ui_mark
ui_step "sketch dimension (D)"           ui_key d
ui_expect_new "Sketch body geometry out of the plane:" "the edges out of the plane read"
ui_expect_new "out of the plane: projected as a line" "the selected edge taken as the line it projects to"
since_mark "cannot be projected onto this sketch" && ui_fail "the selected edge was refused"
ui_step "pick the point (20, 10)"        ui_sketch_click 20 10
ui_step "place the value above it"       ui_sketch_click 30 5
dimension_value 15
ui_expect_new "Projected the body edge E{" "the edge projected onto the sketch plane"
ui_expect_new "point_line_distance" "from the point to the projected edge"
ui_expect_new "Sketch Sketch4: 1 DOF" "the point is 15 mm from the edge"
ui_expect_new "Sketch Sketch4: 1 linked projection(s)" "the projection linked"
ui_step "end the tool (Esc)"             ui_key Escape
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch4 finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI sketch body picks test"
