#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJoints.cpp app/OcctViewer.cpp
# check-all sources: core/model/src/joints.rs core/solver/src/rigid.rs tools/cli
# Dragging components (mitcad#150): which bodies can be dragged, and why a
# drag does not start, on a design mitcad-cli makes:
#   - Base:1 (a 60 x 40 x 10 block, grounded); Pin:1 (a 10 x 10 x 20
#     block) on it with a revolute joint (Joint1); Fixed:1 held where it is
#     by a rigid as-built joint to the base (AsBuiltJoint1); Clip:1 in a
#     rigid group with the base; Plate:1 without joints; Multi:1 with two
#     bodies (Body5, Body6); a body of the root (Body8).
#   - The pointer over the pin: the hint "Drag to move Pin:1 (Joint1: rz)"
#     (and the move cursor); over the base no hint.
#   - A left drag on each body that cannot be dragged says why in the
#     status bar ("Drag refused: <why>" in the log): nothing moves and no
#     window selection starts.
#   - With a command open, a drag on the pin says to close it.
#   - The pin still drags; opened read-only, its drag says why not.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drag-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drag.XXXXXX)
FILE=$WORK/drag.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Base (C1, O1): F1, F2 (Body1); Pin (C2, O2): F3, F4 (Body2); Fixed (C3,
# O3): F5, F6 (Body3); Plate (C4, O4): F7, F8 (Body4); Multi (C5, O5): F9,
# F10 (Body5), F11, F12 (Body6); Clip (C6, O6): F13, F14 (Body7); the
# root: F15, F16 (Body8). The pin's joint stands it off the base's middle,
# so that neither hides the other's middle in the view.
sed "s/RECT/$RECT/g" > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Base"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, -100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Fixed", "transform": {"translation": [0, 100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Plate", "transform": {"translation": [100, 100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F7", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Multi", "transform": {"translation": [-100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F9", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F9", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F11", "corner": [30, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F11", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Clip", "transform": {"translation": [0, -100, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F13", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F13", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F15", "corner": [-100, -100], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F15", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "ground_occurrence", "occurrence": "Base:1"},
  {"cmd": "add_joint", "kind": "revolute", "flip": true,
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:start(RECT)"}},
    "b": {"occurrence": "Base:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"},
          "frame_override": {"origin": [50, 20, 10]}}},
  {"cmd": "add_as_built_joint", "kind": "rigid", "a": "Fixed:1", "b": "Base:1"},
  {"cmd": "add_rigid_group", "occurrences": ["Clip:1", "Base:1"]},
  {"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [50, 20, 20], "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# hover "body" "hint" description: the pointer over a body's middle; the
# app logs the hint it shows ("none" without one).
hover() {
  ui_mark
  xdotool mousemove $(ui_logged_at "Body $1")
  ui_expect_new "Drag hint: $2" "$3"
}

# refused "body" "why" description: a left drag on a body that does not
# drag: the reason, and neither a drag nor a window selection (leftwards,
# a window would take the body it crosses: see the Shift drag below).
refused() {
  ui_mark
  ui_drag_from "Body $1" -80 -40
  ui_expect_new "Drag refused: $2" "$3"
  ui_sync
  if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^(Drag of |Dragged |Selected: )"; then
    ui_fail "$3: the drag moved or selected something"
  fi
  echo "ok   nothing moved or selected"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Body F4.b0 in O2 at" "the pin is shown"

echo "--- The pointer over the bodies: which drag"
hover "F4.b0 in O2" "Drag to move Pin:1 (Joint1: rz)" "over the pin: its hint"
hover "F2.b0 in O1" "none" "over the grounded base: no hint"
hover "F4.b0 in O2" "Drag to move Pin:1 (Joint1: rz)" "over the pin again"
hover "F8.b0 in O4" "none" "over the plate without joints: no hint"

echo "--- With Shift, a drag on a body selects with a window"
ui_mark
xdotool keydown shift
ui_drag_from "Body F8.b0 in O4" -80 -40
xdotool keyup shift
ui_expect_new "Selected: " "a window selection"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^(Drag refused: |Drag of )"; then
  ui_fail "the Shift drag was taken for a component drag"
fi
ui_step "clear the selection (Esc)"      ui_key Escape

echo "--- Drags that do not start, and why"
refused "F2.b0 in O1" "Base:1 is grounded" "the base is grounded"
refused "F14.b0 in O6" "Clip:1 is in a rigid group with Base:1, which is grounded" \
  "the clip moves with the grounded base"
refused "F6.b0 in O3" "Fixed:1 cannot move: AsBuiltJoint1 is rigid" "the fixed part is held by its rigid joint"
refused "F8.b0 in O4" "Plate:1 has no joints in its parent component" "the plate has no joints"
refused "F12.b0 in O5" "Body6 is a body of Multi: bodies move only with their component" \
  "a body of a component with several bodies"
refused "F16.b0" "Bodies of the root component do not move" "a body of the root"

echo "--- A command open: the pin does not drag"
ui_mark
ui_step "joint (J)"                      ui_key j
ui_expect_new "Command Joint started" "Joint started"
refused "F4.b0 in O2" "Close the command to drag components" "close the command first"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Joint cancelled" "Joint cancelled"

echo "--- The pin still drags"
ui_mark
ui_step "drag the pin sideways"          ui_drag_from "Body F4.b0 in O2" 80 0
ui_expect_new "Drag of Pin:1 started" "the drag takes the pin"
ui_expect_new "Dragged Pin:1 to (" "the drag kept"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"

echo "--- Read-only: the pin does not drag"
ui_mark
# Opening the open design again only brings it forward (mitcad#1172): close
# it first, so Open Read-Only reads it into the empty design left.
ui_step "close (Ctrl+W)"                 ui_key ctrl+w
ui_expect_new "Document closed" "closed: an empty design"
ui_step "open read-only (search)"        ui_command "Open Read-Only"
ui_step "the design"                     ui_type_path "Open Read-Only" "$FILE"
ui_focus_main
ui_expect_new "Edit lock: drag.mitcad opened read-only, without an edit lock" "opened read-only" 20
ui_wait_idle
refused "F4.b0 in O2" "drag.mitcad is read-only: it was opened without an edit lock." "read-only"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI drag test"
