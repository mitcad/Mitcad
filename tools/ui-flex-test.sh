#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FlexCommands.cpp app/commands/FlexRoute.cpp app/commands/FlexRoute.hpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionHover.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandPanel.cpp app/framework/TestDriver.cpp app/browser/DocumentSnapshot.cpp
# check-all sources: core/model-api/src/flex.rs core/model/src/features/flex.rs core/model/src/document/flex.rs
# check-all sources: core/model-base/src/flex.rs geometry/src/flex.cpp
# check-all sources: tools/ui-scripts/flex.mitcad-ui tools/ui-scripts/flex.json tools/cli
# Flex laid by clicks (mitcad#1713) through the real UI, the shared input
# script tools/ui-scripts/flex.mitcad-ui (also run by
# tools/ui-windows-flex-test.ps1) on a design that mitcad-cli makes
# (tools/ui-scripts/flex.json: an upright pipe beside a block), in the
# Docked and in the Floating layout: a hose from the pipe's end port over
# the block's top and side with the routed length in the panel, free nodes
# off the model, Backspace, a double click as the end, a flat section's
# twist, Undo and Esc.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-flex-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-flex.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/flex.mitcad-ui
ui_start_display
for chrome in docked floating; do
  echo "--- Flex in the $chrome layout"
  "$cli" run "$here/ui-scripts/flex.json" --save "$work/flex-$chrome.mitcad" || exit 1
  ui_start_app --ui=qt --no-mcp "--chrome=$chrome" --open "$work/flex-$chrome.mitcad"
  for _ in $(seq 1 2100); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "the flex driver timed out ($chrome)"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  ui_expect_log 'Flex: node 1 at an end port' "the first click starts at the pipe's end port ($chrome)"
  ui_expect_log 'Panel Flex note: Length: ' "the panel shows the routed length ($chrome)"
  ui_expect_log 'Added Flex as F6' "Enter added the hose over the block ($chrome)"
  ui_expect_log 'Flex: last node taken back, 1 left' "Backspace took the last node back ($chrome)"
  ui_expect_log 'Flex: a click on the last node ends the item at node 3' "a double click ended the item ($chrome)"
  ui_expect_log 'Undo: Add Flex3' "the flat item with its first node's twist added and undone ($chrome)"
  ui_expect_log 'Command Flex cancelled' "Esc cancelled ($chrome)"
  ui_expect_log 'Flex route under the pointer: 2 node(s)' "the pointer moved shows the route alone ($chrome)"
  ui_expect_log 'Flex: node 1 chosen for its twist' "a click on the first node made Twist that node's ($chrome)"
  ui_expect_log 'Flex: node 2 at an end port' "the second pipe's end port ended the item ($chrome)"
  ui_expect_log 'Edited F6' "Flex1 edited from the timeline ($chrome)"
  ui_expect_log 'Flex: saved type user/' "the panel's item saved as a type ($chrome)"
  ui_expect_log 'TestDriver: done' "the script ran to its end ($chrome)"
  ui_stop_app
done
ui_finish 'UI Flex'
