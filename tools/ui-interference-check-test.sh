#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/InspectCommands.cpp app/framework/CommandSession.cpp app/framework/Command.hpp
# check-all sources: app/browser app/MainWindow.cpp core/model/src/document/analyses.rs core/model/src/file/v2.rs
# check-all sources: core/model-api/src/analysis.rs tools/cli tools/ui-scripts/interference-check.json
# check-all sources: tools/ui-scripts/interference-check.mitcad-ui tools/ui-scripts/interference-check-reopen.mitcad-ui
# Interference checks kept in the Analysis folder (mitcad#2664) through the
# real UI, the shared input scripts
# tools/ui-scripts/interference-check.mitcad-ui and
# interference-check-reopen.mitcad-ui (also run by
# tools/ui-windows-interference-check-test.ps1 and
# tools/ui-macos-interference-check-test.sh) on four blocks mitcad-cli makes
# of tools/ui-scripts/interference-check.json. Inspect > Interference of
# Body1 and Body2 (500 mm3) is kept as Interference1, a check of every body
# as Interference2; opened again from the browser they run on their bodies:
# none once Body2 is raised clear, Body4 checked by Interference2 once it is
# brought in, the bodies' volumes and the undo history left as they were.
# Saved (checked in the file here) and opened again, Interference1 runs on
# its saved bodies; with Body2 deleted its gone body is marked and said,
# removed, Body3 picked, and kept (checked in the file here). The pictures
# stay in the work folder (build/ui-interference-check, overwritten by the
# next run; one is copied to the path given).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-interference-check-test.sh [screenshot.png]
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
shot=${1:-}
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
work=$root/build/ui-interference-check
mkdir -p "$work"
trap 'ui_cleanup' EXIT
file=$work/interference-check.mitcad
"$cli" run "$here/ui-scripts/interference-check.json" --save "$file" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; echo "FAIL: mitcad-cli: the four blocks"; exit 1; }
export MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_WORK=$work
ui_start_display

# run_phase <script>: one of the shared scripts on the design.
run_phase() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --open "$file"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
  ui_expect_log 'TestDriver: done' "$1: the shared steps"
}

run_phase interference-check
ui_expect_log 'Inspect Interference: Body1 x Body2: 500' 'Body1 and Body2 share 500 mm3'
ui_expect_log 'Kept Interference as Interference1' 'the check of two bodies kept'
ui_expect_log 'Kept Interference as Interference2' 'the check of every body kept'
ui_expect_log 'Editing analysis Interference1 with Interference' 'opened again from the browser'
ui_expect_log 'Inspect Interference: none' 'none once Body2 is raised clear'
analyses=$(jq -c '[.analyses[] | {name, type, bodies, visible}]' "$file") || ui_fail "cannot read $file"
[ "$analyses" = '[{"name":"Interference1","type":"interference","bodies":["F2.b0","F4.b0"],"visible":false},{"name":"Interference2","type":"interference","bodies":null,"visible":false}]' ] ||
  ui_fail "the saved checks: $analyses"
echo "ok   both checks are in the saved file"

run_phase interference-check-reopen
ui_expect_log 'Panel Interference bodies: 2 selected, 1 missing' 'the gone body marked'
ui_expect_log 'Edited analysis Interference1' 'the check kept with its new bodies'
bodies=$(jq -c '[.analyses[] | select(.name == "Interference1") | .bodies]' "$file") || ui_fail "cannot read $file"
[ "$bodies" = '[["F2.b0","F6.b0"]]' ] || ui_fail "Interference1's saved bodies: $bodies"
echo "ok   Interference1 saved with Body1 and Body3"
for picture in interference-check.png interference-check-clear.png interference-check-gone.png; do
  [ -s "$work/$picture" ] || ui_fail "no picture $picture"
done
echo "ok   pictures: $work/interference-check.png, $work/interference-check-gone.png"
[ -n "$shot" ] && cp "$work/interference-check-gone.png" "$shot"
ui_finish 'UI interference check test'
