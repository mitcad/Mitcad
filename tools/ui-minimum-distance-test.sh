#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/MinimumDistance.cpp app/commands/MinimumDistance.hpp app/commands/InspectCommands.cpp
# check-all sources: app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp app/browser core/model/src/document/analyses.rs
# check-all sources: core/model-api/src/min_distance.rs core/model-api/src/analysis.rs
# check-all sources: tools/ui-scripts/minimum-distance.mitcad-ui tools/ui-scripts/minimum-distance.json tools/cli
# Minimum Distance (mitcad#2519) through the real UI, the shared input
# script tools/ui-scripts/minimum-distance.mitcad-ui (also run by
# tools/ui-windows-minimum-distance-test.ps1 and
# tools/ui-macos-minimum-distance-test.sh) on the two blocks mitcad-cli makes
# of tools/ui-scripts/minimum-distance.json. Inspect > Minimum Distance
# between Body1 and Body2 picked in the browser gives 5 mm, marked closer
# than a 6 mm clearance and clear of a 4 mm one; OK keeps it as
# MinDistance1, which is measured again when the blocks' parameter changes
# (3 mm, then touching), hidden, shown, edited, saved (checked in the file
# here), deleted and undone. The view's pictures of the line stay in the
# work folder (build/ui-minimum-distance, overwritten by the next run; a
# picture is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-minimum-distance-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-minimum-distance
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/distance-blocks.mitcad
"$cli" run "$here/ui-scripts/minimum-distance.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the two blocks"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/minimum-distance.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "minimum distance driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Minimum Distance: 5.000 mm from Body1' 'Body1 to Body2: 5 mm'
ui_expect_log 'clearance 6 mm: closer' 'closer than a 6 mm clearance'
ui_expect_log 'Kept Minimum Distance as MinDistance1' 'kept in the document'
ui_expect_log 'Minimum distance MinDistance1: 3.000 mm' 'measured again after the parameter changed'
ui_expect_log 'Deleted analysis MinDistance1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Minimum Distance through the UI'
grep -q '"type": "minimum_distance"' "$file" || ui_fail "no minimum distance analysis in the saved file"
grep -q '"name": "MinDistance1"' "$file" || ui_fail "no MinDistance1 in the saved file"
echo "ok   the analysis is in the saved file"
for picture in minimum-distance.png minimum-distance-clearance.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures of the line: $work/minimum-distance.png, $work/minimum-distance-clearance.png"
[ -n "$shot" ] && cp "$work/minimum-distance-clearance.png" "$shot"
ui_finish 'UI minimum distance test'
