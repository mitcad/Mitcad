#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/view/ViewEdges.cpp app/view/ViewEdges.hpp app/framework/TestDriver.cpp
# check-all sources: app/framework/ModelShapes.cpp app/sketch tools/cli
# check-all sources: tools/ui-scripts/edge-mesh.mitcad-ui tools/ui-scripts/edge-mesh.json
# The view's lines (mitcad#1633): a body's round edges are drawn as fine
# polylines from detached copies of its edges, whatever the body's size
# (they were the faces' display meshes' polygons, up to 0.5 mm off on a
# large body), its own edges get no 3D polygon (mitcad#1553), an edge's
# highlight lies on the polyline the body is drawn with, and a sketch
# curve's highlight on the curve, zoomed in far, 200 mm from the origin.
# Through the real UI, the input script tools/ui-scripts/edge-mesh.mitcad-ui
# (also run by tools/ui-windows-edge-mesh-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/edge-mesh.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-edge-mesh-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-edge-mesh.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=20
export MITCAD_TEST_INPUT=$here/ui-scripts/edge-mesh.mitcad-ui
"$cli" run "$here/ui-scripts/edge-mesh.json" --save "$work/edge-mesh.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/edge-mesh.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'edge mesh driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log ' edges drawn from copies, ' 'the body is drawn from fine copies of its edges'
ui_expect_log ' sketch curve highlights, at most ' 'the sketch curves'"'"' highlights lie on the curves'
ui_expect_log 'TestDriver: done' 'the body'"'"'s lines are fine, highlights on them, its edges unmeshed'
ui_finish 'UI view lines from fine edge copies'
