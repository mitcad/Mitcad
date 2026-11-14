#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files/MainWindowSaveAsMesh.cpp app/files/SaveAsMesh.cpp app/files/Print3d.cpp
# check-all sources: app/MainWindowContextMenu.cpp app/browser core/model/src/exchange core/3mf geometry/io tools/cli
# check-all sources: app/files/FileDialogs.cpp app/files/MainWindowFiles.cpp
# Save As Mesh (mitcad#734) through the real UI, on a design that
# mitcad-cli makes: Plate (F1.b0, 40 x 20 x 10, its top the print bed
# face), Lid (F2.b0, 20 x 20 x 5 at x = 60, in its own component Lid:1)
# and Skin (F3.b0, a box with its bottom unstitched: a surface body):
#   1. Plate's browser row: Save As Mesh… is the menu's last entry before
#      Collapse All; the dialog offers the print bed face; 3MF on the bed
#      with the proposed name Plate.3mf, turned over onto its top and
#      centred on the plate, in millimetres.
#   2. The component Lid:1's row: no print bed face to offer; STL (ASCII)
#      in inches, proposed Lid.stl (the component's name): the numbers in
#      inches.
#   3. The root's row (the whole design): Skin left out; OBJ, a file per
#      body, Custom with a 1 mm longest edge, into a folder: Plate.obj and
#      Lid.obj, Plate's mesh split.
#   4. Skin's row: Save As Mesh… disabled with the reason.
#   5. The view's context menu on Plate's top offers it too; the dialog is
#      cancelled.
#   6. File > Export remembers (mitcad#1164): Plate selected, the dialog
#      starts as STEP named Plate in the last folder; 3MF with refinement
#      High chosen and written. Lid selected next: the dialog starts with
#      3MF, High and Lid's name, and Enter writes Lid.3mf.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-save-as-mesh-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-save-as-mesh.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/bench.mitcad
mkdir -p "$WORK/meshes"

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [60, 0],
    "length": 20, "width": 20, "height": 5, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 40],
    "length": 20, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "unstitch", "body": "F3.b0", "faces": ["F3:bottom"]}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"},
  {"cmd": "rename_body", "uid": "F2.b0", "name": "Lid"},
  {"cmd": "rename_body", "uid": "F3.b0", "name": "Skin"},
  {"cmd": "set_print_bed_face", "uid": "F1.b0", "face": "F1:top"},
  {"cmd": "components_from_bodies", "bodies": ["F2.b0"]}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The last "Context menu: ..." line's entries, " | " between them.
menu_line() { grep -F "Context menu: " "$UI_LOG" | tail -1 | sed "s/.*Context menu: //"; }
# last_entry ENTRY: ENTRY is the browser row menu's last entry before
# Collapse All, which ends every row's menu (mitcad#312).
last_entry() {
  [[ "$(menu_line | sed 's/…/.../g')" == *"| ${1//…/...} | Collapse All" ]] ||
    ui_fail "'$1' is not the context menu's last entry before Collapse All: $(menu_line)"
  echo "ok   the context menu ends with $1, then Collapse All"
}
# has ENTRY: the context menu has the entry.
has() {
  [[ "| $(menu_line | sed 's/…/.../g') |" == *"| ${1//…/...} |"* ]] || ui_fail "'$1' is not in the context menu: $(menu_line)"
  echo "ok   the context menu has $1"
}

# row_menu ROW: the context menu of a browser row ("Browser Root/...").
row_menu() {
  ui_focus_main
  ui_mark
  ui_step "right-click $1"                 ui_click_logged "$1" 3
  ui_expect_new "Context menu: " "the menu of $1 opened"
}

# save_dialog: Save as Mesh… chosen, its dialog given the keyboard.
save_dialog() {
  ui_mark
  ui_step "choose Save as Mesh..."         ui_menu_choose "Save as Mesh…"
  ui_expect_new "Save As Mesh dialog opened: " "the dialog opened"
  ui_focus_dialog '^Save as Mesh$'
}

# accept_file TITLE: the file dialog's proposed name (or folder) taken.
accept_file() {
  ui_focus_dialog "$(ui_title_regex "$1")"
  ui_key Return
  ui_focus_main
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Root/Bodies/Plate at" "the browser lists Plate"

echo "--- 1. A body on its print bed face, as 3MF"
row_menu "Browser Root/Bodies/Plate"
last_entry "Save as Mesh…"
save_dialog
ui_expect_new "Save As Mesh dialog opened: bodies Plate; print bed face available" "Plate with its bed face offered"
ui_mark
ui_step "on its print bed face (Alt+B)"    ui_key alt+b
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Save As Mesh dialog: 3mf, mm, one file, medium, on bed" "3MF, millimetres, on the bed"
ui_step "the proposed name (Enter)"        accept_file "Save Mesh"
ui_expect_new "Saved as mesh $WORK/Plate.3mf: 3mf, mm, one file, medium, on bed, 1 body(ies)" \
  "written under the body's name"
ui_expect_new "Save As Mesh bed Plate.3mf: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | 0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000" \
  "turned over onto its top, centred on the plate"
unzip -p "$WORK/Plate.3mf" 3D/3dmodel.model | grep -qF '<model unit="millimeter"' || ui_fail "the 3MF file is not in millimetres"
echo "ok   the 3MF file is in millimetres"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 2. A component as text STL in inches"
ui_expect_log "Browser Root/Lid:1 at" "the browser lists Lid:1"
row_menu "Browser Root/Lid:1"
last_entry "Save as Mesh…"
save_dialog
ui_expect_new "Save As Mesh dialog opened: bodies Lid; print bed face unavailable (No body has a print bed face)" \
  "the component's body, no print bed face"
ui_mark
ui_step "STL (ASCII) (Alt+F, Down Down)"   ui_key alt+f Down Down
ui_step "inches (Alt+U, Down x3)"          ui_key alt+u Down Down Down
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Save As Mesh dialog: stl ascii, in, one file, medium" "text STL in inches"
ui_step "the proposed name (Enter)"        accept_file "Save Mesh"
ui_expect_new "Saved as mesh $WORK/Lid.stl: stl ascii, in, one file, medium, 1 body(ies)" \
  "written under the component's name"
max_x=$(awk '$1 == "vertex" { if (!seen || $2 > m) m = $2; seen = 1 } END { printf "%.4f", m }' "$WORK/Lid.stl")
[ "$max_x" = "3.1496" ] || ui_fail "Lid's largest x in the file is $max_x, expected 80 mm in inches (3.1496)"
echo "ok   the numbers are in inches"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 3. The whole design, OBJ, a file per body, a longest edge"
row_menu "Browser Root"
last_entry "Save as Mesh…"
save_dialog
ui_expect_new "Save As Mesh dialog opened: bodies Plate, Lid; left out Skin" "both solids, the surface body left out"
ui_mark
ui_step "OBJ (Alt+F, Down)"                ui_key alt+f Down
ui_step "millimetres (Alt+U, Up x3)"       ui_key alt+u Up Up Up
ui_step "a file per body (Alt+S, Down)"    ui_key alt+s Down
ui_step "Custom (Alt+R, Down Down)"        ui_key alt+r Down Down
ui_step "longest edge 1 mm (Alt+E, Up)"    ui_key alt+e Up
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Save As Mesh dialog: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm" \
  "OBJ, a file per body, a 1 mm longest edge"
ui_step "the folder"                       ui_type_path "Save Meshes in Folder" "$WORK/meshes"
ui_focus_main
ui_expect_new "Saved as mesh $WORK/meshes/Plate.obj: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm, 1 body(ies)" \
  "Plate's file named after it" 30
ui_expect_new "Saved as mesh $WORK/meshes/Lid.obj: obj, mm, a file per body, custom 0.03 mm 15 deg 1 mm, 1 body(ies)" \
  "Lid's file named after it" 30
faces=$(grep -c '^f ' "$WORK/meshes/Plate.obj")
[ "$faces" -gt 1000 ] || ui_fail "Plate.obj has $faces triangles: its edges were not split to 1 mm"
echo "ok   Plate's mesh split to the longest edge ($faces triangles)"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 4. A surface body: disabled with the reason"
row_menu "Browser Root/Bodies/Skin"
ui_expect_new "Save As Mesh disabled: Surface and empty bodies are not saved as meshes" "disabled, the reason given"
last_entry "Save as Mesh…"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 5. The view's context menu"
ui_focus_main
ui_mark
ui_step "offset face (search)"             ui_command "Offset Face"
ui_expect_new "Pick places:" "where the faces are picked"
ui_step "cancel it (Esc)"                  ui_key Escape
at=$(ui_pick_at "face F1.b0/F1:top at")
ui_mark
ui_step "right-click Plate's top"          xdotool mousemove $at click 3
ui_expect_new "Selected: 1 face [face F1:top of F1.b0" "the top selected"
ui_expect_new "Context menu: " "the context menu opened"
has "Save as Mesh…"
save_dialog
ui_expect_new "Save As Mesh dialog opened: bodies Plate; print bed face available" "the face stands for its body"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Save As Mesh dialog cancelled" "nothing written"

echo "--- 6. File > Export remembers the type and the refinement, names the body (mitcad#1164)"
# export_dialog: File > Export (Alt+F, E) with what is selected.
export_dialog() {
  ui_focus_main
  ui_key alt+f
  ui_sync
  ui_key e
}
ui_focus_main
ui_step "Escape (clear the selection)"     ui_key Escape
ui_mark
ui_step "click Plate's row"                ui_click_logged "Browser Root/Bodies/Plate"
ui_expect_new "Browser selected: Root/Bodies/Plate" "Plate selected"
ui_mark
ui_step "File > Export"                    export_dialog
ui_expect_new ", type step-ap214, refinement medium, file $WORK/meshes/Plate.step" \
  "STEP at first, the selected body's name in the last folder"
ui_focus_dialog '^Export$'
ui_mark
ui_step "3MF (Alt+T, Down x7)"             ui_key alt+t Down Down Down Down Down Down Down
ui_step "High (Alt+R, Down)"               ui_key alt+r Down
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Export dialog: 3MF for 3D printing (*.3mf) to $WORK/meshes/Plate.3mf, refinement high" \
  "3MF with refinement High, the name's extension follows the type"
ui_expect_new "Exported $WORK/meshes/Plate.3mf: 3mf, 1 body" "Plate written as 3MF" 30
[ -s "$WORK/meshes/Plate.3mf" ] || ui_fail "no Plate.3mf"
ui_expect_log "Browser Root/Lid:1/Bodies/Lid at" "the browser lists Lid's body in Lid:1"
ui_mark
ui_step "click Lid's row"                  ui_click_logged "Browser Root/Lid:1/Bodies/Lid"
ui_expect_new "Browser selected: Root/Lid:1/Bodies/Lid" "Lid selected"
ui_mark
ui_step "File > Export again"              export_dialog
ui_expect_new ", type 3mf, refinement high, file $WORK/meshes/Lid.3mf" \
  "the next export starts with 3MF, High and the selected body's name"
ui_focus_dialog '^Export$'
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Export dialog: 3MF for 3D printing (*.3mf) to $WORK/meshes/Lid.3mf, refinement high" \
  "taken as proposed"
ui_expect_new "Exported $WORK/meshes/Lid.3mf: 3mf, 1 body" "Lid written as 3MF" 30
[ -s "$WORK/meshes/Lid.3mf" ] || ui_fail "no Lid.3mf"
echo "ok   both files written"

ui_finish "UI save as mesh"
