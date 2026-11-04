#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession app/framework/CommandPanel app/framework/TestDriver
# Primary Qt frontend's busy-preview/commit/close acceptance on macOS.
# Usage: ui-macos-command-cancel-test.sh <app> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <app> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${2:-ui-macos-command-cancel}"
UI_TIMEOUT=210
# Open during a busy preview exercises replacement and its save question.
# Multi-document tests separately cover the default new-window policy.
mkdir -p "$MITCAD_SETTINGS_DIR/nocodo.fi"
printf '\n[documents]\nopenInCurrentWindow=true\n' >> "$MITCAD_SETTINGS_DIR/nocodo.fi/Mitcad.ini"
export MITCAD_TEST_WORK=$UI_OUT/files
mkdir -p "$MITCAD_TEST_WORK"
cat > "$MITCAD_TEST_WORK/root-design.mitcad" <<'JSON'
{
  "format": "mitcad", "version": 2,
  "units": {"length": "mm", "angle": "deg"},
  "parameters": [
    {"name": "d1", "expression": "40 mm", "unit": "mm", "owner": "F1"},
    {"name": "d2", "expression": "25 mm", "unit": "mm", "owner": "F1"},
    {"name": "d3", "expression": "10 mm", "unit": "mm", "owner": "F1"}
  ],
  "features": [
    {"uid": "F1", "name": "Box1", "type": "box", "plane": "xy", "corner": [0, 0],
     "length": "d1", "width": "d2", "height": "d3", "operation": "new_body"}
  ]
}
JSON
MITCAD_TEST_STEP_MS=20 MITCAD_TEST_SETTLE_MS=60 \
  MITCAD_TEST_RECOMPUTE_DELAY_MS=0,preview=1500,add_feature=1500,edit_feature=1500 \
  MITCAD_TEST_INPUT=$here/ui-scripts/macos-command-cancel.mitcad-ui \
  ui_run_app cancel --ui=qt --chrome="${MITCAD_TEST_CHROME:-floating}" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/root-design.mitcad"
ui_expect_log 'TestDriver: done' 'Qt preview, pending Return, commit cancellation and close restoration completed'
ui_expect_log 'Qt test: clicked progress Cancel during edit_feature \(busy\)' 'actual Qt progress Cancel invoked during the commit job'
ui_expect_log 'Test preview cancellation requested: Box' 'interrupted background preview handled without a modal dialog'
ui_expect_no_log 'Progress dialog shown: preview' 'interactive preview never blocks the window'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
# The driver echoes its expect-never expressions into the console log.
# Inspect runtime messages only; the driver's own assertion separately
# checks the same errors in its recording, which excludes driver lines.
for message in 'The document was read while a job computes it' 'OCCT view event failed'; do
  if awk -v text="$message" 'index($0, "TestDriver:") != 1 && index($0, text) { found=1 }
      END { exit !found }' "$UI_LOG_FILE"; then
    ui_fail "runtime error: $message"
  else
    ui_pass "no runtime error: $message"
  fi
done
ui_expect_no_log 'Native UI: AppKit shell with embedded OCCT viewport' 'primary Qt frontend selected'
echo "Qt busy-command logs: $UI_OUT"
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt busy-command cancellation and restoration'
