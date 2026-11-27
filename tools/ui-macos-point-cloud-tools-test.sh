#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Point cloud tools (mitcad#2516) on macOS, through the test driver: the
# shared steps tools/ui-scripts/point-cloud-tools.mitcad-ui (also run by
# tools/ui-point-cloud-tools-test.sh and
# tools/ui-windows-point-cloud-tools-test.ps1) on the design mitcad-cli
# makes of tools/ui-scripts/point-cloud-tools.json: a block and a scan of
# its faces, moved, with 3 outliers. Filter Point Cloud takes the outliers
# out; Align Point Cloud states the remaining deviation and moves the
# cloud onto the block; Reconstruct Mesh adds a closed mesh body of it;
# saved (checked in the file here).
# Usage: tools/ui-macos-point-cloud-tools-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-point-cloud-tools}"
UI_TIMEOUT=600
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/point-cloud-tools.json" --save "$files/point-cloud-tools.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-tools.mitcad-ui \
  ui_run_app point-cloud-tools --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$files/point-cloud-tools.mitcad"
ui_expect_log 'Filtered point cloud Scan: outliers statistical' 'the outliers filtered out'
ui_expect_log 'Point clouds drawn: Scan Filtered 110 points' 'the filtered cloud drawn'
ui_expect_log 'Inspect Align Point Cloud: Scan Filtered to Body1: RMS 0\.000 mm' 'the remaining deviation stated'
ui_expect_log 'Aligned point cloud Scan Filtered to Body1' 'the cloud aligned'
ui_expect_log 'Reconstructed a mesh of point cloud Scan Filtered' 'the mesh built'
ui_expect_log 'TestDriver: done' 'the point cloud tools through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"Scan Filtered"' "$files/point-cloud-tools.mitcad" &&
  grep -q '"source": "Scan Filtered"' "$files/point-cloud-tools.mitcad" &&
  grep -q '"Scan Filtered Mesh"' "$files/point-cloud-tools.mitcad"; then
  echo "ok   the filtered cloud and its mesh are in the saved file"
else
  ui_fail "no filtered cloud or mesh of it in the saved file"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
