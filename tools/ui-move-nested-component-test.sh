#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveTarget.cpp app/commands/MoveTarget.hpp
# check-all sources: app/framework/Command.hpp app/framework/CommandSession.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/recompute.rs core/model/src/features/moves.rs tools/cli
# check-all sources: app/commands/MoveDragMotion.cpp app/commands/MoveDragMotion.hpp
# check-all sources: tools/ui-scripts/move-nested-component.mitcad-ui tools/ui-scripts/move-nested-component.json
# Move/Copy, Move Object Components, on a component three levels deep
# picked by its Browser row with the root active (mitcad#2302), through the
# real UI: the shared input script
# tools/ui-scripts/move-nested-component.mitcad-ui (also run by
# tools/ui-windows-move-nested-component-test.ps1 and
# tools/ui-macos-move-nested-component-test.sh) on the design mitcad-cli
# makes from tools/ui-scripts/move-nested-component.json. The move goes
# into the component the picked one is placed in; 40 mm along X moves that
# part only, one undo puts it back, and two picks in different components
# are refused in words; the move made again and edited from the timeline
# opens in its own component and changes to 25 mm (mitcad#2305).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-nested-component-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-move-nested-component.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
"$cli" run "$here/ui-scripts/move-nested-component.json" --save "$work/move-nested-component.mitcad" \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the design"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/move-nested-component.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/move-nested-component.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "move nested component driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Move/Copy: moving in C2 (at O1/O2), the parent of O1/O2/O3' "the move goes into the picked component's parent"
ui_expect_log 'Moved 1 components (' "the nested component moved"
ui_expect_log 'Preview Move/Copy: not built: These components are placed in different components' \
  "picks in two components refused in words"
ui_expect_log 'Edited F' "the nested move edited from the timeline"
ui_expect_log 'TestDriver: done' "Move/Copy of a nested component"
ui_finish 'UI Move/Copy of a component picked inside a sub-assembly'
