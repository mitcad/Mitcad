#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: geometry/src/loft_merge.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/loft-tangent-edges.mitcad-ui tools/ui-scripts/loft-tangent-edges.json
# The Loft panel's Tangent Edges (mitcad#2162) through the real UI: the
# panel shows it with Merge chosen, and a circle lofted to a slot has one
# side face. The shared input script
# tools/ui-scripts/loft-tangent-edges.mitcad-ui (also run by
# tools/ui-windows-loft-tangent-edges-test.ps1 and
# tools/ui-macos-loft-tangent-edges-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/loft-tangent-edges.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-loft-tangent-edges-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-loft-tangent-edges.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/loft-tangent-edges.mitcad-ui
design=$work/loft-tangent-edges.mitcad
"$cli" run "$here/ui-scripts/loft-tangent-edges.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'loft tangent edges driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added loft (New Body) through 2 sections' 'the circle-to-slot loft builds with Tangent Edges Merge'
ui_expect_log 'TestDriver: done' 'the loft has one side face and its caps'
ui_finish 'UI loft tangent edges'
