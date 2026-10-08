#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# View > Rendered (mitcad#32, docs/rendering.md) through the real UI, on a
# red 60 x 40 x 20 mm block with a 16 mm hole through it:
#   - The mode starts the render worker; its frames refine (more samples)
#     and fill the view, the orientation cube stays on top, and the
#     rendered block's silhouette matches the shaded one (screenshots).
#   - Orbit, pan and zoom send the worker the new camera (a new view with
#     its first frame's latency logged); the framing still matches the
#     shaded view after them.
#   - A larger window renders larger frames, in a larger frame memory.
#   - Killing the worker leaves the app working, with a message in the
#     view; the mode starts it again, and leaving the mode ends it.
#   - The worker is the executable mitcad-render next to the app (mitcad#45),
#     and the frames' shared memory has no name: the app and the worker map
#     an anonymous segment (memfd), nothing in /dev/shm. Killing the app
#     ends the worker and leaves nothing behind; the next start renders.
#   - Scene updates (mitcad#49), on a document of two bodies and a sketch:
#     changing one body's feature sends only that body's mesh (the
#     worker's report), undoing it sends none (the worker still holds it).
#     A sketch circle under the block is hidden in the rendered view as in
#     the shaded one (the bodies' depth is drawn under the render), the one
#     in front of it is drawn. Frames are denoised as previews before the
#     last one.
#   - A worker of another protocol version is stopped with a message, and
#     without a worker there is no rendered view (MITCAD_RENDER_WORKER).
#
# Skipped (exit 0) when the build has no render worker (MITCAD_RENDER off).
# The app starts five times.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-render-test.sh [directory for screenshots]

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
WORK=$(mktemp -d /tmp/mitcad-ui-render.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# Few samples: the test checks that frames refine, not how they look.
export MITCAD_RENDER_SAMPLES=16
# Where the orientation cube is (it is drawn over the render).
export MITCAD_LOG_ORIENTATION_CUBE=1

# No ground: its shadows would count as the block's silhouette.
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
  {"cmd": "set_render_settings", "ground": {"shadows": false}}
]
EOF
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# view_region: the view below the orientation cube's corner, in screen
# coordinates ("x y width height").
view_region() {
  local area vx vy vw vh
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  echo $((X + vx + 10)) $((Y + vy + 200)) $((vw - 20)) $((vh - 220))
}

# shot name: a screenshot of the screen (and a PNG of it with a directory).
shot() {
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
  if [ -n "$SHOTS" ]; then
    python3 "$(dirname "$0")/xwd2png.py" "$WORK/$1.xwd" "$SHOTS/render-$1.png"
  fi
}

# same_silhouette shaded rendered description: the two shots show the
# block in the same place: their silhouettes mostly overlap and their
# bounding boxes are within 12 pixels (the shaded view's edges, the
# render's soft shadow and shading make them differ a little).
same_silhouette() {
  local json verdict
  json=$(python3 "$STATS" --silhouettes "$WORK/$1.xwd" "$WORK/$2.xwd" $(view_region))
  echo "     $3: $json"
  verdict=$(python3 -c '
import json, sys
d = json.loads(sys.argv[1])
if d["b"] < 2000:
    print("the rendered view shows nothing")
elif d["iou"] < 0.8:
    print("the silhouettes overlap only %d %%" % (d["iou"] * 100))
elif max(abs(a - b) for a, b in zip(d["box_a"], d["box_b"])) > 12:
    print("the bounding boxes differ: %s and %s" % (d["box_a"], d["box_b"]))
else:
    print("ok %d %% overlap" % (d["iou"] * 100))' "$json")
  [ "${verdict#ok }" != "$verdict" ] || ui_fail "$3: $verdict"
  echo "ok   $3 (${verdict#ok })"
}

# last_view: the sequence number of the last view sent to the worker.
last_view() { grep -o "Render view [0-9]*: " "$UI_LOG" | tail -1 | tr -dc 0-9; }

# expect_done view description: the worker rendered all samples of the
# view (MITCAD_RENDER_SAMPLES) and the view shows its frames.
expect_done() {
  for _ in $(seq 1 300); do
    grep -qE "Render view $1: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "$2: the app crashed"
    sleep 0.2
  done
  grep -qE "Render view $1: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" ||
    ui_fail "$2: view $1 did not get its $MITCAD_RENDER_SAMPLES samples"
  grep -qE "Render frame view $1 " "$UI_LOG" || ui_fail "$2: no frame of view $1 shown"
  echo "ok   $2: $(grep -E "Render view $1: [0-9]+ samples in" "$UI_LOG" | tail -1 | sed 's/.*: //')"
}

# expect_new_view description: after an action, a new view was sent and
# its first frame came; prints the latency.
expect_new_view() {
  local before=$1 view
  for _ in $(seq 1 100); do
    view=$(last_view)
    [ -n "$view" ] && [ "$view" -gt "$before" ] &&
      grep -q "Render view $view: first frame" "$UI_LOG" && break
    sleep 0.2
  done
  [ -n "$view" ] && [ "$view" -gt "$before" ] || ui_fail "$2: no new view sent to the worker"
  grep "Render view $view: first frame" "$UI_LOG" > /dev/null || ui_fail "$2: no frame of view $view"
  echo "ok   $2: $(grep "Render view $view: first frame" "$UI_LOG" | tail -1 | sed 's/.*first frame/first frame/')"
  NEW_VIEW=$view
}

ui_start_display
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the block opened"
ui_expect_log "Camera direction -0.577 0.577 -0.577" "the camera starts isometric"
# Without the layout grid, whose lines the silhouettes would count.
ui_step "hide the layout grid" ui_command "Layout Grid"
shot shaded

echo "--- Entering the rendered view"
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_log "Rendered view on" "the mode is on"
ui_expect_log "Render worker started (pid" "the worker started"
ui_expect_log "Render worker ready: Cycles" "the worker runs Cycles"
ui_expect_log ": 1 bodies, 1 meshes sent (F2.b0)" "the block went to the worker"
ui_expect_log "received F2.b0; added F2.b0;" "the worker added it"
VIEW=$(last_view)
expect_done "$VIEW" "the frames refined to all samples"
[ "$(grep -cE "Render frame view $VIEW [0-9]+x[0-9]+ samples " "$UI_LOG")" -ge 3 ] ||
  ui_fail "the view did not refine in steps"
echo "ok   the view refined in $(grep -cE "Render frame view $VIEW " "$UI_LOG") frames"
shot rendered
same_silhouette shaded rendered "the rendered block is where the shaded one is"
grep -q "Orientation cube front at" "$UI_LOG" || ui_fail "the cube's places were not logged"
python3 "$STATS" "$WORK/rendered.xwd" $(ui_logged_at "Orientation cube front") 4 4 |
  grep -q '"object": [1-9]' || ui_fail "the orientation cube is not drawn over the render"
echo "ok   the orientation cube is drawn over the render"

echo "--- Orbit, pan and zoom"
before=$(last_view)
orbit() { xdotool keydown shift; ui_view_drag 2 40 50 55 45; xdotool keyup shift; }
ui_step "orbit (Shift + middle drag)" orbit
expect_new_view "$before" "the orbit restarted the render"
before=$NEW_VIEW
ui_step "pan (middle drag)" ui_view_drag 2 50 50 45 55
expect_new_view "$before" "the pan restarted the render"
before=$NEW_VIEW
# Out: zoomed in, the block would reach past the region's edges, where the
# silhouettes take their rows' background from.
ui_step "zoom (wheel)" xdotool mousemove $(ui_view_at 50 50) click 5
expect_new_view "$before" "the zoom restarted the render"
expect_done "$NEW_VIEW" "the zoomed view refined"
shot rendered-moved
ui_step "leave the rendered view" ui_command "Rendered"
ui_expect_log "Rendered view off" "the mode is off"
ui_expect_log "Render worker ended" "leaving the mode ended the worker"
shot shaded-moved
same_silhouette shaded-moved rendered-moved "after orbit, pan and zoom the render still matches"

echo "--- Resizing"
ui_step "View > Rendered again" ui_command "Rendered"
ui_expect_log "Render worker ready" "the worker started again"
before=$(last_view)
memories=$(grep -c "Render frame memory [0-9]*: " "$UI_LOG")
ui_step "a larger window" xdotool windowsize "$UI_WINDOW" 1600 1000
expect_new_view "$before" "the resize restarted the render"
[ "$(grep -c "Render frame memory [0-9]*: " "$UI_LOG")" -gt "$memories" ] ||
  ui_fail "the larger view got no larger frame memory"
echo "ok   a larger frame memory: $(grep -o "Render frame memory [0-9]*: .*" "$UI_LOG" | tail -1)"
read -r _ _ _ _ vw vh <<< "$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)"
expect_done "$NEW_VIEW" "the larger view refined"
grep -qE "Render frame view $NEW_VIEW ${vw}x${vh} samples " "$UI_LOG" ||
  ui_fail "the frames are not ${vw} x ${vh}: $(grep "Render frame view $NEW_VIEW " "$UI_LOG" | tail -1)"
echo "ok   the frames are ${vw} x ${vh}"

echo "--- The worker dies"
pid=$(grep -o "Render worker started (pid [0-9]*)" "$UI_LOG" | tail -1 | tr -dc 0-9)
kill -KILL "$pid" || ui_fail "no worker process $pid"
ui_expect_log "Render worker stopped:" "the app noticed"
ui_expect_log "View message: The renderer stopped" "the view says so"
ui_step "the app still works (Home)" ui_command "Home"
ui_crashed && ui_fail "the app ended with the worker"
echo "ok   the app keeps working"
ui_mark
ui_step "View > Rendered starts it again" ui_command "Rendered"
ui_expect_new "Render worker ready" "a new worker" 20
pid=$(grep -o "Render worker started (pid [0-9]*)" "$UI_LOG" | tail -1 | tr -dc 0-9)
ui_step "leave the rendered view" ui_command "Rendered"
for _ in $(seq 1 50); do kill -0 "$pid" 2> /dev/null || break; sleep 0.1; done
kill -0 "$pid" 2> /dev/null && ui_fail "the worker $pid still runs after leaving the mode"
echo "ok   leaving the mode ended the worker"

echo "--- The frame memory"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker ready" "the worker started" 20
grep -qE "Render worker started \(pid [0-9]+\): $(dirname "$UI_APP")/mitcad-render$" "$UI_LOG" ||
  ui_fail "the worker is not mitcad-render next to the app"
echo "ok   the worker is mitcad-render next to the app"
pid=$(grep -o "Render worker started (pid [0-9]*)" "$UI_LOG" | tail -1 | tr -dc 0-9)
app=$(xdotool getwindowpid "$UI_WINDOW")
for _ in $(seq 1 100); do
  grep -q 'memfd:mitcad-render-frames' "/proc/$pid/maps" 2> /dev/null && break
  sleep 0.1
done
for process in "$app" "$pid"; do
  grep -q 'memfd:mitcad-render-frames' "/proc/$process/maps" ||
    ui_fail "process $process does not map the anonymous frame memory"
  grep -q ' /dev/shm/' "/proc/$process/maps" &&
    ui_fail "process $process maps a named segment: $(grep ' /dev/shm/' "/proc/$process/maps")"
done
echo "ok   the app and the worker share anonymous memory, nothing in /dev/shm"
kill -KILL "$app" || ui_fail "no app process $app"
for _ in $(seq 1 100); do kill -0 "$pid" 2> /dev/null || break; sleep 0.1; done
kill -0 "$pid" 2> /dev/null && ui_fail "the worker $pid still runs after the app was killed"
echo "ok   killing the app ended the worker"
ui_stop_app
for process in "$app" "$pid"; do
  [ -e "/proc/$process" ] && grep -q 'memfd:mitcad-render-frames' "/proc/$process/maps" 2> /dev/null &&
    ui_fail "process $process still maps the frame memory"
done
echo "ok   nothing is left of the frame memory"
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/block.mitcad" "the next start"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new "Render worker ready" "the worker started" 20
ui_expect_new ": 1 bodies, 1 meshes sent (F2.b0)" "the block went to the worker" 20
expect_done "$(last_view)" "the next start renders"
ui_stop_app

echo "--- Scene updates and overlays in depth"
# The block, a smaller box beside it, and a sketch no feature uses (shown):
# a circle on the ground under the block and one in front of it.
cat > "$WORK/two.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 60, "height": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [80, 0], "width": 20, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 15}, "operation": "new_body"}},
  {"cmd": "set_body_appearance", "uid": "F2.b0", "appearance": "paint_red"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [30, 20], "diameter": 30},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [30, -25], "diameter": 14}
]
EOF
"$CLI" run "$WORK/two.json" --save "$WORK/two.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# blue_pixels shot: the sketch's blue pixels in the view.
blue_pixels() {
  python3 "$STATS" "$WORK/$1.xwd" $(view_region) | python3 -c 'import json, sys; print(json.load(sys.stdin)["blue"])'
}

# expect_scene description received changed: after the mark, every scene
# the worker applied received the meshes of `received` (or none: "-"), and
# one changed `changed` and kept the other body.
expect_scene() {
  local lines=""
  for _ in $(seq 1 100); do
    lines=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^Render scene [0-9]+ applied in" || true)
    echo "$lines" | grep -qF "changed $3;" && break
    sleep 0.2
  done
  [ -n "$lines" ] || ui_fail "$1: the worker applied no scene"
  echo "$lines" | grep -qF "received $2; added -; changed $3; moved -; removed -; kept 1;" ||
    ui_fail "$1: $(echo "$lines" | tail -1)"
  echo "$lines" | grep -vqF "received $2;" && ui_fail "$1: other meshes were sent: $lines"
  echo "ok   $1: $(echo "$lines" | tail -1 | sed 's/^Render //')"
}

ui_start_app --open "$WORK/two.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/two.mitcad" "the two bodies opened"
ui_step "hide the layout grid" ui_command "Layout Grid"
shot shaded-sketch
blue_shaded=$(blue_pixels shaded-sketch)
[ "$blue_shaded" -gt 50 ] || ui_fail "the sketch is not drawn in the shaded view ($blue_shaded blue pixels)"
ui_mark
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_new ": 2 bodies, 2 meshes sent (F2.b0, F4.b0)" "both bodies went to the worker" 20
ui_expect_new "received F2.b0, F4.b0; added F2.b0, F4.b0;" "the worker added both" 20
VIEW=$(last_view)
expect_done "$VIEW" "the two bodies rendered"
grep -qE "Render view $VIEW: first denoised frame .* after [0-9]+ ms" "$UI_LOG" ||
  ui_fail "no denoised frame of view $VIEW"
denoised=$(grep -cE "Render frame view $VIEW [0-9]+x[0-9]+ samples [0-9]+ denoised" "$UI_LOG")
[ "$denoised" -ge 2 ] || ui_fail "only $denoised denoised frames of view $VIEW: no preview before the last"
echo "ok   $denoised denoised frames, the $(grep -E "Render view $VIEW: first denoised frame" "$UI_LOG" | tail -1 | sed 's/.*: first/first/')"
shot rendered-sketch
blue_rendered=$(blue_pixels rendered-sketch)
echo "     sketch pixels: shaded $blue_shaded, rendered $blue_rendered"
[ "$blue_rendered" -gt $((blue_shaded / 2)) ] || ui_fail "the sketch in front of the block is not drawn over the render"
[ "$blue_rendered" -lt $((blue_shaded * 3 / 2)) ] ||
  ui_fail "the sketch behind the block is drawn over the render ($blue_rendered blue pixels, shaded $blue_shaded)"
echo "ok   the sketch under the block is hidden in the rendered view, as in the shaded one"

ui_step "Change Parameters" ui_command "Change Parameters"
ui_expect_log "Parameters d6 expression at" "the parameters' cells are logged"
ui_focus_dialog '^Parameters$'
ui_mark
ui_double_click_logged "Parameters d6 expression"
ui_sync
ui_key ctrl+a
xdotool type --delay 40 "25 mm"
ui_key Return
ui_expect_new "Parameter d6 expression: 25 mm" "the small box's extrusion is 25 mm"
expect_scene "only the changed body was sent" "F4.b0" "F4.b0"
ui_step "OK" ui_click_logged "Parameters OK"
ui_focus_main
ui_mark
ui_step "undo (Ctrl+Z)" ui_key ctrl+z
ui_expect_new "Undo: " "the change was undone"
expect_scene "the undone change sent no mesh (the worker holds it)" "-" "F4.b0"
ui_stop_app

echo "--- A worker of another protocol version"
printf '%s\n' '#!/bin/sh' "echo '{\"event\":\"hello\",\"protocol\":1,\"version\":\"0.0.1\"}'" \
  'exec cat > /dev/null' > "$WORK/old-render"
chmod +x "$WORK/old-render"
export MITCAD_RENDER_WORKER=$WORK/old-render
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_step "View > Rendered" ui_command "Rendered"
ui_expect_log "View message: The renderer cannot be used: mitcad-render does not match this Mitcad (its protocol version is 1, Mitcad's is " \
  "the view says the worker does not match"
ui_expect_log "Render worker stopped:" "the worker was stopped"
ui_crashed && ui_fail "the app ended"
ui_stop_app

echo "--- No worker"
export MITCAD_RENDER_WORKER=$WORK/no-such-render
ui_start_app --open "$WORK/block.mitcad" --no-native-dialogs
ui_expect_log "No render worker (mitcad-render) next to the application: no rendered view" \
  "no rendered view without the worker"
unset MITCAD_RENDER_WORKER

ui_finish "the rendered view"
