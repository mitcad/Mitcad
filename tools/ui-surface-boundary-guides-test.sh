#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp core/model/src/features/boundary_surface.rs
# check-all sources: geometry/src/surface_fills.cpp core/cpp/bridge/surface_fills.cpp tools/cli
# Boundary Surface's Fill a Loop through guide curves (mitcad#2526, GitHub
# mitcad/Mitcad#711) and Patch's direction for sketch curves alone,
# through the real UI (core/model-api/src/commands.md, "Guide curves of a
# loop"):
#   - Rim (F1, on XY) holds a circle of radius 20 as four arcs c6 to c9;
#     Depth (F2, on XZ) the arc c4 from (20, 0, 0) through (0, 0, -8) to
#     (-20, 0, 0) across it.
#   - Patch, in a design of Depth (F1) and a box beside it (no profile
#     under the arc, which Patch would pick first), with Depth's arc alone
#     picked is not built and says to select the enclosed profile or to
#     use Boundary Surface's Fill a Loop.
#   - Boundary Surface, Fill a Loop of Rim's four arcs: the preview fills
#     the flat disc; Depth's arc picked into Guide Curves, the preview
#     fills through it; OK adds one surface body (F3.b0).
#   - BoundarySurface1 edited: the preview fills again with the loaded
#     picks; saved, its definition keeps the four arcs as the loop and
#     Depth's arc as its guide.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-boundary-guides-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-surface-boundary-guides.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/bowl.mitcad
ARC=$WORK/arc.mitcad

# Depth on XZ: its x along the design's X, its y along -Z; the arc's
# centre (0, -21) in it is (0, 0, 21) in the design, radius 29.
cat > "$WORK/bowl.json" << 'EOF'
[
  {"cmd": "add_feature", "name": "Rim", "def": {"type": "sketch", "plane": "xy", "entities": [
    {"id": "p1", "type": "point", "at": [0, 0]},
    {"id": "p2", "type": "point", "at": [20, 0]},
    {"id": "p3", "type": "point", "at": [0, 20]},
    {"id": "p4", "type": "point", "at": [-20, 0]},
    {"id": "p5", "type": "point", "at": [0, -20]},
    {"id": "c6", "type": "arc", "center": "p1", "start": "p2", "end": "p3"},
    {"id": "c7", "type": "arc", "center": "p1", "start": "p3", "end": "p4"},
    {"id": "c8", "type": "arc", "center": "p1", "start": "p4", "end": "p5"},
    {"id": "c9", "type": "arc", "center": "p1", "start": "p5", "end": "p2"}]}},
  {"cmd": "add_feature", "name": "Depth", "def": {"type": "sketch", "plane": "xz", "entities": [
    {"id": "p1", "type": "point", "at": [0, -21]},
    {"id": "p2", "type": "point", "at": [20, 0]},
    {"id": "p3", "type": "point", "at": [-20, 0]},
    {"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}]}}
]
EOF
"$CLI" run "$WORK/bowl.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli bowl"; }

# Depth's arc without Rim, for Patch: in the bowl, Rim's disc lies under
# the arc in the view, and Patch picks profiles first. A box beside it
# (F2) enables Patch, which needs a profile or a body.
cat > "$WORK/arc.json" << 'EOF'
[
  {"cmd": "add_feature", "name": "Depth", "def": {"type": "sketch", "plane": "xz", "entities": [
    {"id": "p1", "type": "point", "at": [0, -21]},
    {"id": "p2", "type": "point", "at": [20, 0]},
    {"id": "p3", "type": "point", "at": [-20, 0]},
    {"id": "c4", "type": "arc", "center": "p1", "start": "p2", "end": "p3"}]}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [40, 0], "length": 20, "width": 10,
   "height": 5, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/arc.json" --save "$ARC" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli arc"; }

# expect_file file 'python expression of doc' expected what
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$1" "$2") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

ui_start_display

echo "--- Patch of a sketch curve alone"
ui_start_app --open "$ARC"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "patch (search)"                   ui_command "Patch"
ui_expect_new "Panel Patch input boundary at" "the panel laid out"
ui_step "the boundary input"               ui_click_logged "Panel Patch input boundary"
ui_mark
ui_step "Depth's arc"                      ui_click_logged "Sketch entity F1/c4"
ui_expect_new "Preview Patch: not built: Sketch curves alone do not make a patch. Select the enclosed profile" \
  "a sketch curve alone points to the profile and to Boundary Surface"
ui_expect_log "or use Boundary Surface with Type Fill a Loop for a loop picked as curves." "the direction in full"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Patch cancelled" "nothing added"
ui_crashed && ui_fail "the application crashed"
ui_stop_app

echo "--- Fill a Loop of the rim through the guide"
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "boundary surface (search)"        ui_command "Boundary Surface"
ui_expect_new "Panel Boundary Surface input guides at" "Guide Curves laid out"
ui_step "the boundary curves input"        ui_click_logged "Panel Boundary Surface input curves"
ui_step "Rim's first arc"                  ui_click_logged "Sketch entity F1/c6"
ui_step "Rim's second arc"                 ui_click_logged "Sketch entity F1/c7"
ui_step "Rim's third arc"                  ui_click_logged "Sketch entity F1/c8"
ui_mark
ui_step "Rim's fourth arc"                 ui_click_logged "Sketch entity F1/c9"
ui_expect_new "Preview Boundary Surface: ok" "the rim fills"
ui_step "the guide curves input"           ui_click_logged "Panel Boundary Surface input guides"
ui_mark
ui_step "Depth's arc"                      ui_click_logged "Sketch entity F2/c4"
ui_expect_new "Preview Boundary Surface: ok" "the rim fills through the guide"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added boundary surface filling 4 curves and the open edges of 0 faces through 1 guide curves" \
  "the loop filled through its guide"
ui_expect_log "(F3.b0): volume 0.000 mm3" "the filled loop, a surface"

echo "--- BoundarySurface1 edited: its rim and its guide"
ui_double_click_expect "Timeline BoundarySurface1" "Editing F3 with Boundary Surface" "BoundarySurface1 is edited"
ui_expect_new "Preview Boundary Surface: ok" "the loaded picks fill the loop through the guide"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F3" "BoundarySurface1 saved as it is"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file "$FILE" \
  '[(sorted(e["curves"]), [(g["sketch"], g["curves"]) for g in f.get("guides", [])]) for f in doc["features"] if f["type"] == "boundary_surface" for e in f["boundary"]]' \
  "[(['c6', 'c7', 'c8', 'c9'], [('F2', ['c4'])])]" \
  "the rim's arcs the loop, Depth's arc its guide"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_crashed && ui_fail "the application crashed"
ui_stop_app
ui_finish "surface boundary through guide curves"
