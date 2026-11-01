#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/Ribbon.cpp app/framework/Ribbon.hpp app/MainWindowMake.cpp app/print/MakeBodyCard.cpp
# check-all sources: app/browser/BrowserPanel.cpp app/MainWindowContextMenu.cpp app/commands/MeasureHistoryView.cpp
# check-all sources: app/MainWindowMeasurements.cpp app/MainWindowSheet.cpp app/files/MainWindowPrint.cpp
# check-all sources: core/model-api/src/body_notes.rs core/model-api/src/cut_sheets.rs core/model/src/document/annotations.rs tools/cli
# check-all sources: core/model-api/src/annotations.rs app/view/MeasurementOverlay.cpp app/framework/CommandSession.cpp
# The Make tab (mitcad#1049) through the real UI, in the Floating layout,
# then in the Docked one, on a design mitcad-cli makes: a sheet-metal
# plate with its flat pattern (Body1) and a 40 x 30 x 10 block with a
# counterbored hole (Body2) that sets Walls 5 of its own; the design prints
# with the printer Bench and the process Standard:
#   - The tabs Solid, Build and Make; the Solid tab's groups without the
#     moved commands (Modify without Edit Materials…, Inspect without Print
#     Analysis, Sheet Metal without Export Flat Pattern…, Render with Render
#     Image… alone).
#   - The Make tab's groups Materials, Annotate, 3D Print and Laser with
#     their menus; no CNC group.
#   - Each moved command run from its new place: Edit Materials…, Edit
#     Printers and Processes…, Print Analysis, Send to Slicer (once 3D
#     Print) and Export Flat Pattern… (its DXF written); Export Face as
#     DXF… on Body2's top face from the Laser group and from the face's
#     context menu.
#   - The Annotate group places each kind: Dimension (the counterbore's
#     diameter), Note (its text typed; its preview before Add, the note
#     added where the preview was, mitcad#1097), Datum Label (A), Feature Control
#     Frame (its tolerance from Tolerance…), Hole Callout, Material and
#     Process Note (Printer: Bench / Process: Standard / Walls: 5). The
#     Measure panel measures: no Keep as Dimension any more.
#   - Body2 selected on the Make tab: the card shows its material and its
#     print settings with Walls 5 set on the body; Walls typed 3 in the
#     card is set on the body, and one Undo puts 5 back. On the Solid tab
#     there is no card.
#   - The Browser marks Body2 (its own Walls) on the Make tab, not Body1,
#     and nothing on the Solid tab.
#   - Docked: the Make tab with its four groups.
# tools/ui-windows-make-tab-test.ps1 does the same in the Windows VM.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-make-tab-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-make-tab.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/make.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":1,"bend_radius":2,"allowance":{"type":"k_factor","k":0.4},"reference_side":"front","bend_relief":{"type":"rectangular","width":1,"depth":1},"corner_relief":{"type":"round","width":2,"depth":2}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[-40,0],"width":30,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"$R"}]}},
  {"cmd":"set_flat_pattern","body":"F3.b0"},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F4","corner":[0,0],"width":40,"height":30},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F4","region":"$R"}],
    "extent":{"type":"distance","distance":10},"operation":"new_body"}},
  {"cmd":"add_feature","def":{"type":"hole",
    "placement":{"type":"face","body":"F5.b0","face":"F5:end($R)","points":[[20,15,10]]},
    "diameter":6.6,"extent":{"type":"through_all"},
    "kind":{"type":"counterbore","diameter":11,"depth":4}}},
  {"cmd":"create_print_profile","kind":"printer","name":"Bench","values":{"nozzle_diameter":0.4}},
  {"cmd":"create_print_profile","kind":"process","name":"Standard","values":{"layer_height":0.2,"perimeters":2}},
  {"cmd":"set_property","owner":"C0","set":"Print Printer","key":"profile","value":"bench"},
  {"cmd":"set_property","owner":"C0","set":"Print Process","key":"profile","value":"standard"},
  {"cmd":"set_property","owner":"F5.b0","set":"Print Process","key":"perimeters","value":5}
]
EOF
"$CLI" run "$WORK/make.json" --save "$WORK/make.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# menu_line "TAB/GROUP": the entries of that group's menu as last logged.
menu_line() { grep -F "Ribbon menu $1: " "$UI_LOG" | tail -1 | sed "s|.*Ribbon menu $1: ||"; }

# group_menu "TAB/GROUP" "entries": opens the group's menu, checks its
# entries exactly and closes it.
group_menu() {
  ui_mark
  ui_step "open the $1 menu"                ui_click_logged "Ribbon $1"
  ui_expect_new "Ribbon menu $1: " "the $1 menu opened"
  [ "$(menu_line "$1" | sed 's/…/.../g')" = "${2//…/...}" ] ||
    ui_fail "the $1 menu is '$(menu_line "$1")', not '$2'"
  echo "ok   the $1 menu: $2"
  ui_step "close it (Esc)"                  ui_key Escape
}

# annotate_entry downs: the Annotate group's entry that many down its menu.
# Chosen as a step (ui_step's sync): the command's card is laid out and
# its pick places are logged again clear of it (mitcad#1086) before the
# next step reads them; without the sync the pick read the places logged
# as the input started, one of them under the card.
annotate_entry() {
  ui_step "open the Annotate menu"          ui_click_logged "Ribbon MAKE/ANNOTATE"
  ui_expect_new "Ribbon menu MAKE/ANNOTATE: " "the Annotate menu opened"
  xdotool key --delay 80 --repeat "$1" Down
  ui_step "choose entry $1 (Enter)"         ui_key Return
}

ui_start_display
ui_start_app --open "$WORK/make.mitcad" --no-native-dialogs --chrome=floating
ui_expect_log "Opened $WORK/make.mitcad" "the design opened"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Browser Make markers: none" "no markers on the Solid tab"

echo "--- The tabs, and the Solid tab without the moved commands"
ui_expect_log "Ribbon tab SOLID at" "the Solid tab"
ui_expect_log "Ribbon tab BUILD at" "the Build tab"
ui_expect_log "Ribbon tab MAKE at" "the Make tab after them"
for group in "SOLID/MODIFY:Edit Materials" "SOLID/INSPECT:Print Analysis" "SOLID/SHEET METAL:Export Flat Pattern"; do
  ui_mark
  ui_step "open the ${group%%:*} menu"      ui_click_logged "Ribbon ${group%%:*}"
  ui_expect_new "Ribbon menu ${group%%:*}: " "the ${group%%:*} menu opened"
  [ -n "$(menu_line "${group%%:*}")" ] || ui_fail "no entries logged for ${group%%:*}"
  case "$(menu_line "${group%%:*}")" in
  *"${group#*:}"*) ui_fail "${group#*:} is still in ${group%%:*}: $(menu_line "${group%%:*}")" ;;
  esac
  echo "ok   ${group#*:} has left ${group%%:*}"
  ui_step "close it (Esc)"                  ui_key Escape
done
group_menu "SOLID/RENDER" "Render Image…"
# The Solid tab's group of Render Image is Render (mitcad#1070), so Make
# names only the tab.
grep -F "Ribbon SOLID/MAKE" "$UI_LOG" && ui_fail "a Make group on the Solid tab"
echo "ok   no Make group on the Solid tab"

echo "--- The Make tab and its groups"
ui_mark
ui_step "the Make tab"                     ui_click_logged "Ribbon tab MAKE"
ui_expect_new "Ribbon tab shown: MAKE" "the Make tab shown"
for group in MATERIALS ANNOTATE "3D PRINT" LASER; do
  ui_expect_new "Ribbon MAKE/$group at" "its $group group"
done
grep -F "Ribbon MAKE/CNC" "$UI_LOG" && ui_fail "a CNC group without commands"
echo "ok   no CNC group"
ui_expect_new "Browser Make markers: Root/Bodies/Body2: Own print settings: Walls" \
  "the Browser marks Body2, which sets Walls of its own"
grep -F "Browser Make markers: " "$UI_LOG" | tail -1 | grep -qF "Body1" && ui_fail "Body1 marked"
echo "ok   Body1 not marked"
group_menu "MAKE/MATERIALS" "Edit Materials… | Material"
group_menu "MAKE/ANNOTATE" \
  "Dimension | Note | Datum Label | Feature Control Frame | Hole Callout | Material | Process Note | - | Annotations…"
group_menu "MAKE/3D PRINT" "Print Analysis | Send to Slicer | Edit Printers and Processes…"
group_menu "MAKE/LASER" "Export Flat Pattern… | Export Face as DXF…"

echo "--- The moved commands from their new places"
ui_mark
ui_step "Edit Materials…"                  ui_click_logged "Ribbon button solid.materials"
ui_expect_new "Materials opened" "the Materials window from the Materials group"
ui_focus_dialog '^Materials$'
ui_step "close it (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "Edit Printers and Processes…"     ui_click_logged "Ribbon button make.printers"
ui_expect_new "Printers and processes opened" "Printers and Processes from the 3D Print group"
ui_focus_dialog '^Printers and Processes$'
ui_step "close it (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "Print Analysis"                   ui_click_logged "Ribbon button inspect.print_analysis"
ui_expect_new "Command Print Analysis started" "Print Analysis from the 3D Print group"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Print Analysis cancelled" "cancelled"
ui_mark
ui_step "Send to Slicer"                   ui_click_logged "Ribbon button make.print3d"
ui_expect_new "Command Send to Slicer started" "Send to Slicer (once 3D Print) from the 3D Print group"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Send to Slicer cancelled" "cancelled"
ui_mark
ui_step "Export Flat Pattern…"             ui_click_logged "Ribbon button sheet.export_dxf"
ui_type_path "Export Flat Pattern" "$WORK/flat.dxf"
ui_focus_main
ui_expect_new "Exported $WORK/flat.dxf: dxf flat pattern of Body1" "the flat pattern written from the Laser group" 30
[ -s "$WORK/flat.dxf" ] || ui_fail "no flat pattern DXF"

echo "--- Export Face as DXF… (Laser and the face's context menu)"
# The faces' pick places are logged while a command's input takes faces:
# Dimension started and cancelled.
ui_mark
ui_step "Dimension"                        ui_click_logged "Ribbon button make.dimension"
ui_expect_new "Pick face F5.b0/F5:end(r{c1" "the top face's pick place"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Dimension cancelled" "cancelled"
ui_mark
ui_step "right-click the top face"         xdotool mousemove $(ui_pick_at "face F5.b0/F5:end(r{c1") click 3
ui_expect_new "Context menu: " "its context menu"
grep -F "Context menu: " "$UI_LOG" | tail -1 | sed 's/…/.../g' | grep -qF "Export Face as DXF..." ||
  ui_fail "no Export Face as DXF… in the face's context menu"
ui_step "Export Face as DXF…"              ui_menu_choose "Export Face as DXF…"
ui_type_path "Export Face as DXF" "$WORK/face2.dxf"
ui_focus_main
ui_expect_new "Exported $WORK/face2.dxf: dxf outline of Body2 along F5:end(" "written from the context menu" 30
ui_expect_log "Selected: 1 face" "the top face selected by the right click"
# The face still selected: the Laser group's button.
ui_mark
ui_step "Export Face as DXF…"              ui_click_logged "Ribbon button make.export_face_dxf"
ui_type_path "Export Face as DXF" "$WORK/face.dxf"
ui_focus_main
ui_expect_new "Exported $WORK/face.dxf: dxf outline of Body2 along F5:end(" "written from the Laser group" 30
[ -s "$WORK/face.dxf" ] || ui_fail "no face DXF"
ui_step "clear the selection (Esc)"        ui_key Escape

echo "--- The Annotate group places each kind"
ui_mark
ui_step "Dimension"                        ui_click_logged "Ribbon button make.dimension"
ui_expect_new "Command Dimension started" "Dimension started"
ui_step "pick the counterbore"             ui_click_pick "face F5.b0/F6:hole0.cbore_wall"
ui_expect_new "Dimension Selection 1: 1 face" "the round face picked"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: Diameter1" "kept as the counterbore's diameter"
ui_expect_new "Done Dimension" "the panel closed"

ui_mark
ui_step "Note"                             ui_click_logged "Ribbon button make.note"
ui_expect_new "Command Note started" "Note started"
ui_step "pick the top face"                ui_click_pick "face F5.b0/F5:end(r{c1"
ui_expect_new "Note Pick: 1 face" "the top face picked"
ui_type_in "Panel Note input text" "Deburr"
# Its preview before Add (mitcad#1097): the note the model places in the
# view, drawn dimmed; added, the note is where its preview was.
ui_expect_new "Annotation preview: Note1 note" "the note previewed before Add"
ui_expect_new "Annotation preview drawn: Note1 = Deburr" "the preview drawn with its text"
read -r px py <<< "$(ui_logged_at "Annotation preview label Note1")"
ui_mark
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: Note1" "the note added"
ui_expect_new "Annotation preview drawn: none" "the preview gone with the panel"
read -r nx ny <<< "$(ui_logged_at "Annotation label Note1")"
[ "$px,$py" = "$nx,$ny" ] || ui_fail "Note1's text at $nx,$ny, its preview's at $px,$py"
echo "ok   the note added where its preview was ($nx,$ny)"

ui_mark
annotate_entry 3
ui_expect_new "Command Datum Label started" "Datum Label from the Annotate menu"
ui_step "pick the top face"                ui_click_pick "face F5.b0/F5:end(r{c1"
ui_expect_new "Datum Label Pick: 1 face" "the top face picked"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: DatumA" "the datum label, its first free letter"

ui_mark
annotate_entry 4
ui_expect_new "Command Feature Control Frame started" "Feature Control Frame from the Annotate menu"
ui_step "pick the top face"                ui_click_pick "face F5.b0/F5:end(r{c1"
ui_expect_new "Feature Control Frame Pick: 1 face" "the top face picked"
ui_expect_new "Panel Feature Control Frame action tolerance.0 at" "Tolerance… placed"
ui_step "Tolerance…"                       ui_click_logged "Panel Feature Control Frame action tolerance.0"
ui_expect_new "Feature Control Frame dialog opened: " "the frame's dialog"
ui_focus_dialog '^Feature Control Frame$'
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Feature Control Frame tolerance chosen: " "its tolerance chosen"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: Frame1" "the frame added"

ui_mark
annotate_entry 5
ui_expect_new "Command Hole Callout started" "Hole Callout from the Annotate menu"
ui_step "pick the counterbore"             ui_click_pick "face F5.b0/F6:hole0.cbore_wall"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: HoleCallout1" "the hole callout added"
ui_expect_new "HoleCallout1 = Ø6.6 THRU / ⌴ Ø11 ↧ 4" "written from the hole"

ui_mark
annotate_entry 6
ui_expect_new "Command Material started" "Material from the Annotate menu"
ui_step "pick the top face"                ui_click_pick "face F5.b0/F5:end(r{c1"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: Material1" "the material note added"

ui_mark
annotate_entry 7
ui_expect_new "Command Process Note started" "Process Note from the Annotate menu"
ui_step "pick the top face"                ui_click_pick "face F5.b0/F5:end(r{c1"
ui_step "add (Enter)"                      ui_key Return
ui_expect_new "Annotation kept: ProcessNote1" "the process note added"
ui_expect_new "ProcessNote1 = Printer: Bench / Process: Standard / Walls: 5" \
  "written from the body's print settings"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- Measure measures"
ui_mark
ui_step "measure (I)"                      ui_key i
ui_expect_new "Command Measure started" "Measure started"
ui_expect_new "Measure history at" "its panel laid out"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Measure Keep as Dimension" &&
  ui_fail "the Measure panel still has Keep as Dimension"
echo "ok   no Keep as Dimension in the Measure panel"
ui_step "close (Enter)"                    ui_key Return
ui_expect_new "Closed Measure" "Measure closed"

echo "--- The selected body's card"
ui_mark
ui_step "select Body2 in the Browser"      ui_click_logged "Browser Root/Bodies/Body2"
ui_expect_new "printer Bench (component Root); process Standard (component Root); own: Walls = 5 (body Body2)" \
  "the card: the printer, the process and Walls set on the body"
ui_expect_new "Make card: Body2: material " "with its material"
ui_expect_new "Make card value perimeters at" "Walls' field placed"
ui_mark
ui_step "Walls' field"                     ui_click_logged "Make card value perimeters"
ui_key ctrl+a
xdotool type --delay 40 "3"
ui_step "confirm (Enter)"                  ui_key Return
ui_expect_new "Make card Walls set: 3" "Walls set on the body"
ui_expect_new "own: Walls = 3 (body Body2)" "the card shows 3"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "own: Walls = 5 (body Body2)" "one undo step: 5 again"
ui_mark
ui_step "the Solid tab"                    ui_click_logged "Ribbon tab SOLID"
ui_expect_new "Ribbon tab shown: SOLID" "the Solid tab shown"
ui_expect_new "Make card: none" "no card on the Solid tab"
ui_expect_new "Browser Make markers: none" "no markers on the Solid tab"

echo "--- Docked: the Make tab and its groups"
ui_stop_app
ui_start_app --open "$WORK/make.mitcad" --no-native-dialogs --chrome=docked
ui_expect_log "Ribbon tab MAKE at" "the Make tab in the Docked layout"
ui_mark
ui_step "the Make tab"                     ui_click_logged "Ribbon tab MAKE"
for group in MATERIALS ANNOTATE "3D PRINT" LASER; do
  ui_expect_new "Ribbon MAKE/$group at" "its $group group, Docked"
done
ui_mark
ui_step "Dimension (Docked)"               ui_click_logged "Ribbon button make.dimension"
ui_expect_new "Command Dimension started" "Dimension started from the Docked toolbar"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Dimension cancelled" "cancelled"

ui_finish "the Make tab"
