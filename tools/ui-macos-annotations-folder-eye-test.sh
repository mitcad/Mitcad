#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/MainWindowMeasurements.cpp app/view/MeasurementOverlay.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/document/display.rs tools/cli
# check-all sources: tools/ui-scripts/annotations-folder-eye.mitcad-ui tools/ui-scripts/annotations-folder-eye-reopen.mitcad-ui
# The Browser's Annotations folder's eye (mitcad#2041) on macOS, the same
# shared scripts as the Linux and Windows tests'
# (tools/ui-scripts/annotations-folder-eye.mitcad-ui, then
# annotations-folder-eye-reopen.mitcad-ui on the saved design): the eye
# hides every annotation (none drawn, none picked, the row greyed), one
# undo step; saved hidden, the design keeps it (checked with
# annotations-folder-eye-check.json) and opens with none drawn; the eye
# shows the same two again (annotations-folder-eye-shown-check.json).
# Usage: tools/ui-macos-annotations-folder-eye-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-annotations-folder-eye}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/annotations-folder-eye.mitcad
"$2" run "$here/ui-scripts/annotations-folder-eye.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
for phase in annotations-folder-eye annotations-folder-eye-reopen; do
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/$phase.mitcad-ui \
    ui_run_app "$phase" --ui=qt --no-recovery --no-mcp --open "$design"
  ui_expect_log 'TestDriver: done' "$phase: the shared steps"
  ui_expect_no_log '^TestDriver: FAIL' "$phase: no failed driver assertion"
  [ "$UI_FAILURES" = 0 ] || exit 1
  check=annotations-folder-eye-check
  [ "$phase" = annotations-folder-eye ] || check=annotations-folder-eye-shown-check
  if "$2" run "$here/ui-scripts/$check.json" --open "$design" > "$UI_OUT/$check.log" 2>&1; then
    ui_pass "$phase: the saved design ($check.json)"
  else
    cat "$UI_OUT/$check.log" >&2
    ui_fail "$phase: the saved design ($check.json)"
    exit 1
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1
