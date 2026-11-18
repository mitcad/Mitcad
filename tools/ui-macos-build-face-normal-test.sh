#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp app/icons core/model-build tools/cli
# A BUILD profile started on a face (mitcad#2000) on macOS, through the
# test driver (tools/ui-scripts/build-face-normal.mitcad-ui, the same
# script as the Linux and Windows tests'), with the library of one
# synthetic BUILD profile of the mitering test made here and fetched from
# a local git repository: on a leg's bottom face Direction Face normal runs
# it down from the face's centre, its sides along the face's longest edge,
# Rotate offered; on the XY plane Direction -Z runs it down.
# Usage: tools/ui-macos-build-face-normal-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-face-normal}"
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
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-face-normal.mitcad-ui \
  ui_run_app build-face-normal --ui=qt --no-recovery --no-mcp --open "$files/build-face-normal.mitcad"
ui_expect_log 'BUILD face picked: Direction Face normal, at the ' 'a face picked chose Face normal and its centre'
ui_expect_log '; along normal; runs 0.000, 0.000, -1.000; x 0.000, ' 'the preview runs down from the face, x along its edge'
ui_expect_log '; along -z; runs 0.000, 0.000, -1.000; ' '-Z on the XY plane runs it down'
ui_expect_log 'TestDriver: done' 'a profile placed on a face along its normal and on a plane along -Z'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-face-normal'
