#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowNoticeHover.cpp app/MainWindow.cpp app/OcctViewer.cpp app/browser
# check-all sources: app/framework/NoticePlaces.cpp app/framework/CommandMessage.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/MessageBand.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/FilletCommands.cpp core/model-base/src/notice.rs core/model/src/recompute.rs
# check-all sources: core/model-api/src/lib.rs core/ffi/src/kernel/dressup.rs core/cpp/bridge/dressup.cpp
# check-all sources: geometry/src/dressup.cpp geometry/src/dressup_check.cpp
# check-all sources: tools/ui-scripts/notice-places.mitcad-ui tools/ui-scripts/notice-places.json tools/cli
# check-all sources: tools/ui-scripts/notice-places-region.mitcad-ui tools/ui-scripts/notice-places-region.json
# check-all sources: core/model/src/features/extrude.rs core/sketch/src/def.rs
# Notice places (mitcad#1327) through the real UI, the shared input script
# tools/ui-scripts/notice-places.mitcad-ui (also run by
# tools/ui-windows-notice-places-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/notice-places.json): a rim's rounding that runs
# onto the rounding of the block's front edge warns in plain words, and the pointer over the
# warning paints its places in the view (the rounding's face, a point) on
# the Timeline item, in the message band and in the command panel's
# message while it is edited, in the Floating chrome (the message band's);
# moving away clears them, as does another message in the hovered one's
# place (mitcad#1609); a hover while a preview computes reads nothing from
# the design and paints once it has ended (mitcad#1600). Then
# tools/ui-scripts/notice-places-region.mitcad-ui on
# notice-places-region.json: a sketch region's place in a component moved
# by 100 mm painted where the component is shown (mitcad#1609). The
# screenshots (notice-places-*.png) show the highlight.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-notice-places-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-notice-places.XXXXXX")
shots=${MITCAD_UI_LOG_DIR:-$work}/notice-places-shots
mkdir -p "$shots"
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100 MITCAD_TEST_SHOTS=$shots
"$cli" run "$here/ui-scripts/notice-places.json" --save "$work/notice-places.mitcad" || exit 1
"$cli" run "$here/ui-scripts/notice-places-region.json" --save "$work/notice-places-region.mitcad" || exit 1
ui_start_display
# run_script <name> <design>: the app on the design with the input script
# tools/ui-scripts/<name>.mitcad-ui, until its driver is done.
run_script() {
  export MITCAD_TEST_INPUT=$here/ui-scripts/$1.mitcad-ui
  ui_start_app --ui=qt --no-mcp --chrome=floating --open "$2"
  for _ in $(seq 1 1500); do
    kill -0 "$UI_RUNNER" 2> /dev/null || break
    sleep 0.2
  done
  kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail "$1 driver timed out"; exit 1; }
  wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
  UI_RUNNER=""
}
run_script notice-places "$work/notice-places.mitcad"
ui_expect_log 'Notice hover highlighted: timeline: ' 'the Timeline item paints its places'
ui_expect_log 'Notice hover highlighted: message band: ' 'the message band paints its places'
ui_expect_log 'Notice hover highlighted: panel: ' "the command panel's message paints its places"
ui_expect_log 'Panel Fillet message hovered: 0 places' "the hint of no preview paints no warning's places"
ui_expect_log '(drawn when the job ends)' 'hovered while a preview computes, nothing is read from the design'
ui_expect_log 'TestDriver: done' 'the notice places are painted on hover'
# A sketch region of a moved component (mitcad#1609).
run_script notice-places-region "$work/notice-places-region.mitcad"
ui_expect_log 'Notice hover bounds: x 70.0 to 130.0, ' "a moved component's region is painted where it is shown"
ui_expect_log 'TestDriver: done' 'the region is painted on hover'
ui_finish 'UI notice places on hover'
