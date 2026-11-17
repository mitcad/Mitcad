#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/SelectionScope.cpp app/framework/SelectionScope.hpp app/framework/SelectionFilterPanel.cpp
# check-all sources: app/MainWindowSelection.cpp app/sketch app/framework/TestDriver.cpp core/sketch tools/cli
# check-all sources: tools/ui-scripts/sketch-filter-annotations.mitcad-ui tools/ui-scripts/sketch-filter-annotations.json
# The sketch Selection Filter's Dimensions and Constraints rows (GitHub
# #388), through the real UI on a design that mitcad-cli makes
# (tools/ui-scripts/sketch-filter-annotations.json: two parallel lines, a
# Fix symbol, Horizontal, Parallel and a dimension between them). With only
# Dimensions a cross a window takes only the dimension; with only
# Constraints a cross it takes only the constraints, the Fix symbol among
# them; with Dimensions empty a click on the dimension does not select it
# and a click on the Parallel still does. The steps are
# tools/ui-scripts/sketch-filter-annotations.mitcad-ui, which
# tools/ui-windows-sketch-filter-annotations-test.ps1 runs on Windows and
# tools/ui-macos-sketch-filter-annotations-test.sh on macOS.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-sketch-filter-annotations-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-sketch-filter-annotations.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=${MITCAD_UI_LOG_DIR:-$work} MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/sketch-filter-annotations.mitcad-ui
"$cli" run "$here/ui-scripts/sketch-filter-annotations.json" --save "$work/annotations.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/annotations.mitcad"
for _ in $(seq 1 600); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'sketch filter annotations driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Selection filter sketch dimensions cross: ' 'only Dimensions a cross'
ui_expect_log 'Selection filter sketch constraints cross: ' 'only Constraints a cross'
ui_expect_log 'Selection filter sketch dimensions empty: ' 'Dimensions empty'
ui_expect_log 'TestDriver: done' 'a window and a click each followed their own row'
ui_finish 'UI sketch filter: Dimensions and Constraints rows'
