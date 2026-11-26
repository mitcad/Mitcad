#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/browser/BrowserController.cpp app/MainWindowSheet.cpp
# check-all sources: app/framework/CommandRegistry.cpp app/resources/layout/default.json
# check-all sources: app/framework/TestDriver.cpp tools/cli core/model-api/src/drawing_flat.rs
# check-all sources: tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui tools/ui-scripts/drawing-flat-pattern-commands.json
# The Flat Pattern's drawing commands (mitcad#2446) through the real UI, the
# shared input script tools/ui-scripts/drawing-flat-pattern-commands.mitcad-ui
# (also run by tools/ui-windows-drawing-flat-pattern-commands-test.ps1 and
# tools/ui-macos-drawing-flat-pattern-commands-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-flat-pattern-commands.json: an L
# bent once with its Flat Pattern, and an empty drawing): Base View's Shows
# has the Flat Pattern after a separator; Flat Pattern View from the DRAWING
# tab's Create menu places the blank with its bend line; Create Drawing of
# Flat Pattern… in the Flat Pattern row's menu makes a drawing with the blank
# placed on it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-flat-pattern-commands-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-flat-pattern-commands.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-flat-pattern-commands.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-flat-pattern-commands.json" --save "$work/plate.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/plate.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'flat pattern commands driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing tool: Flat Pattern View' 'Flat Pattern View from the Create menu'
ui_expect_log 'Drawing Flat Pattern placed: view 1 of F3.b0' 'Create Drawing of Flat Pattern from the Flat Pattern row'
ui_expect_log 'Drawing Flat Pattern views: view 1: Plate, 50.65 x 20.00, 1 bend line' 'the blank drawn with its bend line'
ui_expect_log 'TestDriver: done' 'both commands placed the Flat Pattern'
ui_finish 'UI drawing Flat Pattern commands'
