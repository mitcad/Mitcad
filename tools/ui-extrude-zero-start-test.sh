#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/ExtrudeCommands.cpp app/framework/AutoOperationPolicy.hpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSessionAuto.cpp
# check-all sources: app/framework/ManipulatorOverlay.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: tools/ui-scripts/extrude-zero-start.mitcad-ui tools/ui-scripts/extrude-zero-start.json
# A new Extrude starts at zero distance (mitcad#2481, GitHub #698) through
# the real UI, the input script tools/ui-scripts/extrude-zero-start.mitcad-ui
# on a design that mitcad-cli makes (tools/ui-scripts/extrude-zero-start.json):
# an edit loads its stored depth; a new Extrude on a 1 x 1 mm square shows
# 0 mm, its arrow and a hint with OK disabled; 0.2 mm typed previews and
# commits; Repeat keeps the last OK's distance; the arrow dragged from zero
# works both ways; Symmetric and Two Sides at zero wait, Two Sides' second
# arrow drags from zero; with Start Offset (mitcad#2508) its arrow and the
# distance's are drawn apart and off the profile, each hovered on its own
# and grabbed at zero without a jump (the first move takes the value about
# as far as the pointer); a body face at zero waits and its first distance
# chooses Cut.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-extrude-zero-start-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-extrude-zero-start.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/extrude-zero-start.mitcad-ui
"$cli" run "$here/ui-scripts/extrude-zero-start.json" --save "$work/extrude-zero-start.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/extrude-zero-start.mitcad"
for _ in $(seq 1 1500); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Extrude zero start driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'A new Extrude starts at zero: hint, arrow, typed and dragged depths, sides, a face'
ui_finish 'UI Extrude zero start'
