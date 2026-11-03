#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowComments.cpp app/view/Comments.cpp app/view/CommentPinOverlay.cpp
# check-all sources: app/view/CommentsWindow.cpp app/view/NavigationBar.cpp app/MainWindowMcp.cpp app/mcp
# check-all sources: core/model-api/src core/model/src/document core/mcp tools/cli
# Comments for AI agents (mitcad#1119) through the real UI, on a design
# mitcad-cli makes: Body1 (F2.b0), a 30 x 20 x 10 block on XY. In the
# Docked layout:
#   - Add Comment (Shift+N) on Body1's top face, its text typed in the
#     panel: comment C1 open, by the user, on the face; its numbered pin
#     drawn in the view.
#   - A click on the pin opens the Comments panel with C1's row selected.
#   - An agent addresses C1 through the live MCP bridge (mitcad-cli mcp
#     --connect, model_command address_comment with a resolution): C1
#     addressed, its resolution in its row, its pin dimmed.
#   - Resolve in the panel: C1 resolved, its pin gone, its row left out of
#     the default filter and listed again under Resolved; undo makes it
#     addressed again.
#   - Comment Pins in the navigation bar hides and shows the pins.
# Then saved, and opened again in the Floating layout: C1 addressed, its
# pin in the view, its row in the panel after a click on the pin.
# tools/ui-windows-comments-test.ps1 (ctest app.ui-windows-comments) does
# the same in the Windows VM.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-comments-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
SHOT=${1:-}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-comments.XXXXXX")
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/design.mitcad
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

cat > "$WORK/model.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs (as ui-mcp-files-camera-test.sh).
# mcp_request leaves the answer in $WORK/answer.json, mcp_tool the tool's
# structured result in $WORK/result.json.

MCP_ID=0
bridge_open() {
  mkfifo "$WORK/to-bridge" "$WORK/from-bridge"
  "$CLI" mcp --connect < "$WORK/to-bridge" > "$WORK/from-bridge" 2> "$WORK/bridge.err" &
  BRIDGE=$!
  exec 7> "$WORK/to-bridge"
  exec 8< "$WORK/from-bridge"
}

bridge_close() {
  exec 7>&-
  wait "$BRIDGE" || ui_fail "the bridge ended with an error: $(cat "$WORK/bridge.err")"
  BRIDGE=""
  exec 8<&-
}

mcp_send() { printf '%s\n' "$1" >&7; }

mcp_request() {
  local line params=${2:-'{}'}
  MCP_ID=$((MCP_ID + 1))
  mcp_send "$(jq -cn --argjson id "$MCP_ID" --arg method "$1" --argjson params "$params" \
    '{jsonrpc: "2.0", id: $id, method: $method, params: $params}')"
  while :; do
    IFS= read -r -t 120 -u 8 line || ui_fail "no answer to $1 from the bridge: $(cat "$WORK/bridge.err")"
    [ -z "$line" ] && continue
    printf '%s\n' "$line" > "$WORK/answer.json"
    # A notification (a method, no id) is not the answer.
    jq -e 'has("id") or (has("method") | not)' "$WORK/answer.json" > /dev/null || continue
    jq -e --argjson id "$MCP_ID" '.id == $id' "$WORK/answer.json" > /dev/null ||
      ui_fail "the answer has another request's id: $(head -c 2000 "$WORK/answer.json")"
    return 0
  done
}

# mcp_tool name arguments: the tool's structured result, which must not
# be an error.
mcp_tool() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = false ] ||
    ui_fail "$1 $2: refused: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# wait_enabled "place": until the last "<place> at x,y" line is not
# " (disabled)".
wait_enabled() {
  local last
  for _ in $(seq 1 30); do
    last=$(grep -F "$1 at " "$UI_LOG" | tail -1)
    [ -n "$last" ] && [[ $last != *"(disabled)" ]] && return 0
    sleep 0.2
  done
  ui_fail "'$1' is not enabled: $last"
}

# ---------------------------------------------------------------------------

ui_start_display
ui_start_app --mcp-folder "$WORK" --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: F2.b0" "Body1 shown"
ui_expect_log "Comments: none" "no comments yet"

echo "--- Add Comment on Body1's top face"
ui_mark
ui_step "Add Comment (Shift+N)"          ui_key shift+n
ui_expect_new "Command Add Comment started" "Add Comment's panel"
ui_expect_new "Panel Add Comment input text at" "its Comment field"
ui_step "pick Body1's top face"          ui_click_pick "face F2.b0/F2:end("
ui_expect_new "Add Comment On: 1 face" "the top face picked"
ui_step "type the comment"               ui_type_in "Panel Add Comment input text" "make this face 2 mm thicker"
ui_mark
ui_step "OK (Enter)"                     ui_key Return
ui_expect_new "Done Add Comment" "added"
ui_expect_new "Comment added on face F2:end(" "on the top face"
ui_expect_new "Comment C1: open, user, " "C1 open, by the user"
ui_expect_new ": make this face 2 mm thicker" "with its text"
ui_expect_new "Comment C1 pin: open at " "its place worked out"
ui_expect_new "Comments open: 1" "one open comment"
ui_expect_new "Comment pins drawn: C1 open" "its pin drawn"
ui_expect_new "Comment pin C1 at " "the pin's place in the view"
[ -z "$SHOT" ] || { ui_capture "$SHOT"; echo "ok   screenshot $SHOT"; }

echo "--- A click on the pin opens the Comments panel"
ui_mark
ui_step "click C1's pin"                 ui_click_logged "Comment pin C1"
ui_expect_new "Comment pin clicked: C1" "the pin took the click"
ui_expect_new "Comments window opened: 1 comment(s)" "the Comments panel"
ui_expect_new "Comments row C1: open, user, " "C1's row"
ui_expect_new "Comments selected: C1" "C1's row selected"
ui_expect_new "Comments shown rows: C1" "listed by the default filter"

echo "--- An agent addresses C1 through the live MCP bridge"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-comments-test", "version": "1"}}'
jq -e '.result.protocolVersion' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
mcp_tool document_list '{}'
DOC=$(jq -r '.documents[0].document' "$WORK/result.json")
[ -n "$DOC" ] && [ "$DOC" != null ] || ui_fail "no document from document_list: $(cat "$WORK/result.json")"
ui_mark
mcp_tool model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "address_comment", id: "C1",
  resolution: "Made the face 2 mm thicker"}}')"
echo "ok   model_command address_comment"
ui_expect_new "Comment C1: addressed, " "C1 addressed"
ui_expect_new "Comment C1 resolution: Made the face 2 mm thicker" "with the agent's resolution"
ui_expect_new "Comment C1 pin: dimmed at " "its pin dimmed"
ui_expect_new "Comment pins drawn: C1 dimmed" "drawn dimmed"
ui_expect_new "Comments row C1: addressed" "its row addressed"
ui_expect_new "resolution: Made the face 2 mm thicker, replies: 0" "the resolution in its row"
bridge_close

echo "--- Resolve in the panel"
ui_step "click C1's row"                 ui_click_logged "Comments row C1"
ui_expect_log "Comments selected: C1" "C1 selected"
wait_enabled "Comments Resolve"
ui_mark
ui_step "Resolve"                        ui_click_logged "Comments Resolve"
ui_expect_new "Comments Resolve clicked: C1" "Resolve clicked"
ui_expect_new "Comment command: {\"cmd\":\"resolve_comment\",\"id\":\"C1\"}" "resolve_comment sent"
ui_expect_new "Comment C1: resolved" "C1 resolved"
ui_expect_new "Comment pins drawn: none" "its pin gone"
ui_expect_new "Comments open: 0" "nothing open"
ui_expect_new "Comments shown rows: none" "left out of the default filter"
ui_mark
ui_step "filter Resolved"                ui_click_logged "Comments filter Resolved"
ui_expect_new "Comments filter: Open, Addressed, Resolved" "resolved ones listed too"
ui_expect_new "Comments shown rows: C1" "C1 listed again"
ui_mark
ui_focus_main
ui_step "undo (Ctrl+Z)"                  ui_key ctrl+z
ui_expect_new "Comment C1: addressed" "undo: C1 addressed again"
ui_expect_new "Comment pins drawn: C1 dimmed" "its pin back, dimmed"

echo "--- Comment Pins in the navigation bar"
ui_mark
ui_step "Comment Pins (bar)"             ui_click_logged "Navigation bar Comment Pins"
ui_expect_new "Comment pins hidden" "the pins turned off"
ui_expect_new "Comment pins drawn: hidden (1)" "C1's pin not drawn"
ui_expect_new "Comment Pins off (View menu off)" "the bar's button and the View menu unchecked"
ui_mark
ui_step "Comment Pins (bar)"             ui_click_logged "Navigation bar Comment Pins"
ui_expect_new "Comment pins shown" "the pins on again"
ui_expect_new "Comment pins drawn: C1 dimmed" "C1's pin drawn again"

echo "--- Saved and opened again in the Floating layout"
ui_focus_main
ui_mark
ui_step "save (Ctrl+S)"                  ui_key ctrl+s
ui_expect_new "Saved " "saved"
ui_stop_app
ui_start_app --chrome=floating --no-mcp --open "$FILE"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Comment C1: addressed, " "C1 addressed in the saved design"
ui_expect_log "Comment C1 resolution: Made the face 2 mm thicker" "with its resolution"
ui_expect_log "Comment pins drawn: C1 dimmed" "its pin drawn"
ui_expect_log "Comment pin C1 at " "the pin's place"
ui_mark
ui_step "click C1's pin"                 ui_click_logged "Comment pin C1"
ui_expect_new "Comments window opened: 1 comment(s)" "the Comments panel in the Floating layout"
ui_expect_new "Comments row C1: addressed" "C1's row"
ui_expect_new "Comments selected: C1" "selected"

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI Comments test"
