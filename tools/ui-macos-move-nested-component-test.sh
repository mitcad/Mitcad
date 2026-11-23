#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveTarget.cpp app/commands/MoveTarget.hpp
# check-all sources: app/framework/Command.hpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/recompute.rs core/model/src/features/moves.rs tools/cli
# check-all sources: app/commands/MoveDragMotion.cpp app/commands/MoveDragMotion.hpp
# check-all sources: tools/ui-scripts/move-nested-component.mitcad-ui tools/ui-scripts/move-nested-component.json
# Move/Copy, Move Object Components, on a component three levels deep
# picked by its Browser row with the root active (mitcad#2302) on macOS,
# through the test driver (tools/ui-scripts/move-nested-component.mitcad-ui,
# the same script as the Linux and Windows tests'): the move goes into the
# component the picked one is placed in; 40 mm along X moves that part
# only, one undo puts it back, and two picks in different components are
# refused in words; the move made again and edited from the timeline opens
# in its own component and changes to 25 mm (mitcad#2305).
# Usage: tools/ui-macos-move-nested-component-test.sh <app> <mitcad-cli> [output directory]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/ui-macos-lib.sh"
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: $0 <app> <mitcad-cli> [output directory]" >&2
  exit 2
fi
ui_init "$1" "${3:-ui-macos-move-nested-component}"
UI_TIMEOUT=240
mkdir -p "$UI_OUT/files"
cli=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
"$cli" run "$here/ui-scripts/move-nested-component.json" --save "$UI_OUT/files/move-nested-component.mitcad" \
  > "$UI_OUT/setup.log" 2>&1 || {
  cat "$UI_OUT/setup.log" >&2
  exit 1
}
MITCAD_TEST_WORK=$UI_OUT/files MITCAD_TEST_STEP_MS=70 \
  MITCAD_TEST_INPUT=$here/ui-scripts/move-nested-component.mitcad-ui \
  ui_run_app move-nested-component --ui=qt --no-recovery --no-mcp --open "$UI_OUT/files/move-nested-component.mitcad"
ui_expect_log 'Move/Copy: moving in C2 \(at O1/O2\), the parent of O1/O2/O3' \
  "the move goes into the picked component's parent"
ui_expect_log 'Moved 1 components \(' "the nested component moved"
ui_expect_log 'Preview Move/Copy: not built: These components are placed in different components' \
  'picks in two components refused in words'
ui_expect_log 'Edited F' 'the nested move edited from the timeline'
ui_expect_log 'TestDriver: done' 'Move/Copy of a nested component'
ui_expect_no_log '^TestDriver: FAIL' 'no failed driver assertion'
[ "$UI_FAILURES" = 0 ] || exit 1
