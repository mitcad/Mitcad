#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/framework/Command.hpp app/framework/Command.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionScript.cpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/exchange tools/cli
# check-all sources: tools/ui-scripts/move-imported-assembly.mitcad-ui tools/ui-scripts/move-imported-assembly.json
# check-all sources: tools/ui-scripts/move-imported-assembly-source.json
# Move/Copy, Move Object Components, on an imported STEP assembly
# (mitcad#1891) on macOS, through the test driver
# (tools/ui-scripts/move-imported-assembly.mitcad-ui, the same script as
# the Linux and Windows tests'): mitcad-cli writes a STEP assembly that
# places a part twice and imports it into a design with a body and a
# component in the root; a click on a body of the imported part takes the
# import's occurrence in the root, not the part's nested one; 40 mm along
# X moves the import only, and one undo puts it back.
# Usage: tools/ui-macos-move-imported-assembly-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-move-imported-assembly}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
# Both scripts name their files relative to the work folder.
(cd "$UI_OUT/files" && "$cli" run "$here/ui-scripts/move-imported-assembly-source.json" &&
  "$cli" run "$here/ui-scripts/move-imported-assembly.json" --save "$UI_OUT/files/move-imported-assembly.mitcad") \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/move-imported-assembly.mitcad-ui \
  ui_run_app move-imported-assembly --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/move-imported-assembly.mitcad"
ui_expect_log 'Move/Copy Objects: 1 component \[component +in O[0-9]+\]' \
  "a click on the imported part's body takes the import's occurrence"
ui_expect_log 'Moved 1 components \(' "the import's occurrence moved"
ui_expect_log 'TestDriver: done' 'Move/Copy of an imported STEP assembly'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
