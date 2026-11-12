#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSession.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/OcctViewer.cpp app/browser/DocumentSnapshot.cpp tools/cli
# Editing a feature of a placed component (mitcad#432), through the real
# UI: a component placed 60 mm along X with a rod (a cylinder about its own
# Z axis), a thread on the rod's side and two construction planes, the
# second offset from the first. A loaded definition's references have no
# occurrence; the edit gives them the one the view shows the component in:
#   - Editing the thread: the Faces input's face is highlighted on the
#     placed rod (about x = 60), not at the component's own coordinates
#     (about x = 0), and a click on the rod's side takes the entry out
#     instead of listing the face a second time.
#   - Editing the second plane: its reference plane is highlighted where
#     the view shows the first plane (x = 60), and a click on that plane
#     takes the entry out instead of replacing it with itself.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-placed-edit-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-placed-edit.XXXXXX)
FILE=$WORK/placed.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
SIDE='F2:side(c1)'

# expect_place "kind owner" x y z description: the newest "View highlight
# places:" line has the highlight "<kind> <owner>... at x,y,z" within
# 0.5 mm of the point.
expect_place() {
  local line at
  line=$(grep "View highlight places: " "$UI_LOG" | tail -1)
  at=$(printf '%s\n' "$line" | sed 's/^.*View highlight places: //' | tr ';' '\n' |
    grep -F -- "$1" | head -1 | sed -n 's/.* at \([-0-9.]*,[-0-9.]*,[-0-9.]*\)$/\1/p')
  [ -n "$at" ] || ui_fail "$5: no highlight of '$1' logged: $line"
  awk -v at="$at" -v x="$2" -v y="$3" -v z="$4" 'BEGIN {
    split(at, p, ",")
    d = (p[1] - x) ^ 2 + (p[2] - y) ^ 2 + (p[3] - z) ^ 2
    exit d <= 0.25 ? 0 : 1
  }' || ui_fail "$5: '$1' highlighted at $at, expected $2,$3,$4"
  echo "ok   $5 ($at)"
}

# Plug:1 (O1, C1) placed at x = 60: Sketch1 (F1) a 10 mm circle about the
# component's origin, Extrude1 (F2) 20 up: the rod Body1 (F2.b0). Thread1
# (F3) M10x1.5 on the rod's side. Plane1 (F4) 30 above the component's XY,
# Plane2 (F5) 10 above Plane1.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Plug", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "thread", "faces": [{"body": "F2.b0", "face": "F2:side(c1)"}],
    "thread": {"designation": "M10x1.5", "class": "6g"}}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "F4", "distance": 10}}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Bodies shown: F2.b0 in O1" "the rod shown in Plug:1"
ui_step "front view (search)"            ui_command "Front View"
ui_step "fit (F6)"                       ui_key F6

echo "--- Thread: the loaded face in the occurrence"
ui_mark
ui_step "double-click Thread1"           ui_double_click_logged "Timeline Thread1"
ui_expect_new "Editing F3 with Thread" "editing Thread1"
ui_expect_new "Thread Faces: 1 face [face $SIDE of F2.b0 in O1]" "the loaded face, in O1"
ui_expect_new "View highlight places: " "the highlights' places logged"
expect_place "face F2.b0/$SIDE" 60 0 10 "the rod's side highlighted on the placed rod"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_expect_new "Pick face F2.b0/$SIDE at" "the rod's side can be picked"
ui_mark
ui_step "click the rod's side"           ui_click_pick "face F2.b0/$SIDE"
ui_expect_new "Thread Faces: nothing" "the click took the loaded face out"
grep -q "Thread Faces: 2 faces" "$UI_LOG" && ui_fail "the face was listed twice"
echo "ok   the face never listed twice"
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Thread cancelled" "edit cancelled"

echo "--- Offset plane: the loaded plane in the occurrence"
ui_mark
ui_step "double-click Plane2"            ui_double_click_logged "Timeline Plane2"
ui_expect_new "Editing F5 with Offset Plane" "editing Plane2"
ui_expect_new "Offset Plane Plane: 1 plane [plane F4 in O1]" "the loaded plane, in O1"
expect_place "plane F4" 60 0 30 "Plane1 highlighted where the view shows it"
ui_expect_log "Datum F4 in O1 at" "Plane1 can be picked"
ui_mark
ui_step "click Plane1"                   ui_click_logged "Datum F4 in O1"
ui_expect_new "Offset Plane Plane: nothing" "the click took the loaded plane out"
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Offset Plane cancelled" "edit cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI placed edit test"
