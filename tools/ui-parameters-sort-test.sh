#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/browser/ParametersDialog.cpp app/browser/ParametersDialog.hpp app/framework/ParameterUnits.cpp
# check-all sources: app/browser/ParameterSort.cpp app/browser/ParameterSort.hpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-sort.mitcad-ui tools/ui-scripts/parameters-sort.json
# Edit Parameters sorted by a column (mitcad#1866, GitHub mitcad/Mitcad#338)
# through the real UI, the shared input script
# tools/ui-scripts/parameters-sort.mitcad-ui (also run by the Windows and
# macOS wrappers) on a design that mitcad-cli makes
# (tools/ui-scripts/parameters-sort.json): user parameters whose names,
# values and units differ, a favorite, and two features' model parameters.
#   - Name's header sorts the user parameters in natural order (len2
#     before len10), again the reverse, a third time the creation order;
#     the groups stay where they are.
#   - Value's header sorts by the numbers in mm (2 cm, 1 in, 30 mm, then
#     the number without a unit), again the reverse; the choice is kept in
#     the settings and when the dialog opens again.
#   - An edited row and an added one move to their place in the sort and
#     stay selected; the search filter works on the sorted table.
#   - The Unit column shows Number for a parameter without a unit, and Add
#     User Parameter's Number adds one (mitcad#1920).
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-sort-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameters-sort.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameters-sort.mitcad-ui
"$cli" run "$here/ui-scripts/parameters-sort.json" --save "$work/parameters-sort.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/parameters-sort.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'the parameters sort driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail 'the parameters sort driver failed'; exit 1; }
ui_expect_log 'Parameters sorted by name ascending' "Name's header sorts the rows"
ui_expect_log 'Parameters sorted by value descending' "Value's header twice sorts them in the reverse order"
ui_expect_log 'Parameters sorted in creation order' 'a third click goes back to the creation order'
ui_expect_log 'TestDriver: done' 'the rows sort within their groups by a column'
ui_finish 'UI Edit Parameters sorted by a column'
