#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/commands/SketchCommands.cpp app/MainWindow.cpp app/framework/Command.cpp
# check-all sources: core/model-api/src/sketch.rs core/sketch/src/text.rs core/sketch/src/code.rs tools/cli
# Edit Text from inside a letter (mitcad#1369, GitHub mitcad/Mitcad#158)
# through the real UI. Zoomed in on a sketch text, a click inside a letter
# picks a profile (the region), not the outline: its context menu and a
# double-click there offer and open the text's Edit Text all the same, on
# that text, inside the letter I's stroke and inside the o's counter (its
# hole); a QR code's dark centre and the light ring round it (its finder
# pattern's) do so with Edit QR Code.
#
# Sketch1 (F1), made with mitcad-cli: "Io" in the bundled Droid Sans, 20 mm
# high from (0, 0) without kerning, and a QR code of "https://mitcad.org"
# with 2 mm modules from (30, 0). The points come from the font's outlines
# (2048 units an em, the em the text's height): the I's stem x 254..440,
# y 143..1319, its centre (3.389, 7.139); the o from its pen at the I's
# advance 694 (6.777 mm), its stroke x 113..301 at y 551, the middle
# (8.799, 5.381), and its counter x 301..881, y 131..967, the middle
# (12.549, 5.361). The code's lower left finder pattern: its dark centre
# (3 x 3 modules from module (2, 2)) at (37, 7), its light ring (one module
# wide round it, a region with the centre as its hole) at (33, 7), a module
# from the outlines on either side. Zoomed in further there (four wheel
# steps): with 1 mm modules and two steps, the outlines' pick margin covered
# the light ring's middle, and a click there picked the code's outline.
#
# Clicks are given in sketch millimetres (ui_sketch_click), from the
# placement the app logs.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-text-region-edit-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-text-region-edit.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

cat > "$WORK/sketch.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_text", "sketch": "F1", "text": "Io", "at": [0, 0], "height": 20, "no_kerning": true},
  {"cmd": "sketch.add_code", "sketch": "F1", "text": "https://mitcad.org", "at": [30, 0], "module": 2}
]
EOF
"$CLI" run "$WORK/sketch.json" --save "$WORK/text.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli sketch"; }

# in_view x y: the sketch point (mm) is in the 3D view, clear of its edges.
in_view() {
  local area vx vy vw vh px py
  area=$(grep -o "View area [0-9]* [0-9]* [0-9]* [0-9]*" "$UI_LOG" | tail -1)
  read -r _ _ vx vy vw vh <<< "$area"
  read -r px py <<< "$(ui_sketch_at "$1" "$2")"
  [ "$px" -gt $((X + vx + 20)) ] && [ "$px" -lt $((X + vx + vw - 20)) ] &&
    [ "$py" -gt $((Y + vy + 20)) ] && [ "$py" -lt $((Y + vy + vh - 20)) ] ||
    ui_fail "the sketch point ($1, $2) is not in the view"
}

# zoom_in_at x y steps: a fit, then wheel steps in about the sketch point
# (mm), which stays where it is.
zoom_in_at() {
  local i
  ui_step "fit (F6)"                       ui_key F6
  in_view "$1" "$2"
  ui_mark
  xdotool mousemove $(ui_sketch_at "$1" "$2")
  for i in $(seq 1 "$3"); do
    ui_step "zoom in at ($1, $2), step $i" xdotool click 4
  done
  ui_expect_new "Sketch view " "the view zoomed in"
  ui_sync
  in_view "$1" "$2"
}

# right_click_region x y command input: a right-click at a sketch point
# (mm) selects the region there, not an outline; the context menu offers
# the command, which opens on the region's text (its input holds the text's
# curve). Cancelled. Sets REGION and PICKED.
right_click_region() {
  local x=$1 y=$2 command=$3 input=$4
  ui_mark
  ui_step "right-click at ($x, $y)"        ui_sketch_click "$x" "$y" 3
  ui_expect_new "Context menu: " "the context menu at ($x, $y)"
  REGION=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n 's/.*Selected: 1 profile \[profile \(.*\) of F1\].*/\1/p' | tail -1)
  [ -n "$REGION" ] || ui_fail "the right-click at ($x, $y) selected no profile of F1"
  echo "ok   the right-click selected the region $REGION"
  ui_step "choose $command"                ui_menu_choose "$command"
  ui_expect_new "Panel $command input " "the $command panel"
  ui_expect_new "$command $input: 1 sketch curve [sketch curve " "on a text"
  PICKED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
    sed -n "s/.*$command $input: 1 sketch curve \[sketch curve \(t[0-9]*\) of F1\].*/\1/p" | tail -1)
  [ -n "$PICKED" ] || ui_fail "$command did not take a text for the region $REGION"
  echo "ok   $command opened on $PICKED"
  ui_mark
  ui_step "cancel (Esc)"                   ui_key Escape
  ui_expect_new "Command $command cancelled" "$command cancelled"
}

# double_click_region x y text command: a double-click at a sketch point
# (mm) opens the command on the region's text; under load the two clicks may
# come as two single clicks, so it is tried once more (as
# ui_double_click_expect). Cancelled.
double_click_region() {
  local attempt at
  for attempt in 1 2; do
    ui_mark
    ui_wait_idle
    at=$(ui_sketch_at "$1" "$2")
    ui_apart $at
    xdotool mousemove $at click --repeat 2 --delay 90 1
    ui_sync
    ui_crashed && ui_fail "crashed during the double-click at ($1, $2)"
    for _ in $(seq 1 15); do
      if grep -qF -- "Double-click on $3: $4" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG"); then
        echo "ok   the double-click at ($1, $2) opened $4 on $3"
        ui_expect_new "Panel " "its panel"
        ui_mark
        ui_step "cancel (Esc)"             ui_key Escape
        ui_expect_new " cancelled" "cancelled"
        return
      fi
      sleep 0.2
    done
    echo "note: no 'Double-click on $3: $4' after the double-click at ($1, $2) (attempt $attempt)"
    sleep 0.45 # not a double-click with the clicks before
  done
  ui_fail "the double-click at ($1, $2) did not open $4 on $3"
}

ui_start_display
ui_start_app --open "$WORK/text.mitcad"

ui_step "double-click Sketch1"             ui_double_click_logged "Timeline Sketch1"
ui_expect_log "Editing sketch F1 (Sketch1)" "the sketch edited"

echo "--- The text: inside the I's stroke and the o's stroke and counter"
zoom_in_at 8 8 3
right_click_region 3.389 7.139 "Edit Text" "Text"
TEXT=$PICKED
I_REGION=$REGION
right_click_region 12.549 5.361 "Edit Text" "Text"
[ "$PICKED" = "$TEXT" ] || ui_fail "the o's counter gave $PICKED, not the text $TEXT"
COUNTER=$REGION
[ "$COUNTER" != "$I_REGION" ] || ui_fail "the I and the o's counter are one region ($COUNTER)"
right_click_region 8.799 5.381 "Edit Text" "Text"
[ "$PICKED" = "$TEXT" ] || ui_fail "the o's stroke gave $PICKED, not the text $TEXT"
[ "$REGION" != "$COUNTER" ] || ui_fail "the o's stroke and counter are one region ($REGION)"
double_click_region 12.549 5.361 "$TEXT" sketch.edit_text
double_click_region 3.389 7.139 "$TEXT" sketch.edit_text

echo "--- Edit Text from the counter changes that text"
ui_mark
ui_step "right-click the counter"          ui_sketch_click 12.549 5.361 3
ui_expect_new "Context menu: " "the context menu"
ui_step "choose Edit Text"                 ui_menu_choose "Edit Text"
ui_expect_new "Edit Text Text: 1 sketch curve [sketch curve $TEXT of F1]" "Edit Text on $TEXT"
ui_step "height 21"                        ui_type_in "Panel Edit Text input height" "21"
ui_expect_log "Preview Edit Text: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Edited text $TEXT: \"Io\", Droid Sans, left" "the text edited"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ui_mark
ui_step "undo (Ctrl+Z)"                    ui_key ctrl+z
ui_expect_new "Undo: " "the edit undone"

echo "--- The QR code: its finder pattern's dark centre and the light ring round it"
zoom_in_at 35 7 4
right_click_region 37 7 "Edit QR Code" "QR Code"
CODE=$PICKED
[ "$CODE" != "$TEXT" ] || ui_fail "the code's region gave the text $TEXT"
right_click_region 33 7 "Edit QR Code" "QR Code"
[ "$PICKED" = "$CODE" ] || ui_fail "the light ring gave $PICKED, not the code $CODE"
MENU=$(grep "Context menu: " "$UI_LOG" | tail -1)
[[ $MENU == *"Edit Text"* ]] && ui_fail "a code's region offers Edit Text: $MENU"
echo "ok   a code's region offers no Edit Text"
double_click_region 37 7 "$CODE" sketch.edit_code
double_click_region 33 7 "$CODE" sketch.edit_code

ui_step "finish sketch (Ctrl+Enter)"       ui_key ctrl+Return
ui_expect_log "Sketch finished" "sketch finished"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI text region edit test"
