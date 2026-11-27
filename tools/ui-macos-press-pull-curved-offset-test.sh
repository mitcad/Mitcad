#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/commands/CommandSupport.cpp app/framework/ManipulatorOverlay.cpp
# check-all sources: app/framework/TestDriver.cpp tools/cli geometry/src/datum.cpp geometry/src/faceops.cpp
# check-all sources: core/model/src/features/offset_face.rs
# Press Pull's Offset on a curved face (mitcad#2509) on macOS, through the
# test driver (tools/ui-scripts/press-pull-curved-offset.mitcad-ui, the
# same script as the Linux and Windows tests') on the design that
# mitcad-cli makes (tools/ui-scripts/press-pull-curved-offset.json): on a
# tapered Extrude's side the arrow starts on the face along its outward
# normal; dragged, the preview is ok and OK commits a body that grew; the
# dragged distance typed gives the same body, 0.5 mm typed the grown
# frustum's volume.
# Usage: tools/ui-macos-press-pull-curved-offset-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-press-pull-curved-offset}"
UI_TIMEOUT=300
cli=$2
files=$UI_OUT/files
mkdir -p "$files"
"$cli" run "$here/ui-scripts/press-pull-curved-offset.json" --save "$files/press-pull-curved-offset.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_TEST_INPUT=$here/ui-scripts/press-pull-curved-offset.mitcad-ui \
  ui_run_app press-pull-curved-offset --ui=qt --no-recovery --no-mcp --open "$files/press-pull-curved-offset.mitcad"
ui_expect_log 'Press Pull made offset face, distance ' 'the side offset by the dragged arrow'
ui_expect_log 'Press Pull made offset face, distance 0.5' 'the side offset by 0.5 mm typed'
ui_expect_log 'TestDriver: done' 'the arrow on the curved face dragged and typed'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
