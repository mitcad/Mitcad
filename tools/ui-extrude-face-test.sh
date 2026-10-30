#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp app/commands/ProfilePicks.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandSession.cpp app/MainWindowContextMenu.cpp
# check-all sources: core/model/src/features/extrude.rs core/ffi/src/kernel/face_extrude.rs
# check-all sources: core/cpp/bridge/face_extrude.cpp geometry/src/face_extrude.cpp tools/cli
# check-all sources: app/commands/SketchNormal.cpp app/framework/ManipulatorOverlay.cpp
# Extrude with a planar body face as the profile (mitcad#535) through the
# real UI, on designs mitcad-cli makes:
#   1. One imported 20 mm cube (written to STEP and read back: a base
#      feature, no sketch at all). Extrude starts with no profile; a face
#      picked in the view is the profile, Join is chosen automatically, 5 mm
#      out: the cube grows by 2000 mm3. Edited from the timeline to 8 mm:
#      3200 mm3; one undo goes back to 5 mm.
#   2. A 20 mm cube from a sketch (Body1, F2.b0), a sphere beside it
#      (F3.b0) and an unused square sketch (Sketch3, F4), whose square
#      would be the newest profile:
#      - E with nothing selected: the square is the profile; a click on
#        the cube's side is not taken beside it (mitcad#868), the status
#        bar says what to click instead, no error in the panel;
#      - the cube's side x = 20 selected, E: the side is the profile, not
#        the square; New Body chosen by hand, 5 mm with a taper of 5
#        degrees (mitcad#547): a new body of 2000 + 1000 t + 500 t^2 / 3
#        mm3 (t = tan 5 degrees: 2088.764 mm3);
#      - the context menu on the cube's top offers Extrude in Solid; Solid >
#        Extrude starts with the top, 5 mm into the cube is Cut, chosen
#        automatically: 8000 -> 6000 mm3;
#      - the sphere's face selected, E: refused on the panel as not planar,
#        and the square does not stand in for it.
#   3. A square sketch in component Brace, whose occurrence is turned a
#      quarter about X (mitcad#958): Extrude's distance arrow points along
#      the sketch's normal in the design (-Y), not the component's +Z.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-face-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-extrude-face.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
TOP="F2:end($R)"
RIGHT='F2:side(c2[c1,c3])'

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

# pick_places: Offset Face started and cancelled, for the log's pick places
# of the faces (Extrude's own come when its Profiles take faces).
pick_places() {
  ui_mark
  ui_step "offset face (search)"           ui_command "Offset Face"
  ui_expect_new "Pick places:" "where the faces are picked"
  ui_step "cancel it (Esc)"                ui_key Escape
}

cat > "$WORK/cube.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"body": "Body1", "volume": 8000, "tolerance": 1e-9}},
  {"cmd": "export", "path": "$WORK/cube.step"}
]
EOF
cat > "$WORK/imported.json" << EOF
[
  {"cmd": "import_file", "path": "$WORK/cube.step"},
  {"expect": {"feature": "Base1", "status": "ok", "volume": 8000, "tolerance": 1e-6}}
]
EOF
cat > "$WORK/turned.json" << EOF
[
  {"cmd": "create_component", "name": "Brace", "transform": [[1, 0, 0, 0], [0, 0, -1, 0], [0, 1, 0, 30]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20}
]
EOF
cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [60, 10], "diameter": 20,
    "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [-40, 0], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/cube.json" --save "$WORK/cube.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli cube"; }
"$CLI" run "$WORK/imported.json" --save "$WORK/imported.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli imported"; }
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli block"; }
"$CLI" run "$WORK/turned.json" --save "$WORK/turned.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli turned"; }

ui_start_display

echo "=== 1. A face of an imported body, picked in the command"
ui_start_app --open "$WORK/imported.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F1.b0" "the imported cube shown"
ui_mark
ui_step "extrude (E): enabled without sketches"  ui_key e
ui_expect_new "Command Extrude started" "Extrude started in a design without sketches"
ui_expect_new "Extrude Profiles: nothing" "no profile to take"
ui_expect_new "Pick places:" "its Profiles take faces"
ui_mark
ui_step "pick a face of the cube"          ui_click_pick "face F1.b0/F1:import("
ui_expect_new "Extrude Profiles: 1 face [face F1:import(" "the face is the profile"
ui_expect_new "Extrude: automatic Operation = Join" "Join: out of the cube"
ui_mark
ui_step "distance 5"                       ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added extrude (Join), distance 5, of a face" "the face extruded"
ui_expect_volume "Body [^(]* (F1.b0): volume 8000.000 -> \([0-9.]*\) mm3" 10000 "the cube 5 mm longer"

ui_mark
ui_step "edit Extrude1 (double-click)"     ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "editing it"
ui_expect_new "Extrude Profiles: 1 face [face F1:import(" "with its face"
# The face found where the feature finds it (before its own extrusion
# joined it away): the distance's arrow along its normal, a preview.
ui_expect_new "Manipulator Extrude distance at " "the distance's arrow while editing"
ui_expect_new "Preview Extrude: ok" "the edit previewed as it opens"
no_new "Extrude takes a planar face" "the face is not refused while editing"
ui_mark
ui_step "distance 8"                      ui_type_in "Panel Extrude input distance" "8"
ui_expect_new "Preview Extrude: ok" "the edit previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F2" "edited"
ui_expect_volume "Body [^(]* (F1.b0): volume 10000.000 -> \([0-9.]*\) mm3" 11200 "8 mm instead"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: " "the edit undone"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== 2. Faces selected before Extrude, beside an unused sketch"
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the cube shown"

echo "--- The square's profile in Profiles, a face clicked: not taken, a hint instead (mitcad#868)"
ui_mark
ui_step "extrude (E), nothing selected"   ui_key e
ui_expect_new "Extrude Profiles: 1 profile" "the unused square is the profile"
ui_expect_new "Pick refused face F2.b0/$RIGHT at " "the cube's side is not taken beside it"
ui_mark
ui_step "click the side x = 20"            ui_click_pick "refused face F2.b0/$RIGHT"
ui_expect_new "Extrude Profiles: face $RIGHT of F2.b0 not taken: Profiles holds sketch profiles" \
  "the click is not taken; the status bar says what to click"
no_new "Panel Extrude error" "no error in the panel"
no_new "Extrude Profiles: 2 " "Profiles still hold the square alone"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"

echo "--- The side selected, E: New Body by hand"
pick_places
at=$(ui_pick_at "face F2.b0/$RIGHT")
ui_mark
ui_step "select the side x = 20"           xdotool mousemove $at click 1
ui_expect_new "Selected: 1 face [face $RIGHT of F2.b0" "the side selected"
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "Extrude Profiles: 1 face [face $RIGHT of F2.b0" "the side is the profile"
no_new "Extrude Profiles: 1 profile" "the unused square does not stand in"
ui_expect_new "Extrude: automatic Operation = Join" "Join at first"
ui_mark
ui_step "New Body (one up from Join)"      ui_choose "Panel Extrude input operation" -1
ui_expect_new "Extrude: Operation = New Body" "New Body chosen"
ui_step "distance 5"                       ui_type_in "Panel Extrude input distance" "5"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_mark
ui_step "taper 5 degrees"                  ui_type_in "Panel Extrude input taper" "5"
ui_expect_new "Preview Extrude: ok" "the tapered face previewed"
no_new "Panel Extrude error" "the face's taper is not refused"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added extrude (New Body), distance 5, of a face" "a new body of the side"
ui_expect_volume "New body [^(]* (F5.b0): volume \([0-9.]*\) mm3" 2088.764375 \
  "20 x 20 x 5 beside the cube, widening by 5 degrees"
ui_step "clear the selection (Esc)"        ui_key Escape

echo "--- The context menu on the top: Solid > Extrude, Cut"
pick_places
at=$(ui_pick_at "face F2.b0/$TOP")
ui_mark
ui_step "right-click the top"              xdotool mousemove $at click 3
ui_expect_new "Selected: 1 face [face $TOP of F2.b0" "the top selected"
ui_expect_new "Context menu: " "the context menu opened"
ui_mark
ui_step "Solid > Extrude"                  ui_menu_choose "Solid" "Extrude"
ui_expect_new "Command Extrude started" "Extrude started from the menu"
ui_expect_new "Extrude Profiles: 1 face [face $TOP of F2.b0" "the top is the profile"
no_new "Extrude Profiles: 1 profile" "the unused square does not stand in"
ui_mark
ui_step "distance -5"                      ui_type_in "Panel Extrude input distance" "-5"
ui_expect_new "Extrude: automatic Operation = Cut" "Cut: into the cube"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added extrude (Cut), distance -5, of a face" "the top cut down"
ui_expect_volume "Body Body1 (F2.b0): volume 8000.000 -> \([0-9.]*\) mm3" 6000 "the cube 15 high"
ui_step "clear the selection (Esc)"        ui_key Escape

echo "--- The sphere's face selected, E: refused"
pick_places
at=$(ui_pick_at "face F3.b0/F3:side0")
ui_mark
ui_step "select the sphere"                xdotool mousemove $at click 1
ui_expect_new "Selected: 1 face [face F3:side0 of F3.b0" "the sphere's face selected"
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "refused as the profile" "the face is refused"
ui_expect_new "Panel Extrude error: Extrude takes a planar face; the selected face is not planar." \
  "the panel says why"
no_new "Extrude Profiles: 1 profile" "the unused square does not stand in"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "=== 3. A sketch in a turned occurrence: the arrow along its normal in the design (mitcad#958)"
ui_start_app --open "$WORK/turned.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "extrude (E)"                      ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F1" "the square in Brace is the profile"
ui_expect_new "Manipulator Extrude distance at " "the distance's arrow"
# Brace:1 turns the component's +Z to the design's -Y.
ui_expect_new "Arrow direction Extrude distance: 0.000,-1.000,0.000" \
  "the arrow along the sketch's normal in the design"
no_new "Arrow direction Extrude distance: 0.000,0.000,1.000" "not along the component's own normal"
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"
ui_stop_app
ui_finish "UI extrude face"
