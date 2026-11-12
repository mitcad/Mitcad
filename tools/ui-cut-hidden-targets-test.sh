#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CutTargetsPanel.hpp app/commands/ExtrudeCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/Command.hpp app/framework/CommandPanel.cpp
# check-all sources: core/model/src/features/extrude_scope.rs tools/cli
# Extrude Cut's Affected components start unchecked for what the view hides
# (mitcad#475), through the real UI: a new cut from Part A's sketch goes
# through Part A's two blocks and Part B's block above them, with Part B's
# light bulb off and Part A's second block hidden:
#   - Part B starts unchecked, marked "(hidden)", and can be checked and
#     unchecked again; Part A starts partial (its hidden block unchecked);
#   - the committed feature's cut_scope lists Part B in `excluded` and the
#     hidden block in `excluded_bodies`; the shown block was cut;
#   - Part A hidden afterwards, editing the cut keeps its checks as stored.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-hidden-targets-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-cut-hidden.XXXXXX)
FILE=$WORK/hidden-targets.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Part A (O1, C1): Box F1.b0 (0..20 in x) and F2.b0 (30..50), 10 high;
# Part B (O2, C2) 20 up: F3.b0 over the first. Sketch F4 in Part A: a strip
# across both of Part A's blocks. Part B's light bulb off, F2.b0 hidden.
cat > "$WORK/model.json" << 'EOF'
[{"cmd": "create_component", "name": "Part A"},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [30, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "C0"},
 {"cmd": "create_component", "name": "Part B", "transform": {"translation": [0, 0, 20]}},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "C1"},
 {"cmd": "sketch.create"},
 {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [4, 4], "width": 40, "height": 4},
 {"cmd": "set_occurrence_visible", "occurrence": "O2", "visible": false},
 {"cmd": "set_body_visible", "uid": "F2.b0", "visible": false}]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_scope 'jq condition' description: on the saved cut's cut_scope.
expect_scope() {
  jq -e "[.features[] | select(.cut_scope != null)] | length == 1 and (.[0].cut_scope | $1)" "$FILE" > /dev/null ||
    ui_fail "$2: $(jq -c '[.features[] | select(.cut_scope != null) | .cut_scope]' "$FILE")"
  echo "ok   $2"
}

# expect_none "text" description: not logged since the mark.
expect_none() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# toggle_row target: selects a row of Affected components and checks or
# unchecks it with Space.
toggle_row() {
  ui_step "select the row $1"              ui_click_logged "Cut target row $1"
  ui_step "Space"                          ui_key space
}

ui_start_display
ui_start_app --open "$FILE"
# High enough for the whole Extrude panel with its list, unscrolled.
ui_step "a high window"                  xdotool windowsize "$UI_WINDOW" 1400 990
eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the design is shown"
# A design opens with the component it was saved with active (mitcad#1678):
# Part A, whose cut this is.
ui_expect_log "Opened with C1 active, as saved (unmodified)" "Part A is active"

echo "--- A new cut through a shown and a hidden component"
ui_mark
ui_step "Extrude (search)"               ui_command "Extrude"
ui_expect_new "Command Extrude started" "Extrude started"
ui_expect_new "Extrude Profiles: 1 profile" "the strip taken"
ui_mark
ui_step "distance 50"                    ui_type_in "Panel Extrude input distance" "50"
ui_expect_new "Preview Extrude: " "previewed"
operation=$(grep -o "Extrude: \(automatic \)\?Operation = [A-Za-z ]*" "$UI_LOG" | tail -1 | sed 's/.*= //')
case ${operation:-New Body} in
"New Body") downs=2 ;;
Join) downs=1 ;;
Cut) downs=0 ;;
*) ui_fail "unexpected operation '$operation'" ;;
esac
if [ "$downs" -gt 0 ]; then
  ui_mark
  ui_step "Cut"                          ui_choose "Panel Extrude input operation" "$downs"
  ui_expect_new "Extrude: Operation = Cut" "Cut chosen"
fi
ui_expect_log "Extrude: hidden targets start unchecked: " "the hidden targets were taken out"
ui_expect_log "Cut target C2@O2: eligible, unchecked, intersects, hidden" "Part B starts unchecked, marked hidden"
ui_expect_log "Cut target C1@O1: eligible, partial, intersects" "Part A partial: its hidden block unchecked"
ui_expect_log "Cut target row C2@O2 at " "Part B's row laid out"

echo "--- Part B can be checked"
ui_mark
toggle_row C2@O2
ui_expect_new "Cut target C2@O2: eligible, checked, intersects, hidden" "Part B checked by hand"
ui_expect_new "Preview Extrude: ok" "previewed with it"
ui_sync
grep "Cut target C2@O2: " "$UI_LOG" | tail -1 | grep -qF "checked, intersects" ||
  ui_fail "Part B did not stay checked: $(grep "Cut target C2@O2: " "$UI_LOG" | tail -1)"
echo "ok   Part B stays checked (not taken out again)"
ui_mark
toggle_row C2@O2
ui_expect_new "Cut target C2@O2: eligible, unchecked, intersects, hidden" "and unchecked again"
ui_expect_new "Preview Extrude: ok" "previewed without it"

echo "--- Committed"
ui_mark
# The list keeps the keys (Enter): the OK button, where the panel showed it
# (a window high enough that it did not scroll for the list).
[ "$(grep "Cut target row C2@O2 at " "$UI_LOG" | head -1 | sed 's/.* at //')" = \
  "$(grep "Cut target row C2@O2 at " "$UI_LOG" | tail -1 | sed 's/.* at //')" ] ||
  ui_fail "the panel scrolled: its logged places are stale"
ui_step "OK"                             ui_click_logged "Panel Extrude OK"
ui_expect_new "Added extrude (Cut)" "the cut added" 30
# The strip over the shown block, 16 x 4 x 10 mm, went; the others stay.
ui_expect_volume "(F1.b0): volume 4000.000 -> \([0-9.]*\) mm3" 3360 "the shown block cut"
expect_none "(F2.b0): volume" "the hidden block not cut"
expect_none "(F3.b0): volume" "Part B's block not cut"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_scope '.targets.type == "automatic"' "an automatic scope"
expect_scope '.targets.excluded == [{"component": "C2", "occurrence": "O2"}]' "Part B in excluded"
expect_scope '.targets.excluded_bodies == [{"component": "C1", "occurrence": "O1", "body": "F2.b0"}]' \
  "the hidden block in excluded_bodies"

echo "--- Part A hidden afterwards: editing keeps the checks"
ui_mark
ui_step "hide Part A"                    ui_click_logged "Browser eye Root/Part A:1"
ui_expect_new "Visibility Root/Part A:1: hidden" "Part A hidden"
ui_mark
ui_step "edit the cut"                   ui_double_click_logged "Timeline Extrude1"
ui_expect_new "Cut target C1@O1: eligible, partial, intersects" "Part A still partial (as stored)"
ui_expect_new "Cut target C2@O2: eligible, unchecked, intersects" "Part B still unchecked"
ui_sync
expect_none "Extrude: hidden targets start unchecked" "the edit changed no check"
expect_none "Cut target C1@O1: eligible, unchecked" "Part A not taken out"
ui_step "cancel (Esc)"                   ui_key Escape
ui_crashed && ui_fail "the application crashed"

ui_finish "UI cut hidden targets test"
