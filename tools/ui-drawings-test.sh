#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser app/files app/MainWindow.cpp core/model/src/drawing core/model-drawing/src
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/drawing.rs core/model/src/document/drawings.rs tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/annotations.rs app/view/AnnotationDialogs.cpp
# check-all sources: core/model-api/src/drawing_tables.rs core/model-api/src/drawing_templates.rs app/print
# check-all sources: app/printing
# check-all sources: core/dwg/src/write.rs
# check-all sources: core/model-api/src/cut_sheets.rs app/browser/DocumentHost.hpp
# check-all sources: app/framework/Ribbon.cpp app/framework/FloatingLayout.cpp app/framework/GlassCard.cpp
# Drawings kept in the design (mitcad#707) through the real UI, in the
# Docked layout, on a 40 x 30 x 20 block with a 10 mm hole through it from
# the top that mitcad-cli makes (Body1, F2.b0):
#   - File › Create Drawing (the command search): its dialog (the ISO
#     template, A3 landscape, first angle: the template's), Drawing1 in the
#     root's Drawings folder, its sheet over the 3D view, its title block
#     the design's, the tools in captioned groups; Base View on at once.
#     The drawing takes the whole workspace (mitcad#1008): the ribbon's
#     DRAWING tab alone (Solid and Build hidden) with Close Drawing and the
#     tools' groups, the drawing's area the 3D view's; Close Drawing brings
#     back Solid and Build with the tab the drawing was opened from shown
#     (Solid, then Build).
#   - Model shortcuts (E, H) on the sheet do nothing (mitcad#725).
#   - Base View: the front view where the sheet is clicked, at 2:1; then
#     Projected View: a click right of it places the view projected from
#     that side. Each view's projection is logged (mitcad#1033): the
#     kernel's when it is added, then from the cache.
#   - Section View: a line across the front view, then a click where the
#     section goes; Detail View: a circle on its corner, then its place;
#     the detail moved (nothing projected again, mitcad#1033) and its
#     circle dragged on its parent (mitcad#715).
#   - Break: a band out of the front view; the section's hatching off in
#     the view panel; the detail deleted (Delete) and back (Ctrl+Z).
#   - A second sheet (+); the sheets switched by clicks on their tabs; the
#     second renamed and deleted from its tab's menu; the Sheet… dialog's
#     Units.
#   - Annotations (phase 2): a linear dimension of the block's height
#     picked on its edges and placed by a click, the hole's diameter picked
#     on its wall in the section, a centre mark (the hole's centreline), a
#     note on a leader; the dimension's ± tolerance from the annotations'
#     tolerance dialog; the dimension dragged; the edge under the pointer
#     highlighted before its click and the dimension's lines previewed
#     while it is placed (mitcad#725). The dimension undone and redone
#     (the redo made 1.5 s longer) right after the pointer moved onto an
#     edge: the hover's question waits for the redo, the document is not
#     read while it computes (mitcad#753).
#   - Several items (mitcad#725): the dimension and the note selected with
#     Ctrl+click, dragged and deleted together, undone; Esc clears the
#     selection and leaves the drawing open; the note's menu (Edit Text…)
#     and a double-click open its text; a view's menu shades it.
#   - Tables (phase 3): a top view at 1:1 (Base View's Orientation and
#     Scale), a hole table of it placed by a click (the hole tagged A1).
#   - Another sheet (mitcad#759): a top view on a second sheet; Dimension's
#     pick on the first sheet, then the second sheet's tab: a diameter
#     there without Esc; a third sheet shown while the second is deleted
#     stays shown; the third deleted too.
#   - Close Drawing; the extrusion made 35 high in its panel; the drawing
#     opened again from the browser: the front view has grown upward and
#     the dimension says 35.
#   - The hole suppressed: its diameter marked broken, the block's
#     dimension still drawn, the hole table empty.
#   - Saved and opened again: the drawing with its five views and its
#     annotations.
#   - Export… PDF: a PDF of the sheet with the note and the dimension;
#     File › Print (Ctrl+P, to a PDF file in place of the system's dialog,
#     mitcad#1089): one page of the sheet's size;
#     Export… DWG: an R2000 DWG file of it (mitcad#728).
#   - Templates and revisions (phase 4): Body1 made PLA (the Material tool), the
#     title block's material follows; Add Revision: the revision table and
#     the title block show it; Export… PDF with them.
#   - An assembly mitcad-cli makes (Component1 placed twice, Component2
#     once), in the Floating layout (the DRAWING tab's group cards, the sheet
#     over the whole view, no card but the browser's, mitcad#1008): a
#     front view, a parts list with its
#     balloons (undone), a parts list without and a balloon on
#     Component1's second placement; the front view dragged with its parts
#     list selected: the list moves too (mitcad#752), undone; Export… SVG
#     with the parts list; the placement
#     deleted from the browser: the list counts one Component1 and the
#     balloon is broken; Undo brings them back.
#   - File › Export Drawing twice: the design's only drawing, made in the
#     workspace, offered first (mitcad#1680), New Drawing chosen; a note
#     added to the new drawing by hand, then the same drawing written as
#     it is, the note kept (mitcad#725).
#   - A cut sheet (mitcad#853) on a plate and a disk mitcad-cli makes:
#     Create Drawing's Kind Cut Sheet (600 x 300) on its CUT SHEET tab
#     (mitcad#1008), Add Components… with both
#     bodies, Export… DXF: the outlines on CUT_OUTER, the disk a circle.
#   - Create Drawing opens with the last choice (mitcad#1218): the last
#     drawing's ISO, A3, first angle (a cut sheet is not kept); ANSI chosen
#     (B, third angle) and a drawing created; the next Create Drawing opens
#     with ANSI, B, third angle (logged), cancelled. File › Export Drawing
#     then offers New Drawing first: two drawings, none laid out by it
#     (mitcad#1680).
#   - Model Items (mitcad#1406) on a block mitcad-cli makes with the
#     model's annotations (its height, datum A on its top, a hole's
#     callout): a sheet of three views, Model Items with nothing selected
#     (the command search, its panel's Sketch Dimensions and Model
#     Annotations, Apply): each annotation drawn once, in the view that
#     sees it (logged as "Drawing model items: …"); the view's panel shows
#     Model Annotations; one Undo takes them all.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawings-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawings.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE="$WORK/block.mitcad"
# File › Print (mitcad#1089) prints to this PDF in place of the system's
# dialog.
PRINTED=$WORK/printed.pdf
export MITCAD_TEST_PRINT_PDF=$PRINTED

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [20, 15], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The sheet's placement, logged as "Drawing sheet view x0,y0 x1,y1 x2,y2":
# where the sheet points (0, 0), (100, 0) and (0, 100) mm are in the
# window. sheet_at x y: the screen position of a sheet point (mm).
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
sheet_click() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2") click 1; ui_sync; }
# sheet_hover x y: the pointer moved to a sheet point (mm), no click.
sheet_hover() { ui_sync; xdotool mousemove $(sheet_at "$1" "$2"); ui_sync; }
# hover_is what "description": the newest "Drawing hover:" line names `what`.
# The hover is logged only when it changes (mitcad#1272), so a pointer that
# finds what it already found logs nothing new.
hover_is() {
  local last
  ui_wait_idle
  last=$(grep -F "Drawing hover: " "$UI_LOG" | tail -n 1)
  case "$last" in
    *"Drawing hover: $1 "*) echo "ok   $2" ;;
    *) ui_fail "$2: the newest hover line is '$last'" ;;
  esac
}
# hover_then_redo x y: the pointer moved to a sheet point (mm) and Redo
# (Ctrl+Y) at once, before the hover's question is asked (mitcad#753).
hover_then_redo() {
  local at
  at=$(sheet_at "$1" "$2")
  ui_sync
  xdotool mousemove $at key ctrl+y
}

# ctrl_click_logged "logged place": a Ctrl+click there (adds to the
# selection or takes from it).
ctrl_click_logged() {
  local x y
  read -r x y <<< "$(ui_logged_at "$1")"
  ui_apart "$x" "$y"
  xdotool mousemove "$x" "$y" keydown ctrl click 1 keyup ctrl
  ui_sync
}

# sheet_drag "logged place" x y: a left drag from a logged place (a view's
# middle) to a sheet point (mm).
sheet_drag() {
  local fx fy tx ty i
  read -r fx fy <<< "$(ui_logged_at "$1")"
  read -r tx ty <<< "$(sheet_at "$2" "$3")"
  ui_apart "$fx" "$fy"
  xdotool mousemove "$fx" "$fy" mousedown 1
  for i in 1 2 3 4 5 6 7 8; do
    xdotool mousemove $((fx + (tx - fx) * i / 8)) $((fy + (ty - fy) * i / 8))
    sleep 0.08
  done
  ui_sync
  xdotool mouseup 1
  ui_sync
}

# shifted_click "logged place" dx dy: a click that many sheet millimetres
# from a logged place (a view's middle), the sheet's scale from its last
# "Drawing sheet view" line.
shifted_click() {
  local view x y
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  read -r x y <<< "$(ui_logged_at "$1")"
  [ -n "$view" ] && [ -n "$x" ] || ui_fail "no place of '$1' or of the sheet"
  ui_sync
  xdotool mousemove $(awk -v view="$view" -v x="$x" -v y="$y" -v dx="$2" -v dy="$3" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    printf "%d %d\n", x + ((a[1] - o[1]) * dx / 100 + (b[1] - o[1]) * dy / 100) * s + 0.5,
                      y + ((a[2] - o[2]) * dx / 100 + (b[2] - o[2]) * dy / 100) * s + 0.5 }') click 1
  ui_sync
}

# annotations_drawn: the last "Drawing annotations drawn" line.
annotations_drawn() {
  grep -o "Drawing annotations drawn: .*" "$UI_LOG" | tail -1
}

# view_y id: the window y of the middle of a view's box, as logged last.
view_y() {
  grep -o "Drawing view $1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.*,//'
}

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait (a panel just built is laid out
# again once; a click must not use the place it had before).
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

# area_is_view: the drawing's area, as logged last ("Drawing area x,y WxH,
# view x,y WxH"), is the 3D view's (mitcad#1008: edge to edge).
area_is_view() {
  local line area view
  for _ in $(seq 1 20); do
    line=$(grep -o "Drawing area .*" "$UI_LOG" | tail -1)
    area=$(sed 's/^Drawing area \(.*\), view .*$/\1/' <<< "$line")
    view=$(sed 's/^.*, view //' <<< "$line")
    if [ -n "$line" ] && [ "$area" = "$view" ]; then
      echo "ok   the drawing takes the whole view: $area"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "the drawing's area is not the view's: ${line:-none logged}"
}

# floating_cards with|without name: the last "Floating cards" line, waited
# for, has the card or has not (mitcad#1008: while a drawing is open only
# the browser's card is shown).
floating_cards() {
  local line
  for _ in $(seq 1 30); do
    line=$(grep -o "Floating cards: .*" "$UI_LOG" | tail -1)
    if [ "$1" = with ] && grep -qF " $2 " <<< "$line"; then
      echo "ok   $2 shown"
      return 0
    fi
    if [ "$1" = without ] && [ -n "$line" ] && ! grep -qF " $2 " <<< "$line"; then
      echo "ok   $2 hidden"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "the cards are not $1 $2: $line"
}

# no_refusal: no drawing command refused since the mark.
no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# projection_logged view bodies how: since the mark, the log's line of the
# view's projection (mitcad#1033) with its bodies and milliseconds, "how"
# being "projected exactly" or "projected from a triangulation" (the
# kernel's) or "from the cache".
projection_logged() {
  local pattern="^Drawing projection of view $1: $2 bodies, [0-9]+\.[0-9] ms, $3\$"
  for _ in $(seq 1 30); do
    if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qE "$pattern"; then
      echo "ok   view $1's projection logged: $2 bodies, $3"
      return 0
    fi
    ui_sync
    sleep 0.2
  done
  ui_fail "no line of view $1's projection ($2 bodies, $3) in the log"
}

# nothing_projected: no view projected since the mark (mitcad#1033).
nothing_projected() {
  grep -E "^Drawing projection of view [0-9]+: .*, projected [a-z ]+$" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
    ui_fail "a view was projected again"
  echo "ok   nothing projected again"
}

ui_start_display
# A redo takes 1.5 s longer in this run (mitcad#753), whatever it changes.
MITCAD_TEST_JOB_SLEEP_MS=redo=1500 ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "block.mitcad" "the design opened"

echo "--- Create Drawing"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog asks for the sheet"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ISO (Alt+T, Home)"     ui_key alt+t Home
ui_step "OK: A3, landscape, first angle"   ui_key Return
ui_focus_main
# The ISO template brings its sheet and its projection method (mitcad#725).
ui_expect_new "Create Drawing dialog: template iso, A3, first_angle" "the ISO template chosen, on its A3, first angle"
ui_expect_new "Drawing created: D1 Drawing1 in C0" "Drawing1 in the root's folder"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet drawn: 0 views" "an empty sheet"
ui_expect_new "Drawing tool Base View at" "the tools laid out"
# The tools in their captioned groups, Export last (mitcad#725).
for group in Create Geometry Dimensions Text Symbols Tables Export; do
  ui_expect_new "Drawing group $group at" "the $group group's caption"
done
ui_expect_new "Drawing sheet view " "the sheet placed"
# The drawing takes the whole workspace (mitcad#1008): the ribbon's DRAWING
# tab alone, Close Drawing on it, the sheet's area the 3D view's.
ui_expect_new "Ribbon tabs: DRAWING (shown DRAWING)" "the DRAWING tab alone: Solid and Build hidden"
ui_expect_new "Ribbon tab DRAWING at" "the DRAWING tab shown"
ui_expect_new "Ribbon button drawing.close at" "Close Drawing on the ribbon"
ui_expect_new "Drawing Close Drawing at" "its place logged as the drawing's"
area_is_view
ui_expect_log "Browser Root/Drawings/Drawing1 at" "the browser lists it under Drawings"
ui_expect_log "Drawing title block (iso): TITLE=block" "the template's title block, the design's name"
# A new drawing starts with Base View (mitcad#725).
ui_expect_new "Drawing tool: Base View" "Base View is on for the new drawing"
ui_expect_new "Drawing hint: Click where the view goes" "its hint over the sheet"

echo "--- Model shortcuts do nothing on the sheet (mitcad#725)"
ui_mark
ui_step "E (Extrude's shortcut) on the sheet" ui_key e
ui_expect_new "Drawing shortcut ignored: E" "the key taken by the drawing"
grep -q "Drawing closed" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a model shortcut closed the drawing"
ui_step "H too"                             ui_key h
ui_expect_new "Drawing shortcut ignored: H" "the key taken by the drawing"
grep -q "Drawing closed" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a model shortcut closed the drawing"

echo "--- A base view and a projected view"
ui_mark
ui_step "click the sheet at 120, 120"      sheet_click 120 120
ui_expect_new '"type":"base"' "the base view asked for"
ui_expect_new "Drawing view 1 at" "the front view drawn"
# Its projection logged (mitcad#1033): the kernel's, then the sheet drawn
# from the cache.
projection_logged 1 1 "projected exactly"
projection_logged 1 1 "from the cache"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
ui_mark
ui_step "click right of it"                sheet_click 215 120
ui_expect_new '"type":"projected"' "a projected view asked for"
ui_expect_new "Drawing view 2 at" "the right view drawn"
no_refusal
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"

echo "--- A section and a detail, each placed by a click"
ui_mark
ui_step "Section View"                     ui_click_logged "Drawing tool Section View"
ui_expect_new "Drawing tool: Section View" "the section tool is on"
ui_step "the line's first end, on the view" sheet_click 120 110
ui_step "its second end, above the view"   sheet_click 120 150
ui_expect_new "Drawing placing view" "the line drawn: its view is placed next"
ui_expect_new "Drawing hint: Click where the section view goes" "the placing hint"
ui_step "where the section goes"           sheet_click 48 120
ui_expect_new '"type":"section"' "the section asked for"
ui_expect_new '"at":[' "where it goes"
ui_expect_new "Drawing view 3 at" "the section drawn"
ui_expect_new "Drawing view 3 selected" "and selected"
ui_mark
ui_step "Detail View"                      ui_click_logged "Drawing tool Detail View"
ui_expect_new "Drawing tool: Detail View" "the detail tool is on"
ui_step "the circle's middle, on the view" sheet_click 86 106
ui_step "its edge"                         sheet_click 96 106
ui_expect_new "Drawing placing view" "the circle drawn: its view is placed next"
ui_step "where the detail goes"            sheet_click 230 70
ui_expect_new '"type":"detail"' "the detail asked for"
ui_expect_new "Drawing view 4 at" "the detail drawn"
no_refusal
# The detail dragged up to the sheet's free corner.
ui_mark
ui_step "drag the detail up" sheet_drag "Drawing view 4" 230 170
ui_expect_new "Drawing view 4 dragged by" "the detail dragged"
ui_expect_new '"move":[' "moved on the sheet"
ui_expect_new "Drawing view 4 at" "drawn at its new place"
# A move projects nothing again (mitcad#1033).
projection_logged 4 1 "from the cache"
nothing_projected
# The detail (selected by the drag) has its circle dragged on its parent
# (mitcad#715).
ui_mark
ui_step "drag the detail's circle"         sheet_drag "Drawing detail 4 circle" 90 110
ui_expect_new "Drawing detail 4 circle dragged by" "the circle dragged"
ui_expect_new '"center":[' "the detail's centre moved"
ui_expect_new "Drawing sheet drawn: 4 views" "the detail drawn again"
no_refusal
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A break, a panel edit, Delete and Undo"
ui_mark
ui_step "Break"                            ui_click_logged "Drawing tool Break"
ui_expect_new "Drawing tool: Break" "the break tool is on"
ui_step "the band's first side, on the view" sheet_click 140 120
ui_step "its second side"                  sheet_click 150 120
ui_expect_new '"add_break":' "a break asked for"
ui_expect_new "Drawing sheet drawn: 4 views" "the front view drawn shorter"
no_refusal
ui_mark
ui_step "select the section"               ui_click_logged "Drawing view 3"
ui_expect_new "Drawing view 3 selected" "the section selected"
ui_expect_new "Drawing panel drawingViewHatch at" "its panel shows its hatching"
ui_mark
settled "Drawing panel drawingViewHatch"
ui_step "hatching off"                     ui_click_logged "Drawing panel drawingViewHatch"
ui_expect_new '"hatch":false' "the section's hatching off"
ui_expect_new "Drawing sheet drawn: 4 views" "the sheet drawn again"
no_refusal
# The panel was built again around the field with the keyboard: Esc is
# still the drawing's.
ui_mark
ui_step "Esc after the panel edit"         ui_key Escape
ui_expect_new "Drawing selection cleared" "Esc reaches the drawing after a panel edit"
ui_mark
ui_step "select the detail"                ui_click_logged "Drawing view 4"
ui_expect_new "Drawing view 4 selected" "the detail selected"
ui_step "Delete"                           ui_key Delete
ui_expect_new '"cmd":"delete_drawing_view"' "its deletion asked for"
ui_expect_new "Drawing sheet drawn: 3 views" "the detail gone"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing sheet drawn: 4 views" "the detail back, the drawing still open"
grep -q "Drawing closed" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "undo closed the drawing"

echo "--- Sheets"
ui_mark
ui_step "+ (a sheet)"                      ui_click_logged "Drawing Add Sheet"
ui_expect_new '"cmd":"add_drawing_sheet"' "a sheet asked for"
ui_expect_new "Drawing sheet drawn: 0 views" "the new sheet shown"
ui_expect_new "Drawing Sheet tab 2 at" "its tab"
# The sheets switched by a click on their tabs (mitcad#725: on Windows a
# tab click did not switch sheets).
ui_mark
settled "Drawing Sheet tab 1"
ui_step "the first sheet's tab"           ui_click_logged "Drawing Sheet tab 1"
ui_expect_new "Drawing sheet tab pressed: 1" "the press on its tab"
ui_expect_new "Drawing sheet 1 shown" "the first sheet shown"
ui_expect_new "Drawing sheet drawn: 4 views" "its four views"
ui_mark
settled "Drawing Sheet tab 2"
ui_step "the second sheet's tab"          ui_click_logged "Drawing Sheet tab 2"
ui_expect_new "Drawing sheet 2 shown" "the second sheet shown"
ui_expect_new "Drawing sheet drawn: 0 views" "its empty sheet"
ui_mark
settled "Drawing Sheet tab 2"
ui_step "its tab's menu"                   ui_click_logged "Drawing Sheet tab 2" 3
ui_expect_new "Drawing sheet menu: sheet 2" "the tab's menu"
ui_step "Rename… (Down, Enter)"            ui_key Down Return
ui_focus_dialog '^Rename Sheet$'
ui_step "type Details"                     ui_type_text "Details"
ui_key Return
ui_focus_main
ui_expect_new '"name":"Details"' "the sheet renamed"
no_refusal
ui_mark
settled "Drawing Sheet tab 2"
ui_step "its tab's menu again"             ui_click_logged "Drawing Sheet tab 2" 3
ui_expect_new "Drawing sheet menu: sheet 2" "the tab's menu"
ui_step "Delete Sheet (Down, Down, Enter)" ui_key Down Down Return
ui_expect_new '"cmd":"delete_drawing_sheet"' "its deletion asked for"
ui_expect_new "Drawing sheet drawn: 4 views" "back on the first sheet"
ui_mark
ui_step "Sheet…"                           ui_click_logged "Drawing Sheet"
ui_expect_new "Drawing sheet dialog opened: A3" "the sheet's dialog"
ui_focus_dialog '^Sheet$'
# The sheet's units (mitcad#725): inches, then the design's again.
ui_step "Units: Inches (Alt+U, i)"         ui_key alt+u i
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new '"cmd":"edit_drawing_sheet"' "the sheet's fields sent"
ui_expect_new '"units":"in"' "in inches"
no_refusal
ui_mark
ui_step "Sheet… again"                     ui_click_logged "Drawing Sheet"
ui_expect_new "Drawing sheet dialog opened: A3" "the sheet's dialog"
ui_focus_dialog '^Sheet$'
ui_step "Units: The Design's (Alt+U, Home)" ui_key alt+u Home
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new '"units":""' "the design's units again"
no_refusal

echo "--- Dimensions, a centre mark and a note on the views (phase 2)"
# The front view at 2:1: the block's bottom edge at y 100, its top edge at
# y 140 on the sheet, its left side at x 80; the hole's axis in the middle
# of the section view, its wall 10 mm of paper either side.
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_expect_new "Drawing hint: Click a straight edge for its length, or the edges, circles or corners to dimension" "its hint"
# A click that picks nothing is a hint, not an error (mitcad#1174): off the
# views, and on the front view away from its edges.
ui_mark
ui_step "the empty sheet"                  sheet_click 120 220
ui_expect_new "Message band: hint Click on a view." "off the views: a hint"
ui_step "the front view, off its edges"    sheet_click 100 128
ui_expect_new "Drawing pick: nothing at" "nothing picked on the view"
ui_expect_new "Message band: hint Nothing to pick there: " "a missed pick: a hint"
grep -qE "^(Status error|Message band: error) (Click on a view|Nothing to pick)" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "a missed pick said as an error"
echo "ok   no missed pick said as an error"
# What a click would pick is highlighted under the pointer (mitcad#725).
ui_step "point at the bottom edge"         sheet_hover 95 100.5
ui_expect_new "Drawing hover: line" "the edge under the pointer highlighted"
ui_step "the front view's bottom edge"     sheet_click 95 100.5
ui_expect_new "Drawing picked line (1)" "the bottom edge picked"
ui_step "its top edge"                     sheet_click 95 139.5
ui_expect_new "Drawing picked line (2)" "the top edge picked"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_expect_new "Drawing preview: dimension" "its lines follow the pointer while it is placed"
ui_step "where the dimension goes"         sheet_click 172 120
ui_expect_new '"type":"dimension"' "a dimension asked for, its picks choosing its type"
ui_expect_new "Drawing annotations drawn: 1 = 20" "drawn: the block is 20 high"
no_refusal
# The hover's question waits while a job computes the model (mitcad#753):
# the dimension undone, then redone (1.5 s longer, MITCAD_TEST_JOB_SLEEP_MS)
# right after the pointer moved onto an edge, so that the hover's timer
# fires while the window waits for the redo.
ui_mark
ui_step "undo the dimension (Ctrl+Z)"      ui_key ctrl+z
ui_expect_new "Drawing annotations drawn: none" "the dimension undone, the tool still on"
ui_step "point at the empty sheet"         sheet_hover 200 75
hover_is nothing "nothing under the pointer"
ui_mark
ui_step "onto the bottom edge, then redo (Ctrl+Y)" hover_then_redo 95 100.5
ui_expect_new "Drawing annotations drawn: 1 = 20" "the dimension redone" 10
ui_wait_idle
ui_expect_new "Drawing hover: line" "the edge under the pointer asked for once the redo was done"
grep -qF "The document was read while a job computes it" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the hover read the model while the redo computed it"
ui_crashed && ui_fail "crashed while the hover waited for the redo"
# A diameter of the hole, seen from the side in the section.
ui_mark
settled "Drawing panel drawingOption_type"
ui_step "Type: Diameter"                   ui_click_logged "Drawing panel drawingOption_type"
ui_key End Return
ui_expect_new "Drawing option type: diameter" "a diameter next"
ui_step "the hole's wall in the section"   shifted_click "Drawing view 3" 10 0
ui_expect_new "Drawing picked axis (1)" "the hole's round face picked"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "above the section"                shifted_click "Drawing view 3" 0 26
ui_expect_new '"type":"diameter"' "a diameter asked for"
ui_expect_new "2 = Ø10" "drawn: the hole is 10 across"
no_refusal
ui_mark
ui_step "Center Mark"                      ui_click_logged "Drawing tool Center Mark"
ui_expect_new "Drawing tool: Center Mark" "the centre mark tool is on"
ui_step "the hole's wall"                  shifted_click "Drawing view 3" -10 0
ui_expect_new '"type":"center_mark"' "a centre mark asked for"
ui_expect_new "3 = center_mark" "its centreline drawn"
no_refusal
ui_mark
ui_step "Note"                             ui_click_logged "Drawing tool Note"
ui_expect_new "Drawing tool: Note" "the note tool is on"
settled "Drawing panel drawingOption_text"
ui_step "its text"                         ui_click_logged "Drawing panel drawingOption_text"
xdotool type --delay 40 -- "DEBURR"
ui_expect_new "Drawing option text: DEBURR" "the text typed"
ui_step "the front view's left side"       sheet_click 80.5 120
ui_expect_new "Drawing picked line (1)" "the side picked"
ui_step "where the text goes"              sheet_click 60 165
ui_expect_new '"type":"note"' "a note asked for"
ui_expect_new "4 = DEBURR" "the note drawn"
no_refusal
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
# The dimension takes a ± tolerance from #702's dialog.
ui_mark
ui_step "select the dimension"             ui_click_logged "Drawing annotation 1"
ui_expect_new "Drawing annotation 1 selected" "the dimension selected"
ui_expect_new "Drawing panel drawingAnnotationTolerance at" "its panel"
settled "Drawing panel drawingAnnotationTolerance"
ui_step "Add Tolerance…"                   ui_click_logged "Drawing panel drawingAnnotationTolerance"
ui_expect_new "Annotation Tolerance dialog opened" "the annotations' tolerance dialog"
ui_focus_dialog '^Annotation Tolerance$'
ui_step "Tolerance: ± (Alt+T, Down)"       ui_key alt+t Down
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new '"tolerance":{"type":"symmetric","value":0.1}' "± 0.1 asked for"
ui_expect_new "1 = 20 ±0.1" "drawn with its tolerance"
no_refusal
# Dragged: its line moves out.
ui_mark
ui_step "drag the dimension"               sheet_drag "Drawing annotation 1" 182 120
ui_expect_new "Drawing annotation 1 dragged by" "the dimension dragged"
ui_expect_new '"cmd":"edit_drawing_annotation"' "moved, an undo step"
no_refusal
ui_step "Esc"                              ui_key Escape

echo "--- Several selected, the sheet's menu, a note's text (mitcad#725)"
ui_mark
ui_step "select the dimension"             ui_click_logged "Drawing annotation 1"
ui_expect_new "Drawing annotation 1 selected" "the dimension selected"
ui_step "Ctrl+click the note"              ctrl_click_logged "Drawing annotation 4"
ui_expect_new "Drawing selection: annotation 1, annotation 4" "both selected"
ui_expect_new "Drawing panel drawingSelectionDelete at" "the panel of several items"
ui_mark
ui_step "drag them together"               sheet_drag "Drawing annotation 4" 66 170
ui_expect_new "Drawing selection dragged by" "both dragged"
ui_expect_new '"cmd":"edit_drawing_annotation","drawing":"D1","move"' "moved by one command each"
grep -q '"annotation":1,"cmd":"edit_drawing_annotation"\|"annotation":1.*"annotation":4\|"annotation":4.*"annotation":1' \
  <(tail -n +$((UI_MARK + 1)) "$UI_LOG") || ui_fail "the dimension and the note were not moved together"
no_refusal
ui_mark
ui_step "Delete both"                      ui_key Delete
ui_expect_new '"cmd":"delete_drawing_annotation"' "their deletion asked for"
ui_expect_new '"annotations":[1,4]' "both, in one step"
ui_expect_new "Drawing annotations drawn: 2 = Ø10 | 3 = center_mark" "the dimension and the note gone"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "1 = 20 ±0.1" "both back with one undo"
ui_expect_new "4 = DEBURR" "the note back"
# Nothing is selected after the undo: the dimension selected, Esc clears it.
ui_mark
settled "Drawing annotation 1"
ui_step "select the dimension"             ui_click_logged "Drawing annotation 1"
ui_expect_new "Drawing annotation 1 selected" "the dimension selected"
ui_mark
ui_step "Esc"                              ui_key Escape
ui_expect_new "Drawing selection cleared" "Esc clears the selection"
# Esc with nothing to end leaves the drawing open (the owner, mitcad#725).
ui_mark
ui_step "Esc again"                        ui_key Escape
ui_expect_new "Drawing Esc: nothing to end" "the drawing stays"
grep -q "Drawing closed" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "Esc closed the drawing"
# The sheet's menu on the note: its text in the note dialog.
ui_mark
settled "Drawing annotation 4"
ui_step "right-click the note"             ui_click_logged "Drawing annotation 4" 3
ui_expect_new "Context menu: Edit Text… | Delete" "the note's menu"
ui_step "choose Edit Text…"                ui_menu_choose "Edit Text…"
ui_expect_new "Note dialog opened: Note 4" "the note's text in its dialog"
ui_focus_dialog '^Note$'
ui_step "cancel (Esc)"                     ui_key Escape
ui_focus_main
# A double-click on the note opens it too.
ui_double_click_expect "Drawing annotation 4" "Note dialog opened: Note 4" "a double-click opens the note's text"
ui_focus_dialog '^Note$'
ui_step "cancel (Esc)"                     ui_key Escape
ui_focus_main
# The menu of a view: its tools and its panel's check boxes. The view's middle
# lies on one of its edges, so Edit Line… comes first (mitcad#1546).
ui_mark
ui_step "Esc"                              ui_key Escape
ui_step "right-click the projected view"   ui_click_logged "Drawing view 2" 3
ui_expect_new "Context menu: Edit Line… | Projected View | Section View | Detail View | Break | Rotate 90° Left | Rotate 90° Right | Hidden Edges | Tangent Edges | Shaded | Center Marks | Delete View" "the view's menu"
ui_step "choose Shaded"                    ui_menu_choose "Shaded"
ui_expect_new '"shaded":true' "the view shaded"
no_refusal
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing sheet drawn: 4 views" "not shaded again"
ui_step "Esc"                              ui_key Escape

echo "--- A hole table on a top view (phase 3)"
# The top view at 1:1 above the front view's right half: the block's
# 40 x 30 at 110..150 x 165..195 on the sheet, the hole at 20, 15 of it.
ui_mark
ui_step "Base View"                        ui_click_logged "Drawing tool Base View"
ui_expect_new "Drawing tool: Base View" "the tool is on"
settled "Drawing panel drawingOption_direction"
ui_step "Orientation: Top"                 ui_click_logged "Drawing panel drawingOption_direction"
ui_key Down Return
ui_expect_new "Drawing option direction: top" "a top view next"
settled "Drawing panel drawingOption_scale"
ui_step "Scale: 1:1"                       ui_click_logged "Drawing panel drawingOption_scale"
ui_key Down Down Down Down Return
ui_expect_new "Drawing option scale: 1:1" "at 1:1"
ui_mark
ui_step "click the sheet at 130, 180"      sheet_click 130 180
ui_expect_new '"direction":"top"' "a top view asked for"
ui_expect_new "Drawing view 5 at" "the top view drawn"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
no_refusal
ui_mark
ui_step "Hole Table"                       ui_click_logged "Drawing tool Hole Table"
ui_expect_new "Drawing tool: Hole Table" "the hole table tool is on"
ui_expect_new "Drawing hint: Click the view whose holes the table lists" "its hint"
ui_step "the top view"                     ui_click_logged "Drawing view 5"
ui_expect_new "Drawing picked view 5" "the view picked"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where the table goes"             sheet_click 25 85
ui_expect_new '"type":"hole_table"' "a hole table asked for"
ui_expect_new "5 = A1 20 15 Ø10 THRU" "the hole tagged A1 and listed from the view's corner"
ui_expect_new "Drawing annotation 5 selected" "the table selected, its panel shown"
ui_expect_new "Drawing panel drawingTableTags at" "its settings in the panel"
no_refusal

echo "--- Another sheet shown: the tool's picks stay behind; an earlier sheet deleted (mitcad#759)"
# A second sheet with a top view at 1:1 (Base View's options as last
# chosen) around 150, 100: the hole's circle (radius 5) in its middle.
ui_mark
settled "Drawing Add Sheet"
ui_step "+ (a second sheet)"               ui_click_logged "Drawing Add Sheet"
ui_expect_new '"cmd":"add_drawing_sheet"' "a sheet asked for"
ui_expect_new "Drawing sheet drawn: 0 views" "the new sheet shown"
ui_mark
settled "Drawing tool Base View"
ui_step "Base View"                        ui_click_logged "Drawing tool Base View"
ui_expect_new "Drawing tool: Base View" "the tool is on"
ui_step "click the sheet at 150, 100"      sheet_click 150 100
ui_expect_new '"direction":"top"' "a top view asked for"
ui_expect_new "Drawing sheet drawn: 1 views" "the top view drawn on the second sheet"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
no_refusal
second=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Drawing view [0-9]* at" | tail -1 | awk '{print $3}')
[ -n "$second" ] || ui_fail "no place of the second sheet's view"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
ui_mark
settled "Drawing Sheet tab 1"
ui_step "the first sheet's tab"            ui_click_logged "Drawing Sheet tab 1"
ui_expect_new "Drawing sheet 1 shown" "the first sheet shown"
ui_expect_new "Drawing sheet drawn: 5 views" "its five views"
# Dimension, a diameter as last chosen: one pick on the first sheet, then
# the second sheet's tab.
ui_mark
settled "Drawing tool Dimension"
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
settled "Drawing view 3"
ui_step "the hole's wall in the section"   shifted_click "Drawing view 3" 10 0
ui_expect_new "Drawing picked axis (1)" "the hole's round face picked on the first sheet"
ui_expect_new "Drawing placing view" "its place would be clicked next"
ui_mark
settled "Drawing Sheet tab 2"
ui_step "the second sheet's tab"           ui_click_logged "Drawing Sheet tab 2"
ui_expect_new "Drawing sheet 2 shown" "the second sheet shown"
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"
ui_expect_new "Drawing view $second at" "the top view placed"
# The pick left behind: the next click is a pick on this sheet, not the
# first sheet's placing click; a dimension on this sheet without Esc.
ui_mark
settled "Drawing view $second"
ui_step "the hole's circle, off its centre marks" shifted_click "Drawing view $second" 3.54 3.54
ui_expect_new "Drawing picked " "the circle picked on the second sheet"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "above the view"                   shifted_click "Drawing view $second" 0 25
ui_expect_new '"type":"diameter"' "a diameter asked for"
ui_expect_new "\"view\":$second" "on the second sheet's view"
ui_expect_new "= Ø10" "drawn on the second sheet: the hole is 10 across"
no_refusal
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
# A third sheet shown, the second deleted from its tab's menu: the third
# stays shown, now the second.
ui_mark
settled "Drawing Add Sheet"
ui_step "+ (a third sheet)"                ui_click_logged "Drawing Add Sheet"
ui_expect_new "Drawing sheet drawn: 0 views" "the third sheet shown"
ui_expect_new "Drawing Sheet tab 3 at" "its tab"
ui_mark
settled "Drawing Sheet tab 2"
ui_step "the second sheet's tab menu"      ui_click_logged "Drawing Sheet tab 2" 3
ui_expect_new "Drawing sheet menu: sheet 2" "the second sheet's menu"
ui_step "Delete Sheet (Down, Down, Enter)" ui_key Down Down Return
ui_expect_new '"cmd":"delete_drawing_sheet"' "its deletion asked for"
ui_expect_new "Drawing sheet 2 shown" "the sheet shown stays shown, now the second"
no_refusal
ui_sync
shown=$(grep -o "Drawing sheet drawn: [0-9]* views" "$UI_LOG" | tail -1)
[ "$shown" = "Drawing sheet drawn: 0 views" ] || ui_fail "another sheet shown after the delete: $shown"
echo "ok   the empty third sheet still shown"
# It deleted too, from its own tab: the first sheet again.
ui_mark
settled "Drawing Sheet tab 2"
ui_step "its tab's menu"                   ui_click_logged "Drawing Sheet tab 2" 3
ui_expect_new "Drawing sheet menu: sheet 2" "the tab's menu"
ui_step "Delete Sheet (Down, Down, Enter)" ui_key Down Down Return
ui_expect_new "Drawing sheet 1 shown" "the first sheet shown"
ui_expect_new "Drawing sheet drawn: 5 views" "its five views"
no_refusal

echo "--- The model changes, the drawing follows"
before=$(view_y 1)
[ -n "$before" ] || ui_fail "no place of the front view"
ui_mark
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
# Solid and Build back, Solid shown, as before the drawing (mitcad#1008).
ui_expect_new "Ribbon tabs: START, SOLID, BUILD, MAKE (shown SOLID)" "the model's tabs back, the one it was opened from shown"
ui_mark
ui_step "edit Extrude1 (double-click)"     ui_double_click_logged "Timeline Extrude1"
ui_expect_new " with Extrude" "editing Extrude1"
ui_step "distance 35"                      ui_type_in "Panel Extrude input distance" "35"
ui_wait_idle
ui_focus_main
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F" "Extrude1 edited"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened again"
ui_expect_new "Drawing sheet drawn: 5 views" "its five views"
ui_expect_new "Drawing view 1 at" "the front view placed"
after=$(view_y 1)
# 15 mm higher on the paper at 2:1: its middle 15 mm of the sheet up.
[ "$after" -lt "$before" ] || ui_fail "the front view did not grow upward ($before -> $after)"
echo "ok   the front view grew upward ($before -> $after)"
# The dimension follows its edges.
ui_expect_new "1 = 35 ±0.1" "the dimension follows the block: 35 high"
annotations_drawn | grep -qF "2 = Ø10" || ui_fail "the hole's diameter is not drawn: $(annotations_drawn)"
annotations_drawn | grep -qF "5 = A1 20 15 Ø10 THRU" || ui_fail "the hole table lost its hole: $(annotations_drawn)"

echo "--- A dimension whose geometry is gone is marked broken"
ui_mark
ui_step "back to the model (Close Drawing)" ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "closed"
ui_focus_main
ui_mark
ui_step "Extrude2's menu (the hole)"       ui_click_logged "Timeline Extrude2" 3
ui_step "choose Suppress Features"         ui_menu_choose "Suppress Features"
ui_expect_new "Suppressed Extrude2" "the hole suppressed"
# Opened from the Build tab, the drawing closes back to it (mitcad#1008).
ui_mark
ui_step "the Build tab"                    ui_click_logged "Ribbon tab BUILD"
ui_expect_new "Ribbon BUILD/PROFILE at" "the Build tab shown"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened again"
ui_expect_new "Ribbon tabs: DRAWING (shown DRAWING)" "the DRAWING tab alone"
ui_expect_new "Drawing annotations drawn:" "its annotations"
annotations_drawn | grep -qF "2 broken" || ui_fail "the diameter of the hole that is gone is not broken: $(annotations_drawn)"
annotations_drawn | grep -qF "1 = 35 ±0.1" || ui_fail "the dimension of the block broke too: $(annotations_drawn)"
# The hole table follows the model: it lists no hole now.
annotations_drawn | grep -qF "5 = no holes" || ui_fail "the hole table still lists the hole: $(annotations_drawn)"
echo "ok   the hole's diameter broken, the hole table empty: $(annotations_drawn)"
ui_expect_new "Drawing annotation 2 at" "the broken one keeps a box to be picked"

echo "--- Saved and opened again"
ui_mark
ui_step "back to the model (Close Drawing)" ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "closed"
ui_expect_new "Ribbon tabs: START, SOLID, BUILD, MAKE (shown BUILD)" "back on the Build tab it was opened from"
ui_focus_main
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_log "Saved $FILE" "saved"
grep -q '"drawings"' "$FILE" || ui_fail "the design file has no drawings"
ui_stop_app
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "block.mitcad" "the design opened again"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet drawn: 5 views" "its five views kept"
ui_expect_new "1 = 35 ±0.1" "its annotations kept"

echo "--- Export a PDF"
ui_mark
ui_step "Export…"                          ui_click_logged "Drawing Export"
ui_step "PDF… (Down, Enter)"               ui_key Down Return
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/drawing.pdf"
ui_focus_main
ui_expect_new "Exported $WORK/drawing.pdf: drawing Drawing1, 1 sheets" "the PDF written" 30
[ "$(head -c 8 "$WORK/drawing.pdf")" = "%PDF-1.4" ] || ui_fail "no PDF file"
grep -aq "/Type /Page " "$WORK/drawing.pdf" || ui_fail "the PDF has no page"
echo "ok   the PDF has its page"
grep -aq "(DEBURR)" "$WORK/drawing.pdf" || ui_fail "the PDF has no note"
grep -aq "(35 " "$WORK/drawing.pdf" || ui_fail "the PDF has no dimension of the block"
echo "ok   the PDF has the note and the dimension"

echo "--- Print (mitcad#1089)"
ui_mark
ui_expect_log "Print available: drawing Drawing1" "File › Print prints the open drawing"
ui_step "Print (Ctrl+P)"                   ui_key ctrl+p
ui_expect_new "Print dialog skipped (test): printing to $PRINTED" "the test's PDF in place of the dialog"
ui_expect_new "Printed drawing Drawing1: 1 page(s), sheets " "the sheet printed on a page of its size" 30
size=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Printed drawing Drawing1: 1 page(s), sheets \([0-9.]*\) x \([0-9.]*\) mm to .*/\1 \2/p' | tail -1)
[ -n "$size" ] || ui_fail "the print did not log its sheet's size"
[ "$(grep -aoE '/Type */Page([^s]|$)' "$PRINTED" | wc -l | tr -d ' ')" = 1 ] || ui_fail "the printed PDF has not one page"
read -r sheet_w sheet_h <<< "$size"
grep -aoE '/MediaBox *\[ *0 +0 +[0-9.]+ +[0-9.]+ *\]' "$PRINTED" | head -1 |
  sed -E 's/.*\[ *0 +0 +([0-9.]+) +([0-9.]+) *\]/\1 \2/' |
  awk -v w="$sheet_w" -v h="$sheet_h" '{ dw = $1 - w * 72 / 25.4; dh = $2 - h * 72 / 25.4
    exit !(dw > -1 && dw < 1 && dh > -1 && dh < 1) }' ||
  ui_fail "the printed page is not the sheet's size ($size mm)"
echo "ok   the printed PDF has one page of the sheet's size ($size mm)"

echo "--- Export a DWG (mitcad#728)"
ui_mark
ui_step "Export…"                          ui_click_logged "Drawing Export"
ui_step "DWG… (Down x4, Enter)"            ui_key Down Down Down Down Return
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/drawing.dwg"
ui_focus_main
ui_expect_new "Exported $WORK/drawing.dwg: drawing Drawing1, 1 sheets" "the DWG written" 30
[ "$(head -c 6 "$WORK/drawing.dwg")" = "AC1015" ] || ui_fail "no R2000 DWG file"
[ "$(stat -c %s "$WORK/drawing.dwg")" -gt 4096 ] || ui_fail "the DWG holds no sheet"
echo "ok   the DWG is an R2000 drawing"

echo "--- The template's title block follows the design; a revision (phase 4)"
ui_mark
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_focus_main
ui_mark
ui_step "right-click Body1"                ui_click_logged "Browser Root/Bodies/Body1" 3
ui_step "choose Material > More..."     ui_menu_choose "Material" "More..."
ui_expect_new "Panel Material input material at" "the Material tool is open"
ui_step "the picker: PLA Plastic"          ui_picker_choose "Panel Material input material" "Material picker" "PLA Plastic"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Material pla set for 1 body(ies)" "Body1 is PLA"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened again"
ui_expect_new "MATERIAL=PLA" "the title block's material follows the body's"
ui_expect_new "REV=-" "no revision yet"
settled "Drawing Add Revision"
ui_mark
ui_step "Add Revision"                     ui_click_logged "Drawing Add Revision"
ui_expect_new "Drawing revision dialog opened: Add Revision" "its dialog"
ui_focus_dialog '^Add Revision$'
ui_step "its description"                  ui_type_text "First release"
ui_key Return
ui_focus_main
ui_expect_new '"cmd":"add_drawing_revision"' "a revision asked for"
ui_expect_new "Drawing revisions: A First release" "the drawing's revision table has it"
ui_expect_new "REV=A" "the title block shows the latest revision"
ui_expect_new "Drawing panel drawingRevision_A at" "the sheet's panel lists it"
no_refusal
ui_mark
ui_step "Export…"                          ui_click_logged "Drawing Export"
ui_step "PDF… (Down, Enter)"               ui_key Down Return
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/revised.pdf"
ui_focus_main
ui_expect_new "Exported $WORK/revised.pdf: drawing Drawing1, 1 sheets" "the PDF written" 30
grep -aq "(First release)" "$WORK/revised.pdf" || ui_fail "the PDF has no revision table"
grep -aq "(MATERIAL)" "$WORK/revised.pdf" || ui_fail "the PDF has no template's title block"
grep -aq "(PLA" "$WORK/revised.pdf" || ui_fail "the PDF's title block has no material"
echo "ok   the PDF has the template's title block and the revision"

echo "--- A parts list and balloons on an assembly (phase 3)"
# Component1 (30 x 20 x 5) placed at the origin and 40 along X, Component2
# (10 x 10 x 10) at y 30 once. The front view at 1:1 with its middle at
# 120, 120: x 0..70 at 85..155, z 0..10 at 115..125 on the sheet.
ASSEMBLY="$WORK/assembly.mitcad"
cat > "$WORK/assembly.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_component"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 30], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_component"}},
  {"cmd": "copy_occurrence", "occurrence": "Component1:1", "transform": {"translation": [40, 0, 0]}}
]
EOF
"$CLI" run "$WORK/assembly.json" --save "$ASSEMBLY" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the assembly)"; }
ui_stop_app
# The assembly's part runs in the Floating layout (mitcad#725): the
# drawing shows the same inside the cards over the 3D view.
ui_start_app --open "$ASSEMBLY" --no-native-dialogs --chrome=floating
ui_expect_log "assembly.mitcad" "the assembly opened"
ui_expect_log "Browser Root/Component1:2 at" "the browser lists Component1's second placement"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing tool Bill of Materials at" "the tables' tools on the ribbon"
ui_expect_new "Drawing tool: Base View" "Base View on for the new drawing"
# In the Floating layout too (mitcad#1008): the DRAWING tab's group cards,
# the sheet over the whole view, of the cards only the browser's.
ui_expect_new "Ribbon tabs: DRAWING (shown DRAWING)" "the DRAWING tab alone"
area_is_view
floating_cards without timelineDock
floating_cards with browserDock
floating_cards with ribbonGroup_DRAWING/CLOSE_DRAWING
ui_mark
ui_step "click the sheet at 120, 120"      sheet_click 120 120
ui_expect_new "Drawing view 1 at" "the front view drawn"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
# A parts list places its balloons, on by default (mitcad#725).
ui_mark
settled "Drawing tool Bill of Materials"
ui_step "Bill of Materials"                       ui_click_logged "Drawing tool Bill of Materials"
ui_expect_new "Drawing tool: Bill of Materials" "the parts list tool is on"
ui_expect_new "Drawing panel drawingOption_balloons at" "its Balloons option"
ui_step "the front view"                   ui_click_logged "Drawing view 1"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where the list goes"              sheet_click 150 190
ui_expect_new '"balloons":true' "a parts list with its balloons asked for"
ui_expect_new "1 = 1 Component1 x2; 2 Component2 x1 | 2 = " "the list and its balloons"
annotations_drawn | grep -qE '\| 2 = [12] \| 3 = [12]$' || ui_fail "not a balloon per row: $(annotations_drawn)"
echo "ok   a balloon per row: $(annotations_drawn)"
no_refusal
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing annotations drawn: none" "the list and its balloons gone in one step"
# Without them, the balloon placed by hand.
ui_mark
settled "Drawing tool Bill of Materials"
ui_step "Bill of Materials again"                 ui_click_logged "Drawing tool Bill of Materials"
ui_expect_new "Drawing tool: Bill of Materials" "the parts list tool is on"
settled "Drawing panel drawingOption_balloons"
ui_step "Balloons off"                     ui_click_logged "Drawing panel drawingOption_balloons"
ui_expect_new "Drawing option balloons: off" "no balloons"
ui_step "the front view"                   ui_click_logged "Drawing view 1"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where the list goes"              sheet_click 150 190
ui_expect_new '"type":"parts_list"' "a parts list asked for"
ui_expect_new "1 = 1 Component1 x2; 2 Component2 x1" "sorted, numbered and counted"
grep -q '"balloons":true' <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "balloons asked for with the option off"
no_refusal
ui_mark
settled "Drawing tool Balloon"
ui_step "Balloon"                          ui_click_logged "Drawing tool Balloon"
ui_expect_new "Drawing tool: Balloon" "the balloon tool is on"
ui_step "Component1:2's right side"        sheet_click 154.8 117.5
ui_expect_new "Drawing picked line (1)" "its edge picked"
ui_expect_new "Drawing placing view" "its place is clicked next"
ui_step "where the balloon goes"           sheet_click 175 150
ui_expect_new '"type":"balloon"' "a balloon asked for"
ui_expect_new "| 2 = 1" "the balloon shows Component1's number"
no_refusal
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
# The front view dragged with its parts list selected: the list, which
# stands on the sheet whatever its view's place, moves by the drag too
# (mitcad#752); undone.
ui_mark
settled "Drawing view 1"
ui_step "select the front view"            ui_click_logged "Drawing view 1"
ui_expect_new "Drawing view 1 selected" "the front view selected"
ui_step "Ctrl+click the parts list"        ctrl_click_logged "Drawing annotation 1"
ui_expect_new "Drawing selection: view 1, annotation 1" "the view and its parts list selected"
read -r list_x list_y <<< "$(grep -o "Drawing annotation 1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //; s/,/ /')"
read -r view_x view_y <<< "$(grep -o "Drawing view 1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //; s/,/ /')"
[ -n "$list_y" ] && [ -n "$view_y" ] || ui_fail "no places of the front view and its parts list"
ui_mark
ui_step "drag them 20 mm down"             sheet_drag "Drawing view 1" 120 100
ui_expect_new "Drawing selection dragged by" "both dragged"
ui_expect_new '"cmd":"edit_drawing_annotation"' "the parts list moved by a command of its own"
ui_expect_new "Drawing view 1 at" "the front view drawn at its new place"
no_refusal
ui_sync
read -r moved_x moved_y <<< "$(grep -o "Drawing annotation 1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //; s/,/ /')"
read -r now_x now_y <<< "$(grep -o "Drawing view 1 at [0-9]*,[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //; s/,/ /')"
# Both moved the same way down the window (y grows downward), about as far.
[ $((now_y - view_y)) -gt 5 ] || ui_fail "the front view did not move down ($view_y -> $now_y)"
[ $((moved_y - list_y)) -gt 5 ] || ui_fail "the parts list stayed behind ($list_y -> $moved_y, the view $view_y -> $now_y)"
gap=$(((moved_y - list_y) - (now_y - view_y)))
[ "${gap#-}" -le 3 ] && [ $((moved_x - list_x)) -le 3 ] && [ $((list_x - moved_x)) -le 3 ] ||
  ui_fail "the parts list did not move as the view did: list $list_x,$list_y -> $moved_x,$moved_y, view $view_y -> $now_y"
echo "ok   the parts list moved with the view: $list_x,$list_y -> $moved_x,$moved_y"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing view 1 at $view_x,$view_y" "the front view back"
ui_expect_new "Drawing annotation 1 at $list_x,$list_y" "the parts list back with one undo"
ui_step "Esc"                              ui_key Escape
ui_mark
ui_step "Export…"                          ui_click_logged "Drawing Export"
ui_step "SVG… (Down, Down, Enter)"         ui_key Down Down Return
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/assembly.svg"
ui_focus_main
ui_expect_new "Exported $WORK/assembly.svg: drawing Drawing1, 1 sheets" "the SVG written" 30
grep -q "BILL OF MATERIALS" "$WORK/assembly.svg" || ui_fail "the SVG has no parts list"
grep -q "Component2" "$WORK/assembly.svg" || ui_fail "the SVG's parts list has no Component2"
echo "ok   the SVG has the parts list"
# The second placement deleted: one Component1, its balloon broken.
ui_mark
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_focus_main
ui_mark
ui_step "right-click Component1:2"         ui_click_logged "Browser Root/Component1:2" 3
ui_step "choose Delete"                    ui_menu_choose "Delete"
ui_expect_new "Deleted Component1:2" "the placement deleted"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened again"
ui_expect_new "1 = 1 Component1 x1; 2 Component2 x1" "the parts list follows the design"
annotations_drawn | grep -qF "2 broken" || ui_fail "the balloon of the deleted placement is not broken: $(annotations_drawn)"
echo "ok   the balloon broken: $(annotations_drawn)"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "1 = 1 Component1 x2; 2 Component2 x1 | 2 = 1" "the placement and the balloon back"

echo "--- File › Export Drawing writes the drawing it made again, as it is (mitcad#725)"
ui_mark
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
# The model's tabs and the cards back (mitcad#1008).
ui_expect_new "Ribbon tabs: START, SOLID, BUILD, MAKE (shown SOLID)" "Solid and Build back"
floating_cards with timelineDock
ui_focus_main
ui_mark
ui_step "Export Drawing (search)"          ui_command "Export Drawing"
ui_expect_new "Drawing dialog opened: " "its dialog"
# The design's only drawing, made in the drawing workspace and never laid
# out by this dialog, comes first (mitcad#1680).
ui_expect_new ", drawing D1" "the design's only drawing first: none laid out yet"
ui_focus_dialog "$(ui_title_regex "Export Drawing")"
ui_step "type the path"                    ui_type_field "Export Drawing" "$WORK/first.pdf"
ui_mark
ui_step "Drawing: New Drawing (Alt+W, End)" ui_key alt+w End
ui_expect_new "Drawing dialog: drawing = new" "New Drawing chosen"
ui_step "OK (Enter)"                       ui_key Return
ui_focus_main
ui_expect_new "Drawing kept: D2 Drawing2" "a drawing laid out and kept"
ui_expect_new "Exported $WORK/first.pdf: drawing" "the PDF written" 30
# A note added by hand to the drawing the export made.
ui_mark
ui_step "open Drawing2 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing2"
ui_expect_new "Drawing opened: D2 Drawing2" "the drawing the export made"
ui_mark
settled "Drawing tool Text"
ui_step "Text"                             ui_click_logged "Drawing tool Text"
ui_expect_new "Drawing tool: Text" "the text tool is on"
settled "Drawing panel drawingOption_text"
ui_step "its text"                         ui_click_logged "Drawing panel drawingOption_text"
xdotool type --delay 40 -- "BY HAND"
ui_expect_new "Drawing option text: BY HAND" "the text typed"
ui_step "where the text goes"              sheet_click 30 40
ui_expect_new '"type":"text"' "a text note asked for"
no_refusal
ui_mark
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_focus_main
ui_mark
ui_step "Export Drawing again (search)"    ui_command "Export Drawing"
ui_expect_new ", drawing D2" "the drawing it laid out last offered"
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/second.pdf"
ui_focus_main
ui_expect_new "Drawing exported as it is: D2" "the same drawing written as it is"
ui_expect_new "Exported $WORK/second.pdf: drawing" "the PDF written" 30
grep -q "Drawing kept: D3" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a new drawing on the second export"
grep -aq "(BY HAND)" "$WORK/second.pdf" || ui_fail "the note added by hand is not in the second PDF"
echo "ok   the note made by hand kept and written"

echo "--- File › Export Drawing, DXF: the unit the dialog shows, not the sheet's (mitcad#1610)"
# Drawing2's sheet set to inches; the design is in millimetres. The
# dialog's DXF unit is the design's (mm): the file is written in mm.
ui_mark
ui_step "open Drawing2 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing2"
ui_expect_new "Drawing opened: D2 Drawing2" "Drawing2 opened"
ui_mark
settled "Drawing Sheet"
ui_step "Sheet…"                           ui_click_logged "Drawing Sheet"
ui_expect_new "Drawing sheet dialog opened: " "the sheet's dialog"
ui_focus_dialog '^Sheet$'
ui_step "Units: Inches (Alt+U, i)"         ui_key alt+u i
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new '"units":"in"' "the sheet in inches"
no_refusal
ui_mark
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_focus_main
ui_mark
ui_step "Export Drawing (search)"          ui_command "Export Drawing"
ui_expect_new ", drawing D2" "Drawing2 offered"
ui_step "type the path, Enter"             ui_type_path "Export Drawing" "$WORK/inches-sheet.dxf"
ui_focus_main
ui_expect_new "Drawing exported as it is: D2" "Drawing2 written as it is"
ui_expect_new "Exported $WORK/inches-sheet.dxf: drawing" "the DXF written" 30
ui_expect_new ", in mm" "in the unit the dialog showed"
tr -d '\r ' < "$WORK/inches-sheet.dxf" | grep -A2 -x '$INSUNITS' | sed -n 3p | grep -qx 4 ||
  ui_fail "the DXF's \$INSUNITS is not millimetres (4)"
echo "ok   the DXF in millimetres although its sheet is in inches"

echo "--- A cut sheet (mitcad#853): Create Drawing › Cut Sheet, two bodies, DXF"
# A plate 100 x 60 (Body1) and a disk of 40 (Body2), 3 thick.
PLATES="$WORK/plates.mitcad"
cat > "$WORK/plates.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 100, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 3}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [150, 30], "diameter": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 3}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/plates.json" --save "$PLATES" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the plates)"; }
ui_stop_app
ui_start_app --open "$PLATES" --no-native-dialogs
ui_expect_log "plates.mitcad" "the plates opened"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "Kind: Cut Sheet (Alt+K, End)"     ui_key alt+k End
ui_step "OK: 600 x 300"                    ui_key Return
ui_focus_main
ui_expect_new "Create Drawing dialog: cut sheet 600 x 300" "a cut sheet of 600 x 300 mm asked for"
ui_expect_new "Drawing created: D1 CutSheet1 in C0" "CutSheet1 in the root's folder"
ui_expect_new "Drawing opened: D1 CutSheet1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing sheet drawn: 0 views" "no parts yet"
# A cut sheet's tab of its own (mitcad#1008): Add Components… and its Export.
ui_expect_new "Ribbon tabs: CUT SHEET (shown CUT SHEET)" "the CUT SHEET tab alone"
ui_expect_new "Ribbon button drawing.add_parts at" "Add Components… on the ribbon"
ui_expect_new "Drawing Add Parts at" "its place logged as the drawing's"
area_is_view
ui_mark
settled "Drawing Add Parts"
ui_step "Add Components…"                       ui_click_logged "Drawing Add Parts"
ui_expect_new "Cut sheet Add Parts dialog opened: 2 bodies" "its dialog lists both bodies"
ui_focus_dialog '^Add Components$'
ui_step "both bodies (Ctrl+A), OK"         ui_key ctrl+a Return
ui_focus_main
ui_expect_new '"cmd":"add_cut_sheet_parts"' "the parts asked for"
ui_expect_new "Drawing sheet drawn: 2 views" "two parts on the sheet"
ui_expect_new "Drawing view 2 at" "the second part drawn"
grep -q "Cut sheet warnings: part" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the parts were placed overlapping or off the sheet"
no_refusal
ui_mark
settled "Drawing Export"
ui_step "Export…"                          ui_click_logged "Drawing Export"
ui_step "DXF… (Down, Enter)"               ui_key Down Return
ui_step "type the path, Enter"             ui_type_path "Export Cut Sheet" "$WORK/parts.dxf"
ui_focus_main
ui_expect_new "Exported $WORK/parts.dxf: cut sheet CutSheet1, 2 parts" "the DXF written" 30
grep -q "CUT_OUTER" "$WORK/parts.dxf" || ui_fail "the DXF has no layer CUT_OUTER"
grep -q "CIRCLE" "$WORK/parts.dxf" || ui_fail "the DXF has no circle (the disk)"
echo "ok   the DXF has the parts' outlines"

echo "--- Create Drawing opens with the last choice (mitcad#1218)"
ui_mark
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
# The cut sheet is not kept: the last drawing's ISO, A3, first angle.
ui_expect_new "Create Drawing dialog starts with template iso, A3, landscape, first_angle (the last choice)" \
  "the last drawing's choice"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "Template: ANSI (Alt+T, Home, Down)" ui_key alt+t Home Down
ui_step "OK: ANSI B, third angle"          ui_key Return
ui_focus_main
ui_expect_new "Create Drawing dialog: template ansi, ANSI_B, third_angle" "the ANSI template on its B, third angle"
ui_expect_new "Drawing created: " "the drawing made"
ui_expect_new "Create Drawing choice kept: template ansi, ANSI_B, landscape, third_angle" "the choice kept"
ui_expect_new "Drawing tool: Base View" "Base View on for the new drawing"
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
settled "Drawing Close Drawing"
ui_step "Close Drawing"                    ui_click_logged "Drawing Close Drawing"
ui_expect_new "Drawing closed" "the 3D view again"
ui_mark
ui_step "Create Drawing again (search)"    ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog starts with template ansi, ANSI_B, landscape, third_angle (the last choice)" \
  "it opens with ANSI, B, third angle"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "Cancel (Esc)"                     ui_key Escape
ui_focus_main
ui_expect_new "Create Drawing cancelled" "nothing made"

echo "--- File › Export Drawing with two drawings, none laid out by it: New Drawing first (mitcad#1680)"
# The cut sheet and the ANSI drawing, both made in the drawing workspace:
# no guess between them.
ui_mark
ui_step "Export Drawing (search)"          ui_command "Export Drawing"
ui_expect_new "Drawing dialog opened: " "its dialog"
ui_expect_new ", drawing new" "New Drawing first: two drawings, none laid out last"
ui_focus_dialog "$(ui_title_regex "Export Drawing")"
ui_step "Cancel (Esc)"                     ui_key Escape
ui_focus_main
ui_expect_new "Drawing dialog cancelled" "nothing written"

echo "--- Model Items on a sheet of three views (mitcad#1406)"
# A 40 x 30 x 20 block with a 6.6 mm hole from its top at (20, 15), and the
# model's annotations: its height (Offset1), datum A on its top and the
# hole's callout. A sheet of three views (the front, one beside it, one
# above it); Model Items with nothing selected brings each of them into
# the one view that sees it, in one undo step.
ITEMS="$WORK/items.mitcad"
cat > "$WORK/items.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "hole",
    "placement": {"type": "face", "body": "F2.b0", "face": "F2:end($R)", "points": [[20, 15, 20]]},
    "diameter": 6.6, "extent": {"type": "distance", "depth": 10}}},
  {"cmd": "add_annotation", "name": "Offset1", "kind": "offset",
    "a": {"body": "F2.b0", "face": "F2:end($R)"}, "b": {"body": "F2.b0", "face": "F2:start($R)"}},
  {"cmd": "add_annotation", "name": "DatumA", "letter": "A", "a": {"body": "F2.b0", "face": "F2:end($R)"}},
  {"cmd": "add_annotation", "name": "HoleCallout1", "kind": "hole_callout",
    "a": {"body": "F2.b0", "face": "F3:hole0.wall"}, "at": [20, 18.3, 20]}
]
EOF
"$CLI" run "$WORK/items.json" --save "$ITEMS" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (the annotated block)"; }
ui_stop_app
ui_start_app --open "$ITEMS" --no-native-dialogs
ui_expect_log "items.mitcad" "the annotated block opened"
ui_mark
ui_step "Create Drawing (search)"          ui_command "Create Drawing"
ui_expect_new "Create Drawing dialog opened" "its dialog"
ui_focus_dialog '^Create Drawing$'
ui_step "OK"                               ui_key Return
ui_focus_main
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "its sheet over the view"
ui_expect_new "Drawing model items: none" "nothing of the model's yet"
ui_expect_new "Drawing tool: Base View" "Base View on"
ui_mark
ui_step "the front view at 120, 120"       sheet_click 120 120
ui_expect_new "Drawing view 1 at" "the front view drawn"
ui_expect_new "Drawing tool: Projected View" "projected views follow"
ui_step "a view beside it"                 sheet_click 215 120
ui_expect_new "Drawing view 2 at" "the side view drawn"
ui_step "a view above it"                  sheet_click 120 200
ui_expect_new "Drawing view 3 at" "the view from above or below drawn"
ui_expect_new "Drawing sheet drawn: 3 views" "three views on the sheet"
no_refusal
ui_mark
ui_step "end the tool (Esc)"               ui_key Escape
ui_expect_new "Drawing tool: selection" "back to the selection"
ui_mark
ui_step "Model Items (search)"             ui_command "Model Items"
ui_expect_new "Drawing tool: Model Items" "the Model Items tool is on"
ui_expect_new "Drawing panel drawingOption_sketch_dimensions at" "its Sketch Dimensions check box"
ui_expect_new "Drawing panel drawingOption_model_annotations at" "its Model Annotations check box"
ui_expect_new "Drawing panel drawingToolApply at" "its Apply"
ui_mark
settled "Drawing panel drawingToolApply"
ui_step "Apply (nothing selected)"         ui_click_logged "Drawing panel drawingToolApply"
ui_expect_new "Drawing Model Items on all views" "on all the sheet's views"
ui_expect_new '"cmd":"drawing_model_items"' "one command asked for"
ui_expect_new "Drawing model items: " "the model's annotations drawn"
ui_expect_new "Drawing tool: selection" "the tool done"
no_refusal
ui_sync
items=$(grep -F "Drawing model items: " "$UI_LOG" | tail -1)
for name in Offset1 DatumA HoleCallout1; do
  count=$(grep -o "$name in view [0-9]*" <<< "$items" | wc -l)
  [ "$count" -eq 1 ] || ui_fail "$name is drawn $count times: $items"
done
echo "ok   each of the model's annotations drawn once: ${items#*: }"
# The datum on the top and the hole's callout in the view from above or
# below, the height in the front or the side view.
grep -q "DatumA in view 3" <<< "$items" || ui_fail "the datum is not in the view from above: $items"
grep -q "HoleCallout1 in view 3" <<< "$items" || ui_fail "the callout is not in the view from above: $items"
grep -Eq "Offset1 in view [12]" <<< "$items" || ui_fail "the height is not in the front or the side view: $items"
echo "ok   each in the view that sees it"
# Each view's panel shows its Model Annotations on.
ui_mark
ui_step "select the front view"            ui_click_logged "Drawing view 1"
ui_expect_new "Drawing view 1 selected" "the front view selected"
ui_expect_new "Drawing panel drawingViewModelAnnotations at" "its Model Annotations in its panel"
ui_mark
ui_step "Esc"                              ui_key Escape
ui_expect_new "Drawing selection cleared" "nothing selected"
# One undo step takes them all.
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Drawing model items: none" "Model Items undone in one step"
no_refusal

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI drawings test"
