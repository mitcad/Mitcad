#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/PanelizeCommands.cpp app/commands/ModifyCommands.cpp
# check-all sources: core/model/src/features/panelize.rs core/model/src/document/panelize.rs tools/cli
# check-all sources: tools/ui-scripts/panelize.mitcad-ui tools/ui-scripts/panelize.json
# Panelize (mitcad#2005) through the real UI, the shared steps
# tools/ui-scripts/panelize.mitcad-ui (also run by
# tools/ui-windows-panelize-test.ps1 and tools/ui-macos-panelize-test.sh)
# on the design mitcad-cli makes (tools/ui-scripts/panelize.json): a
# box made a cabinet of 18 mm panels in three steps (the sides, the back,
# the top and bottom with Remove Remainder), each panel's volume checked.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panelize-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panelize.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/panelize.mitcad-ui
"$cli" run "$here/ui-scripts/panelize.json" --save "$work/panelize.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/panelize.mitcad"
for _ in $(seq 1 3000); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'panelize driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the cabinet made of five panels in three steps'
ui_finish 'UI panelize'
