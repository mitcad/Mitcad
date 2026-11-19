#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SheetProcessCommands.cpp app/commands/SheetCommands.cpp core/model/src/features/sheet.rs core/model/src/sheet app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/kerf-bend.mitcad-ui tools/ui-scripts/kerf-bend.json
# Kerf Bend (mitcad#1939) through the real UI: the shared input script
# tools/ui-scripts/kerf-bend.mitcad-ui (also run by
# tools/ui-windows-kerf-bend-test.ps1 and tools/ui-macos-kerf-bend-test.sh) on a
# design that mitcad-cli makes (tools/ui-scripts/kerf-bend.json). A kerf bend edited from the timeline opens in Kerf Bend with its slit lattice, its row pitch changed.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-kerf-bend-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-kerf-bend.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/kerf-bend.mitcad-ui
design=$work/kerf-bend.mitcad
"$cli" run "$here/ui-scripts/kerf-bend.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'kerf-bend driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Editing F5 with Kerf Bend' 'the kerf bend opened in its own panel'
ui_expect_log 'Edited F5' 'the kerf bend was edited'
ui_expect_log 'TestDriver: done' 'Kerf Bend (mitcad#1939): every step of the script'
ui_finish 'UI kerf-bend'
