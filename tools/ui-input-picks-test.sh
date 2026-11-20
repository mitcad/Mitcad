#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/InputPicks.cpp
# check-all sources: app/framework/CommandSessionPicks.cpp app/framework/TestDriver.cpp app/commands/PatternCommands.cpp
# check-all sources: app/commands/FilletCommands.cpp tools/cli
# check-all sources: tools/ui-scripts/input-picks.mitcad-ui tools/ui-scripts/input-picks.json
# A selection input's picks listed by name on demand, each highlighted in
# the view on hover (mitcad#2064, GitHub mitcad/Mitcad#473), through the
# real UI: the shared input script tools/ui-scripts/input-picks.mitcad-ui
# (also run by tools/ui-windows-input-picks-test.ps1 and
# tools/ui-macos-input-picks-test.sh) on a design that mitcad-cli makes
# (tools/ui-scripts/input-picks.json): Pattern on Path's Objects listed by
# name once opened, a row under the pointer highlighting only its body, a
# row's ✕ taking it out, the open state kept for the next panels, and the
# same for a Fillet's edges and a pattern's faces.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-input-picks-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-input-picks.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=50
export MITCAD_TEST_INPUT=$here/ui-scripts/input-picks.mitcad-ui
design=$work/input-picks.mitcad
"$cli" run "$here/ui-scripts/input-picks.json" --save "$design" || exit 1
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$design"
for _ in $(seq 1 1200); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'input picks driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'Panel Pattern on Path pick bodies hovered: 2. Body2' 'a row under the pointer names its pick'
ui_expect_log 'TestDriver: done' 'the picks are listed by name on demand and highlighted on hover'
ui_finish 'UI a selection input lists its picks by name'
