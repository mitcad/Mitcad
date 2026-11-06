#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindow.cpp app/MainWindowStart.cpp app/files/StartPage.cpp app/browser
# check-all sources: app/framework/Ribbon.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/start-page-delete.json tools/ui-scripts/start-page-delete.mitcad-ui
# Delete on the Start page (mitcad#1313) through the real UI, the shared
# input script tools/ui-scripts/start-page-delete.mitcad-ui (also run by
# tools/ui-windows-start-page-delete-test.ps1) on a block that mitcad-cli
# makes (tools/ui-scripts/start-page-delete.json), in the Floating and the
# Docked layout: Sketch1 selected, the Start tab clears the selection, and
# Delete and Backspace on the page and on its New Design card delete
# nothing of the design behind it.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-start-page-delete-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-start-page-delete.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/start-page-delete.mitcad-ui
"$cli" run "$here/ui-scripts/start-page-delete.json" --save "$work/block.mitcad" || exit 1
ui_start_display

# run_layout <name> [app arguments...]: one start of MitCad with the shared
# steps; the app ends with them.
run_layout() {
  local name=$1
  shift
  ui_start_app --ui=qt --no-mcp --open "$work/block.mitcad" "$@"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$name: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$name: the driver failed"; }
  ui_expect_log 'TestDriver: done' "$name: Delete on the Start page deletes nothing behind it"
  ui_expect_log 'Timeline selected: nothing' "$name: the Start tab clears the selection"
}

run_layout floating --chrome=floating
run_layout docked --chrome=docked
ui_finish 'UI Start page: Delete and Backspace delete nothing of the design behind it'
