#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindow.cpp app/MainWindow.hpp app/browser tools/cli
# check-all sources: app/MainWindowSection.cpp app/framework/SectionWorker.cpp app/framework/SectionWorker.hpp
# check-all sources: app/framework/CommandSession.cpp app/framework/DocumentSession.cpp app/commands/InspectCommands.cpp
# check-all sources: core/cpp/bridge/analysis.cpp core/model-api/src/analysis.rs core/ffi/src/lib.rs geometry/io/src/mesh.cpp
# check-all sources: core/model/src/document/analyses.rs
# check-all sources: app/framework/SectionPreview.cpp app/framework/SectionPreview.hpp
# check-all sources: app/framework/CommandSessionVisibility.cpp app/commands/FilletCommands.cpp
# The caps of Section Analysis through the real UI (mitcad#382), on a design
# of the root and two components, each with a block:
#   - Each component's caps take its timeline colour (the band under its
#     features); the root's keep the default colour. One group per colour,
#     hatched in a darker shade of its own, in turn at 45 and 135 degrees.
#   - The caps keep their colours and their hatch in other visual styles and
#     with the bodies drawn in component colours.
#   - A feature preview at the section (mitcad#943): with Part active,
#     Chamfer on the top front edge of its cut block; the preview's body is
#     clipped at the plane on the section worker's thread and the caps
#     follow the preview (area 599.5 instead of 600: the bevel across the
#     cut); Escape brings back the caps of the blocks at the marker, and the
#     root is activated again.
#   - Remove Section Analysis takes the caps away.
#   - The clips off the UI thread (mitcad#438), each view round made to wait
#     3 s: the window takes a new distance while the round at x = 0 runs,
#     which is told to stop and shows nothing; the blocks are clipped and
#     meshed at x = 10 on the section worker's thread, shown, and then the
#     panel's area and length arrive, measured there too. Cancel while the
#     round at x = 12 runs: it ends as cancelled, the blocks are whole
#     again, nothing of it is shown or measured.
#   - A section turned about an axis (mitcad#823): YZ turned 45 degrees
#     about Z through a 10 mm cube's middle reads 141.42 mm²; kept, edited
#     with its Axis and Angle, and saved with them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-caps-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-section-caps.XXXXXX)
FILE=$WORK/blocks.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# Part (the timeline's first colour) at y = 0, Other (the second) at y = 30,
# the root's block at y = 60; all three cross x = 10.
cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Other"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [0, 30], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [0, 60], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The palette of app/browser/ComponentColors.hpp: the first and second
# component, and the default cap colour.
ROOT=#f29e1a
PART=#e0892a
OTHER=#3aa35b
HATCH='#[0-9a-f]\{6\}'

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"

echo "--- Section Analysis at x = 10"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_step "10 along X"                       ui_type_in "Panel Section Analysis input offset" "10"
ui_expect_log "Section caps: 3 face(s) at" "three blocks cut"
ui_expect_log "Section caps: 3 colour(s) [" "one group of caps per colour"
CAPS=$(grep "Section caps: 3 colour(s)" "$UI_LOG" | tail -1)
echo "$CAPS" | grep -q "$ROOT hatch $HATCH at 45 deg, [1-9][0-9]* lines" ||
  ui_fail "the root's caps are not $ROOT with a hatch at 45 degrees: $CAPS"
echo "$CAPS" | grep -q "$PART hatch $HATCH at 135 deg, [1-9][0-9]* lines" ||
  ui_fail "Part's caps are not $PART with a hatch at 135 degrees: $CAPS"
echo "$CAPS" | grep -q "$OTHER hatch $HATCH at 45 deg, [1-9][0-9]* lines" ||
  ui_fail "Other's caps are not $OTHER with a hatch at 45 degrees: $CAPS"
echo "ok   caps: $CAPS"
# The hatch is darker than the cap, in each group.
echo "$CAPS" | sed 's/.*\[\(.*\)\] in.*/\1/; s/; /\n/g' | while read -r cap hatch colour _; do
  [ "$hatch" = hatch ] || ui_fail "unexpected caps entry: $cap $hatch $colour"
  [ "$cap" != "$colour" ] || ui_fail "the hatch of $cap has the cap's own colour"
done
[ -n "$SHOT" ] && ui_capture "$SHOT"

ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Closed Section Analysis" "closed, the section kept"

echo "--- Other visual styles"
ui_mark
ui_step "wireframe (search)"               ui_command "Wireframe"
ui_expect_new "] in Wireframe, bodies in" "the caps drawn in the wireframe style"
ui_step "shaded (search)"                  ui_command "Shaded"
ui_expect_new "] in Shaded, bodies in" "and in the shaded style"
ui_mark
ui_step "component colours (search)"       ui_command "Component Colors"
ui_expect_new "bodies in component colours" "the bodies in component colours"
CAPS_NOW=$(grep "Section caps: 3 colour(s)" "$UI_LOG" | tail -1 | sed 's/\] in.*/]/')
[ "$CAPS_NOW" = "$(echo "$CAPS" | sed 's/\] in.*/]/')" ] || ui_fail "the caps changed with the bodies' colours: $CAPS_NOW"
echo "ok   the caps keep their colours and hatch"

# pick_where "start" "part"...: clicks where the newest "Pick <start>..."
# line since the last "Pick places:" that has every part too was logged.
pick_where() {
  local start at px py lines part
  start=$(grep -n "Pick places:" "$UI_LOG" | tail -1 | cut -d: -f1)
  lines=$(tail -n +"${start:-1}" "$UI_LOG" | grep -F "Pick $1")
  shift
  for part in "$@"; do
    lines=$(echo "$lines" | grep -F -- "$part")
  done
  at=$(echo "$lines" | tail -1 | sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  [ -n "$at" ] || ui_fail "no pick place logged for '$*'"
  read -r px py <<< "$at"
  xdotool mousemove $((X + px * UI_SCALE)) $((Y + py * UI_SCALE)) click 1
}

# A feature preview at the section (mitcad#943): Chamfer on the top front
# edge of Part's block, which the plane at x = 10 cuts. The preview's body
# is clipped at the plane too, off the UI thread, and its caps follow it:
# 600 mm² at the marker, 599.5 with the 1 mm bevel across the cut.
echo "--- A feature preview cut at the section's plane"
# A feature works on the bodies of the active component: Part first.
ui_mark
ui_step "activate Part (radio)"            ui_click_logged "Browser radio Root/Part:1"
ui_expect_new "Activated Part" "Part active"
ui_mark
ui_step "chamfer (search)"                 ui_command "Chamfer"
ui_expect_new "Pick places:" "Chamfer logs where its picks are"
ui_step "pick Part's top front edge"       pick_where "edge F2.b0/E{" "F2:side(c1[c4,c2])" "F2:end("
ui_expect_new "Chamfer Edges: 1 edge" "the cut block's edge picked"
ui_expect_new "Preview Chamfer: ok" "previewed"
ui_expect_new "Section preview ready at " "the preview's body clipped and handed to the view" 10
ui_expect_new ", area 599.5 (preview)" "the caps follow the preview"
since=$(tail -n +$((UI_MARK + 1)) "$UI_LOG")
echo "$since" | grep -qE '^Section preview clips at .* off the UI thread \(section worker\): 1 bodies, 1 new' ||
  ui_fail "the chamfered body was not clipped on the section worker's thread"
echo "$since" | grep -qF 'Section caps: 3 face(s) at' || ui_fail "no caps logged for the preview"
echo "$since" | grep -E '^Section caps: .*\(preview\)$' | grep -qvF 'Section caps: 3 face(s) at' &&
  ui_fail "the preview's caps are not the three blocks' cuts: $(echo "$since" | grep -E '\(preview\)$')"
echo "$since" | grep -q '^Section: F2.b0 shown whole' && ui_fail "the preview's clip failed"
echo "ok   the chamfered block drawn cut, its caps the preview's"
ui_mark
ui_step "cancel (Escape)"                  ui_key Escape
ui_expect_new "Command Chamfer cancelled" "the chamfer cancelled"
ui_expect_new ", area 600.0" "the caps of the blocks at the marker again"
ui_mark
ui_step "activate Root (radio)"            ui_click_logged "Browser radio Root"
ui_expect_new "Activated Root" "the root active again"

echo "--- Remove"
ui_mark
ui_step "remove section analysis (search)" ui_command "Remove Section Analysis"
ui_expect_new "Section caps: none" "the caps are gone"

# line_of "text": the number of the first line since the mark that has the
# text, 0 when none has.
line_of() {
  local n
  n=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -nF -- "$1" | head -1 | cut -d: -f1)
  echo "${n:-0}"
}

# The clips off the UI thread (mitcad#438), each view round made to wait
# 3 s before it clips (MITCAD_TEST_SECTION_DELAY_MS) so that the panel
# changes and cancels while one runs; the steps that do so send their keys
# without waiting for the app to be idle.
echo "--- Off the UI thread: a newer distance supersedes the round that runs"
ui_stop_app
MITCAD_TEST_SECTION_DELAY_MS=3000 ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_expect_new "Computing section view started" "the clips at x = 0 run"
# While they run: the window takes the distance (the field logs it).
ui_type_in "Panel Section Analysis input offset" "10"
echo "ok   the distance typed while a round runs"
ui_expect_new "Section round view superseded" "the round at x = 0 told to stop"
ui_expect_new "Computing section view cancelled after" "and ended as cancelled"
AT10='{"plane":{"normal":[1,0,0],"origin":[10,0,0]}}'
ui_expect_new "Section clips at $AT10 off the UI thread (section worker): 3 bodies, 3 new, 3 meshed in" \
  "the blocks clipped at x = 10 and meshed on the section worker's thread" 10
ui_expect_new "Section view ready at $AT10: 3 bodies from the section worker" "handed to the view"
ui_expect_new "Section caps: 3 face(s) at" "the view shows them cut"
ui_expect_new "Inspect Section Analysis: area 600, length 180" "then the panel's numbers" 10
ui_expect_new "Section measured off the UI thread (section worker)" "measured on the worker's thread"
since=$(tail -n +$((UI_MARK + 1)) "$UI_LOG")
echo "$since" | grep -q '"origin":\[0,0,0\]}} off the UI thread' && ui_fail "the superseded round's clips were made"
echo "$since" | grep -q 'Section view ready at {"plane":{"normal":\[1,0,0\],"origin":\[0,0,0\]}}' &&
  ui_fail "the superseded round's result was shown"
[ "$(echo "$since" | grep -c '^Inspect Section Analysis: area ')" = 1 ] ||
  ui_fail "the panel was measured more than once: $(echo "$since" | grep '^Inspect Section Analysis: ')"
echo "ok   no result of the superseded round"
TYPED=$(line_of "Panel field offset: 10")
READY=$(line_of "Section view ready at $AT10")
CAPS=$(line_of "Section caps: 3 face(s) at")
MEASURED=$(line_of "Inspect Section Analysis: area 600, length 180")
[ "$TYPED" -gt 0 ] && [ "$TYPED" -lt "$READY" ] && [ "$READY" -lt "$CAPS" ] && [ "$CAPS" -lt "$MEASURED" ] ||
  ui_fail "not typed, clipped, shown and measured in this order: $TYPED, $READY, $CAPS, $MEASURED"
echo "ok   the window took input while the round ran; the numbers came after the view's update"

echo "--- Cancel while a round runs"
ui_wait_idle
ui_mark
ui_type_in "Panel Section Analysis input offset" "12"
ui_expect_new "Computing section view started" "the clips at x = 12 run"
ui_key Escape
ui_expect_new "Command Section Analysis cancelled" "the panel cancelled while they run"
ui_expect_new "Computing section view cancelled after" "the round ended as cancelled"
ui_expect_new "Section caps: none" "the blocks whole again"
ui_wait_idle
ui_sync
ui_crashed && ui_fail "crashed after the cancel"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q '"origin":\[12,0,0\]}}: 3 bodies from the section worker' &&
  ui_fail "the cancelled round's clips were shown"
[ "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c '^Inspect Section Analysis: area ')" = 0 ] ||
  ui_fail "the cancelled round's section was measured"
echo "ok   nothing of the cancelled round shown, the app goes on"

# A section turned about an axis (mitcad#823): a 10 mm cube about the Z
# axis through its middle, YZ turned 45 degrees about Z cuts it along its
# diagonal, 10 x 10 sqrt(2) = 141.42 mm².
echo "--- Turned 45 degrees about an axis through the cube's middle"
ui_stop_app
cat > "$WORK/cube.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [-5, -5], "width": 10, "height": 10},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
CUBE=$WORK/cube.mitcad
"$CLI" run "$WORK/cube.json" --save "$CUBE" > "$WORK/cli-cube.log" 2>&1 || { cat "$WORK/cli-cube.log"; ui_fail "mitcad-cli (cube)"; }
ui_start_app --open "$CUBE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Bodies shown: " "the cube shown"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_expect_new "Inspect Section Analysis: area 100, length 40" "the cube cut square across at x = 0" 10
ui_step "the Axis input"                   ui_click_logged "Panel Section Analysis input axis"
ui_step "pick the Z axis"                  ui_click_logged "Datum z"
ui_expect_new "Section Analysis Axis: 1 axis [axis z]" "Z the axis"
ui_expect_new "Manipulator Section Analysis angle at" "a ring about the axis"
ui_mark
ui_step "45 degrees"                       ui_type_in "Panel Section Analysis input angle" "45"
ui_expect_new "Inspect Section Analysis: area 141.42" "the cut along the diagonal: 141.42 mm²" 10
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Kept Section Analysis as Section1" "kept in the document"
ui_expect_new "Browser Root/Analysis/Section1 at" "listed in the Analysis folder"
ui_double_click_expect "Browser Root/Analysis/Section1" "Editing analysis Section1 with Section Analysis" \
  "its panel opens"
ui_expect_new "Section Analysis Axis: 1 axis [axis z]" "with its axis"
ui_expect_new "Inspect Section Analysis: area 141.42" "and its angle" 10
ui_step "close (Escape)"                   ui_key Escape
ui_expect_new "Command Section Analysis cancelled" "closed unchanged"
ui_mark
ui_step "save (Ctrl+S)"                    ui_key ctrl+s
ui_expect_new "Saved $CUBE" "saved"
turned=$(python3 -c 'import json, sys
a = json.load(open(sys.argv[1]))["analyses"][0]
print(a["plane"], a["axis"], round(a["angle"], 6))' "$CUBE") || ui_fail "no analyses in the file"
[ "$turned" = "yz z 0.785398" ] || ui_fail "the file's turned section: $turned"
echo "ok   saved with its axis and angle"

ui_finish "UI section caps test"
