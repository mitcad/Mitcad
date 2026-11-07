#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetCommands.cpp core/model/src/sheet core/model/src/features/sheet_folded.rs tools/cli
# The Flange panel's Corner (mitcad#1410): a 1 mm sheet, a 100 x 10 base
# with Outside flanges of Inner Length 10 on both long edges and a third
# closing the 10 x 10 section with an open corner (mitcad-cli, as
# cli.sheet_flange_corner):
#   - the closing flange edited from the timeline loads its Corner: Open
#     hides Measured (a corner's length is the inside dimension) and Hem;
#     OK keeps the body, 3400 + 225 pi mm3;
#   - Corner: Half Open: the wall 0.5 mm over the first wall's end, which
#     the 0.00001 mm seam cuts back: 4156.857347057704 mm3;
#   - saved and read back: `corner` half_open, length mode inner;
#   - Corner: None shows Measured and Hem again; Esc leaves the flange.
# The Windows twin is tools/ui-windows-sheet-corner-test.ps1.
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sheet-flange-corner-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-sheet-flange-corner.XXXXXX)
FILE=$WORK/corner.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":1,"bend_radius":1,"allowance":{"type":"k_factor","k":0.44},"reference_side":"front","bend_relief":{"type":"round","width":1,"depth":2},"corner_relief":{"type":"round","width":1,"depth":2}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[0,0],"width":100,"height":10},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"$RECT"}]}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c1[c4,c2]"}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside"}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c3[c2,c4]"}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside"}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F5","slot":0},"curve":{"type":"generated","feature":"F5","slot":51}},"angle":1.5707963267948966,"length":{"mode":"inner","value":10},"position":"outside","corner":"open"}},
  {"expect":{"feature":"F6","status":"ok","body":"F3.b0","volume":4106.8583470577035}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

VOLUME='Body [A-Za-z0-9]* (F3.b0): volume [0-9.]* -> \([0-9.]*\) mm3'

# panel_input_shown input: the last layout of the Flange panel shows it.
panel_input_shown() {
  grep "Panel Flange input $1 at" "$UI_LOG" | tail -1 | grep -qv "(hidden)"
}

# edit_closing: opens the closing flange for editing from the timeline.
edit_closing() {
  ui_double_click_expect "Timeline SheetFlange3" "Editing F6 with Flange" "editing the closing flange"
  ui_expect_new "Panel Flange input corner at" "the panel is laid out"
  ui_sync
}

# ok_edited: OK, and the flange edited without an error.
ok_edited() {
  ui_mark
  ui_step "OK (Enter)"                     ui_key Return
  ui_expect_new "Edited F6" "the flange edited" 30
  ui_sync
  grep -q "Recompute failed" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "a recompute failed"
  grep -q "F6.*error" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") && ui_fail "the edited flange failed"
  return 0
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                         ui_key F6
ui_expect_log "Timeline SheetFlange3 at" "the closing flange is in the timeline"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- The open corner loads and is kept"
edit_closing
panel_input_shown corner || ui_fail "Corner is hidden"
panel_input_shown length_mode && ui_fail "Measured is shown: the flange did not load its corner"
panel_input_shown hem && ui_fail "Hem is shown beside a corner"
echo "ok   Corner: Open loaded, Measured and Hem hidden"
[ -n "$SHOT" ] && ui_capture "$SHOT"
ok_edited
grep -q "Body [A-Za-z0-9]* (F3.b0): volume" < <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the body changed, so the panel did not keep the open corner"
echo "ok   the open corner kept"

echo "--- Corner: Half Open"
edit_closing
ui_mark
ui_step "Corner: Half Open"                ui_choose "Panel Flange input corner" 1
ui_expect_new "Preview Flange: ok" "the half open corner previewed" 30
ok_edited
ui_expect_volume "$VOLUME" 4156.857347057704 "the closing wall 0.5 mm over the first wall, the seam cut back"

echo "--- The saved corner read back"
ui_save_if_changed "$FILE"
cat > "$WORK/check.json" << 'EOF'
[
  {"expect":{"query":{"query":"feature","uid":"F6"},"result":{"status":"ok","def":{"corner":"half_open","length":{"mode":"inner"},"position":"outside"}}}},
  {"expect":{"body":"F3.b0","volume":4156.857347057704}},
  {"expect":{"query":{"query":"sheet_state","body":"F3.b0"},"result":{"status":"valid"}}}
]
EOF
"$CLI" run "$WORK/check.json" --open "$FILE" > "$WORK/check.log" 2>&1 ||
  { cat "$WORK/check.log"; ui_fail "the saved half open corner"; }
echo "ok   the saved half open corner"

echo "--- The half open corner loads; Corner: None shows Measured"
edit_closing
panel_input_shown length_mode && ui_fail "Measured is shown: the half open corner did not load"
ui_step "Corner: None"                     ui_choose "Panel Flange input corner" -2
ui_sync
panel_input_shown length_mode || ui_fail "Measured is hidden without a corner"
panel_input_shown hem || ui_fail "Hem is hidden without a corner"
echo "ok   Corner: None shows Measured and Hem"
ui_mark
ui_step "cancel (Esc)"                     ui_key Escape
ui_expect_new "Command Flange cancelled" "the edit cancelled"

ui_finish "UI sheet flange corner test"
