#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser app/framework/TestDriver.cpp core/model/src/document/auto_fix.rs
# check-all sources: core/model-api/src/auto_fix.rs tools/cli tools/ui-scripts/timeline-auto-fix.mitcad-ui
# Auto Fix in the timeline's context menu (mitcad#1223) through the real
# UI, the shared input script tools/ui-scripts/timeline-auto-fix.mitcad-ui
# (also run by tools/ui-windows-timeline-auto-fix-test.ps1) on a design
# that mitcad-cli makes (tools/ui-scripts/timeline-auto-fix.json): an
# extrusion whose region a sketch change split warns; Auto Fix in its
# timeline menu fixes it as one undo step with the same volume, is no
# longer offered, and comes back with the warning after Undo. The cell's
# and the warnings badge's tooltips name Auto Fix, and the badge's menu
# offers it too (mitcad#1677).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-timeline-auto-fix-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-timeline-auto-fix.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/timeline-auto-fix.mitcad-ui
"$cli" run "$here/ui-scripts/timeline-auto-fix.json" --save "$work/auto-fix.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/auto-fix.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'timeline Auto Fix driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Auto Fix from the timeline menu: the warning goes, one undo step'
ui_finish 'UI Auto Fix in the timeline menu'
