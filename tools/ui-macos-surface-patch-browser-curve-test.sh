#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Patch takes a sketch curve for its boundary from the Browser (mitcad#1978,
# GitHub mitcad/Mitcad#407), in the Qt frontend on macOS: the shared steps
# tools/ui-scripts/surface-patch-browser-curve.mitcad-ui (also run by
# tools/ui-surface-patch-browser-curve-test.sh and
# tools/ui-windows-surface-patch-browser-curve-test.ps1) on the reported
# design of the private test files (MITCAD_MITCAD_REPORTED,
# github-407/surfacing_bug.zip; never committed), unpacked into the test's
# folder: while Patch's boundary is active the Browser lists the sketches'
# curves; Patch takes two free edges in the view and the sketch line from
# its Browser row, and OK adds the patch. Skipped (77) without the design.
# Usage: bash tools/ui-macos-surface-patch-browser-curve-test.sh <app> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
zip=${MITCAD_MITCAD_REPORTED:-}/github-407/surfacing_bug.zip
if [ -z "${MITCAD_MITCAD_REPORTED:-}" ] || [ ! -f "$zip" ]; then
  echo "SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-407) is not here"
  exit 77
fi
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
ui_init "$1" "${2:-ui-macos-surface-patch-browser-curve}"
UI_TIMEOUT=900
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_TEST_STEP_MS=100
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK/reported"
unzip -o -q "$zip" -d "$MITCAD_TEST_WORK/reported" || {
  echo "FAIL: the reported design could not be unpacked" >&2
  exit 1
}
design=$(find "$MITCAD_TEST_WORK/reported" -name '*.mitcad' -type f | sort | head -1)
if [ -z "$design" ]; then
  echo "FAIL: no design in the reported zip" >&2
  exit 1
fi
MITCAD_TEST_INPUT=$here/ui-scripts/surface-patch-browser-curve.mitcad-ui ui_run_app surface-patch-browser-curve \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log '^Browser sketch curves: [1-9][0-9]* sketches, ' 'the Browser lists the sketches'"'"' curves during Patch'
ui_expect_log '^Notice hover highlighted: browser curve: sketch_curve c42 ' 'the curve'"'"'s row paints it under the pointer'
ui_expect_log '^Patch Profiles or Edges: 2 edges, 1 sketch curve ' 'two free edges and the line from the Browser are taken'
ui_expect_log '^Added patch of 3 items$' 'OK adds the patch'
ui_expect_log '^Browser sketch curves: none$' 'the curve rows go with the command'
ui_expect_log 'TestDriver: done' 'every step passed'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: surface patch browser curve; evidence in $UI_OUT"
  exit 1
fi
echo 'PASS: patch with a sketch curve picked in the Browser'
