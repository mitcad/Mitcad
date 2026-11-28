#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/PatternOptions.cpp
# check-all sources: app/framework/CommandSession.cpp core/model/src/features/pattern.rs
# check-all sources: core/model/src/sheet/body_copy.rs tools/cli
# Circular Pattern's Operation (mitcad#2617) through the real UI. Design
# (mitcad-cli): a 20 x 20 x 10 block in the first quadrant at the Z axis
# (Body1, F2.b0, 4000 mm3); sheet metal rules (t = 1) and a flat 10 x 10
# sheet 60 mm from the axis (F5.b0).
#   1. Circular Pattern of Body1 about Z, 4 over a full turn: the panel
#      starts with New Body; Join previews, and OK joins the four quarters
#      into Body1 (16000 mm3), no new bodies.
#   2. The pattern edited: the panel shows Join; New Body previews and OK
#      makes the three copies bodies of their own again (Body1 4000 mm3).
#   3. The Operation row is shown for Bodies only: hidden for Features and
#      Faces, shown again for Bodies.
#   4. The far sheet patterned with Join: its copies do not touch it, the
#      preview fails and the panel says why.
#   5. A design whose pattern already carries the join flag (written by
#      mitcad-cli) opened in the Floating layout: its panel shows Join.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-circular-pattern-join-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-circular-pattern-join.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Sketch1 (F1), Extrude1 (F2): the block, Body1; sheet rules (F3), Sketch2
# (F4), Face (F5): the far sheet.
cat > "$WORK/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "sheet_rules", "thickness": 1, "bend_radius": 1,
    "allowance": {"type": "k_factor", "k": 0.4}, "reference_side": "front",
    "bend_relief": {"type": "rectangular", "width": 1, "depth": 1},
    "corner_relief": {"type": "round", "width": 2, "depth": 2}}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [60, -5], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "sheet_face", "rules": "F3",
    "profiles": [{"sketch": "F4", "region": "$RECT"}]}},
  {"expect": {"body": "F2.b0", "volume": 4000}},
  {"expect": {"body": "F5.b0", "volume": 100}}
]
EOF
"$CLI" run "$WORK/design.json" --save "$WORK/design.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The block with a circular pattern joined by mitcad-cli (CircularPattern1,
# F3), not by the panel.
cat > "$WORK/joined.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "circular_pattern", "objects": {"type": "bodies", "bodies": ["F2.b0"]},
    "axis": {"type": "origin", "axis": "z"}, "quantity": 4, "angle": 6.283185307179586, "combine": true}},
  {"expect": {"feature": "F3", "status": "ok", "bodies": 1}},
  {"expect": {"body": "F2.b0", "volume": 16000}}
]
EOF
"$CLI" run "$WORK/joined.json" --save "$WORK/joined.mitcad" > "$WORK/joined.log" 2>&1 ||
  { cat "$WORK/joined.log"; ui_fail "mitcad-cli (the joined pattern)"; }

# expect_hidden / expect_shown "Panel <command> input <id>" description: the
# input's last logged place says it is hidden or shown (as
# tools/ui-pattern-two-axes-test.sh).
expect_hidden() {
  for _ in $(seq 1 30); do
    grep -F "$1 at " "$UI_LOG" | tail -1 | grep -q '(hidden)$' && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: the last place of '$1' is not hidden"
}
expect_shown() {
  for _ in $(seq 1 30); do
    grep -F "$1 at " "$UI_LOG" | tail -1 | grep -q '[0-9]$' && { echo "ok   $2"; return; }
    sleep 0.2
  done
  ui_fail "$2: the last place of '$1' is not shown"
}

# edit_timeline "Timeline <name>": the cell clicked once and the layout let
# settle before the double-click (as tools/ui-pattern-options-test.sh).
edit_timeline() {
  ui_click_logged "$1"
  ui_wait_idle
  sleep 0.5
  ui_sync
  ui_double_click_logged "$1"
}

# pattern_about_z body: Circular Pattern started, the body picked and the Z
# axis as its axis (4 over a full turn, the defaults).
pattern_about_z() {
  ui_step "circular pattern (search)"      ui_command "Circular Pattern"
  ui_expect_new "Command Circular Pattern started" "Circular Pattern started"
  ui_expect_new "Panel Circular Pattern choices: object_type=bodies, compute=adjust, operation=new_body" "New Body by default"
  ui_step "pick $1"                        ui_click_logged "Body $1"
  ui_expect_new "Circular Pattern Objects: 1 body [body $1]" "$1 picked"
  ui_step "the axis input"                 ui_click_logged "Panel Circular Pattern input axis"
  # The origin's axes in the view as its light bulb shows them (mitcad#1030).
  ui_show_origin
  ui_step "pick the Z axis"                ui_click_logged "Datum z"
  ui_expect_new "Circular Pattern Axis: 1 axis [axis z]" "Z picked"
}

ui_start_display
ui_start_app --open "$WORK/design.mitcad"
ui_step "fit (F6)"                         ui_key F6
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- 1. The block about Z, joined"
ui_mark
pattern_about_z F2.b0
expect_shown "Panel Circular Pattern input operation" "Operation shown for bodies"
ui_expect_new "Preview Circular Pattern: ok" "New Body previewed" 30
ui_mark
ui_step "Join"                             ui_choose "Panel Circular Pattern input operation" 1
ui_expect_new "Circular Pattern: Operation = Join" "Join chosen"
ui_expect_new "Preview Circular Pattern: ok" "Join previewed" 30
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added Circular Pattern as F6" "CircularPattern1 added" 30
# The volumes are logged after the "Added" line: waited for as new lines.
ui_expect_new "Body Body1 (F2.b0): volume 4000.000 -> 16000.000 mm3" "the four quarters one body" 30
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "New body [A-Za-z0-9]* (F6\.b" &&
  ui_fail "the joined pattern made bodies of its own"
echo "ok   no copies of their own"

echo "--- 2. Edited: the panel shows Join, New Body separates the copies"
ui_mark
ui_step "edit CircularPattern1"            edit_timeline "Timeline CircularPattern1"
ui_expect_new "Editing F6 with Circular Pattern" "editing the pattern"
ui_expect_new "Panel Circular Pattern choices: object_type=bodies, compute=adjust, operation=join" "Join restored"
ui_expect_new "Preview Circular Pattern: ok" "the joined pattern previewed" 30
ui_mark
ui_step "New Body"                         ui_choose "Panel Circular Pattern input operation" -1
ui_expect_new "Circular Pattern: Operation = New Body" "New Body chosen"
ui_expect_new "Preview Circular Pattern: ok" "New Body previewed" 30
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Edited F6" "the pattern edited" 30
ui_expect_new "Body Body1 (F2.b0): volume 16000.000 -> 4000.000 mm3" "Body1 a quarter again" 30
ui_expect_new "(F6.b2): volume 4000.000 mm3" "the third copy a body of its own" 30

echo "--- 3. Operation for Bodies only"
ui_mark
ui_step "circular pattern (search)"        ui_command "Circular Pattern"
ui_expect_new "Command Circular Pattern started" "Circular Pattern started"
ui_mark
ui_step "Features"                         ui_choose "Panel Circular Pattern input object_type" 1
ui_expect_new "Circular Pattern: Pattern Type = Features" "pattern of features"
expect_hidden "Panel Circular Pattern input operation" "Operation hidden for features"
ui_mark
ui_step "Faces"                            ui_choose "Panel Circular Pattern input object_type" 1
ui_expect_new "Circular Pattern: Pattern Type = Faces" "pattern of faces"
expect_hidden "Panel Circular Pattern input operation" "Operation hidden for faces"
ui_mark
ui_step "Bodies"                           ui_choose "Panel Circular Pattern input object_type" -2
ui_expect_new "Circular Pattern: Pattern Type = Bodies" "pattern of bodies"
expect_shown "Panel Circular Pattern input operation" "Operation shown again for bodies"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "nothing added"

echo "--- 4. The far sheet joined: refused with the reason"
ui_mark
pattern_about_z F5.b0
ui_mark
ui_step "Join"                             ui_choose "Panel Circular Pattern input operation" 1
ui_expect_new "Circular Pattern: Operation = Join" "Join chosen"
ui_expect_new "Preview Circular Pattern: failed: " "the join refused" 30
for _ in $(seq 1 30); do
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Panel Circular Pattern error: .*does not touch it" && break
  sleep 0.2
done
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Panel Circular Pattern error: .*does not touch it" ||
  ui_fail "the panel does not say that the sheet's copies do not touch it"
echo "ok   the panel says the copies do not touch the sheet"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "nothing added"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- 5. A stored joined pattern opened in the Floating layout"
ui_stop_app
ui_start_app --open "$WORK/joined.mitcad" --chrome=floating
ui_mark
ui_step "edit CircularPattern1"            edit_timeline "Timeline CircularPattern1"
ui_expect_new "Editing F3 with Circular Pattern" "editing the stored pattern"
ui_expect_new "Panel Circular Pattern choices: object_type=bodies, compute=adjust, operation=join" "the stored join flag shows Join"
ui_expect_new "Preview Circular Pattern: ok" "the stored pattern previewed" 30
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Circular Pattern cancelled" "left as it was"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI circular pattern join test"
