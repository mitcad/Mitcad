#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch/SketchBlocks.cpp app/MainWindowSketchBlocks.cpp app/commands/SketchCommands.cpp
# check-all sources: app/browser app/framework/TestDriver.cpp app/sketch/SketchController.cpp
# check-all sources: app/resources/layout/default.json core/sketch core/solver core/model-api/src/sketch_blocks.rs
# check-all sources: core/model/src/document/sketch_blocks.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-blocks.mitcad-ui tools/ui-scripts/sketch-blocks.json
# Sketch blocks (mitcad#2110) through the real UI on a design that
# mitcad-cli makes (tools/ui-scripts/sketch-blocks.json: a slot): Create
# Block of the slot's outline, two instances inserted, one placed by a
# coincident and an angle, the block edited in place (the slot longer) and
# every instance following, one exploded. The steps are
# tools/ui-scripts/sketch-blocks.mitcad-ui, which
# tools/ui-windows-sketch-blocks-test.ps1 runs on Windows and
# tools/ui-macos-sketch-blocks-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-blocks-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-blocks.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-blocks.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-blocks.json" --save "$work/slot.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/slot.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch blocks driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Created block Slot of ' 'Create Block made the slot a block in place'
ui_expect_log 'Inserted block Slot into Sketch1 as instance k8 at (60, 30), turned 0°' 'two instances inserted'
ui_expect_log 'Block instance k7 of Slot: 30.000 mm along its direction' 'an instance followed the edited block'
ui_expect_log 'Block instance k8 of Slot: 30.000 mm along its direction' 'the placed instance followed the edited block'
ui_expect_log 'Exploded instance k7 of block Slot in Sketch1' 'Explode Block'
ui_expect_log 'TestDriver: done' 'sketch blocks made, inserted, edited and exploded'
ui_finish 'UI sketch blocks'
