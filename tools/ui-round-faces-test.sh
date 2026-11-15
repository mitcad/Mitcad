#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/commands/CommandSupport.cpp app/OcctViewer.cpp
# check-all sources: geometry/src/datum.cpp core/model/src/features/construction core/model/src/sheet tools/cli
# check-all sources: app/MainWindowMcpUi.cpp core/model-api/src/face_lists.rs core/mcp core/ffi/src/mcp.rs
# A hole cut from a sketch circle as a round face through the real UI
# (mitcad#1855). Design (mitcad-cli): rules t = 2, r = 3, K = 0.3, the
# reference side the front; a 40 x 20 Face (F3.b0) with a 20 mm flange
# folded up at x = 40 (F4: its front at x = 40, its back at x = 38); a
# circle of diameter 8 at (y, z) = (10, 15) sketched on the flange's front
# and cut through all (F6); a Hole of diameter 3 in the flange (F7), which
# rebuilds the sheet: the cut's wall is then the sheet's B-spline wall
# F4:sheet_side0(F6:sheet_curve(8)) and its rims B-spline circles. Before
# mitcad#1855 a pick called that wall no cylinder and its rims no circles,
# so Axis Through Cylinder/Cone/Torus, Point at Center and the joints
# refused them, while Measure took the wall.
#   1. Axis Through Cylinder/Cone/Torus on the cut's wall: the axis through
#      (39, 10, 15), the middle of the flange's thickness, along X.
#   2. Point at Center of Circle/Sphere/Torus on the wall's rim on the
#      flange's front: (40, 10, 15).
#   3. Through live MCP (mitcad#1863), the actual bridge `mitcad-cli mcp
#      --connect` driven from the shell (jq): the `faces` query calls the
#      wall a cylinder and the `edges` query its rims circles; ui_command
#      takes the wall for Axis Through Cylinder/Cone/Torus and the rim for
#      Point at Center of Circle/Sphere/Torus given without a geometry
#      field (the application then reads their kinds from those queries,
#      which called them bspline before): Axis2 and Point2 as Axis1 and
#      Point1.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-round-faces-test.sh [screenshot.png]

source "$(dirname "$0")/ui-test-lib.sh"

command -v jq > /dev/null || { echo "jq is required"; exit 2; }
SHOT=${1:-}
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
WORK=$(mktemp -d /tmp/mitcad-ui-round-faces.XXXXXX)
FILE=$WORK/flange.mitcad
BRIDGE=""
trap '[ -n "$BRIDGE" ] && kill "$BRIDGE" 2> /dev/null; ui_cleanup; rm -rf "$WORK"' EXIT
mkdir -p "$WORK/allowed"
# The listener's record in a private folder of the test's own.
export MITCAD_MCP_DISCOVERY_DIR=$WORK/discovery

# ---------------------------------------------------------------------------
# The bridge: JSON-RPC lines through two FIFOs, as in
# tools/ui-mcp-input-test.sh. mcp_request leaves the answer in
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

# mcp_tool name arguments: the tool must answer without an error.
mcp_tool() {
  mcp_request tools/call "$(jq -cn --arg name "$1" --argjson arguments "$2" '{name: $name, arguments: $arguments}')"
  jq -e 'has("error") | not' "$WORK/answer.json" > /dev/null ||
    ui_fail "$1: protocol error: $(head -c 2000 "$WORK/answer.json")"
  [ "$(jq -r '.result.isError // false' "$WORK/answer.json")" = false ] ||
    ui_fail "$1 $2: refused: $(head -c 2000 "$WORK/answer.json")"
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

# ---------------------------------------------------------------------------

RECT='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'
cat > "$WORK/model.json" << EOF
[
  {"cmd":"add_feature","def":{"type":"sheet_rules","thickness":2,"bend_radius":3,"allowance":{"type":"k_factor","k":0.3},"reference_side":"front","bend_relief":{"type":"rectangular","width":2,"depth":3},"corner_relief":{"type":"round","width":2,"depth":3}}},
  {"cmd":"sketch.create"},
  {"cmd":"sketch.add_rectangle","sketch":"F2","corner":[0,0],"width":40,"height":20},
  {"cmd":"add_feature","def":{"type":"sheet_face","rules":"F1","profiles":[{"sketch":"F2","region":"$RECT"}]}},
  {"cmd":"add_feature","def":{"type":"sheet_flange","body":"F3.b0","boundary":{"panel":{"feature":"F3","slot":0},"curve":{"type":"sketch","sketch":"F2","segment":"c2[c1,c3]"}},"angle":1.5707963267948966,"length":{"mode":"tangent","value":20},"position":"inside"}},
  {"cmd":"sketch.create","plane":{"face":"F4:sheet_front(0)","body":"F3.b0"}},
  {"cmd":"sketch.add_circle","sketch":"F5","center":[10,15],"diameter":8},
  {"cmd":"add_feature","def":{"type":"extrude","profiles":[{"sketch":"F5","region":"r{c1}"}],"extent":{"type":"through_all"},"flip":true,"operation":"cut"}},
  {"cmd":"add_feature","def":{"type":"hole","placement":{"type":"face","body":"F3.b0","face":"F4:sheet_front(0)","points":[[40,15,8]]},"diameter":3,"extent":{"type":"through_all"}}},
  {"expect":{"feature":"F7","status":"ok"}},
  {"expect":{"body":"F3.b0","has_face":"F4:sheet_side0(F6:sheet_curve(8))"}}
]
EOF
"$CLI" run "$WORK/model.json" --save "$FILE" > "$WORK/cli.log" 2>&1 || { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

WALL_NAME='F4:sheet_side0(F6:sheet_curve(8))'
RIM_NAME='E{F4:sheet_front(0)|F4:sheet_side0(F6:sheet_curve(8))}'
WALL="face F3.b0/$WALL_NAME"
RIM="edge F3.b0/$RIM_NAME"

ui_start_display
ui_start_app --mcp-folder "$WORK/allowed" --open "$FILE"
ui_expect_log "MCP live instance:" "the application's MCP listener is up"
ui_step "fit (F6)"                         ui_key F6
grep -q "Failed features: [^n]" "$UI_LOG" && ui_fail "a feature failed"

echo "--- 1. Axis Through Cylinder/Cone/Torus on the cut's B-spline wall"
ui_mark
ui_step "axis through cylinder (search)"   ui_command "Axis Through Cylinder/Cone/Torus"
ui_step "pick the cut's wall"              ui_click_pick "$WALL"
ui_expect_new "Axis Through Cylinder/Cone/Torus Face: 1 face" "the wall taken"
ui_expect_new "Preview Axis Through Cylinder/Cone/Torus: ok" "previewed"
ui_step "OK (Enter)"                       ui_key Return
grep -E "Added axis Axis1: origin \(39, 10, 15\), direction \(-?1, 0, 0\)" "$UI_LOG" > /dev/null ||
  ui_fail "Axis1 is not the hole's axis: $(grep 'Added axis' "$UI_LOG" | tail -1)"
echo "ok   Axis1 through the hole"

echo "--- 2. Point at Center of Circle/Sphere/Torus on the wall's B-spline rim"
ui_mark
ui_step "point at center (search)"         ui_command "Point at Center of Circle/Sphere/Torus"
ui_step "pick the rim on the front"        ui_click_pick "$RIM"
ui_expect_new "Point at Center of Circle/Sphere/Torus Entity: 1 edge" "the rim taken"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_new "Added point Point1: point (40, 10, 15)" "Point1 at the rim's centre"
[ -n "$SHOT" ] && ui_capture "$SHOT"

echo "--- 3. The same through MCP, the wall and the rim without a geometry field (mitcad#1863)"
bridge_open
mcp_request initialize '{"protocolVersion": "2025-11-25", "capabilities": {},
  "clientInfo": {"name": "mitcad-round-faces", "version": "1"}}'
jq -e '.result.protocolVersion == "2025-11-25"' "$WORK/answer.json" > /dev/null || ui_fail "initialize failed"
mcp_send '{"jsonrpc": "2.0", "method": "notifications/initialized"}'
mcp_tool document_list '{}'
DOC=$(jq -r '.documents[0].document' "$WORK/result.json")

mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "faces", body: "F3.b0"}}')"
expect '[.result[] | select(.names | index($wall))] | length == 1 and .[0].surface == "cylinder"' \
  "the faces query calls the cut's B-spline wall a cylinder" --arg wall "$WALL_NAME"
mcp_tool model_query "$(jq -cn --arg d "$DOC" '{document: $d, query: {query: "faces", body: "F3.b0",
  surface: "cylinder", geometry: true, min_area: 45, max_area: 55}}')"
expect '.result.total == 1 and (.result.faces[0].radius - 4 | fabs) < 1e-6
  and (.result.faces[0].origin | (.[0] - 39 | fabs) < 1e-6 and (.[1] - 10 | fabs) < 1e-6 and (.[2] - 15 | fabs) < 1e-6)' \
  "the cylinder filter finds the wall with its radius and axis point"
mcp_tool model_query "$(jq -cn --arg d "$DOC" --arg rim "$RIM_NAME" '{document: $d, query: {query: "edges", body: "F3.b0"}}')"
expect '[.result[] | select(.name == $rim)] | length == 1 and .[0].curve == "circle"' \
  "the edges query calls the wall's B-spline rim a circle" --arg rim "$RIM_NAME"

ui_mark
mcp_tool ui_command "$(jq -cn --arg d "$DOC" --arg wall "$WALL_NAME" '{document: $d, id: "construct.axis_cylinder",
  inputs: {face: [{kind: "face", owner: "F3.b0", name: $wall}]}}')"
expect '.committed == true' "ui_command took the wall for Axis Through Cylinder/Cone/Torus"
ui_expect_new "Added axis Axis2: origin (39, 10, 15)" "Axis2 through the hole"
mcp_tool ui_command "$(jq -cn --arg d "$DOC" --arg rim "$RIM_NAME" '{document: $d, id: "construct.point_center",
  inputs: {entity: [{kind: "edge", owner: "F3.b0", name: $rim}]}}')"
expect '.committed == true' "ui_command took the rim for Point at Center of Circle/Sphere/Torus"
ui_expect_new "Added point Point2: point (40, 10, 15)" "Point2 at the rim's centre"
bridge_close

grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_finish "UI round faces test"
