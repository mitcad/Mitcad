#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionAuto.cpp
# check-all sources: app/framework/AutoOperationPolicy.hpp app/framework/Command.hpp
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/extrude_scope.rs
# check-all sources: geometry/src/boolean.cpp tools/cli
# Cut or Join chosen by hand in a new extrusion that misses one way and
# works the other turns Flip once (mitcad#2283, the mirror of
# mitcad#2216), through the real UI, on a design mitcad-cli makes: a
# 40 x 30 block 20 deep under the XY plane (Body1, F2.b0; its top face lies
# in XY), a 40 x 30 block 20 to each side of XY at x 100 (F4.b0), and on XY
# a square far from both (Sketch3, F5), a square inside the second block
# (Sketch4, F6) and a square on the first block's top face (Sketch5, F7;
# Extrude takes the newest unused sketch first). A new Extrude starts at
# 0 mm (mitcad#2481): each gets 10 mm typed first.
#   1. The square on the face: automatic Join; Cut chosen by hand misses
#      upward and cuts downward: Flip turns on by itself, the cut
#      previews, no other operation is tried (mitcad#2216's retry stays
#      off); OK cuts 1000 mm3 out of the block.
#   2. The square inside the second block: automatic Cut; Join by hand
#      (merges with the block: no turn, or, should it fail, it fails both
#      ways and comes back); Cut by hand: it works going up, Flip stays
#      off; OK cuts 1000 mm3 out of that block.
#   3. The far square: Cut by hand misses both ways: Operation, Flip and
#      the failure stay as they are (no turn, no switch to Join).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flip-direction-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-flip-direction.XXXXXX)
FILE=$WORK/turn.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# no_new "text" description: no such line since the mark.
no_new() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2"
  echo "ok   $2"
}

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [100, 0], "width": 40, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "symmetric", "distance": 20, "full_length": false}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [200, 0], "width": 10, "height": 10},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [110, 10], "width": 10, "height": 10},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F7", "corner": [10, 10], "width": 10, "height": 10}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0, F4.b0" "both blocks shown"

echo "--- 1. Cut by hand on the face: Flip turns on, the cut previews"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F7]" "the square on the block's face taken"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Extrude: automatic Operation = Join" "Join at once (10 mm typed, up from the face)"
ui_expect_new "Preview Extrude: ok" "the join previewed"
ui_wait_idle
ui_mark
ui_step "Cut by hand"                    ui_choose_text "Panel Extrude input operation" "Cut"
ui_expect_new "Extrude: Operation = Cut" "Cut chosen"
ui_expect_new "Extrude: operation chosen by hand" "the automatic choice is off"
ui_expect_new "Extrude: Cut misses, flipped into the body, Flip = on" "the cut missing upward turns Flip on"
ui_expect_new "Extrude: the flipped Cut works, Flip = on stays" "the cut works going down"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
ui_wait_idle
no_new "Preview Extrude: failed" "the miss is never shown"
no_new "Extrude: flipped" "the turn tries no other operation (mitcad#2216's retry stays off)"
no_new "Extrude: Operation = Join" "Operation stays Cut"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (Cut)" "cut"
ui_expect_volume "(F2.b0): volume 24000.000 -> \([0-9.]*\) mm3" 23000 "the square's 1000 mm3 cut out of the block"

echo "--- 2. A square inside a block: Cut by hand works, Flip stays off"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F6]" "the square inside the second block taken"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Extrude: automatic Operation = Cut" "Cut at once (10 mm typed, inside the block)"
ui_expect_new "Preview Extrude: ok" "the cut previewed"
ui_wait_idle
ui_mark
ui_step "Join by hand"                   ui_choose_text "Panel Extrude input operation" "Join"
ui_expect_new "Extrude: Operation = Join" "Join chosen"
ui_expect_new "Extrude: operation chosen by hand" "the automatic choice is off"
ui_wait_idle
if tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Extrude: Join misses"; then
  # A tool wholly inside a body that the Join refuses: it misses the other
  # way too, and the direction comes back.
  ui_expect_new "Extrude: Join misses both ways, Flip = off again" "the Join inside fails both ways: Flip back off"
else
  no_new "Preview Extrude: failed" "the Join inside the block merges with it"
  no_new "Flip = on" "a Join that works is never turned"
fi
ui_mark
ui_step "Cut by hand again"              ui_choose_text "Panel Extrude input operation" "Cut"
ui_expect_new "Extrude: Operation = Cut" "Cut chosen again"
ui_expect_new "Preview Extrude: ok" "the cut inside works going up"
ui_wait_idle
no_new "misses" "a cut that works is never turned"
no_new "Flip = " "Flip stays off"
no_new "Extrude: flipped" "nothing else is tried"
ui_focus_main
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (Cut)" "cut"
ui_expect_volume "(F4.b0): volume 48000.000 -> \([0-9.]*\) mm3" 47000 "the square's 1000 mm3 cut out of the second block"

echo "--- 3. Cut by hand away from the bodies: it misses both ways, nothing changes"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Extrude Profiles: 1 profile [profile $R of F5]" "the far square taken"
ui_step "distance 10 mm"                 ui_type_in "Panel Extrude input distance" "10 mm"
ui_expect_new "Preview Extrude: ok" "the new body previewed"
ui_wait_idle
ui_mark
ui_step "Cut by hand"                    ui_choose_text "Panel Extrude input operation" "Cut"
ui_expect_new "Extrude: Operation = Cut" "Cut chosen"
ui_expect_new "Preview Extrude: failed: No eligible components intersect the cut" "the cut misses"
ui_wait_idle
no_new "Try Flip" "the model finds nothing the other way either"
no_new "misses" "a cut that misses both ways is not turned"
no_new "Flip = " "Flip stays off"
no_new "Extrude: flipped" "no other operation is tried"
no_new "Extrude: Operation = Join" "no switch to Join: Operation stays Cut, its failure shown"
ui_focus_main
ui_mark
ui_step "cancel (Escape)"                ui_key Escape
ui_expect_new "Command Extrude cancelled" "cancelled"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI flip direction test"
