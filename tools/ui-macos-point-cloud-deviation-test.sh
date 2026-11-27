#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Point clouds and Point Cloud Deviation (mitcad#2240) on macOS, through the
# test driver: the shared steps tools/ui-scripts/point-cloud-deviation.mitcad-ui
# (also run by tools/ui-point-cloud-deviation-test.sh and
# tools/ui-windows-point-cloud-deviation-test.ps1) on the block mitcad-cli
# makes of tools/ui-scripts/point-cloud-deviation.json and the scan written
# here beside it. Insert > Point Cloud reads the scan, chosen with Choose
# File... in Qt's own file dialog (mitcad#2474: no message left, OK
# available at once); Inspect > Point Cloud
# Deviation gives the statistics and colours the points, a point's tooltip
# its distance; the top face only, then 11 of the 32 points measured, the
# others dimmed (mitcad#2265); OK keeps it as Deviation1, saved (checked in
# the file here).
# Usage: tools/ui-macos-point-cloud-deviation-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-point-cloud-deviation}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/point-cloud-deviation.json" --save "$files/deviation-part.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
# The scan: on a 4 x 4 grid, 0.5 mm above the top face and 0.3 mm above the
# bottom face.
: > "$files/scan.xyz"
for x in 4 8 12 16; do
  for y in 4 8 12 16; do
    printf '%s %s 10.5\n%s %s 0.3\n' "$x" "$y" "$x" "$y" >> "$files/scan.xyz"
  done
done
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/point-cloud-deviation.mitcad-ui \
  ui_run_app point-cloud-deviation --ui=qt --no-recovery --no-mcp --no-native-dialogs \
  --open "$files/deviation-part.mitcad"
ui_expect_log 'Panel Point Cloud message: error gone: Choose a point cloud file\.' \
  'the file chosen in the dialog clears the message'
ui_expect_log 'Point clouds drawn: PointCloud1 32 points' 'the scan inserted and drawn'
ui_expect_log '32 of 32 points, signed, mean 0\.100 mm, mean absolute 0\.400 mm' 'the statistics'
ui_expect_log 'deviation 32 point\(s\)' 'the colour map drawn'
ui_expect_log 'Deviation hover: Distance 9\.700 mm \(inside\)' 'the tooltip at a point inside'
ui_expect_log 'Point clouds drawn: PointCloud1 21 points not measured' 'the points not measured drawn dimmed'
ui_expect_log 'Kept Point Cloud Deviation as Deviation1' 'kept in the document'
ui_expect_log 'TestDriver: done' 'Point Cloud Deviation through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"point_clouds"' "$files/deviation-part.mitcad" &&
  grep -q '"type": "point_cloud_deviation"' "$files/deviation-part.mitcad" &&
  grep -q '"faces"' "$files/deviation-part.mitcad"; then
  echo "ok   the cloud and the analysis with its face are in the saved file"
else
  ui_fail "no point cloud, deviation or its faces in the saved file"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
