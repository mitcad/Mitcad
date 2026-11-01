#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/commands/InspectCommands.cpp core/cpp/bridge/analysis.cpp geometry/analysis/src/section.cpp geometry/analysis/src/mesh_section.cpp geometry/analysis/src/common.cpp tools/cli
# Section Analysis through a mesh body (mitcad#392). OCCT's booleans need
# surfaces: on a mesh body's face, which has triangles and none, the pave
# filler read a null handle and the application crashed (Windows) or the
# body was shown whole and the section failed. The analysis now cuts the
# triangles itself.
# A block of 30 x 20 x 10 (F2.b0) and a closed mesh cube of 20 mm from an
# STL file at y = 40, both crossing x = 10:
#   - no body is shown whole, the panel gives the area of both sections
#     (200 + 400 mm²) and their outline (60 + 80 mm);
#   - both cut bodies get a cap (the mesh body's is a planar face).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-mesh-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-section-mesh.XXXXXX)
FILE=$WORK/mesh.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
# The cube from x = 0 to 20, y = 40 to 60, z = 0 to 20, counter-clockwise
# seen from outside.
facet() { printf 'facet normal 0 0 0\nouter loop\nvertex %s\nvertex %s\nvertex %s\nendloop\nendfacet\n' "$1" "$2" "$3"; }
{
  echo "solid cube"
  facet "0 40 0" "20 60 0" "20 40 0";    facet "0 40 0" "0 60 0" "20 60 0"
  facet "0 40 20" "20 40 20" "20 60 20"; facet "0 40 20" "20 60 20" "0 60 20"
  facet "0 40 0" "20 40 0" "20 40 20";   facet "0 40 0" "20 40 20" "0 40 20"
  facet "20 40 0" "20 60 0" "20 60 20";  facet "20 40 0" "20 60 20" "20 40 20"
  facet "20 60 0" "0 60 0" "0 60 20";    facet "20 60 0" "0 60 20" "20 60 20"
  facet "0 60 0" "0 40 0" "0 40 20";     facet "0 60 0" "0 40 20" "0 60 20"
  echo "endsolid cube"
} > "$WORK/cube.stl"
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli run"; }
"$CLI" import "$WORK/cube.stl" --open "$FILE" --save "$FILE" >> "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli import"; }
grep -q "1 faces, 0 edges" "$WORK/cli.log" || { cat "$WORK/cli.log"; ui_fail "no mesh body imported"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the block and the mesh shown"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
# YZ from the browser: the hidden origin stays hidden (mitcad#1030).
ui_pick_origin_row YZ
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_step "10 along X"                       ui_type_in "Panel Section Analysis input offset" "10"
ui_expect_new "Inspect Section Analysis: area 600, length 140" "both sections measured"
ui_expect_new "Section caps: 2 face(s) at" "a cap on the block and on the mesh body"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "shown whole: " &&
  ui_fail "a body was shown whole"
echo "ok   no body shown whole"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Closed Section Analysis" "closed"

ui_finish "UI section mesh test"
