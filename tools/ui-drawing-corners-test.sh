#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/view/ViewController.cpp core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs tools/cli
# Corners on drawings (mitcad#1338) through the real UI, on a plate that
# mitcad-cli makes: 90 x 50 x 5 with a hole of 16 through it at (30, 25), a
# drawing of it with its top view at 2:1 (its automatic centre marks on),
# so that the plate's point (x, y) lies at (120 + 2x, 100 + 2y) on the
# sheet:
#   1. Dimension: the pointer on the plate's corner (90, 0) shows a filled
#      marker there ("Drawing corner marker ..., filled"), on the hole's
#      vertical centre line inside the hole the hole's centre with the line
#      ("Drawing hover: center line").
#   2. The corner, then the hole's centre line, placed above them: a
#      horizontal 60 (from the corner to the vertical centre line); the
#      centre line, then the corner, placed beside them: a vertical 25.
#   3. Export PDF and SVG with Always show corners off; no corner dots.
#   4. Preferences > Display: Always show corners in drawings on, Corner
#      dot size Large (kept in the settings); the drawing again: its
#      corners as dots of 4.5 px; the same PDF and SVG exported, byte for
#      byte (the dots are on the screen only).
#   5. Preferences: off; the drawing again: no dots.
# tools/ui-windows-drawing-corners-test.ps1 is the Windows counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-corners-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-corners.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/plate.mitcad
CONF=$UI_CONFIG/Mitcad/Mitcad.conf

cat > "$WORK/plate.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 90, "height": 50},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 5}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F3", "center": [30, 25], "diameter": 16},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F4", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [210, 150], "scale": "2:1"}
]
EOF
"$CLI" run "$WORK/plate.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# sheet_at x y: the screen position of a sheet point (mm), from the logged
# "Drawing sheet view x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0),
# (0, 100)).
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
# plate_at x y: the sheet point of the plate's point (x, y) in its top view.
plate_at() { echo $((120 + 2 * $1)) $((100 + 2 * $2)); }
plate_click() { ui_sync; xdotool mousemove $(sheet_at $(plate_at "$1" "$2")) click 1; ui_sync; }
plate_hover() { ui_sync; xdotool mousemove $(sheet_at $(plate_at "$1" "$2")); ui_sync; }

# settled "logged place": waits until the place logged last for it stays
# the same over a sync and a short wait.
settled() {
  local before after
  for _ in $(seq 1 20); do
    before=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    ui_sync
    sleep 0.3
    after=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$after" ] && [ "$before" = "$after" ] && return 0
  done
  ui_fail "the place of '$1' did not settle"
}

no_refusal() {
  grep -q "Drawing command refused" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a drawing command was refused"
  return 0
}

# hover_is "text" "description": the newest "Drawing hover:" line starts so
# (its place is the pointer's; the marker's line has the point's own).
hover_is() {
  local last
  for _ in $(seq 1 30); do
    ui_wait_idle
    last=$(grep -F "Drawing hover: " "$UI_LOG" | tail -n 1)
    case "$last" in
      *"Drawing hover: $1"*) echo "ok   $2"; return 0 ;;
    esac
    sleep 0.1
  done
  ui_fail "$2: the newest hover line is '$last'"
}

# open_drawing: Drawing1 opened from the browser.
open_drawing() {
  ui_step "open Drawing1 (double-click)"   ui_double_click_logged "Browser Root/Drawings/Drawing1"
  ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
  ui_expect_new "Drawing sheet view " "the sheet placed"
}

close_drawing() {
  ui_mark
  settled "Drawing Close Drawing"
  ui_step "Close Drawing"                  ui_click_logged "Drawing Close Drawing"
  ui_expect_new "Drawing closed" "the 3D view again"
  ui_focus_main
}

# export_as pdf|svg file: Export… of the open drawing to the file.
export_as() {
  local keys
  [ "$1" = pdf ] && keys="Down Return" || keys="Down Down Return"
  ui_mark
  settled "Drawing Export"
  ui_step "Export…"                        ui_click_logged "Drawing Export"
  ui_step "${1^^}… ($keys)"                ui_key $keys
  ui_step "type the path, Enter"           ui_type_path "Export Drawing" "$2"
  ui_focus_main
  ui_expect_new "Exported $2: drawing Drawing1, 1 sheets" "the ${1^^} written" 30
}

# preferences keys... "logged line": Preferences > Display, the keys, OK.
preferences() {
  local logged=${*: -1}
  ui_mark
  ui_step "preferences, Display (search)"  ui_command "Preferences: Display"
  ui_focus_dialog '^Preferences$'
  for key in "${@:1:$#-1}"; do
    ui_step "key $key"                     ui_key "$key"
  done
  ui_step "OK (Enter)"                     ui_key Return
  ui_focus_main
  ui_expect_new "$logged" "${logged#Preferences: }"
}

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "plate.mitcad" "the design opened"

echo "--- The plate's drawing, Always show corners off (the default)"
ui_mark
open_drawing
ui_expect_new "Drawing sheet drawn: 1 views" "its top view"
grep -q "Drawing corner dots: [0-9]" "$UI_LOG" && ui_fail "corner dots while Always show corners is off"
echo "ok   no corner dots"

echo "--- 1. The corner marker and the centre line under the pointer"
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_step "point at the plate's corner"      plate_hover 90 0
hover_is "point at " "the corner under the pointer"
ui_expect_new "Drawing corner marker at 300.0,100.0, radius 6.0 px, filled" "a filled marker on the corner"
ui_step "point at the hole's centre line"  plate_hover 30 21
hover_is "center line at " "the hole's centre on its centre line"
ui_expect_new "Drawing corner marker at 180.0,150.0, radius 6.0 px, filled" "a filled marker on the hole's centre"
[ -n "$SHOT" ] && ui_capture "$WORK/marker.png"

echo "--- 2. A corner to the hole's centre line, in both orders"
ui_mark
ui_step "the plate's corner"               plate_click 90 0
ui_expect_new "Drawing picked point (1)" "the corner picked; a second pick waited for"
ui_step "the hole's centre line"           plate_click 30 21
ui_expect_new "Drawing picked center line (2)" "the centre line picked as the second pick"
ui_expect_new "Drawing placing view" "the dimension placed next"
ui_step "where it goes, above them"        plate_click 60 40
ui_expect_new "Drawing annotations drawn: 1 = 60" "a horizontal 60 from the corner to the centre line"
no_refusal
ui_mark
ui_step "the hole's centre line"           plate_click 30 21
ui_expect_new "Drawing picked center line (1)" "the centre line picked first"
ui_step "the plate's corner"               plate_click 90 0
ui_expect_new "Drawing picked point (2)" "the corner picked as the second pick"
ui_step "where it goes, beside them"       plate_click 100 10
ui_expect_new " | 2 = 25" "a vertical 25 from the centre line to the corner"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape

echo "--- 3. Exports without the dots"
export_as pdf "$WORK/off.pdf"
export_as svg "$WORK/off.svg"
close_drawing

echo "--- 4. Always show corners on, Large"
preferences alt+w alt+z Down "Preferences: always show corners in drawings on, dot size large"
grep -q '^alwaysShowCorners=true' "$CONF" || ui_fail "the setting is not saved on"
grep -q '^cornerDotSize=large' "$CONF" || ui_fail "the dot size is not saved"
echo "ok   kept in the settings"
ui_mark
open_drawing
ui_expect_new "Drawing corner dots: " "the corners as dots"
line=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -o "Drawing corner dots: .*" | tail -1)
count=$(sed -n 's/^Drawing corner dots: \([0-9]*\), radius 4.5 px$/\1/p' <<< "$line")
[ -n "$count" ] && [ "$count" -ge 4 ] || ui_fail "not the plate's corners as dots of 4.5 px: $line"
echo "ok   $line"
[ -n "$SHOT" ] && ui_capture "$SHOT"
export_as pdf "$WORK/on.pdf"
export_as svg "$WORK/on.svg"
cmp -s "$WORK/off.pdf" "$WORK/on.pdf" || ui_fail "the PDF changed with the corner dots on"
cmp -s "$WORK/off.svg" "$WORK/on.svg" || ui_fail "the SVG changed with the corner dots on"
echo "ok   the PDF and the SVG are the same: no dots in them"
close_drawing

echo "--- 5. Always show corners off"
preferences alt+w "Preferences: always show corners in drawings off, dot size large"
grep -q '^alwaysShowCorners=false' "$CONF" || ui_fail "the setting is not saved off"
ui_mark
open_drawing
ui_expect_new "Drawing corner dots: none" "no dots"
close_drawing

ui_finish "UI drawing corners test"
