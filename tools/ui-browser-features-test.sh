#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser core/model/src/document/browser.rs
# The features under the sketches they use in the browser (mitcad#177),
# through the real UI:
#   - A sketch with two extrusions has them as two rows under it, collapsed
#     at first and without light bulbs; a loft is under each of its two
#     sketches; an extrusion whose body went into a new component is marked
#     linked to that component.
#   - Selection both ways without feedback: a feature row selects the
#     feature in the timeline (and its body in the view), a loft's row
#     selects its rows under both sketches; a click in the timeline selects
#     the feature's rows in the browser and nothing goes back.
#   - A feature row's context menu is the timeline's (Suppress, Find in
#     Timeline).
#   - In-place rename: a slow second click on a selected row renames it, F2
#     still does, a double-click edits the feature instead.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-features-test.sh [screenshot.png]
# check-all sources: tools/cli

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-browser-features.XXXXXX)
FILE=$WORK/features.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# count_new "text": how many lines logged since the mark contain the text.
count_new() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -- "$1"; }

# Sketch1 (F1) 30 x 20 on XY; Extrude1 (F2) 10 up: Body1; Extrude2 (F3) 4
# down as a new component, Component1:1: Body2; Plane1 (F4) 30 up; Sketch2
# (F5) 20 x 10 on it; Loft1 (F6) from Sketch1 to Sketch2: Body3.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": -4}, "operation": "new_component"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F4"},
  {"cmd": "sketch.add_rectangle", "sketch": "F5", "corner": [5, 5], "width": 20, "height": 10},
  {"cmd": "add_feature", "def": {"type": "loft", "sections": [
    {"type": "profile", "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"},
    {"type": "profile", "sketch": "F5", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Sketches/Sketch1 at" "the browser lists Sketch1"
ui_expect_log "Browser Root/Sketches/Sketch2 at" "and Sketch2"
ui_expect_log "Timeline Loft1 at" "the timeline is logged"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- The features under their sketches"
grep -qF "Browser Root/Sketches/Sketch1/" "$UI_LOG" && ui_fail "Sketch1's features shown before it was opened"
echo "ok   the sketches start collapsed"
ui_mark
ui_step "click Sketch1"                  ui_click_logged "Browser Root/Sketches/Sketch1"
ui_expect_new "Timeline selected: F1" "the sketch's row selects it in the timeline"
ui_step "open it (Right)"                ui_key Right
ui_expect_new "Browser Root/Sketches/Sketch1/Extrude1 at" "Extrude1 under Sketch1"
ui_expect_new "Browser Root/Sketches/Sketch1/Extrude2 at" "Extrude2 under Sketch1"
ui_expect_new "Browser Root/Sketches/Sketch1/Loft1 at" "Loft1 under Sketch1"
ui_expect_new "Browser linked Root/Sketches/Sketch1/Extrude2 to Component1" \
  "Extrude2's body is in Component1: linked"
read -r _ y1 <<< "$(ui_logged_at "Browser Root/Sketches/Sketch1/Extrude1")"
read -r _ y2 <<< "$(ui_logged_at "Browser Root/Sketches/Sketch1/Extrude2")"
read -r _ y3 <<< "$(ui_logged_at "Browser Root/Sketches/Sketch1/Loft1")"
[ "$y1" -lt "$y2" ] && [ "$y2" -lt "$y3" ] || ui_fail "the rows are not in timeline order ($y1, $y2, $y3)"
echo "ok   in timeline order"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Browser eye Root/Sketches/Sketch1/" &&
  ui_fail "a feature row has a light bulb"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF "Browser linked Root/Sketches/Sketch1/Extrude1 " &&
  ui_fail "Extrude1 is marked linked"
echo "ok   no light bulbs on the feature rows; only Extrude2 is linked"
ui_mark
ui_step "click Sketch2"                  ui_click_logged "Browser Root/Sketches/Sketch2"
ui_step "open it (Right)"                ui_key Right
ui_expect_new "Browser Root/Sketches/Sketch2/Loft1 at" "Loft1 under Sketch2 too"

echo "--- Selection both ways"
ui_mark
ui_step "click Extrude1 under Sketch1"   ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1"
ui_expect_new "Timeline selected: F2" "Extrude1 selected in the timeline"
ui_expect_new "Selected: 1 body [body F2.b0]" "its body selected in the view"
ui_expect_new "Browser selected: Root/Bodies/Body1, Root/Sketches/Sketch1/Extrude1" \
  "the browser shows its body and its row"
sleep 1
[ "$(count_new "Timeline selected:")" = 1 ] || ui_fail "the timeline's selection changed more than once"
[ "$(count_new "Browser selected:")" -le 2 ] || ui_fail "the browser's selection went back and forth"
echo "ok   no feedback"
ui_mark
ui_step "click Loft1 under Sketch2"      ui_click_logged "Browser Root/Sketches/Sketch2/Loft1"
ui_expect_new "Timeline selected: F6" "Loft1 selected in the timeline"
ui_expect_new "Root/Sketches/Sketch1/Loft1, Root/Sketches/Sketch2/Loft1" "both of its rows selected"
ui_mark
ui_step "click Extrude2 in the timeline" ui_click_logged "Timeline Extrude2"
ui_expect_new "Selected: 1 body [body F3.b0 in O1]" "its body selected in the view"
ui_expect_new "Browser selected: Root/Sketches/Sketch1/Extrude2" "its row selected in the browser"
sleep 1
[ "$(count_new "Timeline selected:")" = 0 ] || ui_fail "the browser's rows selected in the timeline again"
grep -F "Browser selected:" <<< "$(tail -n +$((UI_MARK + 1)) "$UI_LOG")" | grep -qF "Loft1" &&
  ui_fail "Loft1's rows stayed selected"
echo "ok   no feedback, the loft's rows deselected"

echo "--- The context menu"
ui_mark
ui_step "right-click Extrude1's row"     ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1" 3
ui_expect_new "Context menu: Edit Feature | Rename | Suppress Features | Suppress When... | Delete | Move to Component... | Create Group | Roll Timeline Marker Here | Find in Timeline" \
  "the timeline's menu, finding it in the timeline"
ui_step "choose Suppress Features"       ui_menu_choose "Suppress Features"
ui_expect_new "Suppressed Extrude1" "suppressed from the browser"
ui_expect_new "Removed body Body1 (F2.b0)" "its body is gone"
ui_mark
ui_step "right-click Extrude1's row"     ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1" 3
ui_step "choose Unsuppress Features"     ui_menu_choose "Unsuppress Features"
ui_expect_new "Unsuppressed Extrude1" "unsuppressed"
ui_mark
ui_step "right-click Loft1's row"        ui_click_logged "Browser Root/Sketches/Sketch2/Loft1" 3
ui_step "choose Find in Timeline"        ui_menu_choose "Find in Timeline"
ui_expect_new "Found F6 in the timeline" "found in the timeline"

echo "--- Rename in place"
ui_step "click Sketch2"                  ui_click_logged "Browser Root/Sketches/Sketch2"
ui_mark
ui_step "click Extrude1's row"           ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1"
ui_expect_new "Timeline selected: F2" "selected"
sleep 1 # longer than a double-click
ui_step "click it again, slowly"         ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1"
ui_expect_new "Renaming Root/Sketches/Sketch1/Extrude1" "the slow second click renames it in place"
ui_step "type Base, Enter"               ui_key ctrl+a
xdotool type --delay 40 "Base"
ui_key Return
ui_expect_new "Renamed Extrude1 to Base" "Extrude1 is Base"
ui_expect_new "Browser Root/Sketches/Sketch1/Base at" "the row shows the new name"
ui_expect_new "Timeline Base at" "and the timeline"
ui_mark
ui_step "click Sketch2"                  ui_click_logged "Browser Root/Sketches/Sketch2"
ui_step "F2"                             ui_key F2
ui_expect_new "Renaming Root/Sketches/Sketch2" "F2 still renames"
ui_step "Esc"                            ui_key Escape
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A double-click edits"
ui_step "click Base's row"               ui_click_logged "Browser Root/Sketches/Sketch1/Base"
sleep 1 # selected and current: the double-click's first click could rename
ui_double_click_expect "Browser Root/Sketches/Sketch1/Base" "Editing F2 with Extrude" \
  "a double-click on the row edits the feature"
sleep 1
[ "$(count_new "Renaming ")" = 0 ] || ui_fail "the double-click started a rename"
echo "ok   no rename"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Command Extrude cancelled" "the edit cancelled"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI browser features test"
