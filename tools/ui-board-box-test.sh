#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp core/model-api/src/sheet_process.rs core/model-api/src/sheet_process core/model/src/sheet app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/board-box.mitcad-ui tools/ui-scripts/board-box.json
# Standard boxes of board (mitcad#1939) through the real UI: the shared input script
# tools/ui-scripts/board-box.mitcad-ui (also run by
# tools/ui-windows-board-box-test.ps1 and tools/ui-macos-board-box-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/board-box.json). A FEFCO 0201 box of corrugated board made by Standard Box, folded with its glue flap touching the first panel, Finger Joint greyed for the board, the box taken back by one undo; then a FEFCO 0203 box and a FEFCO 0427 folder chosen from the Style list and built (mitcad#2226); then a FEFCO 0421 folder with its Tuck and Tabs set, kept in its rules, its tuck changed in Edit Standard Box (mitcad#2269), then its number of tabs, the blank made again and the box following, and another style refused by the model in words in the panel's own message area (mitcad#2285).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-board-box-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-board-box.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/board-box.mitcad-ui
design=$work/board-box.mitcad
"$cli" run "$here/ui-scripts/board-box.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'board-box driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Standard box fefco_0201, ' 'the standard box was made'
ui_expect_log 'Standard box fefco_0203, ' 'a FEFCO 0203 box was made from the Style list (mitcad#2226)'
ui_expect_log 'Standard box fefco_0427, ' 'a FEFCO 0427 folder was made from the Style list (mitcad#2226)'
ui_expect_log ', tuck 25 mm, tabs 3' 'a FEFCO 0421 folder was made with Tuck 25 mm and Tabs 3 (mitcad#2269)'
ui_expect_log ', tuck 30 mm, tabs 3' 'Edit Standard Box changed the folder tuck to 30 mm (mitcad#2269)'
ui_expect_log ', tuck 30 mm, tabs 4' 'Edit Standard Box made the folder blank again with four tabs, the box following (mitcad#2285)'
ui_expect_log 'Panel Standard Box error: another style changes the shape of the box' 'another style was refused in words in the panel (mitcad#2285)'
ui_expect_log 'Standard Box: OK refused by the model: ' 'the model refusal kept the panel open (mitcad#2285)'
ui_expect_log 'TestDriver: done' 'Standard boxes of board (mitcad#1939): every step of the script'
ui_finish 'UI board-box'
