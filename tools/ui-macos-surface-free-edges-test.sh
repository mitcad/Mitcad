#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A surface body's free edges with no command, and Extend's Whole Opening
# (mitcad#1955, GitHub mitcad/Mitcad#407), in the Qt frontend on macOS: the
# shared steps tools/ui-scripts/surface-free-edges.mitcad-ui (also run by
# tools/ui-surface-free-edges-test.sh and
# tools/ui-windows-surface-free-edges-test.ps1) on the reported design of
# the private test files (MITCAD_MITCAD_REPORTED,
# github-407/surfacing_bug.zip; never committed), unpacked into the test's
# folder: with no command a free edge hovers and selects as an edge, Extend
# started then takes it as its whole opening, and with Whole Opening off the
# single edge extends. Skipped (77) without the design.
# Usage: bash tools/ui-macos-surface-free-edges-test.sh <app> [out directory]
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
ui_init "$1" "${2:-ui-macos-surface-free-edges}"
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
MITCAD_TEST_INPUT=$here/ui-scripts/surface-free-edges.mitcad-ui ui_run_app surface-free-edges \
  --ui=qt --no-mcp --no-recovery --open "$design"
ui_expect_log '^Idle hover: edge [{]"at":' 'with no command a free edge hovers as an edge'
ui_expect_log '^Selected: 1 edge [[]edge [{]"at":' 'a click selects the free edge'
ui_expect_log '^Extend Edges: 1 edge [[]edge [{]"faces":' 'Extend takes it as its whole opening'
ui_expect_log '^Hover opening: 4 edge' 'the hover shows the whole opening'
ui_expect_log '^Added extend of 1 edges' 'with Whole Opening off the single edge extends'
ui_expect_log 'TestDriver: done' 'every step passed'
if grep -Eq '^TestDriver: FAIL' "$UI_LOG_FILE"; then
  ui_fail 'a step of the script failed'
fi
if [ "$UI_FAILURES" -gt 0 ]; then
  echo "FAIL: surface free edges; evidence in $UI_OUT"
  exit 1
fi
echo 'PASS: free edges picked with no command, Extend with and without Whole Opening'
