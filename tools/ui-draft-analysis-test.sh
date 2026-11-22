#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/DraftAnalysis.cpp app/commands/DraftAnalysis.hpp app/commands/InspectCommands.cpp
# check-all sources: app/commands/PrintAnalysis.cpp app/commands/PrintAnalysis.hpp app/MainWindowPrintAnalysis.cpp
# check-all sources: app/view/PrintOverlay.cpp app/view/PrintOverlay.hpp app/browser app/framework/Theme.cpp
# check-all sources: core/model/src/document/analyses.rs core/model/src/print_analysis.rs core/model-api/src/print.rs
# check-all sources: tools/ui-scripts/draft-analysis.mitcad-ui tools/ui-scripts/draft-analysis.json tools/cli
# Draft Analysis (mitcad#2166, GitHub mitcad/Mitcad#535) through the real UI,
# the shared input script tools/ui-scripts/draft-analysis.mitcad-ui (also run
# by tools/ui-windows-draft-analysis-test.ps1 and
# tools/ui-macos-draft-analysis-test.sh) on the drafted part mitcad-cli makes
# of tools/ui-scripts/draft-analysis.json. Inspect > Draft Analysis gives the
# three classes' areas pulled along +Z, then at 3 degrees, flipped and
# pulled along Plane1 picked in the browser; OK keeps it as Draft1, which is
# hidden, shown, edited, saved (checked in the file here), deleted and
# undone. The view's pictures of the three colours stay in the work folder
# (build/ui-draft-analysis, overwritten by the next run; a picture is
# copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-draft-analysis-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-draft-analysis
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/draft-part.mitcad
"$cli" run "$here/ui-scripts/draft-analysis.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the drafted part"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/draft-analysis.mitcad-ui
export MITCAD_TEST_WORK=$work
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$file"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "draft analysis driver timed out"; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
UI_RUNNER=""
ui_expect_log 'Inspect Draft Analysis: pull (0, 0, 1), angle 1: Body1 along 1249.4 mm^2' 'pulled along +Z at 1 degree'
ui_expect_log 'Kept Draft Analysis as Draft1' 'kept in the document'
ui_expect_log 'Deleted analysis Draft1' 'deleted from its context menu'
ui_expect_log 'TestDriver: done' 'Draft Analysis through the UI'
grep -q '"type": "draft"' "$file" || ui_fail "no draft analysis in the saved file"
grep -q '"name": "Draft1"' "$file" || ui_fail "no Draft1 in the saved file"
echo "ok   the analysis is in the saved file"
for picture in draft-analysis-z.png draft-analysis-plane.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures of the three colours: $work/draft-analysis-z.png, $work/draft-analysis-plane.png"
[ -n "$shot" ] && cp "$work/draft-analysis-plane.png" "$shot"
ui_finish 'UI draft analysis test'
