#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/acknowledged_warnings.rs
# check-all sources: core/model-api/src/lib.rs tools/cli tools/ui-scripts/acknowledge-warning.mitcad-ui
# check-all sources: tools/ui-scripts/timeline-auto-fix.json
# Acknowledged warnings (mitcad#1935, GitHub mitcad/Mitcad#380) in the Qt
# frontend on macOS, as tools/ui-acknowledge-warning-test.sh on Linux: the
# shared steps tools/ui-scripts/acknowledge-warning.mitcad-ui on a design
# that mitcad-cli makes (tools/ui-scripts/timeline-auto-fix.json): an
# extrusion whose region a sketch change split warns; Acknowledge Warning
# takes it out of the warnings badge and the yellow cell, Show Warning
# Again brings both back, and the badge's menu acknowledges it too.
# Usage: tools/ui-macos-acknowledge-warning-test.sh <app> <mitcad-cli> <output>
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=tools/ui-macos-lib.sh
. "$here/ui-macos-lib.sh"
if [ "$#" != 3 ]; then
  echo 'usage: ui-macos-acknowledge-warning-test.sh <app> <cli> <output>' >&2
  exit 2
fi
ui_init "$1" "$3"
cli=$2
UI_TIMEOUT=180
export MITCAD_TEST_WORK=$UI_OUT/files
rm -rf "$MITCAD_TEST_WORK"
mkdir -p "$MITCAD_TEST_WORK"
design=$MITCAD_TEST_WORK/warned.mitcad
"$cli" run "$here/ui-scripts/timeline-auto-fix.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 ||
  { cat "$UI_OUT/setup.log"; echo 'FAIL: mitcad-cli run timeline-auto-fix.json' >&2; exit 1; }
MITCAD_TEST_INPUT=$here/ui-scripts/acknowledge-warning.mitcad-ui MITCAD_TEST_STEP_MS=100 \
  ui_run_app acknowledge-warning --ui=qt --no-recovery --no-mcp --no-native-dialogs --open "$design"
ui_expect_log '^Acknowledged warning of Extrude1$' 'Acknowledge Warning in the timeline menu'
ui_expect_log '^Showing warning again of Extrude1$' 'Show Warning Again in the timeline menu'
ui_expect_log '^TestDriver: done' 'an acknowledged warning leaves the badge and the yellow cell, and comes back'
ui_expect_no_log 'TestDriver: FAIL' 'no failed acknowledged warning assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt acknowledged warnings'
