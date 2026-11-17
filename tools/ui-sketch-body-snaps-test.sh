#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/document/plane_geometry.rs core/model-api/src/sketch.rs
# check-all sources: tools/ui-scripts/sketch-body-snaps.mitcad-ui tools/ui-scripts/sketch-body-snaps.json
# Every drawing tool snaps to body corners and edges in the sketch plane
# (mitcad#1967, GitHub mitcad/Mitcad#392), through the real UI on a design
# that mitcad-cli makes (tools/ui-scripts/sketch-body-snaps.json: a
# 60 x 40 x 20 block, Sketch2 on its top face): a line drawn from a block
# corner starts on a new point held on the corner's linked projection
# (Coincident); a rectangle's corner clicked on another block corner
# likewise; a line started on a block edge away from its middle starts on
# the edge's linked projection (Coincident). The steps are
# tools/ui-scripts/sketch-body-snaps.mitcad-ui, which
# tools/ui-windows-sketch-body-snaps-test.ps1 runs on Windows and
# tools/ui-macos-sketch-body-snaps-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-body-snaps-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-body-snaps.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-body-snaps.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-body-snaps.json" --save "$work/block.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch body snaps driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log ', the projected corner V{' 'a drawn point held on a projected block corner'
ui_expect_log ', the projected edge E{' 'a line start held on a projected block edge'
ui_expect_log 'TestDriver: done' 'a line and a rectangle from block corners, a line from a block edge'
ui_finish 'UI sketch body snaps'
