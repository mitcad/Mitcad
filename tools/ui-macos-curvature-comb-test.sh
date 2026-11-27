#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Curvature Comb (mitcad#2520) on macOS, through the test driver: the shared
# steps tools/ui-scripts/curvature-comb.mitcad-ui (also run by
# tools/ui-curvature-comb-test.sh and tools/ui-windows-curvature-comb-test.ps1)
# on the design mitcad-cli makes of tools/ui-scripts/curvature-comb.json.
# Inspect > Curvature Comb combs a cylinder's top edge; in Sketch2 the
# palette's Curvature comb shows an S-shaped spline's comb with its one
# inflection, read again while its end point is dragged.
# Usage: tools/ui-macos-curvature-comb-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-curvature-comb}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/curvature-comb.json" --save "$files/curvature-comb.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/curvature-comb.mitcad-ui \
  ui_run_app curvature-comb --ui=qt --no-recovery --no-mcp --open "$files/curvature-comb.mitcad"
ui_expect_log 'Inspect Curvature Comb: 1 curve\(s\), 24 teeth each, no inflection, minimum radius 10 mm' 'an edge combed'
ui_expect_log 'Sketch curvature comb: c' 'a sketch curve combed in sketch mode'
ui_expect_log 'Sketch curvature comb \(dragging\): c' 'the comb read again while dragged'
ui_expect_log 'TestDriver: done' 'Curvature Comb through the UI'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
