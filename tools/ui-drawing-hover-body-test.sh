#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_spacing.rs
# check-all sources: core/model-drawing/src/annotate.rs tools/cli
# The Dimension tool's hover names and outlines the body a corner or an
# edge is on, and its placing preview writes the value the dimension gets
# (mitcad#1691), through the real UI, on a frame that mitcad-cli makes: two
# square bars 20 x 20 standing for tubes, one along x from (0, 0) to
# (200, 20), the other along y from (200, 0) to (220, 200), touching at
# x 200 so that the corner (200, 0) is a corner of both; a drawing of them
# with its top view at 1:2, the frame's middle (110, 100) at (180, 160) on
# the sheet:
#   1. Dimension: the pointer on the shared corner: a filled marker there,
#      the body the model's pick is on outlined ("Drawing hover outline:
#      <uid> (<name>), N lines") and named in words ("Drawing hover words:
#      Corner of <name>"); a click picks that very body's corner.
#   2. The pointer on the first bar's long side: "Edge of <its name>".
#   3. The first bar's far corner (0, 0) as the second pick; the pointer
#      below them: the preview writes the dimension's value ("Drawing
#      preview value: 200"); the click there places a dimension that
#      writes the same.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-hover-body-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-hover-body.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/frame.mitcad

cat > "$WORK/frame.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 200, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [200, 0], "width": 20, "height": 200},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"expect": {"feature": "F2", "status": "ok"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"expect": {"bodies": 2}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [180, 160], "scale": "1:2"}
]
EOF
"$CLI" run "$WORK/frame.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)); as tools/ui-drawing-corners-test.sh.
sheet_at() {
  local view
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  awk -v view="$view" -v x="$1" -v y="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    px = o[1] + (a[1] - o[1]) * x / 100 + (b[1] - o[1]) * y / 100
    py = o[2] + (a[2] - o[2]) * x / 100 + (b[2] - o[2]) * y / 100
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
# frame_at x y: the sheet point of the frame's point (x, y) in its top view.
frame_at() { awk -v x="$1" -v y="$2" 'BEGIN { printf "%.2f %.2f\n", 180 + (x - 110) / 2, 160 + (y - 100) / 2 }'; }
frame_click() { ui_sync; xdotool mousemove $(sheet_at $(frame_at "$1" "$2")) click 1; ui_sync; }
frame_hover() { ui_sync; xdotool mousemove $(sheet_at $(frame_at "$1" "$2")); ui_sync; }

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# newest "text" "description": waits for a line with the text since the
# mark and prints the newest one's rest after the text.
newest() {
  local line
  for _ in $(seq 1 50); do
    ui_wait_idle
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "$1" | tail -n 1)
    if [ -n "$line" ]; then
      echo "${line#*"$1"}"
      return 0
    fi
    sleep 0.1
  done
  ui_fail "$2: no '$1' in the log since the mark"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "frame.mitcad" "the design opened"

ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet view " "the sheet placed"
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"

echo "--- 1. The shared corner: its body outlined and named"
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_expect_new "Drawing hint: Click a straight edge for its length" "the hint says one straight edge gives its length"
ui_mark
ui_step "point at the shared corner"       frame_hover 200 0
ui_expect_new "Drawing corner marker at 225.0,110.0, radius 6.0 px, filled" "a filled marker on the corner"
outline=$(newest "Drawing hover outline: " "the corner's body outlined")
uid=${outline%% (*}
name=${outline#* (}
name=${name%%), *}
count=${outline##*), }
count=${count%% lines}
case "$uid" in F2.b0 | F4.b0) ;; *) ui_fail "the outline names no bar: '$outline'" ;; esac
[ "$count" -ge 4 ] 2> /dev/null || ui_fail "not the bar's lines outlined: '$outline'"
echo "ok   outlined: $outline"
words=$(newest "Drawing hover words: " "the corner in words")
[ "$words" = "Corner of $name" ] || ui_fail "the hover's words are '$words', not 'Corner of $name'"
echo "ok   in words: $words"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "the shared corner"                frame_click 200 0
ui_expect_new "Drawing picked point (1): {\"body\":\"$uid\"" "the click picks the outlined body's corner"

echo "--- 2. The first bar's long side in words"
ui_mark
ui_step "point at the first bar's side"    frame_hover 100 20
words=$(newest "Drawing hover words: Edge of " "the edge in words")
outline=$(newest "Drawing hover outline: F2.b0 (" "the first bar outlined")
first=${outline%%), *}
[ "$words" = "$first" ] || ui_fail "the edge is named 'Edge of $words', not 'Edge of $first'"
echo "ok   Edge of $words"

echo "--- 3. The preview writes the value the dimension gets"
ui_mark
ui_step "the first bar's far corner"       frame_click 0 0
ui_expect_new "Drawing picked point (2)" "the second corner picked"
ui_expect_new "Drawing placing view" "the dimension placed next"
ui_step "the pointer below them"           frame_hover 100 -30
value=$(newest "Drawing preview value: " "the preview's value")
[ "$value" = "200" ] || ui_fail "the preview writes '$value', not 200"
echo "ok   the preview writes $value"
ui_step "where it goes, below them"        frame_click 100 -30
ui_expect_new "Drawing annotations drawn: 1 = $value" "the placed dimension writes what the preview did"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape

ui_finish "UI drawing hover body test"
