#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/commands/CommandSupport.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/extrude.rs core/ffi/src/kernel/face_extrude.rs
# check-all sources: core/cpp/bridge/face_extrude.cpp geometry/src/face_extrude.cpp geometry/src/extrude.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-face-to-object.mitcad-ui tools/ui-scripts/extrude-face-to-object.json
# Extrude of a body face up to another face of the same body (mitcad#2033)
# on macOS, through the test driver (tools/ui-scripts/extrude-face-to-object.mitcad-ui,
# the same script as the Linux and Windows tests') on the stepped block
# mitcad-cli makes (tools/ui-scripts/extrude-face-to-object.json): the lower
# step's top is the profile, Extent To Object takes the higher step's top
# (To Selected Face), the automatic Join builds and OK adds it: one level
# body of 72000 mm3.
# Usage: tools/ui-macos-extrude-face-to-object-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-extrude-face-to-object}"
UI_TIMEOUT=300
cli=$2
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
"$cli" run "$here/ui-scripts/extrude-face-to-object.json" --save "$files/stepped.mitcad" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$files MITCAD_TEST_STEP_MS=100 MITCAD_LOG_PICKS=1 \
  MITCAD_TEST_INPUT=$here/ui-scripts/extrude-face-to-object.mitcad-ui \
  ui_run_app extrude-face-to-object --ui=qt --no-recovery --no-mcp --open "$files/stepped.mitcad"
ui_expect_log 'Extrude Profiles: 1 face [[]face F2:end[(]' 'the lower top is the profile'
ui_expect_log 'Extrude Object: 1 face [[]face F4:end[(]' 'the higher top of the same body is the object'
ui_expect_log 'Added extrude [(]Join[)], distance to object, of a face' 'the Join up to the face added'
ui_expect_log '^TestDriver: done' 'Extrude of a face up to a face of its own body'
ui_expect_no_log 'TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: extrude of a body face up to an object'
