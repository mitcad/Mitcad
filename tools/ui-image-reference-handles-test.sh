#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowImageReferences.cpp app/OcctViewer.cpp app/view app/browser app/framework tools/cli
# check-all sources: core/model/src/document core/model-api/src core/model-base/src
# Image Reference's handles (mitcad#1747, GitHub #281) through the real UI,
# in the Docked layout, on a 60 x 60 x 3 plate and a 200 x 100 pixel grey
# BMP file of 0.25 mm pixels (50 x 25 mm):
#   - Insert > Image Reference on the plate's top, the file's path typed:
#     the image itself drawn while the panel is open (named by its file,
#     50 x 25 mm, 50 % opaque), with its handles: arrows along the plane's
#     x and y at its centre, a ring round it, an arrow at each corner.
#   - The X arrow dragged along itself: X changed (the drag's value, and
#     the arrow's knob, the image's centre, moved).
#   - The ring dragged about its centre: Angle changed.
#   - The bottom right corner dragged outward: Width larger, the image
#     drawn that wide while the panel is open, its aspect ratio kept, and
#     the top left corner (the corner arrow's origin) where it was, within
#     0.01 mm.
#   - OK: Image1 inserted that wide.
#   - Edited (its row double-clicked) and X dragged: the image drawn once,
#     at its new place, not also where it was; Esc keeps it as it was.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-image-reference-handles-test.sh [screenshot.png]
# (the screenshot shows the handles while the panel is open, a corner's
# value box under the mouse).

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-image-reference-handles.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# A 60 x 60 x 3 plate (Box1 F1, Body1).
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [0, 0], "length": 60, "width": 60, "height": 3, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$WORK/plate.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli model"; }

# The BMP file: a 54-byte header (200 x 100, 24 bits, 4000 pixels per
# metre), then 100 rows of 600 grey bytes.
{
  printf 'BM\x96\xea\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00'
  printf '\x28\x00\x00\x00\xc8\x00\x00\x00\x64\x00\x00\x00\x01\x00\x18\x00'
  printf '\x00\x00\x00\x00\x60\xea\x00\x00\xa0\x0f\x00\x00\xa0\x0f\x00\x00'
  printf '\x00\x00\x00\x00\x00\x00\x00\x00'
  head -c 60000 /dev/zero | tr '\0' '\200'
} > "$WORK/plan.bmp"
[ "$(wc -c < "$WORK/plan.bmp")" = 60054 ] || ui_fail "the BMP file is not 60054 bytes"

PANEL="Image Reference"

# handle_world key: the newest "Handle world Image Reference <key>: origin
# x,y,z, knob x,y,z, ..." as "ox oy oz kx ky kz" (an arrow's start and
# knob in the design).
handle_world() {
  grep -F "Handle world $PANEL $1: origin " "$UI_LOG" | tail -1 |
    sed -n 's/.*: origin \([-0-9.]*\),\([-0-9.]*\),\([-0-9.]*\), knob \([-0-9.]*\),\([-0-9.]*\),\([-0-9.]*\),.*/\1 \2 \3 \4 \5 \6/p'
}

# arrow_direction key: the newest "Arrow direction Image Reference <key>:
# x,y,z" as "x y z".
arrow_direction() {
  grep -F "Arrow direction $PANEL $1: " "$UI_LOG" | tail -1 | sed -n 's/.*: \([-0-9.]*\),\([-0-9.]*\),\([-0-9.]*\)$/\1 \2 \3/p'
}

# expect_handles: the panel's handles logged since the mark.
expect_handles() {
  local key
  for key in x y angle width.0 width.1 width.2 width.3; do
    ui_expect_new "Manipulator $PANEL $key at " "the handle $key in the view"
  done
}

# drag_along key pixels: drags arrow <key> from its middle along itself,
# towards its head, that many pixels of the screen.
drag_along() {
  local mx my hx hy dx dy
  read -r mx my <<< "$(ui_logged_at "Manipulator $PANEL $1")"
  read -r hx hy <<< "$(ui_logged_at "Arrow head $PANEL $1")"
  read -r dx dy <<< "$(awk -v x=$((hx - mx)) -v y=$((hy - my)) -v n="$2" \
    'BEGIN { l = sqrt(x * x + y * y); if (l < 1) l = 1; printf "%d %d\n", x / l * n, y / l * n }')"
  ui_drag_from "Manipulator $PANEL $1" "$dx" "$dy"
}

# dragged key: sets DRAGGED to the value of the newest "Manipulator Image
# Reference <key> dragged to <value>" since the mark.
dragged() {
  DRAGGED=""
  for _ in $(seq 1 30); do
    DRAGGED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n "s/^Manipulator $PANEL $1 dragged to \([-0-9.e+]*\)$/\1/p" | tail -1)
    [ -n "$DRAGGED" ] && return
    sleep 0.2
  done
  ui_fail "no drag of $1 ended"
}

# moved_world key before: waits until the newest "Handle world" line of
# <key> has its knob more than 0.4 mm from the knob in <before> ("ox oy oz
# kx ky kz"), and sets WORLD to that line's numbers.
moved_world() {
  WORLD=""
  for _ in $(seq 1 30); do
    WORLD=$(handle_world "$1")
    if [ -n "$WORLD" ] && awk -v a="$2" -v b="$WORLD" 'BEGIN {
         split(a, p, " "); split(b, q, " ");
         exit !((q[4] - p[4]) ^ 2 + (q[5] - p[5]) ^ 2 + (q[6] - p[6]) ^ 2 > 0.16) }'; then
      return
    fi
    sleep 0.2
  done
  ui_fail "the knob of $1 did not move: $2 then $WORLD"
}

# expect_drawn_size name width when: the newest "Image references drawn:
# <name> W x H mm ..." since the mark is <width> wide and half as high,
# within 0.06 mm (the log's one decimal).
expect_drawn_size() {
  local drawn="" ok=0
  for _ in $(seq 1 30); do
    drawn=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
      sed -n "s/^Image references drawn: $1 \([0-9.]*\) x \([0-9.]*\) mm.*/\1 \2/p" | tail -1)
    if [ -n "$drawn" ] && awk -v d="$drawn" -v w="$2" \
      'BEGIN { split(d, s, " "); exit !((s[1] - w) ^ 2 < 0.0036 && (s[2] - w / 2) ^ 2 < 0.0036) }'; then
      ok=1
      break
    fi
    sleep 0.2
  done
  [ "$ok" = 1 ] || ui_fail "$1 is drawn '$drawn' mm, not $2 mm wide and half as high, $3"
  echo "ok   $1 drawn $drawn mm $3"
}

# panel_centre name: the newest "Image Reference panel image <name> centre
# x,y,z" since the mark as "x y z" (where the panel's image is drawn).
panel_centre() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "$PANEL panel image $1 centre " | tail -1 |
    sed -n 's/.* centre \([-0-9.]*\),\([-0-9.]*\),\([-0-9.]*\).*/\1 \2 \3/p'
}

ui_start_display
ui_start_app --open "$WORK/plate.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Insert > Image Reference on the plate's top: the image drawn with its handles"
ui_mark
ui_step "Image Reference"                  ui_command "$PANEL"
ui_expect_new "Command $PANEL started" "the panel opened"
ui_step "click the plate's top"            ui_click_pick "face F1.b0/F1:top"
ui_expect_log "$PANEL Plane: 1 face" "the face picked"
ui_step "the image's file"                 ui_type_in "Panel $PANEL input file" "$WORK/plan.bmp"
ui_expect_new "$PANEL panel image plan.bmp decoded: 200 x 100 pixels" "the file's picture decoded"
ui_expect_new "Image references drawn: plan.bmp 50.0 x 25.0 mm 50% behind" \
  "the image drawn while the panel is open, 50 x 25 mm"
expect_handles
grep -q "Inserted image reference" "$UI_LOG" && ui_fail "inserted before OK"

echo "--- The X arrow: X changes"
BEFORE=$(handle_world x)
read -r DX DY DZ <<< "$(arrow_direction x)"
[ -n "$BEFORE" ] && [ -n "$DZ" ] || ui_fail "the X arrow's place is not logged"
# X now: the knob's distance from the arrow's start along it.
X0=$(awk -v w="$BEFORE" -v dx="$DX" -v dy="$DY" -v dz="$DZ" \
  'BEGIN { split(w, p, " "); printf "%.4f\n", (p[4] - p[1]) * dx + (p[5] - p[2]) * dy + (p[6] - p[3]) * dz }')
ui_mark
# Back along the arrow: the ring round the image stays on the plate's side
# of the view.
ui_step "drag the X arrow"                 drag_along x -60
dragged x
awk -v a="$X0" -v b="$DRAGGED" 'BEGIN { exit !((b - a) ^ 2 > 0.16) }' ||
  ui_fail "X dragged to $DRAGGED from $X0: not changed"
echo "ok   X dragged from $X0 to $DRAGGED mm"
moved_world x "$BEFORE"
echo "ok   the X arrow's knob (the image's centre) moved with it"

echo "--- The ring: Angle changes"
read -r CX CY <<< "$(ui_logged_at "Ring centre $PANEL angle")"
read -r KX KY <<< "$(ui_logged_at "Manipulator $PANEL angle")"
# 60 pixels across the ring's radius at its knob.
read -r TX TY <<< "$(awk -v x=$((KX - CX)) -v y=$((KY - CY)) \
  'BEGIN { l = sqrt(x * x + y * y); if (l < 1) l = 1; printf "%d %d\n", -y / l * 60, x / l * 60 }')"
ui_mark
ui_step "drag the ring"                    ui_drag_from "Manipulator $PANEL angle" "$TX" "$TY"
dragged angle
awk -v a="$DRAGGED" 'BEGIN { exit !(a * a > 0.0004) }' || ui_fail "Angle dragged to $DRAGGED radians: not changed"
echo "ok   Angle dragged to $DRAGGED radians"

echo "--- The bottom right corner: Width grows, the top left corner stays"
CORNER=$(handle_world width.2)
[ -n "$CORNER" ] || ui_fail "the bottom right corner's place is not logged"
ui_mark
ui_step "drag the bottom right corner"     drag_along width.2 50
dragged width
awk -v w="$DRAGGED" 'BEGIN { exit !(w > 50.5) }' || ui_fail "Width dragged to $DRAGGED: not larger than 50 mm"
echo "ok   Width dragged to $DRAGGED mm"
moved_world width.2 "$CORNER"
awk -v a="$CORNER" -v b="$WORLD" 'BEGIN {
    split(a, p, " "); split(b, q, " ");
    exit !((q[1] - p[1]) ^ 2 + (q[2] - p[2]) ^ 2 + (q[3] - p[3]) ^ 2 < 0.0001) }' ||
  ui_fail "the top left corner moved: $CORNER then $WORLD"
echo "ok   the top left corner stayed (within 0.01 mm): $CORNER then $WORLD"
# Drawn that wide while the panel is open, half as high (aspect kept).
expect_drawn_size "plan\.bmp" "$DRAGGED" "while the panel is open"
WIDTH=$DRAGGED

if [ -n "$SHOT" ]; then
  # The handles, and a corner's value box under the mouse.
  xdotool mousemove $(ui_logged_at "Manipulator $PANEL width.2")
  ui_sync
  ui_capture "$SHOT"
fi

echo "--- OK: inserted that wide"
ui_mark
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Inserted image reference from plan.bmp" "inserted"
expect_drawn_size Image1 "$WIDTH" "after OK"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- Edited: drawn at its new place only"
ui_double_click_expect "Browser Root/Images/Image1" "Editing image reference Image1 with $PANEL" \
  "the edit panel opened"
EDIT_MARK=$UI_MARK
expect_handles
BEFORE=$(handle_world x)
ui_mark
ui_step "drag the X arrow"                 drag_along x -60
dragged x
moved_world x "$BEFORE"
# The image drawn where X's knob is now: its centre.
CENTRE=""
for _ in $(seq 1 30); do
  CENTRE=$(panel_centre Image1)
  [ -n "$CENTRE" ] && awk -v c="$CENTRE" -v w="$WORLD" 'BEGIN {
      split(c, p, " "); split(w, q, " ");
      exit !((q[4] - p[1]) ^ 2 + (q[5] - p[2]) ^ 2 + (q[6] - p[3]) ^ 2 < 0.0001) }' && break
  CENTRE="(not at the knob: $CENTRE)"
  sleep 0.2
done
[[ $CENTRE != "("* ]] && [ -n "$CENTRE" ] || ui_fail "while edited, the image is not drawn at X's knob: $CENTRE, $WORLD"
echo "ok   while edited, the image drawn at its new centre $CENTRE"
# One entry since the panel opened: the image where the panel puts it,
# not also where it was.
if tail -n +$((EDIT_MARK + 1)) "$UI_LOG" | grep '^Image references drawn: ' | grep -q ', '; then
  ui_fail "while edited, drawn twice: $(tail -n +$((EDIT_MARK + 1)) "$UI_LOG" | grep '^Image references drawn: ' | tail -1)"
fi
LAST=$(grep '^Image references drawn: ' "$UI_LOG" | tail -1)
[[ $LAST =~ ^Image\ references\ drawn:\ Image1\ [0-9.]+\ x\ [0-9.]+\ mm\ [0-9]+%\ (behind|through)$ ]] ||
  ui_fail "while edited, drawn as '$LAST' (one Image1 expected)"
echo "ok   while edited, drawn once: $LAST"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command $PANEL cancelled" "cancelled"
grep -q "Edited image reference Image1" "$UI_LOG" && ui_fail "edited although cancelled"

ui_finish "UI image reference handles test"
