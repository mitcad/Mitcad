#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/OcctViewer.cpp app/OcctViewer.hpp app/view/CameraHeight.hpp app/MainWindow.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/sketch-resize.mitcad-ui tools/ui-scripts/sketch-resize.json
# The window resized while a sketch is edited (mitcad#1996) in the macOS
# VM, through the Qt frontend: the driver script
# tools/ui-scripts/sketch-resize.mitcad-ui shared with the Linux and
# Windows tests, on the design mitcad-cli makes
# (tools/ui-scripts/sketch-resize.json). Edit Sketch1 in a wide window, the
# window made narrow and tall, Finish Sketch; then the other way. Each time
# the restored camera's aspect is the view's width over its height and a
# circle at its target is drawn round.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-sketch-resize}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/sketch-resize.json" --save "$UI_OUT/files/resize.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
export MITCAD_TEST_WORK=$UI_OUT/files
MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/sketch-resize.mitcad-ui \
  ui_run_app sketch-resize --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/resize.mitcad"
ui_expect_log 'TestDriver: done' 'Finish Sketch after a resize restores the view in the window aspect'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
restored=$(grep -Ec '^View: camera restored, .*\(the view.s aspect\), .*\(round\)$' "$UI_LOG_FILE")
if [ "$restored" = 2 ]; then
  ui_pass "restored twice in the view's aspect, a circle drawn round"
else
  ui_fail "the view before the sketch was restored in the view's aspect $restored times, not twice"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
