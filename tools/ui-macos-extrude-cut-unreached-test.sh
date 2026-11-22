#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CutTargetsPanel.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp geometry/src/sweep.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-cut-unreached.mitcad-ui tools/ui-scripts/extrude-cut-unreached.json
# An Extrude Cut whose preview fails before its cut's targets are found
# (mitcad#2118, GitHub mitcad/Mitcad#492) on macOS, through the test driver
# (tools/ui-scripts/extrude-cut-unreached.mitcad-ui, the same script as the
# Linux and Windows tests'): Extrude2 edited in a 1280 x 600 window, Docked
# and then Floating, Operation New Body and Cut again. The Affected
# components line last says "Affected components could not be found: …" in
# words, and the panel's error box lies within the page's view ("Panel
# Extrude view y A to B") without the test scrolling to it.
# Usage: tools/ui-macos-extrude-cut-unreached-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-extrude-cut-unreached}"
UI_TIMEOUT=180
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/extrude-cut-unreached.mitcad
"$2" run "$here/ui-scripts/extrude-cut-unreached.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

for layout in docked floating; do
  echo "--- $layout"
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/extrude-cut-unreached.mitcad-ui \
    ui_run_app "extrude-cut-unreached-$layout" --ui=qt --no-recovery --no-mcp --chrome=$layout --open "$design"
  ui_expect_log 'Cut targets status: Affected components could not be found: ' "$layout: the list says why in words"
  ui_expect_log 'TestDriver: done' "$layout: the message is brought into view again"
  ui_expect_no_log 'TestDriver: FAIL' "$layout: no failed driver assertion"
  lines=$(sed -n '1,/^Command Extrude cancelled/p' "$UI_LOG_FILE")
  status=$(printf '%s\n' "$lines" | grep -E '^Cut targets status: ' | tail -1)
  case "$status" in
    'Cut targets status: Affected components could not be found: '*) ui_pass "$layout: the list's last line: $status" ;;
    *) ui_fail "$layout: the list's last line is not the failure in words: $status" ;;
  esac
  view=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude view y [0-9-]+ to [0-9-]+, ' | tail -1)
  top=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y \([0-9-]*\) to .*/\1/p')
  bottom=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y [0-9-]* to \([0-9-]*\),.*/\1/p')
  y=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude message at ' | tail -1 | sed -n 's/.* at [0-9-]*,\([0-9-]*\).*/\1/p')
  if [ -n "$top" ] && [ -n "$bottom" ] && [ -n "$y" ] && [ "$y" -ge "$top" ] && [ "$y" -le "$bottom" ]; then
    ui_pass "$layout: the error box at y $y, within the page's view (y $top to $bottom)"
  else
    ui_fail "$layout: the error box at y ${y:-?}, outside the page's view (y ${top:-?} to ${bottom:-?})"
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1
