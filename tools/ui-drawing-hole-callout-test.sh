#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/drawings app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model-api/src/drawing_annotations.rs core/model-api/src/hole_callouts.rs
# check-all sources: core/model-api/src/drawings.rs core/model-drawing/src/annotate.rs core/model-drawing/src/compose.rs
# check-all sources: core/model/src/drawing/annotate_def.rs
# check-all sources: tools/ui-scripts/drawing-hole-callout.mitcad-ui tools/ui-scripts/drawing-hole-callout.json
# A drawing's own hole callout (mitcad#2054, GitHub #438) through the real
# UI, the shared input script tools/ui-scripts/drawing-hole-callout.mitcad-ui
# (also run by tools/ui-windows-drawing-hole-callout-test.ps1 and
# tools/ui-macos-drawing-hole-callout-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/drawing-hole-callout.json: a block with a
# counterbored hole through it, seen end on in a top view): Hole Callout
# on the hole writes its lines; Text After typed in its panel goes after
# them; the drill made larger, the callout follows.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-drawing-hole-callout-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-drawing-hole-callout.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/drawing-hole-callout.mitcad-ui
"$cli" run "$here/ui-scripts/drawing-hole-callout.json" --save "$work/holed.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/holed.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'hole callout driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log '"type":"hole_callout"' 'the hole callout asked for'
ui_expect_log 'Drawing panel hole callout: before , after , replace none' 'its panel with no own texts'
ui_expect_log 'TestDriver: done' 'the callout written, its own text after it, the hole followed'
ui_finish 'UI drawing hole callout'
