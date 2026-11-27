#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp
# check-all sources: app/commands/CommandSupport.hpp app/framework/Selection.cpp
# check-all sources: core/model/src/features/extrude.rs core/model/src/features/reference.rs
# check-all sources: tools/cli
# check-all sources: app/framework/CommandSession.cpp app/commands/HoleCommands.cpp
# check-all sources: app/framework/TestDriver.cpp
# Extrude up to a point of another sketch (mitcad#1329) through the real
# UI, on a design mitcad-cli makes: a 20 x 10 rectangle on XY (Sketch1, F1)
# and, at right angles to it, a sketch on XZ (Sketch2, F2, shown) with one
# point at x 5, z 35 (its sketch y runs along -Z). Extrude (E) takes the
# rectangle; Extent To Object takes the point of Sketch2 in the view, and
# the new body ends on the plane through the point parallel to XY: 35 mm
# long, 7000 mm3.
# Then the same in a component (mitcad#1599), through the shared driver
# script tools/ui-scripts/extrude-to-point-component.mitcad-ui (also
# app.ui-windows-extrude-to-point): editing an extrusion up to a sketch
# point, a hole down to one and an extrusion from a sketch line keeps the
# loaded object, previews and is not refused as another occurrence's.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-to-point-test.sh

source "$(dirname "$0")/ui-test-lib.sh"

HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/.." && pwd)
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-extrude-to-point.XXXXXX")
trap 'ui_cleanup; rm -rf "$WORK"' EXIT
R='r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}'

cat > "$WORK/design.json" << EOF
[
  {"cmd": "sketch.create"},
  {"cmd": "sketch.add_rectangle", "sketch": "F1", "corner": [0, 0], "width": 20, "height": 10},
  {"cmd": "sketch.create", "plane": "xz"},
  {"cmd": "sketch.add_point", "sketch": "F2", "at": [5, -35]},
  {"cmd": "set_feature_visible", "uid": "F2", "visible": true}
]
EOF
"$CLI" run "$WORK/design.json" --save "$WORK/design.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; }

ui_start_display
ui_start_app --open "$WORK/design.mitcad"
ui_step "fit (F6)"                         ui_key F6

echo "--- Extrude the rectangle up to the point of the sketch on XZ"
ui_step "extrude (E)"                      ui_key e
ui_expect_log "Extrude Profiles: 1 profile [profile $R of F1]" "the rectangle taken"
ui_step "To Object"                        ui_choose "Panel Extrude input extent" 1
ui_expect_log "Extrude: Extent = To Object" "to object chosen"
ui_expect_log "Sketch entity F2/p1 at" "the point of the other sketch is pickable"
ui_mark
ui_step "pick the point"                   ui_click_logged "Sketch entity F2/p1"
ui_expect_new "Extrude Object: 1 sketch point [sketch point p1 of F2]" "the sketch point the object"
ui_expect_new "Preview Extrude: ok" "previewed up to the point"
ui_step "OK (Enter)"                       ui_key Return
ui_expect_log "Added extrude (New Body), distance to object" "extrusion added"
ui_expect_volume "New body Body1 (F3.b0): volume \([0-9.]*\) mm3" 7000 "35 mm of the rectangle"
grep -q "Recompute failed" "$UI_LOG" && ui_fail "a recompute failed"
ui_stop_app

echo "--- Edited in a component: the loaded point and line where the profiles are (mitcad#1599)"
"$CLI" run "$HERE/ui-scripts/extrude-to-point-component.json" --save "$WORK/component.mitcad" \
  > "$WORK/cli-component.log" 2>&1 || { cat "$WORK/cli-component.log"; ui_fail "mitcad-cli (component)"; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$HERE/ui-scripts/extrude-to-point-component.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$WORK/component.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && ui_fail "the component edit driver timed out"
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "the component edit driver failed"; }
ui_expect_log "TestDriver: done" "a component's loaded point and line kept by the edits"
unset MITCAD_TEST_INPUT MITCAD_TEST_STEP_MS
ui_finish "UI extrude to point test"
