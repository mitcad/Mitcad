#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/CameraHeight.hpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-resize.mitcad-ui tools/ui-scripts/sketch-resize.json
# The window resized while a sketch is edited (mitcad#1996) through the
# real UI, the shared input script tools/ui-scripts/sketch-resize.mitcad-ui
# (also run by tools/ui-windows-sketch-resize-test.ps1 and
# tools/ui-macos-sketch-resize-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-resize.json): Edit Sketch1 in a wide window, the
# window made narrow and tall, Finish Sketch; then the other way. Each time
# the restored camera's aspect is the view's width over its height and a
# circle at its target is drawn round.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-resize-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-resize.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-resize.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-resize.json" --save "$work/resize.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/resize.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch resize driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
[ "$(grep -c '^View: camera restored, .*(the view.s aspect), .*(round)$' "$UI_LOG")" = 2 ] ||
  ui_fail "the view before the sketch was not restored in the view's aspect twice: $(grep '^View: camera restored' "$UI_LOG")"
echo "ok   restored twice in the view's aspect, a circle drawn round"
ui_expect_log 'TestDriver: done' 'Finish Sketch after a resize restores the view in the window aspect'
ui_finish 'UI sketch resize'
