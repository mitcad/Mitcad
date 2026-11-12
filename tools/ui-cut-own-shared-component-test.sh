#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CutTargetsPanel.hpp app/commands/ExtrudeCommands.cpp
# check-all sources: core/model/src/features/extrude_scope.rs tools/cli
# Extrude Cut inside a component with several occurrences cuts that
# component's own definition (mitcad#769), through the real UI: Ring is
# placed twice (Ring:1 shown, Ring:2 hidden) with a block and a sketch of
# its own, a square inside the block; Other, placed twice too, has a block
# over Ring:1. With Ring active, a new cut from Ring's square:
#   - lists Ring once, eligible and checked, saying that every occurrence
#     changes, and no row for the hidden Ring:2; Other stays disabled;
#   - OK is enabled and commits; Ring's block (its definition, which both
#     occurrences show) is cut, Other's is not;
#   - the saved cut_scope has Ring:1 as its context and no exclusions.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cut-own-shared-component-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-cut-own-shared.XXXXXX)
FILE=$WORK/own-shared.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# Ring (C1, O1): Box F1.b0 (0..20 x 0..20, 10 high) and Sketch F2 with a
# 12 x 4 square inside it. Other (C2, O2) 20 up: F3.b0 over Ring's block.
# Ring:2 (O3) 100 along x, hidden; Other:2 (O4) far away.
cat > "$WORK/model.json" << 'EOF'
[{"cmd": "create_component", "name": "Ring"},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "sketch.create"},
 {"cmd": "sketch.add_rectangle", "sketch": "F2", "corner": [4, 4], "width": 12, "height": 4},
 {"cmd": "activate_component", "component": "C0"},
 {"cmd": "create_component", "name": "Other", "transform": {"translation": [0, 0, 20]}},
 {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0], "length": 20, "width": 20,
  "height": 10, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "C0"},
 {"cmd": "copy_occurrence", "occurrence": "Ring:1", "transform": {"translation": [100, 0, 0]}},
 {"cmd": "copy_occurrence", "occurrence": "Other:1", "transform": {"translation": [200, 0, 20]}},
 {"cmd": "set_occurrence_visible", "occurrence": "O3", "visible": false}]
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

ui_start_display
ui_start_app --open "$FILE"
# High enough for the whole Extrude panel with its list, unscrolled.
ui_step "a high window"                  xdotool windowsize "$UI_WINDOW" 1400 990
eval "$(xdotool getwindowgeometry --shell "$UI_WINDOW")"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the design is shown"
# Saved with the root active, it opens so (mitcad#1678); the cut is Ring's.
ui_mark
ui_step "activate Ring (radio)"          ui_click_logged "Browser radio Root/Ring:1"
ui_expect_new "Activated Ring" "Ring is active"

echo "--- A new cut inside Ring, which has two occurrences"
ui_mark
ui_step "Extrude (search)"               ui_command "Extrude"
ui_expect_new "Command Extrude started" "Extrude started"
ui_expect_new "Extrude Profiles: 1 profile" "the square taken"
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
ui_expect_log "Cut target C1@O1: eligible, checked, intersects, own definition" \
  "Ring listed once, eligible and checked, as the component being edited"
ui_expect_log "Cut target C2@O2: unavailable, unchecked, intersects" "Other's shared definition stays disabled"
ui_expect_log "Preview Extrude: ok" "the cut previews"
ui_expect_log "Cut target row C1@O1 at " "Ring's row laid out"
ui_sync
grep -qF "Cut target C1@O3" "$UI_LOG" && ui_fail "a row for the hidden Ring:2"
echo "ok   no row for the hidden Ring:2"
grep -qF "Cut target C1@O1: unavailable" "$UI_LOG" && ui_fail "Ring was refused"
echo "ok   Ring never refused"

echo "--- Committed"
ui_mark
ui_step "OK"                             ui_click_logged "Panel Extrude OK"
ui_expect_new "Added extrude (Cut)" "the cut added (OK enabled)" 30
# The square through Ring's block, 12 x 4 x 10 mm, went; Other's stays.
ui_expect_volume "(F1.b0): volume 4000.000 -> \([0-9.]*\) mm3" 3520 "Ring's block cut"
expect_none "(F3.b0): volume" "Other's block not cut"
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved $FILE" "saved"
expect_scope '.context == {"component": "C1", "occurrence": "O1"}' "Ring:1 is the context"
expect_scope '.targets.type == "automatic"' "an automatic scope"
expect_scope '(.targets.excluded // []) == [] and (.targets.excluded_bodies // []) == []' "no exclusions"
ui_crashed && ui_fail "the application crashed"

ui_finish "UI cut own shared component test"
