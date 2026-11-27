#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Point pairs on bodies of a placed component (mitcad#2566) on macOS,
# through the test driver: the shared steps
# tools/ui-scripts/point-cloud-pairs-placed.mitcad-ui (also run by
# tools/ui-point-cloud-pairs-placed-test.sh and
# tools/ui-windows-point-cloud-pairs-placed-test.ps1) on the design
# mitcad-cli makes in the work folder of
# tools/ui-scripts/point-cloud-pairs-placed.json: a block and a mesh block in
# a component whose occurrence is turned 90 degrees and moved, and a scan of
# the block's faces in the root. Align Point Cloud by three point pairs
# clicked on the placed block moves the scan onto the block as the view
# shows it within 0.01 mm RMS; Align Mesh by three pairs moves the mesh onto
# the block within 0.01 mm RMS.
# Usage: tools/ui-macos-point-cloud-pairs-placed-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-point-cloud-pairs-placed}"
UI_TIMEOUT=600
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
(cd "$files" && "$cli" run "$here/ui-scripts/point-cloud-pairs-placed.json" --save "$files/point-cloud-pairs-placed.mitcad") \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-pairs-placed.mitcad-ui \
  ui_run_app point-cloud-pairs-placed --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$files/point-cloud-pairs-placed.mitcad"
ui_expect_log 'Align Point Cloud target point: F1\.b0 at ' 'a target point taken into the design'
ui_expect_log 'Align Point Cloud points: 6' 'three point pairs picked on the placed block'
ui_expect_log 'Aligned point cloud Scan to Body1: RMS 0\.00' 'the cloud aligned within 0.01 mm'
ui_expect_log 'Align Mesh points: 6' 'three point pairs picked on the placed mesh and block'
ui_expect_log 'Added mesh align of F4\.b0 onto F1\.b0, 3 point pairs, best fit' 'the mesh aligned'
ui_expect_log 'TestDriver: done' 'point pairs on a placed component through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
