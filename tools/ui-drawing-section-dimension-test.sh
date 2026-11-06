#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings core/model-api/src/drawing_annotations.rs
# check-all sources: core/model-drawing/src/annotate.rs core/model/src/drawing/annotate_def.rs tools/cli
# A section's cut edge dimensioned alone (mitcad#1361) through the real UI,
# on a part that mitcad-cli makes: a base 64 x 30 x 10 with a boss 32 wide
# and 20 high in its middle (x 16..48) and a hole of 4 through both at
# (32, 15); a drawing with its top view at 2:1 and a section through the
# hole along x (view 2, at 2:1 too), so that the bottom's cut is two
# pieces, x 0..30 and 34..64, either side of the hole:
#   1. Dimension: the bottom's cut edge beside the hole picked alone is
#      placed at once ("Drawing picked line (1)", no second pick) and reads
#      30, the piece under the click, not 64 across the hole.
#   2. The pointer on the hole's corner (30, 0) says what comes after a
#      corner ("Drawing hint: Click it, then a second edge ..."); picked,
#      it waits for a second pick and says so; the plate's far corner
#      (64, 0) then, placed below them: 34.
# The part is symmetric about x 32, so the clicks hold whichever way the
# section looks. tools/ui-windows-drawing-section-dimension-test.ps1 is the
# Windows counterpart.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-section-dimension-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-drawing-section-dimension.XXXXXX)
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
FILE=$WORK/stepped.mitcad

cat > "$WORK/stepped.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 64, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [16, 0], "width": 32, "height": 30},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F3", "region": "$R"}],
    "extent": {"type": "distance", "distance": 20}, "operation": "join"}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_circle", "sketch": "F5", "center": [32, 15], "diameter": 4},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F5", "region": "r{c1}"}],
    "extent": {"type": "through_all"}, "operation": "cut"}},
  {"expect": {"feature": "F6", "status": "ok"}},
  {"cmd": "add_drawing", "size": "A3"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "base", "direction": "top",
    "at": [150, 200], "scale": "2:1"},
  {"cmd": "add_drawing_view", "drawing": "D1", "type": "section", "parent": 1,
    "line": [[76, 200], [224, 200]], "flip": true, "at": [150, 100]}
]
EOF
"$CLI" run "$WORK/stepped.json" --save "$FILE" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# section_at x z: the screen position of the part's point (x, z) in the
# section, whose box (the part's cut, 64 x 20 at 2:1) has its middle at
# the logged "Drawing view 2 at x,y", along the logged "Drawing sheet view
# x0,y0 x1,y1 x2,y2" (sheet points (0, 0), (100, 0), (0, 100)).
section_at() {
  local view middle
  view=$(grep -o "Drawing sheet view [-0-9,]* [-0-9,]* [-0-9,]*" "$UI_LOG" | tail -1)
  [ -n "$view" ] || ui_fail "the app did not log the drawing's sheet"
  middle=$(grep -o "Drawing view 2 at -\?[0-9]*,-\?[0-9]*" "$UI_LOG" | tail -1 | sed 's/.* at //')
  [ -n "$middle" ] || ui_fail "the app did not log where the section is"
  awk -v view="$view" -v middle="$middle" -v x="$1" -v z="$2" -v wx="$X" -v wy="$Y" -v s="$UI_SCALE" 'BEGIN {
    split(view, part, " "); split(part[4], o, ","); split(part[5], a, ","); split(part[6], b, ",")
    split(middle, m, ",")
    px = m[1] + (a[1] - o[1]) / 100 * 2 * (x - 32)
    py = m[2] + (b[2] - o[2]) / 100 * 2 * (z - 10)
    printf "%d %d\n", wx + px * s + 0.5, wy + py * s + 0.5 }'
}
section_click() { ui_sync; xdotool mousemove $(section_at "$1" "$2") click 1; ui_sync; }
section_hover() { ui_sync; xdotool mousemove $(section_at "$1" "$2"); ui_sync; }

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

# hover_is "text" "description": the newest "Drawing hover:" line starts so.
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

ui_start_display
ui_start_app --open "$FILE" --no-native-dialogs
ui_expect_log "stepped.mitcad" "the design opened"

echo "--- The part's drawing: its top view and the section"
ui_mark
ui_step "open Drawing1 (double-click)"     ui_double_click_logged "Browser Root/Drawings/Drawing1"
ui_expect_new "Drawing opened: D1 Drawing1, sheet 1 of 1" "the drawing opened"
ui_expect_new "Drawing sheet drawn: 2 views" "its top view and the section"
ui_expect_new "Drawing view 2 at " "the section placed"
settled "Drawing view 2"

echo "--- 1. The cut edge beside the hole, alone"
ui_mark
ui_step "Dimension"                        ui_click_logged "Drawing tool Dimension"
ui_expect_new "Drawing tool: Dimension" "the dimension tool is on"
ui_step "point at the bottom's cut edge"   section_hover 15 0
hover_is "line at " "the cut edge under the pointer"
ui_step "the bottom's cut edge"            section_click 15 0
ui_expect_new "Drawing picked line (1)" "the cut edge picked"
ui_expect_new '"on_cut":' "the piece under the click"
ui_expect_new "Drawing placing view" "placed at once, alone"
ui_step "where it goes, below it"          section_click 15 -8
ui_expect_new "Drawing annotations drawn: 1 = 30" "the piece's length, 30, not 64 across the hole"
no_refusal
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- 2. A corner waits for a second pick"
ui_mark
ui_step "point at the hole's corner"       section_hover 30 0
hover_is "point at " "the hole's corner under the pointer"
ui_expect_new "Drawing hint: Click it, then a second edge, circle or corner" "what comes after a corner"
ui_step "the hole's corner"                section_click 30 0
ui_expect_new "Drawing picked point (1)" "the corner picked"
ui_expect_new "Drawing hint: Click a second edge, circle or corner to dimension it from" "a second pick waited for"
grep -q "Drawing placing view" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a corner alone was placed"
echo "ok   not placed alone"
ui_step "the part's far corner"            section_click 64 0
ui_expect_new "Drawing picked point (2)" "the second corner picked"
ui_expect_new "Drawing placing view" "the dimension placed next"
ui_step "where it goes, below them"        section_click 47 -16
ui_expect_new " | 2 = 34" "34 from the hole's corner to the far one"
no_refusal
ui_step "end the tool (Esc)"               ui_key Escape

ui_finish "UI drawing section dimension test"
