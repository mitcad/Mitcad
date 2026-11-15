#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandSession.cpp app/framework/CommandSession.hpp app/framework/Command.hpp
# check-all sources: app/framework/RememberedValues.hpp app/commands/HoleCommands.cpp app/commands/HoleSizes.cpp
# check-all sources: app/MainWindow.cpp core/model/src/features/hole_sizes.rs tools/cli
# check-all sources: tools/ui-scripts/hole-repeat.mitcad-ui tools/ui-scripts/hole-repeat.json
# Repeat runs a value's reaction with the repeated value (mitcad#1844)
# and keeps typed only what was typed (mitcad#1845) through the real UI,
# the shared input script tools/ui-scripts/hole-repeat.mitcad-ui (also run
# by tools/ui-windows-hole-repeat-test.ps1 and
# tools/ui-macos-hole-repeat-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/hole-repeat.json): a tapped countersink Hole with a
# custom thread of 12 mm; Enter repeats it, its sizes filled for 12 mm
# before the panel is there, and its countersink, not typed at the OK,
# follows a new Major Diameter; one typed stays typed after Repeat.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-hole-repeat-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-hole-repeat.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/hole-repeat.mitcad-ui
"$cli" run "$here/ui-scripts/hole-repeat.json" --save "$work/hole-repeat.mitcad" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/hole-repeat.mitcad"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'hole repeat driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel Hole repeated: ' 'Enter repeated Hole with its last settings'
ui_expect_log 'TestDriver: done' 'Repeat of Hole: the sizes follow the thread, a typed one stays typed'
ui_finish 'UI Repeat of Hole with a custom thread'
