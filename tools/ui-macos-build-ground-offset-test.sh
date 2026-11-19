#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/icons core/model-build tools/cli
# A BUILD profile placed off a face by Offset and Rotate, both edited later
# with Placement (mitcad#2046) on macOS, through the test driver
# (tools/ui-scripts/build-ground-offset.mitcad-ui, the same script as the
# Linux and Windows tests'), with the library of one synthetic BUILD
# profile of the mitering test made here and fetched from a local git
# repository: on a leg's bottom face Offset 20 mm and Rotate 30 deg are
# previewed and placed, then Placement moves the profile to Offset 50 mm
# and Rotate 45 deg in one undo step.
# Usage: tools/ui-macos-build-ground-offset-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-ground-offset}"
UI_TIMEOUT=300
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
export MITCAD_LIBRARIES_DIR=$files/cache
lib=$files/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
setup() {
  "$2" library init "$lib" --id test-profiles --name "Test profiles" --license MIT &&
    mkdir -p "$lib/profiles" &&
    "$2" run "$here/ui-scripts/build-end-meets-profile.json" --save "$lib/profiles/p-slot-test.mitcad" &&
    cp "$here/ui-scripts/build-end-meets-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    "$2" run "$here/ui-scripts/build-face-normal.json" --save "$files/build-face-normal.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-ground-offset.mitcad-ui \
  ui_run_app build-ground-offset --ui=qt --no-recovery --no-mcp --open "$files/build-face-normal.mitcad"
ui_expect_log 'BUILD face picked: Direction Face normal, at the ' 'a face picked chose Face normal and its centre'
ui_expect_log '; end A at 120[.]0, 230[.]0, -20[.]0; offset 20 mm, rotate 30 deg' 'the preview stands 20 off the face, turned 30 deg'
ui_expect_log '^Placement of C[0-9]+: offset 50 mm, rotate 45 deg$' 'Placement moved it to offset 50 mm, rotate 45 deg'
ui_expect_log '^BUILD rotate C[0-9]+: 45 deg to 135 deg$' 'Rotate 90 deg turned it a quarter further'
ui_expect_log 'TestDriver: done' 'a profile placed off a face and edited with Placement'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-ground-offset'
