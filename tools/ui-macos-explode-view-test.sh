#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowExplode.cpp app/motion/ExplodeViewDialog.cpp app/commands/ExplodeCommands.cpp
# check-all sources: app/view/ExplodeTrailOverlay.cpp app/browser app/drawings app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/explode.rs core/model-api/src/explode.rs tools/cli
# check-all sources: tools/ui-scripts/explode-view.mitcad-ui tools/ui-scripts/explode-view-check.json
# Exploded views (mitcad#2113) on macOS, the same shared script as the
# Linux and Windows tests' (tools/ui-scripts/explode-view.mitcad-ui) on the
# design mitcad-cli makes (tools/ui-scripts/explode-view.json): Auto
# Explode, a step by Explode Step's handle, the steps played back and
# forth, the Browser's row, a drawing view with the explode view, Measure
# on the exploded bodies and a step along a picked edge (mitcad#2328); the
# saved design checked with explode-view-check.json.
# Usage: tools/ui-macos-explode-view-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-explode-view}"
UI_TIMEOUT=480
mkdir -p "$UI_OUT/files"
design=$UI_OUT/files/explode-view.mitcad
"$2" run "$here/ui-scripts/explode-view.json" --save "$design" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/explode-view.mitcad-ui \
  ui_run_app explode-view --ui=qt --no-recovery --no-mcp --open "$design"
ui_expect_log 'TestDriver: done' 'explode-view: the shared steps'
ui_expect_no_log '^TestDriver: FAIL' 'explode-view: no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
if "$2" run "$here/ui-scripts/explode-view-check.json" --open "$design" > "$UI_OUT/check.log" 2>&1; then
  ui_pass 'the saved design (explode-view-check.json)'
else
  cat "$UI_OUT/check.log" >&2
  ui_fail 'the saved design (explode-view-check.json)'
  exit 1
fi
[ "$UI_FAILURES" = 0 ] || exit 1
