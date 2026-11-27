#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineHistoryList.cpp app/browser/TimelineHistoryList.hpp
# check-all sources: app/browser/TimelineWidget.cpp app/browser/BrowserController.cpp app/MainWindowMcp.cpp
# check-all sources: app/mcp core/mcp core/ffi/src/mcp.rs tools/cli
# The History list's rows rebuilt while a feature is dragged (mitcad#2527):
# four boxes Alpha (F1), Beta (F2), Gamma (F3) and Delta (F4); Gamma's row
# pressed and dragged up over Alpha's, and while the button is held an
# agent deletes Alpha. The drag ends with the change ("History drag ended:
# the timeline changed") and the release moves nothing: before, the press's
# index named Delta by then, and Delta was moved.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-history-drag-rebuild-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-history-drag-rebuild.XXXXXX")
FILE=$WORK/boxes.mitcad
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; xdotool mouseup 1 2> /dev/null; ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

cat > "$WORK/model.json" << 'EOF'
[{"cmd": "add_feature", "name": "Alpha", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
  "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
 {"cmd": "add_feature", "name": "Beta", "def": {"type": "box", "plane": "xy", "corner": [20, 0],
  "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
 {"cmd": "add_feature", "name": "Gamma", "def": {"type": "box", "plane": "xy", "corner": [40, 0],
  "length": 10, "width": 10, "height": 10, "operation": "new_body"}},
 {"cmd": "add_feature", "name": "Delta", "def": {"type": "box", "plane": "xy", "corner": [60, 0],
  "length": 10, "width": 10, "height": 10, "operation": "new_body"}}]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# The timeline as the History list (Preferences > Display > Timeline).
mkdir -p "$UI_CONFIG/Mitcad"
printf '\n[timeline]\npresentation=list\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"

# ---------------------------------------------------------------------------
# The MCP bridge (as tools/ui-sketch-plane-pick-test.sh): JSON-RPC lines
# through two FIFOs; mcp_tool leaves the tool's result in $WORK/result.json.

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
    jq -e 'has("id") or (has("method") | not)' "$WORK/answer.json" > /dev/null || continue
    jq -e --argjson id "$MCP_ID" '.id == $id' "$WORK/answer.json" > /dev/null ||
      ui_fail "the answer has another request's id: $(head -c 2000 "$WORK/answer.json")"
    return 0
  done
}

mcp_tool() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = "false" ] ||
    ui_fail "$1 $2: unexpected result: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_expect_log "History list: Alpha, Beta, Gamma, Delta; marker after Delta" "the rows in timeline order"
ui_expect_log "History row Gamma at " "Gamma's row is where a press meets it"
ui_expect_log "History row Alpha at " "Alpha's row is where a drag goes"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-history-drag", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
ui_wait_idle

echo "--- Gamma dragged up over Alpha, the button held"
read -r fx fy <<< "$(ui_logged_at "History row Gamma")"
read -r tx ty <<< "$(ui_logged_at "History row Alpha")"
ty=$((ty - 4)) # above Alpha's middle: the boundary before it
ui_mark
ui_apart "$fx" "$fy"
xdotool mousemove "$fx" "$fy" mousedown 1
for i in 1 2 3 4 5 6 7 8; do
  xdotool mousemove "$fx" $((fy + (ty - fy) * i / 8))
  sleep 0.08
done
ui_sync

echo "--- An agent deletes Alpha while the button is held"
mcp_tool model_command '{"command": {"cmd": "delete_feature", "uid": "F1"}}'
ui_expect_new "MCP: an agent ran delete_feature" "the agent's command ran during the drag"
ui_expect_new "History drag ended: the timeline changed" "the drag ends with the change"
ui_expect_new "History list: Beta, Gamma, Delta; marker after Delta" "the rows were rebuilt"
ui_step "release the button"             xdotool mouseup 1
ui_wait_idle
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -E "^Moved .* in the timeline|^Move of .* refused" &&
  ui_fail "the release after the rebuild moved a feature"
echo "ok   the release moved nothing"
grep -F "History list: " "$UI_LOG" | tail -1 | grep -qF "History list: Beta, Gamma, Delta; marker after Delta" ||
  ui_fail "the rows changed after the release: $(grep -F "History list: " "$UI_LOG" | tail -1)"
echo "ok   the timeline is Beta, Gamma, Delta as the agent left it"
bridge_close
ui_crashed && ui_fail "the application crashed"

ui_finish "UI History list drag across a rebuild"
