#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/sketch app/framework/TestDriver.cpp core/sketch core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/sketch-glyph-clear.mitcad-ui tools/ui-scripts/sketch-glyph-clear.json
# A constraint symbol keeps clear of the curves (mitcad#1594), the shared
# input script tools/ui-scripts/sketch-glyph-clear.mitcad-ui (also run by
# tools/ui-windows-sketch-glyph-clear-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/sketch-glyph-clear.json): two close parallel
# lines, a construction centre line and a line 2 mm above it (about 12 px
# after a fit), Parallel k12 between them. Both of k12's symbols are in
# the glyph log; a click on the upper line's middle selects the line, not
# the Parallel whose second symbol covered it; a click on that symbol
# selects the Parallel. The screenshot (sketch-glyph-clear.png) shows them.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-glyph-clear-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
ROOT=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
CLI=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$ROOT/build"
WORK=$(mktemp -d "$ROOT/build/ui-sketch-glyph-clear.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${WORK:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$WORK}
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-glyph-clear.mitcad-ui
"$CLI" run "$here/ui-scripts/sketch-glyph-clear.json" --save "$WORK/glyph-clear.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$WORK/glyph-clear.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch glyph clear driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Constraint k12 glyph at ' "k12's symbols are logged"
grep -qE '^Constraint k12 glyph at [0-9]+,[0-9]+ \(parallel by c6\)$' "$UI_LOG" ||
  ui_fail "k12's symbol beside c6 is not logged"
grep -qE '^Constraint k12 glyph at [0-9]+,[0-9]+ \(parallel by c3\)$' "$UI_LOG" ||
  ui_fail "k12's symbol beside c3 is not logged"
echo "ok   both of k12's symbols are logged"
ui_expect_log 'TestDriver: done' 'a click on the upper line selects it, the moved symbol the Parallel'
ui_finish 'UI sketch glyph clear of curves'
