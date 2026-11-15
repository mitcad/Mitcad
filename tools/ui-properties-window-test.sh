#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/commands/PropertiesReport.cpp app/MainWindowNoticeHover.cpp app/framework/NoticePlaces.cpp
# check-all sources: core/model-api/src/analysis.rs core/model-api/src/physical_extent.rs tools/cli app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/properties-window.mitcad-ui tools/ui-scripts/properties-window.json
# The Properties window (mitcad#1816) through the real UI, the shared input
# script tools/ui-scripts/properties-window.mitcad-ui (also run by
# tools/ui-windows-properties-window-test.ps1 and
# tools/ui-macos-properties-window-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/properties-window.json: a plate, two equal cubes
# and a box without a material): the summary and the merged rows of all
# four, a row's bodies painted under the pointer, a row's details, Copy's
# tab-separated text, and one body's groups with More collapsed.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-properties-window-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-properties-window.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/properties-window.mitcad-ui
"$cli" run "$here/ui-scripts/properties-window.json" --save "$work/properties-window.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/properties-window.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Properties window driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'summary, merged rows, hover, details, Copy and one body'
ui_finish 'UI Properties window'
