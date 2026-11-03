#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/ParameterPicker.cpp app/framework/ParameterPicker.hpp
# check-all sources: app/framework/CommandSessionPicks.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandPresenter.hpp
# check-all sources: app/framework/Command.hpp app/MainWindowParameterPicks.cpp
# check-all sources: app/sketch/SketchOverlay.cpp app/sketch/SketchModel.cpp
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/FilletCommands.cpp app/commands/CommandSupport.cpp tools/cli
# check-all sources: app/view/ViewController.cpp
# Dimension names in feature panels (mitcad#250), through the real UI:
#   - A design (mitcad-cli): a 40 x 25 rectangle, its width k5 (d1) and
#     height k6 (d2) dimensioned, its left side measured by the driven k7
#     (its reference d3); Extrude1 10 (d4, F2) of it, its corner at (40, 0)
#     rounded by Fillet1 (r 3, d5, F3), and Extrude2 -4 (d6, F4) of it as a
#     second body; the sketch's light bulb on.
#   - Edit Extrude1: the distance field's tooltip says what d4 is, as a
#     sketch dimension's does.
#   - Edit Extrude2: its tooltip; no labels while the distance field has
#     the keyboard only because the panel opened (mitcad#587); a click into
#     the field labels the shown sketch's dimensions in the view, and only
#     those (not Extrude1's or Fillet1's values); Right View, Front View
#     and both again by keys, the field keeping the keyboard: the
#     labels stay, and the width's is drawn where each view puts it once
#     the camera rests (mitcad#1179); a click on the width's
#     label inserts d1 at the field's cursor, d4 and d5 are typed by name;
#     the tooltip follows, and OK keeps 2*d1+d4+d5.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-dimension-names-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-dimension-names.XXXXXX)
FILE=$WORK/names.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 40, "height": 25},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k5", "text": [20, -10]},
  {"cmd": "sketch.set_dimension_text", "sketch": "F1", "dimension": "k6", "text": [50, 12]},
  {"cmd": "sketch.add_dimension", "sketch": "F1", "dimension": {"type": "length", "line": "c4"},
   "driven": true, "text": [-10, 12]},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "fillet", "body": "F2.b0",
    "edges": ["E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"], "radius": 3}},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$RECT"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_body"}},
  {"cmd": "set_feature_visible", "uid": "F1", "visible": true}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# Right View and Front View by keys of our own, as Tools > Keyboard
# Shortcuts saves them: they turn the view while the field keeps the
# keyboard (mitcad#1179).
mkdir -p "$XDG_CONFIG_HOME/Mitcad"
printf '[shortcuts]\nview.right=F7\nview.front=F8\n' > "$XDG_CONFIG_HOME/Mitcad/Mitcad.conf"

# label_at "<label>" [drawn]: the last place logged since the mark for the
# label ("Parameter label <label> at x,y"), or where a paint drew it
# ("... drawn at x,y").
label_at() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "Parameter label $1 ${2:+$2 }at " | tail -1 | sed 's/.* at //'
}

# expect_label_follows "<label>" "<description>": once the camera rests, the
# label has a new place since the mark, and the last paint drew it there;
# the place in LABEL_AT.
expect_label_follows() {
  local drawn i
  for i in $(seq 1 100); do
    LABEL_AT=$(label_at "$1")
    drawn=$(label_at "$1" drawn)
    [ -n "$LABEL_AT" ] && [ "$LABEL_AT" = "$drawn" ] && break
    sleep 0.05
  done
  [ -n "$LABEL_AT" ] || ui_fail "$2: the label's place did not change as the camera turned"
  [ "$LABEL_AT" = "$drawn" ] ||
    ui_fail "$2: the label was drawn at ${drawn:-its old place}, the camera puts it at $LABEL_AT"
  grep -q "Parameter labels for Extrude hidden" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
    ui_fail "$2: the labels were hidden"
  echo "ok   $2 (at $LABEL_AT)"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Timeline Extrude2 at" "the timeline is logged"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- A feature panel field's tooltip (mitcad#250)"
ui_mark
ui_step "edit Extrude1 (double-click)"   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Editing F2 with Extrude" "Extrude1 opens for editing" 30
ui_expect_new "Panel Extrude label: Distance (d4)" "the distance field names d4"
ui_expect_new "Panel Extrude tooltip distance: d4 = 10 mm (Extrude1 distance)" "its tooltip says what d4 is"
ui_step "cancel (Esc)"                   ui_key Escape

echo "--- Names picked from the view into the field (mitcad#250, mitcad#587)"
ui_mark
ui_step "edit Extrude2 (double-click)"   ui_double_click_logged "Timeline Extrude2"
ui_expect_new "Editing F4 with Extrude" "Extrude2 opens for editing" 30
ui_expect_new "Panel Extrude tooltip distance: d6 = -4" "its tooltip says what d6 is"
ui_expect_new "(Extrude2 distance)" "with its comment"
# The panel gives the first value's field the keyboard as it opens: no
# labels until the field is clicked or typed in (mitcad#587).
grep -q "Parameter labels for Extrude:" <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the view showed parameter labels as the panel opened"
echo "ok   no labels as the panel opened"
ui_mark
ui_step "click into the distance field"  ui_click_logged "Panel Extrude input distance"
ui_expect_new "Parameter labels for Extrude: Sketch1 k5 d1" "the click showed the names to pick"
ui_expect_new "Parameter label Sketch1 k5 d1 at" "the shown sketch's width is labelled"
ui_expect_new "Parameter label Sketch1 k7 d3 at" "and its driven dimension, by its reference"
grep -qE "Parameter label (Extrude1|Fillet1) " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "an earlier feature's value was labelled (only sketch dimensions, mitcad#587)"
echo "ok   only the sketch's dimensions are labelled"
# The labels follow the camera as it turns (mitcad#1179): Right View and
# Front View by their keys, the field keeping the keyboard and the labels;
# each time the width's label has a new place, drawn where the camera puts
# it once the camera rests, and the same place for the same view.
start=$(label_at "Sketch1 k5 d1")
ui_mark
ui_step "Right View (F7)"                ui_key F7
ui_expect_new "Camera direction -1 0 0 up 0 0 1" "the view turned to the right" 10
expect_label_follows "Sketch1 k5 d1" "the width's label follows to the right view"
right=$LABEL_AT
[ "$right" != "$start" ] || ui_fail "the width's label stayed at $start"
ui_mark
ui_step "Front View (F8)"                ui_key F8
ui_expect_new "Camera direction 0 1 0 up 0 0 1" "the view turned to the front" 10
expect_label_follows "Sketch1 k5 d1" "the width's label follows to the front view"
front=$LABEL_AT
[ "$front" != "$right" ] || ui_fail "the width's label stayed at $right"
ui_mark
ui_step "Right View again (F7)"          ui_key F7
ui_expect_new "Camera direction -1 0 0 up 0 0 1" "the view turned to the right again" 10
expect_label_follows "Sketch1 k5 d1" "the width's label follows back to the right view"
[ "$LABEL_AT" = "$right" ] || ui_fail "the width's label is at $LABEL_AT in the right view, before at $right"
# Front View again, where the width's label is well inside the view for
# the click on it below.
ui_mark
ui_step "Front View again (F8)"          ui_key F8
ui_expect_new "Camera direction 0 1 0 up 0 0 1" "the view turned to the front again" 10
expect_label_follows "Sketch1 k5 d1" "the width's label follows back to the front view"
[ "$LABEL_AT" = "$front" ] || ui_fail "the width's label is at $LABEL_AT in the front view, before at $front"
echo "ok   the labels follow the camera"
ui_step "type 2* into the distance"      ui_type_in "Panel Extrude input distance" "2*"
ui_mark
ui_step "click the width's label"        ui_click_logged "Parameter label Sketch1 k5 d1"
ui_expect_new "Parameter label picked: d1 (Sketch1 k5)" "the width's label picked"
ui_expect_new "Panel Extrude input distance: inserted d1, now 2*d1" "d1 inserted at the cursor"
ui_expect_new "Extrude Distance: 2*d1 = 80" "the distance follows the width"
ui_mark
# The features' values are used by their names, typed.
ui_step "type +d4+d5"                    xdotool type --delay 40 "+d4+d5"
ui_expect_new "Extrude Distance: 2*d1+d4+d5 = 93" "the distance follows all three"
ui_expect_new "Panel Extrude tooltip distance: d6 = 2*d1+d4+d5" "the tooltip shows the expression"
ui_expect_new "93 mm (Extrude2 distance)" "with its value and comment"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Edited F4" "Extrude2 edited" 30

ui_finish "UI dimension names test"
