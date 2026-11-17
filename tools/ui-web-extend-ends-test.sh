#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/web-extend-ends.mitcad-ui tools/ui-scripts/web-extend-ends.json
# A Web's Extend Ends and mitred corner (mitcad#1968) through the real UI,
# the shared input script tools/ui-scripts/web-extend-ends.mitcad-ui (also
# run by tools/ui-windows-web-extend-ends-test.ps1 and
# tools/ui-macos-web-extend-ends-test.sh) on a plate with four walls that
# mitcad-cli makes (tools/ui-scripts/web-extend-ends.json): Web on an L of
# two lines short of the walls shows Extend Ends checked and End Offset;
# OK makes walls that reach the walls, mitred at the corner, one body of
# the analytic volume; Undo takes them away.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-web-extend-ends-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-web-extend-ends.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/web-extend-ends.mitcad-ui
design=$work/web-extend-ends.mitcad
"$cli" run "$here/ui-scripts/web-extend-ends.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'web extend ends driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added web on 2 curve(s), thickness 2' 'the web is added'
ui_expect_log 'TestDriver: done' 'Extend Ends is on for a new web; its walls reach the walls, mitred'
ui_finish 'UI web extend ends'
