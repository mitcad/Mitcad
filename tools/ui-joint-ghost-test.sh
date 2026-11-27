#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/framework/Command.hpp app/framework/CommandSession.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp tools/cli
# The ghost of side A in Joint and As-Built Joint (mitcad#147) through the
# real UI, on a design mitcad-cli makes: Base:1 (a 10 x 10 x 10 block at
# the origin, grounded) and Lid:1 (a 40 x 40 x 5 block above it at
# z = 30), seen from the top, so that the lid hides the base:
#   - Joint: before origin A is picked the base's top face cannot be
#     picked; once the lid's top face is origin A the lid is a ghost and
#     the base's top face can be picked through it and its highlight.
#   - Re-picking origin A (its input activated) shows the lid normally;
#     the lid's face clicked again deselects it; picked again, the lid is a
#     ghost again; origin B through it; the preview shows the lid where the
#     joint puts it, normally; flipped, OK adds the joint: the lid lies on
#     the base, still hiding it.
#   - As-Built Joint: the lid's body clicked as component A ghosts it; a
#     click where the base lies behind it takes the base as component B.
#   - Esc with a ghost shown restores the bodies.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-joint-ghost-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-joint-ghost.XXXXXX)
FILE=$WORK/ghost.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Base (C1, Base:1 O1): Sketch1 F1, Extrude1 F2 (Body1); Lid (C2, Lid:1
# O2): Sketch2 F3, Extrude2 F4 (Body2).
sed "s/RECT/$RECT/g" > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Base"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Lid", "transform": {"translation": [-15, -15, 30]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Base:1"}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# check "description" expectations...: the saved design opened by mitcad-cli.
check() {
  local description=$1; shift
  local IFS=,
  echo "[$*]" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$description"; }
  echo "ok   $description"
}

# The pick places logged last (after the last "Pick places:").
last_places() {
  local start
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  tail -n +"${start:-1}" "$UI_LOG" | grep "^Pick "
}

# expect_side A|B occurrence "description": As-Built Joint's last line for
# the side names the component in that occurrence (a body clicked in the
# view stands for the occurrence that places it).
expect_side() {
  grep "^As-Built Joint Component $1: " "$UI_LOG" | tail -1 | grep -qF " in $2]" ||
    ui_fail "$3: $(grep "^As-Built Joint Component $1: " "$UI_LOG" | tail -1)"
  echo "ok   $3"
}

ui_start_display
ui_start_app --open "$FILE"
ui_mark
ui_step "top view (search)"              ui_command "Top View"
ui_expect_new "Camera direction 0 0 -1" "looking down: the lid hides the base" 8

echo "--- Joint: origin B through the ghost of A"
ui_mark
ui_step "joint (J)"                      ui_key j
ui_expect_new "Command Joint started" "Joint started"
ui_expect_new "Pick face F4.b0/F4:end(" "the lid's top face can be picked"
last_places | grep -qF "face F2.b0/F2:end(" && ui_fail "the base's top face could be picked through the lid"
echo "ok   the base's top face cannot be picked: the lid hides it"
ui_mark
ui_step "pick the lid's top face"        ui_click_pick "face F4.b0/F4:end("
ui_expect_new "Joint Origin A: 1 face [face F4:end($RECT) of F4.b0 in O2]" "origin A on the lid"
ui_expect_new "View ghosts: O2 (1 body)" "the lid is a ghost"
ui_expect_new "Pick face F2.b0/F2:end(" "the base's top face can be picked through the lid and its highlight"

echo "--- Re-picking and deselecting origin A"
ui_mark
ui_step "activate origin A"              ui_click_logged "Panel Joint input a"
ui_expect_new "View ghosts: none" "re-picking origin A shows the lid normally"
ui_expect_new "Pick face F4.b0/F4:end(" "the lid's top face can be clicked again"
ui_mark
ui_step "click the lid's top face"       ui_click_pick "face F4.b0/F4:end("
ui_expect_new "Joint Origin A: nothing" "origin A deselected"
grep -q "View ghosts: O2" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a deselected origin A left the lid a ghost"
echo "ok   the lid stays normal"
ui_mark
ui_step "pick the lid's top face again"  ui_click_pick "face F4.b0/F4:end("
ui_expect_new "Joint Origin A: 1 face [face F4:end($RECT) of F4.b0 in O2]" "origin A on the lid again"
ui_expect_new "View ghosts: O2 (1 body)" "the lid is a ghost again"
ui_expect_new "Pick face F2.b0/F2:end(" "the base's top face can be picked through it"
# The lid as a ghost, its top face highlighted, the base under it.
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "pick the base's top face"       ui_click_pick "face F2.b0/F2:end("
ui_expect_new "Joint Origin B: 1 face [face F2:end($RECT) of F2.b0 in O1]" "origin B on the base, through the lid"
ui_expect_new "View ghosts: none" "with both origins the lid is shown normally"
ui_expect_new "Preview Joint: ok" "previewed"
ui_expect_new "Preview Joint moves O2" "the preview shows the lid where the joint puts it"
ui_mark
ui_step "flip"                           ui_click_logged "Panel Joint input flip"
ui_expect_new "Joint: Flip = on" "flipped: the lid lies on the base"
ui_expect_new "Preview Joint: ok" "previewed"
grep -q "View ghosts: O2" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the lid became a ghost again"
echo "ok   the lid stays normal in the preview"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added joint Joint1 (rigid): placed, moved O2" "the lid placed by the joint"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
check "the lid lies on the base, its top face on the base's top face" \
  '{"expect": {"occurrence": "Lid:1", "body": "Body2", "center": [5, 5, 12.5], "tolerance": 1e-6}}'

echo "--- As-Built Joint: component B through the ghost of A"
ui_mark
ui_step "as-built joint (search)"        ui_command "As-Built Joint"
ui_expect_new "Command As-Built Joint started" "As-Built Joint started"
ui_mark
ui_step "click the lid"                  ui_click_logged "Body F4.b0 in O2"
ui_expect_new "As-Built Joint Component A: 1 component" "A picked"
expect_side A O2 "A: the lid"
ui_expect_new "View ghosts: O2 (1 body)" "the lid is a ghost"
ui_mark
ui_step "click the base behind the lid"  ui_click_logged "Body F2.b0 in O1"
ui_expect_new "As-Built Joint Component B: 1 component" "B picked"
expect_side B O1 "B: the base, through the lid"
ui_expect_new "View ghosts: none" "with both components the lid is shown normally"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command As-Built Joint cancelled" "cancelled"

echo "--- Esc with the ghost shown"
ui_mark
ui_step "joint (J)"                      ui_key j
# The lid lies upside down now: the face it shows upwards.
ui_expect_new "Pick face F4.b0/" "Joint started"
ui_mark
ui_step "pick the lid's upper face"      ui_click_pick "face F4.b0/"
ui_expect_new "View ghosts: O2 (1 body)" "the lid is a ghost"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "View ghosts: none" "Esc shows the lid normally"
ui_expect_new "Command Joint cancelled" "cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI joint ghost test"
