#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/commands/SketchCommands.cpp app/framework/CommandPanel.cpp app/framework/CommandSession.cpp
# check-all sources: app/framework/Command.hpp app/framework/TestDriver.cpp app/sketch core/sketch
# check-all sources: core/model/src/features/sketch.rs
# check-all sources: core/model/src/features/projection_successors.rs core/model-api/src/sketch.rs tools/cli
# check-all sources: tools/ui-scripts/project-again-successor.json tools/ui-scripts/project-again-successor.mitcad-ui
# Project Again's choice of the edge that takes a lost projection's
# source's place (mitcad#1794) through the real UI, the shared input
# script tools/ui-scripts/project-again-successor.mitcad-ui (also run by
# tools/ui-windows-project-again-successor-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/project-again-successor.json): the
# palette offers Project Again under the lost projection's warning, its
# panel lists the two edges in words, the pointer over a row paints its
# edge, a click chooses it, and OK keeps it without the warning.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-project-again-successor-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-project-again-successor.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/project-again-successor.mitcad-ui
"$cli" run "$here/ui-scripts/project-again-successor.json" --save "$work/project-again-successor.mitcad" \
  > "$work/cli.log" 2>&1 || { cat "$work/cli.log"; ui_fail 'mitcad-cli'; exit 1; }
[ -f "$work/project-again-successor.mitcad" ] || { ui_fail 'the design was not made'; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/project-again-successor.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'Project Again successor driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'Project Again lists the edges in words, paints the hovered one and takes the chosen one'
ui_finish 'UI Project Again chooses the edge in a lost source'"'"'s place'
