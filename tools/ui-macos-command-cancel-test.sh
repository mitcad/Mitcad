#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession app/framework/CommandPanel app/framework/TestDriver
# check-all sources: app/framework/DocumentSession
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
scenario=${MITCAD_TEST_CANCEL_CASE:-all}
case "$scenario" in
  all|previews|cancel|quit) ;;
  *) echo "Unknown command-cancel scenario: $scenario" >&2; exit 2 ;;
esac
input=$UI_OUT/command-cancel.mitcad-ui
python3 - "$here/ui-scripts/macos-command-cancel.mitcad-ui" "$input" "$scenario" <<'PYCODE' || exit 1
import pathlib, sys
source, output = map(pathlib.Path, sys.argv[1:3])
text = source.read_text()
scenario = sys.argv[3]
if scenario != "all":
    prefix, rest = text.split("# Replace the value while", 1)
    replacement, rest = ("# Replace the value while" + rest).split("# Return to the already", 1)
    cache, rest = ("# Return to the already" + rest).split("# Interrupt the background", 1)
    cancellation, quit_case = ("# Interrupt the background" + rest).split("# Exit requests Quit", 1)
    guard = "expect-never 'The document was read while a job computes it|OCCT view event failed'\n"
    # Every independent case first makes the same real, unsaved 12 mm edit.
    # Capture its depth rather than relying on another process's history.
    baseline = prefix + replacement + "capture-undo-depth committed_depth\n"
    if scenario == "previews":
        text = prefix + replacement + cache + guard
    elif scenario == "cancel":
        text = baseline + cancellation + guard
    else:
        text = baseline + "# Exit requests Quit" + quit_case
output.write_text(text)
PYCODE
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
  MITCAD_TEST_INPUT=$input \
  ui_run_app cancel --ui=qt --chrome="${MITCAD_TEST_CHROME:-floating}" --no-recovery --no-mcp --open "$MITCAD_TEST_WORK/root-design.mitcad"
ui_expect_log 'TestDriver: done' 'Qt preview, pending Return, commit cancellation and close restoration completed'
if [ "$scenario" = all ] || [ "$scenario" = cancel ]; then
  ui_expect_log 'Qt test: clicked progress Cancel during edit_feature \(busy\)' 'actual Qt progress Cancel invoked during the commit job'
  ui_expect_log 'Test preview cancellation requested: Box' 'interrupted background preview handled without a modal dialog'
fi
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
