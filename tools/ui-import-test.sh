#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Files (U6) through the File menu, on the demo block (60 x 40 x 20 mm):
#   - STEP, STL and DXF round trips: File > Export writes the block (its
#     sketch for DXF), File > Import brings the file back: a base feature
#     of the same volume, a mesh body, a sketch with the rectangle's profile.
#   - Insert DXF's placement and layers (P9): a layered drawing placed by
#     its lower left corner, one layer left out.
#   - --open of a STEP and a DXF file: new documents; Open Recent.
#   - Insert Component of a saved project, linked.
#   - FreeCAD documents through File > Open: a stored shape in a part and a
#     sketch, then a PartDesign Body whose pad is replayed and whose helix
#     comes in as its stored shape; the import report's counts and text
#     (mitcad#22), with the parameters made of a spreadsheet and the
#     expressions translated or kept as FreeCAD's values (mitcad#4).
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
#   - Saving in a project (P12a): version 3, the B-rep data in the
#     project's store, opened again with the same body.
#
# Runs headless on Xvfb (see ui-test-lib.sh); Qt's own file dialogs
# (--no-native-dialogs), so that paths can be typed.
# Usage: tools/ui-import-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-import.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# file_menu key: the File menu (Alt+F), then an entry's underlined letter.
file_menu() {
  ui_key alt+f
  ui_sync
  ui_key "$1"
}

# type_into_dialog title path: a path typed into a dialog's focused field.
type_into_dialog() {
  ui_focus_dialog "$1"
  ui_key ctrl+a
  xdotool type --delay 20 "$2"
  sleep 0.3
  ui_key Return
}

export_to() {
  file_menu e
  type_into_dialog '^Export$' "$1"
  ui_focus_main
}

import_from() {
  file_menu i
  type_into_dialog '^Import$' "$1"
}

expect_title() {
  local title=""
  for _ in $(seq 1 25); do
    title=$(xdotool getwindowname "$UI_WINDOW" 2> /dev/null)
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
ui_focus_main
ui_expect_new "Imported block.step as" "the STEP came in as a base feature"
ui_expect_volume "New body .*(F3\.b0): volume \([0-9.]*\) mm3" 48000 "the STEP body's volume"
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Import block.step" "the import is one undo step"

echo "--- STL"
ui_mark
ui_step "File > Export an STL file"        export_to "$WORK/block.stl"
ui_expect_new "Exported $WORK/block.stl: stl, 1 body(ies)" "the block written as STL"
ui_mark
ui_step "File > Import it"                 import_from "$WORK/block.stl"
ui_step "in millimetres (Enter)"           type_into_dialog '^Insert Mesh$' ""
ui_focus_main
ui_expect_new "Imported block.stl as" "the STL came in"
ui_expect_volume "New body .*(F[0-9]*\.b0): volume \([0-9.]*\) mm3" 48000 "the mesh body's volume" 1e-4
ui_step "undo the import (Ctrl+Z)"         ui_key ctrl+z
ui_expect_log "Undo: Import block.stl" "undone"

echo "--- DXF"
ui_mark
ui_step "File > Export the sketch to DXF"  export_to "$WORK/sketch.dxf"
ui_expect_new "Exported $WORK/sketch.dxf: dxf, 4 entities" "the rectangle written as DXF"
ui_mark
ui_step "File > Import it"                 import_from "$WORK/sketch.dxf"
ui_step "on XY, in mm (Enter)"             type_into_dialog '^Insert DXF$' ""
ui_focus_main
ui_expect_new "Inserted sketch.dxf into Sketch2: 4 curves, 4 points, 0 texts, 1 profiles, 0 warnings" \
  "the rectangle in a new sketch, a profile again"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
type_into_dialog '^Save As$' "$WORK/block.mitcad"
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
ui_step "new (Ctrl+N)"                     ui_key ctrl+n
ui_expect_log "New document" "an empty document"
ui_mark
ui_step "File > Insert Component"          file_menu m
type_into_dialog '^Insert Component$' "$WORK/block.mitcad"
ui_focus_dialog '^Insert Component$'
ui_step "linked (Enter)"                   ui_key Return
ui_focus_main
ui_expect_new "Inserted component" "the saved block placed as a component"
ui_expect_new "(linked) from block.mitcad" "linked to its file"
ui_stop_app

echo "--- --open of other files, Open Recent"
ui_start_app --open "$WORK/block.step" --no-native-dialogs
ui_expect_log "Imported block.step as" "a STEP opened as a new document"
expect_title "block* - Mitcad"
ui_stop_app
ui_start_app --open "$WORK/sketch.dxf" --no-native-dialogs
ui_expect_log "Inserted sketch.dxf into Sketch1: 4 curves" "a DXF opened as a sketch of a new document"
ui_mark
ui_step "File > Open Recent"               file_menu r
ui_expect_new "Recent files: sketch.dxf | block.step | block.mitcad" "the files opened and saved last"
ui_step "the second (Down, Enter)"         ui_key Down Return
ui_focus_dialog '^Mitcad$'
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
  ui_focus_dialog '^Mitcad$'
  ui_key d
  type_into_dialog '^Open$' "$1"
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
expect_title "block* - Mitcad"

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
ui_expect_new "Import report summary: history.FCStd (FreeCAD 1.0R39319 (Git)) 5 objects: 1 body, 0 component, 0 occurrence, 2 parametric, 0 partial, 1 fallback, 0 included, 1 skipped. 1 bodies; of the history's 2 features, 1 were replayed as Mitcad features (checked against the shapes FreeCAD stored), 1 came in as their stored shapes (fallback) and 0 were skipped. 2 parameters (1 of FreeCAD's expressions, 1 of its values); of the document's 2 expressions, 1 were translated, 1 kept FreeCAD's value and 0 drive nothing the import carries over." \
  "the report: the history replayed, one fallback; the parameters and expressions"
ui_expect_new "Import report kept: Helix.Height = Params.Length * Params.Ratio: Helix was not replayed" \
  "the expression kept as FreeCAD's value, with the reason"
ui_focus_dialog '^Import Report$'
ui_step "close the report (Enter)"         ui_key Return
ui_focus_main
ui_expect_new "Imported history.FCStd: 1 bodies" "a new document with the Body's body"
expect_title "history* - Mitcad"

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
    ui_focus_dialog '^Mitcad$'
    ui_key d
    type_into_dialog '^Open$' "$SMALL"
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
  expect_title "${NAME%.f3d}* - Mitcad"
  grep -q "Bodies shown: none" <(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep "Bodies shown" | tail -1) &&
    ui_fail "the imported bodies are not shown"
  # The light bulbs (mitcad#6, #7): the sketches the view shows (logged
  # when they change) are those the model shows, and some are hidden (a
  # part's sketches are used).
  sketches_shown=$(grep "Sketches shown: " "$UI_LOG" | tail -1)
  sketches_shown=${sketches_shown#*Sketches shown: }
  ui_mark
  ui_step "save as (Ctrl+Shift+S)"         ui_key ctrl+shift+s
  type_into_dialog '^Save As$' "$WORK/part.mitcad"
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
  expect_title "${NAME%.f3d}* - Mitcad"
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
  expect_title "${NAME%.f3d}* - Mitcad"
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
  ui_key ctrl+a
  xdotool type --delay 20 "$1"
  sleep 0.3
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
  {"cmd": "import_file", "path": "$WORK/pin-local.step"},
  {"expect": {"body": "Pin (2)", "volume": 1000, "bbox": {"min": [0, 0, 0], "max": [20, 10, 5]}, "tolerance": 1e-5}}
]
EOF
"$CLI" run "$WORK/placed.json" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "the STEP files' places"; }
echo "ok   read back: the pin where the view shows it, and where its component has it"

echo "--- Saving in a project (P12a)"
# A project folder: its files are saved in version 3, the B-rep data of
# base features in the project's store.
mkdir -p "$WORK/project/.mitcad"
printf '{"format": "mitcad-project", "version": 1}\n' > "$WORK/project/.mitcad/project.json"
ui_stop_app
ui_start_app --open "$WORK/block.step" --no-native-dialogs
ui_expect_log "Imported block.step as" "the STEP opened as a new document"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
type_into_dialog '^Save As$' "$WORK/project/block.mitcad"
ui_focus_main
ui_expect_log "Saved $WORK/project/block.mitcad" "saved into the project"
grep -q '"version": 3' "$WORK/project/block.mitcad" || ui_fail "the project file is not version 3"
grep -q '"data":' "$WORK/project/block.mitcad" && ui_fail "the B-rep data is inside the project file"
breps=$(find "$WORK/project/.mitcad/brep" -name '*.brep.zlib' | wc -l)
[ "$breps" = 1 ] || ui_fail "$breps B-rep files in the project's store, expected 1"
echo "ok   version 3, the B-rep data in the project's store"
ui_stop_app
ui_start_app --open "$WORK/project/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/project/block.mitcad" "opened from the project"
ui_expect_log "Bodies shown: F1.b0" "the base feature's body, its data from the store"
grep -q "missing from the project store" "$UI_LOG" && ui_fail "the B-rep data was not found"
expect_title "block.mitcad - Mitcad"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI import test"
