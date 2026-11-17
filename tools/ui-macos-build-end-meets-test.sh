#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# Mitering two BUILD profiles placed apart (mitcad#1940) on macOS, through
# the test driver (tools/ui-scripts/build-end-meets.mitcad-ui, the same
# script as the Linux and Windows tests'), with a library of one synthetic
# BUILD profile with a slot made here and fetched from a local git
# repository: the bound-length refusal says how to miter; Ends labels the
# ends, a click near the corner chooses the end, and Also the Component It
# Meets makes the shared miter in one undo step.
# Usage: tools/ui-macos-build-end-meets-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-end-meets}"
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
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-end-meets.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/build-end-meets.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-end-meets.mitcad-ui \
  ui_run_app build-end-meets --ui=qt --no-recovery --no-mcp --open "$files/build-end-meets.mitcad"
ui_expect_log 'Miter Auto or Shared' 'the bound length refused with the way to miter'
ui_expect_log 'BUILD ends of C2: end a picked, meets end b of C1' 'a click near the corner chose the end that meets'
ui_expect_log 'Ends of Arm: end a shared miter with end b of C1' 'Also the Component It Meets made the shared miter'
ui_expect_log 'TestDriver: done' 'two profiles placed apart mitered'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-end-meets'
