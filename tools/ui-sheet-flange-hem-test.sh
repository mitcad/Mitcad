#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp core/model/src/sheet core/model/src/features/sheet_folded.rs tools/cli
# The Flange panel edits a flange's hem form and width (mitcad#1383): a
# flange edited from the timeline keeps its teardrop, rope or double hem
# (`hem`, mitcad#1378) and its width from the middle (`width`, mitcad#242),
# the panel changes them, and a bend past 180 degrees with a length of 0
# (a rolled hem) is taken.
# Design (mitcad-cli, as cli.sheet_flange_hem_past and
# cli.sheet_flange_overhang): t = 2, r = 3, K = 0.3, back reference; the
# 35 x 20 panel spans x = -30..5, the flange on its edge x = 5 at Tangent to
# Side. The volumes are those fixtures':
#   - teardrop, gap 2, length 25: 2510.920187986527;
#   - rope, the same: 2560.6299117462577;
#   - double, gap 2, setback 1, length 25, own bend radius 1:
#     3462.654824574367;
#   - no hem, 90 degrees, outer tangent length 15, width from the middle
#     12 and 12 past the 20 mm edge, overhang root 0: 1931.3274122871834;
#     root 3: 1907.3274122871834;
#   - rolled: 270 degrees, tangent length 0, the whole edge:
#     1953.9822368615503.
# Each form and the width are loaded by an edit and kept by its OK (the
# volume does not change), and the saved definitions are read back.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-flange-hem-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sheet-flange-hem.XXXXXX)
FILE=$WORK/hem.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":2,"bend_radius":3,"allowance":{"type":"k_factor","k":0.3},"reference_side":"back","bend_relief":{"type":"rectangular","width":2,"depth":3},"corner_relief":{"type":"round","width":2,"depth":3}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[-30,0],"width":35,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"$RECT"}]}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c2[c1,c3]"}},"angle":3.141592653589793,"length":{"mode":"outer_tangent","value":25},"position":"side_tangent","hem":{"form":"teardrop","gap":2}}},
  {"expect":{"feature":"F4","status":"ok","body":"F3.b0","volume":2510.920187986527}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

VOLUME='Body [A-Za-z0-9]* (F3.b0): volume [0-9.]* -> \([0-9.]*\) mm3'

# panel_input_shown input: the last layout of the Flange panel shows it.
panel_input_shown() {
  grep "Panel Flange input $1 at" "$UI_LOG" | tail -1 | grep -qv "(hidden)"
}

# edit_flange: opens the flange for editing from the timeline.
edit_flange() {
  ui_double_click_expect "Timeline SheetFlange1" "Editing F4 with Flange" "editing the flange"
  ui_expect_new "Panel Flange input hem at" "the panel is laid out"
  ui_sync
}

# ok_edited: OK, and the flange edited without an error.
ok_edited() {
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F4" "the flange edited" 30
  ui_sync
  grep -q "Recompute failed" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a recompute failed"
  grep -q "F4.*error" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the edited flange failed"
  return 0
}

# kept "description": OK on the panel as it loaded; the body keeps its
# volume (no volume line after the edit).
kept() {
  ok_edited
  grep -q "Body [A-Za-z0-9]* (F3.b0): volume" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
    ui_fail "$1: the body changed, so the panel did not keep the flange"
  echo "ok   $1"
}

# check "description" expectations...: the design saved and opened by
# mitcad-cli, with the script's expectations (commands.md, "Scripts").
check() {
  local description=$1
  shift
  ui_save_if_changed "$FILE"
  local IFS=,
  echo "[$*]" > "$WORK/check.json"
  "$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
    { cat "$WORK/check.log"; ui_fail "$description"; }
  echo "ok   $description"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline SheetFlange1 at" "the flange is in the timeline"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- The teardrop hem loads and is kept"
edit_flange
panel_input_shown hem_gap || ui_fail "Gap is hidden: the flange did not load as a hem"
panel_input_shown hem_setback && ui_fail "Setback is shown for a teardrop hem"
panel_input_shown angle && ui_fail "Angle is shown for a hem"
panel_input_shown length_mode && ui_fail "Measured is shown for a hem"
echo "ok   Hem: a form with its Gap, no Angle or Measured"
[ -n "$SHOT" ] && ui_capture "$SHOT"
kept "the teardrop hem kept"
check "the saved teardrop hem" \
  '{"expect":{"query":{"query":"feature","uid":"F4"},"result":{"status":"ok","def":{"hem":{"form":"teardrop"},"length":{"mode":"outer_tangent"},"position":"side_tangent"}}}}' \
  '{"expect":{"body":"F3.b0","volume":2510.920187986527}}' \
  '{"expect":{"query":{"query":"sheet_state","body":"F3.b0"},"result":{"status":"valid","bends":[{"signed_angle":3.34609747654756}]}}}'

echo "--- Hem: Rope"
edit_flange
ui_mark
ui_step "Hem: Rope"                        ui_choose "Panel Flange input hem" 1
ui_expect_new "Preview Flange: ok" "the rope hem previewed"
ok_edited
ui_expect_volume "$VOLUME" 2560.6299117462577 "a rope hem of gap 2"

echo "--- The rope hem loads and is kept"
edit_flange
panel_input_shown hem_gap || ui_fail "Gap is hidden: the rope hem did not load as a hem"
panel_input_shown hem_setback && ui_fail "Setback is shown for a rope hem"
kept "the rope hem kept"

echo "--- Hem: Double, with its own bend radius"
edit_flange
ui_step "Hem: Double"                      ui_choose "Panel Flange input hem" 1
ui_sync
panel_input_shown hem_setback || ui_fail "Setback is hidden for a double hem"
ui_step "Own Bend Radius"                  ui_click_logged "Panel Flange input custom_radius"
ui_sync
ui_mark
ui_step "Bend Radius 1"                    ui_type_in "Panel Flange input radius" "1"
ui_expect_new "Preview Flange: ok" "the double hem previewed"
ok_edited
ui_expect_volume "$VOLUME" 3462.654824574367 "a double hem of gap 2 and setback 1"

echo "--- The double hem loads and is kept"
edit_flange
panel_input_shown hem_setback || ui_fail "Setback is hidden: the double hem did not load as one"
panel_input_shown radius || ui_fail "Bend Radius is hidden: the own radius did not load"
kept "the double hem kept"
check "the saved double hem" \
  '{"expect":{"query":{"query":"feature","uid":"F4"},"result":{"status":"ok","def":{"hem":{"form":"double"},"length":{"mode":"outer_tangent"},"position":"side_tangent"}}}}' \
  '{"expect":{"body":"F3.b0","volume":3462.654824574367}}' \
  '{"expect":{"query":{"query":"sheet_state","body":"F3.b0"},"result":{"status":"valid","bends":[{"signed_angle":1.5707963267948966},{"signed_angle":1.5707963267948966},{"signed_angle":3.141592653589793}]}}}'

echo "--- No hem: 90 degrees, Width From the Middle past the edge's ends"
edit_flange
ui_step "Hem: None"                        ui_choose "Panel Flange input hem" -3
ui_sync
panel_input_shown angle || ui_fail "Angle is hidden without a hem"
panel_input_shown length_mode || ui_fail "Measured is hidden without a hem"
ui_step "Angle 90"                         ui_type_in "Panel Flange input angle" "90"
ui_step "Length 15"                        ui_type_in "Panel Flange input length" "15"
ui_step "Own Bend Radius off"              ui_click_logged "Panel Flange input custom_radius"
ui_step "Width: From the Middle"           ui_choose "Panel Flange input width_type" 2
ui_sync
panel_input_shown before || ui_fail "Before the Middle is hidden"
ui_step "Before the Middle 12"             ui_type_in "Panel Flange input before" "12"
ui_step "After the Middle 12"              ui_type_in "Panel Flange input after" "12"
ui_mark
ui_step "Own Overhang Root"                ui_click_logged "Panel Flange input custom_root"
ui_sync
ui_step "Overhang Root 0"                  ui_type_in "Panel Flange input root" "0"
ui_expect_new "Preview Flange: ok" "the wide flange previewed"
ok_edited
ui_expect_volume "$VOLUME" 1931.3274122871834 "a flange 2 mm past each end of its edge"

echo "--- The width loads and is kept"
edit_flange
panel_input_shown hem_gap && ui_fail "Gap is shown for a flange without a hem"
panel_input_shown before || ui_fail "Before the Middle is hidden: the width did not load"
panel_input_shown root || ui_fail "Overhang Root is hidden: the width's root did not load"
kept "the width kept"
check "the saved width" \
  '{"expect":{"query":{"query":"feature","uid":"F4"},"result":{"status":"ok","def":{"width":{"type":"middle"},"length":{"mode":"outer_tangent"},"position":"side_tangent"}}}}' \
  '{"expect":{"body":"F3.b0","volume":1931.3274122871834,"min":[-30,-2,-2],"max":[5,22,13]}}'

echo "--- Overhang Root 3"
edit_flange
ui_mark
ui_step "Overhang Root 3"                  ui_type_in "Panel Flange input root" "3"
ui_expect_new "Preview Flange: ok" "the root previewed"
ok_edited
ui_expect_volume "$VOLUME" 1907.3274122871834 "the overhangs cut back by 3"

echo "--- A rolled hem: 270 degrees, length 0, the whole edge"
edit_flange
ui_step "Angle 270"                        ui_type_in "Panel Flange input angle" "270"
ui_step "Measured: Tangent Length"         ui_choose "Panel Flange input length_mode" -3
ui_step "Length 0"                         ui_type_in "Panel Flange input length" "0"
ui_mark
ui_step "Width: Whole Edge"                ui_choose "Panel Flange input width_type" -2
ui_expect_new "Preview Flange: ok" "the rolled hem previewed"
ok_edited
ui_expect_volume "$VOLUME" 1953.9822368615503 "a bend of 270 degrees without a wall"

echo "--- The rolled hem loads and is kept"
edit_flange
panel_input_shown before && ui_fail "Before the Middle is shown for the whole edge"
kept "the rolled hem kept"
check "the saved rolled hem" \
  '{"expect":{"query":{"query":"feature","uid":"F4"},"result":{"status":"ok","def":{"length":{"mode":"tangent"},"position":"side_tangent"}}}}' \
  '{"expect":{"body":"F3.b0","volume":1953.9822368615503}}' \
  '{"expect":{"query":{"query":"sheet_state","body":"F3.b0"},"result":{"status":"valid","bends":[{"signed_angle":4.71238898038469}]}}}'

ui_finish "UI sheet flange hem test"
