#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/PreviewHold.cpp app/framework/CommandSessionHold.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/ManipulatorOverlay.cpp app/OcctViewer.cpp
# check-all sources: app/framework/TestDriver.cpp app/commands/ExtrudeCommands.cpp app/commands/FilletCommands.cpp
# check-all sources: tools/ui-scripts/ctrl-hides-preview.mitcad-ui tools/ui-scripts/ctrl-hides-preview.json tools/cli
# Ctrl held alone hides a command's preview while its inputs are picked
# (mitcad#1421) through the real UI, the shared input script
# tools/ui-scripts/ctrl-hides-preview.mitcad-ui (also run by
# tools/ui-windows-ctrl-hides-preview-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/ctrl-hides-preview.json): a new Extrude on three
# circles hides its bodies while Ctrl is held, a fourth circle Ctrl+clicked
# then joins and the release draws the extrude of all four; Ctrl+Z and
# Ctrl+S show it again at once, a Ctrl+scroll keeps it hidden until the
# release; Fillet1 edited from the timeline gains an
# edge while Ctrl hides its rounding. "Preview bodies:" and the shape the
# view draws are read before, during and after each hold.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-ctrl-hides-preview-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-ctrl-hides-preview.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/ctrl-hides-preview.mitcad-ui
"$cli" run "$here/ui-scripts/ctrl-hides-preview.json" --save "$work/ctrl-hides-preview.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/ctrl-hides-preview.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Ctrl hides preview driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Preview hidden (Ctrl held)' 'Ctrl held alone hides the preview'
ui_expect_log 'Extrude Profiles: 4 profiles' 'a circle Ctrl+clicked while the preview is hidden joins Profiles'
ui_expect_log 'Preview shown (Ctrl released)' 'the release shows the preview again'
ui_expect_log 'Preview shown (a key pressed)' 'a shortcut with Ctrl shows the preview again at once'
ui_expect_log 'Added extrude (New Body)' 'OK adds the extrude of four circles'
ui_expect_log 'Fillet Edges: 2 edges' 'an edge Ctrl+clicked while the rounding is hidden joins the edited fillet'
ui_expect_log 'Edited F3' 'OK keeps the edited fillet'
ui_expect_log 'TestDriver: done' 'Ctrl hides the preview while the inputs are picked'
ui_finish 'UI Ctrl hides the preview while the inputs are picked'
