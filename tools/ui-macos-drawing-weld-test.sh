#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp app/icons core/model-api/src/drawing_welds.rs
# check-all sources: core/model-drawing/src/weld.rs core/model-build tools/cli
# check-all sources: tools/ui-scripts/drawing-weld.mitcad-ui tools/ui-scripts/drawing-weld.json
# Weld symbols on a drawing (mitcad#2350) on macOS: the shared input script
# tools/ui-scripts/drawing-weld.mitcad-ui of tools/ui-drawing-weld-test.sh in
# the Qt frontend, with the mitering test's synthetic BUILD library made
# here and fetched from a local git repository: Weld Symbol puts a fillet
# weld on an edge, Propose Welds one all around a T joint.
# Usage: tools/ui-macos-drawing-weld-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-drawing-weld}"
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
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/drawing-weld.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/welded.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/drawing-weld.mitcad-ui \
  ui_run_app drawing-weld --ui=qt --no-recovery --no-mcp --open "$files/welded.mitcad"
ui_expect_log '^Drawing command: .*"type":"weld"' 'the weld symbol asked for'
ui_expect_log '^Drawing command: .*"cmd":"propose_drawing_welds"' 'the welds proposed'
ui_expect_log '^TestDriver: done' 'a fillet weld placed on an edge and one proposed all around the T joint'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
echo 'PASS: Qt drawing weld symbols'
