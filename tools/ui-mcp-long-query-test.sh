#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/mcp/LiveServer.cpp app/MainWindowJobs.cpp
# check-all sources: app/framework/DocumentSession.cpp core/mcp/src/query_jobs.rs core/ffi/src/mcp.rs
# check-all sources: core/model-api/src/analysis.rs core/model-api/src/assembly_path.rs
# check-all sources: geometry/analysis/src/interference.cpp core/cpp/bridge/analysis.cpp tools/cli
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp
# check-all sources: geometry/analysis/src/measure.cpp core/model/src/assembly_path.rs
# Long live MCP model queries (mitcad#2540), through the real UI on Xvfb and
# the actual MCP bridge (`mitcad-cli mcp --connect`), two clients at once. A
# design of two 60 x 40 x 20 blocks that overlap by half (24000 mm³). The
# job's test delay (MITCAD_TEST_RECOMPUTE_DELAY_MS, ending early on a
# cancel) makes the whole-design interference check and each probe of an
# assembly path long, as the large designs of the report were:
#   1. Interference over the whole design (min_volume 1) runs as a model
#      job, not on the window's thread: the progress dialog shows; a second
#      client's model_query is answered busy with retry_after_ms at once;
#      the first client's notifications/cancelled ends the call within a
#      second with "the computation was canceled"; the next query runs.
#   2. An assembly path of the first block against the second (a 0.01 mm
#      slide, one step): the progress dialog's Esc (the user's Cancel) ends
#      the call with "the computation was canceled", the probe ending less
#      than a second after the cancel (mitcad#2577); the next query runs.
#   3. Interference run to its end keeps its result: 2 bodies, one pair
#      of 24000 mm³.
#   4. The application's own Interference command (mitcad#2577) runs its
#      check as a model job too: the progress dialog shows, the window
#      answers while the check runs, the dialog's Esc stops it and the
#      panel shows the inspection as canceled.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-long-query-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-mcp-long-query.XXXXXX)
BRIDGE=""
SECOND=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; [ -n "$SECOND" ] && kill "$SECOND" 2> /dev/null;
  ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
jq -cn --arg r "$RECT" '[{cmd: "sketch.create"},
  {cmd: "sketch.add_rectangle", sketch: "F1", corner: [0, 0], width: 60, height: 40},
  {cmd: "add_feature", def: {type: "extrude", profiles: [{sketch: "F1", region: $r}],
    extent: {type: "distance", distance: 20}, operation: "new_body"}},
  {cmd: "sketch.create"},
  {cmd: "sketch.add_rectangle", sketch: "F3", corner: [30, 0], width: 60, height: 40},
  {cmd: "add_feature", def: {type: "extrude", profiles: [{sketch: "F3", region: $r}],
    extent: {type: "distance", distance: 20}, operation: "new_body"}}]' > "$WORK/blocks.json"
"$CLI" run "$WORK/blocks.json" --save "$WORK/blocks.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; exit 1; }
# Long enough to see the job, show the dialog and cancel; the end of
# part 3 waits for the whole interference delay. Part 4's command runs
# its check as the job "interference" too.
export MITCAD_TEST_RECOMPUTE_DELAY_MS="interference=15000,assembly_path=15000"

# ---------------------------------------------------------------------------
# Two bridges: JSON-RPC lines through two FIFOs each (as
# ui-kernel-cancel-test.sh); the first client writes to fd 7 and reads fd 8,
# the second writes to fd 5 and reads fd 6.

MCP_ID=0

# bridge_start name write-fd read-fd: a bridge process, initialized.
bridge_start() {
  local name=$1 out=$2 in=$3
  mkfifo "$WORK/to-$name" "$WORK/from-$name"
  "$CLI" mcp --connect < "$WORK/to-$name" > "$WORK/from-$name" 2> "$WORK/$name.err" &
  local pid=$!
  eval "exec $out> \"\$WORK/to-$name\""
  eval "exec $in< \"\$WORK/from-$name\""
  MCP_ID=$((MCP_ID + 1))
  send "$out" "$(jq -cn --argjson id "$MCP_ID" --arg name "$name" '{jsonrpc: "2.0", id: $id, method: "initialize",
    params: {protocolVersion: "2025-11-25", capabilities: {}, clientInfo: {name: ("mitcad-long-query-" + $name), version: "1"}}}')"
  answer "$in" "$MCP_ID"
  jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize of $name failed"
  send "$out" '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
  LAST_PID=$pid
}

send() { printf '%s\n' "$2" >&"$1"; }

# answer read-fd id: reads the answer to request id into answer.json.
answer() {
  local fd=$1 id=$2 line
  while :; do
    IFS= read -r -t 120 -u "$fd" line || ui_fail "no answer to request $id from the bridge"
    [ -z "$line" ] && continue
    printf '%s\n' "$line" > "$WORK/answer.json"
    jq -e 'has("id") or (has("method") | not)' "$WORK/answer.json" > /dev/null || continue
    jq -e --argjson id "$id" '.id == $id' "$WORK/answer.json" > /dev/null ||
      ui_fail "the answer has another request's id: $(head -c 2000 "$WORK/answer.json")"
    return 0
  done
}

# call write-fd name arguments: sends a tools/call without waiting for it.
call() {
  MCP_ID=$((MCP_ID + 1))
  send "$1" "$(jq -cn --argjson id "$MCP_ID" --arg name "$2" --argjson arguments "$3" \
    '{jsonrpc: "2.0", id: $id, method: "tools/call", params: {name: $name, arguments: $arguments}}')"
}

# result [true]: the last answer's tool result into result.json; true when
# the tool must have refused or failed (isError).
result() {
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = "${1:-false}" ] ||
    ui_fail "unexpected result: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# tool name arguments [true]: a tools/call of the first client and its result.
tool() {
  call 7 "$1" "$2"
  answer 8 "$MCP_ID"
  result "${3:-false}"
}

# expect 'jq condition' description: on the last result.
expect() {
  jq -e "$1" "$WORK/result.json" > /dev/null || ui_fail "$2: $(head -c 2000 "$WORK/result.json")"
  echo "ok   $2"
}

# wait_idle: document_list until the design is idle.
wait_idle() {
  for _ in $(seq 1 120); do
    tool document_list '{}'
    jq -e '.documents[0] | .editable and (.busy | not)' "$WORK/result.json" > /dev/null && return 0
    sleep 0.5
  done
  ui_fail "the design did not become idle: $(head -c 2000 "$WORK/result.json")"
}

now_ms() { echo $(($(date +%s%N) / 1000000)); }

INTERFERENCE='{"query": {"query": "interference", "min_volume": 1}}'

ui_start_display
ui_open_in_current_window
ui_start_app --no-native-dialogs --mcp-folder "$WORK" --open "$WORK/blocks.mitcad"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
bridge_start first 7 8
BRIDGE=$LAST_PID
bridge_start second 5 6
SECOND=$LAST_PID
wait_idle
tool model_query '{"query": {"query": "bodies"}}'
expect 'length == 2' "the design has its two blocks"
MOVING=$(jq -r '.result[0].uid' "$WORK/result.json")
AGAINST=$(jq -r '.result[1].uid' "$WORK/result.json")

echo "--- 1. Interference as a job: progress, a competing request busy, the client's cancel"
ui_mark
call 7 model_query "$INTERFERENCE"
CALL=$MCP_ID
ui_expect_new "MCP: an agent's interference query runs as a job" "the interference query runs as a model job" 10
ui_expect_new "Progress dialog shown: interference" "the window shows the progress with its Cancel" 10
STARTED=$(now_ms)
call 5 model_query '{"query": {"query": "timeline"}}'
answer 6 "$MCP_ID"
ANSWERED=$(now_ms)
result true
expect '.busy == true and .retry_after_ms > 0' "the second client's query is answered busy with when to retry"
[ $((ANSWERED - STARTED)) -lt 3000 ] ||
  ui_fail "the competing query answered $((ANSWERED - STARTED)) ms later, not at once"
echo "ok   the competing query answered after $((ANSWERED - STARTED)) ms, while the job runs"
REQUESTED=$(now_ms)
send 7 "$(jq -cn --argjson id "$CALL" '{jsonrpc: "2.0", method: "notifications/cancelled",
  params: {requestId: $id, reason: "the test cancels it"}}')"
answer 8 "$CALL"
ANSWERED=$(now_ms)
result true
expect '.error | test("cancel")' "the cancelled call answers that the computation was canceled"
[ $((ANSWERED - REQUESTED)) -lt 2000 ] ||
  ui_fail "the call answered $((ANSWERED - REQUESTED)) ms after the cancellation"
echo "ok   the call answered $((ANSWERED - REQUESTED)) ms after the cancellation"
ui_expect_new "MCP: mitcad-long-query-first cancelled its running call" "the window logged the cancellation"
wait_idle
tool model_query '{"query": {"query": "bodies"}}'
expect 'length == 2' "the next query runs once the design is idle"

echo "--- 2. An assembly path cancelled with the progress dialog's Esc"
ui_mark
call 7 model_query "$(jq -cn --arg a "$MOVING" --arg b "$AGAINST" '{query: {query: "assembly_path",
  body: $a, against: [$b], slide: {direction: [0, 0, 1], distance: 0.01}, steps: 1}}')"
CALL=$MCP_ID
ui_expect_new "MCP: an agent's assembly_path query runs as a job" "the assembly path runs as a model job" 10
ui_expect_new "Progress dialog shown: assembly_path" "the progress dialog of the assembly path" 10
ui_focus_dialog '^Computing$'
ui_key Escape # not a step: it waits for the job
answer 8 "$CALL"
result true
expect '.error | test("cancel")' "the user's Cancel ends the call: the computation was canceled"
ui_expect_new "Computing assembly_path cancelled after" "the job stopped" 30
# Cancel ends a running probe within a second (mitcad#2577): from the
# cancel to the job's end, on the job's own clock.
REQUESTED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Computing assembly_path: cancel requested after \([0-9]*\) ms$/\1/p' | tail -1)
ENDED=$(tail -n +$((UI_MARK + 1)) "$UI_LOG" | sed -n 's/^Computing assembly_path cancelled after \([0-9]*\) ms$/\1/p' | tail -1)
[ -n "$REQUESTED" ] && [ -n "$ENDED" ] || ui_fail "the cancel and the job's end are not both logged"
[ $((ENDED - REQUESTED)) -lt 1000 ] || ui_fail "the probe ended $((ENDED - REQUESTED)) ms after the cancel"
echo "ok   the probe ended $((ENDED - REQUESTED)) ms after the cancel"
ui_focus_main
wait_idle
tool model_query '{"query": {"query": "bodies"}}'
expect 'length == 2' "the next query runs once the design is idle"

echo "--- 3. Interference to its end keeps its result"
tool model_query "$INTERFERENCE"
expect '.result.bodies == 2 and (.result.pairs | length) == 1' "two bodies, one interfering pair"
expect '.result.pairs[0].volume | . > 23999 and . < 24001' "the pair overlaps by 24000 mm³"
NAMED=$(jq -r '.document' "$WORK/result.json")
[ -n "$NAMED" ] && [ "$NAMED" != null ] || ui_fail "the result names no document: $(head -c 2000 "$WORK/result.json")"
echo "ok   the result names its document as before"

echo "--- 4. The Interference command runs as a job: the window answers, Cancel ends it"
ui_focus_main
ui_mark
ui_command "Interference"
ui_expect_new "Progress dialog shown: interference" "the command's check runs as a model job with the progress" 10
# The window keeps answering while the check runs: the sync key is
# handled long before the check's 15 s end (a window frozen by the check
# answers only after it), and the job still runs then.
STARTED=$(now_ms)
ui_sync
ANSWERED=$(now_ms)
ui_busy || ui_fail "the check ended before the window answered"
[ $((ANSWERED - STARTED)) -lt 8000 ] || ui_fail "the window answered $((ANSWERED - STARTED)) ms later, not while the check runs"
echo "ok   the window answered after $((ANSWERED - STARTED)) ms while the check runs"
ui_focus_dialog '^Computing$'
ui_key Escape # not a step: it waits for the job
ui_expect_new "Computing interference cancelled after" "Cancel stopped the check" 30
ui_expect_new "Inspect Interference: cancelled" "the panel shows the check as canceled, not as an error"
ui_focus_main
ui_step "close Interference (Escape)" ui_key Escape

exec 7>&- 5>&-
wait "$BRIDGE" || ui_fail "the first bridge ended with an error: $(cat "$WORK/first.err")"
BRIDGE=""
wait "$SECOND" || ui_fail "the second bridge ended with an error: $(cat "$WORK/second.err")"
SECOND=""
exec 8<&- 6<&-
ui_stop_app
echo "PASS"
