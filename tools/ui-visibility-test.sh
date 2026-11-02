#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework app/MainWindow.cpp app/MainWindowVisibility.cpp tools/cli app/OcctViewer.cpp
# The browser's light bulbs in every mode (mitcad#295) and Create Sketch's
# origin planes (mitcad#844) through the real UI. A design without
# components: Body1 (20 x 20 x 10) inside Body2 (60 x 60 x 30), and Sketch3
# with a free 20 x 20 square beside them:
#   - Create Sketch shows the origin planes; no face of Body1 can be picked.
#     Body2 hidden by its light bulb while the plane is asked for: the pick
#     goes on, Body1's top face can be picked and the sketch starts on it.
#   - Transparent (mitcad#1093) on Body2 from its Browser row: the row shows
#     the half-open eye with its name in no colour (mitcad#1143), and a
#     click where Body1 lies inside Body2 selects a face of Body1; the menu
#     has Transparent checked; one undo step makes Body2 opaque, and the
#     click takes Body2 again.
#   - Extrude of the square, Body1 hidden meanwhile: the preview is drawn
#     again without Body1, not computed again; Cancel keeps Body1 hidden
#     (the change is the last undo step).
#   - Extrude again, Body1 hidden meanwhile, OK: one undo step takes the
#     extrusion back and leaves Body1 hidden; the next one shows it.
#   - Edit of Extrude2 (the timeline rolled back), Body1 hidden meanwhile,
#     Cancel: Body1 stays hidden, as the last undo step (no marker move
#     left behind).
#   - A Flat Pattern's or an analysis's light bulb and the other browser
#     actions still wait for the command: Rename says so.
#   - The Origin folder's light bulb off: Offset Plane's plane input and
#     Extrude's Start Object leave the origin hidden (mitcad#1030), and XY
#     picked from the browser's Origin folder goes to the input. With the
#     light bulb on, the origin stays shown during and after Offset Plane,
#     and XY is picked in the view.
# The same design with a component (Part, its body across the origin) and
# the root active:
#   - Create Sketch shows the origin planes too, the Origin folder's light
#     bulb off and unchanged; they lie behind the bodies for the pick: a
#     click on Part's top face, which XZ and YZ run through and XY lies
#     below, picks the face; the design's origin is hidden again in the
#     sketch.
#   - A click on XZ beside the bodies picks the plane; after the pick the
#     origin is hidden again, as its light bulb is off.
#   - The Origin folder's light bulb switched on during the pick: XY can be
#     picked, and the origin stays shown in the sketch, as the bulb says.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-visibility-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-visibility.XXXXXX)
FILE=$WORK/blocks.mitcad
PARTS=$WORK/parts.mitcad
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
ORIGIN_SHOWN="Design origin shown in the view; the Origin folder's light bulb off"
ORIGIN_HIDDEN="Design origin hidden in the view; the Origin folder's light bulb off"

R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Sketch1 (F1) 20 x 20 at (40, -60), Extrude1 (F2) 10 up: Body1 (F2.b0).
# Sketch2 (F3) 60 x 60 around it, Extrude2 (F4) 30 up: Body2 (F4.b0),
# Body1 inside it. Sketch3 (F5) 20 x 20 at (100, -140): a profile in front of
# the blocks as the view looks at them.
BLOCKS=$(cat << EOF
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [40, -60], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [20, -80], "width": 60, "height": 60},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 30}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [100, -140], "width": 20, "height": 20}
EOF
)
printf '[\n%s\n]\n' "$BLOCKS" > "$WORK/blocks.json"
# The same with Part:1 (O1, C1) at the origin: Sketch4 (F6) 20 x 20 around
# its origin, Extrude3 (F7) 10 up: across XZ and YZ, on XY; the root active
# again.
cat > "$WORK/parts.json" << EOF
[
$BLOCKS,
  {"cmd": "create_component", "name": "Part", "transform": {"translation": [0, 0, 0]}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F6", "corner": [-10, -10], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F6", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$WORK/blocks.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
"$CLI" run "$WORK/parts.json" --save "$PARTS" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark "text": whether a line logged after the mark has the text.
since_mark() { tail -n +$((${UI_MARK:-0} + 1)) "$UI_LOG" | grep -qF -- "$1"; }

# expect_none "text" description: no line since the mark has the text.
expect_none() {
  ui_sync
  since_mark "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# last_line "prefix": the last line logged that starts with the prefix.
last_line() { grep -F -- "$1" "$UI_LOG" | tail -1; }

# extrude_square: Extrude, which takes the newest free profile (the
# square), its preview computed with both blocks drawn.
extrude_square() {
  ui_mark
  ui_step "extrude (E)"                  ui_key e
  ui_expect_new "Command Extrude started" "Extrude started"
  ui_expect_new "Preview Extrude: ok" "the square's preview computed"
  ui_expect_new "Preview bodies: " "the preview drawn"
  last_line "Preview bodies: " | grep -qF "F2.b0 at" || ui_fail "the preview does not draw Body1"
  echo "ok   the preview draws Body1"
}

# hide_body1 command: Body1's light bulb during the command; the preview
# drawn again without it, not computed again.
hide_body1() {
  ui_mark
  ui_step "hide Body1"                   ui_click_logged "Browser eye Root/Bodies/Body1"
  ui_expect_new "Visibility Root/Bodies/Body1: hidden (during $1)" "Body1 hidden during $1"
  ui_expect_new "Preview bodies: " "the preview drawn again"
  last_line "Preview bodies: " | grep -qF "F2.b0 at" && ui_fail "the preview still draws Body1"
  last_line "Preview bodies: " | grep -qF "F4.b0 at" || ui_fail "the preview lost Body2"
  echo "ok   the preview without Body1, with Body2"
  expect_none "Preview $1: " "the preview not computed again"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0, F4.b0" "both blocks shown"

echo "--- Create Sketch: the origin planes; Body2 hidden while a plane is asked for"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch: select a plane" "Create Sketch asks for a plane"
ui_expect_new "Create Sketch: origin planes shown" "no components: the origin planes shown"
ui_expect_new "Datum xy at" "the XY plane can be picked"
ui_expect_new "Pick places:" "the faces' pick places logged"
expect_none "Pick face F2.b0/" "Body1, inside Body2, cannot be picked"
ui_mark
ui_step "hide Body2"                     ui_click_logged "Browser eye Root/Bodies/Body2"
ui_expect_new "Visibility Root/Bodies/Body2: hidden (during Create Sketch)" "Body2 hidden during the plane pick"
ui_expect_new "Bodies shown: F2.b0" "only Body1 shown"
ui_expect_new "Pick face F2.b0/F2:end(" "Body1's top face can be picked now"
expect_none "Create Sketch is waiting for a plane" "the light bulb was not refused"
ui_step "pick Body1's top face"          ui_click_pick "face F2.b0/F2:end("
ui_expect_new 'Sketch started on {"body":"F2.b0","face":"F2:end(' "the sketch started on Body1's top face"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the empty sketch finished"
ui_mark
ui_step "show Body2"                     ui_click_logged "Browser eye Root/Bodies/Body2"
ui_expect_new "Visibility Root/Bodies/Body2: shown" "Body2 shown again"
expect_none "(during" "no command runs"

echo "--- Transparent (mitcad#1093): Body2 from its Browser row; a click goes through it to Body1"
ui_mark
ui_step "right-click Body2's row"        ui_click_logged "Browser Root/Bodies/Body2" 3
ui_expect_new "Context menu: " "the row's menu"
ui_step "Transparent"                    ui_menu_choose "Transparent"
ui_expect_new "Transparent: Body2" "Body2 made transparent"
ui_expect_new "Browser eye Root/Bodies/Body2: transparent" "its row shows the half-open eye"
ui_expect_new "View transparent: 1 body" "the view draws Body2 see-through"
# Its name keeps no colour (mitcad#1143: the transparency once read as a
# colour role and painted the name red, logged as "unknown 1: ...").
ui_sync
colours=$(last_line "Browser row colours: ")
[ -n "$colours" ] || ui_fail "no 'Browser row colours' line logged"
case "$colours" in
  *Root/Bodies/Body2*|*unknown*) ui_fail "the transparent row's name has a colour: $colours" ;;
esac
echo "ok   Body2's name keeps the plain colour (no colour role on its row)"
ui_step "Escape (clear the selection)"   ui_key Escape
ui_mark
ui_step "click where Body1 lies inside Body2" ui_click_logged "Body F2.b0"
ui_expect_new "Selected: 1 face [face F2:" "the click goes through Body2 to Body1 behind it"
ui_step "Escape (clear the selection)"   ui_key Escape
ui_mark
ui_step "right-click Body2's row"        ui_click_logged "Browser Root/Bodies/Body2" 3
ui_expect_new "Context menu checked: Transparent" "Transparent is checked on its row"
ui_step "Escape (close the menu)"        ui_key Escape
ui_step "Escape (clear the selection)"   ui_key Escape
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Make Body2 Transparent" "one undo step makes Body2 opaque again"
ui_expect_new "Browser eye Root/Bodies/Body2: opaque" "its eye is open again"
ui_expect_new "View transparent: none" "the view draws Body2 as before"
ui_mark
ui_step "click there again"              ui_click_logged "Body F2.b0"
ui_expect_new "Selected: 1 face [face F4:" "opaque, Body2 takes the click"
ui_step "Escape (clear the selection)"   ui_key Escape

echo "--- Extrude, Body1 hidden meanwhile, Cancel"
extrude_square
hide_body1 Extrude
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"
expect_none "Bodies shown: F2.b0" "Body1 stays hidden"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Hide Body1" "the light bulb is the last undo step, kept by Cancel"
ui_expect_new "Bodies shown: F2.b0, F4.b0" "Body1 shown by the undo"

echo "--- Extrude, Body1 hidden meanwhile, OK: one undo step for the extrusion"
extrude_square
hide_body1 Extrude
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Added Extrude as F" "Extrude3 added"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Add Extrude3" "one undo step takes the extrusion back"
expect_none "Bodies shown: F2.b0" "Body1 stays hidden after that undo"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Hide Body1" "the light bulb is an undo step of its own"
ui_expect_new "Bodies shown: F2.b0, F4.b0" "Body1 shown by it"

echo "--- Edit of Extrude2 (rolled back), Body1 hidden meanwhile, Cancel"
ui_expect_log "Timeline Extrude2 at" "Extrude2 in the timeline"
ui_mark
ui_step "edit Extrude2 (double-click)"   ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F4 with Extrude" "editing Extrude2"
ui_expect_new "Bodies shown: F2.b0" "the timeline rolled back to before Body2"
ui_mark
ui_step "hide Body1"                     ui_click_logged "Browser eye Root/Bodies/Body1"
ui_expect_new "Visibility Root/Bodies/Body1: hidden (during Extrude)" "Body1 hidden during the edit"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude: 1 visibility change(s) kept" "the change run again after the roll-back"
ui_expect_new "Command Extrude cancelled" "the edit cancelled"
ui_expect_new "Bodies shown: F4.b0" "Body2 back, Body1 hidden"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Hide Body1" "the light bulb is the last undo step, no marker move left"
ui_expect_new "Bodies shown: F2.b0, F4.b0" "Body1 shown by the undo"

echo "--- Offset Plane and Extrude: a hidden origin stays hidden, picked from the browser (mitcad#1030)"
ui_mark
ui_step "offset plane (search)"          ui_command "Offset Plane"
ui_expect_new "Command Offset Plane started" "Offset Plane started"
expect_none "$ORIGIN_SHOWN" "the hidden origin stays hidden for the plane input"
expect_none "Datum xy at" "no origin plane in the view"
ui_pick_origin_row XY
ui_expect_new "Offset Plane Plane: 1 plane [plane xy]" "XY picked from the browser while the origin is hidden"
expect_none "$ORIGIN_SHOWN" "the origin still hidden after the pick"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Offset Plane cancelled" "Offset Plane cancelled"
extrude_square
ui_step "Start: Object"                  ui_choose "Panel Extrude input start" 2
ui_expect_new "Extrude: Start = Object" "the start is an object"
expect_none "$ORIGIN_SHOWN" "the hidden origin stays hidden for Start Object"
expect_none "Datum xy at" "no origin plane in the view"
ui_pick_origin_row XY
ui_expect_new "Extrude Start Object: 1 plane [plane xy]" "XY picked from the browser as the start"
expect_none "$ORIGIN_SHOWN" "the origin still hidden after the pick"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"

echo "--- Offset Plane: a shown origin stays shown, picked in the view (mitcad#1030)"
ui_mark
ui_step "show the origin"                ui_click_logged "Browser eye Root/Origin"
ui_expect_new "Visibility Root/Origin: shown" "the Origin folder's light bulb on"
ui_expect_new "Design origin shown in the view; the Origin folder's light bulb on" "the origin shown"
ui_mark
ui_step "offset plane (search)"          ui_command "Offset Plane"
ui_expect_new "Command Offset Plane started" "Offset Plane started"
ui_step "pick XY in the view"            ui_click_logged "Datum xy"
ui_expect_new "Offset Plane Plane: 1 plane [plane xy]" "XY picked in the view"
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Offset Plane cancelled" "Offset Plane cancelled"
expect_none "Design origin hidden in the view" "the shown origin stayed shown during and after the command"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "--- A design with a component: the origin planes shown, behind the bodies (mitcad#844)"
ui_start_app --open "$PARTS"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the blocks shown"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch: select a plane" "Create Sketch asks for a plane"
ui_expect_new "Create Sketch: origin planes shown" "a design with a component: the origin planes shown"
ui_expect_new "$ORIGIN_SHOWN" "the design's origin shown for the pick, its light bulb off"
ui_expect_new "Datum xy at" "the XY plane can be picked"
ui_expect_new "Pick face F4.b0/F4:end(" "Body2's top face can be picked"
# Part's body lies across the origin: XZ and YZ run through it, XY lies
# below it, so a plane is in front of or behind each point of its top face.
ui_expect_new "Pick face F7.b0/F7:end(" "Part's top face, which the origin planes run through, can be picked"
ui_mark
ui_step "activate Part (radio)"          ui_click_logged "Browser radio Root/Part:1"
ui_expect_new "Create Sketch is waiting for a plane: pick one or press Esc." "other browser actions wait for the plane pick"
expect_none "Activated Part" "Part not activated"
ui_step "pick Part's top face"           ui_click_pick "face F7.b0/F7:end("
ui_expect_new 'Sketch started on {"body":"F7.b0","face":"F7:end(' "the face is picked, not the plane through it"
ui_expect_new "$ORIGIN_HIDDEN" "the design's origin hidden again in the sketch"
expect_none "Visibility Root/Origin" "the Origin's light bulb not changed"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the empty sketch finished"

echo "--- A click on an origin plane beside the bodies picks the plane; the light bulb as before"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch: origin planes shown" "the origin planes shown"
ui_expect_new "$ORIGIN_SHOWN" "the design's origin shown for the pick, its light bulb off"
ui_expect_new "Datum xz at" "the XZ plane can be picked"
ui_step "pick XZ in the view"            ui_click_logged "Datum xz"
ui_expect_new "Sketch started on xz" "the sketch started on the origin's XZ plane"
ui_expect_new "$ORIGIN_HIDDEN" "the design's origin hidden again: its light bulb is off"
expect_none "Visibility Root/Origin" "the Origin's light bulb not changed"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the empty sketch finished"
expect_none "$ORIGIN_SHOWN" "the design's origin stays hidden after the sketch"

echo "--- The Origin folder's light bulb during the pick: the origin stays shown after it"
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Create Sketch: origin planes shown" "the origin planes shown"
ui_mark
ui_step "show the origin"                ui_click_logged "Browser eye Root/Origin"
ui_expect_new "Visibility Root/Origin: shown (during Create Sketch)" "the Origin folder shown during the pick"
ui_expect_new "Design origin shown in the view; the Origin folder's light bulb on" "its light bulb on"
ui_step "pick XY"                        ui_click_logged "Datum xy"
ui_expect_new "Sketch started on xy" "the sketch started on the origin's XY plane"
ui_sync
[ "$(grep -o "Design origin .*" "$UI_LOG" | tail -1)" = "Design origin shown in the view; the Origin folder's light bulb on" ] ||
  ui_fail "the origin is not shown in the sketch with its light bulb on: $(grep -o "Design origin .*" "$UI_LOG" | tail -1)"
echo "ok   the origin stays shown in the sketch, as its light bulb says"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the empty sketch finished"

echo "--- During Extrude: a component's light bulb works, other browser actions wait"
ui_mark
ui_step "extrude (E)"                    ui_key e
ui_expect_new "Command Extrude started" "Extrude started"
ui_expect_new "Preview Extrude: ok" "the square's preview computed"
ui_mark
ui_step "hide Part:1"                    ui_click_logged "Browser eye Root/Part:1"
ui_expect_new "Visibility Root/Part:1: hidden (during Extrude)" "Part:1 hidden during Extrude"
ui_mark
ui_step "activate Part (radio)"          ui_click_logged "Browser radio Root/Part:1"
ui_expect_new "Finish the command or the sketch first." "other browser actions wait for the command"
expect_none "Activated Part" "Part not activated"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "Extrude cancelled"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI visibility test"
