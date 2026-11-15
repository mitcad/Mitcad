#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DerivedPartCommands.cpp app/MainWindow.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/document/component_options.rs tools/cli
# The edit panel of a derived part (mitcad#1812) on macOS, through the test
# driver (tools/ui-scripts/derived-part-edit.mitcad-ui, the same script as
# the Linux and Windows tests'), with a library of one synthetic BUILD
# profile (two rows) made here and fetched from a local git repository:
# the profile's timeline cell opens Derived Component, its length and row
# change in one undo step and Undo restores them; a feature type without an
# edit panel shows the plain message.
# Usage: tools/ui-macos-derived-part-edit-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-derived-part-edit}"
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
    "$2" run "$here/ui-scripts/derived-part-edit-profile.json" --save "$lib/profiles/p-rows-test.mitcad" &&
    cp "$here/ui-scripts/derived-part-edit-library.json" "$lib/mitcad-library.json" &&
    g "$lib" add -A && g "$lib" commit -q -m 'Test part' && g "$lib" tag v1.0.0 &&
    "$2" library fetch "$lib" &&
    sed "s|LIBRARY_URL|$lib|g" "$here/ui-scripts/derived-part-edit.json" > "$files/design.json" &&
    "$2" run "$files/design.json" --save "$files/derived-part-edit.mitcad"
}
setup "$1" "$2" > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_STEP_MS=70 MITCAD_TEST_INPUT=$here/ui-scripts/derived-part-edit.mitcad-ui \
  ui_run_app derived-part-edit --ui=qt --no-recovery --no-mcp --open "$files/derived-part-edit.mitcad"
ui_expect_log 'TestDriver: done' 'the panel opened in place, its edit was one undo step, and a feature without a panel says so'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
