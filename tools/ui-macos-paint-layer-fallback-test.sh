#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/OcctViewer.cpp tools/cli
# check-all sources: tools/ui-scripts/paint-layer-fallback.mitcad-ui tools/ui-scripts/paint-layer-fallback.json
# The overlay layer's fallback (mitcad#2406) in the Qt frontend on macOS,
# as tools/ui-paint-layer-fallback-test.sh (Linux, its gl-error run) and
# tools/ui-windows-paint-layer-fallback-test.ps1 (Windows): with
# MITCAD_TEST_PAINT_LAYER_FAILURE=gl-error, the shared steps
# tools/ui-scripts/paint-layer-fallback.mitcad-ui on the design mitcad-cli
# makes (tools/ui-scripts/paint-layer-fallback.json): Sketch1 edited, the
# log saying that the paint layers are widgets over the view for the rest
# of the session and why, the widget drawing the sketch, a line drawn with
# the Line tool painted by it, an orbit counting its paints.
# Usage: bash tools/ui-macos-paint-layer-fallback-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
cli=$2
ui_init "$1" "${3:-ui-macos-paint-layer-fallback}"
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_PAINT_LAYER_FAILURE=gl-error
export MITCAD_TEST_PAINT_LAYER_WHY='GL error GL_OUT_OF_MEMORY \(0x0505\).* after its texture upload \([0-9]+ x [0-9]+ RGBA\)'
mkdir -p "$MITCAD_TEST_WORK"
"$cli" run "$here/ui-scripts/paint-layer-fallback.json" --save "$MITCAD_TEST_WORK/rectangle.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || { cat "$UI_OUT/setup.log" >&2; exit 1; }
UI_TIMEOUT=240
MITCAD_TEST_INPUT=$here/ui-scripts/paint-layer-fallback.mitcad-ui \
  ui_run_app paint-layer-fallback --ui=qt --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/rectangle.mitcad"
ui_expect_log 'View paint layers drawn as widgets over the view for the rest of the session: sketch overlay: ' \
  'the log says the paint layers are widgets over the view, and why'
ui_expect_log "View paint layer 'sketch overlay' painted as a widget over the view: " 'the widget drew the sketch'
ui_expect_log 'as a widget over the view' "the orbit counted the widget's paints"
ui_expect_log 'TestDriver: done' 'the shared steps passed'
ui_expect_no_log 'TestDriver: FAIL|OCCT view event failed' 'all assertions passed'
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: paint layer fallback; evidence in $UI_OUT"
  exit 1
fi
echo "PASS: paint layer fallback; evidence in $UI_OUT"
