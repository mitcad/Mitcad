#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/build app/commands/BuildCommands.cpp app/commands/DerivedPartCommands.cpp app/framework/TestDriver.cpp app/library
# check-all sources: core/model/src/features/edge_frame.rs core/model-build core/model-api/src tools/cli
# check-all sources: tools/ui-timber-lib.sh tools/ui-scripts/timber-edge.mitcad-ui tools/ui-scripts/timber-edge.json
# Timber's Along an edge (mitcad#1814) through the real panel: the shared
# input script tools/ui-scripts/timber-edge.mitcad-ui (also run by
# tools/ui-windows-timber-edge-test.ps1 and tools/ui-macos-timber-edge-test.sh)
# on a plate that mitcad-cli makes (tools/ui-scripts/timber-edge.json), with
# the timber library of tools/ui-timber-lib.sh (a 98 x 48 member): a member
# along the plate's long edge, flush and then moved by Inset, Start, End and
# Lift; the plate made shorter, the member follows with its offsets kept; a
# round edge is refused in words.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timber-edge-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$here/ui-test-lib.sh"
source "$here/ui-timber-lib.sh"

timber_library ui-timber-edge
export MITCAD_TEST_WORK=$WORK MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/timber-edge.mitcad-ui
"$CLI" run "$here/ui-scripts/timber-edge.json" --save "$WORK/timber-edge.mitcad" > /dev/null ||
  ui_fail "mitcad-cli run timber-edge.json failed"
ui_start_display
ui_start_app --ui=qt --no-mcp --no-native-dialogs --open "$WORK/timber-edge.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Along an edge driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '0.00, 49.00, 33.00; inset 0 mm, start 0 mm, end 0 mm, lift 0 mm;' \
  'the member along the long edge, 1800 long, flush on the top face'
ui_expect_log 'length along an edge, 1730.00 mm; end A at 50.00, 59.00, 38.00' \
  'Inset 10, Start 50, End 20, Lift 5: placed where they say'
ui_expect_log 'TestDriver: done' 'the plate 1500 long: the member 1430 long, its offsets kept; a round edge refused'
ui_finish 'UI Timber along an edge'
