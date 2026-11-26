#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/view/ReplacedPictures.hpp
# check-all sources: app/OcctViewer.cpp app/sketch/SketchOverlay.cpp app/sketch/SketchController.cpp
# check-all sources: app/framework/FloatingLayout.cpp app/framework/MessageBand.cpp tools/cli
# Edit Sketch left alone keeps its memory (mitcad#2453), through the real UI
# in the Floating layout, on a design mitcad-cli makes: Sketch1 (a 40 x 25
# rectangle with its two dimensions and a free circle), Extrude1 of the
# rectangle. The view's frames and the sketch overlay's paints are counted
# from the timing lines (MITCAD_LOG_TIMING), the app's memory is its RSS:
#   - Edit Sketch1, the hint faded to the message band's stub: 20 s with the
#     pointer resting over the view draw no frame, paint the overlay no
#     time, and the RSS grows less than 8 MB.
#   - Then 300 small moves of the pointer over the view, as a hand resting
#     on a mouse or a touchpad gives while the sketch is "left alone": each
#     move shows another place over the badge, so the overlay is painted
#     again, each time a new picture of the view's size (4 MB at 1280 x 800)
#     with a texture of its own. The replaced pictures' textures are let go
#     by the view's full frames (mitcad#2437, view/ReplacedPictures.hpp):
#     at rest afterwards the RSS is less than 120 MB above what it was;
#     without that rule each picture's texture stayed (1.2 GB here).
#
# Runs headless on Xvfb (see ui-test-lib.sh), where Mesa's software renderer
# keeps the textures in the app's own memory.
# Usage: tools/ui-sketch-idle-memory-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-idle-memory.XXXXXX)
FILE=$WORK/idle.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [70, 10], "diameter": 12},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# Every frame of the view ("Timing frame: …") and every paint of a paint
# layer ("Timing view paint layer: …, sketch overlay") is logged.
export MITCAD_LOG_TIMING=1

# rss: the app's resident memory in kB.
rss() { awk '/^VmRSS:/ {print $2}' "/proc/$(ui_app_pid)/status"; }
# since pattern: the lines since the mark that start with the pattern.
since() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -c "^$1"; }

ui_start_display
ui_start_app --open "$FILE" --chrome=floating
ui_expect_log "Opened $FILE" "the design is open"
ui_sync

echo "--- Edit Sketch1, the hint faded"
ui_mark
ui_step "edit Sketch1 (double-click)"    ui_double_click_logged "Timeline Sketch1"
ui_expect_new "Editing sketch F1 (Sketch1)" "Sketch1 edited" 30
ui_expect_new "Sketch badge 'Sketch1: " "the badge placed"
ui_step "pointer over the view"          xdotool mousemove $(ui_view_at 60 40)
ui_sync
# The sketch's hint fades to the band's stub after 6 s: the band's card
# moves, and the overlay keeps clear of it (one paint).
ui_expect_new "Message band stub: hint Sketch" "the hint faded to the stub" 20
ui_sync
sleep 1

echo "--- Left alone for 20 s"
ui_mark
before=$(rss)
sleep 20
after=$(rss)
frames=$(since "Timing frame:")
paints=$(since "Timing view paint layer")
[ "$frames" = 0 ] || ui_fail "the view drew $frames frame(s) while the sketch was left alone: $(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -v '^Timing' | head -5)"
[ "$paints" = 0 ] || ui_fail "the overlay was painted $paints time(s) while the sketch was left alone"
[ $((after - before)) -lt 8192 ] || ui_fail "the RSS grew from $before kB to $after kB while the sketch was left alone"
echo "ok   no frame, no paint, RSS $before kB -> $after kB"

echo "--- 300 small moves of the pointer"
read -r PX PY <<< "$(ui_view_at 60 40)"
ui_mark
before=$(rss)
for i in $(seq 1 300); do
  xdotool mousemove $((PX + i % 60)) $((PY + i / 60 * 3))
  sleep 0.03
done
ui_sync
sleep 2
ui_sync
after=$(rss)
paints=$(since "Timing view paint layer")
[ "$paints" -ge 50 ] || ui_fail "the overlay was painted only $paints time(s) for 300 moves: the moves did not show their places"
[ $((after - before)) -lt 122880 ] ||
  ui_fail "the RSS grew from $before kB to $after kB over $paints paints of the overlay: the replaced pictures' textures stayed"
echo "ok   $paints paints of the overlay, RSS $before kB -> $after kB"

echo "--- Left alone again"
ui_mark
before=$(rss)
sleep 10
after=$(rss)
paints=$(since "Timing view paint layer")
[ "$paints" = 0 ] || ui_fail "the overlay was painted $paints time(s) while the sketch was left alone after the moves"
[ $((after - before)) -lt 8192 ] || ui_fail "the RSS grew from $before kB to $after kB while left alone after the moves"
echo "ok   no paint, RSS $before kB -> $after kB"

ui_finish "UI sketch idle memory test"
