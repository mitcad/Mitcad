#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConvertMeshCommands.cpp app/commands/ModifyCommands.cpp app/files
# check-all sources: app/browser/BrowserController.cpp core/model/src/features/mesh_convert.rs
# check-all sources: core/model/src/features/mesh_convert core/model/src/document/mesh_convert.rs
# check-all sources: core/ffi/src/kernel/boundary.rs geometry/src/brep_import.cpp tools/cli
# Convert Mesh's panel and Insert Mesh's Convert to solid (mitcad#1082)
# through the real UI, on an empty design mitcad-cli makes (its root
# component active, so the bodies are the root's: an untitled new design
# has Component1 active):
#   1. File > Import of a closed cube of 20 mm from an STL file (12
#      triangles), Convert to solid off: the mesh body cube (F1.b0).
#   2. Convert Mesh from the cube's browser row (its context menu offers
#      it): the panel starts with the mesh body, Faceted Solid with its
#      defaults (0.01 mm, 0.5 deg) previews, OK makes the solid Body1
#      (F2.b0) of 8000 mm3 that the browser lists; the mesh body is hidden.
#      Shell, which takes no mesh bodies, offers Body1's faces.
#   3. Insert Mesh of the cube with one triangle taken out, Convert to
#      solid on: refused with its 3 open edges, the mesh body kept and
#      shown.
#   4. Insert Mesh of the cube again with Convert to solid (remembered on):
#      a solid at once, one undo step with the import.
# The Windows counterpart is tools/ui-windows-convert-mesh-test.ps1 (ctest
# app.ui-windows-convert-mesh).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-convert-mesh-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-convert-mesh.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# The cube from 0 to 20 on every axis, counter-clockwise seen from outside;
# `skip` leaves out the triangle of that number (none when empty).
facet() { printf 'facet normal 0 0 0\nouter loop\nvertex %s\nvertex %s\nvertex %s\nendloop\nendfacet\n' "$1" "$2" "$3"; }
cube_stl() {
  local skip=$1 n=0 tri
  echo "solid cube"
  while read -r tri; do
    n=$((n + 1))
    [ "$n" = "$skip" ] && continue
    IFS='|' read -r a b c <<< "$tri"
    facet "$a" "$b" "$c"
  done << 'EOF'
0 0 0|0 20 0|20 20 0
0 0 0|20 20 0|20 0 0
0 0 20|20 0 20|20 20 20
0 0 20|20 20 20|0 20 20
0 0 0|20 0 0|20 0 20
0 0 0|20 0 20|0 0 20
20 0 0|20 20 0|20 20 20
20 0 0|20 20 20|20 0 20
20 20 0|0 20 0|0 20 20
20 20 0|0 20 20|20 20 20
0 20 0|0 0 0|0 0 20
0 20 0|0 0 20|0 20 20
EOF
  echo "endsolid cube"
}
cube_stl "" > "$WORK/cube.stl"
cube_stl 5 > "$WORK/open.stl"
echo '[{"comment": "An empty design for the inserted meshes."}]' > "$WORK/empty.json"
"$CLI" run "$WORK/empty.json" --save "$WORK/design.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli run"; }

file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}
# has "entry": the context menu logged last offers it.
has() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Context menu: " | tail -1 | grep -qF -- "$1" ||
    ui_fail "the context menu has no '$1'"
  echo "ok   the context menu offers $1"
}
# import_mesh path: File > Import of a mesh file; the Insert Mesh dialog is
# left open with the keyboard.
import_mesh() {
  ui_mark
  ui_step "File > Import $(basename "$1")"  file_menu i
  ui_type_path 'Import' "$1"
  ui_expect_new "Insert Mesh dialog opened: unit mm" "the Insert Mesh dialog opened"
  ui_expect_new "Insert Mesh OK at" "the dialog is laid out"
  ui_focus_dialog '^Insert Mesh$'
}

ui_start_display
ui_start_app --open "$WORK/design.mitcad" --no-native-dialogs

echo "--- 1. The cube as a mesh body"
import_mesh "$WORK/cube.stl"
ui_expect_new "Insert Mesh convert to solid off" "Convert to solid is off by default"
ui_step "OK"                               ui_click_logged "Insert Mesh OK"
ui_focus_main
ui_expect_new "Imported cube.stl as" "the cube came in"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the mesh body shown"
ui_browser_reveal "Browser Root/Bodies/cube"
ui_expect_log "Browser Root/Bodies/cube at" "the browser lists the mesh body"

echo "--- 2. Convert Mesh from the mesh body's row"
ui_mark
ui_step "right-click cube's row"           ui_click_logged "Browser Root/Bodies/cube" 3
ui_expect_new "Context menu: " "the row's menu"
has "Convert Mesh"
ui_step "choose Convert Mesh"              ui_menu_choose "Convert Mesh"
ui_expect_new "Command Convert Mesh started" "the panel opened" 30
ui_expect_new "Convert Mesh Mesh Body: 1 body [body F1.b0]" "with the mesh body" 30
ui_expect_new "Preview Convert Mesh: ok" "the faceted solid previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added convert mesh (Faceted Solid) of F1.b0, tolerance 0.01" "the conversion added" 30
ui_expect_volume "New body Body1 (F2.b0): volume \([0-9.]*\) mm3" 8000 "a solid of the cube's volume"
ui_expect_log "Timeline ConvertMesh1 at" "ConvertMesh1 in the timeline"
ui_browser_reveal "Browser Root/Bodies/Body1"
ui_expect_log "Browser Root/Bodies/Body1 at" "the browser lists the solid Body1"
ui_expect_log "Bodies shown: F2.b0" "the solid shown, the mesh body hidden"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_mark
ui_step "shell (search)"                   ui_command "Shell"
ui_expect_new "Pick face F2.b0/F2:import(" "Shell, which takes no mesh bodies, offers the solid's faces"
ui_step "cancel (Esc)"                     ui_key Escape

echo "--- 3. Insert Mesh with Convert to solid: an open mesh is refused"
import_mesh "$WORK/open.stl"
ui_mark
ui_step "check Convert to solid"           ui_click_logged "Insert Mesh convert"
ui_expect_new "Insert Mesh convert to solid on" "Convert to solid checked"
ui_step "OK"                               ui_click_logged "Insert Mesh OK"
ui_focus_main
ui_expect_new "Insert Mesh: unit mm, convert to solid on, weld tolerance 0.01 mm, merge angle 0.5 deg" \
  "the dialog's choice"
ui_expect_new "Insert Mesh did not convert open (F3.b0)" "the open mesh (named after its file) not converted"
ui_expect_new "3 open edges" "the reason names its open edges"
ui_expect_log "Bodies shown: F2.b0, F3.b0" "the open mesh body kept and shown"

echo "--- 4. Insert Mesh with Convert to solid: the cube at once"
import_mesh "$WORK/cube.stl"
ui_expect_new "Insert Mesh convert to solid on" "Convert to solid remembered"
ui_mark
ui_step "OK"                               ui_click_logged "Insert Mesh OK"
ui_focus_main
ui_expect_new "Insert Mesh converted cube (2) (F4.b0) to a solid by ConvertMesh2" \
  "converted on insertion (the second cube body named cube (2))"
ui_expect_log "Bodies shown: F2.b0, F3.b0, F5.b0" "the new solid shown, its mesh body hidden"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Insert cube.stl" "the import and the conversion are one undo step"
ui_expect_new "Bodies shown: F2.b0, F3.b0" "both gone"

ui_finish "UI convert mesh test"
