#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ConstructCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/construct-reverse-normal.mitcad-ui
# Offset Plane's Reverse Normal and signed Distance (mitcad#2048) on macOS,
# through the test driver (tools/ui-scripts/construct-reverse-normal.mitcad-ui,
# the same script as the Linux and Windows tests'): toggling Reverse Normal
# turns the preview's normal marker over, and Distance -10 mm puts the
# plane on the other side of the face.
# Usage: tools/ui-macos-construct-reverse-normal-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-construct-reverse-normal}"
UI_TIMEOUT=300
mkdir -p "$UI_OUT/files"
"$2" run "$here/ui-scripts/construct-reverse-normal.json" --save "$UI_OUT/files/construct-reverse-normal.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/construct-reverse-normal.mitcad-ui \
  ui_run_app construct-reverse-normal --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/construct-reverse-normal.mitcad"
ui_expect_log 'Preview marker direction Offset Plane normal: 0[.]000,0[.]000,-1[.]000' \
  'Reverse Normal turned the marker over'
ui_expect_log 'Added plane Plane[0-9]+: origin [(][-0-9.e]+, [-0-9.e]+, 20[)], normal [(]0, 0, -1[)]' \
  'Distance -10 mm put the plane on the other side of the face'
ui_expect_log 'TestDriver: done' 'Reverse Normal and a negative distance'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
