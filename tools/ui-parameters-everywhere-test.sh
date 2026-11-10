#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/view/NavigationBar.cpp app/view/NavigationBar.hpp app/MainWindowNavigationBar.cpp
# check-all sources: app/drawings/DrawingWorkspace.cpp app/drawings/DrawingWorkspace.hpp
# check-all sources: app/drawings/MainWindowDrawings.cpp app/MainWindowParameterEdits.cpp app/MainWindow.cpp
# check-all sources: app/browser/ParametersDialog.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/parameters-everywhere.mitcad-ui tools/ui-scripts/parameters-everywhere.json
# Edit Parameters one click away in every workspace (mitcad#1582, GitHub
# mitcad/Mitcad#222) through the real UI, the shared input script
# tools/ui-scripts/parameters-everywhere.mitcad-ui (also run by
# tools/ui-windows-parameters-everywhere-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/parameters-everywhere.json), in the
# Floating and then the Docked layout:
#   - the navigation bar's Edit Parameters opens the parameter table in the
#     SOLID, BUILD and MAKE workspaces and, as the sketch's entry, in a
#     sketch, which stays open; the pinned button has one action in the
#     sketch and after it (mitcad#1610);
#   - in a drawing, where the navigation bar is hidden, the drawing bar's
#     Edit Parameters opens it over the drawing, which stays open.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-parameters-everywhere-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-parameters-everywhere.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/parameters-everywhere.mitcad-ui
"$cli" run "$here/ui-scripts/parameters-everywhere.json" --save "$work/parameters.mitcad" > "$work/cli.log" 2>&1 ||
  { cat "$work/cli.log"; ui_fail "mitcad-cli: the design"; exit 1; }
ui_start_display

# run_layout floating|docked: the script in that layout, to its end.
run_layout() {
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/parameters.mitcad"
  for _ in $(seq 1 900); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the parameters driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the parameters driver failed"; exit 1; }
  ui_expect_log 'Navigation bar: Edit Parameters' "$1: the navigation bar's Edit Parameters clicked"
  ui_expect_log 'Parameters closed: back to Sketch1' "$1: the table opened from the bar in a sketch, the sketch kept"
  ui_expect_log 'Drawing bar: Edit Parameters' "$1: the drawing bar's Edit Parameters clicked"
  ui_expect_log 'TestDriver: done' "$1: Edit Parameters one click away in SOLID, BUILD, MAKE, a sketch and a drawing"
}

run_layout floating
run_layout docked
ui_finish "UI Edit Parameters in every workspace"
