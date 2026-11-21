#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawings.rs core/model-api/src/annotations.rs core/model-drawing/src
# check-all sources: core/model/src/drawing
# check-all sources: tools/ui-scripts/drawing-standard.mitcad-ui tools/ui-scripts/drawing-standard.json
# A drawing's dimensioning standard (mitcad#2047) through the real UI, the
# shared input script tools/ui-scripts/drawing-standard.mitcad-ui (also run
# by tools/ui-windows-drawing-standard-test.ps1 and
# tools/ui-macos-drawing-standard-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/drawing-standard.json: an ISO drawing with a
# vertical dimension): Standard ASME in the sheet's panel writes the value
# horizontal and one Undo puts ISO back; Create Drawing's ANSI template
# sets Standard to ASME, and the choice is kept for the next time.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-standard-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-standard.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-standard.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-standard.json" --save "$work/standard.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/standard.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'drawing standard driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Drawing dimension texts (asme): ' 'Standard ASME wrote the vertical value horizontal'
ui_expect_log 'Create Drawing choice kept: template ansi, ANSI_B, landscape, third_angle, standard asme' \
  'the ANSI template brought ASME and the choice was kept'
ui_expect_log 'TestDriver: done' 'the standard relaid the dimensions out, undid in one step, and Create Drawing kept it'
ui_finish 'UI drawing dimensioning standard'
