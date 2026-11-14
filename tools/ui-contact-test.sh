#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowJoints.cpp app/MainWindowMotion.cpp app/motion app/commands app/browser
# check-all sources: core/model/src/contact.rs core/model/src/document/motion.rs geometry/src/motion_contact.cpp tools/cli
# Contact sets (mitcad#454) through the real UI, on a design mitcad-cli
# makes: Plate:1 (a 40 x 40 x 10 block, grounded), Pin:1 (a 10 x 10 x 20
# block) sliding along x on the plate (Joint1; at tx = 0 from x = 15 to 25),
# Wall:1 (a 5 x 40 x 10 block standing on the plate at x = 35, grounded),
# the contact set Contact1 of the pin and the wall, and the study Push
# (the pin 20 mm at step 10 of 10). Each motion toward the wall stops where
# the pin touches it (tx = 10):
#   - Drive Joint to 20 mm: the panel says where it stopped; kept, saved
#     and checked by mitcad-cli.
#   - Animate Joint: the animation turns back at the contact on every pass
#     (mitcad#1790), the status bar saying where once, until Esc.
#   - The study played: the playback stops at step 5.
#   - Contact Set's panel edits the set (suppressed): the pin drives
#     through the wall.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-contact-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-contact.XXXXXX)
FILE=$WORK/contact.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Plate (O1): F1, F2 (Body1); Pin (O2): F3, F4 (Body2); Wall (O3): F5, F6
# (Body3); Joint1 F7.
sed "s/RECT/$RECT/g" > "$WORK/model.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Plate"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Pin", "transform": {"translation": [100, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Wall", "transform": {"translation": [35, 0, 10]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 0], "width": 5, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "ground_occurrence", "occurrence": "Plate:1"},
  {"cmd": "ground_occurrence", "occurrence": "Wall:1"},
  {"cmd": "add_joint", "kind": "slider", "slide_axis": "x", "flip": true, "position": {"tx": 0},
    "a": {"occurrence": "Pin:1", "geometry": {"body": "F4.b0", "face": "F4:start(RECT)"}},
    "b": {"occurrence": "Plate:1", "geometry": {"body": "F2.b0", "face": "F2:end(RECT)"}}},
  {"cmd": "add_contact_set", "occurrences": ["Pin:1", "Wall:1"]},
  {"cmd": "add_motion_study", "name": "Push", "steps": 10, "keys": [
    {"joint": "Joint1", "motion": "tx", "points": [[0, 0], [10, 20]]}]},
  {"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [20, 20, 20], "tolerance": 1e-6}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

check() {
  printf '[%s]\n' "$2" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$1"; }
  echo "ok   $1"
}

# between line "sed pattern" low high description: the number the pattern
# takes from the last log line with `line` lies between low and high.
between() {
  local value
  value=$(grep -F -- "$1" "$UI_LOG" | tail -1 | sed -n "$2")
  awk -v v="$value" -v lo="$3" -v hi="$4" 'BEGIN { exit !(v != "" && v >= lo && v <= hi) }' ||
    ui_fail "$5: '$value' is not between $3 and $4"
  echo "ok   $5 ($value)"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Contact Sets at" "the browser's Contact Sets folder"
# A design longer than the browser opens folded below the root (mitcad#426).
if ! grep -q "Browser Root/Contact Sets/Contact1 at" "$UI_LOG"; then
  ui_step "open the folder"              ui_click_logged "Browser arrow Root/Contact Sets"
fi
ui_expect_log "Browser Root/Contact Sets/Contact1 at" "the browser lists the contact set"

echo "--- Drive Joint into the wall"
ui_mark
ui_step "drive joint (search)"           ui_command "Drive Joint"
ui_expect_new "Command Drive Joint started" "Drive Joint started"
ui_mark
ui_step "20 mm"                          ui_type_in "Panel Drive Joint input distance" "20"
ui_expect_new "Preview Drive Joint: ok, warning: stopped at contact: Body2 (Pin:1) and Body3 (Wall:1) in Contact1" \
  "the panel says where the contact stopped it"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Drove joint Joint1" "kept"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
check "the pin stopped at the wall" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [30, 20, 20], "tolerance": 0.02}}'
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Drive Joint1" "back at the start"

echo "--- Animate Joint into the wall: it turns back at the contact on every pass"
ui_mark
ui_step "animate (search)"               ui_command "Animate Joint"
ui_expect_new "Animating Joint1 tx: back and forth from -25 mm to 25 mm" "the animation starts"
ui_expect_new "Animation of Joint1 turned back by contact at tx " "the animation turns back at the contact" 30
between "Animation of Joint1 turned back by contact at tx " 's/.* at tx \([-0-9.]*\) mm: .*/\1/p' 9.99 10.011 \
  "the pin turned back at tx = 10"
# Two passes that end at the wall (mitcad#1790): the stop shows on every
# pass; the status bar said where once.
contacts=0
for _ in $(seq 1 300); do
  contacts=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -e "Animation of Joint1: pass " | tr -d ' ')
  [ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Animation of Joint1: pass " | grep -cF " (contact), ")" -ge 2 ] && break
  sleep 0.1
done
passes=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Animation of Joint1: pass ")
[ "$(grep -cF " (contact), " <<< "$passes")" -ge 2 ] || ui_fail "fewer than two passes ended at the wall: '$passes'"
grep -q " down to " <<< "$passes" || ui_fail "the pin did not go back from the wall: '$passes'"
[ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF "Animation of Joint1 turned back by contact")" = 1 ] ||
  ui_fail "the contact was said more than once"
echo "ok   two passes turned back at the wall ($contacts passes), said once"
ui_mark
ui_step "stop (Esc)"                     ui_key Escape
ui_expect_new "Animation of Joint1 stopped: Esc (" "Esc stops it"

echo "--- The study played into the wall"
ui_mark
ui_step "motion study (search)"          ui_command "Motion Study"
ui_expect_new "Motion study Push opened: 10 steps, 1 keys" "the study's panel"
ui_focus_dialog "Motion Study"
ui_mark
ui_step "play (Alt+P)"                   ui_key alt+p
ui_expect_new "Playback of Push stopped at step " "the playback stops at the contact" 60
between "Playback of Push stopped at step " 's/.* at step \([-0-9.]*\): .*/\1/p' 4.995 5.006 \
  "stopped at step 5"
ui_expect_new "Paused Push at step" "paused there"
ui_step "close (Esc)"                    ui_key Escape
ui_expect_log "Motion study Push closed" "closed"
ui_focus_main

echo "--- The contact set suppressed in its panel"
ui_mark
ui_step "contact set's menu"             ui_click_logged "Browser Root/Contact Sets/Contact1" 3
ui_expect_new "Context menu: Edit Contact1" "the set's menu"
ui_step "edit"                           ui_menu_choose "Edit Contact1"
ui_expect_new "Editing analysis Contact1 with Contact Set" "the set in its panel"
ui_expect_new "Contact Set Components: 2" "both components"
ui_step "suppressed"                     ui_click_logged "Panel Contact Set input suppressed"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited contact set Contact1" "edited"
ui_mark
ui_step "drive joint (search)"           ui_command "Drive Joint"
ui_mark
ui_step "20 mm"                          ui_type_in "Panel Drive Joint input distance" "20"
ui_expect_new "Preview Drive Joint: ok" "previewed"
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "stopped at contact"; then
  ui_fail "the suppressed set still stopped the pin"
fi
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Drove joint Joint1" "kept"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
check "the pin passed through the wall" \
  '{"expect": {"occurrence": "Pin:1", "body": "Body2", "center": [40, 20, 20], "tolerance": 1e-6}},
   {"expect": {"query": {"query": "contact_sets"}, "result": [{"name": "Contact1", "suppressed": true}]}}'

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI contact test"
