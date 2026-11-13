#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/framework/Selection.cpp app/framework/Selection.hpp
# check-all sources: app/framework/VisibleTextGuard.cpp app/framework/VisibleTextGuard.hpp
# check-all sources: core/model-base/src/profile.rs core/model-api/src/sketch.rs tools/cli
# The selection card says what is selected in words (mitcad#1741, GitHub
# mitcad/Mitcad#308), through the real UI, on a design mitcad-cli makes:
# Sketch1 (F1) on XY with a 40 x 30 mm rectangle from the origin.
#   1. Its profile selected with no command open: the card says "Profile
#      of Sketch1" with its area (1200 mm²) and perimeter (140 mm), and
#      neither the region key (r{…}) nor a topological name or an id.
#   2. Its bottom line selected: "Line of Sketch1" with its length (40 mm),
#      no id.
# The visible-text guard (tools/ui-test-lib.sh, ui_guard_check) reads the
# card at every step too.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-idle-card-words-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-idle-card-words.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/card.mitcad

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_card "text" "description": the card's last logged text has it
# (up to 6 s), and no region key, topological name or sketch item id.
expect_card() {
  local card=""
  for _ in $(seq 1 30); do
    card=$(grep -F "Idle card sketch items: " "$UI_LOG" | tail -1)
    [[ $card == *"$1"* ]] && break
    sleep 0.2
  done
  [[ $card == *"$1"* ]] || ui_fail "$2: '$1' not on the card: $card"
  if grep -qE '(^|[^A-Za-z0-9_])([EVr]\{|F[0-9]+:(side|top|bottom|end|start)|[kpc][0-9]+([^A-Za-z0-9_]|$))' \
    <<< "${card#Idle card sketch items: }"; then
    ui_fail "$2: an internal name on the card: $card"
  fi
  echo "ok   $2"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Timeline Sketch1 at" "the timeline is logged"

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

echo "--- 1. The profile"
# Extrude once for the log's pick places of the profiles: it takes the
# rectangle, the newest profile. Cancelled.
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Pick profile F1/$RECT at" "the profile can be picked"
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"
ui_mark
ui_step "select the rectangle's profile" ui_click_pick "profile F1/$RECT"
ui_expect_new "Selected: 1 profile" "the profile selected"
expect_card "Profile of Sketch1" "the card names the profile and its sketch"
expect_card "Area: 1200 mm²" "the card gives its area"
expect_card "Perimeter: 140 mm" "the card gives its perimeter"

echo "--- 2. The bottom line"
# Selected in the sketch, at a sketch point of it; the selection stays when
# the sketch is finished.
ui_step "clear the selection (Esc)"      ui_key Escape
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "Sketch1 is edited"
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "select the bottom line"         ui_sketch_click 20 0
ui_expect_new "Selected: 1 sketch curve" "the line selected"
ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_expect_log "Sketch finished" "back to the design, no command open"
expect_card "Line of Sketch1" "the card names the line and its sketch"
expect_card "Length: 40 mm" "the card gives its length"

ui_finish "UI idle card words test"
