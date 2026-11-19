#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/BrowserController.cpp app/MainWindowJoints.cpp core/model/src/joints.rs tools/cli
# check-all sources: tools/ui-scripts/flexible-free-parts.json tools/ui-scripts/flexible-free-parts.mitcad-ui
# Flexible on a subassembly without joints of its own (mitcad#2016) on
# macOS, through the test driver (tools/ui-scripts/flexible-free-parts.mitcad-ui,
# the same script as the Linux and Windows tests'): Gantry:1 is made
# Flexible from its row's menu and its free Joiner dragged along the
# root's slider, alone; Gantry:1's Carriage and Gantry:2 stay.
# Usage: tools/ui-macos-flexible-free-parts-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-flexible-free-parts}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/flexible-free-parts.json" --save "$UI_OUT/files/flexible-free-parts.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/flexible-free-parts.mitcad-ui \
  ui_run_app flexible-free-parts --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/flexible-free-parts.mitcad"
ui_expect_log 'TestDriver: done' 'Gantry:1 made flexible, its free Joiner dragged along the root'"'"'s joint alone'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
