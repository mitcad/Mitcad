#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model/src/features/finger_joint.rs core/model/src/features/t_nut_slot.rs app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/laser-sheet.mitcad-ui tools/ui-scripts/laser-sheet.json
# Laser sheet joints (mitcad#1939) through the real UI: the shared input script
# tools/ui-scripts/laser-sheet.mitcad-ui (also run by
# tools/ui-windows-laser-sheet-test.ps1 and tools/ui-macos-laser-sheet-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/laser-sheet.json). A finger joint of two plates at a corner selected in the browser, then a T-nut slot from a plate's end face with the screw's hole in its mate, with the volumes cli.sheet_laser_joints gives.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-laser-sheet-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-laser-sheet.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/laser-sheet.mitcad-ui
design=$work/laser-sheet.mitcad
"$cli" run "$here/ui-scripts/laser-sheet.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'laser-sheet driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added finger joint of 2 plates' 'the finger joint was added'
ui_expect_log 'Added T-nut slot with the screw.s hole' 'the T-nut slot was added'
ui_expect_log 'TestDriver: done' 'Laser sheet joints (mitcad#1939): every step of the script'
ui_finish 'UI laser-sheet'
