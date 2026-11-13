#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcpInput.cpp app/mcp/DialogWatch.cpp app/mcp/UiKeys.cpp app/mcp/UiCommandList.cpp
# check-all sources: app/mcp/LiveServer.cpp app/mcp/AgentFolders.cpp app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp
# check-all sources: app/framework/CommandPanel.cpp app/browser/ParametersDialog.cpp core/mcp core/ffi/src/mcp.rs tools/cli
# Live MCP's input tools (mitcad#1735) through the real UI on Xvfb and the
# actual bridge, `mitcad-cli mcp --connect`, driven from the shell (jq), in
# an application started with --mcp-folder (the tools are listed only then):
#   - ui_click on a command panel's value field by its input key (the
#     place ui_command's log gives), then ui_type with Ctrl+A and the
#     Unicode text pyörä_d (a user parameter's name): the panel's input
#     holds pyörä_d; a place outside the window and an input the panel
#     does not show are refused;
#   - ui_dialog: commands that ui_command runs, or whose dialogs read or
#     write files, are refused; open Edit Parameters, read its title,
#     buttons and the parameters' table (pyörä_d in it), ui_click and
#     ui_type refused while it is open, a whole-window ui_screenshot shows
#     it (the window's pixels where it lies change), press OK refused (it
#     would act), press Close closes it;
#   - Send to Slicer's panel opened for viewing: its inputs and buttons,
#     Send refused, Cancel closes it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-input-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

for tool in jq compare convert; do
  command -v "$tool" > /dev/null || { echo "$tool is required"; exit 2; }
done
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-mcp-input.XXXXXX)
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
ALLOWED=$WORK/allowed
mkdir -p "$ALLOWED"
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery
NAME="pyörä_d"

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs, as in
# tools/ui-mcp-files-camera-test.sh. mcp_request leaves the answer in
# $WORK/answer.json, mcp_tool the tool's structured result in
# $WORK/result.json; expect checks a jq condition on it.

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

# mcp_tool name arguments [true]: true when the tool must refuse (isError).
mcp_tool() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = "${3:-false}" ] ||
    ui_fail "$1 $2: unexpected result: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# mcp_retry name arguments: as mcp_tool, while the result is busy (the
# posted events and ui_dialog's window come right after a call).
mcp_retry() {
  local i
  for i in $(seq 1 50); do
    mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
    jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
    jq -e '.result.isError and (.result.structuredContent.busy // false)' "$WORK/answer.json" > /dev/null || break
    sleep 0.1
  done
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = false ] ||
    ui_fail "$1 $2: unexpected result: $(head -c 2000 "$WORK/answer.json")"
}

# expect 'jq condition' description [--arg name value ...]: on the last result.
expect() {
  local condition=$1 description=$2
  shift 2
  jq -e "$@" "$condition" "$WORK/result.json" > /dev/null ||
    ui_fail "$description: $(head -c 2000 "$WORK/result.json")"
  echo "ok   $description"
}

# region file x y w h out: that part of a picture, clipped to it.
region() {
  convert "$1" -crop "$4x$5+$2+$3" +repage "$6"
}

# differing a b: how many pixels of two pictures differ by more than 10 %.
differing() { compare -metric AE -fuzz 10% "$1" "$2" null: 2>&1 | sed 's/[^0-9.].*//'; }

# ---------------------------------------------------------------------------

ui_start_display
ui_start_app --mcp-folder "$ALLOWED"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-input", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
mcp_request tools/list
jq -e '[.result.tools[].name] | index("ui_click") and index("ui_type") and index("ui_dialog")' \
  "$WORK/answer.json" > /dev/null ||
  ui_fail "the catalog of a run with --mcp-folder lacks the input tools: $(jq -c '[.result.tools[].name]' "$WORK/answer.json")"
echo "ok   ui_click, ui_type and ui_dialog are listed with --mcp-folder"

mcp_tool document_list '{}'
DOC=$(jq -r '.documents[0].document' "$WORK/result.json")
mcp_tool model_command "$(jq -cn --arg d "$DOC" --arg n "$NAME" '{document: $d, command: [
  {cmd: "add_parameter", name: $n, expression: "12 mm"},
  {cmd: "add_feature", def: {type: "box", plane: {type: "origin", plane: "xy"}, corner: [0, 0],
   length: 30, width: 20, height: 10, operation: "new_body"}}]}')"

echo "--- Click a panel's field and type Unicode into it (mitcad#1735)"
mcp_tool ui_command "$(jq -cn --arg d "$DOC" '{document: $d, id: "solid.extrude", finish: "keep_open"}')"
expect '.open == true and (.panel.inputs | has("distance"))' "Extrude's panel stays open with its distance field"
# The panel logs its fields' places once it is laid out, after the answer.
ui_expect_log "Panel Extrude input distance at " "its log gives the field's place"
mcp_tool ui_click "$(jq -cn --arg d "$DOC" '{document: $d, input: "distance"}')"
expect '.posted == true and .target.field == true and .input == "distance" and (.at | length == 2)' \
  "ui_click clicks the distance field by its input key"
AT=$(jq -c '.at' "$WORK/result.json")
ui_expect_log "MCP: an agent clicked at" "the click is logged"
mcp_tool ui_type '{"keys": "Ctrl+A"}'
expect '.posted == true and .keys == ["Ctrl+A"] and .target.field == true' "Ctrl+A goes to the field"
mcp_tool ui_type "$(jq -cn --arg t "$NAME" '{text: $t}')"
expect '.posted == true and .characters == 7' "the parameter's name is typed, ö and ä included"
for _ in $(seq 1 30); do
  mcp_tool ui_state '{}'
  jq -e --arg n "$NAME" '.command.inputs.distance.text == $n' "$WORK/result.json" > /dev/null && break
  sleep 0.1
done
expect '.command.inputs.distance.text == $n' "the panel's distance holds pyörä_d" --arg n "$NAME"
mcp_tool ui_click "$(jq -cn --argjson at "$AT" '{at: $at}')"
expect '.posted == true and .target.field == true' "ui_click at the same place reaches the same field"
mcp_tool ui_click '{"at": [5, 99999]}' true
expect '.error | contains("outside the window")' "a place outside the window is refused"
mcp_tool ui_click '{"input": "no_such_input"}' true
expect '.error | contains("shows no input")' "an input the panel does not show is refused"
mcp_tool ui_type '{"keys": "Ctrl+Flurb"}' true
expect '.error | contains("not a key name")' "an unknown key name is refused"
mcp_tool ui_command "$(jq -cn --arg d "$DOC" '{document: $d, finish: "cancel"}')"
mcp_tool ui_state '{}'
expect '.command == null' "the panel is cancelled"

echo "--- Edit Parameters opened for viewing (mitcad#1735)"
mcp_tool ui_dialog '{"open": "solid.extrude"}' true
expect '.error | contains("ui_command runs it")' "a command without a dialog is ui_command's"
mcp_tool ui_dialog '{"open": "file.save_as"}' true
expect '.error | contains("files")' "a dialog that writes files is refused"
mcp_tool ui_dialog '{"read": true}' true
expect '.error | contains("No dialog is open")' "with nothing open read is refused"
mcp_tool ui_screenshot '{"window": true, "path": "before.png"}'
WIDTH=$(jq -r '.full_width' "$WORK/result.json")
HEIGHT=$(jq -r '.full_height' "$WORK/result.json")
mcp_tool ui_dialog '{"open": "solid.parameters"}'
expect '.opening == true and .command == "solid.parameters"' "ui_dialog opens Edit Parameters"
mcp_retry ui_dialog '{"read": true}'
expect '.kind == "dialog" and .title == "Parameters" and .command == "solid.parameters"' "read names the window"
expect '[.buttons[].text] | index("OK")' "read lists its buttons"
expect '[.fields[] | select(.kind == "tree" or .kind == "table") | .rows[][]] | index($n)' \
  "read lists the parameters' table with pyörä_d" --arg n "$NAME"
read -r PX PY PW PH < <(jq -r '.place | map(tostring) | join(" ")' "$WORK/result.json")
mcp_tool ui_click '{"at": [10, 10]}' true
expect '.error | contains("for viewing only")' "ui_click is refused while it is open"
mcp_tool ui_type '{"text": "x"}' true
expect '.error | contains("for viewing only")' "ui_type is refused while it is open"
mcp_tool ui_screenshot '{"window": true, "path": "parameters.png"}'
expect '.of == "window"' "a whole-window screenshot is taken with it open"
# Where the dialog lies over the window, the picture changed.
X0=$((PX > 0 ? PX : 0))
Y0=$((PY > 0 ? PY : 0))
X1=$((PX + PW < WIDTH ? PX + PW : WIDTH))
Y1=$((PY + PH < HEIGHT ? PY + PH : HEIGHT))
[ "$X1" -gt "$((X0 + 20))" ] && [ "$Y1" -gt "$((Y0 + 20))" ] ||
  ui_fail "the dialog at $PX,$PY ${PW}x$PH lies outside the window ${WIDTH}x$HEIGHT"
region "$ALLOWED/before.png" "$X0" "$Y0" "$((X1 - X0))" "$((Y1 - Y0))" "$WORK/before-region.png"
region "$ALLOWED/parameters.png" "$X0" "$Y0" "$((X1 - X0))" "$((Y1 - Y0))" "$WORK/parameters-region.png"
CHANGED=$(differing "$WORK/before-region.png" "$WORK/parameters-region.png")
AREA=$(((X1 - X0) * (Y1 - Y0)))
awk -v changed="$CHANGED" -v area="$AREA" 'BEGIN { exit !(changed * 10 > area) }' ||
  ui_fail "the window's screenshot does not show the dialog ($CHANGED of $AREA pixels changed where it lies)"
echo "ok   ui_screenshot shows the dialog ($CHANGED of $AREA pixels changed where it lies)"
mcp_tool ui_dialog '{"press": "OK"}' true
expect '.error | contains("would act on the window")' "OK is refused: it would act"
mcp_tool ui_dialog '{"press": "Close"}'
expect '.closed == true and .kind == "dialog" and .title == "Parameters"' "press Close closes it"
mcp_tool ui_dialog '{"read": true}' true
expect '.error | contains("No dialog is open")' "nothing is open after it"

echo "--- Send to Slicer's panel opened for viewing (mitcad#1735)"
mcp_tool ui_dialog "$(jq -cn --arg d "$DOC" '{document: $d, open: "make.print3d"}')"
expect '.opened == true and .kind == "panel" and .command == "make.print3d" and (.panel.inputs | length > 0)' \
  "ui_dialog opens Send to Slicer's panel with its inputs"
expect '[.buttons[] | select(.closes) | .text] == ["Cancel"]' "only its Cancel closes"
mcp_tool ui_type '{"text": "x"}' true
expect '.error | contains("for viewing only")' "ui_type is refused while the panel is open"
mcp_tool ui_dialog '{"press": "Send"}' true
expect '.error | contains("only with Cancel")' "Send is refused"
mcp_tool ui_dialog '{"press": "Cancel"}'
expect '.closed == true and .kind == "panel"' "Cancel closes the panel"
mcp_tool ui_state '{}'
expect '.command == null' "no command is open after it"

bridge_close
ui_crashed && ui_fail "the application crashed"
ui_finish "UI live MCP input test"
