#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/FilletCommands.cpp app/framework/ManipulatorOverlay.cpp app/framework/Command.hpp
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandPanel.cpp app/framework/TestDriver.cpp tools/cli
# check-all sources: core/model/src/features/press_pull.rs core/model-api/src/press_pull.rs core/ffi/src/kernel/press_pull.rs
# check-all sources: core/cpp/bridge/press_pull.cpp geometry/src/press_pull.cpp geometry/src/face_extrude.cpp geometry/src/face_revolve.cpp
# check-all sources: tools/ui-scripts/press-pull-continued.mitcad-ui tools/ui-scripts/press-pull-continued.json
# Press Pull on a face continued (mitcad#1850) through the real UI, the
# shared input script tools/ui-scripts/press-pull-continued.mitcad-ui (also
# run by tools/ui-windows-press-pull-continued-test.ps1 and
# tools/ui-macos-press-pull-continued-test.sh) on a design that mitcad-cli
# makes (tools/ui-scripts/press-pull-continued.json): on a bent tube's end
# Continue offers Revolve1's own edit; Along the Walls with its ring
# dragged past the walls' inner radius, then 25 mm, adds the revolution's
# volume; Straight with a Scale taper of 1.1 on a T-slot bar's end adds the
# scaled run's.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-press-pull-continued-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-press-pull-continued.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/press-pull-continued.mitcad-ui
"$cli" run "$here/ui-scripts/press-pull-continued.json" --save "$work/press-pull-continued.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/press-pull-continued.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'press pull continued driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Press Pull opened Revolve1 for its angle' "the tube's end opened Revolve1 at its Angle"
ui_expect_log 'Press Pull made press pull along the walls, distance 25' "Along the Walls on the bent tube's end"
ui_expect_log 'Press Pull made press pull straight, distance 10' 'Straight with a Scale taper on the T-slot bar'
ui_expect_log 'TestDriver: done' 'the bent tube and the T-slot bar continued'
ui_finish 'UI Press Pull continued Straight or Along the walls'
