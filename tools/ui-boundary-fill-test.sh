#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/BoundaryFillCommands.cpp app/commands/CreateCommands.cpp
# check-all sources: core/model/src/features/boundary_fill.rs geometry/src/boundary_fill.cpp core/model-api/src/boundary_regions.rs
# check-all sources: tools/ui-scripts/boundary-fill.mitcad-ui tools/ui-scripts/boundary-fill.json
# Boundary Fill (mitcad#2149) through the real UI, the shared steps
# tools/ui-scripts/boundary-fill.mitcad-ui (also run by
# tools/ui-windows-boundary-fill-test.ps1 and tools/ui-macos-boundary-fill-test.sh)
# on the design mitcad-cli makes (tools/ui-scripts/boundary-fill.json):
# four surfaces and two planes picked, their one closed region taken at
# once and filled as a new body; a clicked cell cut from a block; Undo,
# and the first fill edited with its cell.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-boundary-fill-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-boundary-fill.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/boundary-fill.mitcad-ui
"$cli" run "$here/ui-scripts/boundary-fill.json" --save "$work/boundary-fill.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/boundary-fill.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'boundary fill driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the closed region filled, a clicked cell cut, the fill edited with its cell'
ui_finish 'UI boundary fill'
