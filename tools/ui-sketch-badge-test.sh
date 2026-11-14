#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchOverlay.cpp app/sketch/SketchOverlay.hpp app/framework/FloatingLayout.cpp
# check-all sources: app/OcctViewer.hpp app/OcctViewer.cpp app/framework/Language.cpp app/translations
# check-all sources: app/browser/BrowserPanel.cpp app/view/AnnotationsWindow.cpp app/MainWindowMeasurements.cpp
# check-all sources: core/model-api/src/annotations.rs core/model/src/document/annotations.rs tools/cli
# Sketch mode's badge, its English and the annotations while a sketch is
# edited (mitcad#1756), through the real UI in the Floating layout, on a
# design mitcad-cli makes: 30 empty sketches, then Sketch31 (a free
# rectangle and a free line), Extrude1 of its rectangle (Body1, F32.b0) and
# two material labels on Body1's faces (Material1, Material2):
#   - The Browser card open down to the timeline (its Sketches folder
#     open): editing Sketch31 puts its badge ("Sketch31: <n> degrees of
#     freedom") and the pointer's place above it clear of every floating
#     card, right of the Browser card, which reaches down past the badge's
#     row; with the pointer in the view too.
#   - English plurals (mitcad#1756): "degrees of freedom", never
#     "degree(s)".
#   - The edit rolls the timeline back to Sketch31: the labels are rolled
#     back with Extrude1, the Annotations row of the Browser never turns a
#     warning, the Annotations window's rows say "Rolled back" without a
#     warning; Finish Sketch brings them back as before.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# The Windows counterpart is tools/ui-windows-sketch-badge-test.ps1.
# Usage: tools/ui-sketch-badge-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sketch-badge.XXXXXX)
FILE=$WORK/badge.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

# 30 empty sketches (F1-F30), so that the Browser's rows overflow with
# Sketches open; Sketch31 (F31) with a rectangle and a line, neither
# constrained; Extrude1 (F32) of the rectangle, Body1 (F32.b0); the labels
# on its top and front faces.
{
  echo '['
  for _ in $(seq 1 30); do echo '  {"cmd": "sketch.create"},'; done
  sed "s/RECT/$RECT/g" << 'JSON'
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F31", "corner": [0, 0], "width": 20, "height": 20},
  {"cmd": "sketch.add_line", "sketch": "F31", "start": [40, 0], "end": [50, 10]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F31", "region": "RECT"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "new_body"}},
  {"cmd": "add_annotation", "kind": "material", "a": {"body": "F32.b0", "face": "F32:end(RECT)"}, "at": [5, 5, 20]},
  {"cmd": "add_annotation", "kind": "material", "a": {"body": "F32.b0", "face": "F32:side(c1[c4,c2])"},
    "at": [10, 0, 10]}
]
JSON
} > "$WORK/badge.json"
"$CLI" run "$WORK/badge.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# No Projects card: the Browser's card has the column from the rows down
# to the timeline. The Browser on its card.
mkdir -p "$UI_CONFIG/Mitcad"
printf '[projects]\npanel=false\n[appearance]\nbrowserBackground=card\n' > "$UI_CONFIG/Mitcad/Mitcad.conf"

# card_rect name: "x y width height" of the card in the last "Floating
# cards:" line (window coordinates).
card_rect() {
  grep '^Floating cards: ' "$UI_LOG" | tail -1 | tr ';' '\n' | sed 's/^ *//' | grep "^$1 " |
    sed -n 's/^[^ ]* \(-\{0,1\}[0-9]*\),\(-\{0,1\}[0-9]*\) \([0-9]*\)x\([0-9]*\)$/\1 \2 \3 \4/p'
}

# badge: the last "Sketch badge" line as "bx by bw bh px py pw ph": the
# badge's box and the pointer's place's, in the window's coordinates.
badge() {
  grep "^Sketch badge '" "$UI_LOG" | tail -1 |
    sed -n "s/.*' at \(-*[0-9]*\),\(-*[0-9]*\) \([0-9]*\)x\([0-9]*\), pointer place \(-*[0-9]*\),\(-*[0-9]*\) \([0-9]*\)x\([0-9]*\), view .*/\1 \2 \3 \4 \5 \6 \7 \8/p"
}

# covering x y w h: the floating cards of the last "Floating cards:" line
# that the rectangle overlaps ("name x,y wxh; …"); empty when none.
covering() {
  grep '^Floating cards: ' "$UI_LOG" | tail -1 | sed 's/^[^;]*; //' | tr ';' '\n' | sed 's/^ *//' |
    awk -v x="$1" -v y="$2" -v w="$3" -v h="$4" '
      match($0, /(-?[0-9]+),(-?[0-9]+) ([0-9]+)x([0-9]+)$/) {
        split(substr($0, RSTART), p, /[, x]/)
        cx = p[1]; cy = p[2]; cw = p[3]; ch = p[4]
        if (x < cx + cw && cx < x + w && y < cy + ch && cy < y + h) printf "%s; ", $0
      }'
}

# expect_clear description: within five seconds the badge and the pointer's
# place overlap no floating card (the cards and the badge follow each
# other); BADGE keeps the place ("bx by bw bh px py pw ph").
expect_clear() {
  local hit
  for _ in $(seq 1 25); do
    read -r -a BADGE <<< "$(badge)"
    if [ "${#BADGE[@]}" = 8 ]; then
      hit="$(covering "${BADGE[0]}" "${BADGE[1]}" "${BADGE[2]}" "${BADGE[3]}")$(covering "${BADGE[4]}" "${BADGE[5]}" "${BADGE[6]}" "${BADGE[7]}")"
      [ -z "$hit" ] && { echo "ok   $1: the badge at ${BADGE[0]},${BADGE[1]} ${BADGE[2]}x${BADGE[3]}, the pointer's place at ${BADGE[4]},${BADGE[5]} ${BADGE[6]}x${BADGE[7]}, under no card"; return 0; }
    fi
    sleep 0.2
  done
  ui_fail "$1: the badge or the pointer's place lies under a card: ${hit:-no badge logged} ($(grep "^Sketch badge '" "$UI_LOG" | tail -1))"
}

ui_start_display
ui_start_app --open "$FILE" --chrome=floating
ui_expect_log "Opened $FILE" "the design is open"
ui_expect_log "Annotations: Material1 material | Material2 material" "both labels listed, none broken"

echo "--- The Browser card down to the timeline"
# Its 31 sketches do not fit: the design opens folded below the root
# (mitcad#426); Sketches opened again, the card grows down to the timeline.
ui_expect_log "Browser opened folded: " "the design opens folded"
ui_expect_log "Browser arrow Root/Sketches at" "the Sketches folder's arrow"
ui_sync
ui_mark
ui_step "open Sketches"                  xdotool mousemove $(ui_logged_at "Browser arrow Root/Sketches") click 1
ui_expect_new "Browser Root/Sketches/Sketch1 at" "the sketches in sight"
ui_sync

echo "--- Edit Sketch31: the badge clear of the cards (mitcad#1756)"
ui_mark
ui_step "edit Sketch31 (its Browser row, double-click)" ui_double_click_logged "Browser Root/Sketches/Sketch31"
ui_expect_new "Editing sketch F31 (Sketch31)" "Sketch31 edited"
ui_expect_new "Sketch badge 'Sketch31: " "the badge placed"
ui_sync
expect_clear "the pointer outside the view"
read -r LX LY LW LH <<< "$(card_rect browserDock)"
[ -n "$LH" ] || ui_fail "no Browser card logged: $(grep '^Floating cards: ' "$UI_LOG" | tail -1)"
# The case of the report: the Browser card reaches down past the badge's
# row, and the badge is right of it.
[ $((LY + LH)) -gt "${BADGE[1]}" ] ||
  ui_fail "the Browser card ($LX,$LY ${LW}x$LH) ends above the badge's row (y ${BADGE[1]}): open its rows down to the timeline"
[ "${BADGE[0]}" -ge $((LX + LW)) ] ||
  ui_fail "the badge (x ${BADGE[0]}) is not right of the Browser card ($LX,$LY ${LW}x$LH)"
echo "ok   right of the Browser card ($LX,$LY ${LW}x$LH), which reaches past the badge's row"

echo "--- English plurals (mitcad#1756)"
text=$(grep "^Sketch badge '" "$UI_LOG" | tail -1 | sed -n "s/^Sketch badge '\([^']*\)'.*/\1/p")
[[ $text =~ ^Sketch31:\ [0-9]+\ degrees\ of\ freedom$ ]] || ui_fail "the badge says '$text', not 'Sketch31: <n> degrees of freedom'"
echo "ok   the badge says '$text'"
grep -q "degree(s)" "$UI_LOG" && ui_fail "a text says 'degree(s)': $(grep -m1 "degree(s)" "$UI_LOG")"
echo "ok   no 'degree(s)' anywhere in the log"

echo "--- The pointer in the view: its place above the badge, clear of the cards too"
ui_mark
ui_step "pointer into the view"         xdotool mousemove $(ui_view_at 60 50)
ui_expect_new "Sketch pointer x " "the pointer's place logged"
ui_sync
expect_clear "the pointer in the view"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- The labels rolled back with Extrude1, no warning"
ui_expect_log "Annotations: Material1 rolled back: " "Material1 rolled back with the timeline"
grep "^Annotations: " "$UI_LOG" | tail -1 | grep -q "| Material2 rolled back: " ||
  ui_fail "Material2 is not rolled back: $(grep "^Annotations: " "$UI_LOG" | tail -1)"
echo "ok   Material2 rolled back too"
ui_mark
ui_step "Annotations window (the Browser's folder, double-click)" ui_double_click_logged "Browser Root/Annotations"
ui_focus_dialog '^Annotations$'
ui_expect_new "Annotations row Material1: material, Rolled back, " "Material1's row: rolled back"
ui_expect_new "Annotations row Material2: material, Rolled back, " "Material2's row: rolled back"
ui_step "close the window (Escape)"     ui_key Escape
ui_focus_main

echo "--- Finish Sketch: as before"
ui_mark
ui_step "finish sketch (Ctrl+Enter)"    ui_key ctrl+Return
ui_expect_new "Sketch finished" "Sketch31 finished"
ui_expect_new "Annotations: Material1 material | Material2 material" "both labels back, none broken"
ui_sync
grep -q "^Browser level .*/Annotations: warning" "$UI_LOG" &&
  ui_fail "the Browser's Annotations row turned a warning: $(grep -m1 "^Browser level .*/Annotations: warning" "$UI_LOG")"
grep -q "^Annotations level Material[12]: warning" "$UI_LOG" &&
  ui_fail "a label was a warning: $(grep -m1 "^Annotations level Material" "$UI_LOG")"
echo "ok   no warning on the Annotations row or a label, while editing or after"

ui_finish "UI sketch badge test"
