#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FaceCommands.cpp app/commands/FilletCommands.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandSessionHover.cpp core/model/src/features/offset_face.rs
# check-all sources: core/ffi/src/kernel/faceops.rs core/cpp/bridge/faceops.cpp geometry/src/faceops.cpp
# check-all sources: geometry/src/face_select.cpp geometry/src/tangent_chain.cpp tools/cli
# Offset Face and Press Pull with Tangent Chain (mitcad#2438) through the
# real UI, on a 60 x 40 x 30 mm box whose front right vertical corner is
# rounded (8 mm): the front (y = 0), the rounding and the right side
# (x = 60) are joined through smooth edges.
#   1. Offset Face: hovering the front shows the 3 faces; one click on it
#      and OK move all three 2 mm out: the section grows to 62 x 42 with a
#      10 mm rounding.
#   2. Undone; Offset Face with Tangent Chain off: the hover shows the front
#      alone, and OK moves it alone, its 52 mm up to the rounding, with a
#      step at the rounding.
#   3. Undone; Press Pull: with Continue at Offset the hover shows the 3
#      faces, with Straight the face alone; back at Offset, OK moves the 3
#      faces 5 mm out (65 x 45 with a 13 mm rounding).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-offset-face-tangent-chain-test.sh

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
WORK=$(mktemp -d "$BUILD_DIR/ui-offset-face-tangent-chain.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FRONT='face F1.b0/F1:side0'
RIGHT='face F1.b0/F1:side1'

cat > "$WORK/box.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 60, "width": 40, "height": 30, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F1.b0", "edges": ["E{F1:side0|F1:side1}"],
    "radius": 8}},
  {"expect": {"feature": "Fillet1", "status": "ok"}}
]
EOF
"$CLI" run "$WORK/box.json" --save "$WORK/box.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The volumes: a W x D x 30 box with one corner rounded by r is
# (W·D − r²·(1 − π/4))·30. Before: 60 x 40, r 8.
SECTION_CUT=$(awk 'BEGIN { printf "%.10f", 1 - atan2(1, 1) }')
box_volume() { awk -v w="$1" -v d="$2" -v r="$3" -v c="$SECTION_CUT" 'BEGIN { printf "%.6f", (w * d - r * r * c) * 30 }'; }
BEFORE=$(box_volume 60 40 8)

# off: the pointer off the box (the hover shows nothing).
off() { xdotool mousemove $(ui_view_at 3 50); }

# volume_now expected description: the box's volume logged since the mark
# ("Body <name> (F1.b0): volume <before> -> <after> mm3") is the expected
# one (relative 1e-6).
volume_now() {
  local line="" after
  for _ in $(seq 1 150); do
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "(F1.b0): volume " | tail -1)
    [ -n "$line" ] && break
    sleep 0.2
  done
  [ -n "$line" ] || ui_fail "$2: no volume logged"
  after=$(sed -n 's/.* -> \([0-9.]*\) mm3$/\1/p' <<< "$line")
  [ -n "$after" ] || ui_fail "$2: no new volume in '$line'"
  awk -v v="$after" -v e="$1" 'BEGIN { exit !((v - e) ^ 2 < (1e-6 * e) ^ 2 + 1e-4) }' ||
    ui_fail "$2: volume $after mm3, expected $1"
  echo "ok   $2: $after mm3"
}

# hover "pick text" count what: the pointer off the box, then over the
# face: the hover shows `count` faces.
hover() {
  ui_step "pointer off the box" off
  ui_mark
  ui_step "hover the $3" xdotool mousemove $(ui_pick_at "$1")
  ui_expect_new "Hover chain: $2 face(s) of F1.b0" "the hover over the $3 shows $2 face(s)"
}

ui_start_display
ui_start_app --open "$WORK/box.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/box.mitcad" "the box opened"
ui_step "fit (F6)" ui_key F6

echo "--- 1. Offset Face with Tangent Chain (the default)"
ui_mark
ui_step "offset face (search)" ui_command "Offset Face"
ui_expect_new "Pick $FRONT at" "Offset Face: the pick places are logged"
ui_expect_new "Panel Offset Face input tangent_chain at" "Offset Face: the panel has Tangent Chain"
hover "$FRONT" 3 "front"
ui_mark
ui_step "pick the front" xdotool click 1
ui_expect_new "Offset Face Faces: 1 face [" "the pick is the one face"
ui_expect_new "Preview Offset Face: ok" "previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)" ui_key Return
ui_expect_new "Added offset face of 1 face(s), distance 2" "the offset added"
volume_now "$(box_volume 62 42 10)" "the front, the rounding and the right side 2 mm out"
ui_mark
ui_step "undo (Ctrl+Z)" ui_key ctrl+z
ui_expect_new "Undo: " "the offset undone"

echo "--- 2. Offset Face with Tangent Chain off"
ui_mark
ui_step "offset face (search)" ui_command "Offset Face"
ui_expect_new "Pick $FRONT at" "Offset Face again: the pick places are logged"
ui_expect_new "Panel Offset Face input tangent_chain at" "Offset Face again: the panel has Tangent Chain"
ui_step "Tangent Chain off" ui_click_logged "Panel Offset Face input tangent_chain"
ui_expect_new "Offset Face: Tangent Chain = off" "Tangent Chain is off"
hover "$FRONT" 1 "front"
ui_mark
ui_step "pick the front" xdotool click 1
ui_expect_new "Offset Face Faces: 1 face [" "the pick is the one face"
ui_expect_new "Preview Offset Face: ok" "previewed"
ui_focus_main
ui_mark
ui_step "OK (Enter)" ui_key Return
ui_expect_new "Added offset face of 1 face(s), distance 2" "the offset added"
volume_now "$(awk -v b="$BEFORE" 'BEGIN { printf "%.6f", b + 52 * 2 * 30 }')" \
  "the front alone 2 mm out, up to the rounding"
ui_mark
ui_step "undo (Ctrl+Z)" ui_key ctrl+z
ui_expect_new "Undo: " "the offset undone"

echo "--- 3. Press Pull on the front, Continue at Offset"
ui_mark
ui_step "press pull (Q)" ui_key q
ui_expect_new "Pick $FRONT at" "Press Pull: the pick places are logged"
ui_expect_new "Panel Press Pull input tangent_chain at" "Press Pull: the panel has Tangent Chain"
hover "$FRONT" 3 "front"
ui_mark
ui_step "pick the front" xdotool click 1
ui_expect_new "Press Pull Faces, Edges or Profiles: 1 face [" "the pick is the one face"
ui_expect_new "Preview Press Pull: ok" "previewed"
# Straight carries the picked faces alone: no chain on the hover.
ui_step "Continue: Straight" ui_choose "Panel Press Pull input continue" 1
ui_expect_new "Press Pull: Continue = Straight" "Continue is Straight"
hover "$RIGHT" 1 "right side, Straight"
ui_step "Continue: Offset" ui_choose "Panel Press Pull input continue" -1
ui_expect_new "Press Pull: Continue = Offset" "Continue is Offset again"
hover "$RIGHT" 3 "right side, Offset"
ui_step "pointer off the box" off
ui_focus_main
ui_mark
ui_step "OK (Enter)" ui_key Return
ui_expect_new "Press Pull made offset face, distance 5" "the offset added"
volume_now "$(box_volume 65 45 13)" "the front, the rounding and the right side 5 mm out"

ui_finish "Offset Face and Press Pull with Tangent Chain"
