#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/Sketch3dController.cpp app/sketch/Sketch3dController.hpp app/sketch/Sketch3dTools.cpp
# check-all sources: app/sketch/Sketch3dDraw.cpp app/MainWindowSketch3d.cpp app/MainWindowMcp.cpp
# check-all sources: app/mcp core/mcp core/ffi/src/mcp.rs tools/cli
# A 3D sketch tool's pending click on a curve that went (mitcad#2527): in
# 3DSketch1 (F1, on XY) with the line c3, Line's first click snaps to c3
# (its Midpoint or On Curve, which names only the curve); an agent's
# sketch3d.remove takes c3 away while the click waits. The tool drops the
# pending click ("its pending points went with the model's change"), so
# the next click starts a new line instead of a shape whose relation to
# the removed curve the model would refuse.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch3d-pending-curve-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch3d-pending-curve.XXXXXX")
FILE=$WORK/pending-curve.mitcad
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

# 3DSketch1 (F1) on XY with the line c3 from p1 to p2, away from the origin.
cat > "$WORK/model.json" << 'EOF'
[{"cmd": "sketch3d.create", "support": "xy"},
 {"cmd": "sketch3d.add_line", "sketch": "F1", "start": [10, 20, 0], "end": [50, 20, 0]},
 {"expect": {"feature": "3DSketch1", "status": "ok"}}]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

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
ui_step "fit (F6)"                       ui_key F6
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-pending-curve", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'

echo "--- Line's first click snapped to the line c3"
ui_double_click_expect "Timeline 3DSketch1" "Editing 3D sketch F1 (3DSketch1)" "3DSketch1 is edited"
ui_expect_new "3D sketch entity F1/c3 at" "the line c3 is drawn where a click meets it"
ui_mark
ui_step "Line (L)"                       ui_key l
ui_expect_new "3D sketch tool Line: started" "Line draws in the 3D sketch"
ui_sync
ui_mark
ui_step "click the line c3"              ui_click_logged "3D sketch entity F1/c3"
ui_expect_new "3D sketch Line: pending point 1 at " "the start is pending"
tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -F "3D sketch Line: pending point 1 at " | tail -1 |
  grep -qE "[(](Midpoint|On Curve): c3[)]$" ||
  ui_fail "the start did not snap to c3 alone: $(grep -F "3D sketch Line: pending point 1 at " "$UI_LOG" | tail -1)"
echo "ok   the start snapped to c3 (Midpoint or On Curve)"

echo "--- An agent removes c3 while the click waits"
ui_mark
mcp_tool model_command '{"command": {"cmd": "sketch3d.remove", "sketch": "F1", "items": ["c3"]}}'
ui_expect_new "MCP: an agent ran sketch3d.remove in sketch F1" "the agent's command ran in the open 3D sketch"
ui_expect_new "3D sketch tool: its pending points went with the model's change" \
  "the pending click on the removed curve is dropped"
ui_sync
ui_mark
ui_step "click where c3 was"             ui_click_logged "3D sketch entity F1/c3"
ui_expect_new "3D sketch Line: pending point 1 at " "the next click starts a new line"
ui_sync
grep -q "3D sketch command: " <(tail -n +$((UI_MARK + 1)) "$UI_LOG") &&
  ui_fail "the click after the removal committed a shape"
echo "ok   nothing committed with the removed curve"
ui_step "Esc (the pending point)"        ui_key Escape
ui_step "Esc (the tool)"                 ui_key Escape
bridge_close
ui_crashed && ui_fail "the application crashed"

ui_finish "UI 3D sketch pending click on a removed curve"
