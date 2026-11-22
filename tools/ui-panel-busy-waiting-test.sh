#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/BusyMark.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/CommandSession.hpp app/framework/DocumentSession.cpp app/framework/TestDriver.cpp
# check-all sources: tools/ui-scripts/panel-busy-waiting.mitcad-ui tools/ui-scripts/panel-message-top.json tools/cli
# A panel's preview held while the model finishes another job shows the
# busy mark (mitcad#2118) through the real UI, the shared input script
# tools/ui-scripts/panel-busy-waiting.mitcad-ui (also run by
# tools/ui-windows-panel-busy-waiting-test.ps1 and
# tools/ui-macos-panel-busy-waiting-test.sh) on a cube that mitcad-cli makes
# (tools/ui-scripts/panel-message-top.json), in the Docked layout and then
# in the Floating one: Extrude1 edited, its Distance 7 mm, whose preview's
# display refinement the test's worker gate holds; a Distance of 9 mm
# typed meanwhile waits for it with the busy mark beside OK and Cancel
# shown ("Waiting for the design to finish computing…"), and once the job
# is let go its preview follows and the mark hides. (The panel waited with
# no mark.)
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-panel-busy-waiting-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-panel-busy-waiting.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/panel-busy-waiting.mitcad-ui
"$cli" run "$here/ui-scripts/panel-message-top.json" --save "$work/panel-busy-waiting.mitcad" || exit 1
ui_start_display

run_layout() {
  echo "--- $1"
  ui_start_app --ui=qt --no-mcp --chrome="$1" --open "$work/panel-busy-waiting.mitcad"
  for _ in $(seq 1 600); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1: the driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; ui_fail "$1: the driver failed"; exit 1; }
  ui_expect_log 'Preview Extrude: refining the display meshes' "$1: the preview's refinement, another job"
  ui_expect_log 'Preview Extrude: waits for the model' "$1: the next preview waits for it"
  ui_expect_log 'Panel Extrude busy mark shown after ' "$1: the busy mark shown while it waits"
  ui_expect_log 'Panel Extrude busy: Waiting for the design to finish computing' "$1: its text says what it waits for"
  ui_expect_log 'Panel Extrude busy mark hidden after ' "$1: and hidden once the preview has ended"
  ui_expect_log 'TestDriver: done' "$1: a preview held by another job shows the busy mark"
}

run_layout docked
run_layout floating
ui_finish "UI a preview held by another job shows the busy mark"
