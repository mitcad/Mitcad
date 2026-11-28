#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Interference checks kept in the Analysis folder (mitcad#2664) on macOS,
# through the test driver: the shared steps
# tools/ui-scripts/interference-check.mitcad-ui and
# interference-check-reopen.mitcad-ui (also run by
# tools/ui-interference-check-test.sh and
# tools/ui-windows-interference-check-test.ps1) on the four blocks
# mitcad-cli makes of tools/ui-scripts/interference-check.json. Body1 and
# Body2 kept as Interference1, every body as Interference2, both run again
# from the browser after the geometry changes; saved, opened again, and
# Interference1 edited once Body2 is gone (checked in the file here).
# Usage: tools/ui-macos-interference-check-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-interference-check}"
UI_TIMEOUT=300
files=$UI_OUT/files
mkdir -p "$files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
design=$files/interference-check.mitcad
"$cli" run "$here/ui-scripts/interference-check.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/interference-check.mitcad-ui \
  ui_run_app interference-check --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'Kept Interference as Interference1' 'the check of two bodies kept'
ui_expect_log 'Kept Interference as Interference2' 'the check of every body kept'
ui_expect_log 'TestDriver: done' 'interference checks kept and run again'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"type": "interference"' "$design" && grep -q '"name": "Interference2"' "$design"; then
  echo "ok   both checks are in the saved file"
else
  ui_fail "no interference checks in the saved file"
fi
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/interference-check-reopen.mitcad-ui \
  ui_run_app interference-check-reopen --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'Edited analysis Interference1' 'the check kept with its new bodies'
ui_expect_log 'TestDriver: done' 'the checks opened again'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
if grep -q '"F6.b0"' "$design"; then
  echo "ok   Interference1 saved with Body3"
else
  ui_fail "Interference1 not saved with Body3"
fi
[ "$UI_FAILURES" = 0 ] || exit 1
