#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands app/framework/CommandSession.cpp app/framework/TestDriver.cpp core/model/src/print_fits.rs core/model/src/print_fits
# Shared printed-fit workflow through real Qt or AppKit controls.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 4 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory] [qt|native]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-print-fits}"
frontend=${4:-qt}
case "$frontend" in qt|native) ;; *) exit 2 ;; esac
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/printed-fits.json" --save "$UI_OUT/files/shaft.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_LOG_PICKS=1 MITCAD_TEST_STEP_MS=70 MITCAD_TEST_SETTLE_MS=150 \
  MITCAD_TEST_INPUT=$here/ui-scripts/printed-fits.mitcad-ui \
  ui_run_app fits --ui="$frontend" --no-recovery --no-mcp --open "$UI_OUT/files/shaft.mitcad"
ui_expect_log 'TestDriver: done' 'printed-fit names, editing, first use, comments and undo passed'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
