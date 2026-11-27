#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/ViewPaintLayer.cpp app/view/ViewPaintLayer.hpp app/view/ReplacedPictures.hpp
# check-all sources: app/OcctViewer.cpp app/sketch/SketchOverlay.cpp app/framework/HangWatchdog.cpp tools/cli
# check-all sources: tools/ui-scripts/paint-layer-fallback.mitcad-ui tools/ui-scripts/paint-layer-fallback.json
# The overlay layer's fallback (mitcad#2406: in 0.72 and 0.73 sketch mode
# stopped refreshing the window on old Intel graphics on Windows): with the
# layer's failure forced (MITCAD_TEST_PAINT_LAYER_FAILURE), sketch mode's
# overlay is drawn as a widget over the view for the rest of the session,
# as before 0.72, and the log says why. Twice, once per failure:
#   - gl-error: GL_OUT_OF_MEMORY after the texture upload, first as BGRA
#     ("painted again as RGBA"), then as RGBA, which falls back;
#   - slow-frame: the second picture's upload and draw over a second.
# Each run: Sketch1 edited, the fallback's line, the widget drawing the
# sketch (the pixels its paints drew), a line drawn with the Line tool
# painted by the widget, and an orbit whose navigation line counts the
# widget's paints. The steps are tools/ui-scripts/paint-layer-fallback.mitcad-ui,
# which tools/ui-windows-paint-layer-fallback-test.ps1 runs on Windows and
# tools/ui-macos-paint-layer-fallback-test.sh on macOS (gl-error).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-paint-layer-fallback-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-paint-layer-fallback.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/paint-layer-fallback.mitcad-ui
"$cli" run "$here/ui-scripts/paint-layer-fallback.json" --save "$work/rectangle.mitcad" || exit 1
ui_start_display

# run_mode mode why: the shared steps with the failure `mode` forced and
# `why` (a regular expression) the reason the fallback's line gives.
run_mode() {
  export MITCAD_TEST_PAINT_LAYER_FAILURE=$1
  export MITCAD_TEST_PAINT_LAYER_WHY=$2
  echo "--- MITCAD_TEST_PAINT_LAYER_FAILURE=$1"
  ui_start_app --ui=qt --no-mcp --open "$work/rectangle.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the paint layer fallback driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the paint layer fallback driver failed"; exit 1; }
  ui_expect_log 'View paint layers drawn as widgets over the view for the rest of the session: sketch overlay: ' \
    "$1: the log says the paint layers are widgets over the view, and why"
  ui_expect_log "View paint layer 'sketch overlay' painted as a widget over the view: " \
    "$1: the widget drew the sketch"
  ui_expect_log 'as a widget over the view' "$1: the orbit counted the widget's paints"
  ui_expect_log 'TestDriver: done' "$1: the shared steps passed"
}

run_mode gl-error 'GL error GL_OUT_OF_MEMORY \(0x0505\).* after its texture upload \([0-9]+ x [0-9]+ RGBA\)'
# Mesa takes BGRA textures: the BGRA upload failed first and was tried
# again as RGBA.
ui_expect_log "View paint layer 'sketch overlay': GL error GL_OUT_OF_MEMORY (0x0505)" \
  'gl-error: the BGRA upload failed first'
ui_expect_log '; painted again as RGBA' 'gl-error: painted again as RGBA before the fallback'

run_mode slow-frame 'its frame took [0-9]+ ms \(texture upload 5000[.]0 ms'
ui_expect_log "View paint layer 'sketch overlay' drawn by the view: " \
  'slow-frame: the first picture drawn by the view, the driver path logged'

ui_finish 'UI paint layer fallback'
