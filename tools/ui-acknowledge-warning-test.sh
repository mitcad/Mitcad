#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/acknowledged_warnings.rs
# check-all sources: core/model-api/src/lib.rs tools/cli tools/ui-scripts/acknowledge-warning.mitcad-ui
# check-all sources: tools/ui-scripts/timeline-auto-fix.json
# Acknowledged warnings (mitcad#1935, GitHub mitcad/Mitcad#380) through
# the real UI, the shared input script
# tools/ui-scripts/acknowledge-warning.mitcad-ui (also run by
# tools/ui-windows-acknowledge-warning-test.ps1 and
# tools/ui-macos-acknowledge-warning-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/timeline-auto-fix.json): an extrusion whose
# region a sketch change split warns; Acknowledge Warning takes it out of
# the warnings badge and its cell is no longer yellow, Show Warning Again
# brings both back, and the badge's menu acknowledges it too.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-acknowledge-warning-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-acknowledge-warning.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/acknowledge-warning.mitcad-ui
"$cli" run "$here/ui-scripts/timeline-auto-fix.json" --save "$work/warned.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/warned.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'acknowledged warning driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Acknowledged warning of Extrude1' 'Acknowledge Warning in the timeline menu'
ui_expect_log 'Showing warning again of Extrude1' 'Show Warning Again in the timeline menu'
# The cell's painted state (yellow, then not, then yellow again).
ui_expect_log 'Timeline cell Extrude1: warning, ' 'the warned cell is yellow'
ui_expect_log 'Timeline cell Extrude1: ok, acknowledged' 'the acknowledged cell is not yellow'
ui_expect_log 'TestDriver: done' 'an acknowledged warning leaves the badge and the yellow cell, and comes back'
ui_finish 'UI acknowledged warnings'
