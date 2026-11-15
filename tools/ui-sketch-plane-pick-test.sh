#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowVisibility.cpp app/MainWindowMcp.cpp app/MainWindowMcpUi.cpp
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/ModeBanner.cpp app/view/ModeBanner.hpp
# check-all sources: app/browser/BrowserController.cpp app/browser/DocumentHost.hpp app/framework/FloatingLayout.cpp
# check-all sources: app/mcp core/mcp core/ffi/src/mcp.rs tools/cli
# Create Sketch waiting for a plane (mitcad#474), through the real UI, in
# the Docked and the Floating layout, on a block and a ball:
#   - the view's banner "Create Sketch: pick a plane, a planar face or a
#     cylindrical or conical face. Esc cancels." shows while the mode lasts
#     and below the cards floating over the view's top; the MCP's ui_state
#     reports it as its hint, the status line beside it;
#   - a click on the ball, which cannot take a sketch, says so (neither
#     planar nor cylindrical or conical, a spherical surface) and logs
#     "Create Sketch: refused pick"; the banner stays and no sketch starts;
#   - the browser's Design Policy... is refused with what the window waits
#     for, not "Finish the command or the sketch first.";
#   - Esc ends the mode and the banner; a planar face still starts the
#     sketch after a refused pick.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-plane-pick-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-plane-pick.XXXXXX)
FILE=$WORK/block-cylinder.mitcad
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

BANNER="Create Sketch: pick a plane, a planar face or a cylindrical or conical face. Esc cancels."
WAITING="Create Sketch is waiting for a plane: pick one or press Esc."

# A block (F1.b0) in the root and beside it a ball (F2.b0) centred on xy
# in the component Pin (O1). The origin planes are shown for the pick in
# every design (mitcad#844), behind the model: a click on the ball is
# refused, also where a plane lies behind it, never picked through it (a
# cylinder's side takes a 2D sketch since mitcad#1768, so the refused face
# is a sphere's). The root's sketches may lie on Pin's faces (mitcad#220).
cat > "$WORK/model.json" << 'EOF'
[{"cmd": "add_feature", "def": {"type": "box", "plane": {"type": "origin", "plane": "xy"},
  "corner": [0, 0], "length": 40, "width": 40, "height": 20, "operation": "new_body"}},
 {"cmd": "create_component", "name": "Pin"},
 {"cmd": "add_feature", "def": {"type": "sphere", "plane": "xy", "center": [80, 20],
  "diameter": 30, "operation": "new_body"}},
 {"cmd": "activate_component", "component": "Root"},
 {"cmd": "set_policy", "component": "Pin", "policy": {"attachments_from_others": true}},
 {"cmd": "set_policy", "component": "Root", "policy": {"attach_to_others": true}}]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

# ---------------------------------------------------------------------------
# The MCP bridge (as tools/ui-mcp-files-camera-test.sh): JSON-RPC lines
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

# expect 'jq condition' description [--arg name value ...]: on the last result.
expect() {
  local condition=$1 description=$2
  shift 2
  jq -e "$@" "$condition" "$WORK/result.json" > /dev/null ||
    ui_fail "$description: $(head -c 2000 "$WORK/result.json")"
  echo "ok   $description"
}

# expect_none "text" description: not logged since the mark.
expect_none() {
  tail -n +$((UI_MARK + 1)) "$UI_LOG" | grep -qF -- "$1" && ui_fail "$2: '$1' logged"
  echo "ok   $2"
}

# start_pick layout: Create Sketch with nothing selected, and its banner.
start_pick() {
  ui_mark
  ui_step "Create Sketch (search)"         ui_command "Create Sketch"
  ui_expect_new "Create Sketch: select a plane" "$1: Create Sketch asks for a plane"
  ui_expect_new "View banner: $BANNER" "$1: the banner says what it waits for"
  ui_expect_new "View banner at " "$1: the banner is placed"
  ui_sync
}

# refused_sphere layout: a click on the ball says why it cannot take the
# sketch, and the mode goes on.
refused_sphere() {
  ui_expect_new "Pick refused face F2.b0/" "$1: the ball is where a click hits it"
  ui_mark
  ui_step "click the ball"                 ui_click_pick "refused face F2.b0/"
  ui_expect_new "Create Sketch: refused pick face " "$1: the refused pick logged"
  ui_expect_new "is neither planar nor cylindrical or conical (a spherical surface); pick a plane or a planar, cylindrical or conical face, or make a plane with Construct." \
    "$1: the reason shown"
  ui_sync
  expect_none "Sketch started" "$1: no sketch started"
  expect_none "View banner hidden" "$1: the banner stays"
}

echo "=== Docked"
ui_start_display
ui_start_app --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Bodies shown: " "the bodies shown"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-plane-pick", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'

start_pick Docked
mcp_tool ui_state '{}'
expect '.mode == "pick_plane" and .hint == $banner and .hint_is_error == false' \
  "ui_state's hint is the banner's text while the mode lasts" --arg banner "$BANNER"
refused_sphere Docked
mcp_tool ui_state '{}'
expect '.mode == "pick_plane" and .hint == $banner and (.status_line | contains("is neither planar")) and .status_line_is_error' \
  "after the refused pick too, the refusal in its status line" --arg banner "$BANNER"

echo "--- A browser action blocked by the pick"
ui_mark
ui_step "right-click the root"           ui_click_logged "Browser Root" 3
ui_expect_new "Context menu: " "the root's context menu"
ui_step "Design Policy..."               ui_menu_choose "Design Policy..."
ui_expect_new "$WAITING" "Design Policy... is refused with what the window waits for"
expect_none "Finish the command or the sketch first." "not the command's or the sketch's refusal"
expect_none "Design Policy dialog opened" "no dialog"
expect_none "View banner hidden" "the banner stays"

echo "--- Esc"
ui_mark
ui_step "cancel (Esc)"                   ui_key Escape
ui_expect_new "Create Sketch cancelled" "Esc ends the mode"
ui_expect_new "View banner hidden" "and the banner"
mcp_tool ui_state '{}'
expect '.mode == "idle" and .hint != $banner and (has("status_line") | not)' "ui_state: idle again" \
  --arg banner "$BANNER"
bridge_close
ui_crashed && ui_fail "the application crashed"
ui_stop_app

echo "=== Floating"
ui_start_app --open "$FILE" --chrome=floating
ui_step "fit (F6)"                       ui_key F6
ui_expect_log "Floating cards: " "the floating cards laid out"
start_pick Floating
rows=$(grep '^Floating cards: ' "$UI_LOG" | tail -1 | sed -n 's/.*floating rows bottom \([0-9]*\);.*/\1/p')
top=$(grep -o 'View banner at [0-9]*,[0-9]*' "$UI_LOG" | tail -1 | sed 's/.*,//')
[ -n "$rows" ] && [ -n "$top" ] || ui_fail "Floating: no rows' bottom ($rows) or banner place ($top) logged"
[ "$top" -ge "$rows" ] || ui_fail "Floating: the banner at y $top is under the floating rows (bottom $rows)"
echo "ok   Floating: the banner below the rows over the view (y $top, rows end at $rows)"
refused_sphere Floating
ui_mark
ui_step "click a face of the block"      ui_click_pick "face F1.b0/"
ui_expect_new "Sketch started on " "Floating: a planar face still starts the sketch"
ui_expect_new "View banner hidden" "Floating: the banner goes with the mode"
ui_step "finish the sketch (Ctrl+Enter)" ui_key ctrl+Return
ui_crashed && ui_fail "the application crashed"

ui_finish "UI Create Sketch plane pick test"
