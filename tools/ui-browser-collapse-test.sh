#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindow.cpp core/model-api/src/components.rs tools/cli
# The browser's Collapse All (mitcad#312), through the real UI, on a
# synthetic design with several branches and four nested levels
# (Root > Sketches > Sketch1 > Extrude1):
#   - The design fits the browser: it opens expanded (mitcad#426).
#   - Several branches are opened with their disclosure arrows, and
#     Collapse All is chosen from the context menu of the deepest row: the
#     whole tree below the top row folds, descendants of folded rows too;
#     the top row stays open, its children in sight and folded (mitcad#426).
#   - Model contents and visibility are unchanged (nothing is logged for them).
#   - The top row's arrow still folds it; opened again it shows only its
#     children: folders that open by default and branches that were open
#     before stay folded.
#   - A refresh (a light bulb toggled and back) does not restore the states.
#   - Each arrow click opens one level; the levels below stay folded.
#   - A second Collapse All, also when everything is folded already and the
#     top row too, leaves the top row open and the rest folded.
#   - Opening a design: one saved with a component active opens with that
#     component active and unmodified (mitcad#1678); one with more rows than the
#     browser shows opens with the root's children in sight and folded, and
#     Collapse All there keeps the root open.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-browser-collapse-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-browser-collapse.XXXXXX)
FILE=$WORK/collapse.mitcad
trap 'ui_cleanup; rm -rf "$WORK"' EXIT

# count_new "text": how many lines logged since the mark contain the text.
count_new() { tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -cF -- "$1"; }

# expect_none "text" "description": nothing logged since the mark contains it.
expect_none() {
  [ "$(count_new "$1")" = 0 ] || ui_fail "$2: '$1' was logged"
  echo "ok   $2"
}

# arrow "path": a click on the row's disclosure arrow; apart from a click
# there a moment ago (the top row's arrow folds and opens it in turn),
# which Qt would make a double-click.
arrow() {
  local at
  at=$(ui_logged_at "Browser arrow $1") || exit 1
  ui_apart $at
  xdotool mousemove $at click 1
}

# Sketch1 (F1) 30 x 20 on XY; Extrude1 (F2) 10 up: Body1; Plane1 (F3) 30 up;
# Sketch2 (F4) 20 x 10 on it; Loft1 (F5) from Sketch1 to Sketch2: Body2.
cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude",
    "profiles": [{"sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "construction_plane",
    "definition": {"type": "offset", "plane": "xy", "distance": 30}}},
  {"cmd": "sketch.create", "plane": "F3"},
  {"cmd": "sketch.add_rectangle", "sketch": "F4", "corner": [5, 5], "width": 20, "height": 10},
  {"cmd": "add_feature", "def": {"type": "loft", "sections": [
    {"type": "profile", "sketch": "F1", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"},
    {"type": "profile", "sketch": "F4", "region": "r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}"}],
    "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "Browser opened expanded: " "the design fits the browser: it opens expanded"
grep -q "Browser opened folded" "$UI_LOG" && ui_fail "the small design opened folded"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Browser Root/Sketches/Sketch1 at" "the browser lists Sketch1"
ui_expect_log "Browser arrow Root at" "the top row has a disclosure arrow"
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- Several branches open, four levels deep"
ui_step "open Sketch1"                   arrow "Root/Sketches/Sketch1"
ui_expect_log "Browser Root/Sketches/Sketch1/Extrude1 at" "Extrude1, four levels down, is in sight"
ui_step "open Named Views"               arrow "Root/Named Views"
ui_expect_log "Browser Root/Named Views/Top at" "the named views are listed"
ui_expect_log "Browser Root/Bodies/Body1 at" "the Bodies folder is open (by default)"

echo "--- Collapse All from the deepest row"
ui_mark
ui_step "right-click Extrude1's row"     ui_click_logged "Browser Root/Sketches/Sketch1/Extrude1" 3
ui_expect_new "Collapse All" "the menu has Collapse All"
ui_step "choose Collapse All"            ui_menu_choose "Collapse All"
ui_expect_new "Browser collapsed all" "the browser folded"
ui_expect_new "Browser Root/Sketches/Sketch1/Extrude1 out of sight" "the deepest row is folded away"
ui_expect_new "Browser Root/Named Views/Top out of sight" "so are the named views"
ui_expect_new "Browser Root/Bodies/Body1 out of sight" "and the bodies"
ui_place_valid "Browser Root" || ui_fail "the top row is gone"
echo "ok   the top row stays"
# The top row stays open (mitcad#426): its children in sight, folded.
for row in "Root/Sketches" "Root/Bodies" "Root/Named Views" "Root/Construction"; do
  ui_place_valid "Browser $row" || ui_fail "$row is not in sight after Collapse All"
done
echo "ok   the root's children stay in sight"
expect_none "Browser Root/Sketches/Sketch1 at" "the Sketches folder is folded"
expect_none "Visibility " "no light bulb changed"
expect_none "Suppressed" "no feature changed"
expect_none "Removed body" "no body changed"

echo "--- The top row's arrow folds it; opened again it opens one level"
ui_mark
ui_step "fold the top row"               arrow "Root"
ui_expect_new "Browser Root/Sketches out of sight" "the root's children are folded away"
ui_mark
ui_step "open the top row"               arrow "Root"
ui_expect_new "Browser Root/Sketches at" "its children are in sight"
ui_expect_new "Browser Root/Bodies at" "also the folders that open by default"
expect_none "Browser Root/Sketches/Sketch1 at" "the folded Sketches folder stays folded"
expect_none "Browser Root/Bodies/Body1 at" "the Bodies folder stays folded"
expect_none "Browser Root/Named Views/Top at" "Named Views stays folded"

echo "--- A refresh does not restore the states"
ui_mark
ui_step "hide the Bodies folder's bulb"  ui_click_logged "Browser eye Root/Bodies"
ui_expect_new "Visibility Root/Bodies" "the model changed and the browser was rebuilt"
ui_step "show it again"                  ui_click_logged "Browser eye Root/Bodies"
sleep 1
expect_none "Browser Root/Sketches/Sketch1 at" "Sketch1's row did not come back"
expect_none "Browser Root/Bodies/Body1 at" "nor did the bodies"
expect_none "Browser Root/Named Views/Top at" "nor the named views"

echo "--- One level per arrow click"
ui_mark
ui_step "open Sketches"                  arrow "Root/Sketches"
ui_expect_new "Browser Root/Sketches/Sketch1 at" "Sketch1 is in sight"
ui_expect_new "Browser Root/Sketches/Sketch2 at" "and Sketch2"
expect_none "Browser Root/Sketches/Sketch1/Extrude1 at" "Sketch1, open before, is folded"
ui_mark
ui_step "open Sketch1"                   arrow "Root/Sketches/Sketch1"
ui_expect_new "Browser Root/Sketches/Sketch1/Extrude1 at" "Extrude1 is in sight"
expect_none "Browser Root/Sketches/Sketch2/Loft1 at" "Sketch2 stays folded"
ui_mark
ui_step "open Bodies"                    arrow "Root/Bodies"
ui_expect_new "Browser Root/Bodies/Body1 at" "Body1 is in sight"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- A second Collapse All, also when everything is folded"
ui_mark
ui_step "right-click the top row"        ui_click_logged "Browser Root" 3
ui_step "choose Collapse All"            ui_menu_choose "Collapse All"
ui_expect_new "Browser Root/Sketches/Sketch1 out of sight" "the open branches folded"
ui_expect_new "Browser Root/Bodies/Body1 out of sight" "the bodies too"
ui_place_valid "Browser Root/Sketches" || ui_fail "the root's children left sight"
echo "ok   the root's children stay in sight"
ui_mark
ui_step "fold the top row"               arrow "Root"
ui_expect_new "Browser Root/Sketches out of sight" "everything is folded, the top row too"
ui_mark
ui_step "right-click the top row"        ui_click_logged "Browser Root" 3
ui_step "choose Collapse All again"      ui_menu_choose "Collapse All"
ui_expect_new "Browser collapsed all" "the second one ran"
ui_expect_new "Browser Root/Sketches at" "the top row is open again, one level in sight"
ui_place_valid "Browser Root" || ui_fail "the top row is gone"
echo "ok   the top row stays"
expect_none "Browser Root/Sketches/Sketch1 at" "no more"
expect_none "Visibility " "no light bulb changed"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

echo "--- A design saved with a component active opens with it active (mitcad#1678)"
ui_stop_app
# Bracket (C1) with Cylinder1 (F1), left active when saved.
cat > "$WORK/active.json" << 'EOF'
[
  {"cmd": "create_component", "name": "Bracket"},
  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],
    "diameter": 10, "height": 10, "operation": "new_body"}},
  {"expect": {"query": {"query": "components"}, "result": {"active": "C1"}}}
]
EOF
"$CLI" run "$WORK/active.json" --save "$WORK/active.mitcad" > "$WORK/cli-active.log" 2>&1 ||
  { cat "$WORK/cli-active.log"; ui_fail "mitcad-cli (active component)"; }
grep -q '"active_component": *"C1"' "$WORK/active.mitcad" || ui_fail "the file does not store Bracket active"
ui_start_app --open "$WORK/active.mitcad"
ui_expect_log "Opened with C1 active, as saved (unmodified)" \
  "Bracket is active as saved, the design unmodified"
ui_expect_log "Browser opened expanded: " "a small design opens expanded"
ui_expect_log "Browser Root/Bracket:1/Bodies/Body1 at" "Bracket's rows are open"
ui_sync
# The empty design of the start had its Component1 active: the view's
# active component is Bracket's now.
ui_expect_log "View inactive ghosts: 0 bodies (active Bracket:1)" "the view has Bracket active"
grep -q "^Opened with the root active" "$UI_LOG" && ui_fail "the design opened with the root active"
echo "ok   not with the root active"
case "$(ui_window_title)" in
*"* - MitCad") ui_fail "the window title marks the design modified" ;;
esac
echo "ok   the window title marks no change"
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
sleep 1
expect_none "Undo: Activate" "the activation is no undo step"
expect_none "Activated " "nothing was activated by hand"

echo "--- A design longer than the browser opens folded (mitcad#426)"
ui_stop_app
# Part1..Part16, each a component with a cylinder of its own: 4 rows of
# the root and 3 of each part (its row, Bodies, the body), more than the
# browser shows.
{
  echo '['
  for i in $(seq 1 16); do
    echo "  {\"cmd\": \"create_component\", \"name\": \"Part$i\", \"transform\": {\"translation\": [$((i * 20)), 0, 0]}},"
    echo '  {"cmd": "add_feature", "def": {"type": "cylinder", "plane": "xy", "center": [0, 0],'
    echo '    "diameter": 10, "height": 10, "operation": "new_body"}},'
    echo '  {"cmd": "activate_component", "component": "Root"},'
  done
  echo '  {"expect": {"bodies": 16}}'
  echo ']'
} > "$WORK/large.json"
"$CLI" run "$WORK/large.json" --save "$WORK/large.mitcad" > "$WORK/cli-large.log" 2>&1 ||
  { cat "$WORK/cli-large.log"; ui_fail "mitcad-cli (large design)"; }
ui_start_app --open "$WORK/large.mitcad"
ui_expect_log "Browser opened folded: " "the long tree (52 rows) opens folded"
ui_sync # the rows' places as folded (logged after the fold)
ui_expect_log "Browser Root/Part1:1 at" "the root's children are in sight"
for row in "Root/Part1:1" "Root/Part2:1" "Root/Named Views" "Root/Origin"; do
  ui_place_valid "Browser $row" || ui_fail "$row is not in sight"
  ui_place_valid "Browser arrow $row" || ui_fail "$row has no arrow in sight"
done
echo "ok   the first parts and the folders are in sight with their arrows"
ui_place_valid "Browser Root/Part1:1/Bodies" && ui_fail "Part1 is open"
echo "ok   Part1 is folded"
ui_mark
ui_step "open Part1"                     arrow "Root/Part1:1"
ui_expect_new "Browser Root/Part1:1/Bodies at" "its Bodies folder is in sight"
expect_none "Browser Root/Part1:1/Bodies/Body1 at" "and folded"
ui_mark
ui_step "right-click Part1's Bodies"     ui_click_logged "Browser Root/Part1:1/Bodies" 3
ui_step "choose Collapse All"            ui_menu_choose "Collapse All"
ui_expect_new "Browser collapsed all" "Collapse All ran"
ui_expect_new "Browser Root/Part1:1/Bodies out of sight" "Part1 folded again"
ui_place_valid "Browser Root/Part1:1" || ui_fail "the root's children left sight"
echo "ok   the root stays open"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"

ui_finish "UI browser collapse test"
