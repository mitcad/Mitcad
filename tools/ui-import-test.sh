#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/files app/projects app/web third_party/three
# check-all sources: core/model/src/document/repeated_parts.rs geometry/src/repeated_parts.cpp
# Files (U6) through the File menu, on the demo block (60 x 40 x 20 mm):
#   - STEP, STL and DXF round trips: File > Export writes the block (its
#     sketch for DXF), File > Import brings the file back: a base feature
#     of the same volume, a mesh body, a sketch with the rectangle's profile.
#   - Export > Web page (mitcad#423): one .html file with the design's data,
#     its size in the log.
#   - USDZ export (mitcad#123): the Export dialog takes the type from the
#     extension; the package holds the binary layer first, 64-byte aligned.
#   - Insert DXF's placement and layers (P9): a layered drawing placed by
#     its lower left corner, one layer left out.
#   - --open of a STEP and a DXF file: new documents; Open Recent.
#   - Insert Component of a saved project, linked.
#   - FreeCAD documents through File > Open: a stored shape in a part and a
#     sketch, then a PartDesign Body whose pad is replayed and whose helix
#     comes in as its stored shape; the import report's counts and text
#     (mitcad#22), with the parameters made of a spreadsheet and the
#     expressions translated or kept as FreeCAD's values (mitcad#4).
#   - An .ipt part through File > Open (mitcad#60): Mitcad's own test part
#     (a cube and a cylinder, inches, aluminium; test_exchange --write-ipt)
#     imported in the import process, its report shown: the bodies as base
#     features, the part's material and units. With MITCAD_IPT_CORPUS (real
#     part files, never committed) the smallest of them too, its features
#     replayed with the history and reported.
#   - An .iam assembly through File > Open (mitcad#60, stage 4): the test
#     project of test_exchange --write-iam (a sub-assembly, parts found
#     relative to the assembly and by name, a missing one, a suppressed
#     occurrence) imported in the import process, its report shown.
#   - An IFC building model through File > Open (mitcad#273): the IFC
#     reader's test building (mitcad-cli import-ifc --write-test), every
#     product accounted for in the report. With MITCAD_IFC_CORPUS (never
#     committed) the MIT-licensed project's IFC4 export too, none of its
#     products failed.
#   - A SolidCraft design through File > Open (mitcad#450): one of the
#     import's hand-written test designs, its features in with their
#     parameters, its report shown; the same design saved as .sc
#     (mitcad#1622) likewise, and a .sc file of another program refused
#     by its format key ("Not a SolidCraft design: ...").
#   - A DWG drawing through File > Open (mitcad#453): the reader's test
#     drawing (mitcad-cli import-dwg --write-test), its entities accounted
#     for, its blocks as components.
#   - With MITCAD_F3D_CORPUS (real .f3d files, never committed): a small
#     .f3d (MITCAD_F3D_UI_PART, else the corpus' smallest part) opened
#     through File > Open, imported with its history to its end behind a
#     progress dialog, its report shown; the view shows the sketches the
#     model shows (mitcad#6, #7: a sketch a feature uses is hidden unless
#     the file shows it), as mitcad-cli reads the part saved. Then, each
#     definition the import tries made slower (MITCAD_TEST_RECOMPUTE_DELAY_MS),
#     the same part stopped (Stop and Keep What Is Imported, T1e): the
#     items before the stop stay, the rest come in as the file's bodies,
#     and the report says where it stopped; and once more cancelled (Esc):
#     the open document stays as it was.
#   - Export where the design shows the bodies (mitcad#19): a STEP file of
#     a pin in a moved and turned occurrence, read back where the view
#     shows it, and with the Coordinates row set to the component's where
#     its component has it.
#   - A STEP assembly through File > Import (mitcad#1274): the Import STEP
#     Assembly question, Enter keeps the assembly, the part placed twice
#     one component with two occurrences; one undo step.
#   - Import into a design with content (mitcad#1629): Where to Import,
#     Enter puts the file into a new component named after it (the
#     Browser's row block:1, pins:1), Alt+A into the active component.
#   - Saving in a project (P12a): version 3, the B-rep data in the
#     project's store, opened again with the same body.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-test.sh
# check-all sources: core/import core/f3d core/freecad core/ipt core/zip core/dxf core/tests tools/cli core/usdz
# check-all sources: core/ffi/src/brep_import.rs core/ffi/src/f3d_import.rs core/ffi/src/fcstd_import.rs
# check-all sources: core/ffi/src/ipt_import.rs core/ffi/src/ipt_history.rs core/ffi/src/iam_import.rs
# check-all sources: core/ifc core/ffi/src/ifc_import.rs
# check-all sources: core/solidcraft core/ffi/src/solidcraft_import.rs
# check-all sources: core/dwg core/ffi/src/dwg_import.rs
# check-all sources: core/dump core/asm core/f3d-names core/f3d-file core/f3d-design core/f3d-decode
# check-all sources: core/import-base core/import-ifc core/import-fcstd
# check-all sources: core/import-solidcraft core/import-replay core/import-joints core/import-sheet core/import-primitives core/import-appearances

source "$(dirname "$0")/ui-test-lib.sh"
# New and Open replace the design, as these steps expect (mitcad#1172).
ui_open_in_current_window

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
TEST_EXCHANGE=${UI_TEST_EXCHANGE:-$(cd "$(dirname "$UI_APP")/.." && pwd)/core/test_exchange}
WORK=$(mktemp -d /tmp/mitcad-ui-import.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# file_menu key: the File menu (Alt+F), then an entry's underlined letter.
file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}

# type_into_dialog title path: a path typed into a dialog's focused field
# (ui_type_path), then Enter; no path: only Enter.
type_into_dialog() {
  if [ -z "$2" ]; then
    ui_focus_dialog "$(ui_title_regex "$1")"
    ui_key Return
  else
    ui_type_path "$1" "$2"
  fi
}

export_to() {
  file_menu e
  type_into_dialog 'Export' "$1"
  ui_focus_main
}

import_from() {
  file_menu i
  type_into_dialog 'Import' "$1"
}

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(ui_window_title)
    [ "$title" = "$1" ] && { echo "ok   title '$1'"; return; }
    sleep 0.2
  done
  ui_fail "window title is '$title', expected '$1'"
}

ui_start_display
ui_start_app --demo --no-native-dialogs
ui_step "fit (F6)"                         ui_key F6

echo "--- STEP"
ui_mark
ui_step "File > Export a STEP file"        export_to "$WORK/block.step"
ui_expect_new "Exported $WORK/block.step: step, 1 body(ies)" "the block written as STEP"
[ -s "$WORK/block.step" ] || ui_fail "no STEP file"
ui_mark
ui_step "File > Import it"                 import_from "$WORK/block.step"
# The demo has content: Import asks where the file goes, and Enter makes a
# component named after the file (mitcad#1629).
ui_expect_new "Import place dialog opened: block.step" "Import asks where the file goes"
ui_focus_dialog '^Where to Import$'
ui_step "into a new component (Enter)"     ui_key Return
ui_focus_main
ui_expect_new "Import place: new component" "a new component chosen"
ui_expect_new "Imported block.step as" "the STEP came in as a base feature"
ui_expect_new ": 1 body(ies), into component block" "in the component block"
ui_expect_volume "New body .*(F3\.b0): volume \([0-9.]*\) mm3" 48000 "the STEP body's volume"
ui_browser_reveal "Browser Root/block:1"
ui_expect_new "Browser Root/block:1 at" "the browser lists the component block, named after the file"
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Import block.step" "the import, its component with it, is one undo step"

echo "--- STL"
ui_mark
ui_step "File > Export an STL file"        export_to "$WORK/block.stl"
ui_expect_new "Exported $WORK/block.stl: stl, 1 body(ies)" "the block written as STL"
ui_mark
ui_step "File > Import it"                 import_from "$WORK/block.stl"
ui_step "in millimetres (Enter)"           type_into_dialog 'Insert Mesh' ""
ui_focus_main
ui_expect_new "Imported block.stl as" "the STL came in"
ui_expect_volume "New body .*(F[0-9]*\.b0): volume \([0-9.]*\) mm3" 48000 "the mesh body's volume" 1e-4
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Import block.stl" "undone"

echo "--- USDZ (mitcad#123)"
ui_mark
ui_step "File > Export a USDZ file"        export_to "$WORK/block.usdz"
ui_expect_new "Export dialog: USDZ for AR and 3D viewers (*.usdz) to $WORK/block.usdz" \
  "the extension chose the USDZ type"
ui_expect_new "Exported $WORK/block.usdz: usdz, 1 body(ies), 1 material(s)" "the block written as USDZ"
# A package: a zip whose first file, the binary layer, starts 64-byte
# aligned (the stored name block.usdc at 30, the padding, PXR-USDC).
[ "$(head -c 2 "$WORK/block.usdz")" = 'PK' ] || ui_fail "no zip package"
[ "$(dd if="$WORK/block.usdz" bs=1 skip=30 count=10 2> /dev/null)" = 'block.usdc' ] || ui_fail "no layer first"
[ "$(dd if="$WORK/block.usdz" bs=1 skip=64 count=8 2> /dev/null)" = 'PXR-USDC' ] || ui_fail "no aligned layer"
echo "ok   the package's layer at 64"

echo "--- Web page (mitcad#423)"
ui_mark
ui_step "File > Export a web page"         export_to "$WORK/block.html"
ui_expect_new "Export dialog: Web page (*.html) to $WORK/block.html" "the extension chose the web page type"
ui_expect_new "Exported $WORK/block.html: html, 1 body(ies), " "the block written as a web page"
[ -s "$WORK/block.html" ] || ui_fail "no web page"
ui_expect_new "Exported $WORK/block.html: html, 1 body(ies), $(stat -c %s "$WORK/block.html") bytes" \
  "the log says the page's size"
grep -qF 'id="mitcad-data"' "$WORK/block.html" || ui_fail "no design data in the page"
if grep -qF '<!--MITCAD:' "$WORK/block.html"; then ui_fail "a placeholder left in the page"; fi
echo "ok   the page holds the design's data"

echo "--- DXF"
ui_mark
ui_step "File > Export the sketch to DXF"  export_to "$WORK/sketch.dxf"
ui_expect_new "Exported $WORK/sketch.dxf: dxf, 4 entities, in mm, construction geometry left out" \
  "the rectangle written as DXF, without construction geometry (mitcad#1445)"
ui_mark
ui_step "File > Import it"                 import_from "$WORK/sketch.dxf"
ui_step "on XY, in mm (Enter)"             type_into_dialog 'Insert DXF' ""
ui_focus_main
ui_expect_new "Inserted sketch.dxf into Sketch2: 4 curves, 4 points, 0 texts, 1 profiles, 0 warnings" \
  "the rectangle in a new sketch, a profile again"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
type_into_dialog 'Save As' "$WORK/block.mitcad"
ui_focus_main
ui_expect_log "Saved $WORK/block.mitcad" "saved"

echo "--- DXF placement and layers (P9)"
# A 60 x 40 rectangle on Outline, a circle on the frozen layer Holes, a
# text on Notes; millimetres.
tr ' ' '\n' > "$WORK/plate.dxf" << 'EOF'
0 SECTION 2 HEADER 9 $INSUNITS 70 4 0 ENDSEC 0 SECTION 2 TABLES 0 TABLE 2 LAYER 70 2
0 LAYER 2 Outline 70 0 62 7 6 CONTINUOUS 0 LAYER 2 Holes 70 1 62 -1 6 CONTINUOUS 0 ENDTAB 0 ENDSEC
0 SECTION 2 ENTITIES
0 LINE 8 Outline 10 0 20 0 11 60 21 0 0 LINE 8 Outline 10 60 20 0 11 60 21 40
0 LINE 8 Outline 10 60 20 40 11 0 21 40 0 LINE 8 Outline 10 0 20 40 11 0 21 0
0 CIRCLE 8 Holes 10 30 20 20 40 5 0 TEXT 8 Notes 10 0 20 -5 40 2 1 PLATE
0 ENDSEC 0 EOF
EOF
ui_mark
ui_step "File > Import a layered DXF"      import_from "$WORK/plate.dxf"
ui_expect_new "Insert DXF drawing: unit mm, layers Outline (4) | Holes (1) off | Notes (1)" \
  "its layers are offered, the frozen one unchecked"
ui_expect_new "Insert DXF OK at" "the dialog is laid out"
ui_focus_dialog '^Insert DXF$'
ui_step "place its lower left corner"      ui_click_logged "Insert DXF place lower_left"
ui_step "at X 10"                          ui_type_in "Insert DXF x" "10"
ui_step "at Y 5"                           ui_type_in "Insert DXF y" "5"
ui_step "select Notes"                     ui_click_logged "Insert DXF layer Notes"
ui_step "uncheck it (Space)"               ui_key space
ui_step "OK"                               ui_click_logged "Insert DXF OK"
ui_focus_main
ui_expect_new "Insert DXF: lower left corner at (10, 5), layers Outline" "placed by its corner, one layer"
ui_expect_new "Inserted plate.dxf into Sketch3: 4 curves, 4 points, 0 texts, 1 profiles" \
  "only the outline came in"
ui_step "undo the insert (Ctrl+Z)"         ui_key ctrl+z
ui_expect_new "Undo: Insert plate.dxf" "one undo step; the document is as saved"

echo "--- Insert Component"
ui_mark
ui_step "new design (Ctrl+N)"              ui_key ctrl+n
ui_expect_new "New document" "an empty document"
# The component goes into Component1, which takes components (mitcad#421):
# a new design starts with its Component1 active (mitcad#898), unlike an
# import (mitcad#426).
ui_expect_new "Opened the new design with Component1 active" "Component1 is active"
ui_mark
ui_step "Insert Component (search)"        ui_command "Insert Component"
type_into_dialog 'Insert Component' "$WORK/block.mitcad"
ui_focus_dialog '^Insert Component$'
ui_step "linked (Enter)"                   ui_key Return
ui_focus_main
ui_expect_new "Inserted component" "the saved block placed as a component"
ui_expect_new "(linked) from block.mitcad" "linked to its file"
ui_stop_app

echo "--- --open of other files, Open Recent"
ui_start_app --open "$WORK/block.step" --no-native-dialogs
ui_expect_log "Imported block.step as" "a STEP opened as a new document"
expect_title "block* - MitCad"
ui_stop_app
ui_start_app --open "$WORK/sketch.dxf" --no-native-dialogs
ui_expect_log "Inserted sketch.dxf into Sketch1: 4 curves" "a DXF opened as a sketch of a new document"
ui_mark
ui_step "File > Open Recent"               file_menu r
ui_expect_new "Recent files: sketch.dxf | block.step | block.mitcad" "the files opened and saved last"
ui_step "the second (Down, Enter)"         ui_key Down Return
ui_focus_dialog '^MitCad$'
ui_step "don't save (D)"                   ui_key d
ui_focus_main
ui_expect_new "Imported block.step as" "block.step opened again"

echo "--- FreeCAD document"
# A document in FreeCAD's form made here: the block (exported as B-rep) as
# a Part::Feature in an App::Part 100 mm up, and a sketch of a circle,
# packed with cmake -E tar; opened through File > Open in the import
# process.
mkdir -p "$WORK/fc"
ui_mark
ui_step "File > Export the block as B-rep" export_to "$WORK/fc/PartShape.brp"
ui_expect_new "Exported $WORK/fc/PartShape.brp: brep, 1 body(ies)" "the block written as B-rep"
cat > "$WORK/fc/Document.xml" << 'EOF'
<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion="4" ProgramVersion="0.21R33771 (Git)" FileVersion="1">
  <Properties Count="0"></Properties>
  <Objects Count="3" Dependencies="1">
    <Object type="App::Part" name="Holder" id="1"/>
    <Object type="Part::Feature" name="Block" id="2"/>
    <Object type="Sketcher::SketchObject" name="Sketch" id="3"/>
  </Objects>
  <ObjectData Count="3">
    <Object name="Holder"><Properties Count="2">
      <Property name="Group" type="App::PropertyLinkList"><LinkList count="1"><Link value="Block"/></LinkList></Property>
      <Property name="Placement" type="App::PropertyPlacement"><PropertyPlacement Px="0" Py="0" Pz="100" Q0="0" Q1="0" Q2="0" Q3="1" A="0" Ox="0" Oy="0" Oz="1"/></Property>
    </Properties></Object>
    <Object name="Block"><Properties Count="1">
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="PartShape.brp"/></Property>
    </Properties></Object>
    <Object name="Sketch"><Properties Count="1">
      <Property name="Geometry" type="Part::PropertyGeometryList"><GeometryList count="1">
        <Geometry type="Part::GeomCircle"><Circle CenterX="0" CenterY="0" CenterZ="0" NormalX="0" NormalY="0" NormalZ="1" AngleXU="0" Radius="5"/></Geometry>
      </GeometryList></Property>
    </Properties></Object>
  </ObjectData>
</Document>
EOF
(cd "$WORK/fc" && cmake -E tar cf "$WORK/block.FCStd" --format=zip Document.xml PartShape.brp) ||
  ui_fail "cannot pack the FreeCAD document"
# open_fcstd file: File > Open of a FreeCAD document, the changes not saved.
open_fcstd() {
  ui_key ctrl+o
  ui_focus_dialog '^MitCad$'
  ui_key d
  type_into_dialog 'Open' "$1"
}
ui_mark
ui_step "open it (Ctrl+O)"                 open_fcstd "$WORK/block.FCStd"
ui_expect_new "Import of block.FCStd started" "the import runs in its own process"
ui_expect_new "Import report: block.FCStd: 3 items (1 body, 1 component, 0 occurrence, 1 parametric, 0 partial, 0 fallback, 0 included, 0 skipped), 1 bodies" \
  "the block came in as a body in a component, the sketch (a circle) as a sketch" 120
ui_expect_new "Import report summary: block.FCStd (FreeCAD 0.21R33771 (Git)) 3 objects: 1 body, 1 component, 0 occurrence, 1 parametric, 0 partial, 0 fallback, 0 included, 0 skipped. 1 bodies, the shapes FreeCAD stored: the document has no feature history to replay." \
  "the report: no history, the stored shape"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported block.FCStd: 1 bodies" "a new document with the body"
expect_title "block* - MitCad"

# The history (mitcad#22): a PartDesign Body (Plate) whose sketch of a 30 x
# 20 rectangle is padded 10 mm, then a helix without a profile, which
# Mitcad cannot build, whose stored shape is the box 12 mm high. The pad is
# replayed and checked against its stored shape; the helix comes in as its
# stored shape. A spreadsheet's cells (Length = 2 * 5 mm, Ratio = 1.2)
# drive the pad's length (translated) and the helix's height (kept as
# FreeCAD's value: the helix is not replayed); the report sums the
# parameters up and lists the expression kept with its reason.
mkdir -p "$WORK/fc-history"
cat > "$WORK/fc-history/shapes.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 30, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 30, "width": 20, "height": 12, "operation": "new_body"}},
  {"cmd": "export", "path": "Pad.brp", "bodies": ["F1.b0"]},
  {"cmd": "export", "path": "Helix.brp", "bodies": ["F2.b0"]}
]
EOF
(cd "$WORK/fc-history" && "$CLI" run shapes.json) > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "the stored shapes of the history"; }
cat > "$WORK/fc-history/Document.xml" << 'EOF'
<?xml version='1.0' encoding='utf-8'?>
<Document SchemaVersion="4" ProgramVersion="1.0R39319 (Git)" FileVersion="1">
  <Properties Count="0"></Properties>
  <Objects Count="5" Dependencies="1">
    <Object type="PartDesign::Body" name="Body" id="1"/>
    <Object type="Sketcher::SketchObject" name="Sketch" id="2"/>
    <Object type="PartDesign::Pad" name="Pad" id="3"/>
    <Object type="PartDesign::AdditiveHelix" name="Helix" id="4"/>
    <Object type="Spreadsheet::Sheet" name="Params" id="5"/>
  </Objects>
  <ObjectData Count="5">
    <Object name="Body"><Properties Count="4">
      <Property name="Label" type="App::PropertyString"><String value="Plate"/></Property>
      <Property name="Group" type="App::PropertyLinkList"><LinkList count="3"><Link value="Sketch"/><Link value="Pad"/><Link value="Helix"/></LinkList></Property>
      <Property name="Tip" type="App::PropertyLink"><Link value="Helix"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Helix.brp"/></Property>
    </Properties></Object>
    <Object name="Sketch"><Properties Count="2">
      <Property name="Geometry" type="Part::PropertyGeometryList"><GeometryList count="4">
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="0" StartY="0" StartZ="0" EndX="30" EndY="0" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="30" StartY="0" StartZ="0" EndX="30" EndY="20" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="30" StartY="20" StartZ="0" EndX="0" EndY="20" EndZ="0"/><Construction value="0"/></Geometry>
        <Geometry type="Part::GeomLineSegment"><LineSegment StartX="0" StartY="20" StartZ="0" EndX="0" EndY="0" EndZ="0"/><Construction value="0"/></Geometry>
      </GeometryList></Property>
      <Property name="Constraints" type="Sketcher::PropertyConstraintList"><ConstraintList count="4">
        <Constrain Name="" Type="1" Value="0" First="0" FirstPos="2" Second="1" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="1" FirstPos="2" Second="2" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="2" FirstPos="2" Second="3" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
        <Constrain Name="" Type="1" Value="0" First="3" FirstPos="2" Second="0" SecondPos="1" Third="-2000" ThirdPos="0" IsDriving="1" IsActive="1"/>
      </ConstraintList></Property>
    </Properties></Object>
    <Object name="Pad"><Properties Count="5">
      <Property name="Profile" type="App::PropertyLinkSub"><LinkSub value="Sketch" count="0"></LinkSub></Property>
      <Property name="Length" type="App::PropertyLength"><Float value="10"/></Property>
      <Property name="Type" type="App::PropertyEnumeration"><Integer value="0"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Pad.brp"/></Property>
      <Property name="ExpressionEngine" type="App::PropertyExpressionEngine"><ExpressionEngine count="1"><Expression path="Length" expression="Params.Length"/></ExpressionEngine></Property>
    </Properties></Object>
    <Object name="Helix"><Properties Count="4">
      <Property name="BaseFeature" type="App::PropertyLink"><Link value="Pad"/></Property>
      <Property name="Height" type="App::PropertyLength"><Float value="12"/></Property>
      <Property name="Shape" type="Part::PropertyPartShape"><Part file="Helix.brp"/></Property>
      <Property name="ExpressionEngine" type="App::PropertyExpressionEngine"><ExpressionEngine count="1"><Expression path="Height" expression="Params.Length * Params.Ratio"/></ExpressionEngine></Property>
    </Properties></Object>
    <Object name="Params"><Properties Count="1">
      <Property name="cells" type="Spreadsheet::PropertySheet"><Cells Count="2"><Cell address="B1" content="=2 * 5 mm" alias="Length"/><Cell address="B2" content="1.2" alias="Ratio"/></Cells></Property>
    </Properties></Object>
  </ObjectData>
</Document>
EOF
(cd "$WORK/fc-history" && cmake -E tar cf "$WORK/history.FCStd" --format=zip Document.xml Pad.brp Helix.brp) ||
  ui_fail "cannot pack the FreeCAD document with a history"
ui_mark
ui_step "open the history (Ctrl+O)"        open_fcstd "$WORK/history.FCStd"
ui_expect_new "Import report: history.FCStd: 5 items (1 body, 0 component, 0 occurrence, 2 parametric, 0 partial, 1 fallback, 0 included, 1 skipped), 1 bodies" \
  "the Body, its sketch and pad parametric, the helix a fallback, the spreadsheet no body" 120
ui_expect_new "Import report summary: history.FCStd (FreeCAD 1.0R39319 (Git)) 5 objects: 1 body, 0 component, 0 occurrence, 2 parametric, 0 partial, 1 fallback, 0 included, 1 skipped. 1 bodies; of the history's 2 features, 1 were replayed as MitCad features (checked against the shapes FreeCAD stored), 1 came in as their stored shapes (fallback) and 0 were skipped. 2 parameters (1 of FreeCAD's expressions, 1 of its values); of the document's 2 expressions, 1 were translated, 1 kept FreeCAD's value and 0 drive nothing the import carries over." \
  "the report: the history replayed, one fallback; the parameters and expressions"
ui_expect_new "Import report kept: Helix.Height = Params.Length * Params.Ratio: Helix was not replayed" \
  "the expression kept as FreeCAD's value, with the reason"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported history.FCStd: 1 bodies" "a new document with the Body's body"
expect_title "history* - MitCad"

echo "--- .ipt part (mitcad#60)"
"$TEST_EXCHANGE" --write-ipt "$WORK/part.ipt" || ui_fail "cannot write the test part"
ui_mark
ui_step "open the part (Ctrl+O)"           open_fcstd "$WORK/part.ipt"
ui_expect_new "Import of part.ipt started" "the import runs in its own process"
ui_expect_new "Import report: part.ipt: 2 bodies (2 solids, 2 valid, 0 sheets, 0 not built)" \
  "the cube and the cylinder came in as solids" 120
ui_expect_new "Import report summary: part.ipt (part number MITCAD-TEST-1) 2 bodies stored in the file, each a base feature: 2 solids, 2 valid, 0 sheets, 0 not built. Material: Aluminum (MitCad's aluminum). Units: in. Saved by release Mitcad test writer." \
  "the report: the bodies, the part's material and units"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported part.ipt: 2 bodies" "a new document with the part's bodies"
expect_title "part* - MitCad"
if [ -n "${MITCAD_IPT_CORPUS:-}" ]; then
  REAL_IPT=$(IFS=:; find $MITCAD_IPT_CORPUS -iname '*.ipt' -printf '%s %p\n' 2> /dev/null | sort -n | head -1 | cut -d' ' -f2-)
fi
if [ -n "${REAL_IPT:-}" ]; then
  REAL_NAME=$(basename "$REAL_IPT")
  ui_mark
  ui_step "open a real part (Ctrl+O)"      open_fcstd "$REAL_IPT"
  ui_expect_new "Import report: $REAL_NAME: " "the real part's report" 300
  report=$(grep "Import report: $REAL_NAME: " "$UI_LOG" | tail -1)
  grep -qE ": ([0-9]+) bodies \(([0-9]+) solids, [0-9]+ valid, [0-9]+ sheets, 0 not built\)$" <<< "$report" ||
    ui_fail "the real part's bodies did not all come in: $report"
  echo "ok   ${report#*: }"
  # Its features replayed with the history (mitcad#60, stage 3).
  features=$(grep "Import report features: $REAL_NAME: " "$UI_LOG" | tail -1)
  grep -qE ": [0-9]+ parametric, [0-9]+ partial, [0-9]+ fallback, [0-9]+ skipped$" <<< "$features" ||
    ui_fail "the real part's features were not reported: $features"
  echo "ok   ${features#*: }"
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"       ui_key Return
  ui_focus_main
  ui_expect_new "Imported $REAL_NAME:" "a new document with the real part's bodies"
else
  echo "skip a real .ipt part: MITCAD_IPT_CORPUS is not set"
fi

echo "--- .iam assembly (mitcad#60, stage 4)"
"$TEST_EXCHANGE" --write-iam "$WORK/iam" || ui_fail "cannot write the test assemblies"
ui_mark
ui_step "open the assembly (Ctrl+O)"       open_fcstd "$WORK/iam/top.iam"
ui_expect_new "Import Assembly dialog opened: top.iam, parts with their design history on" \
  "asked how the parts come in, with their history by default (mitcad#507)"
ui_focus_dialog '^Import Assembly$'
ui_step "import (Enter)"                   ui_key Return
ui_expect_new "Import of top.iam started" "the import runs in its own process"
ui_expect_new "Import report: top.iam: 7 occurrences placed, 1 suppressed, 3 part files imported, 1 missing, 0 failed" \
  "the occurrences placed, the parts imported, the missing one reported" 120
ui_expect_new "Import report summary: top.iam (assembly) 7 occurrences placed, 1 suppressed, 3 part files imported, 1 missing, 0 failed; 1 sub-assemblies. Files found: 1 by name, 1 missing, 3 relative to the assembly. Checked against the file: 7 of 7 placements agree with the displayed ones, 5 of 5 parts' bodies lie within their range boxes." \
  "the report: how the files were found and the checks against the file"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported top.iam: 5 bodies" "a new document with the parts' bodies in their components"
expect_title "top* - MitCad"

echo "--- SolidCraft design (mitcad#450)"
cp "$(dirname "$0")/../core/solidcraft/tests/designs/box-fillet-hole.solidcraft.json" "$WORK/box-fillet-hole.solidcraft" ||
  ui_fail "cannot copy the test design"
ui_mark
ui_step "open the design (Ctrl+O)"         open_fcstd "$WORK/box-fillet-hole.solidcraft"
ui_expect_new "Import of box-fillet-hole.solidcraft started" "the import runs in its own process"
ui_expect_new "Import report: box-fillet-hole.solidcraft: 5 parametric, 0 partial, 0 unmapped, 0 failed, 0 skipped" \
  "every feature came in with its parameters" 120
ui_expect_new "Import report summary: box-fillet-hole.solidcraft (SolidCraft design Box fillet hole, file format 27) 5 timeline features: 5 parametric, 0 partial, 0 unmapped, 0 failed, 0 skipped. 1 bodies, 2 sketches," \
  "the report: the features' outcomes, the bodies and sketches"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported box-fillet-hole.solidcraft: 1 bodies" "a new document with the block"
expect_title "box-fillet-hole* - MitCad"
# The same design saved as .sc, as SolidCraft saves designs now
# (mitcad#1622): recognised by its format key, it comes in the same.
cp "$WORK/box-fillet-hole.solidcraft" "$WORK/box-fillet-hole-copy.sc" || ui_fail "cannot copy the test design"
ui_mark
ui_step "open the design saved as .sc (Ctrl+O)" open_fcstd "$WORK/box-fillet-hole-copy.sc"
ui_expect_new "Import of box-fillet-hole-copy.sc started" "a .sc design is imported as SolidCraft's"
ui_expect_new "Import report: box-fillet-hole-copy.sc: 5 parametric, 0 partial, 0 unmapped, 0 failed, 0 skipped" \
  "every feature came in, as from the .solidcraft file" 120
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported box-fillet-hole-copy.sc: 1 bodies" "a new document with the block"
expect_title "box-fillet-hole-copy* - MitCad"

echo "--- IFC building model (mitcad#273)"
"$CLI" import-ifc --write-test "$WORK/building.ifc" || ui_fail "cannot write the test building"
ui_mark
ui_step "open the building (Ctrl+O)"       open_fcstd "$WORK/building.ifc"
ui_expect_new "Import of building.ifc started" "the import runs in its own process"
ui_expect_new "Import report: building.ifc: 16 products (1 axes, 4 component, 1 cut, 1 mesh, 2 not_imported, 2 occurrence, 4 parametric, 1 stored), 7 bodies" \
  "every product accounted for: storeys, the wall with its opening cut, the window type's occurrences" 120
ui_expect_new "Import report summary: building.ifc (IFC4, view ReferenceView_V1.2, from Mitcad test) 16 products: 1 axes, 4 component, 1 cut, 1 mesh, 2 not_imported, 2 occurrence, 4 parametric, 1 stored. 7 bodies, 16 features, 1 type shapes as components; 2 storey planes, 2 grid axes. Checked against the file: 1 of 1 element volumes agree with its quantities." \
  "the report: the products' outcomes and the check against the file's quantities"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported building.ifc: 7 bodies" "a new document with the storeys' bodies"
expect_title "building* - MitCad"
# The MIT-licensed project's IFC4 export (content id bfdf36ff) from
# MITCAD_IFC_CORPUS (never committed): every product accounted for, none
# failed.
REAL_IFC=""
if [ -n "${MITCAD_IFC_CORPUS:-}" ] && [ -f "$MITCAD_IFC_CORPUS/index.tsv" ]; then
  REAL_IFC=$(awk -F'\t' '$2 ~ /^bfdf36ff/ {print $1}' "$MITCAD_IFC_CORPUS/index.tsv")
fi
if [ -n "$REAL_IFC" ]; then
  REAL_NAME=$(basename "$REAL_IFC")
  ui_mark
  ui_step "open the project bfdf36ff (Ctrl+O)" open_fcstd "$MITCAD_IFC_CORPUS/$REAL_IFC"
  ui_expect_new "Import report: $REAL_NAME: " "the project's report" 900
  report=$(grep "Import report: $REAL_NAME: " "$UI_LOG" | tail -1)
  grep -qE ": [0-9]+ products \(" <<< "$report" || ui_fail "the project's report has no products: $report"
  grep -q " failed" <<< "$report" && ui_fail "a product of the project failed: $report"
  echo "ok   bfdf36ff: ${report#*: }"
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"       ui_key Return
  ui_focus_main
  ui_expect_new "Imported $REAL_NAME:" "a new document with the project's storeys and elements" 900
else
  echo "skip the IFC project: MITCAD_IFC_CORPUS is not set"
fi

echo "--- DWG drawing (mitcad#453)"
"$CLI" import-dwg --write-test "$WORK/truss.dwg" --version R2018 || ui_fail "cannot write the test drawing"
ui_mark
ui_step "open the drawing (Ctrl+O)"        open_fcstd "$WORK/truss.dwg"
ui_expect_new "Import of truss.dwg started" "the import runs in its own process"
ui_expect_new "Import report: truss.dwg: 2 3DFACE, 2 3DSOLID, 5 INSERT, 1 MESH, 1 MINSERT, 1 POLYLINE (polyface mesh)" \
  "every entity accounted for" 120
ui_expect_new "Import report summary: truss.dwg (drawing R2018 (AC1032)) 6 bodies (3 solids, 3 meshes), 2 components." \
  "the report: bodies, components, entities taken and left out"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported truss.dwg: 6 bodies" "a new document with the drawing's bodies"
ui_expect_new "Imported truss.dwg: components " "the blocks as components"
expect_title "truss* - MitCad"

if [ -n "${MITCAD_F3D_CORPUS:-}" ] && [ -d "${MITCAD_F3D_CORPUS:-}" ]; then
  echo "--- .f3d import ($MITCAD_F3D_CORPUS)"
  # A part to import (MITCAD_F3D_UI_PART, else the corpus' smallest part
  # file that is not tiny).
  SMALL=${MITCAD_F3D_UI_PART:-}
  [ -n "$SMALL" ] || SMALL=$(find "$MITCAD_F3D_CORPUS" -name '*.f3d' ! -name '*Assembly*' -size +40k \
    -printf '%s\t%p\n' | sort -n | head -1 | cut -f2)
  NAME=$(basename "$SMALL")
  # open_f3d: File > Open of the part, its changes not saved.
  open_f3d() {
    ui_key ctrl+o
    ui_focus_dialog '^MitCad$'
    ui_key d
    type_into_dialog 'Open' "$SMALL"
  }
  # report_count outcome: how many items came in so, by the last report.
  report_count() {
    grep "Import report: $NAME:" "$UI_LOG" | tail -1 | sed -n "s/.*[(, ]\([0-9][0-9]*\) $1[,)].*/\1/p"
  }
  echo "     importing $NAME"
  ui_mark
  ui_step "open (Ctrl+O)"                  open_f3d
  ui_expect_new "Import of $NAME started" "the import runs in its own process"
  ui_expect_new "Import report: $NAME:" "the import finished with a report" 300
  report=$(grep "Import report: $NAME:" "$UI_LOG" | tail -1)
  bodies=$(sed -n 's/.*), \([0-9]*\) bodies,.*/\1/p' <<< "$report")
  [ "${bodies:-0}" -gt 0 ] || ui_fail "no bodies came in: $report"
  echo "ok   $bodies bodies: ${report#*: }"
  grep -q ", stopped " <<< "$report" && ui_fail "the import did not run to its end: $report"
  echo "ok   no time limit: the import ran to its end"
  grep -q "Import item 1:" "$UI_LOG" || ui_fail "no progress was shown"
  echo "ok   the progress dialog followed the timeline"
  full_fallback=$(report_count fallback)
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"       ui_key Return
  ui_focus_main
  expect_title "${NAME%.f3d}* - MitCad"
  grep -q "Bodies shown: none" <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Bodies shown" | tail -1) &&
    ui_fail "the imported bodies are not shown"
  # The light bulbs (mitcad#6, #7): the sketches the view shows (logged
  # when they change) are those the model shows, and some are hidden (a
  # part's sketches are used).
  sketches_shown=$(grep "Sketches shown: " "$UI_LOG" | tail -1)
  sketches_shown=${sketches_shown#*Sketches shown: }
  ui_mark
  ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
  type_into_dialog 'Save As' "$WORK/part.mitcad"
  ui_focus_main
  ui_expect_new "Saved $WORK/part.mitcad" "the imported part saved"
  "$CLI" info "$WORK/part.mitcad" --json > "$WORK/part.json" 2> "$WORK/part.err" ||
    { cat "$WORK/part.err"; ui_fail "mitcad-cli info"; }
  read -r model_shown model_hidden <<< "$(python3 -c '
import json, sys
features = json.load(open(sys.argv[1]))["timeline"]["features"]
sketches = [f for f in features if f["type"] == "sketch" and f["status"] in ("ok", "warning")]
shown = [f["uid"] for f in sketches if f["visible"]]
print(",".join(shown) or "none", sum(1 for f in sketches if not f["visible"]))' "$WORK/part.json")"
  [ "${sketches_shown:-none}" = "${model_shown//,/, }" ] ||
    ui_fail "the view shows the sketches '${sketches_shown:-none}', the model '$model_shown'"
  [ "${model_hidden:-0}" -gt 0 ] || ui_fail "no sketch of the imported part is hidden"
  echo "ok   sketches shown: ${sketches_shown:-none}; $model_hidden hidden"

  echo "--- .f3d import stopped and cancelled (T1e)"
  # Every definition the import tries takes 1.5 s longer, so that it is
  # still running when the dialog stops or cancels it.
  ui_stop_app
  export MITCAD_TEST_RECOMPUTE_DELAY_MS=import_f3d=1500
  ui_start_app --open "$WORK/block.step" --no-native-dialogs
  unset MITCAD_TEST_RECOMPUTE_DELAY_MS
  ui_expect_log "Imported block.step as" "a document to replace"
  ui_mark
  ui_step "open the part again (Ctrl+O)"   open_f3d
  ui_expect_new "Import item 2:" "two items replayed" 120
  ui_focus_dialog '^Import$'
  ui_step "Stop and Keep (Alt+S)"          ui_key alt+s
  ui_expect_new "Import of $NAME: stop requested after" "a stop was asked for"
  ui_expect_new "Import of $NAME stopping: the remaining items take the file's bodies" "the worker stops"
  ui_expect_new "Import report: $NAME:" "the stopped import finished with a report" 300
  report=$(grep "Import report: $NAME:" "$UI_LOG" | tail -1)
  grep -q ", stopped at .* (item [0-9]*)$" <<< "$report" || ui_fail "the report does not say where it stopped: $report"
  echo "ok   the report says where it stopped: ${report##*, stopped }"
  bodies=$(sed -n 's/.*), \([0-9]*\) bodies,.*/\1/p' <<< "$report")
  [ "${bodies:-0}" -gt 0 ] || ui_fail "no bodies came in: $report"
  kept=$(($(report_count parametric) + $(report_count partial)))
  [ "$kept" -gt 0 ] || ui_fail "nothing imported before the stop stayed: $report"
  [ "$(report_count fallback)" -gt "${full_fallback:-0}" ] ||
    ui_fail "no more items came in as the file's bodies than without the stop: $report"
  echo "ok   $kept items parametric (those before the stop, sketches after it), the rest as the file's bodies"
  echo "     ${report#*: }"
  ui_expect_new "; stopped at " "the stop is in the import's log line"
  ui_focus_dialog '^Import Report$'
  ui_step "close the report (Enter)"       ui_key Return
  ui_focus_main
  expect_title "${NAME%.f3d}* - MitCad"
  ui_mark
  ui_step "open the part once more (Ctrl+O)" open_f3d
  ui_expect_new "Import of $NAME started" "the import started again"
  sleep 2
  ui_focus_dialog '^Import$'
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Import of $NAME cancelled" "the import was cancelled"
  ui_focus_main
  sleep 1
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "Imported $NAME|Import report|New document" &&
    ui_fail "the cancelled import changed the document"
  expect_title "${NAME%.f3d}* - MitCad"
  echo "ok   the open document stays as it was"
else
  echo "skip .f3d import: MITCAD_F3D_CORPUS is not set"
fi

echo "--- Export where the design shows the bodies (mitcad#19)"
# A 20 x 10 x 5 pin in a component turned a quarter about z and moved
# 100 mm along x: the STEP file has it where the view shows it (an
# assembly that places its part), or with component coordinates chosen
# where its component has it.
cat > "$WORK/assembly.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 10, "height": 5, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Pin"},
  {"cmd": "components_from_bodies", "bodies": ["F1.b0"]},
  {"cmd": "set_occurrence_transform", "occurrence": "Pin:1",
    "transform": {"translation": [100, 0, 0], "rotation": {"axis": [0, 0, 1], "angle": 1.5707963267948966}}},
  {"expect": {"occurrence": "Pin:1", "body": "Pin",
    "bbox": {"min": [90, 0, 0], "max": [100, 20, 5]}, "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/assembly.json" --save "$WORK/assembly.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
# export_local path: File > Export with Coordinates (Alt+D) set to the
# component's.
export_local() {
  file_menu e
  ui_focus_dialog '^Export$'
  ui_type_field "Export" "$1"
  ui_key alt+d
  ui_key Down
  ui_key Return
  ui_focus_main
}
ui_stop_app
ui_start_app --open "$WORK/assembly.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/assembly.mitcad" "the assembly opened"
ui_mark
ui_step "File > Export a STEP file"        export_to "$WORK/pin.step"
ui_expect_new "Export dialog opened: unit mm (mm | cm | m | in | ft), coordinates design (design | component)" \
  "Export offers the coordinates, the design's first"
ui_expect_new "Exported $WORK/pin.step: step, 1 body(ies), mm, design coordinates" "written where the design shows it"
ui_mark
ui_step "File > Export in component coordinates" export_local "$WORK/pin-local.step"
ui_expect_new "Exported $WORK/pin-local.step: step, 1 body(ies), mm, component coordinates" \
  "written in its component's coordinates"
cat > "$WORK/placed.json" << EOF
[
  {"cmd": "import_file", "path": "$WORK/pin.step"},
  {"expect": {"body": "Pin", "volume": 1000, "bbox": {"min": [90, 0, 0], "max": [100, 20, 5]}, "tolerance": 1e-5}},
  {"cmd": "import_file", "path": "$WORK/pin-local.step", "wrap": false},
  {"expect": {"body": "Pin (2)", "volume": 1000, "bbox": {"min": [0, 0, 0], "max": [20, 10, 5]}, "tolerance": 1e-5}}
]
EOF
"$CLI" run "$WORK/placed.json" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "the STEP files' places"; }
echo "ok   read back: the pin where the view shows it, and where its component has it"

# A STEP assembly that places the pin's part twice (mitcad#1274): Import
# asks how it comes in, and Enter keeps the assembly, the part one
# component placed twice.
cat > "$WORK/pins.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 20, "width": 10, "height": 5, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Pin"},
  {"cmd": "components_from_bodies", "bodies": ["F1.b0"]},
  {"cmd": "copy_occurrence", "occurrence": "Pin:1", "transform": {"translation": [0, 50, 0]}},
  {"cmd": "export", "path": "$WORK/pins.step"}
]
EOF
"$CLI" run "$WORK/pins.json" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "the pins' STEP file"; }
ui_mark
ui_step "File > Import the STEP assembly"  import_from "$WORK/pins.step"
ui_expect_new "Import STEP Assembly dialog opened: pins.step" "Import asks how the assembly comes in"
ui_focus_dialog '^Import STEP Assembly$'
ui_step "as components (Enter)"            ui_key Return
ui_expect_new "Import STEP Assembly: components" "the assembly kept"
# The design has content (the pin): where the file goes (mitcad#1629).
ui_expect_new "Import place dialog opened: pins.step" "Import asks where the assembly goes"
ui_focus_dialog '^Where to Import$'
ui_step "into a new component (Enter)"     ui_key Return
ui_focus_main
ui_expect_new "pins.step as Base1: 1 body(ies), 1 component(s), 2 occurrence(s), into component pins" \
  "the pin's part once, placed twice, in the component pins"
ui_browser_reveal "Browser Root/pins:1"
ui_expect_new "Browser Root/pins:1 at" "the browser lists the component pins"
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_new "Undo: Import pins.step" "the import is one undo step"
# Find repeated parts (mitcad#1277): with One base feature the two pins
# are bodies of their own, and the box makes them one component placed
# twice, in the import's undo step.
ui_mark
ui_step "File > Import the STEP assembly again" import_from "$WORK/pins.step"
ui_expect_new "Import STEP Assembly dialog opened: pins.step" "Import asks again"
ui_focus_dialog '^Import STEP Assembly$'
ui_step "Find repeated parts (Alt+R)"      ui_key alt+r
ui_step "One base feature (Alt+O)"         ui_key alt+o
ui_expect_new "Import STEP Assembly: base, repeated parts" "one base feature, its copies to be found"
ui_expect_new "Import place dialog opened: pins.step" "Import asks where it goes again"
ui_focus_dialog '^Where to Import$'
ui_step "Into the Active Component (Alt+A)" ui_key alt+a
ui_focus_main
ui_expect_new "Import place: active component" "into the active component, as before"
ui_expect_new "pins.step as Base1: 2 body(ies), repeated parts: 1 component(s), 1 copies" \
  "the two pins one component placed twice"
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_new "Undo: Import pins.step" "the import and its repeated parts are one undo step"

echo "--- A .sc file of another program (mitcad#1622)"
# Other programs use .sc too: one whose format key is not SolidCraft's is
# refused by its content before an import starts.
printf '{"format": "other/1", "name": "A"}\n' > "$WORK/other-program.sc"
ui_stop_app
timeout 120 "$UI_APP" --no-recovery --open "$WORK/other-program.sc" --no-native-dialogs > "$WORK/other-program.log" 2>&1
status=$?
[ "$status" = 2 ] || { cat "$WORK/other-program.log"; ui_fail "--open of another program's .sc file exited with $status"; }
grep -q "Not a SolidCraft design: the file's format is \`other/1\`" "$WORK/other-program.log" ||
  { cat "$WORK/other-program.log"; ui_fail "no clear refusal for another program's .sc file"; }
grep -q "Import of other-program.sc started" "$WORK/other-program.log" &&
  { cat "$WORK/other-program.log"; ui_fail "an import started for another program's .sc file"; }
echo "ok   another program's .sc file refused: Not a SolidCraft design"
ui_start_app --demo --no-native-dialogs

echo "--- Saving in a project (P12a)"
# A project folder: its files are saved in version 3, the B-rep data of
# base features in the project's store.
mkdir -p "$WORK/project/.mitcad"
printf '{"format": "mitcad-project", "version": 1}\n' > "$WORK/project/.mitcad/project.json"
ui_stop_app
ui_start_app --open "$WORK/block.step" --no-native-dialogs
ui_expect_log "Imported block.step as" "the STEP opened as a new document"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
type_into_dialog 'Save As' "$WORK/project/block.mitcad"
ui_focus_main
ui_expect_log "Saved $WORK/project/block.mitcad" "saved into the project"
grep -q '"version": 3' "$WORK/project/block.mitcad" || ui_fail "the project file is not version 3"
# The bodies' B-rep stays out of the file (the design's preview, mitcad#1539,
# is base64 data of its own and may be there).
python3 -c '
import json, sys
doc = json.load(open(sys.argv[1]))
inline = [b for f in doc.get("features", []) for b in f.get("bodies", [])
          if isinstance(b.get("brep"), dict) and "data" in b["brep"]]
sys.exit(1 if inline else 0)' "$WORK/project/block.mitcad" ||
  ui_fail "the B-rep data is inside the project file"
breps=$(find "$WORK/project/.mitcad/brep" -name '*.brep.zlib' | wc -l)
[ "$breps" = 1 ] || ui_fail "$breps B-rep files in the project's store, expected 1"
echo "ok   version 3, the B-rep data in the project's store"
ui_stop_app
ui_start_app --open "$WORK/project/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/project/block.mitcad" "opened from the project"
ui_expect_log "Bodies shown: F1.b0" "the base feature's body, its data from the store"
grep -q "missing from the project store" "$UI_LOG" && ui_fail "the B-rep data was not found"
expect_title "block.mitcad - MitCad"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI import test"
