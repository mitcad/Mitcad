#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MoveCommands.cpp app/commands/MoveDragMotion.cpp app/commands/CommandSupport.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/framework/TestDriver.cpp core/model/src/features/moves.rs tools/cli
# Move/Copy's Free Move handles (mitcad#1580, mitcad#1581, mitcad#1583,
# mitcad#1707),
# the shared input script tools/ui-scripts/move-handles.mitcad-ui (also run
# by tools/ui-windows-move-handles-test.ps1): an edit's rings at the
# middle of what it moves, where the feature moves it, for a body, a
# component and a linked component (8650 mm along Y) and a turned body;
# OK without changes keeps the feature's matrix (the saved design checked
# with tools/ui-scripts/move-handles-check.json); with X Distance set the
# Y and Z arrows start at the moved middle, a drag of the Z arrow goes
# along Z from there; the arrows and rings in the axis colors; in the
# front view the X and Z arrows grabbed over the rings seen edge-on, in
# HandleActive while hovered and dragged (mitcad#1707).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-move-handles-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-move-handles.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=$WORK
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/move-handles.mitcad-ui
# The linked part first, then the design that links it from this folder.
"$CLI" run "$here/ui-scripts/move-handles-part.json" --save "$WORK/part.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail 'the linked part could not be made'; }
sed "s#LINKED_DIR#$WORK#g" "$here/ui-scripts/move-handles.json" > "$WORK/design.json"
"$CLI" run "$WORK/design.json" --save "$WORK/design.mitcad" > "$WORK/cli.log" 2>&1 ||
  { cat "$WORK/cli.log"; ui_fail 'the design could not be made'; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$WORK/design.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Move/Copy handles driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the edits, the moved arrows and the axis colors'
"$CLI" run "$here/ui-scripts/move-handles-check.json" --open "$WORK/design.mitcad" ||
  ui_fail 'OK without changes did not keep the matrix of the turned move'
ui_finish 'UI Move/Copy Free Move handles'
