#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/Command.hpp app/framework/CommandSession.cpp
# check-all sources: core/model/src/features/revolve.rs core/ffi/src/kernel/face_revolve.rs
# check-all sources: core/cpp/bridge/face_revolve.cpp geometry/src/face_revolve.cpp tools/cli
# Revolve with a planar body face as the profile (mitcad#543, as Extrude's
# since mitcad#535) through the real UI, on a design mitcad-cli makes: a
# 20 mm cube from x = 10 to 30 (Body1, F2.b0) and an unused square sketch
# (Sketch2, F3), whose square would be the newest profile.
#   - The cube's face at y = 0 (in the XZ plane, x 10 to 30) selected,
#     Revolve: the face is the profile, not the square; the Z axis picked,
#     Angle -90 degrees (away from the cube), New Body by hand: Pappus, the
#     face's 400 mm2 times a quarter of the circle of its centre (x = 20):
#     12566.37 mm3.
#   - Edited from the timeline to -45 degrees: half of that.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-revolve-face-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-revolve-face.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FRONT='F2:side(c1[c4,c2])'

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [10, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"body": "Body1", "volume": 8000, "tolerance": 1e-9}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-40, 0], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli block"; }

ui_start_display
ui_start_app --open "$WORK/block.mitcad"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: F2.b0" "the cube shown"

echo "--- The face at y = 0 selected, Revolve about Z"
# The pick places of faces come when an input takes them.
ui_mark
ui_step "offset face (search)"             ui_command "Offset Face"
ui_expect_new "Pick places:" "where the faces are picked"
ui_step "cancel it (Esc)"                  ui_key Escape
at=$(ui_pick_at "face F2.b0/$FRONT")
ui_mark
ui_step "select the face at y = 0"         xdotool mousemove $at click 1
ui_expect_new "Selected: 1 face [face $FRONT of F2.b0" "the face selected"
ui_mark
ui_step "revolve (search)"                 ui_command "Revolve"
ui_expect_new "Command Revolve started" "Revolve started"
ui_expect_new "Revolve Profiles: 1 face [face $FRONT of F2.b0" "the face is the profile"
no_new "Revolve Profiles: 1 profile" "the unused square does not stand in"
ui_step "the axis input"                   ui_click_logged "Panel Revolve input axis"
ui_show_origin
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Revolve Axis: 1 axis [axis z]" "Z axis picked"
ui_step "Angle"                            ui_choose "Panel Revolve input type" 1
ui_step "angle -90"                        ui_type_in "Panel Revolve input angle" "-90"
ui_step "New Body (one up from Join)"      ui_choose "Panel Revolve input operation" -1
ui_expect_new "Revolve: Operation = New Body" "New Body chosen"
ui_expect_new "Preview Revolve: ok" "previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added revolve (New Body), angle -90 deg, of a face" "the face revolved"
ui_expect_volume "New body [^(]* (F4.b0): volume \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 400 * 3.14159265358979 / 2 * 20 }')" "a quarter turn of the face"

echo "--- Edited to -45 degrees"
ui_mark
ui_step "edit Revolve1 (double-click)"     ui_double_click_logged "Timeline Revolve1"
ui_expect_new "Editing F4 with Revolve" "editing it"
ui_expect_new "Revolve Profiles: 1 face [face $FRONT" "with its face"
no_new "Revolve takes a planar face" "the face is not refused while editing"
ui_step "angle -45"                        ui_type_in "Panel Revolve input angle" "-45"
ui_expect_new "Preview Revolve: ok" "the edit previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F4" "edited"
ui_expect_volume "Body [^(]* (F4.b0): volume [0-9.]* -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 400 * 3.14159265358979 / 4 * 20 }')" "an eighth of a turn"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app
ui_finish "UI revolve face"
