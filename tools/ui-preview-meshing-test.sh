#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/DisplayMeshes.cpp app/framework/DocumentSession.cpp
# check-all sources: app/framework/CommandSession.cpp app/MainWindow.cpp app/OcctViewer.cpp
# check-all sources: app/view/ViewTessellation.hpp geometry/io/src/mesh.cpp geometry/src/history.cpp tools/cli
# A dragged Extrude's preview meshes (mitcad#635) through the real UI, on
# the synthetic block of the ticket that mitcad-cli makes: 120 x 100 x 20
# mm with 83 through holes of 4 mm, and a 20 x 10 mm rectangle at (50, 45)
# on a plane 20 mm up (its top), whose Extrude joins it (a body of 94
# faces). With MITCAD_MESH_TRACE=1:
#   - a typed distance previews in the interactive tier ("Display meshes
#     of preview: 2 bodies ... (interactive)"), the 88 faces the Join keeps
#     taken from the block as shown, only the top and the boss meshed
#     ("Mesh trace preview interactive: ... 88 faces reused, 0 faces from the
#     result store; faces=... kept=88
#     meshed=11"); once no input changes, the bodies themselves are meshed
#     in the final tier and drawn ("refining the display meshes of 2
#     bodies", "display meshes refined");
#   - a drag of the distance's arrow previews in the interactive tier while
#     the button is held and refines once it is released;
#   - OK keeps the final meshes; undo and redo show the block and the
#     joined body again;
#   - the view never meshes a plane of the perforated body itself, on the
#     UI thread: no "mesh-trace: presentation" line with a plane of 84 or
#     85 wires, from the opening to the end.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preview-meshing-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-preview-meshing.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
export MITCAD_MESH_TRACE=1

# The drill sketch's circles and their regions (F3: each circle is c1, c3, c5 ...).
circles=""
regions=""
n=0
for x in 8 18 28 38 48 58 68 78 88 98 108; do
  for y in 8 18 28 38 48 58 68 78 88; do
    if [ "$x" -gt 45 ] && [ "$x" -lt 80 ] && [ "$y" -gt 35 ] && [ "$y" -lt 70 ]; then
      continue
    fi
    n=$((n + 1))
    circles+="{\"cmd\": \"sketch.add_circle\", \"sketch\": \"F3\", \"center\": [$x, $y], \"diameter\": 4},"
    regions+="${regions:+, }{\"sketch\": \"F3\", \"region\": \"r{c$((2 * n - 1))}\"}"
  done
done
[ "$n" -eq 83 ] || ui_fail "83 holes, not $n"
cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 120, "height": 100},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  $circles
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [$regions],
    "start": {"type": "offset", "offset": -5}, "extent": {"type": "distance", "distance": 30}, "operation": "cut"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 20}}},
  {"cmd": "sketch.create", "plane": "F5"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [50, 45], "width": 20, "height": 10},
  {"expect": {"body": "Body1", "faces": 89}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli block"; }

# no_presentation_meshing description: the view meshed no plane of the
# perforated body itself (the trace names the slowest faces it meshed).
no_presentation_meshing() {
  grep -E "mesh-trace: presentation .*plane wires=8[45] " "$UI_LOG" &&
    ui_fail "$1: the view meshed a plane of the perforated body on the UI thread"
  echo "ok   $1"
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the perforated block shown"
ui_expect_log "Mesh trace block.mitcad final: " "the block meshed in the opening job"

echo "=== A typed distance"
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "Command Extrude started" "Extrude started"
# A new Extrude starts at 0 mm and builds nothing (mitcad#2481).
ui_step "distance 10 mm"                   ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Extrude: automatic Operation = Join" "Join onto the block (10 mm typed)"
ui_mark
ui_step "distance 5"                       ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_expect_new "(interactive)" "the preview meshed in the interactive tier"
ui_expect_new "Mesh trace preview interactive: 0 bodies ready, 2 meshed, 88 faces reused, 0 faces from the result store; faces=" \
  "the 88 faces the Join keeps taken from the block as shown"
ui_expect_new "kept=88 meshed=11 failed=0" "only the top and the boss meshed (and the tool)"
ui_expect_new "Preview Extrude: refining the display meshes of 2 bodies" "refined once the input rests" 10
ui_expect_new "Display meshes of preview refinement: 2 bodies in" "the bodies meshed in the final tier"
ui_expect_new "Preview Extrude: display meshes refined" "drawn with the final meshes"
no_presentation_meshing "no plane of the preview meshed by the view"

echo "=== A drag of the distance's arrow"
ui_expect_log "Manipulator Extrude distance at " "the distance's arrow"
ui_focus_main
ui_mark
ui_step "drag the arrow up"                ui_drag_from "Manipulator Extrude distance" 0 -60
ui_expect_new "Manipulator Extrude distance dragged to " "dragged"
ui_expect_new "(interactive)" "previews in the interactive tier during the drag"
ui_expect_new "Preview Extrude: refining the display meshes of 2 bodies" "refined after the release" 10
ui_expect_new "Preview Extrude: display meshes refined" "drawn with the final meshes after the drag"
no_presentation_meshing "no plane of the dragged previews meshed by the view"

echo "=== OK, undo and redo"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added extrude (Join)" "the boss joined"
ui_expect_new "(F2.b0): volume " "the joined body's volume changed"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: " "undone"
ui_mark
ui_step "redo (Ctrl+Shift+Z)"              ui_key ctrl+shift+z
ui_expect_new "Redo: " "redone"
no_presentation_meshing "no plane meshed by the view after OK, undo and redo"
grep -q "faces that could not be meshed" "$UI_LOG" && ui_fail "faces failed to mesh"
# The jobs' display meshings, for the record.
grep -E "Display meshes of |Mesh trace " "$UI_LOG" | cut -c1-260
ui_stop_app
echo "PASS: preview meshing"
