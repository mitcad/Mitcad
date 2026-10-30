#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A feature preview cut by Section Analysis (mitcad#943): a one-off visual
# check, not a recurring test (its name is not ui-*-test.sh, so no check
# picks it up). It models a tube along Y in its component Tube (outside
# 40 mm, inside 30 mm, 80 long) with a ring in the component Nut around
# its first 20 mm (outside 50 mm, inside 42 mm), cuts both lengthwise with
# Section Analysis on the YZ plane, starts Chamfer (4 mm) on the outside
# edge of the tube's free end, which the plane cuts in two, and saves
# pictures of the screen on the light and the dark background:
#   <dir>/section-light.png         the section alone
#   <dir>/preview-light.png         the section with the Chamfer preview
#   <dir>/preview-close-light.png   the same, zoomed in at the free end
#   <dir>/section-dark.png    <dir>/preview-dark.png    <dir>/preview-close-dark.png
# each with <dir>/<name>-view.png, the 3D view cut out of that same
# picture; the design as <dir>/look.mitcad, the app's log as
# <dir>/look.log, its section lines (the preview's clip and caps) as
# <dir>/section-lines.log (the last screen dump stays as <dir>/screen.xwd).
#
# What to look for: in the preview pictures the tube cut at the plane as in
# the section pictures, its caps (two strips along the tube) in Tube's
# colour and hatch, the bevel of the chamfer cut too where the strips end;
# no uncut tube (its far half, its whole inside) drawn across the removed
# side, and the ring cut with its own caps throughout (drawn faded while
# Tube is active, as another component's bodies are: the Chamfer pictures
# and section-dark). The picked edge's highlight is a marker of the input,
# not a body, and is drawn whole.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-section-preview-look.sh <dir>

source "$(dirname "$0")/ui-test-lib.sh"

DIR=${1:?usage: tools/ui-section-preview-look.sh <dir>}
mkdir -p "$DIR"
DIR=$(cd "$DIR" && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
trap 'cp -f "$UI_LOG" "$DIR/look.log" 2> /dev/null; ui_cleanup' EXIT

# Tube: F1/F2 the outside (F2.b0), F3/F4 the bore cut through it.
# Nut: F5/F6 the ring's outside (F6.b0), F7/F8 its bore.
cat > "$DIR/look.json" << EOF
[
  {"cmd": "create_component", "name": "Tube"},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_circle", "sketch": "F1", "center": [0, 0], "diameter": 40},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 80}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [0, 0], "diameter": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F2.b0"]}},
  {"cmd": "activate_component", "component": "Root"},
  {"cmd": "create_component", "name": "Nut"},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [0, 0], "diameter": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_circle", "sketch": "F7", "center": [0, 0], "diameter": 42},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F7", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut", "participants": ["F6.b0"]}},
  {"cmd": "activate_component", "component": "Root"}
]
EOF
"$CLI" run "$DIR/look.json" --save "$DIR/look.mitcad" > "$DIR/cli.log" 2>&1 ||
  { cat "$DIR/cli.log"; ui_fail "mitcad-cli (the tube and the ring)"; }

# away: the mouse off the window (no hover, no tooltip).
away() { xdotool mousemove 5000 5000; }

# shot name: the whole screen once the view has settled, the mouse away and
# a tooltip gone, and the 3D view cut out of that same picture.
shot() {
  local dump="$DIR/screen.xwd" from to
  away
  ui_wait_idle
  ui_sync
  sleep 2
  xwd -root -silent > "$dump"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1.png"
  read -r -a from <<< "$(ui_view_at 0 0)"
  read -r -a to <<< "$(ui_view_at 100 100)"
  python3 "$(dirname "$0")/xwd2png.py" "$dump" "$DIR/$1-view.png" \
    "${from[0]}" "${from[1]}" $((to[0] - from[0])) $((to[1] - from[1]))
  echo "ok   $DIR/$1.png $DIR/$1-view.png"
}

# pick_where "start" "part"...: clicks where the newest "Pick <start>..."
# line since the last "Pick places:" that has every part too was logged
# (as in ui-section-caps-test.sh).
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

# chamfer background: Chamfer on the outside edge of the tube's free end,
# its preview clipped at the plane, the pictures taken, then cancelled.
chamfer() {
  ui_mark
  ui_step "chamfer (search)"                 ui_command "Chamfer"
  ui_expect_new "Pick places:" "Chamfer logs where its picks are"
  ui_step "pick the tube's free end, outside" pick_where "edge F2.b0/E{" "F2:side(c1" "F2:end("
  ui_expect_new "Chamfer Edges: 1 edge" "the cut tube's edge picked"
  ui_expect_new "Preview Chamfer: ok" "previewed"
  ui_expect_new "Section preview ready at " "the preview's body clipped and handed to the view" 20
  ui_expect_new " (preview)" "the caps follow the preview"
  # 4 mm (of the 5 mm wall), so that the bevel shows; the Distance field
  # has the focus with its text selected once the edge is picked.
  local first=$UI_MARK at px py
  ui_mark
  ui_step "type 4 mm"                        xdotool type --delay 40 "4"
  ui_expect_new "Chamfer Distance: 4 = 4 mm" "the distance 4 mm"
  ui_expect_new "Section preview ready at " "the new preview clipped and handed to the view" 20
  ui_expect_new " (preview)" "the caps follow it"
  UI_MARK=$first
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -q '^Section: F2.b0 shown whole' && ui_fail "the preview's clip failed"
  shot "preview-$1"
  # Closer to the free end: the wheel at the picked edge's place.
  at=$(grep "Pick edge F2.b0/E{F2:end(" "$UI_LOG" | grep -F "F2:side(c1" | tail -1 |
    sed -n 's/.* at \([0-9]*\),\([0-9]*\)$/\1 \2/p')
  read -r px py <<< "$at"
  xdotool mousemove $((X + px * UI_SCALE)) $((Y + py * UI_SCALE))
  ui_step "zoom in at the free end (wheel)"  xdotool click --repeat 4 --delay 150 4
  shot "preview-close-$1"
  ui_step "fit (F6)"                         ui_key F6
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E '^Section|^Preview Chamfer|^Chamfer (Edges|Distance)' >> "$DIR/section-lines.log"
  ui_mark
  ui_step "cancel (Escape)"                  ui_key Escape
  ui_expect_new "Command Chamfer cancelled" "the chamfer cancelled"
}

: > "$DIR/section-lines.log"
ui_start_display
ui_start_app --open "$DIR/look.mitcad"
ui_expect_log "Bodies shown: " "the tube and the ring shown"
grep -q "Recompute failed\|Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- The light background"
ui_mark
ui_step "light background (search)"        ui_command "Light Background"
ui_expect_new "Background " "the background light"
ui_step "isometric view (search)"          ui_command "Isometric View"
ui_sync
ui_step "fit (F6)"                         ui_key F6

echo "--- Section Analysis on YZ, through the tube's axis"
ui_mark
ui_step "section analysis (search)"        ui_command "Section Analysis"
ui_step "pick YZ"                          ui_click_logged "Datum yz"
ui_expect_new "Section Analysis Faces/Plane: 1 plane [plane yz]" "YZ picked"
ui_expect_new " face(s) at" "the tube and the ring cut" 20
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Closed Section Analysis" "closed, the section kept"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E '^Section' >> "$DIR/section-lines.log"
shot section-light

# A feature works on the bodies of the active component: Tube first.
ui_mark
ui_step "activate Tube (radio)"            ui_click_logged "Browser radio Root/Tube:1"
ui_expect_new "Activated Tube" "Tube active"
chamfer light

echo "--- The dark background"
ui_mark
ui_step "dark background (search)"         ui_command "Dark Background"
ui_expect_new "Background " "the background dark"
shot section-dark
chamfer dark

ui_finish "Section preview look pictures in $DIR"
