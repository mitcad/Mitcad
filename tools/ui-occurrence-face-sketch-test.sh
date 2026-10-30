#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/browser app/MainWindow.cpp app/MainWindow.hpp app/MainWindowVisibility.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/framework/ModelShapes.cpp app/framework/DocumentSession.hpp
# check-all sources: core/model/src/links.rs core/model-api/src/sketch.rs tools/cli
# check-all sources: app/MainWindowActivate.cpp
# A sketch on a face of an occurrence placed by a turn and a move, through
# the real UI (mitcad#505). Box:1 (O1) is turned 90 degrees about X and
# moved by (100, 0, 50); its 30 x 20 x 10 block's top face (z = 10 in Box)
# faces -Y in the design, at y = -10, from x 100 to 130 and z 50 to 70,
# its middle at (115, -10, 60). Seen from the front:
#   - With Box active (its radio button), Create Sketch on that face: the
#     sketch is Box's, its frame in the design lies on the face (origin on
#     its plane, the normal (0, -1, 0)), the view looks along it
#     (+Y), and a line drawn from the face's middle has both ends on the
#     face where the view shows it. Its own face asks nothing.
#   - With the root active, the same in a sketch of the root linked to the
#     face: the pick of the root's child occurrence's face asks whether to
#     activate Box:1 (mitcad#960, "Activate component asked: Box:1 (active
#     Root)"), answered Keep in Root (Alt+K).
#   - The root still active, the question answered Activate Box:1 (Enter,
#     the default): Box is active, and the sketch on the same pick is Box's,
#     shown where Box:1 places it.
# The Windows test is tools/ui-windows-occurrence-face-sketch-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-occurrence-face-sketch-test.sh

source "$(dirname "$0")/ui-test-lib.sh"
# The question whether to activate a picked face's component (mitcad#960),
# which the other tests turn off.
export MITCAD_ASK_ACTIVATE_COMPONENT=1

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-occurrence-face-sketch.XXXXXX)
FILE=$WORK/turned.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# within "x, y, z" "ex ey ez" tolerance description: each coordinate within
# the tolerance of the expected one.
within() {
  awk -v got="$1" -v want="$2" -v tol="$3" 'BEGIN {
    if (split(got, g, /, */) != 3 || split(want, w, / +/) != 3) exit 1
    for (i = 1; i <= 3; i++) { d = g[i] - w[i]; if (d < 0) d = -d; if (d > tol) exit 1 }
  }' || ui_fail "$4: ($1), expected ($2) within $3"
  echo "ok   $4: ($1)"
}

# on_face "x, y, z" description [plane]: a point of the design on the
# block's top face (y = -10 within 1e-6 mm, inside its extent); with
# "plane", on the face's plane.
on_face() {
  awk -v got="$1" -v plane="${3:-}" 'BEGIN {
    if (split(got, g, /, */) != 3) exit 1
    d = g[2] + 10; if (d < 0) d = -d
    if (d > 1e-6) exit 1
    if (plane == "" && (g[1] < 100 || g[1] > 130 || g[3] < 50 || g[3] > 70)) exit 1
  }' || ui_fail "$2: ($1) is not on the face (y = -10, x 100 to 130, z 50 to 70)"
  local where="on the face"
  [ -n "${3:-}" ] && where="on the face's plane"
  echo "ok   $2: ($1) $where"
}

# sketch_on_face where [keep|activate]: Create Sketch on the block's top
# face, seen from the front; the frame and the view checked; a line from
# the face's middle (where the view shows the block's middle) 7 and 4 mm
# along the sketch's axes, its ends checked. With keep or activate, the
# pick asks whether to activate Box:1 (mitcad#960) and is answered so;
# without, it asks nothing.
sketch_on_face() {
  local line origin normal along from to at x y sx sy
  ui_step "front view (search)"          ui_command "Front View"
  ui_step "fit (F6)"                     ui_key F6
  ui_sync
  ui_mark
  ui_step "create sketch (search)"       ui_command "Create Sketch"
  ui_expect_new "Pick face F2.b0/F2:end(" "$1: the top face can be picked"
  ui_step "pick the top face"            ui_click_pick "face F2.b0/F2:end("
  case "${2:-}" in
    keep|activate)
      ui_expect_new "Activate component asked: Box:1 (active Root)" "$1: asked whether to activate Box:1"
      ui_focus_dialog '^Activate Component$'
      if [ "$2" = keep ]; then
        ui_step "keep in Root (Alt+K)"   ui_key alt+k
        ui_focus_main
        ui_expect_new "Activate component answer: keep in Root" "$1: kept in the root"
      else
        ui_step "activate Box:1 (Enter)" ui_key Return
        ui_focus_main
        ui_expect_new "Activate component answer: activate Box:1" "$1: Box:1 to be activated"
        ui_expect_new "Activated Box" "$1: Box is active"
      fi
      ;;
  esac
  ui_expect_new 'Sketch started on {"body":"F2.b0","face":"F2:end(' "$1: a sketch on the face"
  if [ -z "${2:-}" ]; then
    tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Activate component asked" &&
      ui_fail "$1: asked whether to activate a component"
    echo "ok   $1: nothing asked"
  fi
  ui_expect_new "} in O1, origin (" "$1: picked where Box:1 shows it"
  ui_expect_new "Sketch view along" "$1: the view's direction logged"
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Sketch frame in the design" | tail -1)
  origin=$(sed -n 's/.* origin (\([^)]*\)).*/\1/p' <<< "$line")
  normal=$(sed -n 's/.* normal (\([^)]*\)).*/\1/p' <<< "$line")
  on_face "$origin" "$1: the frame's origin in the design" plane
  within "$normal" "0 -1 0" 1e-6 "$1: the frame's normal along the face's"
  along=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Sketch view along" | tail -1 |
    sed -n 's/.*along (\([^)]*\)).*/\1/p')
  within "$along" "0 1 0" 1e-6 "$1: Look At normal to the face"
  ui_sync
  ui_mark
  ui_step "line tool (L)"                ui_key l
  ui_expect_new "Sketch tool Line" "$1: the line tool"
  # The block's middle, seen along the face's normal, is the face's middle:
  # the sketch point the pointer shows there.
  read -r x y <<< "$(ui_logged_at "Body F2.b0 in O1")"
  ui_step "pointer over the block's middle" xdotool mousemove "$x" "$y"
  ui_expect_new "Sketch pointer x " "$1: the pointer's sketch point"
  read -r sx sy <<< "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Sketch pointer x " | tail -1 |
    sed -n 's/.*pointer x \([-0-9.]*\), y \([-0-9.]*\) mm.*/\1 \2/p')"
  [ -n "$sy" ] || ui_fail "$1: no sketch point of the pointer"
  ui_step "start at the face's middle"   ui_sketch_click "$sx" "$sy"
  ui_step "end beside it"                ui_sketch_click "$(awk -v v="$sx" 'BEGIN { print v + 7 }')" \
    "$(awk -v v="$sy" 'BEGIN { print v + 4 }')"
  ui_expect_new "Line c1 in the design from" "$1: a line drawn"
  at=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Line c1 in the design from" | tail -1)
  from=$(sed -n 's/.* from (\([^)]*\)) to.*/\1/p' <<< "$at")
  to=$(sed -n 's/.* to (\([^)]*\))$/\1/p' <<< "$at")
  on_face "$from" "$1: the line's start"
  on_face "$to" "$1: the line's end"
  within "$from" "115 -10 60" 1.5 "$1: the start where the face's middle was clicked"
  ui_step "end the chain and the tool"   ui_key Escape Escape
  ui_mark
  ui_step "finish sketch (Ctrl+Enter)"   ui_key ctrl+Return
  ui_expect_new "Sketch finished" "$1: sketch finished"
  grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
  ui_crashed && ui_fail "the application crashed"
}

# Box:1 (O1, C1): Sketch1 (F1) 30 x 20, Extrude1 (F2) 10 up: Body1 (F2.b0),
# placed by the turn and the move. The root's sketches may lie on Box's
# faces (mitcad#220).
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Box", "transform": [[1, 0, 0, 100], [0, 0, -1, 0], [0, 1, 0, 50]]},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "set_policy", "component": "Box", "policy": {"attachments_from_others": true}},
  {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0 in O1" "Box's block shown"
ui_expect_log "Browser radio Root/Box:1 at" "Box in the browser"

echo "--- Box active: a sketch of Box on its own face"
ui_mark
ui_step "activate Box (radio)"           ui_click_logged "Browser radio Root/Box:1"
ui_expect_new "Activated Box" "Box is active"
sketch_on_face "Box active"
ui_expect_log "Sketch Sketch2 is in Box" "the sketch is Box's"
grep -F "Sketch frame in the design (at O1)" "$UI_LOG" > /dev/null ||
  ui_fail "Box active: the sketch is not shown where Box:1 places it"
echo "ok   Box active: shown where Box:1 places it"

echo "--- The root active: a sketch of the root linked to Box's face"
ui_mark
ui_step "activate the root (radio)"      ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root is active"
sketch_on_face "the root active" keep
ui_expect_log "Sketch Sketch3 is in Root" "the sketch is the root's"

echo "--- The root active, Box:1 activated from the question (mitcad#960)"
FRAMES_AT_O1=$(grep -cF "Sketch frame in the design (at O1)" "$UI_LOG")
sketch_on_face "Box:1 activated" activate
ui_expect_log "Sketch Sketch4 is in Box" "the sketch is Box's"
[ "$(grep -cF "Sketch frame in the design (at O1)" "$UI_LOG")" -gt "$FRAMES_AT_O1" ] ||
  ui_fail "Box:1 activated: the sketch is not shown where Box:1 places it"
echo "ok   Box:1 activated: shown where Box:1 places it"

ui_finish "UI occurrence face sketch test"
