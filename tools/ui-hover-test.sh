#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/PickContext.cpp app/PickContext.hpp app/view app/render app/MainWindow.cpp
# The highlight under the pointer (mitcad#149) through the real UI, on a
# 60 x 40 x 30 mm block (Box1) and a lower, separate block in front of
# its lower left corner (Box2), from several view directions, two of them
# lighting the hovered face head-on (front and top, where a lit highlight
# washed out to white like the face itself):
#   - The pixels beside the pointer on the hovered face differ clearly
#     from the unhovered face, and lie between the face's own colour and
#     the hover colour: the highlight is see-through, the face's shading
#     shows through it.
#   - Hovering the big block's front face, where the small block covers a
#     part of it, leaves the small block's pixels as they were: the
#     highlight is not drawn over nearer geometry.
#   - The hover of an edge is drawn over the faces beside it.
#   - A selected face shows through the selection's highlight as through
#     the hover's.
#   - The same on the dark background and in the rendered view.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hover-test.sh
# check-all sources: tools/cli

UI_APP=${UI_APP:-$(cd "$(dirname "$0")/.." && pwd)/build/dev/app/mitcad}
BUILD_DIR=$(cd "$(dirname "$UI_APP")/.." 2> /dev/null && pwd)
source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$BUILD_DIR/tools/cli/mitcad-cli}
STATS="$(dirname "$0")/ui-image-stats.py"
WORK=$(mktemp -d /tmp/mitcad-ui-hover.XXXXXX)
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_LOG_ORIENTATION_CUBE=1
# The rendered view: few samples, on the CPU (tools/ui-render-test.sh).
export MITCAD_RENDER_SAMPLES=16
mkdir -p "$UI_CONFIG/Mitcad"
printf '[render]\ndevice=cpu\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
# The hover colour outside commands (MainWindow::updateIdleView).
HOVER="47 155 255"
# The selection's colour (MainWindow's kSelectionColor).
SELECTION="31 122 255"
FRONT='face F1.b0/F1:side0'
TOP='face F1.b0/F1:top'
FRONT_TOP_EDGE='edge F1.b0/E{F1:side0|F1:top}'
NEAR='face F2.b0/F2:side0'

cat > "$WORK/blocks.json" << EOF
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
    "length": 60, "width": 40, "height": 30, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [-10, -20],
    "length": 30, "width": 10, "height": 14, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/blocks.json" --save "$WORK/blocks.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# dump name: a dump of the screen.
dump() {
  ui_sync
  xwd -root -silent > "$WORK/$1.xwd" || ui_fail "no screenshot"
}

# mean name x y w h: the region's mean colour in the dump, "r g b".
mean() {
  python3 -c '
import importlib.util, sys
spec = importlib.util.spec_from_file_location("stats", sys.argv[1])
stats = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stats)
rows = stats.region_rows(sys.argv[2], *[int(v) for v in sys.argv[3:7]])
pixels = [p for row in rows for p in row]
print(" ".join("%.1f" % (sum(p[i] for p in pixels) / len(pixels)) for i in range(3)))' \
    "$STATS" "$WORK/$1.xwd" "$2" "$3" "$4" "$5"
}

# changed name x y w h: how many pixels of the region differ from the dump
# "rest".
changed() {
  python3 "$STATS" "$WORK/$1.xwd" "$2" "$3" "$4" "$5" "$WORK/rest.xwd" |
    sed -n 's/.*"changed": \([0-9]*\).*/\1/p'
}

# places: Fillet's Edges input (faces and edges) logs where a click picks
# each face and edge; cancelled, the hover is the one outside commands.
places() {
  ui_mark
  ui_step "fillet (F)" ui_key f
  ui_expect_new "Pick $FRONT_TOP_EDGE at" "the pick places are logged"
  ui_step "cancel (Esc)" ui_key Escape
}

# view cube-place direction: the orientation cube's face, edge or corner,
# fitted (F6); the pick places in it, and the view at rest without hover.
view() {
  echo "--- $1"
  ui_mark
  ui_step "the orientation cube's $1" ui_click_logged "Orientation cube $1"
  ui_expect_new "Camera direction $2 " "seen from the $1"
  ui_step "fit (F6)" ui_key F6
  places
  rest
}

# rest: the pointer off the blocks (nothing highlighted), a dump.
rest() {
  ui_step "pointer off the blocks" xdotool mousemove $(ui_view_at 3 50)
  dump rest
}

# see_through dump x y "r g b" description: in the 10 x 10 pixels at x, y
# the face under the highlight in the dump differs from the face without
# it (the dump "rest") by a clear margin and lies between it and the
# highlight's colour (r g b).
see_through() {
  local before after verdict
  before=$(mean rest "$2" "$3" 10 10)
  after=$(mean "$1" "$2" "$3" 10 10)
  verdict=$(python3 -c '
import sys
rest = [float(v) for v in sys.argv[1].split()]
hover = [float(v) for v in sys.argv[2].split()]
color = [float(v) for v in sys.argv[3].split()]
margin = max(abs(h - r) for h, r in zip(hover, rest))
# How far towards the hover colour, on the channel where they differ most.
k = max(range(3), key=lambda i: abs(color[i] - rest[i]))
share = (hover[k] - rest[k]) / (color[k] - rest[k]) if color[k] != rest[k] else 1.0
if margin < 40:
    print("only %.0f from the face without the highlight" % margin)
elif not 0.2 <= share <= 0.85:
    print("%.0f %% of the way to the highlight colour: not see-through" % (share * 100))
else:
    print("ok %.0f apart, %.0f %% of the way to the highlight colour" % (margin, share * 100))' \
    "$before" "$after" "$4")
  echo "     $5: face $before, highlighted $after"
  [ "${verdict#ok }" != "$verdict" ] || ui_fail "$5: $verdict"
  echo "ok   $5: ${verdict#ok }"
}

# hover_face "pick text" description: hovers the face where a click picks
# it; beside the pointer (left of it: the pointer's arrow is drawn to the
# right and below) the face shows through the highlight (see_through).
hover_face() {
  local px py
  read -r px py <<< "$(ui_pick_at "$1")"
  ui_step "hover $2" xdotool mousemove "$px" "$py"
  dump hover
  see_through hover $((px - 16)) $((py - 5)) "$HOVER" "$2"
}

# select_face "pick text" description: a click selects the face; with the
# pointer off the blocks the face shows through the selection's highlight
# as through the hover's. Esc clears the selection.
select_face() {
  local px py
  read -r px py <<< "$(ui_pick_at "$1")"
  ui_mark
  ui_step "select $2" xdotool mousemove "$px" "$py" click 1
  ui_expect_new "Selected: 1 face" "the face is selected"
  ui_step "pointer off the blocks" xdotool mousemove $(ui_view_at 3 50)
  dump selected
  see_through selected $((px - 16)) $((py - 5)) "$SELECTION" "$2, selected"
  ui_step "clear the selection (Esc)" ui_key Escape
}

# hover_edge "pick text" description: hovers the edge (along the view's x
# axis) where a click picks it; left of the pointer the edge's pixels
# changed to the hover's blue, over the faces on both sides of it.
hover_edge() {
  local px py count color
  read -r px py <<< "$(ui_pick_at "$1")"
  ui_step "hover $2" xdotool mousemove "$px" "$py"
  dump hover
  count=$(changed hover $((px - 24)) $((py - 1)) 16 3)
  color=$(mean hover $((px - 24)) $((py - 1)) 16 3)
  echo "     $2: $count of 48 pixels changed, now $color"
  [ -n "$count" ] && [ "$count" -ge 20 ] || ui_fail "$2: the edge is not highlighted"
  python3 -c 'import sys; r, g, b = map(float, sys.argv[1].split()); sys.exit(0 if b > r + 60 else 1)' \
    "$color" || ui_fail "$2: the edge is not in the hover colour"
  echo "ok   $2"
}

# hidden_behind "pick text" "near pick text" description: hovers a face
# that the near face covers in part; the near face's pixels stay.
hidden_behind() {
  local px py nx ny count
  read -r px py <<< "$(ui_pick_at "$1")"
  read -r nx ny <<< "$(ui_pick_at "$2")"
  ui_step "hover $3" xdotool mousemove "$px" "$py"
  dump hover
  count=$(changed hover $((nx - 5)) $((ny - 5)) 10 10)
  [ -n "$count" ] && [ "$count" -le 2 ] ||
    ui_fail "$3: the highlight is drawn over the nearer block (${count:-?} pixels changed)"
  echo "ok   $3: the nearer block hides the highlight"
}

ui_start_display
ui_start_app --open "$WORK/blocks.mitcad" --no-native-dialogs
ui_expect_log "Opened $WORK/blocks.mitcad" "the blocks opened"
ui_expect_log "Camera direction -0.577 0.577 -0.577" "the camera starts isometric"
# Without the layout grid, whose lines would cross the background.
ui_step "hide the layout grid" ui_command "Layout Grid"

echo "--- Isometric"
ui_step "fit (F6)" ui_key F6
places
rest
hover_face "$TOP" "the top face"
hover_face "$FRONT" "the front face"

view top "0 0 -1"
hover_face "$TOP" "the top face, lit head-on"

echo "--- Home"
ui_mark
ui_step "home" ui_command "Home"
ui_expect_new "Camera direction -0.568 0.654 -0.5 up" "the default view again (Home, mitcad#1698)"
view front-top "0 0.707 -0.707"
hover_face "$TOP" "the top face from the front and above"
hover_face "$FRONT" "the front face from the front and above"
hover_edge "$FRONT_TOP_EDGE" "the front face's top edge"

view front "0 1 0"
hover_face "$FRONT" "the front face, lit head-on"
hidden_behind "$FRONT" "$NEAR" "the front face behind the small block"
select_face "$FRONT" "the front face, lit head-on"

echo "--- The dark background"
ui_step "dark background" ui_command "Dark Background"
rest
hover_face "$FRONT" "the front face on the dark background"
hidden_behind "$FRONT" "$NEAR" "the front face behind the small block on the dark background"

if [ -x "$BUILD_DIR/app/mitcad-render" ]; then
  echo "--- The rendered view"
  ui_step "View > Rendered" ui_command "Rendered"
  ui_expect_log "Render worker ready" "the worker started"
  for _ in $(seq 1 300); do
    grep -qE "Render view [0-9]+: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" && break
    ui_crashed && ui_fail "the app crashed while rendering"
    sleep 0.2
  done
  grep -qE "Render view [0-9]+: $MITCAD_RENDER_SAMPLES samples in" "$UI_LOG" ||
    ui_fail "the render did not get its $MITCAD_RENDER_SAMPLES samples"
  echo "ok   rendered"
  rest
  hover_face "$FRONT" "the front face in the rendered view"
  hidden_behind "$FRONT" "$NEAR" "the front face behind the small block in the rendered view"
  ui_step "leave the rendered view" ui_command "Rendered"
else
  echo "note: no render worker next to the app: the rendered view is not tested"
fi

ui_finish "the hover highlight"
