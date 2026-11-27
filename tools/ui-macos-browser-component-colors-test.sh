#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserPanel.cpp app/browser/ComponentColors.hpp app/browser/TimelineWidget.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/browser-component-colors.mitcad-ui
# The Browser's component color bars (mitcad#1876) on macOS, through the
# test driver (tools/ui-scripts/browser-component-colors.mitcad-ui, the
# same script as the Linux and Windows tests'): every component,
# occurrence and body row logs its component's timeline color, the root's
# rows the section caps' orange, both placements of a component the same;
# the colors follow a rename, a move into another component, a new
# component and its undo, and stay in the dark theme.
# Usage: tools/ui-macos-browser-component-colors-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-browser-component-colors}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/browser-component-colors.json" --save "$UI_OUT/files/browser-component-colors.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/browser-component-colors.mitcad-ui \
  ui_run_app browser-component-colors --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/browser-component-colors.mitcad"
ui_expect_log 'Browser colors: Root #f29e1a, ' "the root's rows in the default orange"
ui_expect_log 'Root/Base:1/Bolt:2 #' "the moved occurrence's rows keep its color"
ui_expect_log 'TestDriver: done' 'the component color bars'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
