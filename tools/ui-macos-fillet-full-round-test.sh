#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/CommandSession.cpp geometry/src/full_round.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli tools/ui-scripts/fillet-full-round.mitcad-ui tools/ui-scripts/fillet-full-round.json
# Fillet's Full Round (mitcad#2279) in the Qt frontend on macOS, through the
# test driver (tools/ui-scripts/fillet-full-round.mitcad-ui, the same script
# as the Linux and Windows tests'): choosing Full Round in Radius Type hides
# Radius; the top face clicked into Edges is refused, the status bar saying
# to pick the face's two edges; its two edges picked, OK adds the full
# round.
# Usage: tools/ui-macos-fillet-full-round-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-fillet-full-round}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/fillet-full-round.json" --save "$UI_OUT/files/fillet-full-round.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=100 \
  MITCAD_TEST_INPUT=$here/ui-scripts/fillet-full-round.mitcad-ui \
  ui_run_app fillet-full-round --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/fillet-full-round.mitcad"
ui_expect_log 'not taken: Full Round takes the two edges of the face it rounds away' \
  'the face is refused, the status bar saying to pick its two edges'
ui_expect_log 'Fillet Edges: 2 edges' 'the two edges are taken'
ui_expect_log 'Added Fillet' 'OK adds the full round'
ui_expect_log 'TestDriver: done' 'Full Round hides Radius and builds'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo "PASS: Full Round hides Radius, refuses a face and builds"
