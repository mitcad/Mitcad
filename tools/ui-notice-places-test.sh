#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/MainWindowNoticeHover.cpp app/MainWindow.cpp app/OcctViewer.cpp app/browser
# check-all sources: app/framework/NoticePlaces.cpp app/framework/CommandMessage.cpp app/framework/CommandPanel.cpp
# check-all sources: app/framework/CommandSession.cpp app/framework/MessageBand.cpp app/framework/TestDriver.cpp
# check-all sources: app/commands/FilletCommands.cpp core/model-base/src/notice.rs core/model/src/recompute.rs
# check-all sources: core/model-api/src/lib.rs core/ffi/src/kernel/dressup.rs core/cpp/bridge/dressup.cpp
# check-all sources: geometry/src/dressup.cpp geometry/src/dressup_check.cpp
# check-all sources: tools/ui-scripts/notice-places.mitcad-ui tools/ui-scripts/notice-places.json tools/cli
# Notice places (mitcad#1327) through the real UI, the shared input script
# tools/ui-scripts/notice-places.mitcad-ui (also run by
# tools/ui-windows-notice-places-test.ps1) on a design that mitcad-cli
# makes (tools/ui-scripts/notice-places.json): a rim's rounding that runs
# onto the rounding of the block's front edge warns in plain words, and the pointer over the
# warning paints its places in the view (the rounding's face, a point) on
# the Timeline item, in the message band and in the command panel's
# message while it is edited, in the Floating chrome (the message band's);
# moving away clears them. The screenshots
# (notice-places-*.png) show the highlight.
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
export MITCAD_TEST_INPUT=$here/ui-scripts/notice-places.mitcad-ui
"$cli" run "$here/ui-scripts/notice-places.json" --save "$work/notice-places.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --chrome=floating --open "$work/notice-places.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'notice places driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Notice hover highlighted: timeline: ' 'the Timeline item paints its places'
ui_expect_log 'Notice hover highlighted: message band: ' 'the message band paints its places'
ui_expect_log 'Notice hover highlighted: panel: ' "the command panel's message paints its places"
ui_expect_log 'TestDriver: done' 'the notice places are painted on hover'
ui_finish 'UI notice places on hover'
