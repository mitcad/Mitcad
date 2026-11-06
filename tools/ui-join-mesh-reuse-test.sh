#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/framework/DisplayMeshes.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/CommandSupport.cpp geometry/io tools/cli
# check-all sources: tools/ui-scripts/join-mesh-reuse.mitcad-ui tools/ui-scripts/join-mesh-reuse.json
# A Join made in the Extrude panel draws every face of the joined body
# (GitHub #126, mitcad#1243) through the real UI, the shared input script
# tools/ui-scripts/join-mesh-reuse.mitcad-ui (also run by
# tools/ui-windows-join-mesh-reuse-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/join-mesh-reuse.json): a 177.2 x 25 plate
# extruded 5 mm, then a 6 x 25 block on its end 12 mm, a Join, refined
# before OK. All 8 faces of the joined body are meshed where the view
# draws them, and Create Sketch can pick the plate's top.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-join-mesh-reuse-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-join-mesh-reuse.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/join-mesh-reuse.mitcad-ui
"$cli" run "$here/ui-scripts/join-mesh-reuse.json" --save "$work/join-mesh-reuse.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/join-mesh-reuse.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Join mesh reuse driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: shown body F2.b0: 6 of 6 faces meshed on their faces' 'the plate is meshed on its faces'
ui_expect_log 'TestDriver: shown body F2.b0: 8 of 8 faces meshed on their faces' 'every face of the joined body is meshed on its face'
ui_expect_log 'TestDriver: done' 'a Join made in the panel draws the top it took from the plate'
ui_finish 'UI Join keeps the reused faces drawn'
