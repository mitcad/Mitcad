#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Component-coordinate section preview, caps, flip, persistence and removal.
# Run in the approved macOS GUI test environment with the matching CLI build.
# Usage: bash tools/ui-macos-section-test.sh <app> <mitcad-cli> [out directory]
set -u
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ $# -lt 2 ] || [ $# -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [out directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-section}"
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
export MITCAD_NO_UPDATE_CHECK=1
mkdir -p "$MITCAD_TEST_WORK"
"$2" run "$here/ui-scripts/section-coordinates.json" --save "$MITCAD_TEST_WORK/section-coordinates.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
for phase in section-coordinates section-coordinates-reopen; do
  MITCAD_TEST_INPUT=$here/ui-scripts/$phase.mitcad-ui ui_run_app "$phase" \
    --ui=native --no-recovery --open "$MITCAD_TEST_WORK/section-coordinates.mitcad"
  ui_expect_no_log 'TestDriver: FAIL' "$phase: every geometry assertion passed"
  ui_expect_no_log 'OCCT view event failed' "$phase: no failed view operation"
  ui_expect_no_log 'The document was read while a job computes it' "$phase: no concurrent document read"
  ui_expect_log 'TestDriver: done' "$phase: the script reached its end"
  if [ "$UI_FAILURES" -gt 0 ]; then
    echo "FAIL: section coordinates; evidence in $UI_OUT"
    exit 1
  fi
done
echo "PASS: section coordinates; logs and screenshots in $UI_OUT"
