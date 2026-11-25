#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SweepCommands.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp
# check-all sources: core/model/src/features/rib.rs geometry/src/rib.cpp tools/cli
# check-all sources: tools/ui-scripts/web-circle.mitcad-ui tools/ui-scripts/web-circle.json
# A Web from a circle (mitcad#2364) through the real UI, the shared input
# script tools/ui-scripts/web-circle.mitcad-ui (also run by
# tools/ui-windows-web-circle-test.ps1 and tools/ui-macos-web-circle-test.sh)
# on a plate with a circle on its top that mitcad-cli makes
# (tools/ui-scripts/web-circle.json): Web on the circle by a depth makes a
# ring wall joined to the plate, one body of the analytic volume; Undo
# takes it away.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-web-circle-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-web-circle.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/web-circle.mitcad-ui
design=$work/web-circle.mitcad
"$cli" run "$here/ui-scripts/web-circle.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'web circle driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Added web on 1 curve(s), thickness 2' 'the web is added'
ui_expect_log 'TestDriver: done' 'a web from a circle is a ring wall joined to the plate'
ui_finish 'UI web circle'
