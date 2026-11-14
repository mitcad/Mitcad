#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CutTargetsPanel.hpp app/commands/ExtrudeCommands.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/Command.hpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/ModelMessages.cpp core/model/src/features/extrude_scope.rs core/model/src/joints.rs tools/cli
# Extrude Cut's Affected components with a part that moves through joints
# (mitcad#1789, GitHub mitcad/Mitcad#327), through the real UI: Part A is
# grounded and Part B turns on it through a revolute joint; a hole sketched
# in Part A goes through both blocks:
#   - the automatic scope leaves Part B out, but its row is enabled and
#     unchecked (`moves through joints`), with its reason under its name;
#   - checking it turns the scope explicit with Part A and Part B in one
#     step, and both blocks are cut;
#   - the body rows' tooltips name the body and its component, no ids.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-joint-targets-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-cut-joint.XXXXXX)
FILE=$WORK/joint-targets.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Part A (O1, C1): Box F1.b0, 20 x 20 x 10, grounded; Part B (O2, C2) 20 up:
# F2.b0 over it, joined to Part A by an as-built revolute joint. Sketch F3
# in Part A: a 4 x 4 hole across both blocks.
cat > "$WORK/model.json" << 'EOF'
[{"cmd": "create_component", "name": "Part A"},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "C0"},
 {"cmd": "create_component", "name": "Part B", "transform": {"translation": [0, 0, 20]}},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "C1"},
 {"cmd": "sketch.create"},
 {"cmd": "sketch.add_rectangle", "sketch": "F3", "corner": [8, 8], "width": 4, "height": 4},
 {"cmd": "activate_component", "component": "C0"},
 {"cmd": "ground_occurrence", "occurrence": "O1"},
 {"cmd": "add_as_built_joint", "kind": "revolute", "a": "O2", "b": "O1"},
 {"cmd": "activate_component", "component": "C1"}]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# expect_scope 'jq condition' description: on the saved cut's cut_scope.
expect_scope() {
  jq -e "[.features[] | select(.cut_scope != null)] | length == 1 and (.[0].cut_scope | $1)" "$FILE" > /dev/null ||
    ui_fail "$2: $(jq -c '[.features[] | select(.cut_scope != null) | .cut_scope]' "$FILE")"
  echo "ok   $2"
}

ui_start_display
ui_start_app --open "$FILE"
# High enough for the whole Extrude panel with its list, unscrolled.
ui_step "a high window"                  xdotool windowsize "$UI_WINDOW" 1400 990
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the design is shown"
ui_expect_log "Opened with C1 active, as saved (unmodified)" "Part A is active"

echo "--- A new cut through Part A and Part B, which turns on it"
ui_mark
ui_step "Extrude (search)"               ui_command "Extrude"
ui_expect_new "Command Extrude started" "Extrude started"
ui_expect_new "Extrude Profiles: 1 profile" "the hole taken"
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
ui_expect_log "Cut target C1@O1: eligible, checked, intersects" "Part A checked"
ui_expect_log "Cut target C2@O2: moves through joints, unchecked, intersects" \
  "Part B left out for its joint, enabled and unchecked"
ui_expect_log "Cut target row C2@O2 at " "Part B's row laid out"
ui_expect_log "Cut body F1.b0@O1 tooltip: " "Part A's body row"
ui_expect_log "Cut body F2.b0@O2 tooltip: " "Part B's body row"

echo "--- Part B checked: the scope turns explicit with both"
ui_mark
ui_step "select the row C2@O2"           ui_click_logged "Cut target row C2@O2"
ui_step "Space"                          ui_key space
ui_expect_new "Cut target C2@O2 checked: explicit scope with it" "the scope turned explicit"
ui_expect_new "Cut target C2@O2: eligible, checked, intersects" "Part B checked"
ui_expect_new "Cut target C1@O1: eligible, checked, intersects" "Part A still checked"
ui_expect_new "Preview Extrude: ok" "previewed with both"

# The body rows' tooltips in words: the body's name and its component's
# path, no body, feature, component or occurrence ids.
tips=$(sed -n 's/^.*Cut body [^ ]* tooltip: //p' "$UI_LOG")
[ -n "$tips" ] || ui_fail "no body tooltips logged"
if grep -E '[A-Z][0-9]+\.b[0-9]+|\b[FCO][0-9]+\b' <<< "$tips"; then
  ui_fail "a body tooltip shows an id"
fi
grep -qF "Body1 in Root / Part A:1" <<< "$tips" || ui_fail "Part A's body tooltip: $tips"
echo "ok   the body tooltips name the body and its component, no ids"

echo "--- Committed: both blocks cut"
ui_mark
ui_step "OK"                           ui_click_logged "Panel Extrude OK"
ui_expect_new "Added extrude (Cut)" "the cut added" 30
# The 4 x 4 hole through each 10 mm block: 160 mm3 out of 4000.
ui_expect_volume "(F1.b0): volume 4000.000 -> \([0-9.]*\) mm3" 3840 "Part A's block cut"
ui_expect_volume "(F2.b0): volume 4000.000 -> \([0-9.]*\) mm3" 3840 "Part B's block cut"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_scope '.targets.type == "explicit"' "an explicit scope"
expect_scope '.targets.included == [{"component": "C1", "occurrence": "O1"}, {"component": "C2", "occurrence": "O2"}]' \
  "Part A and Part B included"
ui_crashed && ui_fail "the application crashed"

ui_finish "UI cut joint targets test"
