#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# Joint conditions of profiles along sketch lines (mitcad#2200) on macOS,
# through the test driver (tools/ui-scripts/build-joints.mitcad-ui, the same
# script as the Linux and Windows tests'), with a library of one synthetic
# BUILD profile made here and fetched from a local git repository: Profile
# along sketch lines with Corners Butt, an L swapped in Ends, a T set to Far
# Side with Shift + click.
# Usage: tools/ui-macos-build-joints-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-joints}"
UI_TIMEOUT=360
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
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-joints.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/build-joints.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-joints.mitcad-ui \
  ui_run_app build-joints --ui=qt --no-recovery --no-mcp --open "$files/build-joints.mitcad"
ui_expect_log 'BUILD profile along sketch lines: ' 'the sketch picked from the Browser'
ui_expect_log 'profile[(]s[)] along sketch lines' 'Place made the profiles along the lines'
ui_expect_log 'joint butt with ' 'Ends swapped the L that runs through'
ui_expect_log 'joint far_side with ' 'Shift + click set the T to Far Side'
ui_expect_log 'TestDriver: done' 'joints placed butted, swapped and set to Far Side'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-joints'
