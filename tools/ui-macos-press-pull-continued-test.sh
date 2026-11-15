#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/ManipulatorOverlay.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/features/press_pull.rs core/model-api/src/press_pull.rs geometry/src/press_pull.cpp
# Press Pull on a face continued (mitcad#1850) on macOS, through the test
# driver (tools/ui-scripts/press-pull-continued.mitcad-ui, the same script
# as the Linux and Windows tests') on the design that mitcad-cli makes
# (tools/ui-scripts/press-pull-continued.json): on a bent tube's end
# Continue offers Revolve1's own edit; Along the Walls with its ring
# dragged past the walls' inner radius, then 25 mm; Straight with a Scale
# taper of 1.1 on a T-slot bar's end.
# Usage: tools/ui-macos-press-pull-continued-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-press-pull-continued}"
UI_TIMEOUT=300
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
"$cli" run "$here/ui-scripts/press-pull-continued.json" --save "$files/press-pull-continued.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/press-pull-continued.mitcad-ui \
  ui_run_app press-pull-continued --ui=qt --no-recovery --no-mcp --open "$files/press-pull-continued.mitcad"
ui_expect_log 'Press Pull opened Revolve1 for its angle' "the tube's end opened Revolve1 at its Angle"
ui_expect_log 'Press Pull made press pull along the walls, distance 25' "Along the Walls on the bent tube's end"
ui_expect_log 'Press Pull made press pull straight, distance 10' 'Straight with a Scale taper on the T-slot bar'
ui_expect_log 'TestDriver: done' 'the bent tube and the T-slot bar continued'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
