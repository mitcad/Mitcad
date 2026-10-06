#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The view (U5) through the real UI, on a red 60 x 40 x 20 mm block with a
# 16 mm hole through it:
#   - The orientation cube: clicks on a face, an edge and a corner turn the
#     view (the camera's direction is logged), the house goes home, a drag
#     on the cube orbits; face on, its arrows turn to the faces beside and
#     roll the view (P9); standard views and the camera's projection by
#     command.
#   - Each visual style, checked on a screenshot of the view: faces, dark
#     edges and the body's red where the style draws them.
#   - Section Analysis: the cut's caps drawn, flipped, removed.
#   - Named views: saved in the document and in its file, shown again from
#     the browser; the environment's background; Preferences' navigation
#     (a scheme saved by index in older settings is read and saved by its
#     id; the middle button orbits); Preferences' pages (mitcad#25: opened
#     on one by its command, again on the last one, the window fits a
#     1366 x 768 screen); the shortcut overview.
#   - The layout grid: its spacing follows the zoom (1 or 5 times a power
#     of ten mm, 7 to 35 pixels apart) and gets finer when the view zooms
#     in.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-test.sh [directory for screenshots]

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-view.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Where the orientation cube's faces, edges and corners can be clicked.
export MITCAD_LOG_ORIENTATION_CUBE=1

cat > "$WORK/block.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 20], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "symmetric", "distance": 50, "full_length": true}, "operation": "cut",
    "participants": ["F2.b0"]}},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# camera_after "action description" expected-direction-text: waits for the
# camera to come to rest after an action logged since the mark.
expect_camera() { ui_expect_new "Camera direction $1" "$2" 8; }

# shot name [other]: a screenshot of the view (below the cube's corner),
# its pixel statistics in STAT_<key> variables; with another shot's name,
# STAT_changed counts the pixels that differ from it.
shot() {
  local area vx vy vw vh dump json other=()
  ui_sync
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  dump="$WORK/$1.xwd"
  xwd -root -silent > "$dump" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$(dirname "$0")/xwd2png.py" "$dump" "$SHOTS/view-$1.png"
  fi
  [ -n "${2:-}" ] && other=("$WORK/$2.xwd")
  # From the view's left edge, where the background is.
  json=$(python3 "$STATS" "$dump" $((X + vx)) $((Y + vy + 200)) $((vw - 20)) $((vh - 220)) "${other[@]}")
  echo "     $1: $json"
  eval "$(python3 -c 'import json, sys; d = json.loads(sys.argv[1]); print(" ".join(f"STAT_{k}={v}" for k, v in d.items()))' "$json")"
}

# more name a b: a > b, or fail.
more() { [ "$2" -gt "$3" ] || ui_fail "$1: $2 is not more than $3"; echo "ok   $1 ($2 > $3)"; }

# Settings as older versions saved them: the navigation scheme by its index
# (3: middle orbits, Ctrl + middle pans).
mkdir -p "$UI_CONFIG/Mitcad"
printf '[view]\nnavigation=3\n' > "$UI_CONFIG/Mitcad/Mitcad.conf"

ui_start_display
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Camera direction -0.577 0.577 -0.577" "the camera starts isometric"
ui_expect_log "Orientation cube front at" "the cube's places logged"
ui_expect_log "View home at" "the home button"

echo "--- Orientation cube"
ui_mark
ui_step "click the cube's front"           ui_click_logged "Orientation cube front"
expect_camera "0 1 0 up 0 0 1 (orthographic)" "looking at the front"
grep -q "Selected: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a cube click selected something"
# The cube's arrows (P9), shown while a face is in sight.
ui_expect_new "Orientation cube arrow up at" "the arrows show face on"
ui_mark
ui_step "the arrow above the cube"         ui_click_logged "Orientation cube arrow up"
expect_camera "0 0 -1 up 0 1 0 (orthographic)" "turned to the top face"
ui_mark
ui_step "the arrow below the cube"         ui_click_logged "Orientation cube arrow down"
expect_camera "0 1 0 up 0 0 1 (orthographic)" "back to the front"
ui_mark
ui_step "roll clockwise"                   ui_click_logged "Orientation cube arrow cw"
expect_camera "0 1 0 up -1 0 0 (orthographic)" "rolled 90 degrees clockwise"
ui_mark
ui_step "roll counter-clockwise"           ui_click_logged "Orientation cube arrow ccw"
expect_camera "0 1 0 up 0 0 1 (orthographic)" "rolled back"
ui_mark
ui_step "the arrow on the right"           ui_click_logged "Orientation cube arrow right"
expect_camera "-1 0 0 up 0 0 1 (orthographic)" "turned to the right face"
ui_mark
ui_step "click home"                       ui_click_logged "View home"
expect_camera "-0.577 0.577 -0.577" "home: isometric again"
ui_expect_new "Orientation cube front-top at" "the edge between front and top can be clicked"
ui_mark
ui_step "click the front-top edge"         ui_click_logged "Orientation cube front-top"
expect_camera "0 0.707 -0.707" "looking down at the front-top edge"
ui_expect_new "Orientation cube front-right-top at" "a corner can be clicked"
ui_mark
ui_step "click the front-right-top corner" ui_click_logged "Orientation cube front-right-top"
expect_camera "-0.577 0.577 -0.577" "the corner gives the isometric view"
ui_expect_new "Orientation cube right at" "the right face can be clicked"
ui_mark
ui_step "click the cube's right"           ui_click_logged "Orientation cube right"
expect_camera "-1 0 0 up 0 0 1" "looking at the right"
ui_expect_new "Orientation cube right at" "the right face's place"
ui_mark
ui_step "drag the cube"                    ui_drag_from "Orientation cube right" -50 35
ui_expect_new "Camera direction" "the drag orbits" 8
last=$(grep "Camera direction" "$UI_LOG" | tail -1)
case "$last" in *"direction -1 0 0 "*) ui_fail "the drag did not turn the view: $last" ;; esac
grep -q "Selected: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a cube drag selected something"
echo "ok   orbited to: ${last#*Camera }"

echo "--- Standard views and the camera"
ui_mark
ui_step "top view (search)"                ui_command "Top View"
expect_camera "0 0 -1 up 0 1 0 (orthographic)" "looking down from the top"
ui_mark
ui_step "perspective (search)"             ui_command "Perspective"
expect_camera "0 0 -1 up 0 1 0 (perspective)" "the camera is a perspective one"
ui_mark
ui_step "orthographic (search)"            ui_command "Orthographic"
expect_camera "0 0 -1 up 0 1 0 (orthographic)" "orthographic again"
ui_mark
ui_step "isometric view (search)"          ui_command "Isometric View"
expect_camera "-0.577 0.577 -0.577" "the isometric view"

echo "--- Visual styles"
ui_step "fit (F6)"                         ui_key F6
ui_mark
ui_step "shaded (search)"                  ui_command "Shaded"
ui_expect_new "Visual style Shaded" "shaded"
shot shaded
SHADED_OBJECT=$STAT_object SHADED_GREY=$STAT_grey SHADED_WARM=$STAT_warm
more "the red body is drawn" "$SHADED_WARM" $((STAT_pixels / 50))
ui_step "visible edges (search)"           ui_command "Shaded with Visible Edges Only"
ui_expect_new "Visual style Shaded with Visible Edges Only" "shaded with visible edges"
shot visible-edges
more "edges are drawn" "$STAT_grey" $((SHADED_GREY + 200))
more "the body stays red" "$STAT_warm" $((SHADED_WARM / 2))
ui_step "hidden edges (search)"            ui_command "Shaded with Hidden Edges"
ui_expect_new "Visual style Shaded with Hidden Edges" "shaded with hidden edges"
shot hidden-edges visible-edges
more "edges are drawn" "$STAT_grey" $((SHADED_GREY + 200))
more "the body stays red" "$STAT_warm" $((SHADED_WARM / 2))
more "the hole's hidden edges show through" "$STAT_changed" 150
ui_step "wireframe (search)"               ui_command "Wireframe"
ui_expect_new "Visual style Wireframe" "wireframe"
shot wireframe
WIRE_OBJECT=$STAT_object
more "the faces are not filled" "$((SHADED_OBJECT / 3))" "$WIRE_OBJECT"
more "the edges are drawn" "$WIRE_OBJECT" 500
ui_step "wireframe with hidden (search)"   ui_command "Wireframe with Hidden Edges"
ui_expect_new "Visual style Wireframe with Hidden Edges" "wireframe with hidden edges"
shot wireframe-hidden wireframe
# The faces hide what is behind them in the background's colour.
more "the faces are not drawn red" "$((SHADED_WARM / 10))" "$STAT_warm"
more "edges behind faces are drawn differently" "$STAT_changed" 150
ui_step "back to visible edges (search)"   ui_command "Shaded with Visible Edges Only"
ui_expect_new "Visual style Shaded with Visible Edges Only" "the default style again"

echo "--- Section Analysis"
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_log "Section analysis at" "the bodies are shown cut"
ui_mark
ui_step "30 along X"                       ui_type_in "Panel Section Analysis input offset" "30"
ui_expect_new "Inspect Section Analysis: area 480, length 128" "the cut beside the hole: 2 x 12 x 20"
ui_expect_new "Section caps: 2 face(s)" "the cut's two caps are drawn"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Closed Section Analysis" "closed, the section kept"
ui_step "right view (search)"              ui_command "Right View"
shot section
more "the caps show in their own colour" "$STAT_yellow" 2000
ui_mark
ui_step "flip (search)"                    ui_command "Flip Section Analysis"
ui_expect_new '"normal":[-1,0,0]' "the other side is shown"
ui_expect_new "Section caps: 2 face(s)" "with its caps"
ui_step "left view (search)"               ui_command "Left View"
shot section-flipped
more "the flipped caps face the other way" "$STAT_yellow" 2000
ui_mark
ui_step "remove section analysis"          ui_command "Remove Section Analysis"
ui_expect_new "Section analysis off" "the bodies are whole again"
shot section-removed
more "no caps" 100 "$STAT_yellow"

echo "--- Named views"
ui_step "front view (search)"              ui_command "Front View"
ui_mark
ui_step "new named view (search)"          ui_command "New Named View"
ui_expect_new "Saved named view NamedView1" "the view saved in the document"
ui_step "select Named Views"               ui_click_logged "Browser Root/Named Views"
ui_step "expand it (Right)"                ui_key Right
ui_expect_new "Browser Root/Named Views/NamedView1 at" "the browser lists it" 8
ui_mark
ui_step "top view (search)"                ui_command "Top View"
expect_camera "0 0 -1" "looking from the top"
ui_mark
ui_step "double-click NamedView1"          ui_double_click_logged "Browser Root/Named Views/NamedView1"
expect_camera "0 1 0 up 0 0 1" "the named view again"
ui_step "save as (Ctrl+Shift+S)"           ui_key ctrl+shift+s
ui_focus_dialog '^Save As$'
ui_key ctrl+a
xdotool type --delay 20 "$WORK/views.mitcad"
ui_key Return
ui_focus_main
ui_expect_log "Saved $WORK/views.mitcad" "saved"
python3 -c 'import json, sys; d = json.load(open(sys.argv[1])); assert [v["name"] for v in d["views"]] == ["NamedView1"], d.get("views")' \
  "$WORK/views.mitcad" || ui_fail "the named view is not in the file"
echo "ok   the file keeps the named view"

echo "--- Environment, preferences, overview"
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_log "Background #" "the background changed"
shot dark
more "the background is dark" "$STAT_dark" $((STAT_pixels / 3))
ui_mark
ui_step "preferences, Display (search)"    ui_command "Preferences: Display"
ui_expect_new "Preferences opened: Display, " "Preferences opens on the page asked for"
# The window, as large as its largest page, fits a 1366 x 768 screen with
# a title bar and a taskbar (mitcad#25).
read -r width _ height <<< "$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/.*Preferences opened: Display, \([0-9]* x [0-9]*\).*/\1/p' | tail -1)"
[ "${width:-0}" -gt 0 ] && [ "$width" -le 1366 ] && [ "${height:-0}" -gt 0 ] && [ "$height" -le 690 ] ||
  ui_fail "Preferences asks for ${width:-?} x ${height:-?}, more than a 1366 x 768 screen shows"
echo "ok   Preferences ($width x $height) fits a 1366 x 768 screen"
ui_focus_dialog '^Preferences$'
ui_step "close it (Esc)"                   ui_key Escape
ui_focus_main
ui_mark
ui_step "preferences (search)"             ui_command "Preferences"
ui_expect_new "Preferences opened: Display, " "it opens on the page shown last"
ui_focus_dialog '^Preferences$'
ui_step "keep the navigation (Enter)"      ui_key Return
ui_focus_main
ui_expect_new "Preferences: navigation middle-orbit" "the older settings' scheme was read"
grep -q '^navigationScheme=middle-orbit' "$UI_CONFIG/Mitcad/Mitcad.conf" ||
  ui_fail "the scheme is not saved by its id"
grep -q '^navigation=' "$UI_CONFIG/Mitcad/Mitcad.conf" && ui_fail "the scheme's index is still saved"
echo "ok   the scheme is saved by its id"
ui_mark
ui_step "middle drag orbits"               ui_view_drag 2 40 60 55 50
ui_expect_new "Camera direction" "the middle button orbits" 8
ui_mark
ui_step "overview (search)"                ui_command "Keyboard and Mouse Overview"
ui_expect_new "Shortcut overview:" "the overview lists the shortcuts"
ui_focus_dialog '^Keyboard and Mouse Overview$'
ui_key Escape
ui_focus_main

echo "--- Layout grid"
# grid_step: GRID_STEP, the last logged spacing, checked against the grid's
# rule (1 or 5 times a power of ten mm, 7 to 35 pixels apart, a major line
# every fifth).
grid_step() {
  local line major px
  line=$(grep -o "Layout grid [0-9.e+-]* mm, major [0-9.e+-]* mm, [0-9.]* px apart" "$UI_LOG" | tail -1)
  read -r _ _ GRID_STEP _ _ major _ px _ <<< "$line"
  [[ "$GRID_STEP" =~ ^(0\.0*[15]|[15]0*)$ ]] || ui_fail "grid step '$GRID_STEP' is not 1 or 5 times a power of ten"
  awk -v s="$GRID_STEP" -v m="$major" 'BEGIN { exit !(m == 5 * s) }' || ui_fail "major lines $major, step $GRID_STEP"
  awk -v p="$px" 'BEGIN { exit !(p >= 7 && p <= 35) }' || ui_fail "grid lines $px px apart"
}
ui_expect_log "Layout grid " "the grid's spacing is logged"
grid_step
before=$GRID_STEP
echo "ok   grid step $before mm"
ui_mark
for _ in $(seq 1 20); do
  xdotool mousemove $(ui_view_at 50 50) click 4
  ui_sync
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q "Layout grid " && break
done
ui_expect_new "Layout grid " "zooming in changes the grid's spacing"
grid_step
awk -v a="$GRID_STEP" -v b="$before" 'BEGIN { exit !(a < b) }' || ui_fail "zoomed in: step $GRID_STEP, before $before"
echo "ok   zoomed in: grid step $GRID_STEP mm"

ui_stop_app
echo "--- The named view comes back with the file"
ui_start_app --open "$WORK/views.mitcad"
ui_expect_log "Camera direction" "the camera is logged"
ui_step "select Named Views"               ui_click_logged "Browser Root/Named Views"
ui_step "expand it (Right)"                ui_key Right
ui_expect_log "Browser Root/Named Views/NamedView1 at" "the browser lists the saved view"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI view test"
