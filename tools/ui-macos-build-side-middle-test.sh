#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# A profile centred across a side with two slots (mitcad#2002) on macOS,
# through the test driver (tools/ui-scripts/build-side-middle.mitcad-ui,
# the same script as the Linux and Windows tests'), with a library of one
# synthetic 20x40 BUILD profile made here and fetched from a local git
# repository: a click near the side's centre line centres the profile
# across the side, one near a slot puts it on that slot.
# Usage: tools/ui-macos-build-side-middle-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-side-middle}"
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
    "$2" run "$here/ui-scripts/build-side-middle-profile.json" --save "$lib/profiles/p-two-slot-test.mitcad" &&
    cp "$here/ui-scripts/build-side-middle-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-side-middle.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/build-side-middle.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-side-middle.mitcad-ui \
  ui_run_app build-side-middle --ui=qt --no-recovery --no-mcp --open "$files/build-side-middle.mitcad"
ui_expect_log '; across middle' 'a click near the centre line previews the profile centred across the side'
ui_expect_log '; across slot' 'a click near a slot previews it on that slot'
ui_expect_log 'TestDriver: done' 'profiles placed centred across a side and on a slot'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-side-middle'
