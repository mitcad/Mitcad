#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreviewBodies.cpp app/framework/CommandSession.cpp app/browser
# check-all sources: core/model/src/document.rs core/model/src/assembly.rs core/model-api/src/components.rs
# check-all sources: tools/cli
# Extrude's preview in a design of components placed off the origin, with
# hidden bodies and occurrences and a linked component, through the real UI
# (mitcad#266):
#   - Board:1 (O1, at 60, 0, 0) with a shown body F2.b0 and a hidden body
#     F3.b0; Part:1 (O2, at 0, 80, 0), a component linked from another
#     design file; Part:2 (O3, at 0, -80, 0), hidden.
#   - Extrude on a root sketch: the preview ("Preview bodies: ...", what it
#     draws and where) draws Board's and Part's bodies where the view shows
#     them, the new body in the root, no component's body at the design's
#     origin, and neither the hidden body nor the hidden occurrence.
#   - OK; then with Board active, Extrude on Board's own sketch: the new
#     body is drawn where Board:1 puts it, still nothing at the origin and
#     nothing hidden drawn.
#   - The design with Board:1 isolated: Extrude on the root sketch draws
#     Board's body and the new one, not the linked body outside the
#     isolation.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-preview-placement-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-preview-placement.XXXXXX)
FILE=$WORK/assembly.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# The linked design: a 10 x 10 x 6 block.
cat > "$WORK/part.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 6}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/part.json" --save "$WORK/part.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (part)"; }

# Board (C1, O1 at 60, 0, 0): Sketch1 (F1) 20 x 10, Extrude1 (F2) 2 up:
# F2.b0, shown; Extrude2 (F3) 3 down: F3.b0, hidden; Sketch2 (F4), a
# 6 x 6 square not used yet. The root: Sketch3 (F5), a 15 x 15 square
# beside the components, not used yet; Part (linked, O2 at 0, 80, 0) and
# its copy Part:2 (O3 at 0, -80, 0), hidden.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Board", "transform": {"translation": [60, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 2}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": -3}, "operation": "new_body"}},
  {"cmd": "set_body_visible", "uid": "F3.b0", "visible": false},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [2, 2], "width": 6, "height": 6},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [-40, 0], "width": 15, "height": 15},
  {"cmd": "insert_component", "path": "$WORK/part.mitcad", "name": "Part",
    "transform": {"translation": [0, 80, 0]}},
  {"cmd": "copy_occurrence", "occurrence": "O2", "transform": {"translation": [0, -80, 0]}},
  {"cmd": "set_occurrence_visible", "occurrence": "O3", "visible": false}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (assembly)"; }
# The same design with Board:1 isolated (the view shows only its bodies).
echo '[{"cmd": "set_isolation", "items": [{"occurrence": "O1"}]}]' > "$WORK/isolate.json"
"$CLI" run "$WORK/isolate.json" --open "$FILE" --save "$WORK/isolated.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli (isolation)"; }

# preview_bodies: the entries of the last "Preview bodies: " line since the
# mark, one per line ("F2.b0 in O1 at 60,0,0", "F7.b0 at 0,0,0 new").
preview_bodies() {
  tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -F "Preview bodies: " | tail -1 |
    sed 's/.*Preview bodies: //' | sed 's/; /\n/g'
}

# expect_drawn regex description / expect_not_drawn regex description: an
# entry of the preview matches the extended regex, or none does.
expect_drawn() {
  preview_bodies | grep -qE -- "$1" || ui_fail "$2: no '$1' in the preview: $(preview_bodies | tr '\n' ';')"
  echo "ok   $2"
}
expect_not_drawn() {
  preview_bodies | grep -qE -- "$1" && ui_fail "$2: '$1' in the preview: $(preview_bodies | tr '\n' ';')"
  echo "ok   $2"
}

# expect_hidden_stay_hidden: what the normal view hides is not drawn, and no
# component's body is drawn at the design's origin.
expect_hidden_stay_hidden() {
  expect_not_drawn '^F3\.b0 ' "Board's hidden body not drawn"
  expect_not_drawn ' in O3 ' "the hidden linked occurrence not drawn"
  expect_not_drawn ' in O[0-9/]+ at 0,0,0( new)?$' "no component's body at the design's origin"
  expect_drawn '^F2\.b0 in O1 at 60,0,0$' "Board's shown body where Board:1 puts it"
  expect_drawn '^F[0-9]+\.b[0-9]+ in O2 at 0,80,0$' "the linked body where Part:1 puts it"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "top view (search)"              ui_command "Top View"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the bodies shown"
shown=$(grep -F "Bodies shown: " "$UI_LOG" | tail -1)
case "$shown" in *"F2.b0 in O1"*) ;; *) ui_fail "Board's body not shown: $shown" ;; esac
case "$shown" in *" in O2"*) ;; *) ui_fail "the linked body not shown: $shown" ;; esac
case "$shown" in *F3.b0* | *" in O3"*) ui_fail "a hidden body shown: $shown" ;; esac
echo "ok   the view shows Board's and Part:1's bodies, not the hidden ones"

echo "--- Extrude on a root sketch"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Command Extrude started" "Extrude started with the root's square"
ui_step "type the distance"              xdotool type "15"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_expect_new "Preview bodies: " "the preview's bodies logged"
expect_hidden_stay_hidden
expect_drawn '^F[0-9]+\.b0 at 0,0,0 new$' "the new body in the root"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added extrude (New Body), distance 15" "the root's block added"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Extrude in Board, placed off the origin"
ui_mark
ui_step "activate Board (radio)"         ui_click_logged "Browser radio Root/Board:1"
ui_expect_new "Activated Board" "Board active"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Command Extrude started" "Extrude started with Board's square"
# The square lies on Board's body: Extrude chooses Join (mitcad#289); this
# test wants the new body, so New Body is chosen by hand (one up from Join).
# A new Extrude starts at 0 mm and chooses nothing until a distance is
# typed (mitcad#2481).
ui_step "type the distance"              xdotool type "5"
ui_expect_new "Extrude: automatic Operation = Join" "Join suggested: the square lies on Board's body (5 typed)"
ui_mark
ui_step "New Body (one up from Join)"    ui_choose "Panel Extrude input operation" -1
ui_expect_new "Extrude: Operation = New Body" "New Body chosen by hand"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_expect_new "Preview bodies: " "the preview's bodies logged"
expect_hidden_stay_hidden
expect_drawn '^F[0-9]+\.b0 in O1 at 60,0,0 new$' "the new body where Board:1 puts it"
expect_drawn '^F[0-9]+\.b0 at 0,0,0$' "the root's block as shown"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"

echo "--- Extrude on the root sketch with Board:1 isolated"
ui_stop_app
ui_start_app --open "$WORK/isolated.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0 in O1" "only Board:1's body shown"
grep -F "Bodies shown: " "$UI_LOG" | tail -1 | grep -qF " in O2" && ui_fail "the linked body shown in the isolation"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Command Extrude started" "Extrude started with the root's square"
ui_step "type the distance"              xdotool type "15"
ui_expect_new "Preview Extrude: ok" "previewed"
ui_expect_new "Preview bodies: " "the preview's bodies logged"
expect_drawn '^F2\.b0 in O1 at 60,0,0$' "Board's shown body where Board:1 puts it"
expect_not_drawn ' in O2 ' "the linked body outside the isolation not drawn"
expect_not_drawn '^F3\.b0 ' "Board's hidden body not drawn"
expect_not_drawn ' in O3 ' "the hidden linked occurrence not drawn"
expect_not_drawn ' in O[0-9/]+ at 0,0,0( new)?$' "no component's body at the design's origin"
expect_drawn '^F[0-9]+\.b0 at 0,0,0 new$' "the body the command makes drawn"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"

ui_finish "UI preview placement test"
