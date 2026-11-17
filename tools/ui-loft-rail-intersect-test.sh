#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandSession.cpp app/framework/CommandPanel.cpp
# check-all sources: geometry/src/loft_skin.cpp geometry/src/loft.cpp core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/loft-rail-intersect.mitcad-ui tools/ui-scripts/loft-rail-intersect.json
# A loft from a circle to a square through a rail tied to Intersect points,
# and a loose rail's miss in words (mitcad#1944) through the real UI, the
# shared input script tools/ui-scripts/loft-rail-intersect.mitcad-ui (also
# run by tools/ui-windows-loft-rail-intersect-test.ps1 and
# tools/ui-macos-loft-rail-intersect-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/loft-rail-intersect.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-loft-rail-intersect-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-loft-rail-intersect.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/loft-rail-intersect.mitcad-ui
design=$work/loft-rail-intersect.mitcad
"$cli" run "$here/ui-scripts/loft-rail-intersect.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'loft rail intersect driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added loft (New Body) through 2 sections' 'the loft through the rail tied to Intersect points builds'
ui_expect_log 'rail 1 misses the second profile by 0.3 mm' 'the loose rail is refused in words'
ui_expect_log 'TestDriver: done' 'the miss message paints its places, without names or coordinates'
ui_finish 'UI loft rail intersect'
