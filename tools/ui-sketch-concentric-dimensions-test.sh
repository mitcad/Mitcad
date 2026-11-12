#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-concentric-dimensions.mitcad-ui tools/ui-scripts/sketch-concentric-dimensions.json
# The values of two concentric circles' diameters keep apart (mitcad#1673),
# the shared input script tools/ui-scripts/sketch-concentric-dimensions.mitcad-ui
# (also run by tools/ui-windows-sketch-concentric-dimensions-test.ps1) on a
# design that mitcad-cli makes (tools/ui-scripts/sketch-concentric-dimensions.json):
# an empty sketch framed by two fixed points. Circle at (5, 3) typed 5.2,
# then Circle at the same centre typed 5: the two values' boxes do not
# overlap after the fit, zoomed in, and zoomed out far. The screenshots
# (sketch-concentric-dimensions*.png) show them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-concentric-dimensions-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-concentric-dimensions.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$WORK}
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-concentric-dimensions.mitcad-ui
"$CLI" run "$here/ui-scripts/sketch-concentric-dimensions.json" --save "$WORK/concentric.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/concentric.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch concentric dimensions driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added circle, diameter 5.2 mm at (5, 3)' 'the outer circle typed 5.2'
ui_expect_log 'Added circle, diameter 5 mm at (5, 3)' 'the inner circle typed 5 round the same centre'
ui_expect_log 'Sketch dimension values apart: 2 values' 'the two diameter values apart'
ui_expect_log 'TestDriver: done' 'the values apart after the fit, zoomed in and zoomed out'
ui_finish 'UI sketch concentric circle dimensions apart'
