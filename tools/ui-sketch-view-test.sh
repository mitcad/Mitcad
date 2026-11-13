#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/OcctViewer.cpp app/OcctViewer.hpp app/view app/MainWindow.cpp tools/cli
# check-all sources: app/framework/FloatingLayout.cpp
# check-all sources: core/model-api/src/sketch.rs core/model/src/features/sketch.rs core/sketch/src/def.rs
# How sketch mode shows its geometry, through the real UI:
#   - A sketch on XY: its origin is the design's (mitcad#254).
#   - A rectangle from the origin, its width and height dimensioned: fully
#     constrained.
#   - The dark background recolours its fully constrained lines light, the
#     light one dark again (mitcad#253).
#   - The pointer moved to known sketch points: the view reports their
#     sketch coordinates in mm (mitcad#254).
#   - Outside sketch editing the shown sketch keeps the fully constrained
#     colour, also when the background changes; editing it again changes
#     no colour (mitcad#286).
#   - A design (mitcad-cli) whose component's sketch, placed by its
#     occurrence, has a fully constrained rectangle and a free line: the
#     rectangle in the constrained colour, the line blue, in the main mode
#     and in its edit alike (mitcad#286), placed by the occurrence in both
#     (mitcad#505).
#   - A design (mitcad-cli) with a block and a sketch on its top face: a
#     fully constrained circle and a centre line on the face, a circle
#     beside the block. In the dark theme the lines on the face are light
#     with dark pixels of their halo beside them, in the light theme dark
#     with white ones; the circle over the empty background shows in both
#     (pixels of a screen dump; mitcad#302). Every background change also
#     restyles the halos ("Sketch halos:"). A free line on the face with a
#     driven length dimension: its dimension line on the face is a row in
#     the line's own tone between rows of its halo, dark ones in the dark
#     theme and light ones in the light theme, its dimensions' halos tone
#     logged ("Sketch dimension halos:"; mitcad#633).
#   - New sketches open with the design's Z up (mitcad#692): Create Sketch
#     on XZ, on YZ and on a block's face that faces -Y: each frame's
#     normal and x axis as commands.md "An upright sketch frame" gives
#     them, and the view shows the sketch's y axis up and its x axis to
#     the right, level (so a Horizontal line is horizontal on the screen).
#     The view cube's side each is seen from (mitcad#1060): XZ from FRONT
#     (normal -Y, x = +X), YZ from RIGHT, the -Y face from FRONT; XZ and
#     YZ also after Isometric View and the Sketch Palette's Look At.
#   - A sketch on a block's top face, edited from the front view (the face
#     edge-on, the view's middle far in front of it in depth): the face's
#     middle in the middle of the free area (mitcad#772).
#   - After New Design, sketches on XY, XZ and YZ show their origin in the
#     middle of the free area between the cards, in the Floating and the
#     Docked layout (mitcad#738: the turn kept the fit's shift), and are
#     seen from the view cube's TOP, FRONT and RIGHT, on entering and
#     after Isometric View and the Sketch Palette's Look At (mitcad#1060).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-view-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-view.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# sketch_colours F1 (an extended regex: "F1 in O1", "F1( in O[0-9]+)?"):
# "<constrained> <unconstrained>", the counts of the sketch's entities in
# the fully constrained colour and in the unconstrained blue from the last
# "Sketch colours:" line; empty when it does not name the sketch.
sketch_colours() {
  grep "Sketch colours: " "$UI_LOG" | tail -1 | sed 's/.*Sketch colours: //' | tr ';' '\n' |
    sed 's/^ *//' | grep -E "^$1 [0-9]+ fully constrained, [0-9]+ unconstrained\$" | head -1 |
    sed -E 's/.* ([0-9]+) fully constrained, ([0-9]+) unconstrained$/\1 \2/'
}

# expect_colours F1 "awk condition on c and u" description: within six
# seconds the sketch's counts (c fully constrained, u unconstrained) pass
# the condition; COLOURS keeps them ("c u").
expect_colours() {
  local counts
  for _ in $(seq 1 30); do
    counts=$(sketch_colours "$1")
    if [ -n "$counts" ] && awk -v c="${counts% *}" -v u="${counts#* }" "BEGIN { exit !($2) }"; then
      COLOURS=$counts
      echo "ok   $3: ${counts% *} fully constrained, ${counts#* } unconstrained"
      return
    fi
    sleep 0.2
  done
  ui_fail "$3: the last colours are '$(grep "Sketch colours: " "$UI_LOG" | tail -1)'"
}

# expect_recoloured dark|light: the sketch entities shown were recoloured
# for that background, the rectangle's four lines among the fully
# constrained ones.
expect_recoloured() {
  local count
  ui_expect_new "Sketch entities recoloured for a $1 background: " "recoloured for the $1 background"
  count=$(sed -n "s/.*Sketch entities recoloured for a $1 background: \([0-9]*\) fully constrained.*/\1/p" "$UI_LOG" |
    tail -1)
  [ -n "$count" ] && [ "$count" -ge 4 ] ||
    ui_fail "$1 background: '$count' fully constrained entities recoloured, expected 4 or more"
  echo "ok   $count fully constrained entities recoloured for the $1 background"
  # Their halos took the background's tone too (mitcad#302).
  count=$(grep "Sketch halos: " "$UI_LOG" | tail -1 | sed -n "s/.*Sketch halos: \([0-9]*\) $1\$/\1/p")
  [ -n "$count" ] && [ "$count" -ge 4 ] ||
    ui_fail "$1 background: the last halos are '$(grep "Sketch halos: " "$UI_LOG" | tail -1)'"
  echo "ok   $count halos for the $1 background"
}

# expect_pointer x y: the pointer moved to the sketch point (x, y) mm; the
# view reports that place, within two pixels.
expect_pointer() {
  local px py view
  ui_mark
  ui_step "pointer to ($1, $2)" xdotool mousemove $(ui_sketch_at "$1" "$2")
  ui_expect_new "Sketch pointer x " "the pointer's place at ($1, $2) logged"
  read -r px py <<< "$(sed -n 's/.*Sketch pointer x \([-0-9.]*\), y \([-0-9.]*\) mm.*/\1 \2/p' "$UI_LOG" | tail -1)"
  [ -n "$py" ] || ui_fail "the pointer's place is not in mm: $(grep 'Sketch pointer' "$UI_LOG" | tail -1)"
  view=$(grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" "$UI_LOG" | tail -1)
  awk -v view="$view" -v px="$px" -v py="$py" -v x="$1" -v y="$2" 'BEGIN {
    split(view, part, " "); split(part[3], o, ","); split(part[4], a, ",")
    tolerance = 2 * 100 / sqrt((a[1] - o[1]) ^ 2 + (a[2] - o[2]) ^ 2)
    exit !((px - x) ^ 2 <= tolerance ^ 2 && (py - y) ^ 2 <= tolerance ^ 2) }' ||
    ui_fail "the view reports ($px, $py) for the pointer at ($1, $2)"
  echo "ok   the view reports x $px, y $py for the pointer at ($1, $2)"
}

ui_start_display
ui_start_app

echo "--- A sketch on XY, its origin the design origin (mitcad#254)"
ui_step "light background (search)"      ui_command "Light Background"
ui_step "create sketch on XY"            ui_create_sketch xy
ui_expect_log "Sketch view" "the sketch view is logged"
ui_expect_log "Sketch origin at 0.00, 0.00, 0.00 mm in the design; the design origin at x 0.00, y 0.00 mm in the sketch" \
  "the sketch origin is the design origin"

echo "--- A fully constrained rectangle on a light background"
ui_step "rectangle tool (R)"             ui_key r
ui_step "first corner at the origin"     ui_sketch_click 0 0
ui_step "opposite corner"                ui_sketch_click 40 20
ui_expect_log "Added rectangle 40 x 20 mm at (0, 0)" "rectangle 40 x 20 at the origin"
ui_step "end the tool (Esc)"             ui_key Escape
ui_step "sketch dimension (D)"           ui_key d
ui_step "pick the bottom line"           ui_sketch_click 20 0
ui_step "place the value below it"       ui_sketch_click 20 -10
ui_step "type 40"                        xdotool type --delay 40 "40"
ui_step "Enter"                          ui_key Return
ui_step "pick the right line"            ui_sketch_click 40 10
ui_step "place the value beside it"      ui_sketch_click 52 10
ui_step "type 20"                        xdotool type --delay 40 "20"
ui_step "Enter"                          ui_key Return
ui_expect_log "Sketch Sketch1: 0 DOF, fully constrained" "fully constrained"
ui_step "end the tool (Esc)"             ui_key Escape

echo "--- Fully constrained lines on a dark background (mitcad#253)"
ui_mark
ui_step "dark background (search)"       ui_command "Dark Background"
expect_recoloured dark
if [ -n "$SHOT" ]; then
  ui_sync
  ui_capture "$SHOT"
fi
ui_mark
ui_step "light background (search)"      ui_command "Light Background"
expect_recoloured light

echo "--- The pointer's place in sketch coordinates (mitcad#254)"
expect_pointer 30 15
expect_pointer -15 -10

echo "--- Outside sketch editing the sketch keeps its colours (mitcad#286)"
# A new design's first sketch goes into a new component, Component1 (O1;
# mitcad#220): outside its edit that occurrence places it.
SKETCH1="F1( in O[0-9]+)?"
expect_colours "$SKETCH1" "c >= 4 && u == 0" "in its edit the rectangle is fully constrained"
EDITING=$COLOURS
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
ui_wait_idle
ui_sync
shown=$(grep "Sketches shown: " "$UI_LOG" | tail -1)
[ "${shown#*Sketches shown: }" = "F1" ] || ui_fail "the finished sketch is not shown: '$shown'"
echo "ok   the finished sketch is shown"
expect_colours "$SKETCH1" "c == ${EDITING% *} && u == ${EDITING#* }" "outside its edit the same colours, fully constrained"
ui_mark
ui_step "dark background (search)"       ui_command "Dark Background"
expect_recoloured dark
ui_mark
ui_step "light background (search)"      ui_command "Light Background"
expect_recoloured light
ui_double_click_expect "Timeline Sketch1" "Editing sketch F1 (Sketch1)" "the sketch is edited again"
ui_wait_idle
ui_sync
expect_colours "$SKETCH1" "c == ${EDITING% *} && u == ${EDITING#* }" "entering its edit changes no colour"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished again"

echo "--- A component's sketch placed by its occurrence (mitcad#286)"
# Part:1 (O1) 100 mm along X, its Sketch1 (F1) a dimensioned 30 x 20
# rectangle (fully constrained) and a free line.
cat > "$WORK/component.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Part"},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F1", "start": [0, 30], "end": [20, 40]},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "set_occurrence_transform", "occurrence": "O1", "transform": {"translation": [100, 0, 0]}}
]
EOF
"$CLI" run "$WORK/component.json" --save "$WORK/component.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_stop_app
ui_start_app --open "$WORK/component.mitcad"
UI_MARK=0
ui_expect_log "Sketches shown: F1" "the component's sketch is shown"
expect_colours "F1 in O1" "c >= 4 && u >= 1" "placed by Part:1, the rectangle fully constrained and the line blue"
PLACED=$COLOURS
ui_mark
ui_step "dark background (search)"       ui_command "Dark Background"
expect_recoloured dark
ui_mark
ui_step "light background (search)"      ui_command "Light Background"
expect_recoloured light
ui_expect_log "Timeline Sketch1 at" "the timeline is laid out"
ui_double_click_expect "Timeline Sketch1" "Editing sketch F1 (Sketch1)" "the component's sketch is edited"
ui_wait_idle
ui_sync
# Edited where Part:1 places it too (mitcad#505), not at Part's own
# coordinates.
expect_colours "F1 in O1" "c == ${PLACED% *} && u == ${PLACED#* }" "in its edit placed by Part:1, the same colours"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "sketch finished"
ui_wait_idle
ui_sync
expect_colours "F1 in O1" "c == ${PLACED% *} && u == ${PLACED#* }" "placed by Part:1 again, the same colours"

echo "--- Fully constrained curves and a centre line over a body's face (mitcad#302)"
# A 60 x 40 x 30 mm block centred on the origin; on its top face (sketch
# origin over the design's) a fully constrained circle on the face, one
# beside the block over the empty background, and a centre line on the
# face. Each is drawn over a halo in the background's tone, which keeps it
# readable over the face in both themes. A free line (c1, first in the
# sketch) on the face has a driven length dimension whose value is placed
# beyond the line's end: its dimension line (y 16 mm, x -28 to -4 mm, and
# on to the value at x 12 mm) is painted over a halo too (mitcad#633).
cat > "$WORK/face.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-30, -20],
    "length": 60, "width": 40, "height": 30, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F1:top", "body": "F1.b0"}},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [-28, 10], "end": [-4, 10]},
  {"cmd": "sketch.add_dimension", "sketch": "F2", "dimension": {"type": "length", "line": "c1"},
   "driven": true, "text": [12, 16]},
  {"cmd": "sketch.add_circle", "sketch": "F2", "center": [10, 0], "diameter": 16},
  {"cmd": "sketch.add_circle", "sketch": "F2", "center": [50, 0], "diameter": 16},
  {"cmd": "sketch.add_line", "sketch": "F2", "start": [-25, -12], "end": [25, -12], "centerline": true}
]
EOF
"$CLI" run "$WORK/face.json" --save "$WORK/face.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_stop_app
ui_start_app --open "$WORK/face.mitcad"
UI_MARK=0
ui_expect_log "Sketches shown: F2" "the face's sketch is shown"
# The circles and their centres fully constrained, the centre line's free
# ends blue.
expect_colours F2 "c == 4" "both circles fully constrained"
ui_expect_log "Timeline Sketch1 at" "the timeline is laid out"
ui_double_click_expect "Timeline Sketch1" "Editing sketch F2 (Sketch1)" "the face's sketch is edited"
ui_wait_idle
ui_sync

# expect_halos "3 dark": within six seconds the last "Sketch halos:" line
# says so.
expect_halos() {
  for _ in $(seq 1 30); do
    [ "$(grep "Sketch halos: " "$UI_LOG" | tail -1 | sed 's/.*Sketch halos: //')" = "$1" ] &&
      { echo "ok   halos: $1"; return; }
    sleep 0.2
  done
  ui_fail "halos: the last line is '$(grep "Sketch halos: " "$UI_LOG" | tail -1)', expected '$1'"
}

# expect_dimension_halos dark|light: within six seconds the last "Sketch
# dimension halos:" line says the dimensions (the two circles' diameters and
# the line's length, three) are drawn over a halo of that tone (mitcad#633).
expect_dimension_halos() {
  for _ in $(seq 1 30); do
    [ "$(grep "Sketch dimension halos: " "$UI_LOG" | tail -1 | sed 's/.*Sketch dimension halos: //')" = "3 $1" ] &&
      { echo "ok   dimension halos: 3 $1"; return; }
    sleep 0.2
  done
  ui_fail "dimension halos: the last line is '$(grep "Sketch dimension halos: " "$UI_LOG" | tail -1)', expected '3 $1'"
}

# tones name x y "w h": the pixels of the region centred on screen point
# (x, y) in the dump of the screen: "light dark white min max", the counts
# of luminance above 200, below 70 and above 244, and the lowest and
# highest luminance.
tones() {
  local w=${4% *} h=${4#* }
  convert "$WORK/$1.xwd" -crop "${w}x${h}+$(($2 - w / 2))+$(($3 - h / 2))" +repage -depth 8 txt:- |
    sed -n 's/^[0-9]*,[0-9]*: *(\([0-9.]*\),\([0-9.]*\),\([0-9.]*\).*/\1 \2 \3/p' |
    awk 'BEGIN { min = 255; max = 0 }
      { l = 0.299 * $1 + 0.587 * $2 + 0.114 * $3
        light += l > 200; dark += l < 70; white += l > 244
        if (l < min) min = l; if (l > max) max = l }
      END { printf "%d %d %d %d %d\n", light, dark, white, min, max }'
}

# region name sketch-x sketch-y "w h" description: the tones of the region
# around a sketch point, in L (light), D (dark), W (white), MIN and MAX.
region() {
  local px py
  read -r px py <<< "$(ui_sketch_at "$2" "$3")"
  read -r L D W MIN MAX <<< "$(tones "$1" "$px" "$py" "$4")"
  [ -n "$MAX" ] || ui_fail "$5: no pixels at ($px, $py)"
  echo "     $5 at ($px, $py): $L light, $D dark, $W white, luminance $MIN-$MAX"
}

# check name "condition" description: an awk condition on L, D, W, MIN, MAX.
check() {
  awk -v L="$L" -v D="$D" -v W="$W" -v MIN="$MIN" -v MAX="$MAX" "BEGIN { exit !($2) }" ||
    ui_fail "$1: $3 ($L light, $D dark, $W white, luminance $MIN-$MAX)"
  echo "ok   $1: $3"
}

# A strip across each curve, wider than the line and its halo: across the
# right side of each circle and across the centre line (along it, so that
# its dashes are in it).
ACROSS="25 5"
ALONG="41 17"
# A strip across the free line's dimension line: 21 px along it, 9 across.
# The 1 px line over its 3 px halo is painted on whole pixel rows
# (mitcad#633), so that the line's own row lies between rows of its halo.
DIMENSION="21 9"

# line_in_halo name sketch-x sketch-y "w h" "line" "halo" description: the
# mean luminance of each pixel row of the region (top to bottom) has a row
# passing the awk condition "line" on l, with a row passing "halo" on l
# among the two rows above it and among the two below it: a level line in
# its own tone with its halo on both sides.
line_in_halo() {
  local px py rows
  read -r px py <<< "$(ui_sketch_at "$2" "$3")"
  local w=${4% *} h=${4#* }
  rows=$(convert "$WORK/$1.xwd" -crop "${w}x${h}+$((px - w / 2))+$((py - h / 2))" +repage -depth 8 txt:- |
    sed -n 's/^[0-9]*,\([0-9]*\): *(\([0-9.]*\),\([0-9.]*\),\([0-9.]*\).*/\1 \2 \3 \4/p' |
    awk '{ sum[$1] += 0.299 * $2 + 0.587 * $3 + 0.114 * $4; n[$1]++ }
      END { for (r = 0; r in n; r++) printf "%d ", sum[r] / n[r] }')
  [ -n "$rows" ] || ui_fail "$7: no pixels at ($px, $py)"
  echo "     $1: the rows across $7 at ($px, $py): $rows"
  awk -v rows="$rows" "
    function line(l) { return $5 }
    function halo(l) { return $6 }
    BEGIN {
      n = split(rows, l, \" \")
      for (r = 3; r <= n - 2; r++)
        if (line(l[r]) && (halo(l[r - 1]) || halo(l[r - 2])) && (halo(l[r + 1]) || halo(l[r + 2])))
          exit 0
      exit 1 }" ||
    ui_fail "$1: $7 is not a line in its tone between rows of its halo (rows $rows)"
  echo "ok   $1: $7 is a line in its tone between rows of its halo"
}

ui_step "dark background (search)"       ui_command "Dark Background"
expect_halos "3 dark"
ui_sync
sleep 1
xwd -root -silent > "$WORK/dark.xwd" || ui_fail "no screenshot"
[ -z "$SHOT" ] || convert "$WORK/dark.xwd" "${SHOT%.png}-face-dark.png"
region dark 18 0 "$ACROSS" "dark: the circle on the face"
check dark "L >= 2 && D >= 2" "the circle on the face is light over a dark halo"
region dark -10 -12 "$ALONG" "dark: the centre line on the face"
check dark "D >= 2 && MAX >= 140" "the centre line on the face is light over a dark halo"
region dark 58 0 "$ACROSS" "dark: the circle beside the block"
check dark "L >= 2" "the circle over the dark background is light"
# The driven dimension's line on the face (about 167, the driven tone for a
# dark background, 0xa0 0xa8 0xb4) between rows of its dark halo (about
# 36), on the face's light grey (about 223).
expect_dimension_halos dark
line_in_halo dark -16 16 "$DIMENSION" "l >= 140" "l <= 70" "the dimension line on the face"

ui_step "light background (search)"      ui_command "Light Background"
expect_halos "3 light"
ui_sync
sleep 1
xwd -root -silent > "$WORK/light.xwd" || ui_fail "no screenshot"
[ -z "$SHOT" ] || convert "$WORK/light.xwd" "${SHOT%.png}-face-light.png"
region light 18 0 "$ACROSS" "light: the circle on the face"
check light "MIN <= 110 && W >= 2" "the circle on the face is dark over a light halo"
region light -10 -12 "$ALONG" "light: the centre line on the face"
check light "W >= 2 && MIN <= 160" "the centre line on the face is darker over a light halo"
region light 58 0 "$ACROSS" "light: the circle beside the block"
check light "MIN <= 110" "the circle over the light background is dark"
# The driven dimension's line on the face (about 117, the driven grey
# 0x70 0x76 0x80) between rows of its light halo (about 250), on the
# face's light grey (about 223).
expect_dimension_halos light
line_in_halo light -16 16 "$DIMENSION" "l <= 150" "l >= 240" "the dimension line on the face"

echo "--- New sketches open with the design's Z up (mitcad#692)"
# A 40 x 30 x 20 block behind the origin as the isometric view sees it
# (x -80 to -40, y 40 to 70), clear of the origin planes; its side0 faces
# -Y at y = 40.
cat > "$WORK/upright.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-80, 40],
    "length": 40, "width": 30, "height": 20, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/upright.json" --save "$WORK/upright.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_upright where "normal" "x axis": the sketch just started lies in
# the design with that normal and x axis (as the "Sketch frame in the
# design" line writes them), and within six seconds the view shows its
# sketch point (100, 0) to the right of (0, 0) at its height and (0, 100)
# straight above it: the sketch's y up, its x level.
expect_upright() {
  local frame view
  ui_expect_new "Sketch frame in the design" "$1: the sketch's frame logged"
  frame=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Sketch frame in the design" | tail -1)
  [[ $frame == *"normal ($2), x axis ($3)"* ]] ||
    ui_fail "$1: '$frame', expected normal ($2), x axis ($3)"
  echo "ok   $1: normal ($2), x axis ($3)"
  for _ in $(seq 1 30); do
    ui_sync
    view=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" | tail -1)
    if [ -n "$view" ] && awk -v view="$view" 'BEGIN {
      split(view, part, " "); split(part[3], o, ","); split(part[4], a, ","); split(part[5], b, ",")
      t = sqrt((a[1] - o[1]) ^ 2 + (a[2] - o[2]) ^ 2) / 100
      exit !(a[1] > o[1] && (a[2] - o[2]) ^ 2 <= t ^ 2 && b[2] < o[2] && (b[1] - o[1]) ^ 2 <= t ^ 2) }'; then
      echo "ok   $1: the sketch's y up and its x level to the right ($view)"
      return
    fi
    sleep 0.2
  done
  ui_fail "$1: the view does not show the sketch's y up and its x to the right: '$view'"
}

# expect_side where SIDE "along": since the mark the view turned to the
# sketch plane from that side of the view cube, looking along that
# direction (mitcad#1060).
expect_side() {
  ui_expect_new "Sketch plane seen from $2 (along $3)" "$1: seen from $2"
}

# look_at_again where SIDE "along": Isometric View turns the view away
# from the sketch, then the Sketch Palette's Look At turns it back to the
# same side.
look_at_again() {
  ui_mark
  ui_step "isometric view (search)"      ui_command "Isometric View"
  ui_expect_new "Camera direction" "$1: the view turned away" 8
  ui_expect_log "Palette Look At at" "$1: the palette's Look At is laid out"
  ui_mark
  ui_step "Look At (palette)"            ui_click_logged "Palette Look At"
  expect_side "$1 after Look At" "$2" "$3"
}

ui_stop_app
# The block lies clear of the origin planes as the 100 mm Centered style
# draws them, which reach into the fitted view (mitcad#1748: the default
# 80 px Corner planes at the origin stay out of sight there).
ui_origin_in_millimeters
ui_start_app --open "$WORK/upright.mitcad"
ui_step "fit (F6)"                       ui_key F6
ui_mark
ui_step "create sketch on XZ"            ui_create_sketch xz
expect_upright "XZ" "0.000000, -1.000000, 0.000000" "1.000000, 0.000000, 0.000000"
expect_side "XZ" FRONT "0 1 0"
look_at_again "XZ" FRONT "0 1 0"
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_log "Sketch finished" "the XZ sketch finished"
ui_mark
ui_step "create sketch on YZ"            ui_create_sketch yz
expect_upright "YZ" "1.000000, 0.000000, 0.000000" "0.000000, 1.000000, 0.000000"
expect_side "YZ" RIGHT "-1 0 0"
look_at_again "YZ" RIGHT "-1 0 0"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the YZ sketch finished"
ui_step "front view (search)"            ui_command "Front View"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_mark
ui_step "create sketch (search)"         ui_command "Create Sketch"
ui_expect_new "Pick face F1.b0/F1:side0 at" "the block's -Y face can be picked"
ui_step "pick the -Y face"               ui_click_pick "face F1.b0/F1:side0"
ui_expect_new 'Sketch started on {"body":"F1.b0","face":"F1:side0"}' "a sketch on the -Y face"
expect_upright "the -Y face" "0.000000, -1.000000, 0.000000" "1.000000, 0.000000, 0.000000"
expect_side "the -Y face" FRONT "0 1 0"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the face's sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- A sketch on a face seen edge-on stays in view (mitcad#772)"
# A 40 x 40 x 20 block far in front (y -400 to -360) and a 40 x 40 x 40
# block (F2.b0, x 20 to 60, y 0 to 40) beside it; a sketch (F3) on the
# second block's top face (z 40), which the front view sees edge-on. The
# view's middle lies half way between the blocks in depth: projected onto
# the top face's plane it is about 180 mm in front of the face, which the
# turn to the sketch plane took out of the view. Edit Sketch from the front
# view shows the middle of the face (40, 20, 40) in the middle of the free
# area instead.
cat > "$WORK/oblique.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-60, -400],
    "length": 40, "width": 40, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [20, 0],
    "length": 40, "width": 40, "height": 40, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": {"face": "F2:top", "body": "F2.b0"}}
]
EOF
"$CLI" run "$WORK/oblique.json" --save "$WORK/oblique.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
ui_stop_app
ui_start_app --open "$WORK/oblique.mitcad"
UI_MARK=0
ui_expect_log "Bodies shown: F1.b0, F2.b0" "the blocks shown"
ui_step "front view (search)"            ui_command "Front View"
ui_step "fit (F6)"                       ui_key F6
ui_sync
ui_expect_log "Timeline Sketch1 at" "the timeline is laid out"
ui_mark
ui_double_click_expect "Timeline Sketch1" "Editing sketch F3 (Sketch1)" "the top face's sketch is edited from the front"
ui_expect_new "Sketch plane centred on the sketch's face: " "the view turned about the middle of the face"
ui_expect_new "Sketch frame in the design" "the sketch's frame logged"
# Within six seconds the face's middle, from the sketch's frame (its origin,
# normal and x axis in the design) and the sketch view (where the sketch
# points (0, 0), (100, 0) and (0, 100) are in the window), is within 6 px
# of the middle of the free area.
frame=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Sketch frame in the design" | tail -1)
centred=""
for i in $(seq 1 30); do
  ui_sync
  line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch plane centred on .*" | tail -1)
  view=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" | tail -1)
  if [ -n "$view" ] && awk -v frame="$frame" -v line="$line" -v view="$view" 'BEGIN {
    if (line !~ /^Sketch plane centred on the sketch.s face: origin at /) exit 1
    # "... origin (x, y, z), normal (x, y, z), x axis (x, y, z)"
    sub(/.*origin \(/, "", frame)
    if (split(frame, part, /\), [a-z ]+ \(/) < 3) exit 1
    split(part[1], o, /, */); split(part[2], nn, /, */); split(part[3], xa, /[,)] */)
    # The sketch y axis: normal x x axis.
    ya[1] = nn[2] * xa[3] - nn[3] * xa[2]; ya[2] = nn[3] * xa[1] - nn[1] * xa[3]
    ya[3] = nn[1] * xa[2] - nn[2] * xa[1]
    d[1] = 40 - o[1]; d[2] = 20 - o[2]; d[3] = 40 - o[3]
    sx = d[1] * xa[1] + d[2] * xa[2] + d[3] * xa[3]
    sy = d[1] * ya[1] + d[2] * ya[2] + d[3] * ya[3]
    split(view, v, " "); split(v[3], p0, ","); split(v[4], pa, ","); split(v[5], pb, ",")
    px = p0[1] + (pa[1] - p0[1]) * sx / 100 + (pb[1] - p0[1]) * sy / 100
    py = p0[2] + (pa[2] - p0[2]) * sx / 100 + (pb[2] - p0[2]) * sy / 100
    sub(/.*free area centre at /, "", line); split(line, f, ",")
    exit !((px - f[1]) ^ 2 + (py - f[2]) ^ 2 <= 36) }'; then
    centred=$i
    break
  fi
  sleep 0.2
done
[ -n "$centred" ] ||
  ui_fail "the top face's middle is not in the middle of the free area: '$frame', '$line', '$view'"
echo "ok   the top face's middle in the middle of the free area ($line; $view)"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"     ui_key ctrl+Return
ui_expect_new "Sketch finished" "the top face's sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- After New Design, a sketch's origin in the middle of the free area (mitcad#738)"
# expect_centred "description": since the mark the view turned to the new
# sketch's plane centred on its origin (a new design shows no bodies), and
# within six seconds the origin, as the view logs it and as the sketch logs
# it ("Sketch view"), is within 6 px of the middle of the free area between
# the cards.
expect_centred() {
  local line view
  ui_expect_new "Sketch plane centred on " "$1: the view turned to the plane"
  for _ in $(seq 1 30); do
    ui_sync
    line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch plane centred on .*" | tail -1)
    view=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Sketch view [-0-9.,]* [-0-9.,]* [-0-9.,]*" | tail -1)
    if [ -n "$view" ] && awk -v line="$line" -v view="$view" 'BEGIN {
      if (line !~ /^Sketch plane centred on the origin: origin at /) exit 1
      sub(/.*origin at /, "", line); split(line, part, /; free area centre at /)
      split(part[1], o, ","); split(part[2], f, ",")
      split(view, v, " "); split(v[3], s, ",")
      exit !((o[1] - f[1]) ^ 2 + (o[2] - f[2]) ^ 2 <= 36 && (s[1] - f[1]) ^ 2 + (s[2] - f[2]) ^ 2 <= 36) }'; then
      echo "ok   $1: the origin in the middle of the free area ($line; $view)"
      return
    fi
    sleep 0.2
  done
  ui_fail "$1: the origin is not in the middle of the free area: '$line', '$view'"
}

# sketch_on_origin plane: Create Sketch on an origin plane, clicked in the
# view: Create Sketch shows the origin planes in every design (mitcad#844),
# also after New Design, which starts with its Component1 active
# (mitcad#898).
sketch_on_origin() {
  local plane=$1 before
  before=$(grep -c "Sketch started on $plane" "$UI_LOG")
  ui_mark
  ui_command "Create Sketch"
  ui_expect_new "Create Sketch: origin planes shown" "Create Sketch shows the origin planes" > /dev/null
  ui_expect_new "Datum $plane at" "the ${plane^^} plane can be picked" > /dev/null
  ui_sync
  ui_click_logged "Datum $plane"
  for _ in $(seq 1 25); do
    [ "$(grep -c "Sketch started on $plane" "$UI_LOG")" -gt "$before" ] && return 0
    sleep 0.2
  done
  ui_fail "no sketch started on $plane"
}

# centred_round layout [app arguments]: New Design (its fit centres the
# design in the free area, which moves the camera's centre off the origin
# once the cards are laid out), then sketches on XY, XZ and YZ in turn;
# each shows its origin in the middle of the free area and is seen from
# the view cube's TOP, FRONT and RIGHT, also after Look At (mitcad#1060).
centred_round() {
  local layout=$1 plane side along
  shift
  ui_stop_app
  ui_start_app "$@"
  ui_mark
  ui_step "$layout: new design (Ctrl+N)" ui_key ctrl+n
  ui_expect_new "New document" "$layout: a new design"
  ui_expect_new "Opened the new design with Component1 active" "$layout: the new design has Component1 active"
  ui_wait_idle
  ui_sync
  for plane in xy xz yz; do
    ui_mark
    ui_step "$layout: create sketch on ${plane^^}" sketch_on_origin "$plane"
    expect_centred "$layout, ${plane^^}"
    case $plane in
      xy) side=TOP along="0 0 -1" ;;
      xz) side=FRONT along="0 1 0" ;;
      yz) side=RIGHT along="-1 0 0" ;;
    esac
    expect_side "$layout, ${plane^^}" "$side" "$along"
    look_at_again "$layout, ${plane^^}" "$side" "$along"
    ui_mark
    ui_step "$layout: finish sketch (Ctrl+Enter)" ui_key ctrl+Return
    ui_expect_new "Sketch finished" "$layout: the ${plane^^} sketch finished"
  done
}

centred_round "Floating" --chrome=floating
centred_round "Docked" --chrome=docked

ui_finish "UI sketch view test"
