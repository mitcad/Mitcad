#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SurfaceCommands.cpp app/commands/SweepCommands.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandPanel.cpp geometry/src/loft.cpp geometry/src/loft_skin.cpp
# check-all sources: core/model/src/features/loft.rs tools/cli
# check-all sources: tools/ui-scripts/surface-loft-perpendicular.mitcad-ui tools/ui-scripts/surface-loft-perpendicular.json
# Surface Loft's Start and End Condition (mitcad#2345) through the real UI:
# Perpendicular to Face at both ends, the start's Face picked (a post's
# wall, the section its top face), the end's weight 2; the surface builds
# as one side face. The shared input script
# tools/ui-scripts/surface-loft-perpendicular.mitcad-ui (also run by
# tools/ui-windows-surface-loft-perpendicular-test.ps1 and
# tools/ui-macos-surface-loft-perpendicular-test.sh) on a design that
# mitcad-cli makes (tools/ui-scripts/surface-loft-perpendicular.json).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-surface-loft-perpendicular-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-surface-loft-perpendicular.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/surface-loft-perpendicular.mitcad-ui
design=$work/surface-loft-perpendicular.mitcad
"$cli" run "$here/ui-scripts/surface-loft-perpendicular.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'surface loft perpendicular driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added surface loft through 2 sections' 'the surface loft builds with Perpendicular to Face at both ends'
ui_expect_log 'TestDriver: done' 'the surface is one side face'
ui_finish 'UI surface loft perpendicular'
