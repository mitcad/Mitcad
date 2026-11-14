#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# check-all sources: app/framework/CommandPanel.cpp app/framework/CommandSession.cpp app/framework/CommandSessionPicks.cpp
# check-all sources: app/framework/Command.hpp app/commands/HoleCommands.cpp core/model/src/features/thread.rs tools/cli
# check-all sources: tools/ui-scripts/gone-pick-edit.json tools/ui-scripts/gone-pick-edit.mitcad-ui
# A selection input with a pick that is gone (mitcad#1780) through the real
# UI, the shared input script tools/ui-scripts/gone-pick-edit.mitcad-ui
# (also run by tools/ui-windows-gone-pick-edit-test.ps1) on a design that
# mitcad-cli makes (tools/ui-scripts/gone-pick-edit.json): editing Thread1,
# whose first face is gone, the Faces chip says one is missing, the message
# names the input, and removing the gone pick lets the edit build.
#
# Runs headless on Xvfb (see ui-test-lib.sh).
# Usage: tools/ui-gone-pick-edit-test.sh
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/.." && pwd)
. "$here/ui-test-lib.sh"
cli=${UI_CLI:-$(cd "$(dirname "$UI_APP")/.." && pwd)/tools/cli/mitcad-cli}
mkdir -p "$root/build"
work=$(mktemp -d "$root/build/ui-gone-pick-edit.XXXXXX")
trap 'ui_cleanup; rm -rf -- "${work:?}"' EXIT
export MITCAD_TEST_WORK=$work MITCAD_TEST_STEP_MS=100
export MITCAD_TEST_INPUT=$here/ui-scripts/gone-pick-edit.mitcad-ui
# The script's last step leaves Thread1 failed on purpose; the design is
# what counts.
"$cli" run "$here/ui-scripts/gone-pick-edit.json" --save "$work/gone-pick-edit.mitcad"
[ -f "$work/gone-pick-edit.mitcad" ] || { ui_fail 'the design was not made'; exit 1; }
ui_start_display
ui_start_app --ui=qt --no-mcp --open "$work/gone-pick-edit.mitcad"
for _ in $(seq 1 900); do
  kill -0 "$UI_RUNNER" 2> /dev/null || break
  sleep 0.2
done
kill -0 "$UI_RUNNER" 2> /dev/null && { ui_fail 'gone pick edit driver timed out'; exit 1; }
wait "$UI_RUNNER" || { cat "$UI_LOG" >&2; exit 1; }
ui_expect_log 'TestDriver: done' 'the gone pick is marked, removed, and the edit builds'
ui_finish 'UI a selection input marks a pick that is gone'
