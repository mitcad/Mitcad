#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Point pairs and the crop box of the point cloud tools (mitcad#2565) on
# macOS, through the test driver: the shared steps
# tools/ui-scripts/point-cloud-pairs.mitcad-ui (also run by
# tools/ui-point-cloud-pairs-test.sh and
# tools/ui-windows-point-cloud-pairs-test.ps1) on the design mitcad-cli makes
# of tools/ui-scripts/point-cloud-pairs.json: a block and a scan of its faces
# turned 30 degrees and moved 20 mm. Align Point Cloud by three point pairs
# clicked in the view and the best fit moves the scan back onto the block
# within 0.01 mm RMS; Filter Point Cloud with a crop box keeps the 140
# points inside it.
# Usage: tools/ui-macos-point-cloud-pairs-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-point-cloud-pairs}"
UI_TIMEOUT=600
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/point-cloud-pairs.json" --save "$files/point-cloud-pairs.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-pairs.mitcad-ui \
  ui_run_app point-cloud-pairs --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$files/point-cloud-pairs.mitcad"
ui_expect_log 'Align Point Cloud points: 6' 'three point pairs picked in the view'
ui_expect_log 'Align Point Cloud: Backspace taken' 'a picked point taken back'
ui_expect_log 'Inspect Align Point Cloud: Scan to Body1: RMS 0\.00' 'the remaining deviation stated'
ui_expect_log 'Aligned point cloud Scan to Body1: RMS 0\.00' 'the cloud aligned within 0.01 mm'
ui_expect_log 'cropped by a box from -1\.000,-1\.000,-1\.000 to 9\.000,21\.000,11\.000, inside kept' 'the box crop'
ui_expect_log 'Point clouds drawn: Scan Filtered 140 points' 'the points inside the box kept'
ui_expect_log 'TestDriver: done' 'point pairs and the crop box through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
