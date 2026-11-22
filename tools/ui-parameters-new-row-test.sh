#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParametersDialog.hpp app/platform/MacParameters.hpp
# check-all sources: app/framework/ParameterUnits.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-new-row.mitcad-ui tools/ui-scripts/parameters-new-row.json
# The Parameters window's empty row, its rows' delete buttons and its
# bottom row (mitcad#2202, GitHub mitcad/Mitcad#512) through the real UI,
# the shared input script tools/ui-scripts/parameters-new-row.mitcad-ui
# (also run by the Windows and macOS wrappers) on a design that mitcad-cli
# makes (tools/ui-scripts/parameters-new-row.json):
#   - three parameters typed one after the other in the empty row (name,
#     Tab, value, Enter), the new empty row taking the keyboard each time;
#   - a taken name refused, kept in the row with the reason;
#   - an unused parameter deleted by its row's button, a used one's button
#     disabled;
#   - the empty row last under a sort, hidden under the search;
#   - a slider's value shown in its row and at its handle during the drag;
#   - the bottom row on both tabs, its pictures in
#     build/ui-parameters-new-row-shots (MITCAD_PARAMETERS_SHOTS).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-new-row-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameters-new-row.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_PARAMETERS_SHOTS=${MITCAD_PARAMETERS_SHOTS:-$root/build/ui-parameters-new-row-shots}
mkdir -p "$MITCAD_PARAMETERS_SHOTS"
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameters-new-row.mitcad-ui
"$cli" run "$here/ui-scripts/parameters-new-row.json" --save "$work/parameters-new-row.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/parameters-new-row.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the parameters new row driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail 'the parameters new row driver failed'; exit 1; }
ui_expect_log 'Created user parameter a3 = 13 mm (mm) in the new row' 'three parameters typed one after the other'
ui_expect_log 'New user parameter spare refused: ' 'a taken name is refused and kept'
ui_expect_log 'Deleted user parameter spare by its row button' "an unused parameter deleted by its row's button"
ui_expect_log 'Parameter slide slider held at ' "the slider's value shown before the release"
ui_expect_log 'TestDriver: done' 'the empty row, the delete buttons and the bottom row'
for tab in parameters tables; do
  [ -s "$MITCAD_PARAMETERS_SHOTS/parameters-bottom-row-$tab.png" ] ||
    ui_fail "no picture of the bottom row on the $tab tab"
done
ui_finish 'UI Parameters window: the empty row, the delete buttons and the bottom row'
