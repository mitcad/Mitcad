#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp app/framework/TestDriverSketch.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/drawing_flat.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src/compose.rs
# check-all sources: core/model/src/drawing/def.rs core/model/src/drawing/annotate_def.rs
# check-all sources: tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui tools/ui-scripts/drawing-flat-pattern-view.json
# A base view of a sheet-metal body's Flat Pattern (mitcad#2280) through
# the real UI, the shared input script
# tools/ui-scripts/drawing-flat-pattern-view.mitcad-ui (also run by
# tools/ui-windows-drawing-flat-pattern-view-test.ps1 and
# tools/ui-macos-drawing-flat-pattern-view-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/drawing-flat-pattern-view.json: an L
# bent once and an empty drawing): Base View with Shows set to the body's
# Flat Pattern places its blank; Dimension on its bend line reads 20; the
# view's panel has Bend Notes, which it turns off.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-flat-pattern-view-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-flat-pattern-view.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-flat-pattern-view.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-flat-pattern-view.json" --save "$work/plate.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/plate.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'flat pattern view driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '"type":"flat_pattern"' 'the Flat Pattern view placed from the Shows list'
ui_expect_log 'Drawing annotations drawn: 1 = 20' 'the bend line dimensioned'
ui_expect_log '"bend_notes":false' 'the bend notes turned off'
ui_expect_log 'TestDriver: done' 'placed, dimensioned, its bend notes turned off'
ui_finish 'UI drawing Flat Pattern view'
