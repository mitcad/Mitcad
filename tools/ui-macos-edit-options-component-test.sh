#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/library/EditOptionsDialog.cpp app/library/PartOptions.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/document/component_options.rs tools/cli
# Edit Options of an inserted library part (mitcad#1929) on macOS, through
# the test driver (tools/ui-scripts/edit-options-component.mitcad-ui, the
# same script as the Linux and Windows tests'), with a library of one
# synthetic plate with options made here and fetched from a local git
# repository: Width changed in Edit Options is previewed without a refusal,
# and OK keeps it in one undo step.
# Usage: tools/ui-macos-edit-options-component-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-edit-options-component}"
UI_TIMEOUT=300
files=$UI_OUT/files
rm -rf "$files"
mkdir -p "$files"
export MITCAD_LIBRARIES_DIR=$files/cache
lib=$files/test-options
g() { git -C "$1" -c user.name=T -c user.email=t@example.invalid "${@:2}" > /dev/null; }
setup() {
  "$2" library init "$lib" --id test-options --name "Test options" --license MIT &&
    "$2" run "$here/ui-scripts/edit-options-component-plate.json" --save "$files/plate.mitcad" &&
    "$2" library add "$lib" "$files/plate.mitcad" --id plate --name Plate --designation Plate --category plates &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/edit-options-component.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/edit-options-component.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/edit-options-component.mitcad-ui \
  ui_run_app edit-options-component --ui=qt --no-recovery --no-mcp --open "$files/edit-options-component.mitcad"
ui_expect_log 'TestDriver: done' 'the preview took the value and OK changed the plate in one undo step'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
