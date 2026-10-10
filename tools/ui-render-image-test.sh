#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/render
# File > Render Image (mitcad#48, docs/rendering.md "Final render") through
# the real UI, on a red 60 x 40 x 20 mm block with a 16 mm hole through it:
#   - The dialog changes the render settings' output section (each change
#     a model command): a fixed aspect at 320 x 240, a transparent
#     background; the view shows the image's frame.
#   - A long render (many samples) reports its progress while the app stays
#     usable; Cancel stops the worker (its process is gone) and the app
#     keeps working.
#   - A render of 8 samples finishes in the worker's batch mode; Save writes
#     the PNG: 320 x 240 pixels with an alpha channel, not empty, and the
#     block's silhouette in it matches the shaded view's inside the frame.
#   - A named view renders too (the frame is hidden for it), and the
#     settings are saved in the file.
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-image-test.sh [directory for screenshots]
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
if ! grep -qs '^MITCAD_RENDER:BOOL=ON' "$BUILD_DIR/CMakeCache.txt"; then
  echo "SKIP: $UI_APP is built without the render worker (MITCAD_RENDER=OFF)"
  exit 0
fi

source "$(dirname "$0")/ui-test-lib.sh"

SHOTS=${1:-}
CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/mitcad-ui-render-image.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# No ground: its shadows would be in the image's alpha. A named view from
# the front.
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
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"},
  {"cmd": "set_render_settings", "ground": {"shadows": false}, "output": {"samples": 8}},
  {"cmd": "add_named_view", "name": "Front", "eye": [30, -300, 10], "target": [30, 20, 10], "up": [0, 0, 1],
    "height": 80}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

shot() {
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$(dirname "$0")/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-image-$1.png"
  fi
}

# expect_new_within seconds text description: like ui_expect_new, longer.
expect_new_within() { ui_expect_new "$2" "$3" "$1"; }

# set_field field value: types a value into a field of Render Image.
set_field() {
  ui_focus_dialog '^Render Image$'
  ui_type_in "Render image field $1" "$2"
  ui_key Return
}

# choose_up field: the entry above the current one in a drop-down of
# Render Image.
choose_up() {
  ui_click_logged "Render image field $1"
  ui_sync
  ui_key Up
  ui_key Return
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_step "hide the layout grid" ui_command "Layout Grid"

echo "--- Render Image"
ui_step "File > Render Image..." ui_command "Render Image"
ui_expect_log "Render image dialog opened" "the dialog opened"
ui_focus_dialog '^Render Image$'
ui_expect_log "Render image field output.width at" "the dialog logs its fields"

echo "--- A long render, cancelled"
# Large and with many samples: Cycles' adaptive sampling ends a small image
# of a simple scene early.
ui_mark
ui_step "Size: 3840 x 2160" ui_choose "Render image size" 6
ui_expect_new "Render setting output.size = 3840 x 2160" "a size preset"
ui_step "samples 65536" set_field output.samples 65536
ui_expect_new "Render setting output.samples = 65536" "many samples"
ui_mark
ui_step "Render" ui_click_logged "Render image render"
expect_new_within 20 "Render image started: 3840 x 2160, 65536 samples, png, current view, 1 bodies" \
  "the render started in the worker"
pid=$(grep -o "Render image started: .* (pid [0-9]*)" "$UI_LOG" | tail -1 | sed 's/.*(pid \([0-9]*\))/\1/')
[ -n "$pid" ] && kill -0 "$pid" 2> /dev/null || ui_fail "no worker process"
expect_new_within 30 "Render image progress " "the worker reports its progress"
grep -q -- "--batch" "/proc/$pid/cmdline" || ui_fail "the worker does not run in its batch mode"
echo "ok   the worker $pid renders in its batch mode"
ui_focus_main
ui_step "the app stays usable while it renders (Orthographic)" ui_command "Orthographic"
kill -0 "$pid" 2> /dev/null || ui_fail "the render ended by itself"
ui_focus_dialog '^Render Image$'
ui_mark
ui_step "Cancel" ui_click_logged "Render image cancel"
expect_new_within 10 "Render image cancelled" "the render is cancelled"
for _ in $(seq 1 50); do kill -0 "$pid" 2> /dev/null || break; sleep 0.1; done
kill -0 "$pid" 2> /dev/null && ui_fail "the worker $pid still runs after Cancel"
echo "ok   Cancel stopped the worker"
ui_focus_main
ui_step "the app keeps working (Orthographic)" ui_command "Orthographic"
ui_crashed && ui_fail "the app ended with the render"

echo "--- The output settings: 320 x 240, transparent"
ui_focus_dialog '^Render Image$'
ui_mark
ui_step "width 320" set_field output.width 320
ui_expect_new "Render setting output.width = 320" "the width changed in the model"
ui_step "height 240" set_field output.height 240
ui_expect_new "Render setting output.height = 240" "the height changed in the model"
ui_step "samples 8" set_field output.samples 8
ui_expect_new "Render setting output.samples = 8" "few samples"
ui_step "Transparent background" ui_click_logged "Render image field output.transparent"
ui_expect_new "Render setting output.transparent = true" "a transparent background"
ui_mark
ui_step "Aspect: From the View" choose_up output.aspect
ui_expect_new "Render setting output.aspect = view" "the view's aspect"
ui_expect_new "Render image frame hidden" "no frame with the view's aspect"
ui_mark
ui_step "Aspect: Fixed" ui_choose "Render image field output.aspect" 1
ui_expect_new "Render setting output.aspect = fixed" "a fixed aspect again"
ui_expect_new "Render image frame " "the view shows the image's frame"
read -r fx fy fw fh <<< "$(grep -o "Render image frame [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1 |
  cut -d' ' -f4-)"
read -r _ _ _ _ vw vh <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
python3 -c "import sys; fw, fh, vw, vh = map(int, sys.argv[1:]); \
  sys.exit(0 if abs(fw / fh - 4 / 3) < 0.02 and (fw == vw or fh == vh) and fw <= vw and fh <= vh else 1)" \
  "$fw" "$fh" "$vw" "$vh" || ui_fail "the frame $fw x $fh is not the largest 4:3 one in the view $vw x $vh"
echo "ok   the frame is ${fw} x ${fh} at $fx,$fy in the view of ${vw} x ${vh}"
shot frame

echo "--- A render of 8 samples, saved"
ui_focus_dialog '^Render Image$'
ui_mark
ui_step "Render" ui_click_logged "Render image render"
expect_new_within 60 "Render image done: 320 x 240, 8 samples in" "the image is rendered"
shot done
ui_step "Save..." ui_click_logged "Render image save"
ui_mark
ui_type_path "Save Rendered Image" "$WORK/render.png"
expect_new_within 10 "Render image saved $WORK/render.png" "Save wrote the image"
[ -s "$WORK/render.png" ] || ui_fail "the image file is empty"
echo "ok   render.png has $(stat -c %s "$WORK/render.png") bytes"
[ -n "$SHOTS" ] && cp "$WORK/render.png" "$SHOTS/render-image-render.png"
# The shaded view without the dialog over it and without the frame.
ui_focus_dialog '^Render Image$'
ui_step "Close" ui_click_logged "Render image close"
ui_expect_log "Render image frame hidden" "closing the dialog hides the frame"
ui_focus_main
shot shaded
# The block in the image where the shaded view shows it: inside the frame
# (off its edge) and below the orientation cube's corner.
read -r _ _ vx vy _ _ <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
top=$((fy + 3 > vy + 200 ? fy + 3 : vy + 200))
json=$(python3 "$STATS" --image-silhouette "$WORK/shaded.xwd" $((X + fx + 3)) $((Y + top)) $((fw - 6)) \
  $((fy + fh - 3 - top)) "$WORK/render.png" $((X + fx)) $((Y + fy)) "$fw" "$fh")
echo "     image against the shaded view: $json"
python3 -c '
import json, sys
d = json.loads(sys.argv[1])
assert (d["width"], d["height"]) == (320, 240), "the image is %d x %d" % (d["width"], d["height"])
assert d["b"] > 2000, "the image shows nothing"
assert d["iou"] > 0.8, "the silhouettes overlap only %d %%" % (d["iou"] * 100)
assert max(abs(a - b) for a, b in zip(d["box_a"], d["box_b"])) <= 12, "the boxes differ"
' "$json" || ui_fail "the saved image does not match the view: $json"
echo "ok   the image's silhouette matches the view"

echo "--- A named view"
ui_step "File > Render Image..." ui_command "Render Image"
ui_focus_dialog '^Render Image$'
ui_mark
ui_step "Camera: Front" ui_choose "Render image camera" 1
ui_expect_new "Render image camera Front" "the named view is chosen"
ui_expect_new "Render image frame hidden" "a named view hides the view's frame"
ui_mark
ui_step "Render" ui_click_logged "Render image render"
expect_new_within 60 "Render image started: 320 x 240, 8 samples, png, Front, 1 bodies" "the named view renders"
expect_new_within 60 "Render image done: 320 x 240, 8 samples in" "the named view is rendered"
ui_step "Close" ui_click_logged "Render image close"

echo "--- Saved in the file"
ui_focus_main
ui_step "save (ctrl+s)" ui_key ctrl+s
ui_expect_log "Saved $WORK/block.mitcad" "the design is saved"
python3 -c '
import json, sys
output = json.load(open(sys.argv[1]))["render"]["output"]
assert (output["width"], output["height"], output["aspect"]) == (320, 240, "fixed"), output
assert output["samples"] == 8 and output["transparent"] is True, output
' "$WORK/block.mitcad" || ui_fail "the output settings are not in the saved file"
echo "ok   the output settings are in the file"

ui_finish "Render Image"
