#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/OrbitCenterView.cpp app/view/Navigation.hpp app/view/SpaceMouseView.cpp
# check-all sources: app/OcctViewer.cpp app/MainWindow.cpp app/framework/TestDriver.cpp
# check-all sources: app/framework/TestDriverOrbitCenter.cpp tools/cli tools/ui-scripts/view-orbit-center.mitcad-ui
# The orbit's center (mitcad#1791, GitHub mitcad/Mitcad#300) through the
# real UI, the shared input script tools/ui-scripts/view-orbit-center.mitcad-ui
# (also run by tools/ui-windows-view-orbit-center-test.ps1) on a block that
# mitcad-cli makes (tools/ui-scripts/view-orbit-center.json): Set Orbit
# Center Here in the view's context menu near a corner of the block fixes
# the center, an orbit keeps that point's place on the screen while the
# block's middle moves, middle drags, a middle click and the wheel leave
# it, Clear Orbit Center clears it, and with Orbit center Cursor an orbit
# started over a face turns about that face's point.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-view-orbit-center-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-view-orbit-center.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/view-orbit-center.mitcad-ui
"$cli" run "$here/ui-scripts/view-orbit-center.json" --save "$work/block.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'orbit center driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the orbit turns about the fixed center and the point under the cursor'
ui_finish 'UI orbit center: fixed from the context menu, and the point under the cursor'
