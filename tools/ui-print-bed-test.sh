#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowContextMenu.cpp app/MainWindow.cpp app/files/MainWindowPrint.cpp
# check-all sources: app/commands/InspectCommands.cpp app/browser core/model/src/document/print_bed.rs
# check-all sources: core/model/src/exchange core/3mf tools/cli
# check-all sources: app/files/Print3d.cpp app/framework/CommandSession.cpp
# A body's print bed face (mitcad#731) through the real UI, on a 40 x 20 x
# 10 box (Plate, F1.b0) that mitcad-cli makes, with a fake slicer (a script
# that writes the arguments it was started with, so no slicer runs):
#   1. The context menu on the box's top offers Set as Print Bed Face (and
#      no Clear); it sets the face, one undo step, and the saved file keeps
#      it with the body.
#   2. The context menu on a side shows the top as the print bed face while
#      it is open, and offers Set (another face would replace it).
#   3. The context menu on the top offers Clear Print Bed Face; it clears
#      the face, and Undo brings it back.
#   4. 3D Print to a 3MF file turns the box over onto its top, its lowest
#      point on the bed and centred on the 256 mm plate: the log's
#      transform after the design's placement.
#   5. Print Analysis starts with the top as its bed face.
#   6. A design whose stored face is gone: 3D Print reports it in the
#      status line and lays the box on its bottom by the automatic
#      orientation (mitcad#1109), centred on the plate. A side's menu
#      offers Clear Print Bed Face (beside Set); cleared, 3D Print warns
#      no more.
#   7. A box made a surface body (its bottom unstitched) that keeps its
#      print bed face (mitcad#737): the menus of its top and of a side
#      offer Clear Print Bed Face only, show no bed face, and Clear clears
#      it.
#   8. Send to Slicer's Bed Face (mitcad#1109) on a box without a print
#      bed face: choosing the body fills it with the automatic
#      orientation's face (its bottom), highlighted in the bed face's
#      green while the panel is open, the body's own highlight kept;
#      picking the top in Bed Face stores it as the body's print bed face
#      (one undo step), the highlight follows, and Send turns the box onto
#      it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-print-bed-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-print-bed.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/plate.mitcad
GONE=$WORK/gone.mitcad
SURFACE=$WORK/surface.mitcad
FRESH=$WORK/fresh.mitcad
# The app's temporary folder, where the files for the slicer go.
export TMPDIR=$WORK/tmp
mkdir -p "$TMPDIR"
ARGS=$WORK/slicer-args.txt

# The fake slicer: one line per argument.
cat > "$WORK/fake-slicer.sh" << EOF
#!/bin/sh
printf '%s\n' "\$@" > "$ARGS.part"
mv "$ARGS.part" "$ARGS"
EOF
chmod +x "$WORK/fake-slicer.sh"
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[print]\nslicerName=Fake slicer\nslicerProgram=%s\nformat=3mf\n' "$WORK/fake-slicer.sh" \
  > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The box with its top as the print bed face, then its bottom unstitched:
# Plate (F1.b0) is a surface body of the other five faces that keeps the
# face (mitcad#737).
cat > "$WORK/surface.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 40, "width": 20, "height": 10, "operation": "new_body"}},
  {"cmd": "set_print_bed_face", "uid": "F1.b0", "face": "F1:top"},
  {"cmd": "add_feature", "def": {"type": "unstitch", "body": "F1.b0", "faces": ["F1:bottom"]}},
  {"cmd": "rename_body", "uid": "F1.b0", "name": "Plate"}
]
EOF
"$CLI" run "$WORK/surface.json" --save "$SURFACE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli surface"; }
[ "$(jq -c '.bodies[] | select(.uid == "F1.b0") | .print_bed_face' "$SURFACE")" = '"F1:top"' ] ||
  ui_fail "the surface body has no print bed face in $SURFACE"

# expect_file file filter expected description: a jq filter on a design file.
expect_file() {
  local actual
  actual=$(jq -c "$2" "$1") || ui_fail "$4: cannot read $1"
  [ "$actual" = "$3" ] || ui_fail "$4: $2 is '$actual', expected '$3'"
  echo "ok   $4"
}

# The last "Context menu: ..." line's entries, " | " between them.
menu_line() { grep -F "Context menu: " "$UI_LOG" | tail -1 | sed "s/.*Context menu: //"; }
# has ENTRY / lacks ENTRY: whether the context menu has the entry.
has() {
  [[ "| $(menu_line) |" == *"| $1 |"* ]] || ui_fail "'$1' is not in the context menu: $(menu_line)"
  echo "ok   the context menu has $1"
}
lacks() {
  [[ "| $(menu_line) |" != *"| $1 |"* ]] || ui_fail "'$1' is in the context menu: $(menu_line)"
  echo "ok   the context menu has no $1"
}

# pick_places: Offset Face started and cancelled, for the log's pick places
# of the faces.
pick_places() {
  ui_mark
  ui_step "offset face (search)"           ui_command "Offset Face"
  ui_expect_new "Pick places:" "where the faces are picked"
  ui_step "cancel it (Esc)"                ui_key Escape
}

# right_click FACE: a right click on the box's face where it is picked.
right_click() {
  local at
  pick_places
  at=$(ui_pick_at "face F1.b0/$1 at")
  ui_mark
  ui_step "right-click $1"                 xdotool mousemove $at click 3
  ui_expect_new "Selected: 1 face [face $1 of F1.b0" "$1 selected"
  ui_expect_new "Context menu: " "the context menu opened"
}

# side_face: SIDE, a side of the box that the view picks (side0 to side3,
# as the view is turned), from fresh pick places.
side_face() {
  pick_places
  SIDE=$(tail -n +"$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)" "$UI_LOG" |
    sed -n 's/.*Pick face F1\.b0\/\(F1:side[0-3]\) at .*/\1/p' | head -1)
  [ -n "$SIDE" ] || ui_fail "no side of the box is picked in the view"
}

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# slicer_ran: waits for the fake slicer's arguments of this run.
slicer_ran() {
  for _ in $(seq 1 50); do
    [ -f "$ARGS" ] && return 0
    sleep 0.2
  done
  ui_fail "the slicer was not started"
}

# send: 3D Print through the search, then Send (Enter).
send() {
  rm -f "$ARGS"
  ui_mark
  ui_command "Send to Slicer"
  ui_expect_new "Panel Send to Slicer input format at " "the panel"
  ui_key Return
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"

echo "--- 1. Set as Print Bed Face on the top"
right_click F1:top
has "Set as Print Bed Face"
lacks "Clear Print Bed Face"
ui_mark
ui_step "Set as Print Bed Face"            ui_menu_choose "Set as Print Bed Face"
ui_expect_new "Print bed face of Plate: F1:top" "the top is the print bed face"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file "$FILE" '.bodies[0].print_bed_face' '"F1:top"' "the file keeps it with the body"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 2. Another face's menu shows it"
side_face
right_click "$SIDE"
ui_expect_new "Print bed face shown: F1.b0/F1:top" "the top highlighted while the menu is open"
has "Set as Print Bed Face"
lacks "Clear Print Bed Face"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 3. Clear Print Bed Face, Undo"
right_click F1:top
has "Clear Print Bed Face"
lacks "Set as Print Bed Face"
ui_mark
ui_step "Clear Print Bed Face"             ui_menu_choose "Clear Print Bed Face"
ui_expect_new "Print bed face of Plate: none" "cleared"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Clear Print Bed Face of Plate" "one undo brings it back"
ui_step "Escape (clear the selection)"     ui_key Escape

echo "--- 4. 3D Print turns the box over onto its top"
ui_step "3D print (Enter)"                 send
ui_expect_new "3D Print bed: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | 0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000" \
  "turned over, the top on the bed, centred on the plate"
slicer_ran
[ -f "$TMPDIR/mitcad-print/plate/plate.3mf" ] || ui_fail "no 3MF file"
echo "ok   the 3MF file went to the slicer"

echo "--- 5. Print Analysis starts from it"
ui_mark
ui_step "print analysis (search)"          ui_command "Print Analysis"
ui_expect_new "Print Analysis bed face: F1.b0/F1:top" "the analysis starts with the top as its bed face"
ui_step "cancel it (Esc)"                  ui_key Escape
ui_stop_app

echo "--- 6. A print bed face that is gone"
jq '.bodies[0].print_bed_face = "F9:top"' "$FILE" > "$GONE" || ui_fail "jq"
ui_start_app --open "$GONE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"
ui_step "3D print (Enter)"                 send
ui_expect_new "3D Print bed: Plate on F1:bottom (automatic), plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | 0.000 1.000 0.000 118.000 | 0.000 0.000 1.000 0.000; Plate: the print bed face is gone" \
  "laid on its bottom by the automatic orientation, centred, the face reported"
ui_expect_new "3D Print last line: " "the status line"
no_new "printed as in the design" "the status line does not say it is printed as in the design"
slicer_ran

echo "--- 6b. The gone face cleared from another face's menu"
side_face
right_click "$SIDE"
has "Clear Print Bed Face"
has "Set as Print Bed Face"
ui_mark
ui_step "Clear Print Bed Face"             ui_menu_choose "Clear Print Bed Face"
ui_expect_new "Print bed face of Plate: none" "the gone face cleared"
ui_step "Escape (clear the selection)"     ui_key Escape
ui_step "3D print (Enter)"                 send
ui_expect_new "3D Print bed: Plate on F1:bottom (automatic), plate 128.000,128.000" \
  "laid on its bottom by the automatic orientation"
slicer_ran
no_new "the print bed face is gone" "3D Print warns no more"
ui_stop_app

echo "--- 7. A surface body that keeps a print bed face"
ui_start_app --open "$SURFACE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the surface body shown"
right_click F1:top
has "Clear Print Bed Face"
lacks "Set as Print Bed Face"
ui_step "Escape (close the menu)"          ui_key Escape
ui_step "Escape (clear the selection)"     ui_key Escape
side_face
right_click "$SIDE"
has "Clear Print Bed Face"
lacks "Set as Print Bed Face"
no_new "Print bed face shown:" "no print bed face shown on a surface body"
ui_mark
ui_step "Clear Print Bed Face"             ui_menu_choose "Clear Print Bed Face"
ui_expect_new "Print bed face of Plate: none" "the surface body's face cleared"
ui_stop_app

echo "--- 8. Send to Slicer's Bed Face (mitcad#1109)"
"$CLI" run "$WORK/model.json" --save "$FRESH" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli fresh"; }
ui_start_app --open "$FRESH"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the box shown"
pick_places
rm -f "$ARGS"
ui_mark
ui_command "Send to Slicer"
ui_expect_new "Panel Send to Slicer input format at " "the panel"
ui_expect_new "3D Print bed face: Plate: automatic; Plate F1:bottom (automatic)" \
  "choosing the body fills Bed Face with the automatic orientation's face"
ui_step "the Bed Face row"                 ui_click_logged "Panel Send to Slicer input print_bed"
ui_expect_new "Send to Slicer marks: face F1.b0/F1:bottom" "the bottom highlighted while the panel is open"
no_new "picks not drawn beside the marks: Bodies" "the body's own highlight kept beside it"
expect_file "$FRESH" '.bodies[0].print_bed_face' 'null' "the automatic face is not stored"
ui_mark
ui_step "pick the top in Bed Face"         ui_click_pick "face F1.b0/F1:top at"
ui_expect_new "3D Print bed face set: Plate F1:top" "the top made the body's print bed face"
ui_expect_new "3D Print bed face: Plate: print bed face; Plate F1:top" "Bed Face shows it"
ui_expect_new "Send to Slicer marks: face F1.b0/F1:top" "the highlight follows"
ui_mark
ui_step "send (Enter)"                     ui_key Return
ui_expect_new "3D Print bed: Plate on F1:top, plate 128.000,128.000, transform 1.000 0.000 0.000 108.000 | 0.000 -1.000 0.000 138.000 | 0.000 0.000 -1.000 10.000" \
  "sent turned over onto the top"
slicer_ran
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $FRESH" "saved"
expect_file "$FRESH" '.bodies[0].print_bed_face' '"F1:top"' "the top stored as the body's print bed face"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: Set Print Bed Face of Plate" "one undo step"

ui_finish "UI print bed"
