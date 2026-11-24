#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildEndCapCommand.cpp app/commands/BuildCommands.cpp app/build/BuildModel.cpp
# check-all sources: app/build/BuildLists.cpp app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# End Cap (mitcad#2243) on macOS, through the test driver
# (tools/ui-scripts/build-end-cap.mitcad-ui, the same script as the Linux
# and Windows tests'), with a library of one synthetic steel tube made here
# and fetched from a local git repository: a click near each end chooses
# it, both ends are capped outward in one undo step, one is switched to
# Inward (the tube cut back in the cut list), and the lists' window shows
# the plates.
# Usage: tools/ui-macos-build-end-cap-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-end-cap}"
UI_TIMEOUT=300
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
export MITCAD_LIBRARIES_DIR=$files/cache
lib=$files/test-tubes
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
setup() {
  "$2" library init "$lib" --id test-tubes --name "Test tubes" --license MIT &&
    mkdir -p "$lib/tubes" &&
    "$2" run "$here/ui-scripts/build-end-cap-profile.json" --save "$lib/tubes/tube-test.mitcad" &&
    cp "$here/ui-scripts/build-end-cap-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/build-end-cap.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/build-end-cap.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-end-cap.mitcad-ui \
  ui_run_app build-end-cap --ui=qt --no-recovery --no-mcp --open "$files/build-end-cap.mitcad"
ui_expect_log 'End Cap: 2 cap\(s\) outward' 'both ends capped outward in one step'
ui_expect_log 'End Cap lists: Tube 40x40x3 L=600 595 mm x1; plates: 2 x Plate 40 x 40 x 5' \
  'end B switched to inward: the tube cut back, two plates'
ui_expect_log 'Cut list plates: 2 x Plate 40 x 40 x 5' 'the lists show the plates'
ui_expect_log 'TestDriver: done' 'a tube capped at both ends'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-end-cap'
