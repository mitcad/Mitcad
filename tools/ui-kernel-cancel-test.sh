#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: core/ffi/src/kernel/abandon.rs core/ffi/src/kernel/cancel.rs core/ffi/src/kernel/mod.rs
# check-all sources: app/mcp/LiveServer.cpp app/framework/DocumentSession.cpp app/MainWindowJobs.cpp
# check-all sources: tools/cli
# A fillet whose geometry kernel call does not return (mitcad#1917), through
# the real UI on Xvfb and the actual MCP bridge (`mitcad-cli mcp --connect`).
# Every fillet's kernel call first waits a minute deaf to any cancel
# (MITCAD_TEST_KERNEL_HANG, core/ffi/src/kernel/abandon.rs), as OCCT's
# fillet did where it never looked at its progress range:
#   1. MCP: a model_command that adds a fillet runs; the client's
#      notifications/cancelled of that call ends it within a second with
#      "the computation was canceled"; the design is as it was and takes
#      the next query and command at once.
#   2. The progress dialog: a design with a fillet opened through File >
#      Open, Esc a second in: the open stops within a second of the request
#      and the window keeps the design it had.
#   3. The kernel's time limit (MITCAD_KERNEL_TIME_LIMIT=2, a new start):
#      the fillet added over MCP fails after about two seconds with "the
#      geometry kernel did not finish within 2 s"; the failed feature
#      stays in the timeline and the design goes on (an undo answers).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-kernel-cancel-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-kernel-cancel.XXXXXX)
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
EDGE='E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}'
# A 60 x 40 x 20 block (48000 mm³), and the same with its vertical edge at
# (60, 0) rounded by 2 (made before the hang is set).
jq -cn --arg r "$RECT" '[{cmd: "sketch.create"},
  {cmd: "sketch.add_rectangle", sketch: "F1", corner: [0, 0], width: 60, height: 40},
  {cmd: "add_feature", def: {type: "extrude", profiles: [{sketch: "F1", region: $r}],
    extent: {type: "distance", distance: 20}, operation: "new_body"}}]' > "$WORK/block.json"
FILLET=$(jq -cn --arg e "$EDGE" '{cmd: "add_feature", def: {type: "fillet", body: "F2.b0", edges: [$e], radius: 2}}')
jq --argjson f "$FILLET" '. + [$f]' "$WORK/block.json" > "$WORK/rounded.json"
"$CLI" run "$WORK/block.json" --save "$WORK/block.mitcad" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; exit 1; }
"$CLI" run "$WORK/rounded.json" --save "$WORK/rounded.mitcad" >> "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; exit 1; }
export MITCAD_TEST_KERNEL_HANG=60

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs (as ui-mcp-document-new-test.sh).

MCP_ID=0
bridge_open() {
  rm -f "$WORK/to-bridge" "$WORK/from-bridge"
  mkfifo "$WORK/to-bridge" "$WORK/from-bridge"
  "$CLI" mcp --connect < "$WORK/to-bridge" > "$WORK/from-bridge" 2> "$WORK/bridge.err" &
  BRIDGE=$!
  exec 7> "$WORK/to-bridge"
  exec 8< "$WORK/from-bridge"
  mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
    "clientInfo": {"name": "mitcad-kernel-cancel", "version": "1"}}'
  jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
  mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
}

bridge_close() {
  exec 7>&-
  wait "$BRIDGE" || ui_fail "the bridge ended with an error: $(cat "$WORK/bridge.err")"
  BRIDGE=""
  exec 8<&-
}

mcp_send() { printf '%s\n' "$1" >&7; }

# mcp_answer: reads the answer to request MCP_ID into answer.json.
mcp_answer() {
  local line
  while :; do
    IFS= read -r -t 120 -u 8 line || ui_fail "no answer to request $MCP_ID from the bridge: $(cat "$WORK/bridge.err")"
    [ -z "$line" ] && continue
    printf '%s\n' "$line" > "$WORK/answer.json"
    jq -e 'has("id") or (has("method") | not)' "$WORK/answer.json" > /dev/null || continue
    jq -e --argjson id "$MCP_ID" '.id == $id' "$WORK/answer.json" > /dev/null ||
      ui_fail "the answer has another request's id: $(head -c 2000 "$WORK/answer.json")"
    return 0
  done
}

# mcp_call name arguments: sends a tools/call without waiting for it.
mcp_call() {
  MCP_ID=$((MCP_ID + 1))
  mcp_send "$(jq -cn --argjson id "$MCP_ID" --arg name "$1" --argjson arguments "$2" \
    '{jsonrpc: "2.0", id: $id, method: "tools/call", params: {name: $name, arguments: $arguments}}')"
}

mcp_request() {
  local params=${2:-'{}'}
  MCP_ID=$((MCP_ID + 1))
  mcp_send "$(jq -cn --argjson id "$MCP_ID" --arg method "$1" --argjson params "$params" \
    '{jsonrpc: "2.0", id: $id, method: $method, params: $params}')"
  mcp_answer
}

# mcp_result [true]: the last answer's tool result into result.json; true
# when the tool must have refused or failed (isError).
mcp_result() {
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = "${1:-false}" ] ||
    ui_fail "unexpected result: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# mcp_tool name arguments [true]: a tools/call and its result.
mcp_tool() {
  mcp_call "$1" "$2"
  mcp_answer
  mcp_result "${3:-false}"
}

# expect 'jq condition' description [--arg name value ...]: on the last result.
expect() {
  local condition=$1 description=$2
  shift 2
  jq -e "$@" "$condition" "$WORK/result.json" > /dev/null ||
    ui_fail "$description: $(head -c 2000 "$WORK/result.json")"
  echo "ok   $description"
}

# wait_editable: document_list until the design takes commands.
wait_editable() {
  for _ in $(seq 1 120); do
    mcp_tool document_list '{}'
    jq -e '.documents[0] | .editable and (.busy | not)' "$WORK/result.json" > /dev/null && return 0
    sleep 0.5
  done
  ui_fail "the design did not become editable: $(head -c 2000 "$WORK/result.json")"
}

# volume: the total volume of the design's bodies, in mm³.
volume() {
  mcp_tool model_query '{"query": {"query": "bodies", "properties": true}}'
  jq -r '[.result[].volume] | add // 0 | round' "$WORK/result.json"
}

now_ms() { echo $(($(date +%s%N) / 1000000)); }

ui_start_display
ui_open_in_current_window

echo "--- 1. MCP: the client cancels its call while the fillet's kernel call hangs"
ui_start_app --no-native-dialogs --mcp-folder "$WORK" --open "$WORK/block.mitcad"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
bridge_open
wait_editable
[ "$(volume)" = 48000 ] || ui_fail "the block's volume is not 48000 mm³: $(head -c 2000 "$WORK/result.json")"
ui_mark
mcp_call model_command "$(jq -cn --argjson c "$FILLET" '{command: $c}')"
CALL=$MCP_ID
ui_expect_new "Computing " "the fillet's command computes" 10
sleep 1
REQUESTED=$(now_ms)
mcp_send "$(jq -cn --argjson id "$CALL" '{jsonrpc: "2.0", method: "notifications/cancelled",
  params: {requestId: $id, reason: "the test cancels it"}}')"
mcp_answer
ANSWERED=$(now_ms)
mcp_result true
expect '.error | test("cancel")' "the cancelled call answers that the computation was canceled"
[ $((ANSWERED - REQUESTED)) -lt 1000 ] ||
  ui_fail "the call answered $((ANSWERED - REQUESTED)) ms after the cancellation"
echo "ok   the call answered $((ANSWERED - REQUESTED)) ms after the cancellation"
ui_expect_new "MCP: mitcad-kernel-cancel cancelled its running call" "the window logged the cancellation"
[ "$(volume)" = 48000 ] || ui_fail "the design changed: $(head -c 2000 "$WORK/result.json")"
echo "ok   the design is as it was"
mcp_tool model_query '{"query": {"query": "timeline"}}'
expect '[.. | .type? | select(. == "fillet")] | length == 0' "no fillet in the timeline"
mcp_tool ui_state '{}'
expect '.busy == false and .editable == true' "the window takes commands at once"
bridge_close

echo "--- 2. The progress dialog: Esc while the fillet's kernel call hangs"
ui_mark
ui_key ctrl+o
ui_type_path "Open" "$WORK/rounded.mitcad"
ui_expect_new "Progress dialog shown: rounded.mitcad" "the progress dialog of the open"
sleep 1
ui_focus_dialog '^Computing$'
ui_key Escape # not a step: it waits for the job
ui_expect_new "Computing rounded.mitcad cancelled after" "the open stopped" 30
REQUESTED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/.*Computing rounded.mitcad: cancel requested after \([0-9]*\) ms.*/\1/p' | tail -1)
STOPPED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" |
  sed -n 's/.*Computing rounded.mitcad cancelled after \([0-9]*\) ms.*/\1/p' | tail -1)
[ -n "$REQUESTED" ] && [ -n "$STOPPED" ] || ui_fail "no times of the cancel logged"
[ $((STOPPED - REQUESTED)) -lt 1000 ] ||
  ui_fail "the open stopped $((STOPPED - REQUESTED)) ms after the cancel request (at $REQUESTED ms)"
echo "ok   the open stopped $((STOPPED - REQUESTED)) ms after the cancel request (at $REQUESTED ms)"
ui_focus_main
[ "$(ui_window_title)" = "block.mitcad - MitCad" ] || ui_fail "the window's design changed: $(ui_window_title)"
echo "ok   the window keeps the block"
ui_stop_app

echo "--- 3. The kernel's time limit fails the fillet in words"
export MITCAD_KERNEL_TIME_LIMIT=2
ui_start_app --no-native-dialogs --mcp-folder "$WORK" --open "$WORK/block.mitcad"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
bridge_open
wait_editable
STARTED=$(now_ms)
mcp_tool model_command "$(jq -cn --argjson c "$FILLET" '{command: $c}')" true
TOOK=$(($(now_ms) - STARTED))
expect '[.. | strings | select(contains("the geometry kernel did not finish within 2 s"))] | length > 0' \
  "the fillet failed: the geometry kernel did not finish within 2 s"
[ "$TOOK" -ge 2000 ] && [ "$TOOK" -lt 6000 ] || ui_fail "the command took $TOOK ms"
echo "ok   the command answered after $TOOK ms"
mcp_tool model_query '{"query": {"query": "timeline"}}'
expect '[.. | .type? | select(. == "fillet")] | length == 1' "the failed fillet stays in the timeline"
mcp_tool model_command '{"command": {"cmd": "undo"}}'
expect '.applied == true' "the design goes on: undo"
[ "$(volume)" = 48000 ] || ui_fail "the block changed: $(head -c 2000 "$WORK/result.json")"
bridge_close
ui_stop_app
echo "PASS"
