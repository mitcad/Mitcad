#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp app/mcp core/mcp core/ffi/src/mcp.rs
# check-all sources: app/WindowManager.cpp app/projects/MainWindowProjectsPanel.cpp core/vcs tools/cli
# Live MCP's document_new (mitcad#1266) through the real UI on Xvfb and the
# actual bridge, `mitcad-cli mcp --connect`, driven from the shell (jq). The
# application opens a design of a project inside the folder the run allows
# (--mcp-folder):
#   - ui_command file.new_design is refused and points to document_new;
#   - a folder outside the allowed one, '..', a name with a folder and a
#     name that exists (named in the refusal) are refused, and nothing opens;
#   - document_new without arguments makes Design1.mitcad in the current
#     project's top folder, recorded as New Design records it (the
#     project's version "Add Design1.mitcad"), and opens it beside the open
#     design: document_list lists both, the new one active, the old handle
#     still valid;
#   - a model command on the new handle, and one without a handle, change
#     only the new design; the first design stays unchanged;
#   - with a name and a folder outside projects the empty design is saved
#     there; from a loose design without a project an untitled design; a
#     name without a folder or project goes to the allowed folder.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-document-new-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

for tool in jq git; do
  command -v "$tool" > /dev/null || { echo "$tool is required"; exit 2; }
done
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-mcp-new.XXXXXX)
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
export HOME=$WORK/home
mkdir -p "$HOME"
export GIT_CONFIG_GLOBAL=$WORK/gitconfig
export GIT_CONFIG_NOSYSTEM=1
unset EMAIL GIT_AUTHOR_NAME GIT_AUTHOR_EMAIL GIT_COMMITTER_NAME GIT_COMMITTER_EMAIL GIT_DIR GIT_WORK_TREE MITCAD_GIT
printf '[user]\n\tname = Ada Tester\n\temail = ada@example.invalid\n[init]\n\tdefaultBranch = main\n' \
  > "$GIT_CONFIG_GLOBAL"
AUTHOR="Ada Tester <ada@example.invalid>"
export MITCAD_PROJECTS_DIR=$WORK/projects
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery
ALLOWED=$WORK/allowed
OUTSIDE=$WORK/outside
SHOP=$ALLOWED/shop
mkdir -p "$ALLOWED/loose" "$OUTSIDE" "$MITCAD_PROJECTS_DIR"

cli() {
  "$CLI" "$@" >> "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli $*"; }
}

# A project with one design, a block 10 x 60 x 5 mm (3000 mm³).
printf '%s\n' '[{"cmd": "add_feature", "def": {"type": "box", "plane": "xy", "corner": [0, 0],' \
  '  "length": 10, "width": 60, "height": 5, "operation": "new_body"}}]' > "$WORK/block.json"
cli project init "$SHOP" --author "$AUTHOR"
cli run "$WORK/block.json" --save "$SHOP/block.mitcad"
cli version save "$SHOP/block.mitcad" -m "Add block" --author "$AUTHOR"

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs (as ui-mcp-files-camera-test.sh).

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

# wait_editable handle: document_list until that document takes commands.
wait_editable() {
  for _ in $(seq 1 120); do
    mcp_tool document_list '{}'
    jq -e --arg d "$1" '.documents[] | select(.document == $d) | .editable and (.busy | not)' \
      "$WORK/result.json" > /dev/null && return 0
    sleep 0.5
  done
  ui_fail "the document $1 did not become editable: $(head -c 2000 "$WORK/result.json")"
}

# volume handle: the total volume of the document's bodies, in mm³.
volume() {
  mcp_tool model_query "$(jq -cn --arg d "$1" '{document: $d, query: {query: "bodies", properties: true}}')"
  jq -r '[.result[].volume] | add // 0 | round' "$WORK/result.json"
}

# box x size: a box command at x, size on each side.
box() {
  jq -cn --argjson x "$1" --argjson s "$2" '{cmd: "add_feature", def: {type: "box", plane: "xy",
    corner: [$x, 0], length: $s, width: $s, height: $s, operation: "new_body"}}'
}

# ---------------------------------------------------------------------------

ui_start_display
ui_start_app --mcp-folder "$ALLOWED" --open "$SHOP/block.mitcad"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-document-new", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
jq -e '.result.instructions | contains("document_new")' "$WORK/answer.json" > /dev/null ||
  ui_fail "the server instructions do not name document_new"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
mcp_request tools/list
jq -e '[.result.tools[].name] | index("document_new")' "$WORK/answer.json" > /dev/null ||
  ui_fail "the live catalog lacks document_new: $(jq -c '[.result.tools[].name]' "$WORK/answer.json")"

echo "--- Refusals: New Design by ui_command, folders, names"
mcp_tool document_list '{}'
expect '(.documents | length == 1) and (.documents[0].path | endswith("/shop/block.mitcad"))' \
  "the project's design is open"
FIRST=$(jq -r '.documents[0].document' "$WORK/result.json")
wait_editable "$FIRST"
mcp_tool ui_command "$(jq -cn --arg d "$FIRST" '{document: $d, id: "file.new_design"}')" true
expect '.error | contains("document_new")' "ui_command file.new_design points to document_new"
mcp_tool document_new "$(jq -cn --arg f "$OUTSIDE" '{folder: $f}')" true
expect '.error | contains("not in a folder for designs")' "a folder outside the allowed one is refused"
mcp_tool document_new '{"folder": "../outside"}' true
expect '.error | contains("..")' "'..' is refused"
mcp_tool document_new '{"name": "sub/part"}' true
expect '.error | contains("not a design'"'"'s name")' "a name with a folder is refused"
mcp_tool document_new '{"name": "block"}' true
expect '(.error | contains("block.mitcad")) and (.error | contains("already exists"))' \
  "a name that exists in the project is refused, naming it"
mcp_tool document_list '{}'
expect '.documents | length == 1' "no refused call opened a design"
[ -e "$OUTSIDE/Design1.mitcad" ] && ui_fail "a refused call wrote outside the folder"

echo "--- A new design in the current project, beside the open one"
ui_mark
mcp_tool document_new '{}'
expect '.created == true and .recorded == true and (.path | endswith("/shop/Design1.mitcad"))
  and .document != $first and .previous == $first' \
  "document_new makes Design1 in the project's top folder with a new handle" --arg first "$FIRST"
SECOND=$(jq -r '.document' "$WORK/result.json")
ui_expect_new "/shop/Design1.mitcad in window-" "the application made it beside the open design"
[ -s "$SHOP/Design1.mitcad" ] || ui_fail "Design1.mitcad was not written"
SUBJECT=$(git -C "$SHOP" log -1 --format=%s 2>&1)
[[ "$SUBJECT" == *"Design1.mitcad"* ]] || ui_fail "the project did not record the new design: $SUBJECT"
echo "ok   the project recorded it as a version ($SUBJECT)"
mcp_tool document_list '{}'
expect '([.documents[].document] | sort) == ([$a, $b] | sort)' "document_list lists both designs" \
  --arg a "$FIRST" --arg b "$SECOND"
expect '[.documents[] | select(.active)][0].document == $b' "the new design is the active one" --arg b "$SECOND"
wait_editable "$SECOND"

echo "--- Commands change only the new design"
mcp_tool model_command "$(jq -cn --arg d "$SECOND" --argjson c "$(box 0 10)" '{document: $d, command: $c}')"
expect '.applied == true and .document == $b' "a model command on the new handle runs there" --arg b "$SECOND"
mcp_tool model_command "$(jq -cn --argjson c "$(box 20 5)" '{command: $c}')"
expect '.applied == true and .document == $b' "a command without a handle goes to the new design" --arg b "$SECOND"
[ "$(volume "$SECOND")" = 1125 ] || ui_fail "the new design does not hold the two boxes: $(cat "$WORK/result.json")"
echo "ok   the new design holds the two boxes (1125 mm³)"
[ "$(volume "$FIRST")" = 3000 ] || ui_fail "the first design changed: $(cat "$WORK/result.json")"
echo "ok   the first design's handle still reads it, unchanged (3000 mm³)"
mcp_tool ui_state "$(jq -cn --arg d "$FIRST" '{document: $d}')"
expect '.modified == false' "the first design has no unsaved changes"

echo "--- Outside projects: a named file, an untitled design, the allowed folder"
mcp_tool document_new '{"name": "loose", "folder": "loose"}'
expect '.recorded == false and (.path | endswith("/allowed/loose/loose.mitcad"))' \
  "a name in a folder outside projects saves the empty design there"
THIRD=$(jq -r '.document' "$WORK/result.json")
[ -s "$ALLOWED/loose/loose.mitcad" ] || ui_fail "loose.mitcad was not written"
wait_editable "$THIRD"
mcp_tool document_new "$(jq -cn --arg d "$THIRD" '{document: $d}')"
expect '.path == null and .recorded == false and .previous == $c' \
  "from a design without a project, an untitled design" --arg c "$THIRD"
FOURTH=$(jq -r '.document' "$WORK/result.json")
wait_editable "$FOURTH"
mcp_tool document_new "$(jq -cn --arg d "$FOURTH" '{document: $d, name: "nipple"}')"
expect '.path | endswith("/allowed/nipple.mitcad")' "a name without a folder or project goes to the allowed folder"
[ -s "$ALLOWED/nipple.mitcad" ] || ui_fail "nipple.mitcad was not written"
mcp_tool document_list '{}'
expect '.documents | length == 5' "five designs are open"
[ "$(volume "$FIRST")" = 3000 ] || ui_fail "the first design changed"

bridge_close
ui_crashed && ui_fail "the application crashed"
ui_finish "UI live MCP document_new test"
