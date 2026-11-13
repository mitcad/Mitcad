#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp app/mcp core/mcp core/ffi/src/mcp.rs
# check-all sources: app/files/Recovery.cpp app/files/MainWindowAutosave.cpp app/files/Autosave.cpp tools/cli
# Live MCP while a dialog waits for the user (mitcad#1725), through the real
# UI on Xvfb and the actual bridge, `mitcad-cli mcp --connect`, driven from
# the shell (jq, two FIFOs, as ui-mcp-document-new-test.sh):
#   - a killed session leaves its autosave; MitCad started again without
#     --no-recovery (and with --mcp-folder) asks "Recover Unsaved Work";
#   - while it is open, ui_state lists the dialog (its title, asked at
#     start-up) with the reason edits wait; model_command, ui_command and
#     document_list's blocked_reason name the dialog and point to
#     --no-recovery; queries still work;
#   - answered (Later), the design takes commands again and ui_state lists
#     no dialog;
#   - File > Recover Designs asks the same question by the user's choice:
#     the refusal names it without the start-up's --no-recovery hint.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-recovery-dialog-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-mcp-recovery.XXXXXX)
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
export MITCAD_TEST_AUTOSAVE_SECONDS=2
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery
TITLE="Recover Unsaved Work"

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs (as ui-mcp-document-new-test.sh).

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

# mcp_tool name arguments [true]: true when the tool must refuse (isError).
mcp_tool() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = "${3:-false}" ] ||
    ui_fail "$1 $2: unexpected result: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# expect 'jq condition' description [--arg name value ...]: on the last result.
expect() {
  local condition=$1 description=$2
  shift 2
  jq -e "$@" "$condition" "$WORK/result.json" > /dev/null ||
    ui_fail "$description: $(head -c 2000 "$WORK/result.json")"
  echo "ok   $description"
}

# wait_dialog 'jq condition' description: ui_state until its dialog meets it.
wait_dialog() {
  for _ in $(seq 1 120); do
    mcp_tool ui_state '{}'
    jq -e "$1" "$WORK/result.json" > /dev/null && { echo "ok   $2"; return 0; }
    sleep 0.5
  done
  ui_fail "$2: $(head -c 2000 "$WORK/result.json")"
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

BOX='{"command": {"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],
  "length": 10, "width": 10, "height": 10, "operation": "new_body"}}}'

# ---------------------------------------------------------------------------

ui_start_display

echo "--- A killed session leaves its unsaved work"
ui_start_app --demo --set d3=30
ui_expect_log "Autosaved Untitled: " "the changed design was autosaved"
ui_stop_app

echo "--- Started again with an agent: the recovery question at start-up"
UI_RECOVERY=1 ui_start_app --mcp-folder "$WORK"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_expect_log "Recovery: 1 document(s) found" "the killed session is offered"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-recovery-dialog", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
mcp_request tools/list
jq -e '.result.tools[] | select(.name == "ui_state") | .description | contains("--no-recovery")' \
  "$WORK/answer.json" > /dev/null || ui_fail "ui_state's description does not mention --no-recovery"
echo "ok   ui_state's description points to --no-recovery"

wait_dialog '.dialog.title == "Recover Unsaved Work"' "ui_state lists the open dialog by its title"
expect '.dialog.recovery_at_start == true' "as the recovery question of the start-up"
expect '.editable == false and (.blocked_reason | contains($t)) and (.blocked_reason | contains("--no-recovery"))' \
  "ui_state says why edits wait, naming the dialog and --no-recovery" --arg t "$TITLE"

ui_mark
mcp_tool model_command "$BOX" true
expect '(.error | contains($t)) and (.error | contains("--no-recovery"))' \
  "model_command is refused, naming the dialog and --no-recovery" --arg t "$TITLE"
ui_expect_new "MCP refused: The dialog \"$TITLE\" is open in MitCad" "the refusal is logged"
mcp_tool ui_command '{"id": "sketch.finish"}' true
expect '(.error | contains($t)) and (.error | contains("--no-recovery"))' \
  "ui_command is refused the same way" --arg t "$TITLE"
mcp_tool document_list '{}'
expect '.documents[0].editable == false and (.documents[0].blocked_reason | contains($t))' \
  "document_list's blocked_reason names the dialog" --arg t "$TITLE"
mcp_tool model_query '{"query": {"query": "bodies"}}'
expect '.result | type == "array"' "queries work while the dialog is open"

echo "--- Answered by the user: the design takes commands"
ui_focus_dialog "^$TITLE\$"
ui_mark
ui_step "later (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Recovery: later, 1 document(s) kept" "Later keeps the work"
wait_dialog '.dialog == null' "ui_state lists no dialog"
expect '(has("blocked_reason") | not)' "and no reason to wait"
wait_editable
mcp_tool model_command "$BOX"
expect '.applied == true' "model_command runs"

echo "--- File > Recover Designs: the same question, asked by the user"
ui_mark
ui_step "recover designs (search)"       ui_command "Recover Designs"
ui_expect_new "Recovery: 1 document(s) found" "the work is offered again"
wait_dialog '.dialog.title == "Recover Unsaved Work" and .dialog.recovery_at_start == false' \
  "ui_state lists the dialog, not as the start-up's question"
mcp_tool model_command "$BOX" true
expect '(.error | contains($t)) and (.error | contains("--no-recovery") | not)' \
  "the refusal names the dialog without the start-up's --no-recovery hint" --arg t "$TITLE"
ui_focus_dialog "^$TITLE\$"
ui_mark
ui_step "later (Esc)"                    ui_key Escape
ui_focus_main
ui_expect_new "Recovery: later, 1 document(s) kept" "Later keeps it again"
wait_dialog '.dialog == null' "no dialog is listed after it"

bridge_close
ui_crashed && ui_fail "the application crashed"
ui_finish "UI live MCP recovery dialog test"
