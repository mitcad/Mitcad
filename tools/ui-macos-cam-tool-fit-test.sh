#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/cam/CamToolFit.cpp app/cam/CamToolFit.hpp app/cam/MainWindowCam.cpp
# check-all sources: core/cam/src/ops core/model/src/cam core/model/src/document/cam.rs tools/cli
# check-all sources: tools/ui-scripts/cam-tool-fit.mitcad-ui tools/ui-scripts/cam-tool-fit.json
# The milling panels' Tool (mitcad#2590) on macOS, through the test driver
# (tools/ui-scripts/cam-tool-fit.mitcad-ui, the same script as the Linux
# test's): a saved pocket's ball end mill flagged and replaced; Pocket,
# Contour and Facing starting with the flat end mill after the ball one
# and generating, Adaptive Clearing starting with it; Contour on the line taking the
# engraving cutter; a setup whose machine has no end mill refused at Tool.
# Usage: tools/ui-macos-cam-tool-fit-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-cam-tool-fit}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/cam-tool-fit.json" --save "$UI_OUT/files/cam-tool-fit.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 MITCAD_CAM_LIBRARY_DIR=$UI_OUT/files/cam-library \
  MITCAD_TEST_INPUT=$here/ui-scripts/cam-tool-fit.mitcad-ui \
  ui_run_app cam-tool-fit --ui=qt --no-native-dialogs --no-recovery --no-mcp --open "$UI_OUT/files/cam-tool-fit.mitcad"
ui_expect_log 'TestDriver: done' 'a saved ball end mill replaced, the flat end mill by default, the engraving cutter on the line, a setup without an end mill'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
