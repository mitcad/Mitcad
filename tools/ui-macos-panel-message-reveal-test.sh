#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandPanel.hpp app/framework/TestDriver.cpp
# check-all sources: tools/cli tools/ui-scripts/panel-message-reveal.mitcad-ui tools/ui-scripts/panel-message-top.json
# A panel's new error scrolled into view by the panel itself (mitcad#1884)
# on macOS, through the test driver (tools/ui-scripts/panel-message-reveal.mitcad-ui,
# the same script as the Linux and Windows tests'): a cube's Extrude edited
# in a 1280 x 600 window, Docked and then Floating, its Distance typed -5 (Symmetric)
# fails the preview, and the panel's error box and the Distance field typed
# in lie within the page's view ("Panel Extrude view y A to B") without the
# test scrolling to the box; Docked, the page scrolled to it by itself.
# Usage: tools/ui-macos-panel-message-reveal-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-panel-message-reveal}"
UI_TIMEOUT=180
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/panel-message-reveal.mitcad
"$2" run "$here/ui-scripts/panel-message-top.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}

# in_view layout what pattern: the y of the last line from the error on
# matching the pattern (… at x,y) lies within the page's view.
in_view() {
  local y
  y=$(printf '%s\n' "$lines" | grep -E "$3" | tail -1 | sed -n 's/.* at [0-9-]*,\([0-9-]*\).*/\1/p')
  if [ -n "$y" ] && [ "$y" -ge "$top" ] && [ "$y" -le "$bottom" ]; then
    ui_pass "$1: $2 at y $y, within the page's view (y $top to $bottom)"
  else
    ui_fail "$1: $2 at y ${y:-?}, outside the page's view (y $top to $bottom)"
  fi
}

for layout in docked floating; do
  echo "--- $layout"
  MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
    MITCAD_TEST_INPUT=$here/ui-scripts/panel-message-reveal.mitcad-ui \
    ui_run_app "panel-message-reveal-$layout" --ui=qt --no-recovery --no-mcp --chrome=$layout --open "$design"
  ui_expect_log '^Panel Extrude message box: error: ' "$layout: its error in the panel's box"
  ui_expect_log 'TestDriver: done' "$layout: the error is brought into view"
  ui_expect_no_log 'TestDriver: FAIL' "$layout: no failed driver assertion"
  lines=$(sed -n '/^Panel Extrude message box: error: /,/^Command Extrude cancelled/p' "$UI_LOG_FILE")
  view=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude view y [0-9-]+ to [0-9-]+, ' | tail -1)
  top=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y \([0-9-]*\) to .*/\1/p')
  bottom=$(printf '%s\n' "$view" | sed -n 's/^Panel Extrude view y [0-9-]* to \([0-9-]*\),.*/\1/p')
  if [ -z "$top" ] || [ -z "$bottom" ]; then
    ui_fail "$layout: the page's view was not logged after the error"
    continue
  fi
  in_view "$layout" "the error box" '^Panel Extrude message at '
  in_view "$layout" "the Distance field typed in" '^Panel Extrude input distance at [0-9-]+,[0-9-]+$'
  if [ "$layout" = docked ]; then
    scrolled=$(printf '%s\n' "$lines" | grep -E '^Panel Extrude message revealed: ' | tail -1 |
      sed -n 's/.*, scrolled \([0-9]*\) of [0-9]*$/\1/p')
    if [ "${scrolled:-0}" -gt 0 ]; then
      ui_pass "docked: the page scrolled $scrolled px to the error by itself"
    else
      ui_fail "docked: the page did not scroll for the error (the panel is no longer higher than its page at 1280 x 600)"
    fi
  fi
done
[ "$UI_FAILURES" = 0 ] || exit 1
