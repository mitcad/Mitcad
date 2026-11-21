#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BuildCommands.cpp app/build/BuildModel.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/MainWindowJoints.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# Drags leave BUILD's placed parts at their Offset and a carriage slides
# within its travel limits (mitcad#2088) on macOS, through the test driver
# (tools/ui-scripts/build-drag-travel.mitcad-ui, the same script as the
# Linux and Windows tests'), with a library of a synthetic profile, guide
# rail and carriage made here and fetched from a local git repository: a
# drag of a placed profile is refused, Placement moves it and the part on
# it follows, the carriage stops at its limits.
# Usage: tools/ui-macos-build-drag-travel-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-build-drag-travel}"
UI_TIMEOUT=300
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
export MITCAD_LIBRARIES_DIR=$files/cache
lib=$files/test-profiles
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
setup() {
  "$2" library init "$lib" --id test-profiles --name "Test profiles" --license MIT &&
    mkdir -p "$lib/profiles" "$lib/linear" &&
    "$2" run "$here/ui-scripts/build-drag-travel-profile.json" --save "$lib/profiles/p-drag-test.mitcad" &&
    "$2" run "$here/ui-scripts/build-drag-travel-rail.json" --save "$lib/linear/rail-drag-test.mitcad" &&
    "$2" run "$here/ui-scripts/build-drag-travel-carriage.json" --save "$lib/linear/carriage-drag-test.mitcad" &&
    cp "$here/ui-scripts/build-drag-travel-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test parts' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|" "$here/ui-scripts/build-drag-travel.json" > "$files/build-drag-travel.json" &&
    "$2" run "$files/build-drag-travel.json" --save "$files/build-drag-travel.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/build-drag-travel.mitcad-ui \
  ui_run_app build-drag-travel --ui=qt --no-recovery --no-mcp --open "$files/build-drag-travel.mitcad"
ui_expect_log '^Drag refused: Arm:1 stays where it was placed: move it with Placement$' 'a drag of the arm is refused, naming Placement'
ui_expect_log '^Placement of C[0-9]+: from a, offset 300 mm$' 'Placement moved the arm, the post on it following'
ui_expect_log 'Carriage on Rail tx 380 mm' 'the carriage stopped flush with the rail end'
ui_expect_log '^BUILD travel of C[0-9]+: min 20, max 380$' 'Placement read the carriage travel limits'
ui_expect_log 'Carriage on Rail tx 300 mm' 'the carriage stopped at its own Max'
ui_expect_log 'TestDriver: done' 'drags and travel limits of BUILD parts'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt build-drag-travel'
