#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/TimelineWidget.cpp app/browser/DocumentSnapshot.cpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/new-component-fillets.mitcad-ui tools/ui-scripts/new-component-fillets.json
# An extrude edited from Join to New Component takes its fillet along
# (mitcad#2185) on macOS, through the test driver
# (tools/ui-scripts/new-component-fillets.mitcad-ui, the same script as the
# Linux and Windows tests'): no feature fails, and the timeline marks the
# extrude and the fillet in the new component's colour at once; undo and
# the edit back to Join bring the component's colour back.
# Usage: tools/ui-macos-new-component-fillets-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-new-component-fillets}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/new-component-fillets.json" --save "$UI_OUT/files/new-component-fillets.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/new-component-fillets.mitcad-ui \
  ui_run_app new-component-fillets --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/new-component-fillets.mitcad"
ui_expect_log 'Timeline feature colors: ' 'the timeline logs its colour marks'
ui_expect_no_log 'Recompute failed: ' 'no feature failed'
ui_expect_log 'TestDriver: done' 'the fillet followed the new component'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
