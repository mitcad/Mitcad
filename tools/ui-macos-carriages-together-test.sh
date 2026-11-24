#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Two bearings on one shaft joined by a Rigid joint or a Rigid Group move
# together (mitcad#2274, GitHub mitcad/Mitcad#575) in the Qt frontend on
# macOS: the shared steps tools/ui-scripts/carriages-together.mitcad-ui
# (also run by tools/ui-carriages-together-test.sh and
# tools/ui-windows-carriages-together-test.ps1) on the reported design of
# the private test files (MITCAD_MITCAD_REPORTED, github-575; never
# committed), unpacked into the test's folder: a Rigid joint between the
# bearings at their distance holds, a drag of one moves both, and a drag
# past one's travel limit stops both there; undone, the same with a Rigid
# Group. Skipped (77) without the design.
# Usage: bash tools/ui-macos-carriages-together-test.sh <app> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
if [ $# -lt 1 ] || [ $# -gt 2 ]; then
  echo "usage: $0 <app> [out directory]" >&2
  exit 2
fi
zip=$(find "${MITCAD_MITCAD_REPORTED:-/nonexistent}/github-575" -maxdepth 1 -name '*.zip' -type f 2> /dev/null | sort | head -1)
if [ -z "$zip" ]; then
  echo "SKIP: the reported design of the private test files (MITCAD_MITCAD_REPORTED/github-575) is not here"
  exit 77
fi
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
ui_init "$1" "${2:-ui-macos-carriages-together}"
UI_TIMEOUT=600
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
MITCAD_TEST_INPUT=$here/ui-scripts/carriages-together.mitcad-ui ui_run_app carriages-together \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log '^Joint Origin B: 1 face' "the Rigid joint picks both bearings' end faces"
ui_expect_log '^Added joint .+ \(rigid\)' 'the Rigid joint at their distance is added'
ui_expect_log '^Added rigid group ' 'the Rigid Group of the two bearings is added'
ui_expect_log '^Dragged ' 'the bearings are dragged along the shaft'
ui_expect_log 'TestDriver: done' 'both move together and stop at the first travel limit'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: carriages together; evidence in $UI_OUT"
  exit 1
fi
echo 'PASS: bearings joined rigidly move together'
