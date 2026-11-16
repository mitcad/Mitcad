#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/JointCommands.cpp app/commands/BuildCommands.cpp app/framework/Selection.cpp
# check-all sources: app/MainWindow.cpp app/framework/TestDriver.cpp core/model-build tools/cli
# A BUILD part's joint and placement edited (mitcad#1885) on macOS,
# through the test driver (tools/ui-scripts/joint-placement-edit.mitcad-ui,
# the same script as the Linux and Windows tests'), with a library of one
# synthetic BUILD profile with a slot made here and fetched from a local
# git repository: a joint onto a mount frame with a frame override opens in
# Joint and keeps the override; Placement changes a side placement's From
# keeping the part in place, then its Offset, each in one undo step.
# Usage: tools/ui-macos-joint-placement-edit-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-joint-placement-edit}"
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
    "$2" run "$here/ui-scripts/joint-placement-edit-profile.json" --save "$lib/profiles/p-slot-test.mitcad" &&
    cp "$here/ui-scripts/joint-placement-edit-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/joint-placement-edit.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/joint-placement-edit.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/joint-placement-edit.mitcad-ui \
  ui_run_app joint-placement-edit --ui=qt --no-recovery --no-mcp --open "$files/joint-placement-edit.mitcad"
ui_expect_log 'Joint origin b keeps its frame override' 'the joint on the mount frame opened in Joint with its override'
ui_expect_log 'Placement of C3: from a, kept in place' 'a new From kept the part where it is'
ui_expect_log 'TestDriver: done' 'the joint and the placement of a BUILD part edited'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt joint-placement-edit'
