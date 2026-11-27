#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MinRadiusAnalysis.cpp app/commands/MinRadiusAnalysis.hpp app/commands/InspectCommands.cpp
# check-all sources: app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp app/view/PrintOverlay.hpp app/browser
# check-all sources: core/model/src/document/analyses.rs core/model-api/src/print.rs core/cpp/bridge/min_radius.cpp
# check-all sources: tools/ui-scripts/min-radius.mitcad-ui tools/ui-scripts/min-radius.json tools/cli
# Minimum Radius (mitcad#2518) through the real UI, the shared input script
# tools/ui-scripts/min-radius.mitcad-ui (also run by
# tools/ui-windows-min-radius-test.ps1 and tools/ui-macos-min-radius-test.sh)
# on the rounded block mitcad-cli makes of tools/ui-scripts/min-radius.json.
# Inspect > Minimum Radius marks the smallest radius, 2 mm; a Radius Limit of
# 11 mm takes all three roundings, 4 mm the 2 mm one alone, drawn over the
# body; OK keeps it as MinRadius1, which is hidden, shown, edited, saved
# (checked in the file here), deleted and undone. The view's pictures stay in
# the work folder (build/ui-min-radius, overwritten by the next run; a
# picture is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-min-radius-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-min-radius
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/min-radius-part.mitcad
"$cli" run "$here/ui-scripts/min-radius.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the rounded block"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/min-radius.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "minimum radius driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Minimum Radius: smallest 2.000 mm on Body1' 'the smallest radius, 2 mm'
ui_expect_log 'under 4 mm: 1 face(s), 62.8 mm^2' 'a limit of 4 mm takes the 2 mm rounding alone'
ui_expect_log 'Kept Minimum Radius as MinRadius1' 'kept in the document'
ui_expect_log 'Deleted analysis MinRadius1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Minimum Radius through the UI'
grep -q '"type": "min_radius"' "$file" || ui_fail "no minimum radius analysis in the saved file"
grep -q '"name": "MinRadius1"' "$file" || ui_fail "no MinRadius1 in the saved file"
echo "ok   the analysis is in the saved file"
for picture in min-radius-marker.png min-radius-limit.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures: $work/min-radius-marker.png, $work/min-radius-limit.png"
[ -n "$shot" ] && cp "$work/min-radius-limit.png" "$shot"
ui_finish 'UI minimum radius test'
