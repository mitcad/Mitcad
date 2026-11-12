#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSession.cpp app/framework/Selection.cpp
# check-all sources: core/model/src/features/extrude.rs tools/cli
# Extrude's profile clicks toggle one entry per region (mitcad#199),
# through the real UI, in a sketch of a placed component (where the view
# places the profiles by the component's occurrence):
#   - The newest profile, taken when Extrude starts, is the region the view
#     shows: a click on it takes it out instead of listing it twice (the
#     "profile ... is listed more than once" the Cut failed with).
#   - Nested squares A, B, C: clicks on the ring between A and B, the ring
#     between B and C and the square C, then the middle ring again, leave
#     the odd/even area "A minus B plus C"; cut from a block below, it
#     takes away just that area.
#   - The saved definition lists each region once; editing the extrusion
#     shows them where the view shows the sketch.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-profile-toggle-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-profile-toggle.XXXXXX)
FILE=$WORK/toggle.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
# The squares' regions in Sketch2: lines c1..c4 (A), c9..c12 (B) and
# c17..c20 (C), each rectangle's corner points after its lines.
RING_A='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
RING_B='r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}'

# expect_file expression expected description: a Python expression on the
# saved project file (doc).
expect_file() {
  local actual
  actual=$(python3 -c 'import json, sys; doc = json.load(open(sys.argv[1])); print(eval(sys.argv[2]))' \
    "$FILE" "$1") || ui_fail "$3: cannot read $FILE"
  [ "$actual" = "$2" ] || ui_fail "$3: $1 is '$actual', expected '$2'"
  echo "ok   $3"
}

# Part:1 (O1, C1): Sketch1 (F1) 80 x 80 about the origin, Extrude1 (F2)
# from 20 below XY 10 up: Body1 (F2.b0), 64000 mm3. Sketch2 (F3) on XY,
# above the block, with squares A (60), B (40) and C (20) nested about the
# origin: the ring a between A and B, the ring b between B and C, the
# square c.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-40, -40], "width": 80, "height": 80},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "$RING_A"}],
    "start": {"type": "offset", "offset": -20},
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-30, -30], "width": 60, "height": 60},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-20, -20], "width": 40, "height": 40},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [-10, -10], "width": 20, "height": 20}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
# A design opens with the component it was saved with active (mitcad#1678):
# Part.
ui_expect_log "Browser radio Root/Part:1 at" "Part in the browser"
ui_expect_log "Opened with C1 active, as saved (unmodified)" "Part is active"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0 in O1" "Part's block shown"

echo "--- The newest profile, clicked, goes out"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile " "the newest profile taken"
SQUARE_C=$(grep "Extrude Profiles: 1 profile \[profile " "$UI_LOG" | tail -1 |
  sed -n 's/.*\[profile \(r{[^ ]*}\) of F3 in O1\].*/\1/p')
[ -n "$SQUARE_C" ] || ui_fail "the newest profile is not Sketch2's where Part:1 shows it: $(grep 'Extrude Profiles:' "$UI_LOG" | tail -1)"
[ "$SQUARE_C" != "$RING_A" ] && [ "$SQUARE_C" != "$RING_B" ] || ui_fail "the newest profile is $SQUARE_C"
echo "ok   the square c, in O1"
ui_expect_new "Pick profile F3/" "Sketch2's profiles can be picked"
ui_mark
ui_step "click the square c"             ui_click_pick "profile F3/$SQUARE_C"
ui_expect_new "Extrude Profiles: nothing" "the click took the newest profile out"

echo "--- Nested squares: a, b, c, then b again"
ui_mark
ui_step "click the ring a"               ui_click_pick "profile F3/$RING_A"
ui_expect_new "Extrude Profiles: 1 profile [profile $RING_A of F3 in O1]" "the ring a"
ui_mark
ui_step "click the ring b"               ui_click_pick "profile F3/$RING_B"
ui_expect_new "Extrude Profiles: 2 profiles" "the ring b too"
ui_mark
ui_step "click the square c"             ui_click_pick "profile F3/$SQUARE_C"
ui_expect_new "Extrude Profiles: 3 profiles" "all of A"
ui_mark
ui_step "click the ring b again"         ui_click_pick "profile F3/$RING_B"
ui_expect_new "Extrude Profiles: 2 profiles [profile $RING_A of F3 in O1; profile $SQUARE_C of F3 in O1]" \
  "A minus B plus C: the ring a and the square c"
grep -q "is listed more than once" "$UI_LOG" && ui_fail "a profile was listed twice"
echo "ok   no profile listed twice"

echo "--- Cut from the block"
ui_mark
ui_step "Cut"                            ui_choose "Panel Extrude input operation" 2
ui_expect_new "Extrude: Operation = Cut" "cut chosen"
ui_mark
ui_step "distance -25"                   ui_type_in "Panel Extrude input distance" "-25"
ui_expect_new "Extrude Distance: -25 = -25 mm" "down through the block"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (Cut), distance -25" "cut added"
# 10 mm of the block under the ring a (60^2 - 40^2) and the square c (20^2)
# go. The cut goes through the block, so the column under the ring b
# (40^2 - 20^2) is cut off from the rest: a piece a cut splits off is a
# body of its own (Extrude's `creates_bodies`).
ui_expect_volume "Body Body1 (F2.b0): volume 64000.000 -> \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 64000 - 10 * 3600 }')" "the odd/even area cut: the block's frame"
ui_expect_volume "New body Body2 (F4.b0): volume \([0-9.]*\) mm3" \
  "$(awk 'BEGIN { printf "%.6f", 10 * (1600 - 400) }')" "the column under the ring b, a body of its own"
ui_mark
ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_file '[p["region"] for p in doc["features"][3]["profiles"]]' "['$RING_A', '$SQUARE_C']" \
  "Extrude2 lists the ring a and the square c, each once"

echo "--- Editing shows the regions where the view shows them"
ui_mark
ui_step "double-click Extrude2"          ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F4 with Extrude" "editing Extrude2"
ui_expect_new "Extrude Profiles: 2 profiles [profile $RING_A of F3 in O1; profile $SQUARE_C of F3 in O1]" \
  "the loaded regions, in O1"
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Extrude cancelled" "edit cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI profile toggle test"
