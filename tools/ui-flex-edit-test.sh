#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/flex app/MainWindowFlexEdit.cpp app/MainWindow.cpp app/MainWindow.hpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/MainWindowNoticeHover.cpp
# check-all sources: app/commands/FlexCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: core/model-api/src/flex.rs core/model/src/features/flex.rs core/model/src/document/flex.rs
# check-all sources: geometry/src/flex.cpp geometry/src/flex_route.cpp geometry/src/flex_mesh.cpp tools/cli
# check-all sources: app/browser/FeaturePresentation.hpp app/browser/TimelineWidget.cpp
# check-all sources: app/framework/TestDriver.cpp tools/ui-scripts/flex-edit-hover.mitcad-ui
# A Flex item edited in its edit session (mitcad#1713, app/COMMANDS.md
# "Flex editing", owner 14.11.2026) through the real UI, on a design
# mitcad-cli makes: Box1 (F1, F1.b0) 40 x 30 x 20 at (20, 20), and Flex1
# (F2), a round hose of 6 mm from a free node n1 left of the box over n2 on
# the box's top to a free node n3 right of it, and Flex2 (F3), a short
# straight hose low right of the box, inside the others' bounds (the fit
# stays as it was), rolled back while Flex1's session runs.
#   1. A single click on Flex1's Timeline cell only selects it: no handles.
#   2. A double-click on its cell opens the edit session: the Flex panel on
#      the item and its handles beside it (nodes, both "+", insert points).
#   3. n3 dragged in the view: no route follows the pointer, the release
#      puts the change into the panel (no model command), and Undo in the
#      session takes it back.
#   4. A click on n2 takes the node in hand: the length is shown beside it;
#      Esc lets it go, the session stays.
#   5. A click on the route between n1 and n2 adds a node there, which is
#      the node in hand; Delete deletes it.
#   6. A click on the "+" at the end turns adding on: two clicks add two end
#      nodes; a click on the "+" again ends adding.
#   7. Enter keeps the session as one undo step; Undo takes all of it back.
#   8. A single click on the item in the view selects it without handles; a
#      double-click there opens the session again; Esc cancels it.
#   9. The app again, with the shared input script
#      tools/ui-scripts/flex-edit-hover.mitcad-ui (the in-app test driver's
#      pointer reaches the Timeline's tooltip on every platform): with
#      Flex1's session open, Flex2's hover text leaves the session's types
#      and end ports as it read them (mitcad#1971).
# The Windows test is tools/ui-windows-flex-edit-test.ps1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flex-edit-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(dirname "$UI_APP")/../tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-flex-edit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
FILE=$WORK/hose.mitcad

cat > "$WORK/model.json" << 'EOF'
[
  {"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
    "corner": [20, 20], "length": 40, "width": 30, "height": 20, "operation": "new_body"}},
  {"cmd": "add_feature", "def": {"type": "flex",
    "item": {"name": "Hose 6", "section": {"type": "round", "diameter": 6}, "bend_radius": 10},
    "nodes": [
      {"id": "n1", "at": [-10, 35, 30]},
      {"id": "n2", "at": [40, 35, 20], "on": {"body": "F1.b0", "face": "F1:top"}},
      {"id": "n3", "at": [90, 35, 30]}]}},
  {"cmd": "add_feature", "def": {"type": "flex",
    "item": {"name": "Hose 6", "section": {"type": "round", "diameter": 6}, "bend_radius": 10},
    "nodes": [
      {"id": "n1", "at": [70, 45, 4]},
      {"id": "n2", "at": [88, 45, 4]}]}},
  {"expect": {"feature": "Flex1", "status": "ok"}},
  {"expect": {"feature": "Flex2", "status": "ok"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# since_mark: the log's lines since the mark.
since_mark() { tail -n +$((UI_MARK + 1)) "$UI_LOG"; }
# session_nodes: the node ids of the last "Flex edit ...: in the session; nodes ..." line.
session_nodes() { sed -n 's/.*Flex edit .*: in the session; nodes \([n0-9 ]*\)$/\1/p' "$UI_LOG" | tail -1; }
# wait_line "text": waits (up to 6 s) for the text in the log since the mark.
wait_line() {
  for _ in $(seq 1 30); do
    since_mark | grep -qF -- "$1" && return 0
    sleep 0.2
  done
  ui_fail "'$1' not in the log"
}

ui_start_display
ui_start_app --open "$FILE"
ui_step "fit (F6)"                       ui_key F6

echo "--- 1. A single click selects the item, no handles"
ui_mark
ui_step "click Flex1 in the timeline"    ui_click_logged "Timeline Flex1"
ui_wait_idle
sleep 0.5
if since_mark | grep -qF "Flex edit shown"; then ui_fail "a single click showed the item's handles"; fi
if since_mark | grep -qF "Editing F2 with Flex"; then ui_fail "a single click opened the edit session"; fi
echo "ok   selected, no handles, no session"
sleep 0.6 # not a double-click with the click before

echo "--- 2. A double-click opens the edit session: the panel and the handles"
ui_double_click_expect "Timeline Flex1" "Editing F2 with Flex" "the double-click opens the Flex panel on the item"
wait_line "Flex edit shown: F2 "
ui_expect_new "Flex node n1 free at" "the first node is a handle"
ui_expect_new "Flex node n2 face at" "the node on the box's top is a handle"
ui_expect_new "Flex node n3 free at" "the last node is a handle"
ui_expect_new "Flex add start at" "a + before the first node"
ui_expect_new "Flex add end at" "a + after the last node"
wait_line "Flex insert after n1 at"
echo "ok   an insert point between n1 and n2 once the preview's route is read"
if since_mark | grep -qF "Sketch started"; then ui_fail "editing a Flex item opened a sketch"; fi

echo "--- 3. A node dragged: into the panel, no model command; Undo in the session"
ui_mark
ui_step "drag n3 up and to the right"    ui_drag_from "Flex node n3 free" 46 -34
ui_expect_new "Flex edit drag: started n3" "the drag started on the node"
ui_expect_new "Flex edit target: " "the pointer gave the node a place"
ui_expect_new "Flex edit drag n3: in the session; nodes n1 n2 n3" "the release changed the session"
ui_expect_new "Flex: Flex handles: drag n3 taken" "the panel took the change"
if since_mark | grep -qF "Flex edit command: "; then ui_fail "a drag in the session ran a model command"; fi
if since_mark | grep -qF "Flex edit preview: "; then ui_fail "a route followed the pointer in the session"; fi
if since_mark | grep -qF "Flex route under the pointer: "; then ui_fail "the panel's route followed the pointer"; fi
echo "ok   no model command, no route on the pointer"
wait_line "Preview Flex: ok"
ui_wait_idle
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
wait_line "Flex: undo in the edit: 0 change(s) left, 3 node(s)"
wait_line "Flex: Undo taken"
if since_mark | grep -qF "Undo: "; then ui_fail "Undo in the session undid a step of the model"; fi
echo "ok   the session's change taken back, the model's history untouched"
wait_line "Preview Flex: ok"
ui_wait_idle

echo "--- 4. A node in hand shows the length; Esc lets it go"
ui_mark
ui_step "click n2"                       ui_click_logged "Flex node n2 face"
ui_expect_new "Flex node selected: n2" "the node is in hand"
ui_expect_new "Flex edit badge: Length " "the length is shown beside it"
ui_mark
ui_step "Esc"                            ui_key Escape
ui_expect_new "Flex node selected: none" "Esc lets the node go"
if since_mark | grep -qF "Command Flex cancelled"; then ui_fail "Esc with a node in hand cancelled the session"; fi
echo "ok   the session stays"

echo "--- 5. A node added on the route, and deleted"
ui_mark
ui_step "click the route between n1 and n2" ui_click_logged "Flex insert after n1"
ui_expect_new '"cmd":"flex.add_node"' "a click on the route adds a node"
ui_expect_new '"place":{"after":"n1"}' "after n1"
ui_expect_new "Flex edit add after n1: in the session; nodes " "the session took it"
read -r -a NODES <<< "$(session_nodes)"
[ "${#NODES[@]}" = 4 ] && [ "${NODES[0]}" = n1 ] && [ "${NODES[2]}" = n2 ] ||
  ui_fail "the nodes after the insert: ${NODES[*]}"
ADDED=${NODES[1]}
echo "ok   four nodes, $ADDED between n1 and n2"
ui_expect_new "Flex node selected: $ADDED" "the new node is in hand"
wait_line "Preview Flex: ok"
ui_wait_idle
ui_mark
ui_step "Delete"                         ui_key Delete
ui_expect_new '"cmd":"flex.delete_node"' "Delete deletes the node in hand"
ui_expect_new "Flex edit delete $ADDED: in the session; nodes n1 n2 n3" "three nodes again"
wait_line "Preview Flex: ok"
ui_wait_idle

echo "--- 6. + at the end: each click adds an end node until + is clicked again"
ui_mark
ui_step "click + at the end"             ui_click_logged "Flex add end"
ui_expect_new "Flex edit add: started at the end" "the + took the click"
ui_expect_new "Flex edit add: armed" "adding nodes is on"
read -r PX PY <<< "$(ui_logged_at "Flex add end")"
ui_mark
ui_step "click in the view"              xdotool mousemove $((PX + 40)) $((PY - 30)) click 1
ui_expect_new '"place":"end"' "flex.add_node at the end"
ui_expect_new "Flex edit add at the end: in the session; nodes n1 n2 n3 " "the item is one node longer"
wait_line "Flex edit add: armed at the end, each click adds a node"
wait_line "Preview Flex: ok"
ui_wait_idle
ui_mark
ui_step "click in the view again"        xdotool mousemove $((PX + 40)) $((PY - 80)) click 1
ui_expect_new "Flex edit add at the end: in the session; nodes n1 n2 n3 " "and one more"
read -r -a NODES <<< "$(session_nodes)"
[ "${#NODES[@]}" = 5 ] || ui_fail "the nodes after two clicks: ${NODES[*]}"
echo "ok   five nodes, the old ends still nodes"
wait_line "Preview Flex: ok"
ui_wait_idle
ui_mark
ui_step "click + at the end again"       ui_click_logged "Flex add end"
ui_expect_new "Flex edit add: ended by a click on +" "a click on the + ends adding"
ui_mark
read -r PX PY <<< "$(ui_logged_at "Flex add end")"
ui_step "click in the view"              xdotool mousemove $((PX + 40)) $((PY + 60)) click 1
ui_wait_idle
if since_mark | grep -qF "Flex edit add at the end"; then ui_fail "a click after the + was ended added a node"; fi
echo "ok   the session as before: a click adds nothing"

echo "--- 7. Enter keeps the session as one undo step"
ui_mark
ui_step "Enter"                          ui_key Return
wait_line "Edited F2"
wait_line "Flex edit hidden: F2"
ui_wait_idle
ui_mark
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Undo: Edit" "one undo step takes the whole session back"
ui_wait_idle

echo "--- 8. In the view: a click selects, a double-click opens the session, Esc cancels"
ui_mark
ui_step "click the item in the view"     ui_click_logged "Flex node n2 face"
ui_wait_idle
sleep 0.5
if since_mark | grep -qF "Flex edit shown"; then ui_fail "a single click in the view showed the handles"; fi
echo "ok   selected, no handles"
sleep 0.6
ui_double_click_expect "Flex node n2 face" "Flex: double-click on F2 opens its edit session" \
  "a double-click on the item in the view opens its session"
wait_line "Editing F2 with Flex"
wait_line "Flex edit shown: F2 "
ui_wait_idle
ui_mark
ui_step "Esc"                            ui_key Escape
wait_line "Command Flex cancelled"
wait_line "Flex edit hidden: F2"
if grep -q "^OCCT view event failed:" "$UI_LOG"; then ui_fail "OCCT reported a failed view operation"; fi

echo "--- 9. Another Flex item's hover text leaves the session's types and end ports"
ui_stop_app
export MITCAD_TEST_WORK=$WORK MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$ROOT/tools/ui-scripts/flex-edit-hover.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$FILE"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the hover text driver timed out"
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "the hover text driver failed"; }
UI_RUNNER=""
ui_expect_log "TestDriver: done" "Flex2's hover text leaves the session's types and end ports"
ui_guard_check "in the hover text driver"

ui_finish "UI Flex editing test"
