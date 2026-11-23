#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowMcp.cpp app/mcp app/view/ViewController.cpp app/framework/SettingsTransfer.cpp
# check-all sources: core/mcp core/ffi/src/mcp.rs core/model-api/src/classes.rs tools/cli
# check-all sources: core/model/src/print_library.rs core/model/src/document/library_items.rs
# The print libraries for live MCP agents (mitcad#2224) through the live
# bridge (mitcad-cli mcp --connect), with My library in the test's own
# folder (MITCAD_PRINT_LIBRARY_DIR), which mitcad-cli gives a printer,
# Bench X, and on a design outside a project (a 30 x 20 x 10 block):
#   1. Preferences › AI Assistants › Agents may use my print libraries on
#      (the setting ai/agentsMayUsePrintLibraries): library_items lists
#      Bench X in My library; set_property with library user chooses it for
#      the design (the root's printer, its copy in the design), one undo
#      step labelled (agent); a call whose project names a folder is
#      refused in words (the design is in no project); slicer_presets stays
#      refused; a material the agent makes in the design is saved to My
#      library with save_to_print_library, its file written, and undo
#      removes the file.
#   2. The preference off (the app started again): library_items,
#      set_property with library and save_to_print_library are refused,
#      the refusal naming the preference; the design's own items still
#      work.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-mcp-print-libraries-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-mcp-print-libraries.XXXXXX")
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
FILE=$WORK/design.mitcad
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery
# My library in the test's own folder.
export MITCAD_PRINT_LIBRARY_DIR=$WORK/print-library
LIB=$MITCAD_PRINT_LIBRARY_DIR
PREFERENCE="Preferences › AI Assistants › Agents may use my print libraries"

cat > "$WORK/model.json" << EOF
[
  {"cmd": "create_print_profile", "library": "user", "kind": "printer", "name": "Bench X",
   "values": {"bed_shape": [[0, 0], [220, 0], [220, 220], [0, 220]], "max_print_height": 250,
              "nozzle_diameter": 0.4}},
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 30, "height": 20},
  {"cmd": "add_feature", "def": {"type": "extrude", "profiles": [{"sketch": "F1", "region": "$R"}],
    "extent": {"type": "distance", "distance": 10}, "operation": "new_body"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }
[ -f "$LIB/print/printers/bench-x.json" ] || ui_fail "Bench X is not in My library"

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs (as ui-comments-test.sh).
# mcp_request leaves the answer in $WORK/answer.json, mcp_tool the tool's
# structured result in $WORK/result.json.

# Each connection has FIFOs of its own (BRIDGES counts them).
MCP_ID=0
BRIDGES=0
bridge_open() {
  BRIDGES=$((BRIDGES + 1))
  mkfifo "$WORK/to-bridge-$BRIDGES" "$WORK/from-bridge-$BRIDGES"
  "$CLI" mcp --connect < "$WORK/to-bridge-$BRIDGES" > "$WORK/from-bridge-$BRIDGES" 2> "$WORK/bridge.err" &
  BRIDGE=$!
  exec 7> "$WORK/to-bridge-$BRIDGES"
  exec 8< "$WORK/from-bridge-$BRIDGES"
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

# mcp_call name arguments: the tool's structured result in result.json.
mcp_call() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  jq -c '.result.structuredContent' "$WORK/answer.json" > "$WORK/result.json"
}

# mcp_tool name arguments: a result that must not be an error.
mcp_tool() {
  mcp_call "$1" "$2"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = false ] ||
    ui_fail "$1 $2: refused: $(head -c 2000 "$WORK/answer.json")"
}

# mcp_refused name arguments text description: an error result whose
# error contains the text.
mcp_refused() {
  mcp_call "$1" "$2"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = true ] ||
    ui_fail "$4: not refused: $(head -c 2000 "$WORK/answer.json")"
  jq -e --arg text "$3" '.error | tostring | contains($text)' "$WORK/result.json" > /dev/null ||
    ui_fail "$4: the refusal does not say '$3': $(head -c 2000 "$WORK/result.json")"
  echo "ok   $4"
}

# expect_result filter description: a jq filter on result.json that must
# be true.
expect_result() {
  jq -e "$1" "$WORK/result.json" > /dev/null || ui_fail "$2: $1 is not true in $(head -c 2000 "$WORK/result.json")"
  echo "ok   $2"
}

connect() {
  bridge_open
  mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
    "clientInfo": {"name": "mitcad-print-libraries-test", "version": "1"}}'
  jq -e '.result.protocolVersion' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
  mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
  mcp_tool document_list '{}'
  DOC=$(jq -r '.documents[0].document' "$WORK/result.json")
  [ -n "$DOC" ] && [ "$DOC" != null ] || ui_fail "no document from document_list: $(cat "$WORK/result.json")"
}

# ---------------------------------------------------------------------------

echo "--- 1. The preference on: the agent uses My library"
mkdir -p "$UI_CONFIG/Mitcad"
printf '\n[ai]\nagentsMayUsePrintLibraries=true\n' >> "$UI_CONFIG/Mitcad/Mitcad.conf"
ui_start_display
ui_start_app --mcp-folder "$WORK" --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
connect

mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "library_items", kind: "printer"}}')"
expect_result '.result.items | any(.id == "bench-x" and .place == "user" and .library == "user")' \
  "library_items lists Bench X in My library"
expect_result '.result.items | any(.id == "bench-x" and .in_design == null)' "the design has no copy of it yet"

mcp_tool model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "set_property", owner: "C0",
  set: "Print Printer", key: "profile", value: "bench-x", library: "user"}}')"
expect_result '.applied == true' "set_property chooses Bench X from My library"
mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "library_items", kind: "printer"}}')"
expect_result '.result.items | any(.id == "bench-x" and .in_design != null and .chosen == true)' \
  "the design's root chose its copy of Bench X"
mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "document"}}')"
expect_result '.result.undo | endswith("(agent)")' "one undo step labelled (agent)"

mcp_refused model_query "$(jq -cn --arg d "$DOC" --arg p "$WORK/other" \
  '{document: $d, query: {query: "library_items", kind: "printer", project: $p}}')" \
  "this design is in no project" "a project folder the design is not in is refused"
mcp_refused model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "slicer_presets", dialect: "orcaslicer"}}')" \
  "not available through MCP" "slicer_presets stays refused"

mcp_tool model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "create_material", name: "Agent PLA",
  density: 1.24}}')"
MATERIAL=$(jq -r '.result.id' "$WORK/result.json")
[ -n "$MATERIAL" ] && [ "$MATERIAL" != null ] || ui_fail "no material id: $(cat "$WORK/result.json")"
echo "ok   a material of the design's own: $MATERIAL"
mcp_tool model_command "$(jq -cn --arg d "$DOC" --arg id "$MATERIAL" '{document: $d, command: {cmd: "save_to_print_library",
  library: "user", kind: "material", id: $id}}')"
SAVED=$(jq -r '.result.path' "$WORK/result.json")
[ -n "$SAVED" ] && [ -f "$SAVED" ] || ui_fail "save_to_print_library wrote no file: $(cat "$WORK/result.json")"
case $SAVED in
  "$LIB"/*) echo "ok   save_to_print_library wrote $SAVED in My library" ;;
  *) ui_fail "the saved item is not in My library: $SAVED" ;;
esac
mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "document"}}')"
expect_result '.result.undo | endswith("to Library (agent)")' "the save is an undo step labelled (agent)"
mcp_tool model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "undo"}}')"
[ ! -e "$SAVED" ] || ui_fail "undo left the saved item's file: $SAVED"
echo "ok   undo removed the saved item's file"
bridge_close
ui_stop_app

echo "--- 2. The preference off: the print libraries are refused, naming it"
sed -i 's/^agentsMayUsePrintLibraries=true$/agentsMayUsePrintLibraries=false/' "$UI_CONFIG/Mitcad/Mitcad.conf"
grep -q '^agentsMayUsePrintLibraries=false$' "$UI_CONFIG/Mitcad/Mitcad.conf" ||
  ui_fail "the preference is not off in the settings"
# ui_start_app writes a fresh log: the line is looked for in the whole of it.
ui_start_app --mcp-folder "$WORK" --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up again"
connect
mcp_refused model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "library_items", kind: "printer"}}')" \
  "$PREFERENCE" "library_items is refused, naming the preference"
mcp_refused model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "set_property", owner: "C0",
  set: "Print Printer", key: "profile", value: "bench-x", library: "user"}}')" \
  "$PREFERENCE" "set_property with library is refused, naming the preference"
mcp_tool model_command "$(jq -cn --arg d "$DOC" '{document: $d, command: {cmd: "create_material", name: "Agent PETG"}}')"
MATERIAL=$(jq -r '.result.id' "$WORK/result.json")
echo "ok   the design's own material still made: $MATERIAL"
mcp_refused model_command "$(jq -cn --arg d "$DOC" --arg id "$MATERIAL" '{document: $d, command: {cmd: "save_to_print_library",
  library: "user", kind: "material", id: $id}}')" \
  "$PREFERENCE" "save_to_print_library is refused, naming the preference"
[ -z "$(ls "$LIB/materials" 2> /dev/null)" ] || ui_fail "My library has materials: $(ls "$LIB/materials")"
echo "ok   nothing was written to My library"
bridge_close
ui_stop_app
echo "PASS"
