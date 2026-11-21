#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PatternCommands.cpp app/commands/CommandSupport.cpp app/framework/CommandSessionHover.cpp
# check-all sources: core/model/src/features/pattern.rs core/model/src/features/path.rs tools/cli
# check-all sources: tools/ui-scripts/path-pattern-edges.mitcad-ui tools/ui-scripts/path-pattern-edges.json
# check-all sources: tools/ui-scripts/path-pattern-edges-check.json
# Pattern on Path along a body's edges with Tangent Chain (mitcad#2105,
# GitHub mitcad/Mitcad#322) through the real UI, the shared input script
# (also run by tools/ui-windows-path-pattern-edges-test.ps1 and
# tools/ui-macos-path-pattern-edges-test.sh):
#   tools/ui-scripts/path-pattern-edges.mitcad-ui on the block that
#   mitcad-cli makes (tools/ui-scripts/path-pattern-edges.json): one top
#   edge picked as the path, its hover showing the tangent chain of five
#   edges, Tangent Chain on; the pattern made, edited, saved. mitcad-cli
#   then checks that the copies follow the rounded outline
#   (tools/ui-scripts/path-pattern-edges-check.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-path-pattern-edges-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-path-pattern-edges.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
design=$work/path-pattern-edges.mitcad
"$cli" run "$here/ui-scripts/path-pattern-edges.json" --save "$design" || exit 1
ui_start_display

export MITCAD_TEST_INPUT=$here/ui-scripts/path-pattern-edges.mitcad-ui
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail 'the driver failed or the application ended'; exit 1; }
UI_RUNNER=""
ui_expect_log 'Hover chain: 5 edge(s) of F2.b0' "the edge's hover shows its tangent chain"
ui_expect_log 'Added Pattern on Path' 'the pattern made along the edge'
ui_expect_log 'Editing F6 with Pattern on Path' 'the pattern along edges edited'
ui_expect_log 'TestDriver: done' 'Pattern on Path along a body edge with Tangent Chain'
"$cli" run "$here/ui-scripts/path-pattern-edges-check.json" --open "$design" > "$work/check.log" 2>&1 ||
  { cat "$work/check.log"; ui_fail 'the copies do not follow the outline'; exit 1; }
echo "ok   the copies follow the rounded outline"
ui_finish 'UI Pattern on Path along a body edge with Tangent Chain'
