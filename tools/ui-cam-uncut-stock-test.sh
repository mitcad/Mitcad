#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam app/view/CamStockOverlay.cpp app/framework/TestDriverViewPixels.cpp app/framework/Theme.cpp tools/cli
# check-all sources: core/model/src/cam core/model/src/document/cam.rs core/model/src/document/cam/ core/cam
# check-all sources: tools/ui-scripts/cam-uncut-stock.mitcad-ui tools/ui-scripts/cam-uncut-stock.json
# The uncut stock on the screen (mitcad#2569, follow-up of mitcad#2552;
# app/COMMANDS.md "The CNC group"): the input script
# tools/ui-scripts/cam-uncut-stock.mitcad-ui on a block on a router's
# setup and a tube on a lathe's setup whose bar has a bore
# (tools/ui-scripts/cam-uncut-stock.json). On the light and the dark
# background, read from the view's pixels at model points: each stock
# drawn see-through over the background and over the block, its outline
# readable on the background, its faces shaded alike from either side;
# each setup's eye hiding its own stock only; Simulate's stock as cut with
# the bore shaded alike on its face and back, the uncut stock set aside
# and back when Simulate closes; the block picked through its stock.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-cam-uncut-stock-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-cam-uncut-stock.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/cam-uncut-stock.mitcad-ui
export MITCAD_CAM_LIBRARY_DIR=$WORK/cam-library
"$CLI" run "$here/ui-scripts/cam-uncut-stock.json" --save "$WORK/cam-uncut-stock.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail "mitcad-cli"; exit 1; }
# The view's frames the script saved, kept beside the kept logs
# (MITCAD_UI_LOG_DIR) when a step fails, for a look at what was drawn.
keep_frames() {
  [ -n "${MITCAD_UI_LOG_DIR:-}" ] || return 0
  for shot in "$WORK"/*.png; do
    [ -s "$shot" ] && cp -f "$shot" "$MITCAD_UI_LOG_DIR/cam-uncut-stock.$(basename "$shot")"
  done
  return 0
}
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/cam-uncut-stock.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { keep_frames; ui_fail "CAM uncut stock driver timed out"; exit 1; }
wait "$UI_RUNNER" || { keep_frames; ui_keep_log; cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: view edge contrast millEdge ' "the box's outline measured on the light background"
ui_expect_log 'TestDriver: view edge contrast latheEdgeDark ' "the bar's outline measured on the dark background"
ui_expect_log 'TestDriver: done' "the uncut stock on light and dark backgrounds, the eyes, Simulate and the pick"
for shot in uncut-light uncut-dark cut-top cut-bottom; do
  [ -s "$WORK/$shot.png" ] || ui_fail "no view frame $shot.png"
done
ui_finish "UI CAM uncut stock: see-through, outlined, alike from both sides"
